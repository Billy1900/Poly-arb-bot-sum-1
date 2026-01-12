pub mod polymarket;

use crate::pm::market_data::MarketDef;
use crate::types::GlobalSnapshot;
use async_trait::async_trait;

/// Abstraction for market data sources (Polymarket, opinion.trade, etc.)
#[async_trait]
pub trait MarketDataSource: Send + Sync {
    /// Fetch list of open/active markets up to max_markets
    async fn fetch_open_markets(&self, max_markets: usize) -> anyhow::Result<Vec<MarketDef>>;

    /// Fetch order book snapshot for given markets
    async fn snapshot_for_markets(&self, markets: &[MarketDef]) -> anyhow::Result<GlobalSnapshot>;
}

pub use polymarket::PolymarketSource;
