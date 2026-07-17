#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
export UV_CACHE_DIR="${UV_CACHE_DIR:-/tmp/lyrit-loom-uv-cache}"

while IFS= read -r script; do
  bash -n "$script"
done < <(find scripts -type f -name '*.sh' -print | sort)

uv run --project apps/transcriber --locked --all-groups \
  python scripts/validate-config.py
docker compose config --quiet
printf '%s\n' 'shell and Docker Compose validation passed'
