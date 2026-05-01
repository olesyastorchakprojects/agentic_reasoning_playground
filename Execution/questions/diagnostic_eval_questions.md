# Diagnostic Eval Questions

Ниже набор вопросов для ручных прогонов приложения. Я специально смешала:
- точные запросы, хорошо совпадающие с корпусом;
- размытые запросы, где важнее нормализация и удержание неопределенности;
- частично покрытые кейсы, где retrieval/reasoning должны работать аккуратнее.

| ID | Query | Query Shape | Corpus Fit | Expected Difficulty | What It Tests |
| --- | --- | --- | --- | --- | --- |
| Q1 | We use a distributed lock in a coordination store to protect an external resource. The key-value store itself looks healthy, but sometimes two workers behave as if they both hold the lock, and after that we get conflicting updates. | Exact, symptom-rich | Very high, directly aligned with incident reports | Low | Golden-path lock-safety case; checks strong primary-card matching and whether the answer proposes a discriminating first check. |
| Q2 | Our lock service seems fine at the read/write level, but after lease expiration or lock contention two clients may both proceed as if they still own the same lock. | Exact but paraphrased | High | Low | Tests paraphrase robustness for the etcd-style unsafe-lock precedent and whether lease/wait-path evidence is surfaced. |
| Q3 | We thought we were using strong enough transactional guarantees, but under load and especially during network issues we started seeing strange behavior: some writes seem to disappear, sometimes reads are inconsistent, and retries after errors seem to make things even more confusing. | Exact, symptom-rich | Very high, directly aligned with incident reports | Medium | Golden-path transaction case; checks whether the system brings up transaction-level settings, retry ambiguity, and competing interpretations. |
| Q4 | Transactions are enabled, writes are acknowledged, but after failures and retries the final state sometimes looks as if one of the commits vanished. | Partial, compact | High | Medium | Tests whether retrieval still finds the MongoDB-like precedent when the query is shorter and omits some of the original phrasing. |
| Q5 | During network partitions the database stays up, but clients disagree about recent values and we are not sure whether this is stale reads, weak isolation, or something in our retry logic. | Ambiguous, multi-cause | Medium | Medium-High | Tests uncertainty handling, hypothesis quality, and whether the model avoids overcommitting when multiple precedents are plausible. |
| Q6 | We are using a consensus-backed store and the cluster looks healthy, but the safety property around an external resource is broken even though ordinary key-value operations look fine. | Partial, abstract | Medium-High | Medium | Tests whether the system can bridge from abstract phrasing to the “KV healthy, higher-level coordination unsafe” pattern. |
| Q7 | Under concurrency the app behaves as if a transaction observed state that should not have been visible yet, but we only notice it intermittently and mostly after retries. | Fuzzy, symptom-level | Medium | High | Stresses query structuring and theory selection when the user describes an anomaly without naming the storage system or exact guarantee. |
| Q8 | Our cluster has periodic latency spikes and after that operators report duplicate workers, but we do not know whether the lock implementation is wrong or whether the workers are acting on stale state. | Ambiguous with competing explanations | Medium | High | Tests whether competing interpretationsWe thought we were using strong enough transactional guarantees, but under load and especially during network issues we started seeing strange behavior: some writes seem to disappear, sometimes reads are inconsistent, and retries after errors seem to make things even more confusing. are preserved instead of collapsing too early to one lock-safety story. |
| Q9 | We wrapped a previously stable workflow in transactions and only then started seeing missing effects and confusing retry outcomes. | Partial, causal hint present | High | Medium | Good check for the “transactions made guarantees weaker in practice” story and whether the first check targets effective transaction settings. |
| Q10 | Something about our distributed coordination is off: the storage layer passes health checks, but higher-level invariants still break under load. | Very vague | Low-Medium | High | Adversarial vague query; useful for judging whether the system stays honest, surfaces uncertainty, and avoids overclaiming. |

## Suggested Usage Notes

| Batch | Recommended Questions | Why |
| --- | --- | --- |
| Basic sanity | Q1, Q3 | Fast regression check for the two strongest precedent-backed paths. |
| Retrieval robustness | Q2, Q4, Q9 | Same families as strong corpus matches, but phrased less directly. |
| Uncertainty quality | Q5, Q8, Q10 | Useful for checking whether the model keeps competing interpretations alive. |
| Query-structuring stress | Q6, Q7, Q10 | Good for testing normalization when the user is abstract, vague, or symptom-first. |

## What To Watch In Outputs

- Does `query_structuring` preserve the real failure mode instead of collapsing everything into generic “network issue” language?
- Does retrieval pick the right primary precedent when the wording is paraphrased?
- Does the answer keep uncertainty alive when the query is genuinely ambiguous?
- Is the `first_check` actually discriminating, not just a generic health check?
- Does the response avoid inventing unsupported hypotheses when corpus coverage is partial?
