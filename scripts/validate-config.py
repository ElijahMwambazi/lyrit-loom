from __future__ import annotations

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

    print("JSON, JSON Schema, YAML, and TOML validation passed")


if __name__ == "__main__":
    main()
