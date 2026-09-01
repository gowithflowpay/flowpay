# Reproduction guide

## Requirements

- Rust 1.88 or newer
- PostgreSQL 16
- RabbitMQ 3.13 or newer
- Kafka 3.x
- Foundry (`forge`, `anvil`, and `cast`)
- Node.js 20 or newer
- `psql`, `pg_isready`, `curl`, and `jq` on `PATH`
- Ollama with a tool-capable local model

PostgreSQL, RabbitMQ, and Kafka run as native services. Configure their endpoints in `.env`.

## Prepare and run

```bash
cp .env.example .env
(cd apps/merchant && npm install)
(cd apps/checkout && npm install)
./scripts/bootstrap-local.sh
make dev
```

The bootstrap verifies PostgreSQL, starts both local Anvil chains, deploys contracts, applies migrations, and seeds local data. RabbitMQ and Kafka must already be running. `make dev` starts the API, worker, event relay, merchant UI, and checkout UI.

```bash
curl http://localhost:8080/health
```

```text
http://localhost:3000  merchant UI
http://localhost:3001  hosted checkout
http://localhost:8080  API
```

## Model-driven investigation

The default `.env.example` uses local Ollama with `qwen2.5-coder:7b`:

```bash
export FLOWPAY_AGENT_MODE=model
export FLOWPAY_MODEL_PROVIDER=ollama
export FLOWPAY_AGENT_MODEL=qwen2.5-coder:7b
make dev
```

Ollama is the only supported investigative provider. If Ollama is unavailable, the worker retries within its configured budget and escalates the claim safely.

## Quality gates

```bash
cargo fmt --manifest-path backend/Cargo.toml --all -- --check
cargo clippy --manifest-path backend/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --manifest-path backend/Cargo.toml --workspace
(cd contracts && forge test)
(cd apps/merchant && npm run build)
(cd apps/checkout && npm run build)
```

## Evaluation

```bash
./scripts/run-evals.sh
./scripts/run-e2e-evals.sh baseline
./scripts/run-e2e-evals.sh model
./scripts/run-e2e-evals.sh all
```

The E2E harness resets PostgreSQL and both Anvil chains for each mode. Model mode requires its explicitly configured provider. Do not report the legacy fixture evaluator as the live-system benchmark.
