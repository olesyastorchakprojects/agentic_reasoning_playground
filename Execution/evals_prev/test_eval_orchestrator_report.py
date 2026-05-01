from __future__ import annotations

from decimal import Decimal
from typing import Any
from unittest.mock import MagicMock

import pytest

from .eval_orchestrator import (
    _CONDITIONAL_AGGREGATE_KEYS,
    _build_conditional_retrieval_generation_section,
    _build_retrieval_quality_section,
    _build_run_report,
)
from .judge_transport import JudgeSettings


# ---------------------------------------------------------------------------
# Shared helpers
# ---------------------------------------------------------------------------

class _FakeCursor:
    def __init__(self, rows: list[Any]) -> None:
        self._rows = rows

    def fetchall(self) -> list[Any]:
        return list(self._rows)

    def fetchone(self) -> Any:
        return self._rows[0] if self._rows else None


def _fake_conn(*batches: list[Any]) -> Any:
    """Return a mock connection whose execute() yields each batch in order."""
    conn = MagicMock()
    responses = iter(_FakeCursor(b) for b in batches)
    conn.execute = MagicMock(side_effect=lambda *a, **kw: next(responses))
    return conn


def _minimal_manifest() -> dict[str, Any]:
    return {
        "run_id": "test-run-id",
        "run_type": "nightly",
        "status": "completed",
        "started_at": "2024-01-01T00:00:00+00:00",
        "request_count": 0,
        "judge_provider": "ollama",
        "judge_model": "test-model",
        "generation_suite_versions": {},
        "retrieval_suite_versions": {},
        "run_scope_request_ids": [],
    }


def _full_retrieval_quality_row() -> dict[str, Any]:
    return {
        "retrieval_evaluated_k": 12,
        "reranking_evaluated_k": 4,
        "retrieval_recall_soft": Decimal("0.5100"),
        "retrieval_recall_strict": Decimal("0.7533"),
        "retrieval_rr_soft": Decimal("1.0000"),
        "retrieval_rr_strict": Decimal("1.0000"),
        "retrieval_ndcg": Decimal("0.6605"),
        "reranking_recall_soft": Decimal("0.2733"),
        "reranking_recall_strict": Decimal("0.5933"),
        "reranking_rr_soft": Decimal("1.0000"),
        "reranking_rr_strict": Decimal("1.0000"),
        "reranking_ndcg": Decimal("0.7875"),
        "retrieval_context_loss_soft": Decimal("0.2367"),
        "retrieval_context_loss_strict": Decimal("0.1600"),
        "retrieval_num_relevant_soft": Decimal("3.5"),
        "retrieval_num_relevant_strict": Decimal("2.1"),
        "reranking_num_relevant_soft": Decimal("1.8"),
        "reranking_num_relevant_strict": Decimal("1.2"),
    }


def _all_null_retrieval_quality_row() -> dict[str, Any]:
    return {
        "retrieval_evaluated_k": None,
        "reranking_evaluated_k": None,
        "retrieval_recall_soft": None,
        "retrieval_recall_strict": None,
        "retrieval_rr_soft": None,
        "retrieval_rr_strict": None,
        "retrieval_ndcg": None,
        "reranking_recall_soft": None,
        "reranking_recall_strict": None,
        "reranking_rr_soft": None,
        "reranking_rr_strict": None,
        "reranking_ndcg": None,
        "retrieval_context_loss_soft": None,
        "retrieval_context_loss_strict": None,
        "retrieval_num_relevant_soft": None,
        "retrieval_num_relevant_strict": None,
        "reranking_num_relevant_soft": None,
        "reranking_num_relevant_strict": None,
    }


def _retrieval_depth_row() -> dict[str, Any]:
    return {
        "avg_retrieval_chunk_count": Decimal("7.6"),
        "min_retrieval_chunk_count": 5,
        "max_retrieval_chunk_count": 12,
    }


def _minimal_judge_settings() -> JudgeSettings:
    return JudgeSettings(
        provider="ollama",
        model_name="test-judge-model",
        tokenizer_source="Qwen/Qwen2.5-1.5B-Instruct",
        base_url="http://judge:11434/v1",
        api_key="unused",
        timeout_sec=30,
        input_cost_per_million_tokens=0.05,
        output_cost_per_million_tokens=0.20,
    )


