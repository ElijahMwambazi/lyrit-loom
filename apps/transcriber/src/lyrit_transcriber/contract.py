from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any


class ContractError(ValueError):
    """Raised when the Rust-to-Python process contract is invalid."""


@dataclass(frozen=True)
class TranscriptionRequest:
    contract_version: str
    request_id: str
    input_path: Path
    output_path: Path
    language: str
    model: str
    word_timestamps: bool
    vad_enabled: bool
    initial_prompt: str | None

    @classmethod
    def from_mapping(cls, value: dict[str, Any]) -> TranscriptionRequest:
        if value.get("contract_version") != "1":
            raise ContractError("contract_version must be '1'")

        request_id = require_string(value, "request_id")
        input_path = Path(require_string(value, "input_path"))
        output_path = Path(require_string(value, "output_path"))
        language = value.get("language", "auto")
        model = value.get("model", "configured-default")
        word_timestamps = value.get("word_timestamps", True)
        vad = value.get("vad", {"enabled": True})
        initial_prompt = value.get("initial_prompt")

        if not isinstance(language, str) or not language:
            raise ContractError("language must be a non-empty string")
        if not isinstance(model, str) or not model:
            raise ContractError("model must be a non-empty string")
        if word_timestamps is not True:
            raise ContractError("word_timestamps must be true")
        if not isinstance(vad, dict) or not isinstance(vad.get("enabled", True), bool):
            raise ContractError("vad.enabled must be a boolean")
        if initial_prompt is not None and not isinstance(initial_prompt, str):
            raise ContractError("initial_prompt must be a string or null")
        if input_path == output_path:
            raise ContractError("input_path and output_path must differ")

        return cls(
            contract_version="1",
            request_id=request_id,
            input_path=input_path,
            output_path=output_path,
            language=language,
            model=model,
            word_timestamps=True,
            vad_enabled=vad.get("enabled", True),
            initial_prompt=initial_prompt,
        )


def require_string(value: dict[str, Any], key: str) -> str:
    item = value.get(key)
    if not isinstance(item, str) or not item:
        raise ContractError(f"{key} must be a non-empty string")
    return item
