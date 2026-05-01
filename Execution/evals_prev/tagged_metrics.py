from __future__ import annotations

import argparse
import csv
import json
import os
from dataclasses import dataclass
from pathlib import Path
from statistics import fmean
from typing import Any

from psycopg import Connection
from psycopg.rows import dict_row

from .common import project_root


DEFAULT_DATASET_DIR = project_root() / "Evidence" / "evals" / "datasets" / "20"
DEFAULT_TAGS_PATH = project_root() / "Execution" / "evals" / "golden_question_tags.json"
DEFAULT_OUTPUT_DIR = project_root() / "Evidence" / "analysis" / "tagged_question_metrics"


@dataclass(frozen=True)
class QuestionRecord:
    question_id: str
    question_text: str


@dataclass(frozen=True)
class TaggedQuestionRecord:
    question_id: str
    question_text: str
    tags: tuple[str, ...]


@dataclass(frozen=True)
class RequestMetricRow:
    run_id: str
    request_id: str
    raw_query: str
    answer_completeness_score: float | None
    groundedness_score: float | None
    answer_relevance_score: float | None
    generation_context_strict_recall_at_4: float | None
    generation_context_ndcg_at_4: float | None


@dataclass(frozen=True)
class TaggedRequestMetricRow:
    run_id: str
    request_id: str
    question_id: str
    question_text: str
    raw_query: str
    tags: tuple[str, ...]
    answer_completeness_score: float | None
    groundedness_score: float | None
    answer_relevance_score: float | None
    generation_context_strict_recall_at_4: float | None
    generation_context_ndcg_at_4: float | None


@dataclass(frozen=True)
class TagAggregateRow:
    run_id: str
    tag: str
    question_count_expected: int
    question_count_observed: int
    request_count_observed: int
    answer_completeness_mean: float | None
    groundedness_mean: float | None
    answer_relevance_mean: float | None
    generation_context_strict_recall_at_4_mean: float | None
    generation_context_ndcg_at_4_mean: float | None
    missing_question_ids: tuple[str, ...]
    is_partial_coverage: bool


class TaggedMetricsError(RuntimeError):
    """Base error for tagged metric analysis."""


class TagConfigurationError(TaggedMetricsError):
    pass


class DatasetMappingError(TaggedMetricsError):
    pass


class DataLookupError(TaggedMetricsError):
    pass


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Recompute request-level answer and generation-context metrics by golden-question tag."
        )
    )
    parser.add_argument("--run-id", dest="run_ids", action="append", required=True)
    parser.add_argument("--postgres-url", default=os.environ.get("POSTGRES_URL"))
    parser.add_argument("--dataset-dir", default=str(DEFAULT_DATASET_DIR))
    parser.add_argument("--tags-path", default=str(DEFAULT_TAGS_PATH))
    parser.add_argument("--output-dir", default=str(DEFAULT_OUTPUT_DIR))
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if not args.postgres_url:
        raise SystemExit("--postgres-url is required when POSTGRES_URL is not set")

    dataset_dir = Path(args.dataset_dir)
    tags_path = Path(args.tags_path)
    output_dir = Path(args.output_dir)

    tagged_questions = load_tagged_questions(dataset_dir, tags_path)
    request_rows = load_request_metric_rows(args.postgres_url, tuple(args.run_ids))
    tagged_rows = map_request_rows_to_tagged_questions(request_rows, tagged_questions)
    aggregates = aggregate_rows_by_tag(tagged_rows, tagged_questions)

    output_dir.mkdir(parents=True, exist_ok=True)
    write_per_question_csv(output_dir / "per_question_metrics.csv", tagged_rows)
    write_summary_csv(output_dir / "tagged_metrics.csv", aggregates)
    write_markdown_report(output_dir / "tagged_metrics.md", aggregates)
    return 0


def load_tagged_questions(
    dataset_dir: Path,
    tags_path: Path,
) -> dict[str, TaggedQuestionRecord]:
    question_catalog = load_question_catalog(dataset_dir)
    raw_tag_specs = json.loads(tags_path.read_text(encoding="utf-8"))
    if not isinstance(raw_tag_specs, list):
        raise TagConfigurationError("tag file must contain a JSON array")

    tagged_questions: dict[str, TaggedQuestionRecord] = {}
    seen_tags: set[str] = set()
    for item in raw_tag_specs:
        if not isinstance(item, dict):
            raise TagConfigurationError("every tag entry must be a JSON object")
        question_id = item.get("question_id")
        tags = item.get("tags")
        if not isinstance(question_id, str) or not question_id.strip():
            raise TagConfigurationError("question_id must be a non-empty string")
        if question_id not in question_catalog:
            raise TagConfigurationError(f"unknown question_id in tags file: {question_id}")
        if not isinstance(tags, list) or not tags:
            raise TagConfigurationError(f"question_id {question_id} must define at least one tag")
        normalized_tags: list[str] = []
        for tag in tags:
            if not isinstance(tag, str) or not tag.strip():
                raise TagConfigurationError(f"question_id {question_id} contains an invalid tag")
            normalized_tag = tag.strip()
            normalized_tags.append(normalized_tag)
            seen_tags.add(normalized_tag)
        if len(set(normalized_tags)) != len(normalized_tags):
            raise TagConfigurationError(f"question_id {question_id} contains duplicate tags")
        question_record = question_catalog[question_id]
        tagged_questions[question_id] = TaggedQuestionRecord(
            question_id=question_id,
            question_text=question_record.question_text,
            tags=tuple(normalized_tags),
        )

    missing_question_ids = sorted(set(question_catalog) - set(tagged_questions))
    if missing_question_ids:
        raise TagConfigurationError(
            "tags file does not cover all dataset questions: " + ", ".join(missing_question_ids)
        )
    if not seen_tags:
        raise TagConfigurationError("tags file must define at least one tag")
    return tagged_questions


