use crate::pm::market_data::MarketData;
use crate::source::MarketDataSource;
use async_trait::async_trait;

/// Wrapper for Polymarket's MarketData implementing MarketDataSource trait
pub struct PolymarketSource {
    inner: MarketData,
}

impl PolymarketSource {
    pub fn new(host: String, chunk_size: usize, concurrency: usize) -> Self {
        Self {
            inner: MarketData::new(host, chunk_size, concurrency),
        }
    }
}

#[async_trait]
impl MarketDataSource for PolymarketSource {
    async fn fetch_open_markets(&self, max_markets: usize) -> anyhow::Result<Vec<crate::pm::market_data::MarketDef>> {
        self.inner.fetch_open_markets(max_markets).await
    }

    async fn snapshot_for_markets(&self, markets: &[crate::pm::market_data::MarketDef]) -> anyhow::Result<crate::types::GlobalSnapshot> {
        self.inner.snapshot_for_markets(markets).await
    }
}
