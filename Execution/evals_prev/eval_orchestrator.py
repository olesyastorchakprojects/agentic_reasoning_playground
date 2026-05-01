from __future__ import annotations

import argparse
import json
import math
import uuid
from dataclasses import dataclass
from datetime import datetime, timezone
from decimal import Decimal
from pathlib import Path
from typing import Any

import jsonschema
from psycopg import Connection
from psycopg.rows import dict_row

from .build_request_summary import BuildRequestSummaryParams, run_build_request_summary
from .common import (
    load_generation_suite_prompts,
    load_retrieval_suite_prompts,
    project_root,
)
from .judge_generation import JudgeGenerationParams, run_judge_generation
from .judge_retrieval import JudgeRetrievalParams, run_judge_retrieval
from .judge_transport import JudgeSettings, load_judge_settings
from .logging_utils import configure_eval_logging, get_logger, log_kv
from .observability import init_eval_tracing, shutdown_eval_tracing, traced_operation


STAGES = (
    "judge_generation",
    "judge_retrieval",
    "build_request_summary",
)
LOGGER = get_logger("evals.eval_orchestrator")

_RETRIEVAL_QUALITY_METRIC_COLS = (
    "retrieval_recall_soft",
    "retrieval_recall_strict",
    "retrieval_rr_soft",
    "retrieval_rr_strict",
    "retrieval_ndcg",
    "reranking_recall_soft",
    "reranking_recall_strict",
    "reranking_rr_soft",
    "reranking_rr_strict",
    "reranking_ndcg",
    "retrieval_context_loss_soft",
    "retrieval_context_loss_strict",
)

_CONDITIONAL_AGGREGATE_KEYS = (
    "groundedness_retrieval_soft",
    "answer_completeness_retrieval_soft",
    "answer_relevance_retrieval_soft",
    "hallucination_retrieval_soft",
    "success_retrieval_soft",
    "groundedness_retrieval_strict",
    "answer_completeness_retrieval_strict",
    "answer_relevance_retrieval_strict",
    "hallucination_retrieval_strict",
    "success_retrieval_strict",
    "groundedness_reranking_soft",
    "answer_completeness_reranking_soft",
    "answer_relevance_reranking_soft",
    "hallucination_reranking_soft",
    "success_reranking_soft",
    "groundedness_reranking_strict",
    "answer_completeness_reranking_strict",
    "answer_relevance_reranking_strict",
    "hallucination_reranking_strict",
    "success_reranking_strict",
)

_CONDITIONAL_ROW_METRICS = (
    ("groundedness_given_relevant_context", "groundedness"),
    ("answer_completeness_given_relevant_context", "answer_completeness"),
    ("answer_relevance_given_relevant_context", "answer_relevance"),
    ("hallucination_rate_when_top1_irrelevant", "hallucination"),
    ("success_rate_when_at_least_one_relevant_in_topk", "success"),
)


@dataclass(frozen=True)
class EvalOrchestratorParams:
    postgres_url: str
    run_type: str
    chunks_path: str
    tracing_endpoint: str
    eval_config_path: str
    resume_run_id: str | None = None


class EvalOrchestratorError(RuntimeError):
    """Base orchestrator error."""


class RunBootstrapError(EvalOrchestratorError):
    pass


class ManifestError(EvalOrchestratorError):
    pass


class ProcessingStateBootstrapError(EvalOrchestratorError):
    pass


class WorkerInvocationError(EvalOrchestratorError):
    pass


class StagePromotionError(EvalOrchestratorError):
    pass


class CompletionDetectionError(EvalOrchestratorError):
    pass


class RunReportError(EvalOrchestratorError):
    pass


class ResumeRunError(EvalOrchestratorError):
    pass


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run the eval pipeline")
    parser.add_argument("--postgres-url", required=True)
    parser.add_argument("--run-type", required=True, choices=("continuous", "nightly", "experiment"))
    parser.add_argument("--chunks-path", required=True)
    parser.add_argument("--tracing-endpoint", required=True)
    parser.add_argument("--eval-config", required=True)
    parser.add_argument("--resume-run-id")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    params = EvalOrchestratorParams(
        postgres_url=args.postgres_url,
        run_type=args.run_type,
        chunks_path=args.chunks_path,
        tracing_endpoint=args.tracing_endpoint,
        eval_config_path=args.eval_config,
        resume_run_id=args.resume_run_id,
    )
    run_eval_orchestrator(params)
    return 0


