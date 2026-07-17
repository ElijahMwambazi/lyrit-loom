.PHONY: dev up down logs smoke install generate-api check rust-check web-check python-check config-check

dev:
	docker compose up --build

up:
	docker compose up --build --detach

down:
	docker compose down

logs:
	docker compose logs --follow api worker web

smoke:
	./scripts/smoke-job.sh

generate-api:
	./scripts/generate-api-client.sh

install:
	pnpm install --frozen-lockfile
	uv sync --project apps/transcriber --locked --all-groups

rust-check:
	cargo fmt --check
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	cargo test --workspace

web-check:
	./scripts/generate-api-client.sh --check
	pnpm typecheck
	pnpm test
	pnpm build:web

python-check:
	./scripts/test-transcriber.sh

config-check:
	./scripts/validate-config.sh
	pnpm exec redocly lint contracts/openapi.yaml

check: rust-check web-check python-check config-check
