#!/usr/bin/env python3
"""Loads incident-card YAML files into diagnostics.incident_cards in PostgreSQL.

Only cards whose source_path resolves to an existing file under the raw corpus
directory are inserted. Cards already present (by case_id) are skipped.
"""

import argparse
import json
import sys
from pathlib import Path

import psycopg2
import psycopg2.extras
import yaml


INSERT_SQL = """
INSERT INTO diagnostics.incident_cards (
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
    source_refs,
    card_json
) VALUES (
    %(case_id)s,
    %(title)s,
    %(source_type)s,
    %(source_name)s,
    %(source_path)s,
    %(vendor_or_project)s,
    %(system_type)s,
    %(version_tested)s,
    %(report_date)s,
    %(short_summary)s,
    %(canonical_symptoms)s,
    %(affected_components)s,
    %(failure_mode_candidates)s,
    %(observed_phases)s,
    %(incident_phases)s,
    %(turning_points)s,
    %(candidate_explanations)s,
    %(diagnostic_patterns)s,
    %(discriminating_checks)s,
    %(expected_observations)s,
    %(investigation_steps)s,
    %(root_cause_summary)s,
    %(reasoning_summary)s,
    %(mitigations_or_workarounds)s,
    %(prevention_or_design_followups)s,
    %(claimed_guarantees)s,
    %(violated_properties)s,
    %(resolution_status)s,
    %(fix_versions)s,
    %(confidence_notes)s,
    %(source_refs)s,
    %(card_json)s
)
ON CONFLICT (case_id) DO NOTHING
"""

JSONB_LIST_FIELDS = [
    "canonical_symptoms",
    "affected_components",
    "failure_mode_candidates",
    "observed_phases",
    "incident_phases",
    "turning_points",
    "candidate_explanations",
    "diagnostic_patterns",
    "discriminating_checks",
    "expected_observations",
    "investigation_steps",
    "mitigations_or_workarounds",
    "prevention_or_design_followups",
    "claimed_guarantees",
    "violated_properties",
    "fix_versions",
    "confidence_notes",
    "source_refs",
]

NULLABLE_SCALAR_FIELDS = [
    "vendor_or_project",
    "system_type",
    "version_tested",
    "report_date",
    "root_cause_summary",
    "reasoning_summary",
    "resolution_status",
]


def load_card(path: Path) -> dict:
    with open(path, "r", encoding="utf-8") as f:
        raw = yaml.safe_load(f)
    if not isinstance(raw, dict):
        raise ValueError(f"expected a YAML mapping in {path}")
    return raw


def card_to_row(card: dict) -> dict:
    row = {}

    row["case_id"] = card["case_id"]
    row["title"] = card["title"]
    row["source_type"] = card["source_type"]
    row["source_name"] = card["source_name"]
    row["source_path"] = card["source_path"]
    row["short_summary"] = card["short_summary"]

    for field in NULLABLE_SCALAR_FIELDS:
        value = card.get(field)
        row[field] = str(value) if value is not None else None

    for field in JSONB_LIST_FIELDS:
        value = card.get(field) or []
        row[field] = psycopg2.extras.Json(value)

    row["card_json"] = psycopg2.extras.Json(card)

    return row


def resolve_raw_paths(raw_dir: Path) -> set:
    return {p.name for p in raw_dir.iterdir() if p.is_file()}


def source_path_matches_raw(source_path: str, raw_filenames: set) -> bool:
    return Path(source_path).name in raw_filenames


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Ingest incident-card YAML files into diagnostics.incident_cards."
    )
    parser.add_argument("--postgres-url", required=True)
    parser.add_argument(
        "--cards-dir",
        required=True,
        help="Directory containing incident-card YAML files.",
    )
    parser.add_argument(
        "--raw-dir",
        required=True,
        help="Directory containing raw source files. Only cards whose source_path "
             "resolves to a file here are inserted.",
    )
    args = parser.parse_args()

    cards_dir = Path(args.cards_dir)
    raw_dir = Path(args.raw_dir)

    if not cards_dir.is_dir():
        print(f"error: cards-dir does not exist: {cards_dir}", file=sys.stderr)
        sys.exit(1)
    if not raw_dir.is_dir():
        print(f"error: raw-dir does not exist: {raw_dir}", file=sys.stderr)
        sys.exit(1)

    raw_filenames = resolve_raw_paths(raw_dir)

    yaml_files = sorted(cards_dir.glob("*.yaml"))
    if not yaml_files:
        print(f"error: no YAML files found in {cards_dir}", file=sys.stderr)
        sys.exit(1)

    cards_to_insert = []
    skipped = []
    for yaml_path in yaml_files:
        card = load_card(yaml_path)
        source_path = card.get("source_path", "")
        if source_path_matches_raw(source_path, raw_filenames):
            cards_to_insert.append((yaml_path.name, card))
        else:
            skipped.append(yaml_path.name)

    if skipped:
        print(f"skipping {len(skipped)} card(s) with no matching raw file:", file=sys.stderr)
        for name in skipped:
            print(f"  {name}", file=sys.stderr)

    if not cards_to_insert:
        print("no cards to insert", file=sys.stderr)
        sys.exit(0)

    conn = psycopg2.connect(args.postgres_url)
    try:
        inserted = 0
        already_present = 0
        with conn:
            with conn.cursor() as cur:
                for filename, card in cards_to_insert:
                    row = card_to_row(card)
                    cur.execute(INSERT_SQL, row)
                    if cur.rowcount == 1:
                        inserted += 1
                        print(f"inserted: {card['case_id']} ({filename})")
                    else:
                        already_present += 1
                        print(f"skipped (already exists): {card['case_id']} ({filename})")
    finally:
        conn.close()

    print(
        f"\ndone: {inserted} inserted, {already_present} already present, "
        f"{len(skipped)} skipped (no raw file)"
    )


if __name__ == "__main__":
    main()
