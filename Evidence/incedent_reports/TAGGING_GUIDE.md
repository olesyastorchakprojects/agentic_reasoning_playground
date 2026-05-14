# Tagging Guide

This guide is for incident-report chunks in `Evidence/parsing/*/chunks/fixed_in_structural_chunks.jsonl`.

Use canonical runtime tags only:
- `chunk_role:symptom`
- `chunk_role:impact`
- `chunk_role:timeline`
- `chunk_role:symptom_change`
- `chunk_role:investigation`
- `chunk_role:diagnostic_step`
- `chunk_role:hypothesis_update`
- `chunk_role:recovery`
- `chunk_role:failure_mode`
- `chunk_role:root_cause`
- `chunk_role:contributing_factor`
- `chunk_role:uncertainty`
- `chunk_role:lesson`

## Core Rule

After `fixed_in_structural` splitting, do not keep inherited tags automatically.
Retag each smaller chunk by its own meaning.
Prefer fewer, more precise tags over a broad inherited set.

## Tag Meanings

### `chunk_role:symptom`
Use when the chunk describes what was observed.
Examples: stale reads, duplicate writes, lost updates, split-brain views, crashes, inconsistent offsets.

### `chunk_role:impact`
Use when the chunk describes consequences for users or system safety.
Examples: data loss, committed writes disappearing, customer-visible outage, silent invariant violation.

### `chunk_role:timeline`
Use when the main value is event order over time.
Examples: first X happened, then failover, then recovery, then stale reads appeared.

### `chunk_role:symptom_change`
Use when the chunk focuses on how symptoms evolved.
Examples: issue became worse after restart, primary symptom stopped but stale state remained.

### `chunk_role:investigation`
Use when the chunk explains what Jepsen, operators, or developers checked or how they narrowed the issue.
Examples: workload setup, analysis approach, evidence gathering, why a specific check was run.

### `chunk_role:diagnostic_step`
Use when the chunk gives a concrete check that helps distinguish states or verify safety.
Examples: run a workload, verify a lock with an external resource, compare observed state against a monotonic token.

### `chunk_role:hypothesis_update`
Use when the chunk explains what some result means for competing explanations.
Examples: this result suggests split-brain rather than stale cache; this narrows the bug to membership handling.

### `chunk_role:recovery`
Use when the chunk gives mitigation, workaround, upgrade, fix status, or safer operating guidance.
Examples: set replication factor to 3, enable idempotence, upgrade to fixed release, use fencing tokens.

### `chunk_role:failure_mode`
Use when the chunk describes the mechanism of failure.
Examples: lease re-check gap, missing no-op on leadership change, interleaving state-machine actions, weak write isolation by design.

### `chunk_role:root_cause`
Use only when the chunk names the concrete cause, bug, or defect with high confidence.
Examples: “caused by a missing re-entrancy check”, “caused by wrong tuple XID selection”, “Scylla traced this to a row-hash bug”.
Do not use it for tentative explanations.

### `chunk_role:contributing_factor`
Use when the chunk names something that enabled or worsened the issue but was not itself the main bug.
Examples: unsafe defaults, membership changes during recovery, clock assumptions, weak documentation, non-fault-tolerant configuration.

### `chunk_role:uncertainty`
Use when the chunk explicitly says interpretation is incomplete or unresolved.
Examples: “might”, “we suspect”, “we did not verify”, “future work”, “documentation is unclear”.

### `chunk_role:lesson`
Use when the chunk gives a general takeaway, design lesson, or operational principle.
Examples: distributed locks do not ensure safety on their own, clock-dependent databases need tighter ops discipline.

## Fast Decision Rules

Ask these in order:
1. Is this chunk mainly about what happened? Use `symptom`.
2. Is it mainly about consequences? Add `impact`.
3. Is it mainly a mechanism? Add `failure_mode`.
4. Is the exact bug explicitly identified? Upgrade to `root_cause`.
5. Is it mostly guidance, workaround, fix status, or safer configuration? Add `recovery`.
6. Is it a general takeaway rather than a fix? Add `lesson`.
7. Is the text openly uncertain? Add `uncertainty`.
8. Is it about how the issue was checked or investigated? Use `investigation` or `diagnostic_step`.

## Common Retagging Patterns

- Large parent chunk -> smaller observed anomaly chunk:
  keep `symptom`; drop explanatory tags if the child no longer contains them.

- Large parent chunk -> smaller explanation chunk:
  keep `failure_mode` or `root_cause`; drop `symptom` if no observed behavior remains.

- Recommendations / mitigations:
  usually `recovery`, sometimes `lesson + recovery`.

- Discussion sections:
  often `lesson`, sometimes `lesson + recovery`, `lesson + uncertainty`, or `lesson + impact`.

- Future work:
  usually `uncertainty`, sometimes `uncertainty + investigation`.

- Test/workload design sections:
  usually `diagnostic_step` when they define a specific verification method; otherwise `investigation`.

## Anti-Patterns

Avoid these unless the chunk truly supports them:
- `root_cause` for a merely plausible explanation.
- `symptom` on a chunk that is only recommendations or lessons.
- `recovery` on a chunk that is only abstract commentary.
- `diagnostic_step` on generic background text.
- 3+ chunk-role tags when 1-2 tags capture the chunk cleanly.

## Preferred Bias

When uncertain:
- prefer `failure_mode` over `root_cause`
- prefer `investigation` over `diagnostic_step`
- prefer `lesson` over `recovery` for abstract guidance
- prefer fewer tags over inherited tag bundles
