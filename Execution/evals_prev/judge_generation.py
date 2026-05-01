from __future__ import annotations

import json
from dataclasses import dataclass
from datetime import datetime, timezone
from decimal import Decimal
from typing import Any

from psycopg import Connection
from psycopg.rows import dict_row
from psycopg.types.json import Json

from .common import (
    extract_text_from_openai_compatible_response,
    SuitePrompt,
    ensure_json_array,
    ensure_json_object,
    load_chunk_index,
    load_generation_suite_prompts,
    parse_json_object_from_model_text,
    render_prompt_template,
)
from .logging_utils import get_logger, log_kv
from .judge_transport import JudgeSettings, build_judge_client, create_chat_completion_with_retry
from .judge_usage import (
    JudgeLlmCallRecord,
    JudgeUsageError,
    build_judge_llm_call_record,
    insert_judge_llm_call,
)
from .observability import span_context_for_child, traced_operation


STAGE_NAME = "judge_generation"
REQUIRED_SUITES = (
    "answer_completeness",
    "groundedness",
    "answer_relevance",
    "correct_refusal",
)
LOGGER = get_logger("evals.judge_generation")

LABEL_TO_SCORE: dict[str, dict[str, Decimal]] = {
    "answer_completeness": {
        "complete": Decimal("1.0"),
        "partial": Decimal("0.5"),
        "incomplete": Decimal("0.0"),
    },
    "groundedness": {
        "grounded": Decimal("1.0"),
        "partially_grounded": Decimal("0.5"),
        "ungrounded": Decimal("0.0"),
    },
    "answer_relevance": {
        "relevant": Decimal("1.0"),
        "partially_relevant": Decimal("0.5"),
        "irrelevant": Decimal("0.0"),
    },
    "correct_refusal": {
        "correct_refusal": Decimal("1.0"),
        "unnecessary_refusal": Decimal("0.0"),
        "non_refusal": Decimal("0.0"),
    },
}


@dataclass(frozen=True)
class JudgeGenerationParams:
    postgres_url: str
    run_id: str
    judge_settings: JudgeSettings
    chunks_path: str
    run_scope_request_ids: tuple[str, ...]


@dataclass(frozen=True)
class ProcessingStateRow:
    request_id: str
    request_received_at: datetime
    current_stage: str
    status: str
    attempt_count: int


@dataclass(frozen=True)
class RequestCaptureRow:
    request_id: str
    trace_id: str
    normalized_query: str
    final_answer: str
    retrieval_results: list[dict[str, Any]]


@dataclass(frozen=True)
class GenerationJudgeResult:
    request_id: str
    run_id: str
    trace_id: str
    suite_name: str
    judge_model: str
    judge_prompt_version: str
    score: Decimal
    label: str
    explanation: str
    raw_response: dict[str, Any]
    judge_llm_call: JudgeLlmCallRecord


class JudgeGenerationStageError(RuntimeError):
    """Base stage error."""


class FIFOSelectionError(JudgeGenerationStageError):
    pass


class RequestCaptureLookupError(JudgeGenerationStageError):
    pass


class ChunkResolutionError(JudgeGenerationStageError):
    pass


class JudgeTransportError(JudgeGenerationStageError):
    pass


class JudgeResponseParsingError(JudgeGenerationStageError):
    pass


class GenerationResultRowMappingError(JudgeGenerationStageError):
    pass


class DatabaseWriteError(JudgeGenerationStageError):
    pass


class StateTransitionWriteError(JudgeGenerationStageError):
    pass


class InternalStageStateError(JudgeGenerationStageError):
    pass