def run_eval_orchestrator(params: EvalOrchestratorParams) -> str:
    configure_eval_logging()
    init_eval_tracing(service_name="eval_engine", endpoint=params.tracing_endpoint)
    judge_settings = load_judge_settings(Path(params.eval_config_path))
    retriever_kind: str | None = None
    reranker_kind: str | None = None
    if params.resume_run_id is None:
        run_id = str(uuid.uuid4())
        started_at = _now()
        run_scope_request_ids: tuple[str, ...] = ()
        manifest = _build_manifest(
            run_id=run_id,
            params=params,
            started_at=started_at,
            request_count=0,
            run_scope_request_ids=run_scope_request_ids,
            judge_settings=judge_settings,
            retriever_kind=retriever_kind,
            reranker_kind=reranker_kind,
            status="running",
            completed_at=None,
            last_error=None,
        )
        artifact_dir = _artifact_dir(run_id, started_at)
        manifest_path = artifact_dir / "run_manifest.json"
        report_path = artifact_dir / "run_report.md"
        is_resume = False
    else:
        (
            run_id,
            started_at,
            run_scope_request_ids,
            manifest,
            artifact_dir,
            manifest_path,
            report_path,
        ) = _load_resume_context(params, params.resume_run_id)
        is_resume = True
        reranker_kind = (
            manifest.get("reranker_kind")
            if isinstance(manifest.get("reranker_kind"), str)
            else None
        )
        retriever_kind = (
            manifest.get("retriever_kind")
            if isinstance(manifest.get("retriever_kind"), str)
            else None
        )

    try:
        with traced_operation(
            tracer_name="Execution.evals.eval_orchestrator",
            span_name="eval.run",
            attributes={
                "run_id": run_id,
                "run_type": params.run_type,
                "request_count": 0,
                "status": "running",
            },
        ) as span:
            log_kv(
                LOGGER,
                "run_started",
                run_id=run_id,
                run_type=params.run_type,
                chunks_path=params.chunks_path,
                resume_run_id=params.resume_run_id,
            )
            artifact_dir.mkdir(parents=True, exist_ok=True)
            _write_manifest(manifest_path, manifest)

            with Connection.connect(params.postgres_url, row_factory=dict_row) as connection:
                connection.autocommit = True
                if not is_resume:
                    try:
                        _store_eval_run_config(connection, run_id, judge_settings)
                    except Exception as error:
                        log_kv(
                            LOGGER,
                            "eval_run_config_persistence_failed",
                            run_id=run_id,
                            error=str(error),
                        )
                if not is_resume:
                    run_scope_request_ids = _bootstrap_processing_state(connection)
                    if not run_scope_request_ids:
                        resume_candidates = _find_resume_candidates(connection)
                        if resume_candidates:
                            raise ResumeRunError(
                                "no new requests were bootstrapped; resumable run(s) still exist: "
                                + ", ".join(resume_candidates[:5])
                                + ". Re-run with --resume-run-id."
                            )
                    manifest["request_count"] = len(run_scope_request_ids)
                    manifest["run_scope_request_ids"] = list(run_scope_request_ids)
                if run_scope_request_ids and reranker_kind is None:
                    reranker_kind = _load_run_reranker_kind(connection, run_scope_request_ids)
                if run_scope_request_ids and retriever_kind is None:
                    retriever_kind = _load_run_retriever_kind(connection, run_scope_request_ids)
                if reranker_kind is not None:
                    manifest["reranker_kind"] = reranker_kind
                if retriever_kind is not None:
                    manifest["retriever_kind"] = retriever_kind
                span.set_attribute("request_count", len(run_scope_request_ids))
                log_kv(
                    LOGGER,
                    "run_bootstrap_completed",
                    run_id=run_id,
                    request_count=len(run_scope_request_ids),
                    resume_mode=is_resume,
                )
                _write_manifest(manifest_path, manifest)

                generation_params = JudgeGenerationParams(
                    postgres_url=params.postgres_url,
                    run_id=run_id,
                    judge_settings=judge_settings,
                    chunks_path=params.chunks_path,
                    run_scope_request_ids=run_scope_request_ids,
                )
                retrieval_params = JudgeRetrievalParams(
                    postgres_url=params.postgres_url,
                    run_id=run_id,
                    judge_settings=judge_settings,
                    chunks_path=params.chunks_path,
                    run_scope_request_ids=run_scope_request_ids,
                )
                summary_params = BuildRequestSummaryParams(
                    postgres_url=params.postgres_url,
                    run_id=run_id,
                    run_scope_request_ids=run_scope_request_ids,
                )

                _drain_stage(
                    connection=connection,
                    stage_name="judge_generation",
                    worker=lambda: run_judge_generation(generation_params),
                    next_stage="judge_retrieval",
                    run_scope_request_ids=run_scope_request_ids,
                )
                _drain_stage(
                    connection=connection,
                    stage_name="judge_retrieval",
                    worker=lambda: run_judge_retrieval(retrieval_params),
                    next_stage="build_request_summary",
                    run_scope_request_ids=run_scope_request_ids,
                )
                _drain_stage(
                    connection=connection,
                    stage_name="build_request_summary",
                    worker=lambda: run_build_request_summary(summary_params),
                    next_stage=None,
                    run_scope_request_ids=run_scope_request_ids,
                )

                _assert_run_complete(connection, run_scope_request_ids)
                completed_at = _now()
                manifest = _build_manifest(
                    run_id=run_id,
                    params=params,
                    started_at=started_at,
                    request_count=len(run_scope_request_ids),
                    run_scope_request_ids=run_scope_request_ids,
                    judge_settings=judge_settings,
                    retriever_kind=retriever_kind,
                    reranker_kind=reranker_kind,
                    status="completed",
                    completed_at=completed_at,
                    last_error=None,
                )
                _write_manifest(manifest_path, manifest)
                report_text = _build_run_report(
                    connection=connection,
                    run_id=run_id,
                    manifest=manifest,
                    judge_settings=judge_settings,
                )
                report_path.write_text(report_text, encoding="utf-8")
                span.set_attribute("status", "completed")
                log_kv(
                    LOGGER,
                    "run_completed",
                    run_id=run_id,
                    request_count=len(run_scope_request_ids),
                    manifest_path=str(manifest_path),
                    report_path=str(report_path),
                )
                return run_id
    except Exception as error:
        failure_manifest = _build_manifest(
            run_id=run_id,
            params=params,
            started_at=started_at,
            request_count=len(run_scope_request_ids),
            run_scope_request_ids=run_scope_request_ids,
            judge_settings=judge_settings,
            retriever_kind=retriever_kind,
            reranker_kind=reranker_kind,
            status="failed",
            completed_at=_now(),
            last_error=str(error),
        )
        try:
            artifact_dir.mkdir(parents=True, exist_ok=True)
            _write_manifest(manifest_path, failure_manifest)
        except Exception as manifest_error:  # pragma: no cover
            raise ManifestError(
                f"failed to write terminal failure manifest: {manifest_error}"
            ) from manifest_error
        log_kv(
            LOGGER,
            "run_failed",
            run_id=run_id,
            error=str(error),
            manifest_path=str(manifest_path),
        )
        raise
    finally:
        shutdown_eval_tracing()


def _bootstrap_processing_state(connection: Connection[Any]) -> tuple[str, ...]:
    try:
        new_rows = connection.execute(
            """
            select rc.request_id, rc.received_at
            from request_captures rc
            left join eval_processing_state eps on eps.request_id = rc.request_id
            where eps.request_id is null
            order by rc.received_at asc, rc.request_id asc
            """
        ).fetchall()
    except Exception as error:  # pragma: no cover
        raise RunBootstrapError(str(error)) from error

    inserted_request_ids: list[str] = []
    try:
        with connection.transaction():
            for row in new_rows:
                inserted = connection.execute(
                    """
                    insert into eval_processing_state (
                        request_id,
                        request_received_at,
                        current_stage,
                        status,
                        attempt_count
                    ) values (%s, %s, 'judge_generation', 'pending', 0)
                    on conflict (request_id) do nothing
                    returning request_id
                    """,
                    (row["request_id"], row["received_at"]),
                ).fetchone()
                if inserted is not None:
                    inserted_request_ids.append(inserted["request_id"])
    except Exception as error:  # pragma: no cover
        raise ProcessingStateBootstrapError(str(error)) from error

    return tuple(inserted_request_ids)


