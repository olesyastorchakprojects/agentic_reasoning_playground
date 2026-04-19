#!/usr/bin/env python3
"""Converts canonical incident cards from PostgreSQL into a pre-ingest chunks.jsonl file."""

import argparse
import hashlib
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

import jsonschema
import psycopg2
import psycopg2.extras


SELECT_SQL = """
    SELECT
        case_id,
        title,
        source_type,
        source_name,
        source_path,
        vendor_or_project,
        system_type,
        version_tested,
        report_date,
        short_summary,
        canonical_symptoms,
        affected_components,
        failure_mode_candidates,
        observed_phases,
        incident_phases,
        turning_points,
        candidate_explanations,
        diagnostic_patterns,
        discriminating_checks,
        expected_observations,
        investigation_steps,
        root_cause_summary,
        reasoning_summary,
        mitigations_or_workarounds,
        prevention_or_design_followups,
        claimed_guarantees,
        violated_properties,
        resolution_status,
        fix_versions,
        confidence_notes,
        source_refs
    FROM diagnostics.incident_cards
    ORDER BY case_id ASC
"""

SCHEMA_VERSION = 1
SECTION_TITLE = "incident_card"
SECTION_PATH = ["incident_card"]
CHUNK_INDEX = 0
PAGE_START = 1
PAGE_END = 1
CHUNKING_VERSION = "v1"


def load_schema(path: str) -> dict:
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def normalize_scalar(value: str) -> str:
    value = value.strip()
    value = value.replace("\r\n", " ").replace("\n", " ").replace("\r", " ")
    while "  " in value:
        value = value.replace("  ", " ")
    return value


def normalize_list(items: list) -> list:
    normalized = []
    seen = []
    for item in items:
        item = normalize_scalar(item)
        if item and item not in seen:
            seen.append(item)
            normalized.append(item)
    return normalized


def build_text(card: dict) -> str:
    sections = []

    scalar_fields = [
        ("Title", "title"),
        ("Summary", "short_summary"),
    ]
    for label, field in scalar_fields:
        value = card.get(field)
        if value is None:
            continue
        value = normalize_scalar(value)
        if value:
            sections.append(f"{label}: {value}")

    list_fields = [
        ("Canonical symptoms", "canonical_symptoms"),
        ("Affected components", "affected_components"),
        ("Failure mode candidates", "failure_mode_candidates"),
        ("Diagnostic patterns", "diagnostic_patterns"),
    ]
    for label, field in list_fields:
        items = card.get(field) or []
        normalized = normalize_list(items)
        if normalized:
            sections.append(f"{label}: {'; '.join(normalized)}")

    root_cause = card.get("root_cause_summary")
    if root_cause is not None:
        root_cause = normalize_scalar(root_cause)
        if root_cause:
            sections.append(f"Root cause summary: {root_cause}")

    trailing_list_fields = [
        ("Violated properties", "violated_properties"),
        ("Claimed guarantees", "claimed_guarantees"),
        ("Mitigations or workarounds", "mitigations_or_workarounds"),
    ]
    for label, field in trailing_list_fields:
        items = card.get(field) or []
        normalized = normalize_list(items)
        if normalized:
            sections.append(f"{label}: {'; '.join(normalized)}")

    return "\n".join(sections)


def derive_url(title: str) -> str:
    normalized = normalize_scalar(title)
    return f"local://incident_cards/{normalized}"


def content_hash(text: str) -> str:
    digest = hashlib.sha256(text.encode("utf-8")).hexdigest()
    return f"sha256:{digest}"


def row_to_card(row: dict) -> dict:
    card = {}
    for key, value in row.items():
        if isinstance(value, list):
            card[key] = value
        elif value is None:
            card[key] = None
        else:
            card[key] = value
    if card.get("report_date") is not None:
        card["report_date"] = card["report_date"].isoformat()
    return card


def convert_card(card: dict, created_at: str) -> dict:
    text = build_text(card)
    return {
        "schema_version": SCHEMA_VERSION,
        "doc_id": card["case_id"],
        "chunk_id": card["case_id"],
        "url": derive_url(card["title"]),
        "document_title": card["title"],
        "section_title": SECTION_TITLE,
        "section_path": SECTION_PATH,
        "chunk_index": CHUNK_INDEX,
        "page_start": PAGE_START,
        "page_end": PAGE_END,
        "content_hash": content_hash(text),
        "chunking_version": CHUNKING_VERSION,
        "chunk_created_at": created_at,
        "text": text,
    }


def current_utc_timestamp() -> str:
    now = datetime.now(timezone.utc)
    return now.strftime("%Y-%m-%dT%H:%M:%SZ")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Convert incident cards from PostgreSQL into a pre-ingest chunks.jsonl file."
    )
    parser.add_argument("--postgres-url", required=True)
    parser.add_argument("--output-path", required=True)
    parser.add_argument("--incident-card-schema-path", required=True)
    parser.add_argument("--chunk-schema-path", required=True)
    args = parser.parse_args()

    output_path = Path(args.output_path)
    if output_path.exists():
        print(f"error: output file already exists: {output_path}", file=sys.stderr)
        sys.exit(1)

    incident_card_schema = load_schema(args.incident_card_schema_path)
    chunk_schema = load_schema(args.chunk_schema_path)

    format_checker = jsonschema.FormatChecker()
    card_validator = jsonschema.Draft202012Validator(
        incident_card_schema, format_checker=format_checker
    )
    chunk_validator = jsonschema.Draft202012Validator(
        chunk_schema, format_checker=format_checker
    )

    created_at = current_utc_timestamp()

    conn = psycopg2.connect(args.postgres_url)
    try:
        with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as cur:
            cur.execute(SELECT_SQL)
            rows = cur.fetchall()
    finally:
        conn.close()

    tmp_path = output_path.parent / (output_path.name + ".tmp")
    if tmp_path.exists():
        print(
            f"error: temporary output file already exists: {tmp_path}",
            file=sys.stderr,
        )
        sys.exit(1)

    def cleanup_tmp() -> None:
        if tmp_path.exists():
            tmp_path.unlink()

    def format_validation_error(err: jsonschema.ValidationError) -> str:
        path = ".".join(str(p) for p in err.absolute_path)
        location = f" at .{path}" if path else ""
        return f"{err.message}{location}"

    def fail(msg: str) -> None:
        print(f"error: {msg}", file=sys.stderr)
        cleanup_tmp()
        sys.exit(1)

    try:
        with open(tmp_path, "w", encoding="utf-8") as out:
            for row in rows:
                card = row_to_card(dict(row))
                case_id = card.get("case_id")

                errors = list(card_validator.iter_errors(card))
                if errors:
                    detail = format_validation_error(errors[0])
                    fail(f"card {case_id!r} failed input validation: {detail}")

                chunk = convert_card(card, created_at)

                errors = list(chunk_validator.iter_errors(chunk))
                if errors:
                    detail = format_validation_error(errors[0])
                    fail(f"chunk for card {case_id!r} failed output validation: {detail}")

                out.write(json.dumps(chunk, ensure_ascii=False) + "\n")

        os.replace(tmp_path, output_path)

    except Exception:
        cleanup_tmp()
        raise


if __name__ == "__main__":
    main()