def _pipeline_configs_row() -> dict[str, Any]:
    return {
        "runtime_run_id": "runtime-run-id-1",
        "retriever_kind": "Dense",
        "retriever_config": {
            "kind": "Dense",
            "embedding_model_name": "test-embedding-model",
            "qdrant_collection_name": "test-collection",
            "corpus_version": "v1",
            "chunking_strategy": "structural",
        },
        "reranker_kind": "PassThrough",
        "reranker_config": None,
        "generation_config": {
            "model": "test-gen-model",
            "model_endpoint": "http://ollama:11434",
            "temperature": 0.0,
            "max_context_chunks": 4,
        },
        "top_k_requested": 12,
    }


def _hybrid_pipeline_configs_row(strategy_kind: str) -> dict[str, Any]:
    return {
        "runtime_run_id": "runtime-run-id-1",
        "retriever_kind": "Hybrid",
        "retriever_config": {
            "kind": "Hybrid",
            "embedding_model_name": "test-embedding-model",
            "qdrant_collection_name": "test-hybrid-collection",
            "corpus_version": "v1",
            "chunking_strategy": "structural",
            "strategy": {
                "kind": strategy_kind,
                "version": "v1",
                "query_weighting": "binary_presence",
            },
        },
        "reranker_kind": "PassThrough",
        "reranker_config": None,
        "generation_config": {
            "model": "test-gen-model",
            "model_endpoint": "http://ollama:11434",
            "temperature": 0.0,
            "max_context_chunks": 4,
        },
        "top_k_requested": 12,
    }


def _runtime_token_usage_row() -> dict[str, Any]:
    return {
        "requests": 2,
        "prompt_tokens": 4000,
        "completion_tokens": 1000,
        "total_tokens": 5000,
        "prompt_cost_usd": Decimal("0.00020000"),
        "completion_cost_usd": Decimal("0.00020000"),
        "total_cost_usd": Decimal("0.00040000"),
    }


def _judge_token_usage_rows() -> list[dict[str, Any]]:
    return [
        {
            "stage_name": "judge_generation",
            "eval_calls": 8,
            "prompt_tokens": 8000,
            "completion_tokens": 800,
            "total_tokens": 8800,
            "prompt_cost_usd": Decimal("0.00040000"),
            "completion_cost_usd": Decimal("0.00016000"),
            "total_cost_usd": Decimal("0.00056000"),
        },
        {
            "stage_name": "judge_retrieval",
            "eval_calls": 24,
            "prompt_tokens": 24000,
            "completion_tokens": 2400,
            "total_tokens": 26400,
            "prompt_cost_usd": Decimal("0.00120000"),
            "completion_cost_usd": Decimal("0.00048000"),
            "total_cost_usd": Decimal("0.00168000"),
        },
        {
            "stage_name": "judge_total",
            "eval_calls": 32,
            "prompt_tokens": 32000,
            "completion_tokens": 3200,
            "total_tokens": 35200,
            "prompt_cost_usd": Decimal("0.00160000"),
            "completion_cost_usd": Decimal("0.00064000"),
            "total_cost_usd": Decimal("0.00224000"),
        },
    ]


def _all_null_conditional_row() -> dict[str, Any]:
    row: dict[str, Any] = {key: None for key in _CONDITIONAL_AGGREGATE_KEYS}
    row["retrieval_evaluated_k"] = 12
    row["reranking_evaluated_k"] = 4
    return row


def _full_conditional_row(rk: int = 12, nk: int = 4) -> dict[str, Any]:
    row: dict[str, Any] = {key: Decimal("0.5000") for key in _CONDITIONAL_AGGREGATE_KEYS}
    row["retrieval_evaluated_k"] = rk
    row["reranking_evaluated_k"] = nk
    return row


# ---------------------------------------------------------------------------
# §3 — _build_retrieval_quality_section
# ---------------------------------------------------------------------------