def load_question_catalog(dataset_dir: Path) -> dict[str, QuestionRecord]:
    structural_path = dataset_dir / "structural_golden_retrievals.json"
    payload = json.loads(structural_path.read_text(encoding="utf-8"))
    questions = payload.get("questions")
    if not isinstance(questions, list):
        raise DatasetMappingError("structural_golden_retrievals.json must contain a questions array")

    catalog: dict[str, QuestionRecord] = {}
    for item in questions:
        if not isinstance(item, dict):
            raise DatasetMappingError("question entries must be objects")
        question_id = item.get("question_id")
        question_text = item.get("question")
        if not isinstance(question_id, str) or not isinstance(question_text, str):
            raise DatasetMappingError("question entries must include question_id and question")
        catalog[question_id] = QuestionRecord(question_id=question_id, question_text=question_text)
    return catalog


def load_request_metric_rows(
    postgres_url: str,
    run_ids: tuple[str, ...],
) -> list[RequestMetricRow]:
    query = """
        select
            run_id,
            request_id,
            raw_query,
            answer_completeness_score,
            groundedness_score,
            answer_relevance_score,
            reranking_recall_strict,
            reranking_ndcg
        from request_run_summaries
        where run_id = any(%s)
        order by run_id, source_received_at, request_id
    """
    with Connection.connect(postgres_url, row_factory=dict_row) as connection:
        rows = connection.execute(query, (list(run_ids),)).fetchall()

    if not rows:
        raise DataLookupError("no request_run_summaries rows found for the supplied run ids")

    return [
        RequestMetricRow(
            run_id=row["run_id"],
            request_id=row["request_id"],
            raw_query=row["raw_query"],
            answer_completeness_score=_optional_float(row["answer_completeness_score"]),
            groundedness_score=_optional_float(row["groundedness_score"]),
            answer_relevance_score=_optional_float(row["answer_relevance_score"]),
            generation_context_strict_recall_at_4=_optional_float(row["reranking_recall_strict"]),
            generation_context_ndcg_at_4=_optional_float(row["reranking_ndcg"]),
        )
        for row in rows
    ]


def map_request_rows_to_tagged_questions(
    request_rows: list[RequestMetricRow],
    tagged_questions: dict[str, TaggedQuestionRecord],
) -> list[TaggedRequestMetricRow]:
    question_lookup = {
        normalize_question_text(record.question_text): record
        for record in tagged_questions.values()
    }
    tagged_rows: list[TaggedRequestMetricRow] = []
    for row in request_rows:
        normalized_query = normalize_question_text(row.raw_query)
        question = question_lookup.get(normalized_query)
        if question is None:
            raise DatasetMappingError(
                "request query does not match the tagged dataset exactly: "
                f"{row.request_id} -> {row.raw_query!r}"
            )
        tagged_rows.append(
            TaggedRequestMetricRow(
                run_id=row.run_id,
                request_id=row.request_id,
                question_id=question.question_id,
                question_text=question.question_text,
                raw_query=row.raw_query,
                tags=question.tags,
                answer_completeness_score=row.answer_completeness_score,
                groundedness_score=row.groundedness_score,
                answer_relevance_score=row.answer_relevance_score,
                generation_context_strict_recall_at_4=row.generation_context_strict_recall_at_4,
                generation_context_ndcg_at_4=row.generation_context_ndcg_at_4,
            )
        )
    return tagged_rows


def aggregate_rows_by_tag(
    tagged_rows: list[TaggedRequestMetricRow],
    tagged_questions: dict[str, TaggedQuestionRecord],
) -> list[TagAggregateRow]:
    expected_by_tag: dict[str, set[str]] = {}
    for question in tagged_questions.values():
        for tag in question.tags:
            expected_by_tag.setdefault(tag, set()).add(question.question_id)

    bucketed_rows: dict[tuple[str, str], list[TaggedRequestMetricRow]] = {}
    for row in tagged_rows:
        for tag in row.tags:
            bucketed_rows.setdefault((row.run_id, tag), []).append(row)

    aggregate_rows: list[TagAggregateRow] = []
    for run_id, tag in sorted(bucketed_rows):
        rows = bucketed_rows[(run_id, tag)]
        observed_question_ids = {row.question_id for row in rows}
        expected_question_ids = expected_by_tag[tag]
        missing_question_ids = tuple(sorted(expected_question_ids - observed_question_ids))
        aggregate_rows.append(
            TagAggregateRow(
                run_id=run_id,
                tag=tag,
                question_count_expected=len(expected_question_ids),
                question_count_observed=len(observed_question_ids),
                request_count_observed=len(rows),
                answer_completeness_mean=_mean_or_none(
                    row.answer_completeness_score for row in rows
                ),
                groundedness_mean=_mean_or_none(row.groundedness_score for row in rows),
                answer_relevance_mean=_mean_or_none(row.answer_relevance_score for row in rows),
                generation_context_strict_recall_at_4_mean=_mean_or_none(
                    row.generation_context_strict_recall_at_4 for row in rows
                ),
                generation_context_ndcg_at_4_mean=_mean_or_none(
                    row.generation_context_ndcg_at_4 for row in rows
                ),
                missing_question_ids=missing_question_ids,
                is_partial_coverage=bool(missing_question_ids),
            )
        )
    return aggregate_rows


