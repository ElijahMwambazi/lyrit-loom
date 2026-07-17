from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Sequence

from .contract import ContractError, TranscriptionRequest
from .fake import transcribe as fake_transcribe


def execute(request_file: Path, mode: str) -> Path:
    raw = json.loads(request_file.read_text(encoding="utf-8"))
    if not isinstance(raw, dict):
        raise ContractError("request document must be a JSON object")

    request = TranscriptionRequest.from_mapping(raw)
    if not request.input_path.is_file():
        raise ContractError("input_path must reference a readable file")
    if mode != "fake":
        raise ContractError(
            "only LYRIT_TRANSCRIBER_MODE=fake is implemented in Milestone 0"
        )

    result = fake_transcribe(request)
    request.output_path.parent.mkdir(parents=True, exist_ok=True)
    temporary = request.output_path.with_suffix(request.output_path.suffix + ".partial")
    temporary.write_text(
        json.dumps(result, ensure_ascii=False, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )
    os.replace(temporary, request.output_path)
    return request.output_path


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Lyrit Loom transcription process adapter")
    parser.add_argument("--request", required=True, type=Path)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    mode = os.environ.get("LYRIT_TRANSCRIBER_MODE", "fake")
    try:
        output = execute(args.request, mode)
    except (ContractError, json.JSONDecodeError, OSError) as error:
        print(
            json.dumps(
                {
                    "level": "error",
                    "code": "invalid_request",
                    "message": str(error),
                }
            ),
            file=sys.stderr,
        )
        return 2

    print(json.dumps({"level": "info", "output_path": str(output)}))
    return 0
