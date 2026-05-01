from __future__ import annotations

import json
from dataclasses import dataclass
from datetime import datetime
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
    load_prompt_catalog,
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


STAGE_NAME = "judge_retrieval"
SUITE_NAME = "retrieval_relevance"
LABEL_TO_SCORE: dict[str, Decimal] = {
    "relevant": Decimal("1.0"),
    "partial": Decimal("0.5"),
    "irrelevant": Decimal("0.0"),
}
LOGGER = get_logger("evals.judge_retrieval")


@dataclass(frozen=True)
class JudgeRetrievalParams:
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
class RetrievalItem:
    chunk_id: str
    document_id: str
    retrieval_rank: int
    retrieval_score: Decimal
    rerank_score: Decimal
    selected_for_generation: bool
    chunk_text: str


@dataclass(frozen=True)
class RequestCaptureRow:
    request_id: str
    trace_id: str
    normalized_query: str
    reranker_kind: str
    reranker_config: dict[str, Any] | None
    retrieval_items: tuple[RetrievalItem, ...]


@dataclass(frozen=True)
class RetrievalJudgeResult:
    request_id: str
    run_id: str
    trace_id: str
    chunk_id: str
    document_id: str
    retrieval_rank: int
    retrieval_score: Decimal
    selected_for_generation: bool
    suite_name: str
    judge_model: str
    judge_prompt_version: str
    score: Decimal
    label: str
    explanation: str
    raw_response: dict[str, Any]
    judge_llm_call: JudgeLlmCallRecord


class JudgeRetrievalStageError(RuntimeError):
    """Base stage error."""


class FIFOSelectionError(JudgeRetrievalStageError):
    pass


class RequestCaptureLookupError(JudgeRetrievalStageError):
    pass


class ChunkResolutionError(JudgeRetrievalStageError):
    pass


class JudgeTransportError(JudgeRetrievalStageError):
    pass


class JudgeResponseParsingError(JudgeRetrievalStageError):
    pass


class RetrievalResultRowMappingError(JudgeRetrievalStageError):
    pass


class DatabaseWriteError(JudgeRetrievalStageError):
    pass


class StateTransitionWriteError(JudgeRetrievalStageError):
    pass


class InternalStageStateError(JudgeRetrievalStageError):
    pass