def write_per_question_csv(path: Path, rows: list[TaggedRequestMetricRow]) -> None:
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=[
                "run_id",
                "request_id",
                "question_id",
                "question_text",
                "tags",
                "answer_completeness_score",
                "groundedness_score",
                "answer_relevance_score",
                "generation_context_strict_recall_at_4",
                "generation_context_ndcg_at_4",
            ],
        )
        writer.writeheader()
        for row in rows:
            writer.writerow(
                {
                    "run_id": row.run_id,
                    "request_id": row.request_id,
                    "question_id": row.question_id,
                    "question_text": row.question_text,
                    "tags": ",".join(row.tags),
                    "answer_completeness_score": _format_metric(row.answer_completeness_score),
                    "groundedness_score": _format_metric(row.groundedness_score),
                    "answer_relevance_score": _format_metric(row.answer_relevance_score),
                    "generation_context_strict_recall_at_4": _format_metric(
                        row.generation_context_strict_recall_at_4
                    ),
                    "generation_context_ndcg_at_4": _format_metric(
                        row.generation_context_ndcg_at_4
                    ),
                }
            )


def write_summary_csv(path: Path, rows: list[TagAggregateRow]) -> None:
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=[
                "run_id",
                "tag",
                "question_count_expected",
                "question_count_observed",
                "request_count_observed",
                "answer_completeness_mean",
                "groundedness_mean",
                "answer_relevance_mean",
                "generation_context_strict_recall_at_4_mean",
                "generation_context_ndcg_at_4_mean",
                "missing_question_ids",
                "is_partial_coverage",
            ],
        )
        writer.writeheader()
        for row in rows:
            writer.writerow(
                {
                    "run_id": row.run_id,
                    "tag": row.tag,
                    "question_count_expected": row.question_count_expected,
                    "question_count_observed": row.question_count_observed,
                    "request_count_observed": row.request_count_observed,
                    "answer_completeness_mean": _format_metric(row.answer_completeness_mean),
                    "groundedness_mean": _format_metric(row.groundedness_mean),
                    "answer_relevance_mean": _format_metric(row.answer_relevance_mean),
                    "generation_context_strict_recall_at_4_mean": _format_metric(
                        row.generation_context_strict_recall_at_4_mean
                    ),
                    "generation_context_ndcg_at_4_mean": _format_metric(
                        row.generation_context_ndcg_at_4_mean
                    ),
                    "missing_question_ids": ",".join(row.missing_question_ids),
                    "is_partial_coverage": str(row.is_partial_coverage).lower(),
                }
            )


def write_markdown_report(path: Path, rows: list[TagAggregateRow]) -> None:
    lines = [
        "# Tagged Question Metrics",
        "",
        "| run_id | tag | coverage | answer completeness | groundedness | answer relevance | gen ctx strict recall@4 | gen ctx nDCG@4 | missing questions |",
        "|---|---|---:|---:|---:|---:|---:|---:|---|",
    ]
    for row in rows:
        lines.append(
            "| {run_id} | {tag} | {observed}/{expected} | {answer_completeness} | {groundedness} | {answer_relevance} | {strict_recall} | {ndcg} | {missing_questions} |".format(
                run_id=row.run_id,
                tag=row.tag,
                observed=row.question_count_observed,
                expected=row.question_count_expected,
                answer_completeness=_format_metric(row.answer_completeness_mean),
                groundedness=_format_metric(row.groundedness_mean),
                answer_relevance=_format_metric(row.answer_relevance_mean),
                strict_recall=_format_metric(row.generation_context_strict_recall_at_4_mean),
                ndcg=_format_metric(row.generation_context_ndcg_at_4_mean),
                missing_questions=", ".join(row.missing_question_ids) or "-",
            )
        )
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def normalize_question_text(text: str) -> str:
    return " ".join(text.split()).casefold()


def _mean_or_none(values: Any) -> float | None:
    present = [value for value in values if value is not None]
    if not present:
        return None
    return fmean(present)


def _optional_float(value: Any) -> float | None:
    if value is None:
        return None
    return float(value)


def _format_metric(value: float | None) -> str:
    if value is None:
        return "n/a"
    return f"{value:.4f}"


if __name__ == "__main__":
    raise SystemExit(main())