def _find_resume_candidates(connection: Connection[Any]) -> list[str]:
    try:
        incomplete_rows = connection.execute(
            """
            select request_id
            from eval_processing_state
            where status in ('pending', 'running', 'failed')
            """
        ).fetchall()
    except Exception as error:  # pragma: no cover
        raise RunBootstrapError(str(error)) from error

    incomplete_request_ids = {
        str(row["request_id"])
        for row in incomplete_rows
        if row.get("request_id")
    }
    if not incomplete_request_ids:
        return []

    candidates: list[tuple[datetime, str]] = []
    runs_root = project_root() / "Evidence" / "evals" / "runs"
    for run_dir in sorted(runs_root.glob("*")):
        if not run_dir.is_dir():
            continue
        manifest_path = run_dir / "run_manifest.json"
        if not manifest_path.is_file():
            continue
        try:
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        except Exception:
            continue
        if manifest.get("status") not in {"failed", "running"}:
            continue
        manifest_run_id = manifest.get("run_id")
        if not isinstance(manifest_run_id, str) or not manifest_run_id:
            continue
        scope = manifest.get("run_scope_request_ids")
        if not isinstance(scope, list):
            continue
        if not any(isinstance(request_id, str) and request_id in incomplete_request_ids for request_id in scope):
            continue
        started_at = _parse_started_at(str(manifest.get("started_at", ""))) or datetime.min.replace(
            tzinfo=timezone.utc
        )
        candidates.append((started_at, manifest_run_id))

    candidates.sort(reverse=True)
    seen: set[str] = set()
    ordered_run_ids: list[str] = []
    for _started_at, run_id in candidates:
        if run_id in seen:
            continue
        seen.add(run_id)
        ordered_run_ids.append(run_id)
    return ordered_run_ids


def _load_resume_context(
    params: EvalOrchestratorParams,
    resume_run_id: str,
) -> tuple[str, str, tuple[str, ...], dict[str, Any], Path, Path, Path]:
    artifact_dir = _find_artifact_dir_for_run(resume_run_id)
    manifest_path = artifact_dir / "run_manifest.json"
    report_path = artifact_dir / "run_report.md"
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except Exception as error:
        raise ResumeRunError(f"failed to read run manifest for resume_run_id={resume_run_id}: {error}") from error

    if manifest.get("run_id") != resume_run_id:
        raise ResumeRunError("resume run manifest run_id does not match requested run_id")

    manifest_status = manifest.get("status")
    if manifest_status not in {"failed", "running"}:
        raise ResumeRunError(
            f"resume run manifest must be in failed or running status, got {manifest_status!r}"
        )

    run_scope_request_ids = tuple(manifest.get("run_scope_request_ids", []))
    if not run_scope_request_ids:
        raise ResumeRunError(
            "resume run manifest is missing non-empty run_scope_request_ids"
        )

    _validate_resume_params(params, manifest)
    # Resume must preserve the original run identity and frozen request scope.
    manifest["status"] = "running"
    manifest.pop("completed_at", None)
    manifest.pop("last_error", None)
    return (
        resume_run_id,
        str(manifest["started_at"]),
        run_scope_request_ids,
        manifest,
        artifact_dir,
        manifest_path,
        report_path,
    )


def _validate_resume_params(params: EvalOrchestratorParams, manifest: dict[str, Any]) -> None:
    expected_pairs = {
        "postgres_url": params.postgres_url,
        "run_type": params.run_type,
        "tracing_endpoint": params.tracing_endpoint,
    }
    mismatches = [
        key
        for key, value in expected_pairs.items()
        if manifest.get(key) != value
    ]
    if mismatches:
        raise ResumeRunError(
            f"resume params do not match manifest fields: {', '.join(mismatches)}"
        )


def _find_artifact_dir_for_run(run_id: str) -> Path:
    runs_root = project_root() / "Evidence" / "evals" / "runs"
    matches = sorted(runs_root.glob(f"*_{run_id}"))
    if not matches:
        raise ResumeRunError(f"artifact directory not found for run_id={run_id}")
    if len(matches) > 1:
        raise ResumeRunError(f"multiple artifact directories found for run_id={run_id}")
    return matches[0]


def _drain_stage(
    connection: Connection[Any],
    stage_name: str,
    worker: Any,
    next_stage: str | None,
    run_scope_request_ids: tuple[str, ...],
) -> None:
    log_kv(LOGGER, "stage_drain_started", run_id=_worker_run_id(worker), stage=stage_name)
    while True:
        remaining_before = _count_stage_remaining(connection, stage_name, run_scope_request_ids)
        if remaining_before == 0:
            log_kv(LOGGER, "stage_drained", run_id=_worker_run_id(worker), stage=stage_name)
            return

        try:
            did_work = worker()
        except Exception as error:
            raise WorkerInvocationError(f"{stage_name} worker failed: {error}") from error

        if next_stage is not None:
            _promote_completed_rows(connection, stage_name, next_stage, run_scope_request_ids)

        remaining_after = _count_stage_remaining(connection, stage_name, run_scope_request_ids)
        if remaining_after == 0:
            log_kv(LOGGER, "stage_drained", run_id=_worker_run_id(worker), stage=stage_name)
            return

        if not did_work:
            raise CompletionDetectionError(
                f"{stage_name} reported no work while {remaining_after} rows still remain"
            )


def _count_stage_remaining(
    connection: Connection[Any],
    stage_name: str,
    run_scope_request_ids: tuple[str, ...],
) -> int:
    if not run_scope_request_ids:
        return 0
    try:
        row = connection.execute(
            """
            select count(*) as row_count
            from eval_processing_state
            where current_stage = %s
              and request_id = any(%s)
              and status in ('pending', 'running', 'failed')
            """,
            (stage_name, list(run_scope_request_ids)),
        ).fetchone()
    except Exception as error:  # pragma: no cover
        raise CompletionDetectionError(str(error)) from error
    return int(row["row_count"])


def _promote_completed_rows(
    connection: Connection[Any],
    stage_name: str,
    next_stage: str,
    run_scope_request_ids: tuple[str, ...],
) -> None:
    if not run_scope_request_ids:
        return
    try:
        with connection.transaction():
            cursor = connection.execute(
                """
                update eval_processing_state
                set
                    current_stage = %s,
                    status = 'pending',
                    updated_at = now()
                where current_stage = %s
                  and request_id = any(%s)
                  and status = 'completed'
                """,
                (next_stage, stage_name, list(run_scope_request_ids)),
            )
            promoted_count = cursor.rowcount
    except Exception as error:  # pragma: no cover
        raise StagePromotionError(str(error)) from error
    log_kv(
        LOGGER,
        "stage_promotion_completed",
        stage=stage_name,
        next_stage=next_stage,
        promoted_count=promoted_count,
    )


def _assert_run_complete(
    connection: Connection[Any],
    run_scope_request_ids: tuple[str, ...],
) -> None:
    if not run_scope_request_ids:
        return
    try:
        rows = connection.execute(
            """
            select request_id, current_stage::text as current_stage, status::text as status
            from eval_processing_state
            where request_id = any(%s)
            order by request_id asc
            """,
            (list(run_scope_request_ids),),
        ).fetchall()
    except Exception as error:  # pragma: no cover
        raise CompletionDetectionError(str(error)) from error

    incomplete = [
        row["request_id"]
        for row in rows
        if row["current_stage"] != "build_request_summary" or row["status"] != "completed"
    ]
    if incomplete:
        raise CompletionDetectionError(
            f"run completed check failed for request_ids={','.join(incomplete)}"
        )


