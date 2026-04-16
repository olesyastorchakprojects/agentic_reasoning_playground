# Manual Multi-Step Loop Test

Goal:

- test whether the same card-plus-chunk scheme can support more than the first response;
- check whether the model can update hypotheses after user observations;
- check whether the loop can keep giving one uncertainty-reducing next check instead of jumping to a final diagnosis.

Branch under test:

1. First response points to weak transaction-level defaults as the primary explanation.
2. User observation confirms that transaction-level concerns were not being set explicitly.
3. Next observation shows that setting strong transaction-level concerns removes most missing writes, but retry-related anomalies remain.

What this branch tests:

- whether the loop can narrow without over-committing too early;
- whether the next check changes appropriately after a supporting observation;
- whether a secondary hypothesis can survive after the primary one is partially supported.
