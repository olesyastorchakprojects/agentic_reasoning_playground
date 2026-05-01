from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from decimal import Decimal
from typing import Any

from psycopg import Connection
from psycopg.rows import dict_row

from .logging_utils import get_logger, log_kv
from .observability import traced_operation


STAGE_NAME = "build_request_summary"
REQUIRED_GENERATION_SUITES = (
    "answer_completeness",
    "groundedness",
    "answer_relevance",
    "correct_refusal",
)
LOGGER = get_logger("evals.build_request_summary")


@dataclass(frozen=True)
class BuildRequestSummaryParams:
    postgres_url: str
    run_id: str
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
    source_received_at: datetime
    raw_query: str
    normalized_query: str
    input_token_count: int
    pipeline_config_version: str
    corpus_version: str
    retriever_version: str
    retriever_kind: str
    embedding_model: str
    reranker_kind: str
    prompt_template_id: str
    prompt_template_version: str
    generation_model: str
    top_k_requested: int
    final_answer: str
    prompt_tokens: int
    completion_tokens: int
    total_tokens: int
    retrieval_chunk_ids: tuple[str, ...]
    retrieval_stage_metrics: dict | None = None
    reranking_stage_metrics: dict | None = None


@dataclass(frozen=True)
class GenerationRow:
    suite_name: str
    judge_model: str
    judge_prompt_version: str
    score: Decimal | None
    label: str | None


@dataclass(frozen=True)
class RetrievalRow:
    chunk_id: str
    retrieval_rank: int
    selected_for_generation: bool
    judge_model: str
    judge_prompt_version: str
    score: Decimal | None
    label: str | None


@dataclass(frozen=True)
class RequestSummaryRow:
    request_id: str
    trace_id: str
    source_received_at: datetime
    raw_query: str
    normalized_query: str
    input_token_count: int
    pipeline_config_version: str
    corpus_version: str
    retriever_version: str
    retriever_kind: str
    embedding_model: str
    reranker_kind: str
    prompt_template_id: str
    prompt_template_version: str
    generation_model: str
    top_k_requested: int
    final_answer: str
    prompt_tokens: int
    completion_tokens: int
    total_tokens: int
    answer_completeness_score: Decimal | None
    answer_completeness_label: str | None
    groundedness_score: Decimal | None
    groundedness_label: str | None
    answer_relevance_score: Decimal | None
    answer_relevance_label: str | None
    correct_refusal_score: Decimal | None
    correct_refusal_label: str | None
    retrieval_relevance_mean: Decimal | None
    retrieval_relevance_selected_mean: Decimal | None
    retrieval_relevance_topk_mean: Decimal | None
    retrieval_relevance_weighted_topk: Decimal | None
    retrieval_relevance_relevant_count: int
    retrieval_relevance_selected_count: int
    retrieval_chunk_count: int
    judge_generation_model: str
    judge_generation_prompt_version: str
    judge_retrieval_model: str | None
    judge_retrieval_prompt_version: str | None
    retrieval_evaluated_k: int | None = None
    retrieval_recall_soft: Decimal | None = None
    retrieval_recall_strict: Decimal | None = None
    retrieval_rr_soft: Decimal | None = None
    retrieval_rr_strict: Decimal | None = None
    retrieval_ndcg: Decimal | None = None
    retrieval_first_relevant_rank_soft: int | None = None
    retrieval_first_relevant_rank_strict: int | None = None
    retrieval_num_relevant_soft: int | None = None
    retrieval_num_relevant_strict: int | None = None
    reranking_evaluated_k: int | None = None
    reranking_recall_soft: Decimal | None = None
    reranking_recall_strict: Decimal | None = None
    reranking_rr_soft: Decimal | None = None
    reranking_rr_strict: Decimal | None = None
    reranking_ndcg: Decimal | None = None
    reranking_first_relevant_rank_soft: int | None = None
    reranking_first_relevant_rank_strict: int | None = None
    reranking_num_relevant_soft: int | None = None
    reranking_num_relevant_strict: int | None = None
    retrieval_context_loss_soft: Decimal | None = None
    retrieval_context_loss_strict: Decimal | None = None


class BuildRequestSummaryStageError(RuntimeError):
    """Base stage error."""


class FIFOSelectionError(BuildRequestSummaryStageError):
    pass


class RequestCaptureLookupError(BuildRequestSummaryStageError):
    pass


class GenerationResultLookupError(BuildRequestSummaryStageError):
    pass


class RetrievalResultLookupError(BuildRequestSummaryStageError):
    pass


class UpstreamIncompletenessError(BuildRequestSummaryStageError):
    pass


class SummaryRowMappingError(BuildRequestSummaryStageError):
    pass


class DatabaseUpsertError(BuildRequestSummaryStageError):
    pass


