from __future__ import annotations

import argparse
import io
import tempfile
import unittest
from pathlib import Path
from unittest.mock import MagicMock, patch

from Execution.bin import run_stack


class RunStackEvalEngineTests(unittest.TestCase):
    def test_run_eval_engine_passes_eval_config_to_orchestrator(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            chunks_path = Path(tmpdir) / "chunks.jsonl"
            eval_config_path = Path(tmpdir) / "eval_engine.toml"
            chunks_path.write_text("", encoding="utf-8")
            eval_config_path.write_text(
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
                        "timeout_sec = 30",
                    ]
                ),
                encoding="utf-8",
            )

            args = argparse.Namespace(
                scenario=None,
                chunk_profile="structural",
                chunks_path=chunks_path,
                eval_config=eval_config_path,
                run_type="experiment",
                postgres_url="postgres://postgres:postgres@localhost:5432/rag_eval",
                tracing_endpoint="http://localhost:4317",
                resume_run_id=None,
                resume_failed_run=False,
                dry_run=False,
            )

            with patch.object(
                run_stack,
                "load_dotenv",
                return_value={},
            ), patch.object(
                run_stack,
                "resolve_judge_runtime",
                return_value=("together", "https://api.together.xyz/v1", "openai/gpt-oss-20b"),
            ) as resolve_mock, patch.object(
                run_stack,
                "print_eval_engine_plan",
            ), patch.object(
                run_stack,
                "maybe_run",
                return_value=0,
            ) as maybe_run_mock:
                result = run_stack.run_eval_engine(args)

        self.assertEqual(result, 0)
        resolve_mock.assert_called_once_with(eval_config_path.resolve(), {})

        command = maybe_run_mock.call_args.args[0]
        self.assertEqual(command[0], str(run_stack.VENV_PYTHON))
        self.assertIn("--eval-config", command)
        self.assertIn(str(eval_config_path.resolve()), command)
        self.assertNotIn("--runtime-config", command)

    def test_run_eval_engine_can_resume_selected_failed_run(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            chunks_path = Path(tmpdir) / "chunks.jsonl"
            eval_config_path = Path(tmpdir) / "eval_engine.toml"
            chunks_path.write_text("", encoding="utf-8")
            eval_config_path.write_text(
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
                        "timeout_sec = 30",
                    ]
                ),
                encoding="utf-8",
            )

            args = argparse.Namespace(
                scenario=None,
                chunk_profile="structural",
                chunks_path=chunks_path,
                eval_config=eval_config_path,
                run_type="experiment",
                postgres_url="postgres://postgres:postgres@localhost:5432/rag_eval",
                tracing_endpoint="http://localhost:4317",
                resume_run_id=None,
                resume_failed_run=True,
                dry_run=False,
            )

            with patch.object(
                run_stack,
                "load_dotenv",
                return_value={},
            ), patch.object(
                run_stack,
                "resolve_judge_runtime",
                return_value=("together", "https://api.together.xyz/v1", "openai/gpt-oss-20b"),
            ), patch.object(
                run_stack,
                "list_failed_eval_runs",
                return_value=[{"run_id": "failed-run-1", "label": "failed-run-1 | experiment | ts"}],
            ), patch.object(
                run_stack,
                "prompt_failed_eval_run",
                return_value={"run_id": "failed-run-1", "label": "failed-run-1 | experiment | ts"},
            ), patch.object(
                run_stack,
                "print_eval_engine_plan",
            ), patch.object(
                run_stack,
                "maybe_run",
                return_value=0,
            ) as maybe_run_mock:
                result = run_stack.run_eval_engine(args)

        self.assertEqual(result, 0)
        command = maybe_run_mock.call_args.args[0]
        self.assertIn("--resume-run-id", command)
        self.assertIn("failed-run-1", command)

    def test_load_dotenv_strips_wrapping_quotes(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            env_path = Path(tmpdir) / ".env"
            env_path.write_text(
                "\n".join(
                    [
                        'TOGETHER_API_KEY="quoted-secret"',
                        "OPENAI_COMPATIBLE_URL='https://api.together.xyz'",
                    ]
                ),
                encoding="utf-8",
            )

            values = run_stack.load_dotenv(env_path)

        self.assertEqual(values["TOGETHER_API_KEY"], "quoted-secret")
        self.assertEqual(values["OPENAI_COMPATIBLE_URL"], "https://api.together.xyz")

    def test_run_show_config_prints_runtime_snapshot(self) -> None:
        args = argparse.Namespace(
            postgres_url="postgres://postgres:postgres@localhost:5432/rag_eval",
            runtime_run_id="runtime-run-1",
            eval_run_id=None,
        )
        fake_row = {
            "runtime_run_id": "runtime-run-1",
            "created_at": "2026-04-08 12:00:00+00:00",
            "config_version": "v1",
            "runtime_config_json": {"generation": {"transport": {"kind": "openai"}}},
        }
        fake_cursor = MagicMock()
        fake_cursor.fetchone.return_value = fake_row
        fake_conn = MagicMock()
        fake_conn.execute.return_value = fake_cursor
        fake_conn.__enter__.return_value = fake_conn
        fake_conn.__exit__.return_value = None
        fake_psycopg = MagicMock()
        fake_psycopg.Connection.connect.return_value = fake_conn

        with patch.dict("sys.modules", {"psycopg": fake_psycopg, "psycopg.rows": MagicMock(dict_row=object())}):
            output = io.StringIO()
            with patch("sys.stdout", output):
                result = run_stack.run_show_config(args)

        self.assertEqual(result, 0)
        rendered = output.getvalue()
        self.assertIn('"kind": "runtime"', rendered)
        self.assertIn('"runtime_run_id": "runtime-run-1"', rendered)
        self.assertIn('"config_version": "v1"', rendered)


class RunStackInteractiveLaunchTests(unittest.TestCase):
    def test_interactive_launch_builds_effective_configs_and_commands(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            runtime_config_path = Path(tmpdir) / "rag_runtime.toml"
            eval_config_path = Path(tmpdir) / "eval_engine.toml"
            ingest_fixed = Path(tmpdir) / "fixed.toml"
            hybrid_fixed_bow = Path(tmpdir) / "fixed_bow.toml"
            hybrid_fixed_bm25 = Path(tmpdir) / "fixed_bm25.toml"
            chunks_fixed = Path(tmpdir) / "fixed_chunks.jsonl"
            golden_fixed = Path(tmpdir) / "fixed_golden_retrievals.json"
            datasets_root = Path(tmpdir) / "datasets"
            default_dataset_dir = datasets_root / "default"
            questions_default = default_dataset_dir / "questions.txt"
            metadata_default = default_dataset_dir / "metadata.json"

            runtime_config_path.write_text(
                "\n".join(
                    [
                        "[generation]",
                        'transport_kind = "openai"',
                        "",
                        "[generation.ollama]",
                        'model_name = "qwen2.5:1.5b-instruct-ctx32k"',
                        "",
                        "[generation.openai]",
                        'model_name = "openai/gpt-oss-20b"',
                        "",
                        "[reranking]",
                        'kind = "pass_through"',
                        "",
                        "[reranking.cross_encoder]",
                        'transport_kind = "mixedbread-ai"',
                        "",
                        "[reranking.cross_encoder.mixedbread-ai]",
                        'model_name = "mixedbread-ai/mxbai-rerank-base-v2"',
                        "batch_size = 3",
                        "timeout_sec = 120",
                        "cost_per_million_tokens = 0.0",
                        'tokenizer_source = "mixedbread-ai/mxbai-rerank-base-v2"',
                        "max_attempts = 3",
                        'backoff = "exponential"',
                        "",
                        "[reranking.cross_encoder.voyageai]",
                        'model_name = "rerank-2.5"',
                        "batch_size = 12",
                        "timeout_sec = 120",
                        "cost_per_million_tokens = 0.0",
                        "max_attempts = 3",
                        'backoff = "exponential"',
                    ]
                ),
                encoding="utf-8",
            )
            eval_config_path.write_text(
                "\n".join(
                    [
                        "[judge]",
                        'provider = "together"',
                        'model_name = "openai/gpt-oss-20b"',
                        'tokenizer_source = "Qwen/Qwen2.5-1.5B-Instruct"',
                        "input_cost_per_million_tokens = 0.05",
                        "output_cost_per_million_tokens = 0.20",
                        "",
                        "[judge.ollama]",
                        "timeout_sec = 120",
                        "",
                        "[judge.together]",
                        "timeout_sec = 120",
                    ]
                ),
                encoding="utf-8",
            )
            ingest_fixed.write_text(
                "\n".join(
                    [
                        "[qdrant.collection]",
                        'name = "chunks_dense_qwen3"',
                    ]
                ),
                encoding="utf-8",
            )
            hybrid_fixed_bow.write_text(
                "\n".join(
                    [
                        "[qdrant.collection]",
                        'name = "chunks_hybrid_fixed_qwen3_bow"',
                    ]
                ),
                encoding="utf-8",
            )
            hybrid_fixed_bm25.write_text(
                "\n".join(
                    [
                        "[qdrant.collection]",
                        'name = "chunks_hybrid_fixed_qwen3_bm25"',
                    ]
                ),
                encoding="utf-8",
            )
            chunks_fixed.write_text("", encoding="utf-8")
            golden_fixed.write_text("{}", encoding="utf-8")
            default_dataset_dir.mkdir(parents=True, exist_ok=True)
            questions_default.write_text("What is TCP?\n", encoding="utf-8")
            metadata_default.write_text(
                "\n".join(
                    [
                        "{",
                        '  "id": "default",',
                        '  "title": "Distributed Systems Basics",',
                        '  "question_count": 5',
                        "}",
                    ]
                ),
                encoding="utf-8",
            )

            args = argparse.Namespace(
                runtime_config=runtime_config_path,
                eval_config=eval_config_path,
                dry_run=False,
                verbose=False,
            )

            with patch.dict(
                run_stack.DENSE_INGEST_PROFILE_PATHS,
                {"fixed": ingest_fixed, "structural": ingest_fixed},
                clear=True,
            ), patch.dict(
                run_stack.HYBRID_INGEST_PROFILE_PATHS,
                {
                    ("fixed", "bag_of_words"): hybrid_fixed_bow,
                    ("fixed", "bm25_like"): hybrid_fixed_bm25,
                    ("structural", "bag_of_words"): hybrid_fixed_bow,
                    ("structural", "bm25_like"): hybrid_fixed_bm25,
                },
                clear=True,
            ), patch.dict(
                run_stack.EVAL_CHUNKS_PATHS,
                {"fixed": chunks_fixed, "structural": chunks_fixed},
                clear=True,
            ), patch.dict(
                run_stack.GOLDEN_RETRIEVALS_PATHS,
                {"fixed": golden_fixed, "structural": golden_fixed},
                clear=True,
            ), patch.object(
                run_stack,
                "QUERY_DATASETS_ROOT",
                datasets_root,
            ), patch.object(
                run_stack,
                "load_dotenv",
                return_value={
                    "POSTGRES_URL": "postgres://postgres:postgres@127.0.0.1:5432/rag_eval",
                    "TRACING_ENDPOINT": "http://localhost:4317",
                },
            ), patch.object(
                run_stack,
                "maybe_run",
                return_value=0,
            ) as maybe_run_mock, patch.object(
                run_stack,
                "list_failed_eval_runs",
                return_value=[{"run_id": "failed-run-1", "label": "failed-run-1 | experiment | ts"}],
            ), patch.object(
                run_stack,
                "prompt_failed_eval_run",
                return_value={"run_id": "failed-run-1", "label": "failed-run-1 | experiment | ts"},
            ), patch(
                "builtins.input",
                side_effect=["3", "2", "1", "3", "2", "3", "1", "2", "2", "1"],
            ):
                result = run_stack.run_interactive_launch(args)

        self.assertEqual(result, 0)
        self.assertEqual(maybe_run_mock.call_count, 2)

        runtime_command = maybe_run_mock.call_args_list[0].args[0]
        eval_command = maybe_run_mock.call_args_list[1].args[0]
        self.assertEqual(eval_command[0], str(run_stack.VENV_PYTHON))
        runtime_config_effective = Path(runtime_command[runtime_command.index("--config") + 1])
        eval_config_effective = Path(eval_command[eval_command.index("--eval-config") + 1])

        runtime_effective_text = runtime_config_effective.read_text(encoding="utf-8")
        eval_effective_text = eval_config_effective.read_text(encoding="utf-8")

        self.assertIn('kind = "dense"', runtime_effective_text)
        self.assertIn('kind = "cross_encoder"', runtime_effective_text)
        self.assertIn('transport_kind = "voyageai"', runtime_effective_text)
        self.assertIn('transport_kind = "openai"', runtime_effective_text)
        self.assertIn('model_name = "openai/gpt-oss-120b"', runtime_effective_text)
        self.assertIn('provider = "together"', eval_effective_text)
        self.assertIn('model_name = "openai/gpt-oss-20b"', eval_effective_text)
        self.assertIn(str(chunks_fixed.resolve()), eval_command)
        self.assertIn("--resume-run-id", eval_command)
        self.assertIn("failed-run-1", eval_command)
        self.assertIn(str(questions_default.resolve()), runtime_command)


if __name__ == "__main__":
    unittest.main()