def run_judge_generation(params: JudgeGenerationParams) -> bool:
    if not params.run_scope_request_ids:
        log_kv(LOGGER, "no_run_scope_requests", stage=STAGE_NAME, run_id=params.run_id)
        return False

    prompts = load_generation_suite_prompts()
    chunk_index = load_chunk_index(params.chunks_path)
    judge_client = build_judge_client(params.judge_settings)

    try:
        with Connection.connect(params.postgres_url, row_factory=dict_row) as connection:
            connection.autocommit = True
            state_row = _select_next_request(connection, params.run_scope_request_ids)
            if state_row is None:
                log_kv(LOGGER, "no_eligible_work", stage=STAGE_NAME, run_id=params.run_id)
                return False

            log_kv(
                LOGGER,
                "request_selected",
                stage=STAGE_NAME,
                run_id=params.run_id,
                request_id=state_row.request_id,
                status=state_row.status,
                attempt_count=state_row.attempt_count,
            )
            with traced_operation(
                tracer_name="Execution.evals.judge_generation",
                span_name="eval.judge_generation.request",
                attributes={
                    "run_id": params.run_id,
                    "request_id": state_row.request_id,
                    "stage": STAGE_NAME,
                    "status": state_row.status,
                    "attempt_count": state_row.attempt_count,
                    "judge_provider": params.judge_settings.provider,
                    "judge_model": params.judge_settings.model_name,
                },
            ) as request_span:
                _mark_running(connection, state_row.request_id)
                request_span_context = span_context_for_child(request_span)

                try:
                    request_capture = _load_request_capture(connection, state_row.request_id)
                    existing_suites = _load_existing_suites(
                        connection,
                        request_capture.request_id,
                        params.run_id,
                    )
                    missing_suites = tuple(
                        suite_name
                        for suite_name in REQUIRED_SUITES
                        if suite_name not in existing_suites
                    )
                    log_kv(
                        LOGGER,
                        "missing_suites_computed",
                        stage=STAGE_NAME,
                        run_id=params.run_id,
                        request_id=request_capture.request_id,
                        missing_suites=list(missing_suites),
                    )

                    selected_context_chunks = _resolve_selected_context_chunks(
                        request_capture,
                        chunk_index,
                    )
                    for suite_name in missing_suites:
                        prompt = prompts[suite_name]
                        result = _evaluate_suite(
                            judge_client=judge_client,
                            params=params,
                            request_capture=request_capture,
                            suite_prompt=prompt,
                            selected_context_chunks=selected_context_chunks,
                            request_span_context=request_span_context,
                        )
                        _insert_judge_llm_call(connection, result.judge_llm_call)
                        _insert_generation_result(connection, result)

                    final_suites = _load_existing_suites(
                        connection,
                        request_capture.request_id,
                        params.run_id,
                    )
                    if set(final_suites) != set(REQUIRED_SUITES):
                        raise InternalStageStateError(
                            "judge_generation completed without all required suite rows"
                        )

                    _mark_completed(connection, state_row.request_id)
                    log_kv(
                        LOGGER,
                        "request_completed",
                        stage=STAGE_NAME,
                        run_id=params.run_id,
                        request_id=state_row.request_id,
                    )
                    return True
                except (JudgeGenerationStageError, JudgeUsageError) as error:
                    _mark_failed(connection, state_row.request_id, str(error))
                    log_kv(
                        LOGGER,
                        "request_failed",
                        stage=STAGE_NAME,
                        run_id=params.run_id,
                        request_id=state_row.request_id,
                        error=str(error),
                    )
                    raise
                except Exception as error:  # pragma: no cover - defensive boundary
                    _mark_failed(connection, state_row.request_id, f"unexpected internal state: {error}")
                    log_kv(
                        LOGGER,
                        "request_failed",
                        stage=STAGE_NAME,
                        run_id=params.run_id,
                        request_id=state_row.request_id,
                        error=str(error),
                    )
                    raise InternalStageStateError(str(error)) from error
    except JudgeGenerationStageError:
        raise
    except Exception as error:  # pragma: no cover - psycopg/openai unexpected wrapper
        raise InternalStageStateError(str(error)) from error


def _select_next_request(
    connection: Connection[Any],
    run_scope_request_ids: tuple[str, ...],
) -> ProcessingStateRow | None:
    try:
        with connection.transaction():
            row = connection.execute(
                """
                select
                    request_id,
                    request_received_at,
                    current_stage::text as current_stage,
                    status::text as status,
                    attempt_count
                from eval_processing_state
                where current_stage = %s
                  and request_id = any(%s)
                  and status != 'completed'
                order by request_received_at asc, request_id asc
                limit 1
                for update
                """,
                (STAGE_NAME, list(run_scope_request_ids)),
            ).fetchone()
    except Exception as error:  # pragma: no cover - DB boundary
        raise FIFOSelectionError(str(error)) from error

    if row is None:
        return None

    return ProcessingStateRow(
        request_id=row["request_id"],
        request_received_at=row["request_received_at"],
        current_stage=row["current_stage"],
        status=row["status"],
        attempt_count=row["attempt_count"],
    )


def _mark_running(connection: Connection[Any], request_id: str) -> None:
    try:
        with connection.transaction():
            connection.execute(
                """
                update eval_processing_state
                set
                    current_stage = %s,
                    status = 'running',
                    attempt_count = attempt_count + 1,
                    started_at = now(),
                    updated_at = now()
                where request_id = %s
                """,
                (STAGE_NAME, request_id),
            )
    except Exception as error:  # pragma: no cover - DB boundary
        raise StateTransitionWriteError(str(error)) from error


