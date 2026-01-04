# Copilot Instructions

## Project Context

Rust arbitrage bot for Polymarket detecting "sum(outcome prices) < 1" opportunities across multi-outcome markets. Runs in observer mode (logs opportunities) or live mode (executes trades).

## Architecture Pattern

**Event Loop Design**: [src/main.rs](../src/main.rs) orchestrates a periodic loop:
1. Refresh market list every `MARKETS_REFRESH_SEC` via `MarketData::fetch_open_markets()`
2. Fetch top-of-book snapshots for all tokens via `MarketData::snapshot_for_markets()`
3. Run strategy logic via `SumArbStrategy::on_snapshot()` → emits `Vec<OrderIntent>`
4. Execute intents via `ExecutionObserver` (logs only) or live executor (feature-gated)

**Data Flow**: `MarketDef` → `GlobalSnapshot` (with `MarketBook` per market) → `Vec<OrderIntent>` → execution layer

**Strategy Pattern**: All strategies implement the `Strategy` trait ([src/strategy/mod_.rs](../src/strategy/mod_.rs)):
```rust
fn on_snapshot(&self, snap: &GlobalSnapshot) -> Vec<OrderIntent>
```

## Critical Performance Requirements

**Hot Path Optimization** - This is a latency-sensitive arbitrage system:
- **NO error handling or I/O operations** in hot paths (strategy logic, order intent generation)
- **Fail hard** for errors affecting strategy execution or data accuracy
- **Minimal** corner case handling and error recovery
- **DO NOT over-engineer** the system design - simplicity over robustness
- Priority: maximize arbitrage profit through minimal latency

**Market Data Fetching** ([src/pm/market_data.rs](../src/pm/market_data.rs)):
- **Never assume sorted order books**: compute best bid/ask as `max(bids)` / `min(asks)` explicitly
- Chunked API calls to `/books` endpoint to avoid large payloads (controlled by `BOOKS_CHUNK_SIZE`, `BOOKS_CONCURRENCY`)
- Markets filtered: `enable_order_book && accepting_orders && !closed`
- Order book computed in `convert_book_to_top()` - compares all levels, no sorting assumption

## Development Workflows

```bash
# Observer mode (default, safe to run)
RUST_LOG=info cargo run

# Build and check
cargo build
cargo check

# Live trading mode (requires .env with POLYMARKET_PRIVATE_KEY, CHAIN_ID, SIGNATURE_TYPE)
cargo run --no-default-features --features live

# Testing
cargo test
cargo test -- --nocapture
```

## Configuration Pattern

All config via environment variables loaded in [src/config.rs](../src/config.rs) using `config` crate:
- Copy `.env.example` to `.env`
- Settings struct derives from environment using `config::Environment::default()`
- Decimal-like values stored as `String` in config, parsed on-demand with `parse::<Decimal>()?`

**Key environment variables**:
- `CLOB_HOST`: Polymarket CLOB API endpoint
- `POLL_MS`: Snapshot refresh interval (default: 750ms)
- `MAX_MARKETS`: Number of markets to scan (default: 200)
- `MARKETS_REFRESH_SEC`: Market list refresh interval
- `BOOKS_CHUNK_SIZE` / `BOOKS_CONCURRENCY`: Batch fetch controls for `/books` endpoint
- `MIN_EDGE_BPS`: execution threshold - condition is `sum(best_ask_i) * (1 + fee) < 1 - edge`
- `WARN_EDGE_BPS`: logs near-arbitrage warnings without executing (sum_ask < 1 + warn_edge)
- `FEE_BPS`: estimated trading fee in basis points
- `MAX_BUNDLE_SIZE`: per-trade size cap
- `MAX_LEG_SPREAD` / `MIN_LEG_SIZE`: optional per-leg filters
- `STATS_LOG_SEC`: stats logging interval
- `STATS_JSONL_PATH`: optional JSONL stats output file

## Key Types ([src/types.rs](../src/types.rs))

- `MarketDef`: Market metadata (market_id, question, token_ids)
- `OutcomeTop`: Top-of-book for single outcome token (best_bid/ask with px/sz)
- `MarketBook`: Collection of `OutcomeTop` for a market
- `GlobalSnapshot`: Timestamped collection of `MarketBook` (ts_ms + markets vec)
- `OrderIntent`: Single order with `bundle_id` for grouping multi-leg trades

## Project-Specific Patterns

**Bundle ID Grouping**: Multi-leg orders share a `bundle_id: Uuid` for atomic tracking - all legs of an arbitrage bundle get same UUID for post-trade reconciliation

**Thread-Safe Stats** ([src/stats.rs](../src/stats.rs)): All counters use `AtomicU64` for lock-free concurrent access. Stats logged every `STATS_LOG_SEC` and optionally written to JSONL.

**Feature Flags**:
- `observer` (default): logs only, no external SDK dependencies
- `live`: includes `polymarket-client-sdk` and `alloy` for order execution

**Bundle Size Cap**: Capped by `min(ask_size_i for all legs, MAX_BUNDLE_SIZE)` - buy size limited by smallest leg liquidity

**Near-Arb Warnings**: Logs when `sum_ask < 1 + WARN_EDGE_BPS/10000` even if not executing, helps track near-miss opportunities

## Code Conventions

- **All code and comments in English** (user may communicate in Chinese/English)
- Use `rust_decimal` for price/size precision (never `f64` for financial values)
- Error context via `anyhow::Context` trait: `.context("descriptive error")?`
- Logging via `tracing` crate with structured fields: `tracing::info!(field=%value, "message")`
- Timestamps as `i64` milliseconds (UNIX epoch)
