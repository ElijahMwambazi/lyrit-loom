#!/usr/bin/env bash
set -euo pipefail

api_base="${API_BASE_URL:-http://localhost:8080/api/v1}"
response="$(curl --fail --silent --show-error \
  --request POST \
  --header 'Accept: application/json' \
  "$api_base/internal/dev/jobs/probe")"

job_id="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' <<<"$response")"
printf 'queued probe job %s\n' "$job_id"

for _ in $(seq 1 60); do
  response="$(curl --fail --silent --show-error "$api_base/jobs/$job_id")"
  status="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["status"])' <<<"$response")"
  phase="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["phase"])' <<<"$response")"
  printf 'status=%s phase=%s\n' "$status" "$phase"

  case "$status" in
    succeeded)
      printf 'durable worker probe passed\n'
      exit 0
      ;;
    failed|cancelled)
      printf 'durable worker probe failed: %s\n' "$response" >&2
      exit 1
      ;;
  esac
  sleep 0.5
done

printf 'timed out waiting for probe job\n' >&2
exit 1