def _load_request_capture(connection: Connection[Any], request_id: str) -> RequestCaptureRow:
    try:
        row = connection.execute(
            """
            select request_id, trace_id, normalized_query, final_answer, retrieval_results
            from request_captures
            where request_id = %s
            """,
            (request_id,),
        ).fetchone()
    except Exception as error:  # pragma: no cover - DB boundary
        raise RequestCaptureLookupError(str(error)) from error

    if row is None:
        raise RequestCaptureLookupError(f"request_captures row not found for request_id={request_id}")

    return RequestCaptureRow(
        request_id=row["request_id"],
        trace_id=row["trace_id"],
        normalized_query=row["normalized_query"],
        final_answer=row["final_answer"],
        retrieval_results=ensure_json_array(row["retrieval_results"]),
    )


def _load_existing_suites(
    connection: Connection[Any],
    request_id: str,
    run_id: str,
) -> tuple[str, ...]:
    rows = connection.execute(
        """
        select suite_name::text as suite_name
        from judge_generation_results
        where request_id = %s and run_id = %s
        """,
        (request_id, run_id),
    ).fetchall()
    return tuple(row["suite_name"] for row in rows)


def _resolve_selected_context_chunks(
    request_capture: RequestCaptureRow,
    chunk_index: dict[str, dict[str, Any]],
) -> str:
    selected_items = [
        item
        for item in request_capture.retrieval_results
        if bool(item["selected_for_generation"])
    ]
    chunk_blocks: list[str] = []
    for position, item in enumerate(selected_items, start=1):
        chunk_id = str(item["chunk_id"])
        chunk_record = chunk_index.get(chunk_id)
        if chunk_record is None:
            raise ChunkResolutionError(f"missing chunk text for chunk_id={chunk_id}")
        chunk_blocks.append(
            f"[Chunk {position}] chunk_id={chunk_id}\n"
            f"{chunk_record['text']}"
        )
    return "\n\n".join(chunk_blocks)


def _evaluate_suite(
    judge_client: OpenAI,
    params: JudgeGenerationParams,
    request_capture: RequestCaptureRow,
    suite_prompt: SuitePrompt,
    selected_context_chunks: str,
    request_span_context: object,
) -> GenerationJudgeResult:
    variables = {
        "normalized_query": request_capture.normalized_query,
        "final_answer": request_capture.final_answer,
        "selected_context_chunks": selected_context_chunks,
    }
    prompt_variables = {
        key: variables[key]
        for key in suite_prompt.input_variables
    }
    rendered_prompt = render_prompt_template(
        suite_prompt.prompt_template,
        prompt_variables,
    )

    try:
        log_kv(
            LOGGER,
            "suite_judge_started",
            stage=STAGE_NAME,
            run_id=params.run_id,
            request_id=request_capture.request_id,
            suite_name=suite_prompt.suite_name,
        )
        with traced_operation(
            tracer_name="Execution.evals.judge_generation",
            span_name="eval.judge_generation.suite",
            attributes={
                "run_id": params.run_id,
                "request_id": request_capture.request_id,
                "stage": STAGE_NAME,
                "suite_name": suite_prompt.suite_name,
                "judge_provider": params.judge_settings.provider,
                "judge_model": params.judge_settings.model_name,
                "status": "running",
            },
            parent_context=request_span_context,
        ):
            response = create_chat_completion_with_retry(
                judge_client,
                params.judge_settings,
                temperature=0.0,
                messages=[{"role": "user", "content": rendered_prompt}],
            )
        log_kv(
            LOGGER,
            "suite_judge_finished",
            stage=STAGE_NAME,
            run_id=params.run_id,
            request_id=request_capture.request_id,
            suite_name=suite_prompt.suite_name,
        )
    except Exception as error:  # pragma: no cover - transport boundary
        raise JudgeTransportError(
            f"suite={suite_prompt.suite_name} transport failure: {error}"
        ) from error

    raw_response = _extract_raw_response(response)
    try:
        judge_llm_call = build_judge_llm_call_record(
            request_id=request_capture.request_id,
            run_id=params.run_id,
            trace_id=request_capture.trace_id,
            stage_name=STAGE_NAME,
            suite_name=suite_prompt.suite_name,
            chunk_id=None,
            judge_prompt_version=suite_prompt.version,
            judge_settings=params.judge_settings,
            prompt_text=rendered_prompt,
            raw_response=raw_response,
            completion_text=None,
        )
    except JudgeUsageError as error:
        if "local token estimate requires extracted completion text" not in str(error):
            raise
        judge_llm_call = build_judge_llm_call_record(
            request_id=request_capture.request_id,
            run_id=params.run_id,
            trace_id=request_capture.trace_id,
            stage_name=STAGE_NAME,
            suite_name=suite_prompt.suite_name,
            chunk_id=None,
            judge_prompt_version=suite_prompt.version,
            judge_settings=params.judge_settings,
            prompt_text=rendered_prompt,
            raw_response=raw_response,
            completion_text=_extract_message_content(raw_response),
        )
    label, score, explanation = _normalize_judge_response(
        suite_prompt.suite_name,
        raw_response,
    )

    return GenerationJudgeResult(
        request_id=request_capture.request_id,
        run_id=params.run_id,
        trace_id=request_capture.trace_id,
        suite_name=suite_prompt.suite_name,
        judge_model=params.judge_settings.model_name,
        judge_prompt_version=suite_prompt.version,
        score=score,
        label=label,
        explanation=explanation,
        raw_response=raw_response,
        judge_llm_call=judge_llm_call,
    )


