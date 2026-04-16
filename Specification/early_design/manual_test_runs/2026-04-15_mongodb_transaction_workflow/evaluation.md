# Manual Run Evaluation: transactional anomaly workflow

Query:

`We thought we were using strong enough transactional guarantees, but under load and especially during network issues we started seeing strange behavior: some writes seem to disappear, sometimes reads are inconsistent, and retries after errors seem to make things even more confusing.`

Primary matched card:

`practice_corpus/incident_cards/mongodb_4_2_6_incident_card.yaml`

Card-derived context:

- A system can look acceptable under stronger-looking baseline settings and still fail once work is wrapped in transactions.
- Transaction-level settings may not inherit the safer read/write settings the operator thinks are already active.
- Partitions, timeouts, and retries can turn confusing semantics into visible anomalies such as missing acknowledged writes and inconsistent reads.

Card-derived hypotheses:

- `weak_default_read_write_concerns`
- `transaction_level_settings_override_safer_handles`
- `transaction_retry_bug`
- `network_partition_exposes_transactional_anomalies`

Card-derived checks:

- Inspect the actual effective read concern and write concern on each transaction.
- Separate anomalies that happen only during faults/retries from those that also happen in healthy conditions.
- Check whether confusing duplicate or missing effects line up with ambiguous transaction error handling.

Selected incident chunks:

1. `evidence_for_match`: transactions plus partitions can make acknowledged writes disappear.
2. `first_check_hint`: transaction-level concerns must be set explicitly and do not inherit automatically.
3. `supporting_explanation`: some error paths that look like aborts may still correspond to committed work, making retries dangerous.
4. `alternative_context`: weaker-than-expected isolation semantics can also cause anomalies even without faults.

Optional theory chunk:

- Isolation weaker than the application assumes can produce lost updates and inconsistent reads.

Prompt context assembled:

`yes`

Model output shape:

`good`

Verdict:

`missing_alternative`

## Success Criteria Review

1. Plausible primary precedent: `pass`

MongoDB is a reasonable primary card because the query explicitly mentions network issues, disappearing writes, inconsistent reads, and retries. That combination aligns well with the MongoDB report.

2. Card-derived context / hypotheses / checks are useful scaffolding: `pass`

The model clearly used the card-derived check and hypotheses. The first check is practical and uncertainty-reducing.

3. Chunks add evidence or nuance instead of just repeating the card: `pass_with_note`

The MongoDB chunks added concrete nuance around transaction-level settings and ambiguous retry/error behavior. The alternative-context chunk was included in the prompt, but the model did not really carry that uncertainty into the answer.

4. Model returns expected response shape: `pass`

The model returned the requested strict JSON object with the correct fields.

5. Answer gives one useful discriminating first check: `pass`

Inspecting the actual transaction-level read/write concern is a strong first discriminator. It can quickly tell us whether the problem is configuration-induced or whether we should escalate toward deeper retry/fault-path analysis.

6. Answer preserves uncertainty and avoids final root cause claims: `pass_with_note`

The model did not claim a final root cause, but it still anchored heavily on the MongoDB-style explanation and did not explicitly preserve the weaker competing interpretation from the alternative context.

## Failure Mode Check

- Implausible card match: `no`
- Chunks fail to support the card: `no`
- Model ignores the card and gives generic advice: `no`
- Model overfits and states final diagnosis too early: `not fully, but some narrowing risk`
- Evidence pack too small, causing vagueness: `no`
- Answer does not help start investigation: `no`

## Notes

- This run is more informative than the etcd case because the retrieval space is noisier and several cards are plausible.
- The workflow still produced a useful first response.
- However, the model mostly ignored the `alternative_context` signal. That is an early sign that one-card prompting can narrow too aggressively when multiple transactional precedents are plausible.
- If we want better uncertainty preservation, we may need either:
  - a stronger instruction to mention at least one competing interpretation when evidence is not unique; or
  - a prompt field that separates `primary_precedent` from `competing_precedent`.

## Rerun With Stronger Alternative-Context Prompt

Saved rerun artifacts:

- `model_response_rerun_with_alternative_prompt_raw.json`
- `model_response_rerun_with_alternative_prompt_clean.json`

Observed change:

- The model still used `MongoDB` as the primary practical precedent.
- But this time it explicitly preserved a broader competing hypothesis: `weaker_than_expected_isolation_level`.
- The interpretation logic also improved: if transaction-level settings are correct, the answer now keeps `retry handling or isolation semantics` alive instead of narrowing almost entirely to the MongoDB-style configuration story.

Updated reading:

- The stronger prompt wording helped.
- It did not fully force the model to mention the competing precedent in `similar_practical_context`, but it did change the hypothesis set in the intended direction.
- This suggests the prompt change is worth keeping for the first iteration workflow.

## Rerun With Explicit `competing_interpretation` Field

Saved rerun artifacts:

- `model_response_with_competing_field_raw.json`
- `model_response_with_competing_field_clean.json`

Observed change:

- The model now returns the competing view explicitly in a dedicated field:
  `Weaker-than-expected isolation semantics causing anomalies even without faults.`
- This is better than relying on a weak hint inside `active_hypotheses`.
- The main body of the answer still keeps `MongoDB` as the primary practical precedent, but the alternative is now machine-readable and easy to surface downstream.

Updated reading:

- Adding the explicit field worked better than prompt-only nudging.
- For implementation, this is a cleaner interface than trying to infer preserved uncertainty from the hypotheses list.
- The remaining weakness is that `result_interpretation` still does not directly mention how the first check affects the competing interpretation as explicitly as it could.

## Rerun With Structured `result_interpretation`

Saved rerun artifacts:

- `model_response_with_structured_interpretation_raw.json`
- `model_response_with_structured_interpretation_clean.json`

Observed change:

- The model returned the strongest version so far for downstream use:
  - one primary practical precedent,
  - one explicit competing interpretation,
  - and machine-readable interpretation rules split into `supports_primary_if` and `supports_competing_if`.
- This is easier to consume than a single free-text interpretation paragraph.

Updated reading:

- This is the cleanest schema variant tested so far.
- It preserves alternative context without making the first response too verbose.
- It also makes the next-turn orchestration easier, because the system can map user observations directly onto:
  - support for the primary precedent;
  - support for the competing interpretation;
  - unresolved ambiguity.

## Rerun With Softened Interpretation Language

Saved rerun artifacts:

- `model_response_with_structured_interpretation_softened_raw.json`
- `model_response_with_structured_interpretation_softened_clean.json`

Observed change:

- The model kept the same useful structure.
- The wording improved:
  - `supports_primary_if` now says `strengthens the explanation`
  - `supports_competing_if` now says `keeps ... alive`
- This is better aligned with first-response uncertainty than earlier wording about confirming a cause.

Updated reading:

- This is the best prompt/result combination tested so far for the first response.
- The structure is machine-friendly and the tone stays appropriately non-final.

## Rerun With Fully User-Facing JSON

Saved rerun artifacts:

- `model_response_user_facing_json_raw.json`
- `model_response_user_facing_json_clean.json`

Observed change:

- `active_hypotheses` are now written as direct English hypotheses rather than internal labels.
- The whole response can be shown to the user with little or no additional rendering.
- The JSON remains structured enough for downstream orchestration.

Updated reading:

- This is the current best end-to-end example of the first response.
- It matches the intended direction:
  - machine-readable;
  - user-readable;
  - uncertainty-preserving;
  - easy to map into the next diagnostic step.
