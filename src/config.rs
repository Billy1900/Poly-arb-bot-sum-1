use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub clob_host: String,
    pub poll_ms: u64,

    pub max_markets: usize,
    pub markets_refresh_sec: u64,

    pub books_chunk_size: usize,
    pub books_concurrency: usize,

    pub fee_bps: i64,
    pub min_edge_bps: i64,
    pub warn_edge_bps: i64,
    pub max_bundle_size: String,

    // Optional filters
    pub max_leg_spread: Option<String>,
    pub min_leg_size: Option<String>,

    // Stats
    pub stats_log_sec: u64,
    pub stats_jsonl_path: Option<String>,
}

impl Settings {
    pub fn from_env() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();
        let c = config::Config::builder()
            .add_source(config::Environment::default())
            .build()?;
        Ok(c.try_deserialize()?)
    }
}
