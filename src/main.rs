mod config;
mod types;
mod stats;

mod pm;
mod strategy;

use anyhow::Result;
use rust_decimal::Decimal;
use tracing_subscriber::EnvFilter;

use crate::config::Settings;
use crate::pm::market_data::{MarketData, MarketDef};
use crate::pm::execution_observer::ExecutionObserver;
use crate::stats::Stats;
use crate::strategy::sum_arb::SumArbStrategy;
use crate::strategy::Strategy;

fn parse_opt_decimal(s: &Option<String>) -> anyhow::Result<Option<Decimal>> {
    Ok(match s {
        Some(v) if !v.trim().is_empty() => Some(v.trim().parse::<Decimal>()?),
        _ => None,
    })
}

fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis() as u64
}

async fn maybe_write_jsonl(path: &Option<String>, line: &str) {
    if let Some(p) = path.as_ref().map(|x| x.trim().to_string()).filter(|x| !x.is_empty()) {
        if let Ok(mut f) = tokio::fs::OpenOptions::new().create(true).append(true).open(&p).await {
            use tokio::io::AsyncWriteExt;
            let _ = f.write_all(line.as_bytes()).await;
            let _ = f.write_all(b"\n").await;
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let s = Settings::from_env()?;
    let md = MarketData::new(s.clob_host.clone(), s.books_chunk_size, s.books_concurrency);

    let stats = Stats::new(now_ms());

    let mut markets: Vec<MarketDef> = vec![];
    let mut last_refresh = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_secs(3600))
        .unwrap_or_else(std::time::Instant::now);

    let strat = SumArbStrategy {
        min_edge_bps: s.min_edge_bps,
        warn_edge_bps: s.warn_edge_bps,
        fee_bps: s.fee_bps,
        max_bundle_size: s.max_bundle_size.parse::<Decimal>()?,
        max_leg_spread: parse_opt_decimal(&s.max_leg_spread)?,
        min_leg_size: parse_opt_decimal(&s.min_leg_size)?,
        stats: stats.clone(),
    };

    let ex = ExecutionObserver::new();

    loop {
        let refresh_due = markets.is_empty()
            || (s.markets_refresh_sec > 0
                && last_refresh.elapsed() >= std::time::Duration::from_secs(s.markets_refresh_sec));

        if refresh_due {
            tracing::info!(max_markets=s.max_markets, "refreshing open markets");
            markets = md.fetch_open_markets(s.max_markets).await?;
            last_refresh = std::time::Instant::now();
            tracing::info!(count=markets.len(), "open markets loaded");
            stats.set_markets_loaded(markets.len() as u64);
        }

        let snap = md.snapshot_for_markets(&markets).await?;
        stats.inc_heartbeat();
        stats.set_markets_in_snapshot(snap.markets.len() as u64);

        tracing::info!(markets=snap.markets.len(), ts=snap.ts_ms, "heartbeat: snapshot fetched");

        let intents = strat.on_snapshot(&snap);
        ex.execute(intents).await?;

        // stats summary
        let t = now_ms();
        if stats.should_log(t, s.stats_log_sec) {
            let ss = stats.snapshot(t);
            stats.mark_logged(t);

            let line = serde_json::to_string(&ss).unwrap_or_default();
            tracing::info!(
                up_sec = ss.up_sec,
                heartbeats = ss.heartbeats,
                markets_loaded = ss.markets_loaded,
                markets_in_snapshot = ss.markets_in_snapshot,
                near_arb_hits = ss.near_arb_hits,
                opportunities = ss.opportunities,
                intents_emitted = ss.intents_emitted,
                "stats"
            );

            maybe_write_jsonl(&s.stats_jsonl_path, &line).await;
        }

        tokio::time::sleep(std::time::Duration::from_millis(s.poll_ms)).await;
    }
}
