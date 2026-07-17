from __future__ import annotations

import hashlib
import json
import os
import tomllib
from pathlib import Path

import yaml
from jsonschema.validators import Draft202012Validator


REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
EXCLUDED_PARTS = {
    ".git",
    ".pytest_cache",
    ".venv",
    "__pycache__",
    "dist",
    "node_modules",
    "target",
}
IMMUTABLE_MIGRATION_SHA384 = {
    "db/migrations/0001_jobs.sql": (
        "e57ff68e88b40b22ffb20c7faf041c02800c17db150579fed807648e02712b19"
        "819869c8267d9b4b6f71f7b4c4a09d64"
    ),
    "db/migrations/0002_projects_and_assets.sql": (
        "c22443a364c10f04cca97d9cb6f62037ba0283111574ca35d0240f434212a613"
        "d2758b8aeb727faab1c4daed5a125f11"
    ),
}


def repository_files(*suffixes: str) -> list[Path]:
    matches: list[Path] = []
    for directory, child_directories, filenames in os.walk(REPOSITORY_ROOT):
        child_directories[:] = [
            name for name in child_directories if name not in EXCLUDED_PARTS
        ]
        root = Path(directory)
        matches.extend(
            root / filename
            for filename in filenames
            if Path(filename).suffix in suffixes
        )
    return sorted(matches)


def main() -> None:
    for relative_path, expected_checksum in IMMUTABLE_MIGRATION_SHA384.items():
        migration_path = REPOSITORY_ROOT / relative_path
        actual_checksum = hashlib.sha384(migration_path.read_bytes()).hexdigest()
        if actual_checksum != expected_checksum:
            raise ValueError(
                f"immutable migration changed: {relative_path}; add a new migration instead"
            )

    for path in repository_files(".json"):
        with path.open(encoding="utf-8") as handle:
            json.load(handle)

    schema_path = REPOSITORY_ROOT / "contracts/transcriber.schema.json"
    with schema_path.open(encoding="utf-8") as handle:
        Draft202012Validator.check_schema(json.load(handle))

    for path in repository_files(".yaml", ".yml"):
        with path.open(encoding="utf-8") as handle:
            list(yaml.safe_load_all(handle))

    for path in repository_files(".toml"):
        with path.open("rb") as handle:
            tomllib.load(handle)

    print("Migration, JSON, JSON Schema, YAML, and TOML validation passed")


if __name__ == "__main__":
    main()
