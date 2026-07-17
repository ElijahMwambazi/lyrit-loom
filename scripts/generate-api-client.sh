#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [[ "${1:-}" == "--check" ]]; then
  generated="$(mktemp)"
  trap 'rm -f "$generated"' EXIT
  pnpm exec openapi-typescript contracts/openapi.yaml -o "$generated"
  if ! cmp --silent "$generated" packages/api-client/src/schema.d.ts; then
    printf '%s\n' 'generated API client is stale; run make generate-api' >&2
    diff --unified packages/api-client/src/schema.d.ts "$generated" || true
    exit 1
  fi
  exit 0
fi

pnpm exec openapi-typescript \
  contracts/openapi.yaml \
  -o packages/api-client/src/schema.d.ts