def run_judge_retrieval(params: JudgeRetrievalParams) -> bool:
    if not params.run_scope_request_ids:
        log_kv(LOGGER, "no_run_scope_requests", stage=STAGE_NAME, run_id=params.run_id)
        return False

    prompt = _load_retrieval_prompt()
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
                tracer_name="Execution.evals.judge_retrieval",
                span_name="eval.judge_retrieval.request",
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
                    request_capture = _load_request_capture(connection, state_row.request_id, chunk_index)
                    existing_chunk_ids = _load_existing_chunk_ids(
                        connection,
                        request_capture.request_id,
                        params.run_id,
                    )
                    missing_items = tuple(
                        item
                        for item in request_capture.retrieval_items
                        if item.chunk_id not in existing_chunk_ids
                    )
                    log_kv(
                        LOGGER,
                        "missing_chunk_judgments_computed",
                        stage=STAGE_NAME,
                        run_id=params.run_id,
                        request_id=request_capture.request_id,
                        missing_chunk_count=len(missing_items),
                    )

                    for item in missing_items:
                        result = _evaluate_retrieval_item(
                            judge_client=judge_client,
                            params=params,
                            request_capture=request_capture,
                            retrieval_item=item,
                            suite_prompt=prompt,
                            request_span_context=request_span_context,
                        )
                        _insert_judge_llm_call(connection, result.judge_llm_call)
                        _insert_retrieval_result(connection, result)

                    final_chunk_ids = _load_existing_chunk_ids(
                        connection,
                        request_capture.request_id,
                        params.run_id,
                    )
                    expected_chunk_ids = {item.chunk_id for item in request_capture.retrieval_items}
                    if final_chunk_ids != expected_chunk_ids:
                        raise InternalStageStateError(
                            "judge_retrieval completed without all required chunk rows"
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
                except (JudgeRetrievalStageError, JudgeUsageError) as error:
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
    except JudgeRetrievalStageError:
        raise
    except Exception as error:  # pragma: no cover - wrapper boundary
        raise InternalStageStateError(str(error)) from error


def _load_retrieval_prompt() -> SuitePrompt:
    catalog = load_prompt_catalog()
    entry = catalog["retrieval_suites"][SUITE_NAME]
    return SuitePrompt(
        suite_name=SUITE_NAME,
        prompt_id=entry["id"],
        version=entry["version"],
        prompt_template=entry["prompt_template"],
        input_variables=tuple(entry["input_variables"]),
    )


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
    except Exception as error:  # pragma: no cover
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
    except Exception as error:  # pragma: no cover
        raise StateTransitionWriteError(str(error)) from error


def _load_request_capture(
    connection: Connection[Any],
    request_id: str,
    chunk_index: dict[str, dict[str, Any]],
) -> RequestCaptureRow:
    try:
        row = connection.execute(
            """
            select request_id, trace_id, normalized_query, reranker_kind, reranker_config, retrieval_results
            from request_captures
            where request_id = %s
            """,
            (request_id,),
        ).fetchone()
    except Exception as error:  # pragma: no cover
        raise RequestCaptureLookupError(str(error)) from error

    if row is None:
        raise RequestCaptureLookupError(f"request_captures row not found for request_id={request_id}")

    retrieval_results = ensure_json_array(row["retrieval_results"])
    retrieval_items: list[RetrievalItem] = []
    for index, item in enumerate(retrieval_results, start=1):
        item_object = ensure_json_object(item)
        chunk_id = str(item_object["chunk_id"])
        chunk_record = chunk_index.get(chunk_id)
        if chunk_record is None:
            raise ChunkResolutionError(f"missing chunk text for chunk_id={chunk_id}")
        retrieval_items.append(
            RetrievalItem(
                chunk_id=chunk_id,
                document_id=str(item_object["document_id"]),
                retrieval_rank=index,
                retrieval_score=Decimal(
                    str(item_object.get("retrieval_score", item_object.get("score")))
                ),
                rerank_score=Decimal(
                    str(
                        item_object.get(
                            "rerank_score",
                            item_object.get("retrieval_score", item_object.get("score")),
                        )
                    )
                ),
                selected_for_generation=bool(item_object["selected_for_generation"]),
                chunk_text=str(chunk_record["text"]),
            )
        )

    return RequestCaptureRow(
        request_id=row["request_id"],
        trace_id=row["trace_id"],
        normalized_query=row["normalized_query"],
        reranker_kind=str(row["reranker_kind"]),
        reranker_config=ensure_json_object(row["reranker_config"])
        if row["reranker_config"] is not None
        else None,
        retrieval_items=tuple(retrieval_items),
    )


def _load_existing_chunk_ids(
    connection: Connection[Any],
    request_id: str,
    run_id: str,
) -> set[str]:
    rows = connection.execute(
        """
        select chunk_id
        from judge_retrieval_results
        where request_id = %s and run_id = %s and suite_name = %s
        """,
        (request_id, run_id, SUITE_NAME),
    ).fetchall()
    return {row["chunk_id"] for row in rows}


def _evaluate_retrieval_item(
    judge_client: OpenAI,
    params: JudgeRetrievalParams,
    request_capture: RequestCaptureRow,
    retrieval_item: RetrievalItem,
    suite_prompt: SuitePrompt,
    request_span_context: object,
) -> RetrievalJudgeResult:
    rendered_prompt = render_prompt_template(
        suite_prompt.prompt_template,
        {
            "normalized_query": request_capture.normalized_query,
            "chunk_text": retrieval_item.chunk_text,
        },
    )
    try:
        log_kv(
            LOGGER,
            "chunk_judge_started",
            stage=STAGE_NAME,
            run_id=params.run_id,
            request_id=request_capture.request_id,
            chunk_id=retrieval_item.chunk_id,
            retrieval_rank=retrieval_item.retrieval_rank,
            reranker_kind=request_capture.reranker_kind,
            rerank_score=str(retrieval_item.rerank_score),
        )
        with traced_operation(
            tracer_name="Execution.evals.judge_retrieval",
            span_name="eval.judge_retrieval.chunk",
            attributes={
                "run_id": params.run_id,
                "request_id": request_capture.request_id,
                "stage": STAGE_NAME,
                "suite_name": SUITE_NAME,
                "chunk_id": retrieval_item.chunk_id,
                "retrieval_rank": retrieval_item.retrieval_rank,
                "selected_for_generation": retrieval_item.selected_for_generation,
                "reranker_kind": request_capture.reranker_kind,
                "rerank_score": float(retrieval_item.rerank_score),
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
            "chunk_judge_finished",
            stage=STAGE_NAME,
            run_id=params.run_id,
            request_id=request_capture.request_id,
            chunk_id=retrieval_item.chunk_id,
            retrieval_rank=retrieval_item.retrieval_rank,
            rerank_score=str(retrieval_item.rerank_score),
        )
    except Exception as error:  # pragma: no cover
        raise JudgeTransportError(
            f"chunk_id={retrieval_item.chunk_id} transport failure: {error}"
        ) from error

    raw_response = _extract_raw_response(response)
    try:
        judge_llm_call = build_judge_llm_call_record(
            request_id=request_capture.request_id,
            run_id=params.run_id,
            trace_id=request_capture.trace_id,
            stage_name=STAGE_NAME,
            suite_name=SUITE_NAME,
            chunk_id=retrieval_item.chunk_id,
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
            suite_name=SUITE_NAME,
            chunk_id=retrieval_item.chunk_id,
            judge_prompt_version=suite_prompt.version,
            judge_settings=params.judge_settings,
            prompt_text=rendered_prompt,
            raw_response=raw_response,
            completion_text=_extract_message_content(raw_response),
        )
    label, score, explanation = _normalize_judge_response(raw_response)
    return RetrievalJudgeResult(
        request_id=request_capture.request_id,
        run_id=params.run_id,
        trace_id=request_capture.trace_id,
        chunk_id=retrieval_item.chunk_id,
        document_id=retrieval_item.document_id,
        retrieval_rank=retrieval_item.retrieval_rank,
        retrieval_score=retrieval_item.retrieval_score,
        selected_for_generation=retrieval_item.selected_for_generation,
        suite_name=SUITE_NAME,
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


def _normalize_judge_response(raw_response: dict[str, Any]) -> tuple[str, Decimal, str]:
    content = _extract_message_content(raw_response)
    try:
        parsed = parse_json_object_from_model_text(content)
    except ValueError as error:
        raise JudgeResponseParsingError(
            f"retrieval_relevance returned non-JSON content: {content!r}"
        ) from error

    label = str(parsed.get("label", "")).strip().lower()
    explanation = str(parsed.get("explanation", "")).strip()
    if not explanation:
        raise JudgeResponseParsingError("retrieval_relevance returned empty explanation")
    if label not in LABEL_TO_SCORE:
        raise JudgeResponseParsingError(
            f"retrieval_relevance returned unsupported label={label!r}"
        )
    return label, LABEL_TO_SCORE[label], explanation


def _extract_message_content(raw_response: dict[str, Any]) -> str:
    try:
        return extract_text_from_openai_compatible_response(raw_response)
    except ValueError as error:
        raise JudgeResponseParsingError(
            f"judge response missing message content; available top-level keys={sorted(raw_response.keys())}"
        ) from error


def _insert_retrieval_result(connection: Connection[Any], result: RetrievalJudgeResult) -> None:
    try:
        with connection.transaction():
            connection.execute(
                """
                insert into judge_retrieval_results (
                    request_id,
                    run_id,
                    trace_id,
                    chunk_id,
                    document_id,
                    retrieval_rank,
                    retrieval_score,
                    selected_for_generation,
                    suite_name,
                    judge_model,
                    judge_prompt_version,
                    score,
                    label,
                    explanation,
                    raw_response
                ) values (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
                on conflict (request_id, run_id, suite_name, chunk_id) do nothing
                """,
                (
                    result.request_id,
                    result.run_id,
                    result.trace_id,
                    result.chunk_id,
                    result.document_id,
                    result.retrieval_rank,
                    result.retrieval_score,
                    result.selected_for_generation,
                    result.suite_name,
                    result.judge_model,
                    result.judge_prompt_version,
                    result.score,
                    result.label,
                    result.explanation,
                    Json(result.raw_response),
                ),
            )
    except Exception as error:  # pragma: no cover
        raise DatabaseWriteError(str(error)) from error


def _insert_judge_llm_call(connection: Connection[Any], record: JudgeLlmCallRecord) -> None:
    try:
        with connection.transaction():
            insert_judge_llm_call(connection, record)
    except Exception as error:  # pragma: no cover
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
    except Exception as error:  # pragma: no cover
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
    except Exception as error:  # pragma: no cover
        raise StateTransitionWriteError(str(error)) from error
