create schema if not exists diagnostics;

create table if not exists diagnostics.incident_cards (
    case_id text primary key,
    title text not null,
    source_type text not null,
    source_name text not null,
    source_path text not null,
    vendor_or_project text,
    system_type text,
    version_tested text,
    report_date date,
    short_summary text not null,
    canonical_symptoms jsonb not null,
    affected_components jsonb not null,
    failure_mode_candidates jsonb not null,
    observed_phases jsonb not null,
    incident_phases jsonb not null,
    turning_points jsonb not null,
    candidate_explanations jsonb not null,
    diagnostic_patterns jsonb not null,
    discriminating_checks jsonb not null,
    expected_observations jsonb not null,
    investigation_steps jsonb not null,
    root_cause_summary text,
    reasoning_summary text,
    mitigations_or_workarounds jsonb not null,
    prevention_or_design_followups jsonb not null,
    claimed_guarantees jsonb not null,
    violated_properties jsonb not null,
    resolution_status text,
    fix_versions jsonb not null,
    confidence_notes jsonb not null,
    source_refs jsonb not null,
    card_json jsonb not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    constraint incident_cards_case_id_not_blank check (length(btrim(case_id)) > 0),
    constraint incident_cards_title_not_blank check (length(btrim(title)) > 0),
    constraint incident_cards_source_type_not_blank check (length(btrim(source_type)) > 0),
    constraint incident_cards_source_name_not_blank check (length(btrim(source_name)) > 0),
    constraint incident_cards_source_path_not_blank check (length(btrim(source_path)) > 0),
    constraint incident_cards_short_summary_not_blank check (length(btrim(short_summary)) > 0),
    constraint incident_cards_canonical_symptoms_is_array check (jsonb_typeof(canonical_symptoms) = 'array'),
    constraint incident_cards_affected_components_is_array check (jsonb_typeof(affected_components) = 'array'),
    constraint incident_cards_failure_mode_candidates_is_array check (jsonb_typeof(failure_mode_candidates) = 'array'),
    constraint incident_cards_observed_phases_is_array check (jsonb_typeof(observed_phases) = 'array'),
    constraint incident_cards_incident_phases_is_array check (jsonb_typeof(incident_phases) = 'array'),
    constraint incident_cards_turning_points_is_array check (jsonb_typeof(turning_points) = 'array'),
    constraint incident_cards_candidate_explanations_is_array check (jsonb_typeof(candidate_explanations) = 'array'),
    constraint incident_cards_diagnostic_patterns_is_array check (jsonb_typeof(diagnostic_patterns) = 'array'),
    constraint incident_cards_discriminating_checks_is_array check (jsonb_typeof(discriminating_checks) = 'array'),
    constraint incident_cards_expected_observations_is_array check (jsonb_typeof(expected_observations) = 'array'),
    constraint incident_cards_investigation_steps_is_array check (jsonb_typeof(investigation_steps) = 'array'),
    constraint incident_cards_mitigations_is_array check (jsonb_typeof(mitigations_or_workarounds) = 'array'),
    constraint incident_cards_prevention_followups_is_array check (jsonb_typeof(prevention_or_design_followups) = 'array'),
    constraint incident_cards_claimed_guarantees_is_array check (jsonb_typeof(claimed_guarantees) = 'array'),
    constraint incident_cards_violated_properties_is_array check (jsonb_typeof(violated_properties) = 'array'),
    constraint incident_cards_fix_versions_is_array check (jsonb_typeof(fix_versions) = 'array'),
    constraint incident_cards_confidence_notes_is_array check (jsonb_typeof(confidence_notes) = 'array'),
    constraint incident_cards_source_refs_is_array check (jsonb_typeof(source_refs) = 'array'),
    constraint incident_cards_card_json_is_object check (jsonb_typeof(card_json) = 'object'),
    constraint incident_cards_card_json_case_id_matches check (card_json ->> 'case_id' = case_id),
    constraint incident_cards_card_json_title_matches check (card_json ->> 'title' = title),
    constraint incident_cards_card_json_source_type_matches check (card_json ->> 'source_type' = source_type),
    constraint incident_cards_card_json_source_name_matches check (card_json ->> 'source_name' = source_name),
    constraint incident_cards_card_json_source_path_matches check (card_json ->> 'source_path' = source_path),
    constraint incident_cards_card_json_short_summary_matches check (card_json ->> 'short_summary' = short_summary),
    constraint incident_cards_card_json_canonical_symptoms_matches check ((card_json -> 'canonical_symptoms') = canonical_symptoms),
    constraint incident_cards_card_json_incident_phases_matches check ((card_json -> 'incident_phases') = incident_phases),
    constraint incident_cards_card_json_discriminating_checks_matches check ((card_json -> 'discriminating_checks') = discriminating_checks),
    constraint incident_cards_card_json_expected_observations_matches check ((card_json -> 'expected_observations') = expected_observations),
    constraint incident_cards_card_json_source_refs_matches check ((card_json -> 'source_refs') = source_refs)
);

create index if not exists incident_cards_source_type_idx
    on diagnostics.incident_cards (source_type);

create index if not exists incident_cards_vendor_or_project_idx
    on diagnostics.incident_cards (vendor_or_project);

create index if not exists incident_cards_system_type_idx
    on diagnostics.incident_cards (system_type);

create index if not exists incident_cards_report_date_idx
    on diagnostics.incident_cards (report_date);

create index if not exists incident_cards_resolution_status_idx
    on diagnostics.incident_cards (resolution_status);

create index if not exists incident_cards_card_json_gin_idx
    on diagnostics.incident_cards
    using gin (card_json);

comment on schema diagnostics is
    'Namespace for canonical diagnostic assistant storage objects.';

comment on table diagnostics.incident_cards is
    'Canonical incident cards. Full card body is preserved in card_json and mirrored by selected columns.';