def _build_manifest(
    run_id: str,
    params: EvalOrchestratorParams,
    started_at: str,
    request_count: int,
    run_scope_request_ids: tuple[str, ...],
    judge_settings: JudgeSettings,
    retriever_kind: str | None,
    reranker_kind: str | None,
    status: str,
    completed_at: str | None,
    last_error: str | None,
) -> dict[str, Any]:
    generation_versions = {
        suite_name: prompt.version
        for suite_name, prompt in load_generation_suite_prompts().items()
    }
    retrieval_versions = {
        suite_name: prompt.version
        for suite_name, prompt in load_retrieval_suite_prompts().items()
    }

    manifest: dict[str, Any] = {
        "run_id": run_id,
        "run_type": params.run_type,
        "status": status,
        "started_at": started_at,
        "stages": list(STAGES),
        "postgres_url": params.postgres_url,
        "chunks_path": params.chunks_path,
        "tracing_endpoint": params.tracing_endpoint,
        "judge_provider": judge_settings.provider,
        "judge_base_url": judge_settings.base_url,
        "judge_model": judge_settings.model_name,
        "generation_suite_versions": generation_versions,
        "retrieval_suite_versions": retrieval_versions,
        "request_count": request_count,
        "run_scope_request_ids": list(run_scope_request_ids),
    }
    if retriever_kind is not None:
        manifest["retriever_kind"] = retriever_kind
    if reranker_kind is not None:
        manifest["reranker_kind"] = reranker_kind
    if completed_at is not None:
        manifest["completed_at"] = completed_at
    if last_error:
        manifest["last_error"] = last_error
    return manifest


def _store_eval_run_config(
    connection: Connection[Any],
    run_id: str,
    judge_settings: JudgeSettings,
) -> None:
    config_json = {
        "judge": {
            "provider": judge_settings.provider,
            "model_name": judge_settings.model_name,
            "tokenizer_source": judge_settings.tokenizer_source,
            "base_url": judge_settings.base_url,
            "timeout_sec": judge_settings.timeout_sec,
            "input_cost_per_million_tokens": judge_settings.input_cost_per_million_tokens,
            "output_cost_per_million_tokens": judge_settings.output_cost_per_million_tokens,
        }
    }
    connection.execute(
        """
        insert into eval_run_configs (
            eval_run_id,
            config_version,
            eval_config_json
        ) values (%s, %s, %s::jsonb)
        on conflict (eval_run_id) do update
        set config_version = excluded.config_version,
            eval_config_json = excluded.eval_config_json
        """,
        (run_id, "v1", json.dumps(config_json)),
    )


def _write_manifest(manifest_path: Path, manifest: dict[str, Any]) -> None:
    schema_path = project_root() / "Execution" / "schemas" / "evals" / "run_manifest.schema.json"
    schema = json.loads(schema_path.read_text(encoding="utf-8"))
    try:
        jsonschema.validate(manifest, schema)
    except jsonschema.ValidationError as error:  # pragma: no cover
        raise ManifestError(str(error)) from error
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


def _artifact_dir(run_id: str, started_at: str) -> Path:
    folder_timestamp = started_at.replace(":", "-")
    return project_root() / "Evidence" / "evals" / "runs" / f"{folder_timestamp}_{run_id}"


def _load_run_pipeline_configs(
    connection: Connection[Any],
    run_scope_request_ids: tuple[str, ...],
) -> dict[str, Any]:
    try:
        row = connection.execute(
            """
            select runtime_run_id, retriever_kind, retriever_config, reranker_kind, reranker_config, generation_config, top_k_requested
            from request_captures
            where request_id = any(%s)
            limit 1
            """,
            (list(run_scope_request_ids),),
        ).fetchone()
    except Exception as error:
        raise RunReportError(str(error)) from error
    if row is None:
        raise RunReportError("no request captures found for run scope")
    return dict(row)


def _build_retriever_section(
    retriever_kind: str,
    retriever_config: dict[str, Any],
    top_k: int | None,
    retrieval_depth_row: dict[str, Any] | None,
) -> list[str]:
    lines = ["### Retriever", f"- kind: `{_format_retriever_label(retriever_kind, retriever_config)}`"]
    for key, label in (
        ("embedding_model_name", "embedding_model"),
        ("qdrant_collection_name", "collection"),
        ("corpus_version", "corpus_version"),
        ("chunking_strategy", "chunking_strategy"),
    ):
        if key in retriever_config:
            lines.append(f"- {label}: `{retriever_config[key]}`")
    if top_k is not None:
        lines.append(f"- top_k: `{top_k}`")
    if retrieval_depth_row is not None and retrieval_depth_row.get("avg_retrieval_chunk_count") is not None:
        avg_returned = float(retrieval_depth_row["avg_retrieval_chunk_count"])
        min_returned = int(retrieval_depth_row["min_retrieval_chunk_count"])
        max_returned = int(retrieval_depth_row["max_retrieval_chunk_count"])
        lines.append(
            "- actual_chunks_returned: "
            f"`mean={avg_returned:.2f}, min={min_returned}, max={max_returned}`"
        )
    return lines


def _format_retriever_label(retriever_kind: str, retriever_config: dict[str, Any]) -> str:
    if retriever_kind != "Hybrid":
        return retriever_kind

    strategy = retriever_config.get("strategy")
    if not isinstance(strategy, dict):
        return "Hybrid"

    strategy_kind = strategy.get("kind")
    if strategy_kind == "bag_of_words":
        return "Hybrid - bow"
    if strategy_kind == "bm25_like":
        return "Hybrid - bm25"
    return "Hybrid"


def _build_reranker_section(reranker_kind: str, reranker_config: dict[str, Any] | None) -> list[str]:
    lines = ["### Reranker", f"- kind: `{reranker_kind}`"]
    if reranker_kind == "CrossEncoder" and reranker_config:
        cross_encoder = reranker_config.get("cross_encoder") or {}
        if isinstance(cross_encoder, dict):
            lines.append(f"- model: `{cross_encoder.get('model_name', '')}`")
            lines.append(f"- url: `{cross_encoder.get('url', '')}`")
    elif reranker_kind == "Heuristic" and reranker_config:
        weights = reranker_config.get("weights") or {}
        for name, value in weights.items():
            lines.append(f"- weight.{name}: `{value}`")
    return lines


def _build_generation_section(generation_config: dict[str, Any]) -> list[str]:
    lines = ["### Generation"]
    for key in ("model", "model_endpoint", "temperature", "max_context_chunks"):
        if key in generation_config:
            lines.append(f"- {key}: `{generation_config[key]}`")
    return lines


