use anyhow::Result;
use crate::types::OrderIntent;

#[derive(Clone)]
pub struct ExecutionObserver;

impl ExecutionObserver {
    pub fn new() -> Self { Self }

    pub async fn execute(&self, intents: Vec<OrderIntent>) -> Result<()> {
        if intents.is_empty() {
            return Ok(());
        }
        let mut by_bundle: std::collections::HashMap<uuid::Uuid, Vec<OrderIntent>> = std::collections::HashMap::new();
        for i in intents {
            by_bundle.entry(i.bundle_id).or_default().push(i);
        }

        for (bid, legs) in by_bundle {
            let market_id = legs.get(0).map(|x| x.market_id.clone()).unwrap_or_default();
            tracing::info!(bundle_id=%bid, market_id=%market_id, legs=legs.len(), "bundle intents");
            for i in legs {
                tracing::info!(
                    bundle_id=%i.bundle_id,
                    market_id=%i.market_id,
                    token_id=%i.token_id,
                    price=%i.price,
                    size=%i.size,
                    reason=%i.reason,
                    "intent"
                );
            }
        }

        Ok(())
    }
}
