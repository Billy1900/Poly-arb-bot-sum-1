use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

use crate::stats::Stats;
use crate::types::{GlobalSnapshot, OrderIntent, Side};
use super::Strategy;

#[derive(Clone)]
pub struct SumArbStrategy {
    pub min_edge_bps: i64,
    pub warn_edge_bps: i64,
    pub fee_bps: i64,
    pub max_bundle_size: Decimal,
    pub max_leg_spread: Option<Decimal>,
    pub min_leg_size: Option<Decimal>,
    pub stats: std::sync::Arc<Stats>,
}

impl SumArbStrategy {
    fn bps(bps: i64) -> Decimal {
        Decimal::from(bps) / dec!(10000)
    }
}

impl Strategy for SumArbStrategy {
    fn on_snapshot(&self, snap: &GlobalSnapshot) -> Vec<OrderIntent> {
        let mut out: Vec<OrderIntent> = vec![];
        let fee = Self::bps(self.fee_bps);
        let min_edge = Self::bps(self.min_edge_bps);
        let warn_edge = Self::bps(self.warn_edge_bps);

        for m in &snap.markets {
            // Sum-arb only makes sense when we have >=2 outcomes. If a market has a single
            // outcome token (bad mapping / partial data), sum_ask will be artificially low
            // and create false positives.
            if m.outcomes.len() < 2 { continue; }

            // Per-leg filters
            let mut ok = true;
            for o in &m.outcomes {
                let (ask, bid) = match (o.best_ask_px, o.best_bid_px) {
                    (Some(a), Some(b)) => (a, b),
                    _ => { ok = false; break; }
                };

                if let Some(max_spread) = self.max_leg_spread {
                    if ask - bid > max_spread { ok = false; break; }
                }
                if let Some(min_sz) = self.min_leg_size {
                    let a_sz = o.best_ask_sz.unwrap_or(Decimal::ZERO);
                    let b_sz = o.best_bid_sz.unwrap_or(Decimal::ZERO);
                    if a_sz < min_sz || b_sz < min_sz { ok = false; break; }
                }
            }
            if !ok { continue; }

            // sum_ask, sum_bid, bundle size cap
            let mut sum_ask = dec!(0);
            let mut sum_bid = dec!(0);
            let mut buy_cap: Option<Decimal> = None;

            for o in &m.outcomes {
                let (ask_px, ask_sz) = match (o.best_ask_px, o.best_ask_sz) {
                    (Some(px), Some(sz)) => (px, sz),
                    _ => { buy_cap = Some(Decimal::ZERO); break; }
                };
                let bid_px = o.best_bid_px.unwrap_or(Decimal::ZERO);

                sum_ask += ask_px;
                sum_bid += bid_px;
                buy_cap = Some(match buy_cap { None => ask_sz, Some(mm) => mm.min(ask_sz) });
            }

            let buy_cap = buy_cap.unwrap_or(Decimal::ZERO).min(self.max_bundle_size);
            if buy_cap <= Decimal::ZERO { continue; }

            // Near-arb warning
            if sum_ask < dec!(1) + warn_edge {
                self.stats.inc_near_arb();
                tracing::warn!(
                    market_id = %m.market_id,
                    question = %m.question,
                    sum_ask = %sum_ask,
                    sum_bid = %sum_bid,
                    spread = %(sum_ask - sum_bid),
                    size = %buy_cap,
                    legs = m.outcomes.len(),
                    "near-arb: bundle pricing close to 1"
                );
            }

            // Execute threshold
            if !(sum_ask * (dec!(1) + fee) < dec!(1) - min_edge) {
                continue;
            }

            self.stats.inc_opportunity();

            let bundle_id = Uuid::new_v4();
            tracing::info!(
                market_id = %m.market_id,
                question = %m.question,
                sum_ask = %sum_ask,
                size = %buy_cap,
                legs = m.outcomes.len(),
                "opportunity: BUY_BUNDLE"
            );

            for o in &m.outcomes {
                if let Some(px) = o.best_ask_px {
                    out.push(OrderIntent {
                        market_id: m.market_id.clone(),
                        token_id: o.token_id.clone(),
                        side: Side::Buy,
                        price: px,
                        size: buy_cap,
                        reason: format!("BUY_BUNDLE sum_ask={} size={}", sum_ask, buy_cap),
                        bundle_id,
                    });
                }
            }
        }

        self.stats.add_intents(out.len() as u64);
        out
    }
}