class StateTransitionWriteError(BuildRequestSummaryStageError):
    pass


class InternalStageStateError(BuildRequestSummaryStageError):
    pass


def run_build_request_summary(params: BuildRequestSummaryParams) -> bool:
    if not params.run_scope_request_ids:
        log_kv(LOGGER, "no_run_scope_requests", stage=STAGE_NAME, run_id=params.run_id)
        return False

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
                tracer_name="Execution.evals.build_request_summary",
                span_name="eval.build_request_summary.request",
                attributes={
                    "run_id": params.run_id,
                    "request_id": state_row.request_id,
                    "stage": STAGE_NAME,
                    "status": state_row.status,
                    "attempt_count": state_row.attempt_count,
                },
            ):
                _mark_running(connection, state_row.request_id)

                try:
                    request_capture = _load_request_capture(connection, state_row.request_id)
                    generation_rows = _load_generation_rows(
                        connection,
                        request_capture.request_id,
                        params.run_id,
                    )
                    retrieval_rows = _load_retrieval_rows(
                        connection,
                        request_capture.request_id,
                        params.run_id,
                    )
                    log_kv(
                        LOGGER,
                        "upstream_rows_loaded",
                        stage=STAGE_NAME,
                        run_id=params.run_id,
                        request_id=request_capture.request_id,
                        generation_row_count=len(generation_rows),
                        retrieval_row_count=len(retrieval_rows),
                    )

                    _verify_readiness(request_capture, generation_rows, retrieval_rows)
                    summary_row = _build_summary_row(
                        request_capture,
                        generation_rows,
                        retrieval_rows,
                    )
                    _upsert_summary_row(connection, summary_row)
                    _upsert_run_summary_row(connection, params.run_id, summary_row)
                    _mark_completed(connection, state_row.request_id)
                    log_kv(
                        LOGGER,
                        "request_completed",
                        stage=STAGE_NAME,
                        run_id=params.run_id,
                        request_id=state_row.request_id,
                    )
                    return True
                except BuildRequestSummaryStageError as error:
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
    except BuildRequestSummaryStageError:
        raise
    except Exception as error:  # pragma: no cover - wrapper boundary
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


def _load_request_capture(connection: Connection[Any], request_id: str) -> RequestCaptureRow:
    try:
        row = connection.execute(
            """
            select
                request_id,
                trace_id,
                received_at,
                raw_query,
                normalized_query,
                input_token_count,
                pipeline_config_version,
                corpus_version,
                retriever_version,
                retriever_kind,
                embedding_model,
                reranker_kind,
                prompt_template_id,
                prompt_template_version,
                generation_model,
                top_k_requested,
                final_answer,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                retrieval_results,
                retrieval_stage_metrics,
                reranking_stage_metrics
            from request_captures
            where request_id = %s
            """,
            (request_id,),
        ).fetchone()
    except Exception as error:  # pragma: no cover
        raise RequestCaptureLookupError(str(error)) from error

    if row is None:
        raise RequestCaptureLookupError(f"request_captures row not found for request_id={request_id}")

    retrieval_results = row["retrieval_results"]
    if isinstance(retrieval_results, str):
        import json

        retrieval_results = json.loads(retrieval_results)
    retrieval_chunk_ids = tuple(str(item["chunk_id"]) for item in retrieval_results)

    retrieval_stage_metrics = row["retrieval_stage_metrics"]
    if isinstance(retrieval_stage_metrics, str):
        import json as _json
        retrieval_stage_metrics = _json.loads(retrieval_stage_metrics)

    reranking_stage_metrics = row["reranking_stage_metrics"]
    if isinstance(reranking_stage_metrics, str):
        import json as _json
        reranking_stage_metrics = _json.loads(reranking_stage_metrics)

    return RequestCaptureRow(
        request_id=row["request_id"],
        trace_id=row["trace_id"],
        source_received_at=row["received_at"],
        raw_query=row["raw_query"],
        normalized_query=row["normalized_query"],
        input_token_count=row["input_token_count"],
        pipeline_config_version=row["pipeline_config_version"],
        corpus_version=row["corpus_version"],
        retriever_version=row["retriever_version"],
        retriever_kind=row["retriever_kind"],
        embedding_model=row["embedding_model"],
        reranker_kind=row["reranker_kind"],
        prompt_template_id=row["prompt_template_id"],
        prompt_template_version=row["prompt_template_version"],
        generation_model=row["generation_model"],
        top_k_requested=row["top_k_requested"],
        final_answer=row["final_answer"],
        prompt_tokens=row["prompt_tokens"],
        completion_tokens=row["completion_tokens"],
        total_tokens=row["total_tokens"],
        retrieval_chunk_ids=retrieval_chunk_ids,
        retrieval_stage_metrics=retrieval_stage_metrics,
        reranking_stage_metrics=reranking_stage_metrics,
    )


