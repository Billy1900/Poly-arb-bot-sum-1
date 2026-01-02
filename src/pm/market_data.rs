use anyhow::{Context, Result};
use futures::{stream, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::types::{GlobalSnapshot, MarketBook, OutcomeTop};

#[derive(Debug, Clone)]
pub struct MarketDef {
    pub market_id: String,
    pub question: String,
    pub token_ids: Vec<String>,
}

#[derive(Clone)]
pub struct MarketData {
    host: String,
    http: reqwest::Client,
    books_chunk_size: usize,
    books_concurrency: usize,
}

impl MarketData {
    pub fn new(host: String, books_chunk_size: usize, books_concurrency: usize) -> Self {
        Self {
            host,
            http: reqwest::Client::new(),
            books_chunk_size: books_chunk_size.max(1),
            books_concurrency: books_concurrency.max(1),
        }
    }

    pub async fn fetch_open_markets(&self, max_markets: usize) -> Result<Vec<MarketDef>> {
        let mut out: Vec<MarketDef> = vec![];
        let mut next: Option<String> = None;

        loop {
            let mut url = format!("{}/markets", self.host.trim_end_matches('/'));
            if let Some(ref c) = next {
                url = format!("{}?next_cursor={}", url, c);
            }

            let resp: MarketsResp = self.http
                .get(url)
                .send()
                .await
                .context("GET /markets failed")?
                .error_for_status()
                .context("GET /markets non-200")?
                .json()
                .await
                .context("decode /markets json failed")?;

            for m in resp.data.into_iter() {
                if m.enable_order_book && m.accepting_orders && !m.closed {
                    let token_ids: Vec<String> = m.tokens.into_iter().map(|t| t.token_id).collect();
                    if !token_ids.is_empty() {
                        out.push(MarketDef {
                            market_id: m.condition_id,
                            question: m.question,
                            token_ids,
                        });
                    }
                    if out.len() >= max_markets {
                        return Ok(out);
                    }
                }
            }

            next = resp.next_cursor;
            if next.is_none() { break; }
        }

        Ok(out)
    }

    pub async fn snapshot_for_markets(&self, markets: &[MarketDef]) -> Result<GlobalSnapshot> {
        let mut all_tokens: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for m in markets {
            for t in &m.token_ids {
                if seen.insert(t.clone()) {
                    all_tokens.push(t.clone());
                }
            }
        }

        let books = self.fetch_books_chunked(&all_tokens).await?;

        let mut top_map: std::collections::HashMap<String, OutcomeTop> = std::collections::HashMap::new();
        for b in books.into_iter() {
            top_map.insert(b.token_id.clone(), b);
        }

        let mut mbooks: Vec<MarketBook> = Vec::with_capacity(markets.len());
        for m in markets {
            let outcomes: Vec<OutcomeTop> = m.token_ids.iter()
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

    async fn fetch_books_chunked(&self, token_ids: &[String]) -> Result<Vec<OutcomeTop>> {
        if token_ids.is_empty() { return Ok(vec![]); }

        let chunks: Vec<Vec<String>> = token_ids
            .chunks(self.books_chunk_size)
            .map(|c| c.to_vec())
            .collect();

        tracing::debug!(
            total_tokens = token_ids.len(),
            chunks = chunks.len(),
            chunk_size = self.books_chunk_size,
            conc = self.books_concurrency,
            "fetching books in chunks"
        );

        let host = self.host.clone();
        let http = self.http.clone();

        let mut out: Vec<OutcomeTop> = Vec::with_capacity(token_ids.len());

        let mut stream = stream::iter(chunks.into_iter().map(|chunk| {
            let url = format!("{}/books", host.trim_end_matches('/'));
            let http = http.clone();
            async move {
                let body: Vec<BooksReqItem> = chunk.into_iter().map(|t| BooksReqItem { token_id: t }).collect();
                let resp: Vec<BookSummary> = http
                    .post(url)
                    .json(&body)
                    .send()
                    .await
                    .context("POST /books failed")?
                    .error_for_status()
                    .context("POST /books non-200")?
                    .json()
                    .await
                    .context("decode /books json failed")?;
                Ok::<Vec<BookSummary>, anyhow::Error>(resp)
            }
        })).buffer_unordered(self.books_concurrency);

        while let Some(res) = stream.next().await {
            let page = res?;
            for b in page {
                out.push(convert_book_to_top(b));
            }
        }

        Ok(out)
    }
}

fn convert_book_to_top(b: BookSummary) -> OutcomeTop {
    let best_bid = b.bids.iter()
        .filter_map(|lvl| parse_dec(&lvl.price).zip(parse_dec(&lvl.size)))
        .max_by(|a,b| a.0.cmp(&b.0));
    let best_ask = b.asks.iter()
        .filter_map(|lvl| parse_dec(&lvl.price).zip(parse_dec(&lvl.size)))
        .min_by(|a,b| a.0.cmp(&b.0));

    let (best_bid_px, best_bid_sz) = best_bid.map(|(p,s)| (Some(p), Some(s))).unwrap_or((None,None));
    let (best_ask_px, best_ask_sz) = best_ask.map(|(p,s)| (Some(p), Some(s))).unwrap_or((None,None));

    OutcomeTop {
        token_id: b.asset_id,
        best_bid_px,
        best_bid_sz,
        best_ask_px,
        best_ask_sz,
    }
}

#[derive(Debug, Clone, Serialize)]
struct BooksReqItem {
    #[serde(rename = "token_id")]
    token_id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BookLvl {
    price: String,
    size: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BookSummary {
    #[serde(rename = "asset_id")]
    asset_id: String,
    bids: Vec<BookLvl>,
    asks: Vec<BookLvl>,
}

fn parse_dec(s: &str) -> Option<Decimal> {
    s.parse::<Decimal>().ok()
}

#[derive(Debug, Clone, Deserialize)]
struct MarketsResp {
    data: Vec<MarketItem>,
    #[serde(default)]
    next_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct MarketItem {
    enable_order_book: bool,
    accepting_orders: bool,
    closed: bool,
    condition_id: String,
    question: String,
    tokens: Vec<TokenItem>,
}

#[derive(Debug, Clone, Deserialize)]
struct TokenItem {
    token_id: String,
}