def _build_judge_section(judge_settings: JudgeSettings) -> list[str]:
    return [
        "### Judge",
        f"- provider: `{judge_settings.provider}`",
        f"- model: `{judge_settings.model_name}`",
        f"- endpoint: `{judge_settings.base_url}`",
    ]


def _format_int(value: Any) -> str:
    if value is None:
        return "0"
    return f"{int(value):,}"


def _format_cost(value: Any) -> str:
    if value is None:
        return "0.00000000"
    return f"{Decimal(str(value)):.8f}"


def _load_runtime_token_usage(
    connection: Connection[Any],
    run_scope_request_ids: tuple[str, ...],
) -> dict[str, Any]:
    row = connection.execute(
        """
        select
            count(*) as requests,
            coalesce(sum(prompt_tokens), 0) as prompt_tokens,
            coalesce(sum(completion_tokens), 0) as completion_tokens,
            coalesce(sum(total_tokens), 0) as total_tokens,
            coalesce(sum(
                prompt_tokens * ((generation_config->>'input_cost_per_million_tokens')::numeric) / 1000000
            ), 0) as prompt_cost_usd,
            coalesce(sum(
                completion_tokens * ((generation_config->>'output_cost_per_million_tokens')::numeric) / 1000000
            ), 0) as completion_cost_usd,
            coalesce(sum(
                (prompt_tokens * ((generation_config->>'input_cost_per_million_tokens')::numeric) / 1000000)
                + (completion_tokens * ((generation_config->>'output_cost_per_million_tokens')::numeric) / 1000000)
            ), 0) as total_cost_usd
        from request_captures
        where request_id = any(%s)
        """,
        (list(run_scope_request_ids),),
    ).fetchone()
    return dict(row or {})


def _load_judge_token_usage(connection: Connection[Any], run_id: str) -> list[dict[str, Any]]:
    rows = connection.execute(
        """
        with grouped as (
            select
                stage_name,
                count(*) as eval_calls,
                coalesce(sum(prompt_tokens), 0) as prompt_tokens,
                coalesce(sum(completion_tokens), 0) as completion_tokens,
                coalesce(sum(total_tokens), 0) as total_tokens,
                coalesce(sum(prompt_tokens * input_cost_per_million_tokens / 1000000), 0) as prompt_cost_usd,
                coalesce(sum(completion_tokens * output_cost_per_million_tokens / 1000000), 0) as completion_cost_usd,
                coalesce(sum(total_cost_usd), 0) as total_cost_usd
            from judge_llm_calls
            where run_id = %s
            group by stage_name
        ),
        unioned as (
            select
                stage_name,
                eval_calls,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                prompt_cost_usd,
                completion_cost_usd,
                total_cost_usd,
                case stage_name
                    when 'judge_generation' then 1
                    when 'judge_retrieval' then 2
                    else 4
                end as sort_key
            from grouped
            union all
            select
                'judge_total' as stage_name,
                coalesce(sum(eval_calls), 0) as eval_calls,
                coalesce(sum(prompt_tokens), 0) as prompt_tokens,
                coalesce(sum(completion_tokens), 0) as completion_tokens,
                coalesce(sum(total_tokens), 0) as total_tokens,
                coalesce(sum(prompt_cost_usd), 0) as prompt_cost_usd,
                coalesce(sum(completion_cost_usd), 0) as completion_cost_usd,
                coalesce(sum(total_cost_usd), 0) as total_cost_usd,
                3 as sort_key
            from grouped
        )
        select
            stage_name,
            eval_calls,
            prompt_tokens,
            completion_tokens,
            total_tokens,
            prompt_cost_usd,
            completion_cost_usd,
            total_cost_usd
        from unioned
        order by sort_key
        """,
        (run_id,),
    ).fetchall()
    return [dict(row) for row in rows]


