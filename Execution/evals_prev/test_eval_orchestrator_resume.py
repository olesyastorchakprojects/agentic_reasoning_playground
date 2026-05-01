from __future__ import annotations

import json
import tempfile
import unittest
from contextlib import contextmanager
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from Execution.evals import eval_orchestrator
from Execution.evals.judge_transport import JudgeSettings


class DummyConnection:
    def __init__(self) -> None:
        self.autocommit = False

    def __enter__(self) -> DummyConnection:
        return self

    def __exit__(self, exc_type, exc, tb) -> bool:
        return False


class DummySpan:
    def __init__(self) -> None:
        self.attributes: dict[str, object] = {}

    def set_attribute(self, key: str, value: object) -> None:
        self.attributes[key] = value


@contextmanager
def dummy_traced_operation(**_: object):
    yield DummySpan()


def test_judge_settings() -> JudgeSettings:
    return JudgeSettings(
        provider="ollama",
        model_name="qwen2.5:1.5b-instruct-q4_K_M",
        tokenizer_source="Qwen/Qwen2.5-1.5B-Instruct",
        base_url="http://localhost:11434/v1",
        api_key="unused",
        timeout_sec=30,
        input_cost_per_million_tokens=0.05,
        output_cost_per_million_tokens=0.20,
    )


