# Prompt Context Assembly

This document explains what is sent to the model before generation.

The runtime does not send all retrieved material to the model. It builds a small context from:

- the user query or continuation observation;
- the top incident report compressed into a structure;
- selected chunks from the top-1 incident report;
- selected chunks from lower-ranked incident reports;
- selected theory chunks;
- the response schema and policy constraints from the prompt asset.

## Initial diagnostic response

The initial flow is used for the first diagnostic answer after the user reports a problem.

Before this step, the pipeline has already selected the primary (top-1) incident report, up to 2 lower-ranked alternative incident reports, chunks from those reports, and theory chunks. Context assembly does not retrieve new data. It only decides what part of that prepared material enters the final prompt.

### Table 1. What enters the model context

| Context block | Prompt assembly input | Selected into the prompt |
|---|---|---|
| User problem |  | Normalized user query text |
| Structured user query | `StructuredUserQuery` | Selected fields needed for matching and reasoning |
| Primary incident report summary | The top-ranked incident report | Selected fields needed for matching and reasoning |
| Response schema and policy constraints |  |  |
| Chunks from the primary incident report | 12 retrieved chunks | 3 |
| Chunks from alternative incident reports | 12 retrieved chunks | 2 total, max 1 per report |
| Theory chunks | 12 retrieved chunks | 1 |

The prompt does not receive the raw incident-report PDF directly. Incident reports are preprocessed offline into structured `IncidentCard` records and stored in PostgreSQL; at request time, the pipeline retrieves the matched card from the database, and prompt assembly derives a smaller prompt-facing `matched_incident_card` object from it. The user query is processed at request time into a `StructuredUserQuery`, but the prompt receives only a reduced view built from the fields needed for matching and reasoning, rather than the full query-structuring output.


### Table 2. Initial flow chunk packing

Chunks are selected by role. Each prompt role looks for a chunk that can serve a specific purpose in the final context, such as grounding the match, suggesting the first check, or adding explanation. A chunk qualifies for a role through its tags, and the tags for that role are ordered by priority: tag 1 is preferred over tag 2, tag 2 over tag 3, and so on. When several chunks are still plausible after tag priority is applied, score is used to break the tie.

| Role | Meaning | Source | Tags |
|---|---|---|---|
| `evidence_for_match` | Shows why the top incident report matches the user problem. | `primary_incident` | 1. `chunk_role:symptom`: observed behavior such as stale reads, duplicate writes, lost updates, split-brain views, crashes, inconsistent offsets.<br>2. `chunk_role:failure_mode`: mechanism of failure such as lease re-check gap, missing no-op on leadership change, weak write isolation.<br>3. `chunk_role:contributing_factor`: condition that enabled or worsened the issue, but was not the main bug. |
| `first_check_hint` | Helps choose the first diagnostic check. | `primary_incident` | 1. `chunk_role:diagnostic_step`: concrete check that distinguishes states or verifies safety.<br>2. `chunk_role:investigation`: what was checked or how the issue was narrowed.<br>3. `chunk_role:lesson`: general takeaway, design lesson, or operational principle. |
| `supporting_explanation` | Adds useful explanation from the top incident report. | `primary_incident` | 1. `chunk_role:failure_mode`: mechanism of failure.<br>2. `chunk_role:contributing_factor`: condition that enabled or worsened the issue.<br>3. `chunk_role:uncertainty`: incomplete, unresolved, or explicitly uncertain interpretation.<br>4. `chunk_role:hypothesis_update`: result that changes competing explanations. |
| `alternative_context` | Shows competing explanations from lower-score incident reports. | `alternative_incident` | 1. `chunk_role:symptom`: observed behavior.<br>2. `chunk_role:failure_mode`: mechanism of failure.<br>3. `chunk_role:uncertainty`: incomplete, unresolved, or explicitly uncertain interpretation. |
| `mechanism_explanation` | Explains the distributed-systems mechanism. | `theory` | Theory chunks do not use incident-report tags. |

Initial flow max packed chunks:

```text
1 evidence_for_match
+ 1 first_check_hint
+ 1 supporting_explanation
+ 2 alternative_context
+ 1 mechanism_explanation
= 6 chunks max
```