def _load_generation_rows(
    connection: Connection[Any],
    request_id: str,
    run_id: str,
) -> tuple[GenerationRow, ...]:
    try:
        rows = connection.execute(
            """
            select
                suite_name::text as suite_name,
                judge_model,
                judge_prompt_version,
                score,
                label
            from judge_generation_results
            where request_id = %s and run_id = %s
            """,
            (request_id, run_id),
        ).fetchall()
    except Exception as error:  # pragma: no cover
        raise GenerationResultLookupError(str(error)) from error

    return tuple(
        GenerationRow(
            suite_name=row["suite_name"],
            judge_model=row["judge_model"],
            judge_prompt_version=row["judge_prompt_version"],
            score=row["score"],
            label=row["label"],
        )
        for row in rows
    )


def _load_retrieval_rows(
    connection: Connection[Any],
    request_id: str,
    run_id: str,
) -> tuple[RetrievalRow, ...]:
    try:
        rows = connection.execute(
            """
            select
                chunk_id,
                retrieval_rank,
                selected_for_generation,
                judge_model,
                judge_prompt_version,
                score,
                label
            from judge_retrieval_results
            where request_id = %s and run_id = %s and suite_name = 'retrieval_relevance'
            order by retrieval_rank asc, chunk_id asc
            """,
            (request_id, run_id),
        ).fetchall()
    except Exception as error:  # pragma: no cover
        raise RetrievalResultLookupError(str(error)) from error

    return tuple(
        RetrievalRow(
            chunk_id=row["chunk_id"],
            retrieval_rank=row["retrieval_rank"],
            selected_for_generation=row["selected_for_generation"],
            judge_model=row["judge_model"],
            judge_prompt_version=row["judge_prompt_version"],
            score=row["score"],
            label=row["label"],
        )
        for row in rows
    )


def _verify_readiness(
    request_capture: RequestCaptureRow,
    generation_rows: tuple[GenerationRow, ...],
    retrieval_rows: tuple[RetrievalRow, ...],
) -> None:
    generation_map = {row.suite_name: row for row in generation_rows}
    missing_generation = [
        suite_name
        for suite_name in REQUIRED_GENERATION_SUITES
        if suite_name not in generation_map
    ]
    if missing_generation:
        raise UpstreamIncompletenessError(
            f"missing generation suite rows: {', '.join(missing_generation)}"
        )

    expected_chunk_ids = set(request_capture.retrieval_chunk_ids)
    actual_chunk_ids = {row.chunk_id for row in retrieval_rows}
    if actual_chunk_ids != expected_chunk_ids:
        raise UpstreamIncompletenessError(
            "retrieval result set does not match expected request_captures.retrieval_results key set"
        )


