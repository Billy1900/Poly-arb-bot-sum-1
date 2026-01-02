use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutcomeTop {
    pub token_id: String,
    pub best_bid_px: Option<Decimal>,
    pub best_bid_sz: Option<Decimal>,
    pub best_ask_px: Option<Decimal>,
    pub best_ask_sz: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketBook {
    pub market_id: String,
    pub question: String,
    pub outcomes: Vec<OutcomeTop>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSnapshot {
    pub ts_ms: i64,
    pub markets: Vec<MarketBook>,
}

#[derive(Debug, Clone)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone)]
pub struct OrderIntent {
    pub market_id: String,
    pub token_id: String,
    pub side: Side,
    pub price: Decimal,
    pub size: Decimal,
    pub reason: String,
    pub bundle_id: Uuid,
}