class TestBuildRetrievalQualitySection:

    def test_3_1_full_data_table_and_scalars(self) -> None:
        row = _full_retrieval_quality_row()
        result = _build_retrieval_quality_section(row)
        output = "\n".join(result)

        assert result
        assert "set" in output and "Recall soft" in output
        assert "| retrieval@12" in output
        assert "0.5100" in output
        assert "0.7533" in output
        assert "0.6605" in output
        assert "| generation_context@4" in output
        assert "0.2733" in output
        assert "0.5933" in output
        assert "0.7875" in output
        assert "retrieval_context_loss_soft: 0.2367" in output
        assert "retrieval_context_loss_strict: 0.1600" in output
        assert "avg_num_relevant_in_retrieval@12_soft: 3.5000" in output
        assert "avg_num_relevant_in_retrieval@12_strict: 2.1000" in output
        assert "avg_num_relevant_in_generation_context@4_soft: 1.8000" in output
        assert "avg_num_relevant_in_generation_context@4_strict: 1.2000" in output

    def test_3_2_all_null_section_omitted(self) -> None:
        result = _build_retrieval_quality_section(_all_null_retrieval_quality_row())
        assert result == []
        assert "Retrieval Quality" not in "\n".join(result)

    def test_3_3_k_values_in_labels(self) -> None:
        row = _full_retrieval_quality_row()
        row["retrieval_evaluated_k"] = 20
        row["reranking_evaluated_k"] = 5
        result = _build_retrieval_quality_section(row)
        output = "\n".join(result)

        assert "| retrieval@20" in output
        assert "| generation_context@5" in output
        assert "avg_num_relevant_in_retrieval@20_soft" in output
        assert "avg_num_relevant_in_retrieval@20_strict" in output
        assert "avg_num_relevant_in_generation_context@5_soft" in output
        assert "avg_num_relevant_in_generation_context@5_strict" in output

    def test_3_4_negative_context_loss(self) -> None:
        row = _full_retrieval_quality_row()
        row["retrieval_context_loss_soft"] = Decimal("-0.0500")
        result = _build_retrieval_quality_section(row)
        output = "\n".join(result)

        assert "retrieval_context_loss_soft: -0.0500" in output
        assert result

    def test_3_5_partial_nulls_section_not_omitted(self) -> None:
        row = _full_retrieval_quality_row()
        row["reranking_ndcg"] = None
        result = _build_retrieval_quality_section(row)
        output = "\n".join(result)

        assert result
        context_rows = [line for line in result if line.startswith("| generation_context@4")]
        assert len(context_rows) == 1
        assert "n/a" in context_rows[0]
        assert "0.2733" in output


# ---------------------------------------------------------------------------
# §4 — _build_conditional_retrieval_generation_section
# ---------------------------------------------------------------------------