def _build_token_usage_section(
    runtime_usage: dict[str, Any],
    judge_usage_rows: list[dict[str, Any]],
) -> list[str]:
    runtime_total_cost = Decimal(str(runtime_usage.get("total_cost_usd", 0)))
    judge_total_row = next(
        (row for row in judge_usage_rows if row.get("stage_name") == "judge_total"),
        {
            "stage_name": "judge_total",
            "eval_calls": 0,
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "total_tokens": 0,
            "prompt_cost_usd": Decimal("0"),
            "completion_cost_usd": Decimal("0"),
            "total_cost_usd": Decimal("0"),
        },
    )
    judge_total_cost = Decimal(str(judge_total_row.get("total_cost_usd", 0)))
    run_total_cost = runtime_total_cost + judge_total_cost

    lines = [
        "## Token Usage",
        "",
        "### Runtime",
        "| scope | requests | prompt tokens | completion tokens | total tokens | prompt cost usd | completion cost usd | total cost usd |",
        "|---|---:|---:|---:|---:|---:|---:|---:|",
        "| runtime "
        f"| {_format_int(runtime_usage.get('requests', 0))} "
        f"| {_format_int(runtime_usage.get('prompt_tokens', 0))} "
        f"| {_format_int(runtime_usage.get('completion_tokens', 0))} "
        f"| {_format_int(runtime_usage.get('total_tokens', 0))} "
        f"| {_format_cost(runtime_usage.get('prompt_cost_usd', 0))} "
        f"| {_format_cost(runtime_usage.get('completion_cost_usd', 0))} "
        f"| {_format_cost(runtime_usage.get('total_cost_usd', 0))} |",
        "",
        "### Judge",
        "| scope | eval calls | prompt tokens | completion tokens | total tokens | prompt cost usd | completion cost usd | total cost usd |",
        "|---|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for row in judge_usage_rows:
        lines.append(
            f"| {row.get('stage_name', '')} "
            f"| {_format_int(row.get('eval_calls', 0))} "
            f"| {_format_int(row.get('prompt_tokens', 0))} "
            f"| {_format_int(row.get('completion_tokens', 0))} "
            f"| {_format_int(row.get('total_tokens', 0))} "
            f"| {_format_cost(row.get('prompt_cost_usd', 0))} "
            f"| {_format_cost(row.get('completion_cost_usd', 0))} "
            f"| {_format_cost(row.get('total_cost_usd', 0))} |"
        )
    lines.extend(
        [
            "",
            "Run total cost usd = runtime total cost usd + judge total cost usd",
            "",
            f"Run total cost usd = {_format_cost(runtime_total_cost)} + {_format_cost(judge_total_cost)} = {_format_cost(run_total_cost)}",
        ]
    )
    return lines


def _build_run_report(
    connection: Connection[Any],
    run_id: str,
    manifest: dict[str, Any],
    judge_settings: JudgeSettings,
) -> str:
    generation_rows = connection.execute(
        """
        select request_id, trace_id, suite_name::text as suite_name, score, label
        from judge_generation_results
        where run_id = %s
        order by request_id asc, suite_name asc
        """,
        (run_id,),
    ).fetchall()
    retrieval_rows = connection.execute(
        """
        select request_id, trace_id, score, label, selected_for_generation, retrieval_rank
        from judge_retrieval_results
        where run_id = %s and suite_name = 'retrieval_relevance'
        order by request_id asc, retrieval_rank asc
        """,
        (run_id,),
    ).fetchall()
    retrieval_quality_row = connection.execute(
        """
        select
            min(retrieval_evaluated_k)   as retrieval_evaluated_k,
            min(reranking_evaluated_k)   as reranking_evaluated_k,
            avg(retrieval_recall_soft)   as retrieval_recall_soft,
            avg(retrieval_recall_strict) as retrieval_recall_strict,
            avg(retrieval_rr_soft)       as retrieval_rr_soft,
            avg(retrieval_rr_strict)     as retrieval_rr_strict,
            avg(retrieval_ndcg)          as retrieval_ndcg,
            avg(reranking_recall_soft)   as reranking_recall_soft,
            avg(reranking_recall_strict) as reranking_recall_strict,
            avg(reranking_rr_soft)       as reranking_rr_soft,
            avg(reranking_rr_strict)     as reranking_rr_strict,
            avg(reranking_ndcg)          as reranking_ndcg,
            avg(retrieval_context_loss_soft)   as retrieval_context_loss_soft,
            avg(retrieval_context_loss_strict) as retrieval_context_loss_strict,
            avg(retrieval_num_relevant_soft)   as retrieval_num_relevant_soft,
            avg(retrieval_num_relevant_strict) as retrieval_num_relevant_strict,
            avg(reranking_num_relevant_soft)   as reranking_num_relevant_soft,
            avg(reranking_num_relevant_strict) as reranking_num_relevant_strict
        from request_run_summaries
        where run_id = %s
        """,
        (run_id,),
    ).fetchone()
    retrieval_depth_row = connection.execute(
        """
        select
            avg(retrieval_chunk_count) as avg_retrieval_chunk_count,
            min(retrieval_chunk_count) as min_retrieval_chunk_count,
            max(retrieval_chunk_count) as max_retrieval_chunk_count
        from request_run_summaries
        where run_id = %s
        """,
        (run_id,),
    ).fetchone()
    conditional_row = connection.execute(
        """
        select
            avg(groundedness_score)
                filter (where retrieval_num_relevant_soft > 0)
                as groundedness_retrieval_soft,
            avg(answer_completeness_score)
                filter (where retrieval_num_relevant_soft > 0)
                as answer_completeness_retrieval_soft,
            avg(answer_relevance_score)
                filter (where retrieval_num_relevant_soft > 0)
                as answer_relevance_retrieval_soft,
            avg(case when groundedness_score < 1.0 then 1.0 else 0.0 end)
                filter (where retrieval_first_relevant_rank_soft is distinct from 1)
                as hallucination_retrieval_soft,
            avg(case when groundedness_score = 1.0 and answer_completeness_score = 1.0
                     then 1.0 else 0.0 end)
                filter (where retrieval_num_relevant_soft > 0)
                as success_retrieval_soft,
            avg(groundedness_score)
                filter (where retrieval_num_relevant_strict > 0)
                as groundedness_retrieval_strict,
            avg(answer_completeness_score)
                filter (where retrieval_num_relevant_strict > 0)
                as answer_completeness_retrieval_strict,
            avg(answer_relevance_score)
                filter (where retrieval_num_relevant_strict > 0)
                as answer_relevance_retrieval_strict,
            avg(case when groundedness_score < 1.0 then 1.0 else 0.0 end)
                filter (where retrieval_first_relevant_rank_strict is distinct from 1)
                as hallucination_retrieval_strict,
            avg(case when groundedness_score = 1.0 and answer_completeness_score = 1.0
                     then 1.0 else 0.0 end)
                filter (where retrieval_num_relevant_strict > 0)
                as success_retrieval_strict,
            avg(groundedness_score)
                filter (where reranking_num_relevant_soft > 0)
                as groundedness_reranking_soft,
            avg(answer_completeness_score)
                filter (where reranking_num_relevant_soft > 0)
                as answer_completeness_reranking_soft,
            avg(answer_relevance_score)
                filter (where reranking_num_relevant_soft > 0)
                as answer_relevance_reranking_soft,
            avg(case when groundedness_score < 1.0 then 1.0 else 0.0 end)
                filter (where reranking_first_relevant_rank_soft is distinct from 1)
                as hallucination_reranking_soft,
            avg(case when groundedness_score = 1.0 and answer_completeness_score = 1.0
                     then 1.0 else 0.0 end)
                filter (where reranking_num_relevant_soft > 0)
                as success_reranking_soft,
            avg(groundedness_score)
                filter (where reranking_num_relevant_strict > 0)
                as groundedness_reranking_strict,
            avg(answer_completeness_score)
                filter (where reranking_num_relevant_strict > 0)
                as answer_completeness_reranking_strict,
            avg(answer_relevance_score)
                filter (where reranking_num_relevant_strict > 0)
                as answer_relevance_reranking_strict,
            avg(case when groundedness_score < 1.0 then 1.0 else 0.0 end)
                filter (where reranking_first_relevant_rank_strict is distinct from 1)
                as hallucination_reranking_strict,
            avg(case when groundedness_score = 1.0 and answer_completeness_score = 1.0
                     then 1.0 else 0.0 end)
                filter (where reranking_num_relevant_strict > 0)
                as success_reranking_strict,
            min(retrieval_evaluated_k) as retrieval_evaluated_k,
            min(reranking_evaluated_k) as reranking_evaluated_k
        from request_run_summaries
        where run_id = %s
        """,
        (run_id,),
    ).fetchone()

    run_scope_request_ids = tuple(manifest.get("run_scope_request_ids", []))
    pipeline_configs = _load_run_pipeline_configs(connection, run_scope_request_ids)
    runtime_usage = _load_runtime_token_usage(connection, run_scope_request_ids)
    judge_usage_rows = _load_judge_token_usage(connection, run_id)

    requests_evaluated = len({row["request_id"] for row in generation_rows})
    metric_lines = _build_metric_lines(generation_rows, retrieval_rows)
    label_distribution_lines = _build_label_distribution_lines(generation_rows)
    retrieval_quality_lines = _build_retrieval_quality_section(retrieval_quality_row)
    conditional_lines = _build_conditional_retrieval_generation_section(conditional_row)
    worst_case_lines = _build_worst_case_lines(generation_rows)

    retriever_section = _build_retriever_section(
        pipeline_configs["retriever_kind"],
        pipeline_configs["retriever_config"] or {},
        pipeline_configs.get("top_k_requested"),
        retrieval_depth_row,
    )
    reranker_section = _build_reranker_section(
        pipeline_configs["reranker_kind"],
        pipeline_configs["reranker_config"],
    )
    generation_section = _build_generation_section(pipeline_configs["generation_config"] or {})
    judge_section = _build_judge_section(judge_settings)
    token_usage_section = _build_token_usage_section(runtime_usage, judge_usage_rows)

    lines: list[str] = [
        "# Eval Run Report",
        "",
        "## Run Metadata",
        f"- eval_run_id: `{manifest['run_id']}`",
        f"- run_type: `{manifest['run_type']}`",
        f"- status: `{manifest['status']}`",
        f"- started_at: `{manifest['started_at']}`",
    ]
    if pipeline_configs.get("runtime_run_id"):
        lines.append(f"- runtime_run_id: `{pipeline_configs['runtime_run_id']}`")
    if "completed_at" in manifest:
        lines.append(f"- completed_at: `{manifest['completed_at']}`")
    lines.extend([
        f"- request_count: `{manifest['request_count']}`",
        f"- requests_evaluated: `{requests_evaluated}`",
        f"- generation_suite_versions: `{json.dumps(manifest['generation_suite_versions'], sort_keys=True)}`",
        f"- retrieval_suite_versions: `{json.dumps(manifest['retrieval_suite_versions'], sort_keys=True)}`",
        "",
        *retriever_section,
        "",
        *reranker_section,
        "",
        *generation_section,
        "",
        *judge_section,
        "",
        "## Aggregated Metrics",
        "| metric | mean/rate | p50 | p90 | count |",
        "|---|---:|---:|---:|---:|",
        *metric_lines,
        "",
        "## Label Distributions",
        "| suite | label | count | percent |",
        "|---|---|---:|---:|",
        *label_distribution_lines,
    ])
    if retrieval_quality_lines:
        lines += ["", *retrieval_quality_lines]
    if conditional_lines:
        lines += ["", *conditional_lines]
    lines.extend([
        "",
        "## Worst-Case Preview",
        *worst_case_lines,
        "",
        *token_usage_section,
        "",
    ])
    return "\n".join(lines)


def _load_run_retriever_kind(
    connection: Connection[Any],
    run_scope_request_ids: tuple[str, ...],
) -> str:
    try:
        rows = connection.execute(
            """
            select distinct retriever_kind
            from request_captures
            where request_id = any(%s)
            order by retriever_kind asc
            """,
            (list(run_scope_request_ids),),
        ).fetchall()
    except Exception as error:  # pragma: no cover
        raise RunBootstrapError(str(error)) from error

    retriever_kinds = [row["retriever_kind"] for row in rows if row.get("retriever_kind")]
    if not retriever_kinds:
        raise RunBootstrapError("run scope does not contain any retriever_kind values")
    if len(set(retriever_kinds)) != 1:
        raise RunBootstrapError(
            "run scope contains multiple retriever_kind values: "
            + ", ".join(sorted(set(str(value) for value in retriever_kinds)))
        )
    return str(retriever_kinds[0])


def _load_run_reranker_kind(
    connection: Connection[Any],
    run_scope_request_ids: tuple[str, ...],
) -> str:
    try:
        rows = connection.execute(
            """
            select distinct reranker_kind
            from request_captures
            where request_id = any(%s)
            order by reranker_kind asc
            """,
            (list(run_scope_request_ids),),
        ).fetchall()
    except Exception as error:  # pragma: no cover
        raise RunBootstrapError(str(error)) from error

    reranker_kinds = [row["reranker_kind"] for row in rows if row.get("reranker_kind")]
    if not reranker_kinds:
        raise RunBootstrapError("run scope does not contain any reranker_kind values")
    if len(set(reranker_kinds)) != 1:
        raise RunBootstrapError(
            "run scope contains multiple reranker_kind values: "
            + ", ".join(sorted(set(str(value) for value in reranker_kinds)))
        )
    return str(reranker_kinds[0])


def _build_metric_lines(
    generation_rows: list[dict[str, Any]],
    retrieval_rows: list[dict[str, Any]],
) -> list[str]:
    generation_by_suite: dict[str, list[Decimal]] = {}
    for row in generation_rows:
        score = row["score"]
        if score is not None:
            generation_by_suite.setdefault(row["suite_name"], []).append(score)

    retrieval_scores = [row["score"] for row in retrieval_rows if row["score"] is not None]
    retrieval_selected_scores = [
        row["score"]
        for row in retrieval_rows
        if row["score"] is not None and row["selected_for_generation"]
    ]
    retrieval_by_request: dict[str, list[dict[str, Any]]] = {}
    for row in retrieval_rows:
        retrieval_by_request.setdefault(row["request_id"], []).append(row)
    weighted_values = [
        value
        for value in (_weighted_request_retrieval(rows) for rows in retrieval_by_request.values())
        if value is not None
    ]

    refusal_rows = [row for row in generation_rows if row["suite_name"] == "correct_refusal"]
    refusal_rate_values = [
        Decimal("1.0") if row["label"] == "correct_refusal" else Decimal("0.0")
        for row in refusal_rows
    ]

    metrics = [
        ("answer_completeness_mean", generation_by_suite.get("answer_completeness", [])),
        ("groundedness_mean", generation_by_suite.get("groundedness", [])),
        ("answer_relevance_mean", generation_by_suite.get("answer_relevance", [])),
        ("correct_refusal_rate", refusal_rate_values),
        ("retrieval_relevance_mean", retrieval_scores),
        ("retrieval_relevance_selected_mean", retrieval_selected_scores),
        ("retrieval_relevance_weighted_topk_mean", weighted_values),
    ]
    return [
        _format_metric_row(metric_name, values)
        for metric_name, values in metrics
    ]


def _format_metric_row(metric_name: str, values: list[Decimal]) -> str:
    if not values:
        return f"| {metric_name} | n/a | n/a | n/a | 0 |"
    sorted_values = sorted(values)
    mean_value = sum(sorted_values) / Decimal(len(sorted_values))
    p50 = _percentile(sorted_values, 0.50)
    p90 = _percentile(sorted_values, 0.90)
    return (
        f"| {metric_name} | {mean_value:.4f} | {p50:.4f} | {p90:.4f} | {len(sorted_values)} |"
    )


def _percentile(sorted_values: list[Decimal], percentile: float) -> Decimal:
    if len(sorted_values) == 1:
        return sorted_values[0]
    index = max(0, min(len(sorted_values) - 1, math.ceil(percentile * len(sorted_values)) - 1))
    return sorted_values[index]


def _weighted_request_retrieval(rows: list[dict[str, Any]]) -> Decimal | None:
    weighted_sum = Decimal("0")
    total_weight = Decimal("0")
    for row in rows:
        if row["score"] is None:
            continue
        weight = Decimal("1") / Decimal(row["retrieval_rank"])
        weighted_sum += row["score"] * weight
        total_weight += weight
    if total_weight == 0:
        return None
    return weighted_sum / total_weight


def _build_label_distribution_lines(generation_rows: list[dict[str, Any]]) -> list[str]:
    ordered_labels = {
        "answer_completeness": ("complete", "partial", "incomplete"),
        "groundedness": ("grounded", "partially_grounded", "ungrounded"),
        "answer_relevance": ("relevant", "partially_relevant", "irrelevant"),
        "correct_refusal": ("correct_refusal", "unnecessary_refusal", "non_refusal"),
    }
    lines: list[str] = []
    for suite_name, labels in ordered_labels.items():
        suite_rows = [row for row in generation_rows if row["suite_name"] == suite_name]
        total = len(suite_rows)
        counts = {
            label: sum(1 for row in suite_rows if row["label"] == label)
            for label in labels
        }
        for label in labels:
            percent = (counts[label] / total * 100.0) if total else 0.0
            lines.append(f"| {suite_name} | {label} | {counts[label]} | {percent:.1f}% |")
    return lines


def _build_worst_case_lines(generation_rows: list[dict[str, Any]]) -> list[str]:
    grouped: dict[str, dict[str, dict[str, Any]]] = {}
    for row in generation_rows:
        grouped.setdefault(row["request_id"], {})[row["suite_name"]] = row

    def suite_preview(suite_name: str) -> list[str]:
        candidates = [
            (
                rows[suite_name]["score"],
                request_id,
                rows[suite_name]["trace_id"],
            )
            for request_id, rows in grouped.items()
            if suite_name in rows and rows[suite_name]["score"] is not None
        ]
        candidates.sort(key=lambda item: (item[0], item[1]))
        lines = [f"### Lowest {suite_name} requests"]
        if not candidates:
            lines.append("- none")
            return lines
        for score, request_id, trace_id in candidates[:5]:
            lines.append(f"- request_id=`{request_id}` score=`{score:.4f}` trace_id=`{trace_id}`")
        return lines

    return [
        *suite_preview("groundedness"),
        "",
        *suite_preview("answer_completeness"),
    ]


def _build_retrieval_quality_section(row: dict[str, Any] | None) -> list[str]:
    if row is None:
        return []
    if all(row.get(col) is None for col in _RETRIEVAL_QUALITY_METRIC_COLS):
        return []

    rk = int(row["retrieval_evaluated_k"])
    nk = int(row["reranking_evaluated_k"])

    def fmt(value: Any) -> str:
        return "n/a" if value is None else f"{value:.4f}"

    retrieval_label = f"retrieval@{rk}"
    reranking_label = f"generation_context@{nk}"

    return [
        "## Retrieval Quality",
        "",
        "| set | Recall soft | Recall strict | MRR soft | MRR strict | nDCG |",
        "|---|---:|---:|---:|---:|---:|",
        (
            f"| {retrieval_label}"
            f" | {fmt(row.get('retrieval_recall_soft'))}"
            f" | {fmt(row.get('retrieval_recall_strict'))}"
            f" | {fmt(row.get('retrieval_rr_soft'))}"
            f" | {fmt(row.get('retrieval_rr_strict'))}"
            f" | {fmt(row.get('retrieval_ndcg'))} |"
        ),
        (
            f"| {reranking_label}"
            f" | {fmt(row.get('reranking_recall_soft'))}"
            f" | {fmt(row.get('reranking_recall_strict'))}"
            f" | {fmt(row.get('reranking_rr_soft'))}"
            f" | {fmt(row.get('reranking_rr_strict'))}"
            f" | {fmt(row.get('reranking_ndcg'))} |"
        ),
        "",
        f"- retrieval_context_loss_soft: {fmt(row.get('retrieval_context_loss_soft'))}",
        f"- retrieval_context_loss_strict: {fmt(row.get('retrieval_context_loss_strict'))}",
        f"- avg_num_relevant_in_{retrieval_label}_soft: {fmt(row.get('retrieval_num_relevant_soft'))}",
        f"- avg_num_relevant_in_{retrieval_label}_strict: {fmt(row.get('retrieval_num_relevant_strict'))}",
        f"- avg_num_relevant_in_{reranking_label}_soft: {fmt(row.get('reranking_num_relevant_soft'))}",
        f"- avg_num_relevant_in_{reranking_label}_strict: {fmt(row.get('reranking_num_relevant_strict'))}",
    ]


def _build_conditional_retrieval_generation_section(row: dict[str, Any] | None) -> list[str]:
    if row is None:
        return []
    if all(row.get(key) is None for key in _CONDITIONAL_AGGREGATE_KEYS):
        return []

    rk = int(row["retrieval_evaluated_k"])
    nk = int(row["reranking_evaluated_k"])

    def fmt(value: Any) -> str:
        return "n/a" if value is None else f"{value:.4f}"

    retrieval_label = f"retrieval@{rk}"
    reranking_label = f"generation_context@{nk}"
    description = (
        f"These aggregates show generation quality conditioned on whether retrieval supplied"
        f" relevant context, separately for {retrieval_label}/{reranking_label} and soft/strict relevance."
    )
    lines: list[str] = [
        "## Conditional Retrieval\u2192Generation Aggregates",
        "",
        description,
        "",
        f"| metric | {retrieval_label}_soft | {retrieval_label}_strict | {reranking_label}_soft | {reranking_label}_strict |",
        "|---|---:|---:|---:|---:|",
    ]
    for row_label, metric_prefix in _CONDITIONAL_ROW_METRICS:
        lines.append(
            f"| {row_label}"
            f" | {fmt(row.get(f'{metric_prefix}_retrieval_soft'))}"
            f" | {fmt(row.get(f'{metric_prefix}_retrieval_strict'))}"
            f" | {fmt(row.get(f'{metric_prefix}_reranking_soft'))}"
            f" | {fmt(row.get(f'{metric_prefix}_reranking_strict'))} |"
        )
    lines.extend([
        "",
        "_Definitions: success = groundedness == 1.0 AND answer\\_completeness == 1.0_",
        "",
        "_Definitions: hallucinated = groundedness < 1.0_",
    ])
    return lines


def _now() -> str:
    return datetime.now(timezone.utc).isoformat()


def _parse_started_at(value: str) -> datetime | None:
    if not value:
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None


def _worker_run_id(worker: Any) -> str | None:
    closure = getattr(worker, "__closure__", None)
    if not closure:
        return None
    for cell in closure:
        value = cell.cell_contents
        if hasattr(value, "run_id"):
            return getattr(value, "run_id")
    return None


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
