#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
export UV_CACHE_DIR="${UV_CACHE_DIR:-/tmp/lyrit-loom-uv-cache}"
PYTHONPATH=apps/transcriber/src \
  uv run --project apps/transcriber --locked --all-groups \
  python -m unittest discover -s apps/transcriber/tests -v