def _build_summary_row(
    request_capture: RequestCaptureRow,
    generation_rows: tuple[GenerationRow, ...],
    retrieval_rows: tuple[RetrievalRow, ...],
) -> RequestSummaryRow:
    generation_map = {row.suite_name: row for row in generation_rows}
    scores = [row.score for row in retrieval_rows if row.score is not None]
    selected_scores = [
        row.score
        for row in retrieval_rows
        if row.selected_for_generation and row.score is not None
    ]
    weighted_topk = _weighted_topk(retrieval_rows)

    ret_m = _unpack_metrics(request_capture.retrieval_stage_metrics, "retrieval")
    rer_m = _unpack_metrics(request_capture.reranking_stage_metrics, "reranking")

    context_loss_soft = (
        ret_m["retrieval_recall_soft"] - rer_m["reranking_recall_soft"]
        if ret_m["retrieval_recall_soft"] is not None and rer_m["reranking_recall_soft"] is not None
        else None
    )
    context_loss_strict = (
        ret_m["retrieval_recall_strict"] - rer_m["reranking_recall_strict"]
        if ret_m["retrieval_recall_strict"] is not None and rer_m["reranking_recall_strict"] is not None
        else None
    )

    return RequestSummaryRow(
        request_id=request_capture.request_id,
        trace_id=request_capture.trace_id,
        source_received_at=request_capture.source_received_at,
        raw_query=request_capture.raw_query,
        normalized_query=request_capture.normalized_query,
        input_token_count=request_capture.input_token_count,
        pipeline_config_version=request_capture.pipeline_config_version,
        corpus_version=request_capture.corpus_version,
        retriever_version=request_capture.retriever_version,
        retriever_kind=request_capture.retriever_kind,
        embedding_model=request_capture.embedding_model,
        reranker_kind=request_capture.reranker_kind,
        prompt_template_id=request_capture.prompt_template_id,
        prompt_template_version=request_capture.prompt_template_version,
        generation_model=request_capture.generation_model,
        top_k_requested=request_capture.top_k_requested,
        final_answer=request_capture.final_answer,
        prompt_tokens=request_capture.prompt_tokens,
        completion_tokens=request_capture.completion_tokens,
        total_tokens=request_capture.total_tokens,
        answer_completeness_score=generation_map["answer_completeness"].score,
        answer_completeness_label=generation_map["answer_completeness"].label,
        groundedness_score=generation_map["groundedness"].score,
        groundedness_label=generation_map["groundedness"].label,
        answer_relevance_score=generation_map["answer_relevance"].score,
        answer_relevance_label=generation_map["answer_relevance"].label,
        correct_refusal_score=generation_map["correct_refusal"].score,
        correct_refusal_label=generation_map["correct_refusal"].label,
        retrieval_relevance_mean=_decimal_mean(scores),
        retrieval_relevance_selected_mean=_decimal_mean(selected_scores),
        retrieval_relevance_topk_mean=_decimal_mean(scores),
        retrieval_relevance_weighted_topk=weighted_topk,
        retrieval_relevance_relevant_count=sum(1 for row in retrieval_rows if row.label == "relevant"),
        retrieval_relevance_selected_count=sum(
            1 for row in retrieval_rows if row.selected_for_generation
        ),
        retrieval_chunk_count=len(retrieval_rows),
        judge_generation_model=generation_map["answer_completeness"].judge_model,
        judge_generation_prompt_version=generation_map["answer_completeness"].judge_prompt_version,
        judge_retrieval_model=retrieval_rows[0].judge_model if retrieval_rows else None,
        judge_retrieval_prompt_version=retrieval_rows[0].judge_prompt_version if retrieval_rows else None,
        retrieval_evaluated_k=ret_m["retrieval_evaluated_k"],
        retrieval_recall_soft=ret_m["retrieval_recall_soft"],
        retrieval_recall_strict=ret_m["retrieval_recall_strict"],
        retrieval_rr_soft=ret_m["retrieval_rr_soft"],
        retrieval_rr_strict=ret_m["retrieval_rr_strict"],
        retrieval_ndcg=ret_m["retrieval_ndcg"],
        retrieval_first_relevant_rank_soft=ret_m["retrieval_first_relevant_rank_soft"],
        retrieval_first_relevant_rank_strict=ret_m["retrieval_first_relevant_rank_strict"],
        retrieval_num_relevant_soft=ret_m["retrieval_num_relevant_soft"],
        retrieval_num_relevant_strict=ret_m["retrieval_num_relevant_strict"],
        reranking_evaluated_k=rer_m["reranking_evaluated_k"],
        reranking_recall_soft=rer_m["reranking_recall_soft"],
        reranking_recall_strict=rer_m["reranking_recall_strict"],
        reranking_rr_soft=rer_m["reranking_rr_soft"],
        reranking_rr_strict=rer_m["reranking_rr_strict"],
        reranking_ndcg=rer_m["reranking_ndcg"],
        reranking_first_relevant_rank_soft=rer_m["reranking_first_relevant_rank_soft"],
        reranking_first_relevant_rank_strict=rer_m["reranking_first_relevant_rank_strict"],
        reranking_num_relevant_soft=rer_m["reranking_num_relevant_soft"],
        reranking_num_relevant_strict=rer_m["reranking_num_relevant_strict"],
        retrieval_context_loss_soft=context_loss_soft,
        retrieval_context_loss_strict=context_loss_strict,
    )


def _unpack_metrics(metrics: dict | None, prefix: str) -> dict:
    """Convert a raw retrieval quality metrics dict (from jsonb) to a flat dict of typed fields."""
    keys = [
        "evaluated_k", "recall_soft", "recall_strict", "rr_soft", "rr_strict", "ndcg",
        "first_relevant_rank_soft", "first_relevant_rank_strict",
        "num_relevant_soft", "num_relevant_strict",
    ]
    int_keys = {"evaluated_k", "first_relevant_rank_soft", "first_relevant_rank_strict",
                "num_relevant_soft", "num_relevant_strict"}
    if metrics is None:
        return {f"{prefix}_{k}": None for k in keys}
    result = {}
    for k in keys:
        v = metrics.get(k)
        if v is None:
            result[f"{prefix}_{k}"] = None
        elif k in int_keys:
            result[f"{prefix}_{k}"] = int(v)
        else:
            result[f"{prefix}_{k}"] = Decimal(str(v))
    return result


def _decimal_mean(values: list[Decimal | None]) -> Decimal | None:
    concrete = [value for value in values if value is not None]
    if not concrete:
        return None
    return sum(concrete) / Decimal(len(concrete))


