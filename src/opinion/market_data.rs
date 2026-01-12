use anyhow::{Context, Result};
use futures::{stream, StreamExt};
use rust_decimal::Decimal;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT};

use crate::pm::market_data::MarketDef;
use crate::source::MarketDataSource;
use crate::types::{GlobalSnapshot, MarketBook, OutcomeTop};
use super::types::*;

const BASE_URL: &str = "https://openapi.opinion.trade/openapi";

/// Opinion.trade market data source implementing MarketDataSource
pub struct OpinionMarketData {
    api_key: String,
    http: reqwest::Client,
    concurrency: usize,
}

impl OpinionMarketData {
    pub fn new(api_key: String, concurrency: usize) -> Self {
        let api_key = api_key.trim().to_string();

        // Some API gateways/WAFs behave differently based on HTTP version and header defaults.
        // Match a conservative curl-like profile to minimize false 403s.
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("*/*"));

        let http = reqwest::Client::builder()
            .http1_only()
            .user_agent("curl/8.0")
            .default_headers(headers)
            .build()
            .expect("build reqwest client");

        tracing::debug!(api_key_len = api_key.len(), "OpinionMarketData initialized");
        Self {
            api_key,
            http,
            concurrency: concurrency.max(1),
        }
    }

    async fn fetch_markets_page(&self, page: usize, limit: usize) -> Result<Vec<MarketItem>> {
        let url = format!(
            "{}/market?page={}&limit={}&status=activated&marketType=2",
            BASE_URL, page, limit
        );

        tracing::debug!(
            api_key_len = self.api_key.len(),
            url = %url,
            "fetching markets page"
        );

        let resp = self
            .http
            .get(&url)
            .header("apikey", &self.api_key)
            .send()
            .await
            .context("GET /market failed")?;

        let status = resp.status();
        let body = resp.text().await.context("read /market body failed")?;
        if !status.is_success() {
            let snippet: String = body.chars().take(512).collect();
            anyhow::bail!(
                "GET /market non-200: status={} body_snippet={}",
                status,
                snippet
            );
        }

        let resp: APIBaseResponse<MarketListResponse> = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => {
                let snippet: String = body.chars().take(2048).collect();
                anyhow::bail!(
                    "decode /market json failed: {} body_snippet={}",
                    e,
                    snippet
                );
            }
        };

        if resp.code != 0 {
            anyhow::bail!("API error: code={}, msg={}", resp.code, resp.msg);
        }

        Ok(resp.result.list)
    }

    fn extract_token_ids(&self, item: &MarketItem) -> Vec<String> {
        match item.market_type {
            0 => {
                // Binary market: yesTokenId and noTokenId
                let mut ids = vec![];
                if !item.yes_token_id.is_empty() {
                    ids.push(item.yes_token_id.clone());
                }
                if !item.no_token_id.is_empty() {
                    ids.push(item.no_token_id.clone());
                }
                ids
            }
            1 => {
                // Categorical market: use YES tokens from activated childMarkets.
                // Note: many "ladder" markets are represented as a set of binary submarkets
                // (often dates / thresholds). Those are filtered out in fetch_open_markets.
                item.child_markets
                    .iter()
                    .filter(|cm| cm.status == 2)
                    .map(|cm| cm.yes_token_id.clone())
                    .filter(|tid| !tid.is_empty())
                    .collect()
            }
            _ => vec![],
        }
    }

    fn looks_like_ladder_market(parent_title: &str, children: &[ChildMarket]) -> bool {
        if children.len() < 2 {
            return false;
        }

        let parent = parent_title.to_ascii_lowercase();
        let parent_ladder_hint = parent.contains(" by ")
            || parent.contains(" before ")
            || parent.contains(" hit ")
            || parent.contains(" price ")
            || parent.contains(" above ")
            || parent.contains(" below ")
            || parent.contains(" over ")
            || parent.contains(" under ")
            || parent.contains(" at least ")
            || parent.contains(" at most ")
            || parent.contains(" or above ")
            || parent.contains(" or below ");

        // If the child option labels look like dates or monotonic numeric thresholds,
        // it's very likely a ladder (nested YES events), which breaks sum-arb semantics.
        let mut numeric_markers: Vec<i64> = Vec::new();
        let mut has_date_like = false;
        let mut has_threshold_symbols = false;

        for cm in children {
            let t = cm.market_title.trim();
            if t.is_empty() {
                continue;
            }
            let tl = t.to_ascii_lowercase();

            // Very lightweight date-ish detection.
            if tl.contains("jan")
                || tl.contains("feb")
                || tl.contains("mar")
                || tl.contains("apr")
                || tl.contains("may")
                || tl.contains("jun")
                || tl.contains("jul")
                || tl.contains("aug")
                || tl.contains("sep")
                || tl.contains("oct")
                || tl.contains("nov")
                || tl.contains("dec")
                || tl.contains("202")
            {
                has_date_like = true;
            }

            if tl.contains('$')
                || tl.contains('%')
                || tl.contains('>')
                || tl.contains('<')
                || tl.contains('↑')
                || tl.contains('↓')
                || tl.contains("bps")
                || tl.contains("million")
                || tl.contains("billion")
                || tl.contains('m')
                || tl.contains('k')
            {
                has_threshold_symbols = true;
            }

            // Extract the first integer-like run; enough to detect monotonic ladders like 20/40/60.
            let mut buf = String::new();
            for ch in tl.chars() {
                if ch.is_ascii_digit() {
                    buf.push(ch);
                } else if !buf.is_empty() {
                    break;
                }
            }
            if !buf.is_empty() {
                if let Ok(v) = buf.parse::<i64>() {
                    numeric_markers.push(v);
                }
            }
        }

        let monotonic_numbers = if numeric_markers.len() >= 3 {
            let mut inc = true;
            let mut dec = true;
            for w in numeric_markers.windows(2) {
                if w[1] <= w[0] {
                    inc = false;
                }
                if w[1] >= w[0] {
                    dec = false;
                }
            }
            inc || dec
        } else {
            false
        };

        parent_ladder_hint && (has_date_like || has_threshold_symbols || monotonic_numbers)
    }

    async fn fetch_orderbook(&self, token_id: &str) -> Option<OutcomeTop> {
        let url = format!("{}/token/orderbook?token_id={}", BASE_URL, token_id);

        // HOT PATH: minimal error handling - fail fast, return None on error
        let resp: APIBaseResponse<OrderbookResponse> = self
            .http
            .get(&url)
            .header("apikey", &self.api_key)
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?
            .json()
            .await
            .ok()?;

        if resp.code != 0 {
            return None;
        }

        Some(Self::convert_orderbook(token_id.to_string(), resp.result))
    }

    fn convert_orderbook(token_id: String, resp: OrderbookResponse) -> OutcomeTop {
        // Opinion.trade returns pre-sorted books:
        // - bids[0] is best bid (descending)
        // - asks[0] is best ask (ascending)
        let best_bid = resp.bids.first().and_then(|lvl| {
            let price = lvl.price.parse::<Decimal>().ok()?;
            let size = lvl.size.parse::<Decimal>().ok()?;
            Some((price, size))
        });

        let best_ask = resp.asks.first().and_then(|lvl| {
            let price = lvl.price.parse::<Decimal>().ok()?;
            let size = lvl.size.parse::<Decimal>().ok()?;
            Some((price, size))
        });

        let (best_bid_px, best_bid_sz) =
            best_bid.map(|(p, s)| (Some(p), Some(s))).unwrap_or((None, None));
        let (best_ask_px, best_ask_sz) =
            best_ask.map(|(p, s)| (Some(p), Some(s))).unwrap_or((None, None));

        OutcomeTop {
            token_id,
            best_bid_px,
            best_bid_sz,
            best_ask_px,
            best_ask_sz,
        }
    }
}