class EvalOrchestratorResumeTests(unittest.TestCase):
    def test_load_resume_context_rejects_completed_run(self) -> None:
        params = eval_orchestrator.EvalOrchestratorParams(
            postgres_url="postgres://postgres:postgres@localhost:5432/rag_eval",
            run_type="experiment",
            chunks_path="Evidence/parsing/understanding_distributed_systems/chunks/chunks.jsonl",
            tracing_endpoint="http://localhost:4317",
            eval_config_path="Execution/evals/eval_engine.toml",
            resume_run_id="run-1",
        )

        with tempfile.TemporaryDirectory() as tmpdir:
            run_dir = Path(tmpdir) / "2026-03-27T20-20-11.888460+00-00_run-1"
            run_dir.mkdir(parents=True)
            (run_dir / "run_manifest.json").write_text(
                json.dumps(
                    {
                        "run_id": "run-1",
                        "run_type": "experiment",
                        "status": "completed",
                        "started_at": "2026-03-27T20:20:11.888460+00:00",
                        "stages": list(eval_orchestrator.STAGES),
                        "postgres_url": params.postgres_url,
                        "chunks_path": params.chunks_path,
                        "tracing_endpoint": params.tracing_endpoint,
                        "judge_provider": "ollama",
                        "judge_base_url": "http://localhost:11434/v1",
                        "judge_model": "qwen2.5:1.5b-instruct-q4_K_M",
                        "generation_suite_versions": {},
                        "retrieval_suite_versions": {},
                        "request_count": 1,
                        "retriever_kind": "Dense",
                        "reranker_kind": "Heuristic",
                        "run_scope_request_ids": ["req-1"],
                        "completed_at": "2026-03-27T20:25:10.050796+00:00",
                    }
                ),
                encoding="utf-8",
            )

            with patch.object(
                eval_orchestrator,
                "_find_artifact_dir_for_run",
                return_value=run_dir,
            ):
                with self.assertRaises(eval_orchestrator.ResumeRunError):
                    eval_orchestrator._load_resume_context(params, "run-1")

    def test_run_eval_orchestrator_resume_reuses_manifest_run_id_and_scope(self) -> None:
        resumed_run_id = "run-1"
        resumed_scope = ("req-1", "req-2")
        captured_params: dict[str, object] = {}

        params = eval_orchestrator.EvalOrchestratorParams(
            postgres_url="postgres://postgres:postgres@localhost:5432/rag_eval",
            run_type="experiment",
            chunks_path="Evidence/parsing/understanding_distributed_systems/chunks/chunks.jsonl",
            tracing_endpoint="http://localhost:4317",
            eval_config_path="Execution/evals/eval_engine.toml",
            resume_run_id=resumed_run_id,
        )

        def capture_generation_params(**kwargs: object) -> SimpleNamespace:
            captured_params["generation"] = kwargs
            return SimpleNamespace(**kwargs)

        def capture_retrieval_params(**kwargs: object) -> SimpleNamespace:
            captured_params["retrieval"] = kwargs
            return SimpleNamespace(**kwargs)

        def capture_summary_params(**kwargs: object) -> SimpleNamespace:
            captured_params["summary"] = kwargs
            return SimpleNamespace(**kwargs)

        with tempfile.TemporaryDirectory() as tmpdir:
            run_dir = Path(tmpdir) / f"2026-03-27T20-20-11.888460+00-00_{resumed_run_id}"
            run_dir.mkdir(parents=True)
            (run_dir / "run_manifest.json").write_text(
                json.dumps(
                    {
                        "run_id": resumed_run_id,
                        "run_type": "experiment",
                        "status": "failed",
                        "started_at": "2026-03-27T20:20:11.888460+00:00",
                        "stages": list(eval_orchestrator.STAGES),
                        "postgres_url": params.postgres_url,
                        "chunks_path": params.chunks_path,
                        "tracing_endpoint": params.tracing_endpoint,
                        "judge_provider": "ollama",
                        "judge_base_url": "http://localhost:11434/v1",
                        "judge_model": "qwen2.5:1.5b-instruct-q4_K_M",
                        "generation_suite_versions": {},
                        "retrieval_suite_versions": {},
                        "request_count": len(resumed_scope),
                        "retriever_kind": "Dense",
                        "reranker_kind": "Heuristic",
                        "run_scope_request_ids": list(resumed_scope),
                        "completed_at": "2026-03-27T20:25:10.050796+00:00",
                        "last_error": "prior failure",
                    }
                ),
                encoding="utf-8",
            )

            with patch.object(eval_orchestrator, "configure_eval_logging"), patch.object(
                eval_orchestrator, "init_eval_tracing"
            ), patch.object(
                eval_orchestrator, "shutdown_eval_tracing"
            ), patch.object(
                eval_orchestrator, "traced_operation", dummy_traced_operation
            ), patch.object(
                eval_orchestrator, "_find_artifact_dir_for_run", return_value=run_dir
            ), patch.object(
                eval_orchestrator, "_write_manifest"
            ), patch.object(
                eval_orchestrator, "_build_run_report", return_value="# report\n"
            ), patch.object(
                eval_orchestrator, "_assert_run_complete"
            ), patch.object(
                eval_orchestrator, "_drain_stage"
            ), patch.object(
                eval_orchestrator, "_bootstrap_processing_state", side_effect=AssertionError("bootstrap should not run during resume")
            ), patch.object(
                eval_orchestrator, "load_generation_suite_prompts", return_value={}
            ), patch.object(
                eval_orchestrator, "load_retrieval_suite_prompts", return_value={}
            ), patch.object(
                eval_orchestrator, "load_judge_settings", return_value=test_judge_settings()
            ), patch.object(
                eval_orchestrator.Connection, "connect", return_value=DummyConnection()
            ), patch.object(
                eval_orchestrator, "JudgeGenerationParams", side_effect=capture_generation_params
            ), patch.object(
                eval_orchestrator, "JudgeRetrievalParams", side_effect=capture_retrieval_params
            ), patch.object(
                eval_orchestrator, "BuildRequestSummaryParams", side_effect=capture_summary_params
            ):
                result_run_id = eval_orchestrator.run_eval_orchestrator(params)

        self.assertEqual(result_run_id, resumed_run_id)
        self.assertEqual(captured_params["generation"]["run_id"], resumed_run_id)
        self.assertEqual(captured_params["retrieval"]["run_id"], resumed_run_id)
        self.assertEqual(captured_params["summary"]["run_id"], resumed_run_id)
        self.assertEqual(captured_params["generation"]["run_scope_request_ids"], resumed_scope)
        self.assertEqual(captured_params["retrieval"]["run_scope_request_ids"], resumed_scope)
        self.assertEqual(captured_params["summary"]["run_scope_request_ids"], resumed_scope)

    def test_resume_allows_chunks_path_override(self) -> None:
        resumed_run_id = "run-1"
        manifest_chunks_path = "Evidence/parsing/understanding_distributed_systems/chunks/chunks.jsonl"
        override_chunks_path = "Evidence/parsing/understanding_distributed_systems/chunks/fixed_chunks.jsonl"

        params = eval_orchestrator.EvalOrchestratorParams(
            postgres_url="postgres://postgres:postgres@localhost:5432/rag_eval",
            run_type="experiment",
            chunks_path=override_chunks_path,
            tracing_endpoint="http://localhost:4317",
            eval_config_path="Execution/evals/eval_engine.toml",
            resume_run_id=resumed_run_id,
        )

        with tempfile.TemporaryDirectory() as tmpdir:
            run_dir = Path(tmpdir) / f"2026-03-27T20-20-11.888460+00-00_{resumed_run_id}"
            run_dir.mkdir(parents=True)
            (run_dir / "run_manifest.json").write_text(
                json.dumps(
                    {
                        "run_id": resumed_run_id,
                        "run_type": "experiment",
                        "status": "failed",
                        "started_at": "2026-03-27T20:20:11.888460+00:00",
                        "stages": list(eval_orchestrator.STAGES),
                        "postgres_url": params.postgres_url,
                        "chunks_path": manifest_chunks_path,
                        "tracing_endpoint": params.tracing_endpoint,
                        "judge_provider": "ollama",
                        "judge_base_url": "http://localhost:11434/v1",
                        "judge_model": "qwen2.5:1.5b-instruct-q4_K_M",
                        "generation_suite_versions": {},
                        "retrieval_suite_versions": {},
                        "request_count": 1,
                        "retriever_kind": "Dense",
                        "reranker_kind": "Heuristic",
                        "run_scope_request_ids": ["req-1"],
                        "completed_at": "2026-03-27T20:25:10.050796+00:00",
                        "last_error": "prior failure",
                    }
                ),
                encoding="utf-8",
            )

            with patch.object(
                eval_orchestrator,
                "_find_artifact_dir_for_run",
                return_value=run_dir,
            ):
                (
                    run_id,
                    _started_at,
                    scope,
                    manifest,
                    _artifact_dir,
                    _manifest_path,
                    _report_path,
                ) = eval_orchestrator._load_resume_context(params, resumed_run_id)

        self.assertEqual(run_id, resumed_run_id)
        self.assertEqual(scope, ("req-1",))
        self.assertEqual(manifest["chunks_path"], manifest_chunks_path)

    def test_new_run_without_new_requests_surfaces_resume_candidates(self) -> None:
        params = eval_orchestrator.EvalOrchestratorParams(
            postgres_url="postgres://postgres:postgres@localhost:5432/rag_eval",
            run_type="experiment",
            chunks_path="Evidence/parsing/understanding_distributed_systems/chunks/chunks.jsonl",
            tracing_endpoint="http://localhost:4317",
            eval_config_path="Execution/evals/eval_engine.toml",
            resume_run_id=None,
        )

        with patch.object(eval_orchestrator, "configure_eval_logging"), patch.object(
            eval_orchestrator, "init_eval_tracing"
        ), patch.object(
            eval_orchestrator, "shutdown_eval_tracing"
        ), patch.object(
            eval_orchestrator, "traced_operation", dummy_traced_operation
        ), patch.object(
            eval_orchestrator, "_write_manifest"
        ), patch.object(
            eval_orchestrator, "_bootstrap_processing_state", return_value=()
        ), patch.object(
            eval_orchestrator, "_find_resume_candidates", return_value=["run-old"]
        ), patch.object(
            eval_orchestrator, "load_judge_settings", return_value=test_judge_settings()
        ), patch.object(
            eval_orchestrator.Connection, "connect", return_value=DummyConnection()
        ):
            with self.assertRaises(eval_orchestrator.ResumeRunError) as ctx:
                eval_orchestrator.run_eval_orchestrator(params)

        self.assertIn("run-old", str(ctx.exception))
        self.assertIn("--resume-run-id", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
