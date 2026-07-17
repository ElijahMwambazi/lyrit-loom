#!/usr/bin/env bash
set -euo pipefail

missing=0
for command in cargo rustc node pnpm python3 uv docker; do
  if ! command -v "$command" >/dev/null 2>&1; then
    printf 'missing: %s\n' "$command" >&2
    missing=1
  else
    printf 'found:   %s\n' "$command"
  fi
done

if ! docker compose version >/dev/null 2>&1; then
  printf 'missing: docker compose plugin\n' >&2
  missing=1
fi

exit "$missing"
