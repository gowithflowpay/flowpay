.PHONY: start dev test e2e backend-test contracts-test merchant checkout migrate seed reset

start:
	./scripts/start-all.sh

dev:
	./scripts/dev.sh

backend-test:
	cargo test --manifest-path backend/Cargo.toml --workspace

contracts-test:
	cd contracts && forge test

test:
	./scripts/test.sh

e2e:
	./scripts/run-e2e.sh all

merchant:
	cd apps/merchant && npm run dev

checkout:
	cd apps/checkout && npm run dev

migrate:
	./scripts/migrate.sh

seed:
	./scripts/seed.sh

reset:
	./scripts/reset-local.sh
