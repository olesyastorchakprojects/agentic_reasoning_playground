from __future__ import annotations

import unittest

from Execution.evals import tagged_metrics


class TaggedMetricsTests(unittest.TestCase):
    def test_normalize_question_text_collapses_whitespace_and_case(self) -> None:
        self.assertEqual(
            tagged_metrics.normalize_question_text(" Why is  TCP difficult? \n"),
            "why is tcp difficult?",
        )

    def test_map_request_rows_to_tagged_questions_uses_exact_normalized_text_match(self) -> None:
        tagged_questions = {
            "q1": tagged_metrics.TaggedQuestionRecord(
                question_id="q1",
                question_text="Why is time difficult in distributed systems?",
                tags=("causal", "failure"),
            )
        }
        request_rows = [
            tagged_metrics.RequestMetricRow(
                run_id="run-1",
                request_id="req-1",
                raw_query="Why is time difficult in distributed systems?",
                answer_completeness_score=0.5,
                groundedness_score=1.0,
                answer_relevance_score=1.0,
                generation_context_strict_recall_at_4=0.75,
                generation_context_ndcg_at_4=0.8,
            )
        ]

        rows = tagged_metrics.map_request_rows_to_tagged_questions(request_rows, tagged_questions)

        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0].question_id, "q1")
        self.assertEqual(rows[0].tags, ("causal", "failure"))

    def test_aggregate_rows_by_tag_reports_missing_questions_and_means(self) -> None:
        tagged_questions = {
            "q1": tagged_metrics.TaggedQuestionRecord(
                question_id="q1",
                question_text="Question 1",
                tags=("failure",),
            ),
            "q2": tagged_metrics.TaggedQuestionRecord(
                question_id="q2",
                question_text="Question 2",
                tags=("failure", "contrast"),
            ),
            "q3": tagged_metrics.TaggedQuestionRecord(
                question_id="q3",
                question_text="Question 3",
                tags=("contrast",),
            ),
        }
        tagged_rows = [
            tagged_metrics.TaggedRequestMetricRow(
                run_id="run-1",
                request_id="req-1",
                question_id="q1",
                question_text="Question 1",
                raw_query="Question 1",
                tags=("failure",),
                answer_completeness_score=0.0,
                groundedness_score=0.5,
                answer_relevance_score=1.0,
                generation_context_strict_recall_at_4=0.5,
                generation_context_ndcg_at_4=0.6,
            ),
            tagged_metrics.TaggedRequestMetricRow(
                run_id="run-1",
                request_id="req-2",
                question_id="q2",
                question_text="Question 2",
                raw_query="Question 2",
                tags=("failure", "contrast"),
                answer_completeness_score=1.0,
                groundedness_score=1.0,
                answer_relevance_score=0.5,
                generation_context_strict_recall_at_4=1.0,
                generation_context_ndcg_at_4=0.8,
            ),
        ]

        aggregates = tagged_metrics.aggregate_rows_by_tag(tagged_rows, tagged_questions)

        self.assertEqual(len(aggregates), 2)
        by_tag = {row.tag: row for row in aggregates}

        failure = by_tag["failure"]
        self.assertEqual(failure.question_count_expected, 2)
        self.assertEqual(failure.question_count_observed, 2)
        self.assertFalse(failure.is_partial_coverage)
        self.assertAlmostEqual(failure.answer_completeness_mean or 0.0, 0.5)
        self.assertAlmostEqual(
            failure.generation_context_strict_recall_at_4_mean or 0.0,
            0.75,
        )

        contrast = by_tag["contrast"]
        self.assertEqual(contrast.question_count_expected, 2)
        self.assertEqual(contrast.question_count_observed, 1)
        self.assertTrue(contrast.is_partial_coverage)
        self.assertEqual(contrast.missing_question_ids, ("q3",))
        self.assertAlmostEqual(contrast.answer_completeness_mean or 0.0, 1.0)


if __name__ == "__main__":
    unittest.main()