def _weighted_topk(retrieval_rows: tuple[RetrievalRow, ...]) -> Decimal | None:
    weighted_sum = Decimal("0")
    total_weight = Decimal("0")
    for row in retrieval_rows:
        if row.score is None:
            continue
        weight = Decimal("1") / Decimal(row.retrieval_rank)
        weighted_sum += row.score * weight
        total_weight += weight
    if total_weight == 0:
        return None
    return weighted_sum / total_weight


def _upsert_summary_row(connection: Connection[Any], summary_row: RequestSummaryRow) -> None:
    try:
        with connection.transaction():
            connection.execute(
                """
                insert into request_summaries (
                    request_id,
                    trace_id,
                    source_received_at,
                    raw_query,
                    normalized_query,
                    input_token_count,
                    pipeline_config_version,
                    corpus_version,
                    retriever_version,
                    retriever_kind,
                    embedding_model,
                    reranker_kind,
                    prompt_template_id,
                    prompt_template_version,
                    generation_model,
                    top_k_requested,
                    final_answer,
                    prompt_tokens,
                    completion_tokens,
                    total_tokens,
                    answer_completeness_score,
                    answer_completeness_label,
                    groundedness_score,
                    groundedness_label,
                    answer_relevance_score,
                    answer_relevance_label,
                    correct_refusal_score,
                    correct_refusal_label,
                    retrieval_relevance_mean,
                    retrieval_relevance_selected_mean,
                    retrieval_relevance_topk_mean,
                    retrieval_relevance_weighted_topk,
                    retrieval_relevance_relevant_count,
                    retrieval_relevance_selected_count,
                    retrieval_chunk_count,
                    retrieval_evaluated_k,
                    retrieval_recall_soft,
                    retrieval_recall_strict,
                    retrieval_rr_soft,
                    retrieval_rr_strict,
                    retrieval_ndcg,
                    retrieval_first_relevant_rank_soft,
                    retrieval_first_relevant_rank_strict,
                    retrieval_num_relevant_soft,
                    retrieval_num_relevant_strict,
                    reranking_evaluated_k,
                    reranking_recall_soft,
                    reranking_recall_strict,
                    reranking_rr_soft,
                    reranking_rr_strict,
                    reranking_ndcg,
                    reranking_first_relevant_rank_soft,
                    reranking_first_relevant_rank_strict,
                    reranking_num_relevant_soft,
                    reranking_num_relevant_strict,
                    retrieval_context_loss_soft,
                    retrieval_context_loss_strict,
                    summarized_at
                ) values (
                    %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s,
                    %s, %s, %s, %s, %s, %s, %s, %s,
                    %s, %s, %s, %s, %s, %s, %s, %s,
                    %s, %s, %s, %s, %s, %s, %s, %s, %s, %s,
                    %s, %s, %s, %s, %s, %s, %s, %s, %s, %s,
                    %s, %s, %s,
                    now()
                )
                on conflict (request_id) do update set
                    trace_id = excluded.trace_id,
                    source_received_at = excluded.source_received_at,
                    raw_query = excluded.raw_query,
                    normalized_query = excluded.normalized_query,
                    input_token_count = excluded.input_token_count,
                    pipeline_config_version = excluded.pipeline_config_version,
                    corpus_version = excluded.corpus_version,
                    retriever_version = excluded.retriever_version,
                    retriever_kind = excluded.retriever_kind,
                    embedding_model = excluded.embedding_model,
                    reranker_kind = excluded.reranker_kind,
                    prompt_template_id = excluded.prompt_template_id,
                    prompt_template_version = excluded.prompt_template_version,
                    generation_model = excluded.generation_model,
                    top_k_requested = excluded.top_k_requested,
                    final_answer = excluded.final_answer,
                    prompt_tokens = excluded.prompt_tokens,
                    completion_tokens = excluded.completion_tokens,
                    total_tokens = excluded.total_tokens,
                    answer_completeness_score = excluded.answer_completeness_score,
                    answer_completeness_label = excluded.answer_completeness_label,
                    groundedness_score = excluded.groundedness_score,
                    groundedness_label = excluded.groundedness_label,
                    answer_relevance_score = excluded.answer_relevance_score,
                    answer_relevance_label = excluded.answer_relevance_label,
                    correct_refusal_score = excluded.correct_refusal_score,
                    correct_refusal_label = excluded.correct_refusal_label,
                    retrieval_relevance_mean = excluded.retrieval_relevance_mean,
                    retrieval_relevance_selected_mean = excluded.retrieval_relevance_selected_mean,
                    retrieval_relevance_topk_mean = excluded.retrieval_relevance_topk_mean,
                    retrieval_relevance_weighted_topk = excluded.retrieval_relevance_weighted_topk,
                    retrieval_relevance_relevant_count = excluded.retrieval_relevance_relevant_count,
                    retrieval_relevance_selected_count = excluded.retrieval_relevance_selected_count,
                    retrieval_chunk_count = excluded.retrieval_chunk_count,
                    retrieval_evaluated_k = excluded.retrieval_evaluated_k,
                    retrieval_recall_soft = excluded.retrieval_recall_soft,
                    retrieval_recall_strict = excluded.retrieval_recall_strict,
                    retrieval_rr_soft = excluded.retrieval_rr_soft,
                    retrieval_rr_strict = excluded.retrieval_rr_strict,
                    retrieval_ndcg = excluded.retrieval_ndcg,
                    retrieval_first_relevant_rank_soft = excluded.retrieval_first_relevant_rank_soft,
                    retrieval_first_relevant_rank_strict = excluded.retrieval_first_relevant_rank_strict,
                    retrieval_num_relevant_soft = excluded.retrieval_num_relevant_soft,
                    retrieval_num_relevant_strict = excluded.retrieval_num_relevant_strict,
                    reranking_evaluated_k = excluded.reranking_evaluated_k,
                    reranking_recall_soft = excluded.reranking_recall_soft,
                    reranking_recall_strict = excluded.reranking_recall_strict,
                    reranking_rr_soft = excluded.reranking_rr_soft,
                    reranking_rr_strict = excluded.reranking_rr_strict,
                    reranking_ndcg = excluded.reranking_ndcg,
                    reranking_first_relevant_rank_soft = excluded.reranking_first_relevant_rank_soft,
                    reranking_first_relevant_rank_strict = excluded.reranking_first_relevant_rank_strict,
                    reranking_num_relevant_soft = excluded.reranking_num_relevant_soft,
                    reranking_num_relevant_strict = excluded.reranking_num_relevant_strict,
                    retrieval_context_loss_soft = excluded.retrieval_context_loss_soft,
                    retrieval_context_loss_strict = excluded.retrieval_context_loss_strict,
                    summarized_at = now()
                """,
                (
                    summary_row.request_id,
                    summary_row.trace_id,
                    summary_row.source_received_at,
                    summary_row.raw_query,
                    summary_row.normalized_query,
                    summary_row.input_token_count,
                    summary_row.pipeline_config_version,
                    summary_row.corpus_version,
                    summary_row.retriever_version,
                    summary_row.retriever_kind,
                    summary_row.embedding_model,
                    summary_row.reranker_kind,
                    summary_row.prompt_template_id,
                    summary_row.prompt_template_version,
                    summary_row.generation_model,
                    summary_row.top_k_requested,
                    summary_row.final_answer,
                    summary_row.prompt_tokens,
                    summary_row.completion_tokens,
                    summary_row.total_tokens,
                    summary_row.answer_completeness_score,
                    summary_row.answer_completeness_label,
                    summary_row.groundedness_score,
                    summary_row.groundedness_label,
                    summary_row.answer_relevance_score,
                    summary_row.answer_relevance_label,
                    summary_row.correct_refusal_score,
                    summary_row.correct_refusal_label,
                    summary_row.retrieval_relevance_mean,
                    summary_row.retrieval_relevance_selected_mean,
                    summary_row.retrieval_relevance_topk_mean,
                    summary_row.retrieval_relevance_weighted_topk,
                    summary_row.retrieval_relevance_relevant_count,
                    summary_row.retrieval_relevance_selected_count,
                    summary_row.retrieval_chunk_count,
                    summary_row.retrieval_evaluated_k,
                    summary_row.retrieval_recall_soft,
                    summary_row.retrieval_recall_strict,
                    summary_row.retrieval_rr_soft,
                    summary_row.retrieval_rr_strict,
                    summary_row.retrieval_ndcg,
                    summary_row.retrieval_first_relevant_rank_soft,
                    summary_row.retrieval_first_relevant_rank_strict,
                    summary_row.retrieval_num_relevant_soft,
                    summary_row.retrieval_num_relevant_strict,
                    summary_row.reranking_evaluated_k,
                    summary_row.reranking_recall_soft,
                    summary_row.reranking_recall_strict,
                    summary_row.reranking_rr_soft,
                    summary_row.reranking_rr_strict,
                    summary_row.reranking_ndcg,
                    summary_row.reranking_first_relevant_rank_soft,
                    summary_row.reranking_first_relevant_rank_strict,
                    summary_row.reranking_num_relevant_soft,
                    summary_row.reranking_num_relevant_strict,
                    summary_row.retrieval_context_loss_soft,
                    summary_row.retrieval_context_loss_strict,
                ),
            )
    except Exception as error:  # pragma: no cover
        raise DatabaseUpsertError(str(error)) from error


