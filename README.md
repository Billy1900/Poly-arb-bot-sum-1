# Polymarket Multi-Outcome Sum Arbitrage Bot (Rust)

This repo is a **runnable starter** for the "sum(outcome prices) < 1" multi-outcome (bundle) arbitrage idea.

Scans **N markets automatically** and detects multi-outcome "bundle" opportunities:

`sum(best_ask_i) * (1 + fee) < 1 - edge`

✅ **Default mode (observer)**: polls CLOB `POST /books` and logs bundle opportunities (safe to run).  
⚠️ **Live mode**: scaffolding is included behind `--features live` for placing concurrent FOK orders and doing basic post-trade reconciliation,
but you must wire the exact SDK order builder methods for your environment if the SDK API has changed.

## Observer mode (default)

1. Copy `.env.example` to `.env`
2. Set `MAX_MARKETS`
    - Periodic refresh of open markets list (`MARKETS_REFRESH_SEC`)
    - Chunked + concurrent `/books` fetch to avoid huge payloads (`BOOKS_CHUNK_SIZE`, `BOOKS_CONCURRENCY`)
3. Run:

```bash
RUST_LOG=info cargo run
```

## How it works

- Fetch markets from CLOB `/markets` with pagination, filter:
  `enable_order_book && accepting_orders && !closed`
- Batch top-of-book with `POST /books`
- Compute best bid/ask as **max bid** / **min ask** (do not assume sorting).

## Project structure

- `src/pm/market_data.rs`: fetches top-of-book for all outcome tokens
- `src/strategy/sum_arb.rs`: computes sum ask and emits bundle buy intents
- `src/pm/execution_observer.rs`: logs intents
- `src/pm/execution_live.rs` (feature-gated): live trading scaffolding
