from __future__ import annotations

import unittest
from contextlib import contextmanager
from datetime import datetime, timezone
from decimal import Decimal

from Execution.evals import build_request_summary


class RecordingConnection:
    def __init__(self) -> None:
        self.calls: list[tuple[str, tuple[object, ...]]] = []

    @contextmanager
    def transaction(self):
        yield

    def execute(self, query: str, params: tuple[object, ...]):
        self.calls.append((query, params))
        return None


class BuildRequestSummaryTests(unittest.TestCase):
    def test_build_summary_row_copies_judge_metadata(self) -> None:
        request_capture = build_request_summary.RequestCaptureRow(
            request_id="req-1",
            trace_id="trace-1",
            source_received_at=datetime(2026, 3, 29, 10, 0, tzinfo=timezone.utc),
            raw_query="What is eventual consistency?",
            normalized_query="what is eventual consistency",
            input_token_count=5,
            pipeline_config_version="pipeline-v1",
            corpus_version="corpus-v1",
            retriever_version="retriever-v1",
            retriever_kind="Dense",
            embedding_model="embed-v1",
            reranker_kind="Heuristic",
            prompt_template_id="prompt-id",
            prompt_template_version="prompt-v1",
            generation_model="gen-v1",
            top_k_requested=3,
            final_answer="answer",
            prompt_tokens=10,
            completion_tokens=12,
            total_tokens=22,
            retrieval_chunk_ids=("chunk-1", "chunk-2"),
        )
        generation_rows = (
            build_request_summary.GenerationRow(
                suite_name="answer_completeness",
                judge_model="judge-gen-v1",
                judge_prompt_version="gen-prompt-v1",
                score=Decimal("0.9"),
                label="complete",
            ),
            build_request_summary.GenerationRow(
                suite_name="groundedness",
                judge_model="judge-gen-v1",
                judge_prompt_version="gen-prompt-v1",
                score=Decimal("0.8"),
                label="grounded",
            ),
            build_request_summary.GenerationRow(
                suite_name="answer_relevance",
                judge_model="judge-gen-v1",
                judge_prompt_version="gen-prompt-v1",
                score=Decimal("0.7"),
                label="relevant",
            ),
            build_request_summary.GenerationRow(
                suite_name="correct_refusal",
                judge_model="judge-gen-v1",
                judge_prompt_version="gen-prompt-v1",
                score=Decimal("1.0"),
                label="correct_refusal",
            ),
        )
        retrieval_rows = (
            build_request_summary.RetrievalRow(
                chunk_id="chunk-1",
                retrieval_rank=1,
                selected_for_generation=True,
                judge_model="judge-ret-v1",
                judge_prompt_version="ret-prompt-v1",
                score=Decimal("0.6"),
                label="relevant",
            ),
            build_request_summary.RetrievalRow(
                chunk_id="chunk-2",
                retrieval_rank=2,
                selected_for_generation=False,
                judge_model="judge-ret-v1",
                judge_prompt_version="ret-prompt-v1",
                score=Decimal("0.2"),
                label="irrelevant",
            ),
        )

        summary_row = build_request_summary._build_summary_row(
            request_capture,
            generation_rows,
            retrieval_rows,
        )

        self.assertEqual(summary_row.judge_generation_model, "judge-gen-v1")
        self.assertEqual(summary_row.judge_generation_prompt_version, "gen-prompt-v1")
        self.assertEqual(summary_row.judge_retrieval_model, "judge-ret-v1")
        self.assertEqual(summary_row.judge_retrieval_prompt_version, "ret-prompt-v1")
        self.assertEqual(summary_row.retriever_kind, "Dense")
        self.assertEqual(summary_row.reranker_kind, "Heuristic")
        self.assertEqual(summary_row.retrieval_relevance_mean, Decimal("0.4"))
        self.assertEqual(summary_row.retrieval_relevance_selected_mean, Decimal("0.6"))
        self.assertEqual(summary_row.retrieval_relevance_relevant_count, 1)
        self.assertEqual(summary_row.retrieval_chunk_count, 2)

    def test_upsert_run_summary_row_uses_run_scoped_key_and_metadata(self) -> None:
        connection = RecordingConnection()
        summary_row = build_request_summary.RequestSummaryRow(
            request_id="req-1",
            trace_id="trace-1",
            source_received_at=datetime(2026, 3, 29, 10, 0, tzinfo=timezone.utc),
            raw_query="raw",
            normalized_query="normalized",
            input_token_count=5,
            pipeline_config_version="pipeline-v1",
            corpus_version="corpus-v1",
            retriever_version="retriever-v1",
            retriever_kind="Dense",
            embedding_model="embed-v1",
            reranker_kind="Heuristic",
            prompt_template_id="prompt-id",
            prompt_template_version="prompt-v1",
            generation_model="gen-v1",
            top_k_requested=3,
            final_answer="answer",
            prompt_tokens=10,
            completion_tokens=12,
            total_tokens=22,
            answer_completeness_score=Decimal("0.9"),
            answer_completeness_label="complete",
            groundedness_score=Decimal("0.8"),
            groundedness_label="grounded",
            answer_relevance_score=Decimal("0.7"),
            answer_relevance_label="relevant",
            correct_refusal_score=Decimal("1.0"),
            correct_refusal_label="correct_refusal",
            retrieval_relevance_mean=Decimal("0.4"),
            retrieval_relevance_selected_mean=Decimal("0.6"),
            retrieval_relevance_topk_mean=Decimal("0.4"),
            retrieval_relevance_weighted_topk=Decimal("0.4667"),
            retrieval_relevance_relevant_count=1,
            retrieval_relevance_selected_count=1,
            retrieval_chunk_count=2,
            judge_generation_model="judge-gen-v1",
            judge_generation_prompt_version="gen-prompt-v1",
            judge_retrieval_model="judge-ret-v1",
            judge_retrieval_prompt_version="ret-prompt-v1",
        )

        build_request_summary._upsert_run_summary_row(connection, "run-1", summary_row)

        self.assertEqual(len(connection.calls), 1)
        query, params = connection.calls[0]
        self.assertIn("insert into request_run_summaries", query)
        self.assertIn("on conflict (request_id, run_id) do update", query)
        self.assertEqual(params[0], "req-1")
        self.assertEqual(params[1], "run-1")
        self.assertIn("retriever_kind", query)
        self.assertEqual(params[-4], "judge-gen-v1")
        self.assertEqual(params[-3], "gen-prompt-v1")
        self.assertEqual(params[-2], "judge-ret-v1")
        self.assertEqual(params[-1], "ret-prompt-v1")


    def test_build_summary_row_unpacks_retrieval_quality_metrics(self) -> None:
        request_capture = build_request_summary.RequestCaptureRow(
            request_id="req-1",
            trace_id="trace-1",
            source_received_at=datetime(2026, 3, 29, 10, 0, tzinfo=timezone.utc),
            raw_query="What is eventual consistency?",
            normalized_query="what is eventual consistency",
            input_token_count=5,
            pipeline_config_version="pipeline-v1",
            corpus_version="corpus-v1",
            retriever_version="retriever-v1",
            retriever_kind="Dense",
            embedding_model="embed-v1",
            reranker_kind="PassThrough",
            prompt_template_id="prompt-id",
            prompt_template_version="prompt-v1",
            generation_model="gen-v1",
            top_k_requested=3,
            final_answer="answer",
            prompt_tokens=10,
            completion_tokens=12,
            total_tokens=22,
            retrieval_chunk_ids=("chunk-1",),
            retrieval_stage_metrics={
                "evaluated_k": 12,
                "recall_soft": 0.75,
                "recall_strict": 0.5,
                "rr_soft": 0.8,
                "rr_strict": 0.6,
                "ndcg": 0.7,
                "first_relevant_rank_soft": 1,
                "first_relevant_rank_strict": 2,
                "num_relevant_soft": 3,
                "num_relevant_strict": 2,
            },
            reranking_stage_metrics={
                "evaluated_k": 4,
                "recall_soft": 0.5,
                "recall_strict": 0.25,
                "rr_soft": 1.0,
                "rr_strict": 0.5,
                "ndcg": 0.6,
                "first_relevant_rank_soft": None,
                "first_relevant_rank_strict": None,
                "num_relevant_soft": 2,
                "num_relevant_strict": 1,
            },
        )
        generation_rows = (
            build_request_summary.GenerationRow(
                suite_name="answer_completeness",
                judge_model="judge-gen-v1",
                judge_prompt_version="gen-prompt-v1",
                score=Decimal("0.9"),
                label="complete",
            ),
            build_request_summary.GenerationRow(
                suite_name="groundedness",
                judge_model="judge-gen-v1",
                judge_prompt_version="gen-prompt-v1",
                score=Decimal("0.8"),
                label="grounded",
            ),
            build_request_summary.GenerationRow(
                suite_name="answer_relevance",
                judge_model="judge-gen-v1",
                judge_prompt_version="gen-prompt-v1",
                score=Decimal("0.7"),
                label="relevant",
            ),
            build_request_summary.GenerationRow(
                suite_name="correct_refusal",
                judge_model="judge-gen-v1",
                judge_prompt_version="gen-prompt-v1",
                score=Decimal("1.0"),
                label="correct_refusal",
            ),
        )
        retrieval_rows = (
            build_request_summary.RetrievalRow(
                chunk_id="chunk-1",
                retrieval_rank=1,
                selected_for_generation=True,
                judge_model="judge-ret-v1",
                judge_prompt_version="ret-prompt-v1",
                score=Decimal("0.6"),
                label="relevant",
            ),
        )

        summary_row = build_request_summary._build_summary_row(
            request_capture,
            generation_rows,
            retrieval_rows,
        )

        self.assertEqual(summary_row.retrieval_evaluated_k, 12)
        self.assertEqual(summary_row.retrieval_recall_soft, Decimal("0.75"))
        self.assertEqual(summary_row.retrieval_recall_strict, Decimal("0.5"))
        self.assertEqual(summary_row.retrieval_first_relevant_rank_soft, 1)
        self.assertEqual(summary_row.retrieval_num_relevant_soft, 3)

        self.assertEqual(summary_row.reranking_evaluated_k, 4)
        self.assertEqual(summary_row.reranking_recall_soft, Decimal("0.5"))
        self.assertIsNone(summary_row.reranking_first_relevant_rank_soft)

        # context_loss = retrieval_recall - reranking_recall
        self.assertEqual(
            summary_row.retrieval_context_loss_soft,
            Decimal("0.75") - Decimal("0.5"),
        )
        self.assertEqual(
            summary_row.retrieval_context_loss_strict,
            Decimal("0.5") - Decimal("0.25"),
        )

    def test_build_summary_row_metrics_are_none_when_not_captured(self) -> None:
        request_capture = build_request_summary.RequestCaptureRow(
            request_id="req-2",
            trace_id="trace-2",
            source_received_at=datetime(2026, 3, 29, 10, 0, tzinfo=timezone.utc),
            raw_query="q",
            normalized_query="q",
            input_token_count=1,
            pipeline_config_version="v1",
            corpus_version="c1",
            retriever_version="r1",
            retriever_kind="Dense",
            embedding_model="em",
            reranker_kind="PassThrough",
            prompt_template_id="pt",
            prompt_template_version="pv",
            generation_model="gm",
            top_k_requested=2,
            final_answer="a",
            prompt_tokens=5,
            completion_tokens=3,
            total_tokens=8,
            retrieval_chunk_ids=("c1",),
            retrieval_stage_metrics=None,
            reranking_stage_metrics=None,
        )
        generation_rows = (
            build_request_summary.GenerationRow(
                suite_name="answer_completeness", judge_model="j", judge_prompt_version="v",
                score=Decimal("1"), label="complete",
            ),
            build_request_summary.GenerationRow(
                suite_name="groundedness", judge_model="j", judge_prompt_version="v",
                score=Decimal("1"), label="grounded",
            ),
            build_request_summary.GenerationRow(
                suite_name="answer_relevance", judge_model="j", judge_prompt_version="v",
                score=Decimal("1"), label="relevant",
            ),
            build_request_summary.GenerationRow(
                suite_name="correct_refusal", judge_model="j", judge_prompt_version="v",
                score=Decimal("1"), label="correct_refusal",
            ),
        )
        retrieval_rows = (
            build_request_summary.RetrievalRow(
                chunk_id="c1", retrieval_rank=1, selected_for_generation=True,
                judge_model="j", judge_prompt_version="v",
                score=Decimal("0.5"), label="relevant",
            ),
        )

        summary_row = build_request_summary._build_summary_row(
            request_capture, generation_rows, retrieval_rows
        )

        self.assertIsNone(summary_row.retrieval_evaluated_k)
        self.assertIsNone(summary_row.retrieval_recall_soft)
        self.assertIsNone(summary_row.reranking_evaluated_k)
        self.assertIsNone(summary_row.reranking_recall_soft)
        self.assertIsNone(summary_row.retrieval_context_loss_soft)
        self.assertIsNone(summary_row.retrieval_context_loss_strict)

    def test_unpack_metrics_returns_none_values_for_none_input(self) -> None:
        result = build_request_summary._unpack_metrics(None, "retrieval")
        for key in result:
            self.assertIsNone(result[key])

    def test_unpack_metrics_converts_floats_to_decimal(self) -> None:
        metrics = {
            "evaluated_k": 12,
            "recall_soft": 0.75,
            "recall_strict": 0.5,
            "rr_soft": 0.8,
            "rr_strict": 0.6,
            "ndcg": 0.7,
            "first_relevant_rank_soft": 1,
            "first_relevant_rank_strict": None,
            "num_relevant_soft": 3,
            "num_relevant_strict": 2,
        }
        result = build_request_summary._unpack_metrics(metrics, "retrieval")
        self.assertIsInstance(result["retrieval_recall_soft"], Decimal)
        self.assertEqual(result["retrieval_evaluated_k"], 12)
        self.assertEqual(result["retrieval_first_relevant_rank_soft"], 1)
        self.assertIsNone(result["retrieval_first_relevant_rank_strict"])


if __name__ == "__main__":
    unittest.main()
