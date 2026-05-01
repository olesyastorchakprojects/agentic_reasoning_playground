from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import jsonschema

from Execution.evals.judge_transport import (
    JudgeConfigError,
    create_chat_completion_with_retry,
    load_judge_settings,
)


REPO_ROOT = Path(__file__).resolve().parents[2]


class JudgeTransportTests(unittest.TestCase):
    def test_retry_helper_retries_retryable_status_and_then_succeeds(self) -> None:
        settings = load_judge_settings(
            REPO_ROOT / "Execution" / "evals" / "eval_engine.toml",
            env={
                "OPENAI_COMPATIBLE_URL": "https://api.together.xyz",
                "TOGETHER_API_KEY": "test-key",
            },
        )

        class RetryableError(RuntimeError):
            def __init__(self, status_code: int) -> None:
                super().__init__(f"status={status_code}")
                self.status_code = status_code

        class FakeCompletions:
            def __init__(self) -> None:
                self.calls = 0

            def create(self, **_: object) -> dict[str, str]:
                self.calls += 1
                if self.calls < 3:
                    raise RetryableError(503)
                return {"ok": "true"}

        fake_completions = FakeCompletions()
        fake_client = type(
            "FakeClient",
            (),
            {"chat": type("FakeChat", (), {"completions": fake_completions})()},
        )()

        with patch("Execution.evals.judge_transport.time.sleep") as sleep_mock:
            response = create_chat_completion_with_retry(
                fake_client,
                settings,
                messages=[{"role": "user", "content": "hello"}],
            )

        self.assertEqual(response, {"ok": "true"})
        self.assertEqual(fake_completions.calls, 3)
        self.assertEqual(sleep_mock.call_count, 2)

    def test_retry_helper_does_not_retry_non_retryable_status(self) -> None:
        settings = load_judge_settings(
            REPO_ROOT / "Execution" / "evals" / "eval_engine.toml",
            env={
                "OPENAI_COMPATIBLE_URL": "https://api.together.xyz",
                "TOGETHER_API_KEY": "test-key",
            },
        )

        class NonRetryableError(RuntimeError):
            def __init__(self, status_code: int) -> None:
                super().__init__(f"status={status_code}")
                self.status_code = status_code

        class FakeCompletions:
            def __init__(self) -> None:
                self.calls = 0

            def create(self, **_: object) -> dict[str, str]:
                self.calls += 1
                raise NonRetryableError(400)

        fake_completions = FakeCompletions()
        fake_client = type(
            "FakeClient",
            (),
            {"chat": type("FakeChat", (), {"completions": fake_completions})()},
        )()

        with patch("Execution.evals.judge_transport.time.sleep") as sleep_mock:
            with self.assertRaises(NonRetryableError):
                create_chat_completion_with_retry(
                    fake_client,
                    settings,
                    messages=[{"role": "user", "content": "hello"}],
                )

        self.assertEqual(fake_completions.calls, 1)
        sleep_mock.assert_not_called()

    def test_loads_ollama_settings_with_only_active_env_and_costs(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = Path(tmpdir) / "eval_engine.toml"
            config_path.write_text(
                "\n".join(
                    [
                        "[judge]",
                        'provider = "ollama"',
                        'model_name = "qwen2.5:1.5b-instruct-q4_K_M"',
                        'tokenizer_source = "Qwen/Qwen2.5-1.5B-Instruct"',
                        "input_cost_per_million_tokens = 0.05",
                        "output_cost_per_million_tokens = 0.20",
                        "",
                        "[judge.ollama]",
                        "timeout_sec = 30",
                    ]
                ),
                encoding="utf-8",
            )

            settings = load_judge_settings(
                config_path,
                env={"OLLAMA_URL": "http://localhost:11434"},
            )

        self.assertEqual(settings.provider, "ollama")
        self.assertEqual(settings.model_name, "qwen2.5:1.5b-instruct-q4_K_M")
        self.assertEqual(settings.tokenizer_source, "Qwen/Qwen2.5-1.5B-Instruct")
        self.assertEqual(settings.base_url, "http://localhost:11434/v1")
        self.assertEqual(settings.api_key, "unused")
        self.assertEqual(settings.timeout_sec, 30)
        self.assertEqual(settings.input_cost_per_million_tokens, 0.05)
        self.assertEqual(settings.output_cost_per_million_tokens, 0.20)

    def test_loads_together_settings_with_only_active_env_and_costs(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = Path(tmpdir) / "eval_engine.toml"
            config_path.write_text(
                "\n".join(
                    [
                        "[judge]",
                        'provider = "together"',
                        'model_name = "openai/gpt-oss-20b"',
                        'tokenizer_source = "Qwen/Qwen2.5-1.5B-Instruct"',
                        "input_cost_per_million_tokens = 0.05",
                        "output_cost_per_million_tokens = 0.20",
                        "",
                        "[judge.together]",
                        "timeout_sec = 45",
                    ]
                ),
                encoding="utf-8",
            )

            settings = load_judge_settings(
                config_path,
                env={
                    "OPENAI_COMPATIBLE_URL": "https://api.together.xyz",
                    "TOGETHER_API_KEY": "test-key",
                },
            )

        self.assertEqual(settings.provider, "together")
        self.assertEqual(settings.model_name, "openai/gpt-oss-20b")
        self.assertEqual(settings.tokenizer_source, "Qwen/Qwen2.5-1.5B-Instruct")
        self.assertEqual(settings.base_url, "https://api.together.xyz/v1")
        self.assertEqual(settings.api_key, "test-key")
        self.assertEqual(settings.timeout_sec, 45)
        self.assertEqual(settings.input_cost_per_million_tokens, 0.05)
        self.assertEqual(settings.output_cost_per_million_tokens, 0.20)

    def test_missing_active_env_var_raises_clear_error(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = Path(tmpdir) / "eval_engine.toml"
            config_path.write_text(
                "\n".join(
                    [
                        "[judge]",
                        'provider = "together"',
                        'model_name = "openai/gpt-oss-20b"',
                        'tokenizer_source = "Qwen/Qwen2.5-1.5B-Instruct"',
                        "input_cost_per_million_tokens = 0.05",
                        "output_cost_per_million_tokens = 0.20",
                        "",
                        "[judge.together]",
                        "timeout_sec = 45",
                    ]
                ),
                encoding="utf-8",
            )

            with self.assertRaises(JudgeConfigError) as ctx:
                load_judge_settings(
                    config_path,
                    env={"OPENAI_COMPATIBLE_URL": "https://api.together.xyz"},
                )

        self.assertIn("TOGETHER_API_KEY", str(ctx.exception))

    def test_example_eval_engine_toml_validates_against_schema(self) -> None:
        config_path = REPO_ROOT / "Execution" / "evals" / "eval_engine.toml"
        schema_path = REPO_ROOT / "Execution" / "schemas" / "evals" / "eval_engine_config.schema.json"

        try:
            import tomllib as toml_parser  # type: ignore
        except ModuleNotFoundError:
            import tomli as toml_parser  # type: ignore

        config_data = toml_parser.loads(config_path.read_text(encoding="utf-8"))
        schema = json.loads(schema_path.read_text(encoding="utf-8"))

        jsonschema.validate(instance=config_data, schema=schema)


if __name__ == "__main__":
    unittest.main()