class TestBuildConditionalSection:

    def test_4_1_worked_example_exact_values(self) -> None:
        # Input dict (pre-aggregated; dict keys are fixed SQL aliases, not k-embedded names).
        # The reranking_soft condition corresponds to generation_context@4_soft when reranking_evaluated_k=4.
        row: dict[str, Any] = {
            "groundedness_retrieval_soft": Decimal("0.8000"),
            "answer_completeness_retrieval_soft": Decimal("0.9000"),
            "answer_relevance_retrieval_soft": Decimal("0.9000"),
            "hallucination_retrieval_soft": Decimal("0.5000"),
            "success_retrieval_soft": Decimal("0.6000"),
            "groundedness_retrieval_strict": Decimal("0.8000"),
            "answer_completeness_retrieval_strict": Decimal("0.9000"),
            "answer_relevance_retrieval_strict": Decimal("0.9000"),
            "hallucination_retrieval_strict": Decimal("0.5000"),
            "success_retrieval_strict": Decimal("0.6000"),
            # Verified values for the reranking_soft condition (rendered as generation_context@4_soft):
            "groundedness_reranking_soft": Decimal("0.7500"),
            "answer_completeness_reranking_soft": Decimal("1.0000"),
            "answer_relevance_reranking_soft": Decimal("1.0000"),
            "hallucination_reranking_soft": Decimal("1.0000"),
            "success_reranking_soft": Decimal("0.5000"),
            "groundedness_reranking_strict": Decimal("0.8000"),
            "answer_completeness_reranking_strict": Decimal("0.9000"),
            "answer_relevance_reranking_strict": Decimal("0.9000"),
            "hallucination_reranking_strict": Decimal("0.5000"),
            "success_reranking_strict": Decimal("0.6000"),
            "retrieval_evaluated_k": 12,
            "reranking_evaluated_k": 4,
        }
        result = _build_conditional_retrieval_generation_section(row)
        output = "\n".join(result)

        assert result
        assert "Conditional Retrieval" in output
        # Rendered table headers use k-substituted names
        assert "retrieval@12_soft" in output
        assert "retrieval@12_strict" in output
        assert "generation_context@4_soft" in output
        assert "generation_context@4_strict" in output

        def _cells(label: str) -> list[str]:
            matching = [line for line in result if label in line]
            assert len(matching) == 1, f"expected 1 row for {label!r}, got {len(matching)}"
            return [c.strip() for c in matching[0].split("|") if c.strip()]

        # Column order: metric | retrieval_soft | retrieval_strict | reranking_soft | reranking_strict
        # reranking_soft = generation_context@4_soft is at index 3 (0-based, after metric at 0)
        assert _cells("groundedness_given_relevant_context")[3] == "0.7500"
        assert _cells("answer_completeness_given_relevant_context")[3] == "1.0000"
        assert _cells("hallucination_rate_when_top1_irrelevant")[3] == "1.0000"
        assert _cells("success_rate_when_at_least_one_relevant_in_topk")[3] == "0.5000"

    def test_4_2_all_null_dict_section_omitted(self) -> None:
        # Omission check must inspect only the 20 aggregate keys, not k-value keys.
        row = _all_null_conditional_row()
        result = _build_conditional_retrieval_generation_section(row)
        assert result == []
        assert "Conditional Retrieval" not in "\n".join(result)

    def test_4_3_none_input_section_omitted(self) -> None:
        result = _build_conditional_retrieval_generation_section(None)
        assert result == []

    def test_4_4_zero_denominator_renders_na_not_zero(self) -> None:
        # All reranking_strict values are None; other conditions are non-null.
        row = _full_conditional_row()
        for key in _CONDITIONAL_AGGREGATE_KEYS:
            if key.endswith("_reranking_strict"):
                row[key] = None

        result = _build_conditional_retrieval_generation_section(row)
        output = "\n".join(result)

        assert result  # section not omitted
        assert "0.0000" not in output

        metric_labels = [label for label, _ in (
            ("groundedness_given_relevant_context", ""),
            ("answer_completeness_given_relevant_context", ""),
            ("answer_relevance_given_relevant_context", ""),
            ("hallucination_rate_when_top1_irrelevant", ""),
            ("success_rate_when_at_least_one_relevant_in_topk", ""),
        )]
        for label in metric_labels:
            matching = [line for line in result if label in line]
            assert len(matching) == 1
            cells = [c.strip() for c in matching[0].split("|") if c.strip()]
            # reranking_strict is the last (index 4) column
            assert cells[4] == "n/a", f"expected n/a in reranking_strict for {label!r}"

    def test_4_5_null_rank_treated_as_irrelevant(self) -> None:
        # The helper receives a pre-aggregated dict. hallucination_reranking_soft is non-null
        # because the SQL query counted NULL first_relevant_rank as top1_irrelevant.
        # The helper must render the value, not re-implement the aggregation logic.
        row: dict[str, Any] = {key: None for key in _CONDITIONAL_AGGREGATE_KEYS}
        row["hallucination_reranking_soft"] = Decimal("1.0000")
        row["retrieval_evaluated_k"] = 12
        row["reranking_evaluated_k"] = 4

        result = _build_conditional_retrieval_generation_section(row)
        assert result  # not omitted

        hallucination_lines = [line for line in result if "hallucination_rate_when_top1_irrelevant" in line]
        assert len(hallucination_lines) == 1
        cells = [c.strip() for c in hallucination_lines[0].split("|") if c.strip()]
        # reranking_soft is at index 3; it must be the rendered non-null value
        assert cells[3] == "1.0000"
        assert cells[3] != "n/a"

    def test_4_6_k_values_substituted_into_headers(self) -> None:
        # Dict keys use fixed SQL aliases (no k in key name).
        # Rendered headers must use k-substituted column names.
        row = _full_conditional_row(rk=20, nk=5)
        result = _build_conditional_retrieval_generation_section(row)
        output = "\n".join(result)

        assert "retrieval@20_soft" in output
        assert "generation_context@5_soft" in output
        assert "retrieval_evaluated_k" not in output
        assert "reranking_evaluated_k" not in output
        assert "_conditioned" not in output


