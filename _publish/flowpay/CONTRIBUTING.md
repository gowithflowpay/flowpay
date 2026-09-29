# Contributing to FlowPay

## Development workflow

1. Create a focused branch from `main`.
2. Keep changes scoped to one concern.
3. Add or update tests for behavior changes.
4. Run the relevant formatters, linters, builds, and tests.
5. Open a pull request using the repository template.

## Repository boundaries

- `backend/` contains Rust services and database migrations.
- `contracts/` contains Solidity contracts and tests.
- `apps/` contains merchant and checkout web applications.
- `sdk/` contains supported client integrations.
- `infra/` and `scripts/` contain development and deployment tooling.

## Security requirements

Never commit `.env` files, private keys, seed phrases, credentials, webhook secrets, production configuration, database dumps, or generated deployment broadcasts. Use `.env.example` with inert placeholders when documenting configuration.

All consequential recovery changes must preserve deterministic policy, simulation, approval, restricted signing, and receipt verification boundaries.