#[async_trait::async_trait]
impl MarketDataSource for OpinionMarketData {
    async fn fetch_open_markets(&self, max_markets: usize) -> Result<Vec<MarketDef>> {
        let mut out: Vec<MarketDef> = vec![];
        let mut page: usize = 1; // opinion.trade uses 1-based pagination
        let page_size: usize = 20; // max page size

        while out.len() < max_markets {
            let items = self.fetch_markets_page(page, page_size).await?;

            if items.is_empty() {
                break;
            }

            for item in items {
                // Opinion categorical markets are often represented as multiple binary submarkets.
                // Some of those are ladder-style (nested YES events), which are not suitable for
                // sum(outcome prices) arbitrage. Filter them out early.
                if item.market_type == 1 && Self::looks_like_ladder_market(&item.market_title, &item.child_markets) {
                    tracing::debug!(market_id=item.market_id, title=%item.market_title, children=item.child_markets.len(), "skipping ladder-like market");
                    continue;
                }

                let token_ids = self.extract_token_ids(&item);
                if token_ids.len() >= 2 {
                    out.push(MarketDef {
                        market_id: item.market_id.to_string(),
                        question: item.market_title.clone(),
                        token_ids,
                    });

                    if out.len() >= max_markets {
                        return Ok(out);
                    }
                }
            }

            page += 1;
        }

        Ok(out)
    }

    async fn snapshot_for_markets(&self, markets: &[MarketDef]) -> Result<GlobalSnapshot> {
        // Collect all unique token IDs
        let mut all_tokens: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for m in markets {
            for t in &m.token_ids {
                if seen.insert(t.clone()) {
                    all_tokens.push(t.clone());
                }
            }
        }

        // Fetch order books concurrently (HOT PATH - minimal error handling)
        let api_key = self.api_key.clone();
        let http = self.http.clone();

        let mut stream = stream::iter(all_tokens.into_iter().map(|token_id| {
            let http = http.clone();
            let api_key = api_key.clone();
            async move {
                // Individual fetch errors are silently skipped (return None)
                let url = format!("{}/token/orderbook?token_id={}", BASE_URL, token_id);
                let resp: APIBaseResponse<OrderbookResponse> = http
                    .get(&url)
                    .header("apikey", &api_key)
                    .send()
                    .await
                    .ok()?
                    .error_for_status()
                    .ok()?
                    .json()
                    .await
                    .ok()?;

                if resp.code != 0 {
                    return None;
                }

                Some(OpinionMarketData::convert_orderbook(token_id, resp.result))
            }
        }))
        .buffer_unordered(self.concurrency);

        let mut top_map: std::collections::HashMap<String, OutcomeTop> =
            std::collections::HashMap::new();

        while let Some(result) = stream.next().await {
            if let Some(ot) = result {
                top_map.insert(ot.token_id.clone(), ot);
            }
        }

        // Build MarketBook vectors
        let mut mbooks: Vec<MarketBook> = Vec::with_capacity(markets.len());
        for m in markets {
            let outcomes: Vec<OutcomeTop> = m
                .token_ids
                .iter()
                .filter_map(|tid| top_map.get(tid).cloned())
                .collect();

            if outcomes.len() == m.token_ids.len() {
                mbooks.push(MarketBook {
                    market_id: m.market_id.clone(),
                    question: m.question.clone(),
                    outcomes,
                });
            }
        }

        Ok(GlobalSnapshot {
            ts_ms: chrono::Utc::now().timestamp_millis(),
            markets: mbooks,
        })
    }
}
