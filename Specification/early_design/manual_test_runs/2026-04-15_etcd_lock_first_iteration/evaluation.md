# Manual Run Evaluation: etcd lock first-iteration workflow

Query:

`We use a distributed lock in a coordination store to protect an external resource. The key-value store itself looks healthy, but sometimes two workers behave as if they both hold the lock, and after that we get conflicting updates.`

Primary matched card:

`practice_corpus/incident_cards/etcd_3_4_3_incident_card.yaml`

Card-derived context:

- Healthy key-value behavior can coexist with unsafe lock behavior.
- The user-visible failure is overlap in external critical sections, not necessarily plain key-value inconsistency.
- Lease expiry, delayed keepalive, and post-wait lock paths are plausible trigger zones.

Card-derived hypotheses:

- `lock_ownership_violation`
- `stale_lock_after_lease_change`
- `missing_lease_validity_recheck`
- `coordination_primitive_unsound_for_external_state`

Card-derived checks:

- Check whether conflicting updates cluster around lease expiry, delayed keepalive, or lock wait paths.
- Check whether the protected resource is outside the coordination store rather than inside it.
- Check whether the failure disappears if the workflow uses transactional guards on store-local state.

Selected incident chunks:

1. `evidence_for_match`: the report states that etcd locks are unsafe for protecting an external resource and can permit multiple holders and lost updates.
2. `first_check_hint`: Jepsen tested lock behavior via lease acquisition, keepalive, and lock-history validation, then checked whether an external protected set still lost updates.
3. `supporting_explanation`: the report distinguishes healthy key-value semantics from unsafe lock semantics.

Optional theory chunk:

- External-resource protection often needs fencing/version checks rather than trusting lock ownership alone.

Prompt context assembled:

`yes`

Model output shape:

`good`

Verdict:

`good`

## Success Criteria Review

1. Plausible primary precedent: `pass`

The etcd card is a strong match. The user's symptoms center on duplicate lock holders, conflicting updates, and a healthy-looking coordination store, which is exactly the practical distinction highlighted by the etcd report.

2. Card-derived context / hypotheses / checks are useful scaffolding: `pass`

The scaffolding materially shaped the answer. The model did not fall back to generic distributed-systems advice; it used the card's lock-specific framing and pulled a first check directly from the discriminating checks.

3. Chunks add evidence or nuance instead of just repeating the card: `pass_with_note`

The chunk pack was useful, especially the separation between healthy key-value semantics and unsafe lock semantics. That said, in this run the chunks mostly reinforced the card rather than widening it with an alternative context or edge condition.

4. Model returns expected response shape: `pass_with_note`

The model returned the five required sections, but as a JSON object instead of prose headings. Since we now prefer JSON for parsing, this is acceptable and arguably better, but it means the protocol wording should eventually be updated from "response shape" to "response schema".

5. Answer gives one useful discriminating first check: `pass`

The first check is strong: correlate conflicts with lock acquisition, lease renewal, lease expiry, and missed keepalive timing. This is diagnostic rather than generic and can reduce uncertainty quickly.

6. Answer preserves uncertainty and avoids final root cause claims: `pass`

The model kept the hypotheses tentative and did not assert a final diagnosis.

## Failure Mode Check

- Implausible card match: `no`
- Chunks fail to support the card: `no`
- Model ignores the card and gives generic advice: `no`
- Model overfits and states final diagnosis too early: `no`
- Evidence pack too small, causing vagueness: `no`
- Answer does not help start investigation: `no`

## Notes

- The current workflow works for this kind of tightly matched query.
- The response is slightly more specific than the user input strictly proves, because the card match is very strong and the prompt allows the model to lean on it.
- For weaker matches, we should watch whether the same prompt style causes the model to sound too confident.