## Diagnostic update / continuation flow

The continuation flow is used when the user adds new information after the first diagnostic answer.

This flow does not ask the model to start over. It sends the current diagnostic state, the new observation, the top incident report summary, and a fresh packed chunk set.

### Table 3. What enters the continuation model context

| Context block | Prompt assembly input | Selected into the prompt |
|---|---|---|
| Current problem understanding | `DiagnosticContext` | Current problem understanding text |
| New observation | `ResolvedObservation` | Resolved from prior context observation text |
| Structured observations | `ObservationExtractionOutput` | Selected fields needed for matching and reasoning |
| Active hypotheses | `DiagnosticContext` | Active hypotheses |
| Rejected hypotheses | `DiagnosticContext` | Rejected hypotheses with rejection reasons |
| Last check | Previous suggested check | The previous check, if one exists |
| Primary incident report summary | The top-ranked `IncidentCard` | Selected fields grouped as `context`, `hypotheses`, and `checks` |
| Chunks from the primary incident report | 12 retrieved chunks | 3 |
| Chunks from alternative incident reports | 12 retrieved chunks | 2 total, max 1 per report |
| Theory chunks | 12 retrieved chunks | 1 |
| Response schema and policy constraints | Prompt asset | Full response schema and all policy constraints |

Here, `DiagnosticContext` means the structured carry-over of the ongoing diagnostic case rather than raw chat history: the current problem understanding, the live and rejected hypotheses, and the last suggested check. The new user input also enters the continuation prompt twice: first as one resolved observation text, and then as a structured set of extracted observations used for updating hypotheses and choosing the next check.


### Table 4. Continuation flow chunk packing

The continuation flow uses the same packing structure as the initial flow, but the check role is named `next_check_hint` because the model is choosing the next check after seeing a new observation.

| Role | Meaning | Source | Tags |
|---|---|---|---|
| `evidence_for_match` | Keeps the top incident report grounded against the current problem. | `primary_incident` | 1. `chunk_role:symptom`: observed behavior.<br>2. `chunk_role:symptom_change`: how symptoms evolved over time.<br>3. `chunk_role:failure_mode`: mechanism of failure.<br>4. `chunk_role:investigation`: what was checked or how the issue was narrowed.<br>5. `chunk_role:hypothesis_update`: result that changes competing explanations. |
| `next_check_hint` | Helps choose the next diagnostic check. | `primary_incident` | 1. `chunk_role:diagnostic_step`: concrete check that distinguishes states or verifies safety.<br>2. `chunk_role:investigation`: what was checked or how the issue was narrowed.<br>3. `chunk_role:hypothesis_update`: result that changes competing explanations.<br>4. `chunk_role:lesson`: general takeaway, design lesson, or operational principle. |
| `supporting_explanation` | Adds useful explanation for the update. | `primary_incident` | 1. `chunk_role:failure_mode`: mechanism of failure.<br>2. `chunk_role:contributing_factor`: condition that enabled or worsened the issue.<br>3. `chunk_role:uncertainty`: incomplete, unresolved, or explicitly uncertain interpretation.<br>4. `chunk_role:hypothesis_update`: result that changes competing explanations. |
| `alternative_context` | Shows competing explanations from lower-score incident reports. | `alternative_incident` | 1. `chunk_role:symptom`: observed behavior.<br>2. `chunk_role:failure_mode`: mechanism of failure.<br>3. `chunk_role:uncertainty`: incomplete, unresolved, or explicitly uncertain interpretation.<br>4. `chunk_role:diagnostic_step`: concrete check that distinguishes states or verifies safety. |
| `mechanism_explanation` | Explains the distributed-systems mechanism. | `theory` | Theory chunks do not use incident-report tags. |

Continuation flow max packed chunks:

```text
1 evidence_for_match
+ 1 next_check_hint
+ 1 supporting_explanation
+ 2 alternative_context
+ 1 mechanism_explanation
= 6 chunks max
```

In the continuation prompt, selected incident and theory chunks are serialized as text grouped by role. Chunk ids, scores, tags, raw run state, and internal step metadata stay outside the prompt context.
