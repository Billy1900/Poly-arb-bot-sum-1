use serde::de::Deserializer;
use serde::Deserialize;

fn vec_or_empty<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let opt = Option::<Vec<T>>::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

fn string_or_empty<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

/// API wrapper for all responses
#[derive(Debug, Deserialize)]
pub struct APIBaseResponse<T> {
    // Opinion responses appear in two variants:
    // - {"code": 0, "msg": "", "result": ...}
    // - {"errno": 0, "errmsg": "", "result": ...}
    // Accept both without branching at call sites.
    #[serde(default, alias = "errno")]
    pub code: i32,
    #[serde(default, alias = "errmsg", deserialize_with = "string_or_empty")]
    pub msg: String,
    pub result: T,
}

/// Market list response wrapper
#[derive(Debug, Deserialize)]
pub struct MarketListResponse {
    pub total: i64,
    #[serde(default, deserialize_with = "vec_or_empty")]
    pub list: Vec<MarketItem>,
}

/// Market item from opinion.trade API
#[derive(Debug, Clone, Deserialize)]
pub struct MarketItem {
    #[serde(rename = "marketId")]
    pub market_id: i64,
    #[serde(rename = "marketTitle")]
    pub market_title: String,
    #[serde(rename = "marketType")]
    pub market_type: i32, // 0=Binary, 1=Categorical
    pub status: i32, // 1=Created, 2=Activated, 3=Resolving, 4=Resolved, 5=Failed, 6=Deleted
    #[serde(rename = "statusEnum")]
    pub status_enum: String,

    // For binary markets
    #[serde(rename = "yesTokenId", default, deserialize_with = "string_or_empty")]
    pub yes_token_id: String,
    #[serde(rename = "noTokenId", default, deserialize_with = "string_or_empty")]
    pub no_token_id: String,

    // For categorical markets
    #[serde(rename = "childMarkets", default, deserialize_with = "vec_or_empty")]
    pub child_markets: Vec<ChildMarket>,
}

/// Child market for categorical outcomes
#[derive(Debug, Clone, Deserialize)]
pub struct ChildMarket {
    #[serde(rename = "marketId")]
    pub market_id: i64,
    #[serde(rename = "yesTokenId", default, deserialize_with = "string_or_empty")]
    pub yes_token_id: String,
    #[serde(rename = "noTokenId", default, deserialize_with = "string_or_empty")]
    pub no_token_id: String,
}

/// Order book response
#[derive(Debug, Deserialize)]
pub struct OrderbookResponse {
    #[serde(rename = "tokenId")]
    pub token_id: String,
    #[serde(default, deserialize_with = "vec_or_empty")]
    pub bids: Vec<PriceLevel>,
    #[serde(default, deserialize_with = "vec_or_empty")]
    pub asks: Vec<PriceLevel>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PriceLevel {
    pub price: String,
    pub size: String,
}
