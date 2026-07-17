import json
import tempfile
import unittest
from pathlib import Path

from lyrit_transcriber.cli import execute
from lyrit_transcriber.contract import ContractError


class FakeTranscriberTests(unittest.TestCase):
    def test_fake_mode_writes_atomic_contract_output(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            audio = root / "audio.wav"
            audio.write_bytes(b"fixture")
            output = root / "transcript.json"
            request_file = root / "request.json"
            request_file.write_text(
                json.dumps(
                    {
                        "contract_version": "1",
                        "request_id": "00000000-0000-4000-8000-000000000001",
                        "input_path": str(audio),
                        "output_path": str(output),
                        "language": "auto",
                        "model": "configured-default",
                        "word_timestamps": True,
                        "vad": {"enabled": True},
                        "initial_prompt": None,
                    }
                ),
                encoding="utf-8",
            )

            result_path = execute(request_file, "fake")
            result = json.loads(result_path.read_text(encoding="utf-8"))

            self.assertEqual(result["contract_version"], "1")
            self.assertEqual(result["model"]["engine"], "fake")
            self.assertGreater(len(result["segments"]), 0)
            self.assertFalse(output.with_suffix(".json.partial").exists())

    def test_rejects_unknown_contract_version(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            request_file = root / "request.json"
            request_file.write_text(
                json.dumps({"contract_version": "2"}), encoding="utf-8"
            )

            with self.assertRaises(ContractError):
                execute(request_file, "fake")


if __name__ == "__main__":
    unittest.main()