def _extract_raw_response(response: Any) -> dict[str, Any]:
    if hasattr(response, "model_dump"):
        dumped = response.model_dump()
        if isinstance(dumped, dict):
            return dumped
    if hasattr(response, "to_dict"):
        dumped = response.to_dict()
        if isinstance(dumped, dict):
            return dumped
    raise JudgeResponseParsingError("judge response could not be serialized into a JSON object")


def _normalize_judge_response(
    suite_name: str,
    raw_response: dict[str, Any],
) -> tuple[str, Decimal, str]:
    content = _extract_message_content(raw_response)
    try:
        parsed = parse_json_object_from_model_text(content)
    except ValueError as error:
        raise JudgeResponseParsingError(
            f"suite={suite_name} returned non-JSON content: {content!r}"
        ) from error

    label = str(parsed.get("label", "")).strip().lower()
    explanation = str(parsed.get("explanation", "")).strip()
    if not explanation:
        raise JudgeResponseParsingError(f"suite={suite_name} returned empty explanation")

    suite_mapping = LABEL_TO_SCORE.get(suite_name)
    if suite_mapping is None:
        raise GenerationResultRowMappingError(f"unsupported suite_name={suite_name}")
    if label not in suite_mapping:
        raise JudgeResponseParsingError(
            f"suite={suite_name} returned unsupported label={label!r}"
        )
    return label, suite_mapping[label], explanation


def _extract_message_content(raw_response: dict[str, Any]) -> str:
    try:
        return extract_text_from_openai_compatible_response(raw_response)
    except ValueError as error:
        raise JudgeResponseParsingError(
            f"judge response missing message content; available top-level keys={sorted(raw_response.keys())}"
        ) from error


def _insert_generation_result(connection: Connection[Any], result: GenerationJudgeResult) -> None:
    try:
        with connection.transaction():
            connection.execute(
                """
                insert into judge_generation_results (
                    request_id,
                    run_id,
                    trace_id,
                    suite_name,
                    judge_model,
                    judge_prompt_version,
                    score,
                    label,
                    explanation,
                    raw_response
                ) values (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
                on conflict (request_id, run_id, suite_name) do nothing
                """,
                (
                    result.request_id,
                    result.run_id,
                    result.trace_id,
                    result.suite_name,
                    result.judge_model,
                    result.judge_prompt_version,
                    result.score,
                    result.label,
                    result.explanation,
                    Json(result.raw_response),
                ),
            )
    except Exception as error:  # pragma: no cover - DB boundary
        raise DatabaseWriteError(str(error)) from error


def _insert_judge_llm_call(connection: Connection[Any], record: JudgeLlmCallRecord) -> None:
    try:
        with connection.transaction():
            insert_judge_llm_call(connection, record)
    except Exception as error:  # pragma: no cover - DB boundary
        raise DatabaseWriteError(str(error)) from error


def _mark_completed(connection: Connection[Any], request_id: str) -> None:
    try:
        with connection.transaction():
            connection.execute(
                """
                update eval_processing_state
                set
                    current_stage = %s,
                    status = 'completed',
                    completed_at = now(),
                    updated_at = now(),
                    last_error = null
                where request_id = %s
                """,
                (STAGE_NAME, request_id),
            )
    except Exception as error:  # pragma: no cover - DB boundary
        raise StateTransitionWriteError(str(error)) from error


def _mark_failed(connection: Connection[Any], request_id: str, error_message: str) -> None:
    try:
        with connection.transaction():
            connection.execute(
                """
                update eval_processing_state
                set
                    current_stage = %s,
                    status = 'failed',
                    updated_at = now(),
                    last_error = %s
                where request_id = %s
                """,
                (STAGE_NAME, error_message, request_id),
            )
    except Exception as error:  # pragma: no cover - DB boundary
        raise StateTransitionWriteError(str(error)) from error