def _upsert_run_summary_row(
    connection: Connection[Any],
    run_id: str,
    summary_row: RequestSummaryRow,
) -> None:
    try:
        with connection.transaction():
            connection.execute(
                """
                insert into request_run_summaries (
                    request_id,
                    run_id,
                    trace_id,
                    source_received_at,
                    raw_query,
                    normalized_query,
                    input_token_count,
                    pipeline_config_version,
                    corpus_version,
                    retriever_version,
                    retriever_kind,
                    embedding_model,
                    prompt_template_id,
                    prompt_template_version,
                    generation_model,
                    top_k_requested,
                    final_answer,
                    prompt_tokens,
                    completion_tokens,
                    total_tokens,
                    answer_completeness_score,
                    answer_completeness_label,
                    groundedness_score,
                    groundedness_label,
                    answer_relevance_score,
                    answer_relevance_label,
                    correct_refusal_score,
                    correct_refusal_label,
                    retrieval_relevance_mean,
                    retrieval_relevance_selected_mean,
                    retrieval_relevance_topk_mean,
                    retrieval_relevance_weighted_topk,
                    retrieval_relevance_relevant_count,
                    retrieval_relevance_selected_count,
                    retrieval_chunk_count,
                    retrieval_evaluated_k,
                    retrieval_recall_soft,
                    retrieval_recall_strict,
                    retrieval_rr_soft,
                    retrieval_rr_strict,
                    retrieval_ndcg,
                    retrieval_first_relevant_rank_soft,
                    retrieval_first_relevant_rank_strict,
                    retrieval_num_relevant_soft,
                    retrieval_num_relevant_strict,
                    reranking_evaluated_k,
                    reranking_recall_soft,
                    reranking_recall_strict,
                    reranking_rr_soft,
                    reranking_rr_strict,
                    reranking_ndcg,
                    reranking_first_relevant_rank_soft,
                    reranking_first_relevant_rank_strict,
                    reranking_num_relevant_soft,
                    reranking_num_relevant_strict,
                    retrieval_context_loss_soft,
                    retrieval_context_loss_strict,
                    judge_generation_model,
                    judge_generation_prompt_version,
                    judge_retrieval_model,
                    judge_retrieval_prompt_version,
                    summarized_at
                ) values (
                    %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s,
                    %s, %s, %s, %s, %s, %s, %s, %s,
                    %s, %s, %s, %s, %s, %s, %s,
                    %s, %s, %s, %s, %s, %s, %s, %s, %s, %s,
                    %s, %s, %s, %s, %s, %s, %s, %s, %s, %s,
                    %s, %s, %s, %s,
                    %s, %s, %s, %s,
                    now()
                )
                on conflict (request_id, run_id) do update set
                    trace_id = excluded.trace_id,
                    source_received_at = excluded.source_received_at,
                    raw_query = excluded.raw_query,
                    normalized_query = excluded.normalized_query,
                    input_token_count = excluded.input_token_count,
                    pipeline_config_version = excluded.pipeline_config_version,
                    corpus_version = excluded.corpus_version,
                    retriever_version = excluded.retriever_version,
                    retriever_kind = excluded.retriever_kind,
                    embedding_model = excluded.embedding_model,
                    prompt_template_id = excluded.prompt_template_id,
                    prompt_template_version = excluded.prompt_template_version,
                    generation_model = excluded.generation_model,
                    top_k_requested = excluded.top_k_requested,
                    final_answer = excluded.final_answer,
                    prompt_tokens = excluded.prompt_tokens,
                    completion_tokens = excluded.completion_tokens,
                    total_tokens = excluded.total_tokens,
                    answer_completeness_score = excluded.answer_completeness_score,
                    answer_completeness_label = excluded.answer_completeness_label,
                    groundedness_score = excluded.groundedness_score,
                    groundedness_label = excluded.groundedness_label,
                    answer_relevance_score = excluded.answer_relevance_score,
                    answer_relevance_label = excluded.answer_relevance_label,
                    correct_refusal_score = excluded.correct_refusal_score,
                    correct_refusal_label = excluded.correct_refusal_label,
                    retrieval_relevance_mean = excluded.retrieval_relevance_mean,
                    retrieval_relevance_selected_mean = excluded.retrieval_relevance_selected_mean,
                    retrieval_relevance_topk_mean = excluded.retrieval_relevance_topk_mean,
                    retrieval_relevance_weighted_topk = excluded.retrieval_relevance_weighted_topk,
                    retrieval_relevance_relevant_count = excluded.retrieval_relevance_relevant_count,
                    retrieval_relevance_selected_count = excluded.retrieval_relevance_selected_count,
                    retrieval_chunk_count = excluded.retrieval_chunk_count,
                    retrieval_evaluated_k = excluded.retrieval_evaluated_k,
                    retrieval_recall_soft = excluded.retrieval_recall_soft,
                    retrieval_recall_strict = excluded.retrieval_recall_strict,
                    retrieval_rr_soft = excluded.retrieval_rr_soft,
                    retrieval_rr_strict = excluded.retrieval_rr_strict,
                    retrieval_ndcg = excluded.retrieval_ndcg,
                    retrieval_first_relevant_rank_soft = excluded.retrieval_first_relevant_rank_soft,
                    retrieval_first_relevant_rank_strict = excluded.retrieval_first_relevant_rank_strict,
                    retrieval_num_relevant_soft = excluded.retrieval_num_relevant_soft,
                    retrieval_num_relevant_strict = excluded.retrieval_num_relevant_strict,
                    reranking_evaluated_k = excluded.reranking_evaluated_k,
                    reranking_recall_soft = excluded.reranking_recall_soft,
                    reranking_recall_strict = excluded.reranking_recall_strict,
                    reranking_rr_soft = excluded.reranking_rr_soft,
                    reranking_rr_strict = excluded.reranking_rr_strict,
                    reranking_ndcg = excluded.reranking_ndcg,
                    reranking_first_relevant_rank_soft = excluded.reranking_first_relevant_rank_soft,
                    reranking_first_relevant_rank_strict = excluded.reranking_first_relevant_rank_strict,
                    reranking_num_relevant_soft = excluded.reranking_num_relevant_soft,
                    reranking_num_relevant_strict = excluded.reranking_num_relevant_strict,
                    retrieval_context_loss_soft = excluded.retrieval_context_loss_soft,
                    retrieval_context_loss_strict = excluded.retrieval_context_loss_strict,
                    judge_generation_model = excluded.judge_generation_model,
                    judge_generation_prompt_version = excluded.judge_generation_prompt_version,
                    judge_retrieval_model = excluded.judge_retrieval_model,
                    judge_retrieval_prompt_version = excluded.judge_retrieval_prompt_version,
                    summarized_at = now()
                """,
                (
                    summary_row.request_id,
                    run_id,
                    summary_row.trace_id,
                    summary_row.source_received_at,
                    summary_row.raw_query,
                    summary_row.normalized_query,
                    summary_row.input_token_count,
                    summary_row.pipeline_config_version,
                    summary_row.corpus_version,
                    summary_row.retriever_version,
                    summary_row.retriever_kind,
                    summary_row.embedding_model,
                    summary_row.prompt_template_id,
                    summary_row.prompt_template_version,
                    summary_row.generation_model,
                    summary_row.top_k_requested,
                    summary_row.final_answer,
                    summary_row.prompt_tokens,
                    summary_row.completion_tokens,
                    summary_row.total_tokens,
                    summary_row.answer_completeness_score,
                    summary_row.answer_completeness_label,
                    summary_row.groundedness_score,
                    summary_row.groundedness_label,
                    summary_row.answer_relevance_score,
                    summary_row.answer_relevance_label,
                    summary_row.correct_refusal_score,
                    summary_row.correct_refusal_label,
                    summary_row.retrieval_relevance_mean,
                    summary_row.retrieval_relevance_selected_mean,
                    summary_row.retrieval_relevance_topk_mean,
                    summary_row.retrieval_relevance_weighted_topk,
                    summary_row.retrieval_relevance_relevant_count,
                    summary_row.retrieval_relevance_selected_count,
                    summary_row.retrieval_chunk_count,
                    summary_row.retrieval_evaluated_k,
                    summary_row.retrieval_recall_soft,
                    summary_row.retrieval_recall_strict,
                    summary_row.retrieval_rr_soft,
                    summary_row.retrieval_rr_strict,
                    summary_row.retrieval_ndcg,
                    summary_row.retrieval_first_relevant_rank_soft,
                    summary_row.retrieval_first_relevant_rank_strict,
                    summary_row.retrieval_num_relevant_soft,
                    summary_row.retrieval_num_relevant_strict,
                    summary_row.reranking_evaluated_k,
                    summary_row.reranking_recall_soft,
                    summary_row.reranking_recall_strict,
                    summary_row.reranking_rr_soft,
                    summary_row.reranking_rr_strict,
                    summary_row.reranking_ndcg,
                    summary_row.reranking_first_relevant_rank_soft,
                    summary_row.reranking_first_relevant_rank_strict,
                    summary_row.reranking_num_relevant_soft,
                    summary_row.reranking_num_relevant_strict,
                    summary_row.retrieval_context_loss_soft,
                    summary_row.retrieval_context_loss_strict,
                    summary_row.judge_generation_model,
                    summary_row.judge_generation_prompt_version,
                    summary_row.judge_retrieval_model,
                    summary_row.judge_retrieval_prompt_version,
                ),
            )
    except Exception as error:  # pragma: no cover
        raise DatabaseUpsertError(str(error)) from error


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
