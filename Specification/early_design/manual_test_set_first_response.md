# Manual Test Set: First Response Evaluation

This file contains a compact manual test set for evaluating the first-response stage of the diagnostic assistant.

Goal:
- test incident-card matching;
- test small evidence-pack usefulness;
- test whether the first response gives useful hypotheses and one good first discriminating check;
- detect premature narrowing or overly generic answers.

---

## 1. Recovery amplification after primary outage

### Query

```text
Our object storage API was almost completely down. After the service started coming back, we got a second wave of errors: clients were reconnecting aggressively, backlog started draining, and the metadata layer began to choke.
```

### What this tests

- whether the system finds the Cloudflare R2 pattern;
- whether it separates `primary outage` from `recovery amplification`;
- whether it proposes a first check around reconnect rate vs metadata saturation.

### Expected primary precedent

- `cloudflare_r2_2025_02_06`

### Good competing context

- alternative explanation that metadata degradation might be independent, not recovery-induced

---

## 2. Failover + stale/inconsistent reads

### Query

```text
After a short network problem, the database automatically moved write traffic to another region. Then users started seeing stale data and inconsistent state. The records do not seem lost, but some reads were missing recent changes.
```

### What this tests

- whether the system matches the GitHub October 21 pattern;
- whether it distinguishes simple replica lag from failover/topology divergence;
- whether it proposes a first check around failover timeline, write window, and replica lag.

### Expected primary precedent

- `github_oct21_2018`

### Good competing context

- `amazon_rds_postgresql_17_4` as a weaker competing case around reader inconsistency

---

## 3. Cache/control-plane churn -> cascading datastore overload

### Query

```text
During a rollout, cache nodes started behaving strangely: some dropped out and came back, the config was regenerated several times, cache hit rate fell, then database load spiked, and users stopped being able to log in or open sessions reliably.
```

### What this tests

- whether the system finds the Slack 2-22-22 pattern;
- whether it can retrieve the cache churn -> datastore overload -> cascading failure shape;
- whether the first check asks about cache hit rate, config churn, and dominant query path.

### Expected primary precedent

- `slack_2022_02_22`

### Good competing context

- generic database overload should not win unless backed by cache/control-plane evidence

---

## 4. Coordination primitive looks safe, but protected workflow corrupts state

### Query

```text
We use a distributed lock in a coordination store to protect an external resource. The key-value store itself looks healthy, but sometimes two workers behave as if they both hold the lock, and after that we get conflicting updates.
```

### What this tests

- whether the system finds the etcd lock-unsafety case;
- whether it distinguishes healthy key-value behavior from unsafe locking for external critical sections;
- whether it proposes a first check around lease validity, lock wait paths, and duplicate holders.

### Expected primary precedent

- `etcd_3_4_3`

### Good competing context

- `redis_raft_1b3fbf6` as a broader coordination/failover immaturity case

---

## 5. Transactions look strong, but anomalies appear under defaults or stress

### Query

```text
We thought we were using strong enough transactional guarantees, but under load and especially during network issues we started seeing strange behavior: some writes seem to disappear, sometimes reads are inconsistent, and retries after errors seem to make things even more confusing.
```

### What this tests

- whether retrieval can distinguish MongoDB, RavenDB, MySQL, and PostgreSQL style cases;
- whether the system avoids collapsing everything into one vague transaction-anomaly answer;
- whether the first check is actually discriminating:
  - defaults vs explicit transaction settings;
  - retry path;
  - isolation-level assumptions;
  - single-node vs cluster path.

### Expected primary precedent

- often `mongodb_4_2_6` or `ravendb_6_0_2`, depending on retrieval behavior

### Good competing context

- transaction/isolation cases from `mysql_8_0_34` and `postgresql_12_3`

---

## Suggested review rubric

For each manual run, inspect:

- chosen primary card;
- whether 2–3 plausible hypotheses are preserved;
- whether the first check is specific and uncertainty-reducing;
- whether the answer overfits too early to one precedent;
- whether the answer becomes generic because evidence was over-pruned.

Suggested short labels:

- `good`
- `weak`
- `overfit`
- `too_generic`
- `missing_alternative`
