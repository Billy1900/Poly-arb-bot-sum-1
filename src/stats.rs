use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Default)]
pub struct Stats {
    start_ms: AtomicU64,
    last_log_ms: AtomicU64,

    heartbeats: AtomicU64,
    markets_loaded: AtomicU64,
    markets_in_snapshot: AtomicU64,

    near_arb_hits: AtomicU64,
    opportunities: AtomicU64,
    intents_emitted: AtomicU64,
}

impl Stats {
    pub fn new(now_ms: u64) -> Arc<Self> {
        let s = Arc::new(Self::default());
        s.start_ms.store(now_ms, Ordering::Relaxed);
        s.last_log_ms.store(now_ms, Ordering::Relaxed);
        s
    }

    pub fn inc_heartbeat(&self) {
        self.heartbeats.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_markets_loaded(&self, n: u64) {
        self.markets_loaded.store(n, Ordering::Relaxed);
    }

    pub fn set_markets_in_snapshot(&self, n: u64) {
        self.markets_in_snapshot.store(n, Ordering::Relaxed);
    }

    pub fn inc_near_arb(&self) {
        self.near_arb_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_opportunity(&self) {
        self.opportunities.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_intents(&self, n: u64) {
        self.intents_emitted.fetch_add(n, Ordering::Relaxed);
    }

    pub fn should_log(&self, now_ms: u64, every_sec: u64) -> bool {
        if every_sec == 0 { return false; }
        let last = self.last_log_ms.load(Ordering::Relaxed);
        now_ms.saturating_sub(last) >= every_sec.saturating_mul(1000)
    }

    pub fn mark_logged(&self, now_ms: u64) {
        self.last_log_ms.store(now_ms, Ordering::Relaxed);
    }

    pub fn snapshot(&self, now_ms: u64) -> StatsSnapshot {
        let start = self.start_ms.load(Ordering::Relaxed);
        StatsSnapshot {
            now_ms,
            up_sec: ((now_ms.saturating_sub(start)) / 1000) as u64,
            heartbeats: self.heartbeats.load(Ordering::Relaxed),
            markets_loaded: self.markets_loaded.load(Ordering::Relaxed),
            markets_in_snapshot: self.markets_in_snapshot.load(Ordering::Relaxed),
            near_arb_hits: self.near_arb_hits.load(Ordering::Relaxed),
            opportunities: self.opportunities.load(Ordering::Relaxed),
            intents_emitted: self.intents_emitted.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StatsSnapshot {
    pub now_ms: u64,
    pub up_sec: u64,
    pub heartbeats: u64,
    pub markets_loaded: u64,
    pub markets_in_snapshot: u64,
    pub near_arb_hits: u64,
    pub opportunities: u64,
    pub intents_emitted: u64,
}