# ---------------------------------------------------------------------------
# §5 — _build_run_report integration
# ---------------------------------------------------------------------------

class TestBuildRunReportIntegration:

    def test_5_1_includes_retrieval_quality_when_data_available(self) -> None:
        conn = _fake_conn(
            [],                                      # judge_generation_results
            [],                                      # judge_retrieval_results
            [_full_retrieval_quality_row()],         # request_run_summaries (retrieval quality)
            [_retrieval_depth_row()],                # request_run_summaries (retrieval depth)
            [_all_null_conditional_row()],           # request_run_summaries (conditional)
            [_pipeline_configs_row()],               # request_captures (pipeline configs)
            [_runtime_token_usage_row()],            # request_captures (runtime token usage)
            _judge_token_usage_rows(),               # judge_llm_calls
        )
        report = _build_run_report(conn, "test-run-id", _minimal_manifest(), _minimal_judge_settings())

        assert "Retrieval Quality" in report
        assert "retrieval@12" in report

    def test_5_2_omits_retrieval_quality_when_no_data(self) -> None:
        conn = _fake_conn(
            [],
            [],
            [_all_null_retrieval_quality_row()],
            [_retrieval_depth_row()],
            [_all_null_conditional_row()],
            [_pipeline_configs_row()],
            [_runtime_token_usage_row()],
            _judge_token_usage_rows(),
        )
        report = _build_run_report(conn, "test-run-id", _minimal_manifest(), _minimal_judge_settings())

        assert "Retrieval Quality" not in report

    def test_5_3_omits_conditional_section_when_all_null(self) -> None:
        conn = _fake_conn(
            [],
            [],
            [_full_retrieval_quality_row()],
            [_retrieval_depth_row()],
            [_all_null_conditional_row()],
            [_pipeline_configs_row()],
            [_runtime_token_usage_row()],
            _judge_token_usage_rows(),
        )
        report = _build_run_report(conn, "test-run-id", _minimal_manifest(), _minimal_judge_settings())

        assert "Conditional Retrieval" not in report

    def test_5_4_run_metadata_h3_subsections_present(self) -> None:
        conn = _fake_conn(
            [],
            [],
            [_all_null_retrieval_quality_row()],
            [_retrieval_depth_row()],
            [_all_null_conditional_row()],
            [_pipeline_configs_row()],
            [_runtime_token_usage_row()],
            _judge_token_usage_rows(),
        )
        report = _build_run_report(conn, "test-run-id", _minimal_manifest(), _minimal_judge_settings())

        assert "### Retriever" in report
        assert "### Reranker" in report
        assert "### Generation" in report
        assert "### Judge" in report
        assert "- eval_run_id: `test-run-id`" in report
        assert "- runtime_run_id: `runtime-run-id-1`" in report
        assert "- kind: `Dense`" in report
        assert "- kind: `PassThrough`" in report
        assert "- embedding_model: `test-embedding-model`" in report
        assert "- collection: `test-collection`" in report
        assert "- corpus_version: `v1`" in report
        assert "- chunking_strategy: `structural`" in report
        assert "- model: `test-gen-model`" in report
        assert "- model_endpoint: `http://ollama:11434`" in report
        assert "- temperature: `0.0`" in report
        assert "- max_context_chunks: `4`" in report
        assert "- provider: `ollama`" in report
        assert "- model: `test-judge-model`" in report
        assert "- endpoint: `http://judge:11434/v1`" in report
        assert "- top_k: `12`" in report
        assert "- actual_chunks_returned: `mean=7.60, min=5, max=12`" in report

    def test_5_5_run_metadata_no_legacy_retriever_reranker_bullets(self) -> None:
        conn = _fake_conn(
            [],
            [],
            [_all_null_retrieval_quality_row()],
            [_retrieval_depth_row()],
            [_all_null_conditional_row()],
            [_pipeline_configs_row()],
            [_runtime_token_usage_row()],
            _judge_token_usage_rows(),
        )
        report = _build_run_report(conn, "test-run-id", _minimal_manifest(), _minimal_judge_settings())

        assert "- retriever_kind:" not in report
        assert "- reranker_kind:" not in report
        assert "- judge_model:" not in report

    @pytest.mark.parametrize(
        ("strategy_kind", "expected_label"),
        [
            ("bag_of_words", "Hybrid - bow"),
            ("bm25_like", "Hybrid - bm25"),
        ],
    )
    def test_5_6_hybrid_retriever_header_shows_sparse_strategy(
        self,
        strategy_kind: str,
        expected_label: str,
    ) -> None:
        conn = _fake_conn(
            [],
            [],
            [_all_null_retrieval_quality_row()],
            [_retrieval_depth_row()],
            [_all_null_conditional_row()],
            [_hybrid_pipeline_configs_row(strategy_kind)],
            [_runtime_token_usage_row()],
            _judge_token_usage_rows(),
        )
        report = _build_run_report(conn, "test-run-id", _minimal_manifest(), _minimal_judge_settings())

        assert f"- kind: `{expected_label}`" in report

    def test_5_6_crossencoder_reranker_shows_model_and_url(self) -> None:
        configs_row = _pipeline_configs_row()
        configs_row["reranker_kind"] = "CrossEncoder"
        configs_row["reranker_config"] = {
            "kind": "CrossEncoder",
            "cross_encoder": {
                "model_name": "cross-encoder/test",
                "url": "http://reranker:8080",
                "total_tokens": 12,
                "cost_per_million_tokens": 0.0,
            },
        }
        conn = _fake_conn(
            [],
            [],
            [_all_null_retrieval_quality_row()],
            [_retrieval_depth_row()],
            [_all_null_conditional_row()],
            [configs_row],
            [_runtime_token_usage_row()],
            _judge_token_usage_rows(),
        )
        report = _build_run_report(conn, "test-run-id", _minimal_manifest(), _minimal_judge_settings())

        assert "- kind: `CrossEncoder`" in report
        assert "- model: `cross-encoder/test`" in report
        assert "- url: `http://reranker:8080`" in report

    def test_5_7_token_usage_section_and_run_total_formula_present(self) -> None:
        conn = _fake_conn(
            [],
            [],
            [_all_null_retrieval_quality_row()],
            [_retrieval_depth_row()],
            [_all_null_conditional_row()],
            [_pipeline_configs_row()],
            [_runtime_token_usage_row()],
            _judge_token_usage_rows(),
        )
        report = _build_run_report(conn, "test-run-id", _minimal_manifest(), _minimal_judge_settings())

        assert "## Token Usage" in report
        assert "### Runtime" in report
        assert "### Judge" in report
        assert "| runtime | 2 | 4,000 | 1,000 | 5,000 | 0.00020000 | 0.00020000 | 0.00040000 |" in report
        assert "| judge_generation | 8 | 8,000 | 800 | 8,800 | 0.00040000 | 0.00016000 | 0.00056000 |" in report
        assert "| judge_retrieval | 24 | 24,000 | 2,400 | 26,400 | 0.00120000 | 0.00048000 | 0.00168000 |" in report
        assert "| judge_total | 32 | 32,000 | 3,200 | 35,200 | 0.00160000 | 0.00064000 | 0.00224000 |" in report
        assert "Run total cost usd = runtime total cost usd + judge total cost usd" in report
        assert "Run total cost usd = 0.00040000 + 0.00224000 = 0.00264000" in report
        assert report.rfind("## Token Usage") > report.rfind("## Worst-Case Preview")


class TestConditionalSectionDefinitions:

    def test_definitions_note_present_when_section_rendered(self) -> None:
        row = _full_conditional_row()
        result = _build_conditional_retrieval_generation_section(row)
        output = "\n".join(result)

        assert "groundedness == 1.0 AND answer\\_completeness == 1.0" in output
        assert "groundedness < 1.0" in output

    def test_definitions_note_absent_when_section_omitted(self) -> None:
        result = _build_conditional_retrieval_generation_section(_all_null_conditional_row())
        assert result == []
