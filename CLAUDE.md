# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust-based arbitrage bot for Polymarket that detects "sum(outcome prices) < 1" opportunities across multi-outcome markets. The bot runs in two modes:

- **Observer mode (default)**: Scans markets and logs bundle opportunities without trading
- **Live mode** (feature-gated): Places concurrent FOK orders for real arbitrage execution

## Build and Run Commands

### Development (Observer Mode)
```bash
# Run with default observer features
RUST_LOG=info cargo run

# Build only
cargo build

# Check code
cargo check
```

### Live Trading Mode
```bash
# Requires additional .env variables (POLYMARKET_PRIVATE_KEY, CHAIN_ID, SIGNATURE_TYPE)
cargo run --no-default-features --features live
```

### Testing
```bash
# Run tests
cargo test

# Run tests with output
cargo test -- --nocapture
```

### Configuration
Copy [`.env.example`](.env.example) to `.env` and configure:
- `CLOB_HOST`: Polymarket CLOB API endpoint
- `POLL_MS`: Snapshot refresh interval (default: 750ms)
- `MAX_MARKETS`: Number of markets to scan (default: 200)
- `BOOKS_CHUNK_SIZE` / `BOOKS_CONCURRENCY`: Batch fetch controls for `/books` endpoint
- `FEE_BPS`: Estimated fee in basis points
- `MIN_EDGE_BPS`: Execution threshold (must exceed to trigger buy)
- `WARN_EDGE_BPS`: Warning threshold (logs when close to arbitrage)

## Architecture

### Core Components

**[src/main.rs](src/main.rs)**: Main event loop
- Periodically refreshes market list ([`MARKETS_REFRESH_SEC`](.env.example#L11))
- Fetches top-of-book snapshots via [`MarketData`](src/pm/market_data.rs#L15)
- Executes strategy logic via [`SumArbStrategy`](src/strategy/sum_arb.rs#L10)
- Logs stats summary every [`STATS_LOG_SEC`](.env.example#L34)

**[src/pm/market_data.rs](src/pm/market_data.rs)**: Market data fetching
- `fetch_open_markets()`: Paginated GET `/markets` with filtering (`enable_order_book && accepting_orders && !closed`)
- `snapshot_for_markets()`: Batch POST `/books` with chunking/concurrency for top-of-book
- Computes best bid/ask as **max bid** / **min ask** (no sorting assumption)

**[src/strategy/sum_arb.rs](src/strategy/sum_arb.rs)**: Arbitrage strategy
- Implements `Strategy` trait from [`src/strategy/mod_.rs`](src/strategy/mod_.rs#L3)
- Condition: `sum(best_ask_i) * (1 + fee) < 1 - edge`
- Optional per-leg filters: [`MAX_LEG_SPREAD`](.env.example#L28), [`MIN_LEG_SIZE`](.env.example#L30)
- Emits `OrderIntent` for each outcome token with shared `bundle_id`

**[src/pm/execution_observer.rs](src/pm/execution_observer.rs)**: Default execution (logs only)
- Groups intents by `bundle_id` and logs

**[src/stats.rs](src/stats.rs)**: Thread-safe statistics tracking
- Atomic counters for heartbeats, opportunities, intents
- Periodic logging to console and optional JSONL file ([`STATS_JSONL_PATH`](.env.example#L36))

### Data Flow

1. **Market Discovery**: [`fetch_open_markets()`](src/pm/market_data.rs#L33) → `Vec<MarketDef>`
2. **Snapshot**: [`snapshot_for_markets()`](src/pm/market_data.rs#L77) → `GlobalSnapshot` with top-of-book for all tokens
3. **Strategy**: [`SumArbStrategy::on_snapshot()`](src/strategy/sum_arb.rs#L27) → `Vec<OrderIntent>`
4. **Execution**: [`ExecutionObserver::execute()`](src/pm/execution_observer.rs#L10) → logging or live orders

### Key Types ([src/types.rs](src/types.rs))

- `MarketDef`: Market metadata (ID, question, token IDs)
- `OutcomeTop`: Top-of-book for single outcome token
- `MarketBook`: Collection of `OutcomeTop` for a market
- `GlobalSnapshot`: Timestamped collection of `MarketBook`
- `OrderIntent`: Single order with `bundle_id` for grouping

## Feature Flags

- `observer` (default): Observer mode only
- `live`: Live trading with `polymarket-client-sdk` (requires SDK wiring for order placement)

## Important Notes

- **Do not assume sorted order books**: Best bid/ask are computed as max/min
- **Chunked API calls**: `/books` endpoint is called in chunks to avoid large payloads
- **Bundle size cap**: Limited by `MAX_BUNDLE_SIZE` and minimum available ask size across legs
- **Near-arb warnings**: Logs when `sum_ask < 1 + WARN_EDGE_BPS/10000` even if not executing
- **Thread safety**: `Stats` uses atomic counters for concurrent access

## Important Remarks
- User may speak Chinese or English, but **ALL code and comments SHALL BE IN ENGLISH**.
- Implementation should **priority latency performance to maximize arb profit**.
    - Ensure hot path to have **NO error handling and I/O operations**
    - **Minimal** error handling and corner case handling.
    - Fail hard for error that will affect strategies execution and data accuracy.
    - DO NOT over-engineering about the system design.
