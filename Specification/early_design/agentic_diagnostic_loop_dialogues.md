# Demo Dialogues: Agentic Diagnostic Loop over RAG + Practice Corpus

These dialogues demonstrate the target behavior for an **Agentic System over RAG + tools** in the distributed systems domain.

The important point: the assistant does not stop at “possible causes + missing data”. It continues the diagnostic loop:

```text
hypotheses
  -> one discriminating check
  -> user observation
  -> hypothesis update
  -> next check
  -> closest practical precedent
  -> containment / mitigation / follow-up actions
```

The examples assume the `practice_corpus` contains public incident reports and analyses such as:

- GitHub October 21, 2018 post-incident analysis
- Cloudflare R2 incident, February 6, 2025
- Slack incident on 2-22-22
- Fastly June 8, 2021 outage

---

## Dialogue 1: Network partition + failover + stale/inconsistent reads

### Turn 1

**User**

After a short network issue, some users started seeing stale data. We did not lose records, but read-after-write was broken for part of the system. The database is replicated across two regions.

**Assistant**

I see three plausible classes of failure:

1. **Lagging replica served reads**  
   Reads were routed to a replica that had not caught up after the network issue.

2. **Failover changed the write/read topology incorrectly**  
   A node was promoted or treated as authoritative while part of the topology still had stale state.

3. **Background workers replayed or processed stale state**  
   The data itself may be mostly intact, but async processing exposed old state to users.

The closest practical precedent so far is **GitHub October 21, 2018**, where a short network partition led to outdated and inconsistent information being displayed, while GitHub stated that no user data was ultimately lost.

**Next discriminating check**

Do not check everything yet. First check the **failover timeline**:

```text
network event timestamp
  -> failover/promote timestamp
  -> first stale read report
  -> replica lag during that window
```

This separates:

- “reads from lagging replica”
- from “wrong topology after failover”
- from “async workers exposing stale derived state”

---

### Turn 2

**User**

There was automatic failover about one minute after the network issue. Replica lag spiked, but the bigger issue is that some writes were accepted in the old primary region for a short time after another region was promoted.

**Assistant**

That strongly weakens the “simple lagging replica” hypothesis and strengthens:

> **H1: split-brain-like write acceptance / divergent write topology after failover**

Updated hypotheses:

| Hypothesis | Status | Why |
|---|---|---|
| H1: failover-induced divergent write topology | Stronger | Writes accepted in old primary after another region was promoted |
| H2: lagging replica served reads | Still possible | Replica lag spiked, but does not explain writes on old primary |
| H3: background workers exposed stale state | Secondary | Could amplify symptoms, but not primary cause |

This is now closer to the GitHub 2018 pattern: a short network partition created a much longer recovery problem involving database topology and inconsistent/outdated information.

**Next discriminating check**

Check whether the stale reads correspond to objects written during the **dual-write window**.

```text
For affected objects:
  - write timestamp
  - accepted_by_node / region
  - read source node / region
  - replication state after failover
```

If stale reads cluster around writes accepted during that window, H1 becomes the main explanation.

---

### Turn 3

**User**

Yes. Almost all affected objects were written during a 40–60 second window around failover. Reads from the new primary region did not include some of those writes until manual reconciliation.

**Assistant**

Now the likely diagnosis is:

> **Replication topology divergence after network partition and failover.**

The practical precedent is very close to **GitHub October 21, 2018**:

- short network event;
- database topology affected;
- users saw outdated/inconsistent information;
- no broad permanent data loss claimed;
- recovery required reconciliation and careful restoration of topology.

At this point I would not call this “replica lag” as the main explanation. Replica lag is a symptom. The central issue is that the system temporarily accepted writes in a topology that was no longer globally coherent.

**Next action: containment**

Immediate containment should focus on stopping further divergence:

```text
1. Freeze automatic failover for the affected cluster.
2. Force a single authoritative write leader.
3. Disable reads from uncertain replicas.
4. Pause async workers that materialize or fan out derived state.
5. Preserve binlogs/WAL/event logs from all nodes.
6. Build a reconciliation set for writes during the dual-write window.
```

**Next action: reconciliation**

```text
1. Identify all writes accepted by old primary during the disputed window.
2. Identify writes accepted by new primary during the same window.
3. Detect conflicts by object/entity key.
4. Apply deterministic conflict policy or manual review for conflict cases.
5. Replay non-conflicting writes into the chosen authoritative topology.
6. Only then re-enable normal read routing.
```

**What the assistant should store in RunState**

```json
{
  "current_best_explanation": "failover-induced divergent write topology",
  "closest_practical_precedent": "github_2018_10_21",
  "strengthened_evidence": [
    "automatic failover after network issue",
    "writes accepted in old primary after new region promoted",
    "affected objects written during failover window",
    "manual reconciliation required"
  ],
  "weakened_hypotheses": [
    "simple lagging replica reads"
  ],
  "next_phase": "containment_and_reconciliation"
}
```

---

## Dialogue 2: Object storage outage + reconnect storm during recovery

### Turn 1

**User**

Our object storage API was unavailable for almost an hour. After we started bringing it back, we got a second wave of errors. Clients reconnected aggressively and the metadata service became saturated.

**Assistant**

Initial hypotheses:

1. **Primary storage gateway outage**  
   The object storage API itself was unavailable.

2. **Recovery amplification / reconnect storm**  
   Recovery triggered many clients to reconnect at once, overloading the metadata layer.

3. **Metadata layer was already degraded independently**  
   The metadata service may have been the original bottleneck, not only a recovery victim.

4. **Internal tooling depended on the failing storage system**  
   Recovery may have been slowed by dependency inversion.

The closest practical precedent is **Cloudflare R2 incident on February 6, 2025**. Cloudflare reported that R2 operations were impacted during the primary incident window, and during the secondary incident window errors increased as clients reconnected and put pressure on R2’s metadata layer.

**Next discriminating check**

First compare:

```text
metadata saturation timeline
vs.
client reconnect rate timeline
```

This separates:

- H2: reconnect storm caused the second wave;
- from H3: metadata layer was already independently degraded.

---

### Turn 2

**User**

Metadata CPU and request queue depth were normal during the primary outage because traffic was blocked at the gateway. They spiked only when the gateway came back and clients started reconnecting.

**Assistant**

That strongly supports:

> **H2: recovery amplification caused by reconnect storm**

Updated hypotheses:

| Hypothesis | Status | Why |
|---|---|---|
| H1: primary storage gateway outage | Confirmed as phase 1 | API unavailable first |
| H2: reconnect storm overloaded metadata layer | Stronger | Metadata saturation began only after gateway returned |
| H3: metadata independently degraded | Weaker | It was normal before reconnect wave |
| H4: internal tooling dependency issue | Unknown | Need one check |

**Next discriminating check**

Check whether recovery tools, admin tools, or service control paths depended on the same object storage system.

```text
During outage:
  - could on-call disable/enable gateway normally?
  - did internal admin tooling depend on object storage?
  - were recovery scripts blocked by unavailable storage?
```

This matters because it changes the mitigation plan. If recovery tooling depends on the failing system, the system has a **self-recovery dependency problem**.

---

### Turn 3

**User**

Yes. The internal admin tool used to re-enable the gateway depended on the same storage layer. On-call had to use a lower-level workaround.

**Assistant**

Now the incident pattern is very close to **Cloudflare R2 February 6, 2025**:

```text
primary storage outage
  -> normal control/recovery path impaired
  -> manual/lower-level recovery path
  -> clients reconnect
  -> metadata layer pressure
  -> secondary error wave
```

This is no longer only an object storage outage. It is a compound reliability issue:

1. **critical dependency outage**;
2. **recovery tooling dependency inversion**;
3. **reconnect storm during recovery**.

**Immediate containment plan**

```text
1. Keep gateway recovery gradual, not all-at-once.
2. Rate-limit reconnecting clients if possible.
3. Add server-side admission control on metadata requests.
4. Prioritize internal recovery/control traffic over normal client traffic.
5. Temporarily reduce expensive metadata operations.
6. Keep a lower-level break-glass recovery path available.
```

**Follow-up engineering actions**

```text
1. Remove hard dependency of recovery tooling on the storage system it controls.
2. Add reconnect jitter/backoff guidance to clients.
3. Add metadata-layer overload protection.
4. Add staged traffic restoration.
5. Add dashboards that distinguish primary outage from recovery amplification.
6. Run a game day: object storage down + admin tool unavailable + reconnect storm.
```

**Current diagnosis**

> The closest practical case is Cloudflare R2 2025: object storage outage with secondary recovery pressure on metadata due to reconnecting clients, complicated by recovery/control-plane dependency on the affected system.

---

## Dialogue 3: Cascading failure from cache/control-plane instability

### Turn 1

**User**

During peak traffic, some cache nodes started leaving and rejoining. The config kept being regenerated, cache hit rate dropped, backend load increased, and the incident got worse gradually rather than instantly.

**Assistant**

Initial hypotheses:

1. **Cache fleet churn caused backend overload**  
   Cache instability reduced hit rate and pushed traffic to backend.

2. **Control-plane/config churn amplified the failure**  
   The system kept recomputing or propagating config in response to unstable membership.

3. **Partial rollout introduced incompatible behavior**  
   A change may have affected only part of the fleet and created inconsistent behavior.

4. **Backend capacity was insufficient without cache**  
   The cache issue may have exposed missing backend headroom.

The closest practical precedent is **Slack’s incident on 2-22-22**, which Slack described as a complex systems failure involving multiple contributing factors and a cascading failure scenario.

**Next discriminating check**

First check:

```text
cache membership events
vs.
config regeneration events
vs.
backend QPS
```

This tells us whether config churn follows cache churn, or whether config churn started first and destabilized the cache.

---

### Turn 2

**User**

Config regeneration started first after a partial rollout. Then cache nodes started cycling. Hit rate dropped after that, and backend QPS increased about 3x.

**Assistant**

Updated hypotheses:

| Hypothesis | Status | Why |
|---|---|---|
| H1: cache churn caused backend overload | Stronger | hit rate dropped and backend QPS rose 3x |
| H2: config/control-plane churn amplified failure | Stronger | config regeneration started first |
| H3: partial rollout introduced incompatible behavior | Stronger | rollout preceded config churn |
| H4: backend capacity alone was root cause | Weaker | backend overload appears downstream |

This looks like:

```text
partial rollout
  -> config/control-plane churn
  -> cache fleet instability
  -> hit rate collapse
  -> backend overload
  -> cascading degradation
```

**Next discriminating check**

Check whether rollback of the partial rollout stabilized config regeneration and cache membership.

This is the highest-value check because it distinguishes:

- rollout-triggered cascade;
- from independent cache/backend capacity failure.

---

### Turn 3

**User**

Rollback stopped config regeneration almost immediately. Cache membership stabilized over the next few minutes. Backend load dropped after cache hit rate recovered.

**Assistant**

That makes the primary pattern:

> **Partial rollout triggered control-plane/config churn, which destabilized cache membership and caused cascading backend overload.**

Closest precedent: **Slack 2-22-22**, not because every component is identical, but because the shape is the same:

```text
complex systems failure
  + multiple contributing factors
  + cascading failure
  + operational difficulty during the incident
```

**Containment plan**

```text
1. Freeze the rollout.
2. Keep config generation in a known-stable mode.
3. Prevent unstable nodes from repeatedly entering/leaving the cache pool.
4. Temporarily increase backend protection: rate limits, load shedding, queue limits.
5. Warm cache gradually before returning full traffic.
6. Monitor cache hit rate as the main recovery signal.
```

**Longer-term fixes**

```text
1. Add blast-radius limits for config-impacting rollouts.
2. Add circuit breaker around config regeneration loops.
3. Add cache membership dampening / hysteresis.
4. Add backend overload protection for cache-miss storms.
5. Add a synthetic test for partial rollout + cache churn.
6. Create a runbook: “cache hit rate collapse after config change”.
```

**Agent state update**

```json
{
  "eliminated_hypotheses": [
    "backend capacity as independent root cause"
  ],
  "current_best_explanation": "partial rollout triggered config churn and cache cascade",
  "closest_practical_precedent": "slack_2022_02_22",
  "next_phase": "containment_then_prevention"
}
```

---

## Dialogue 4: Valid customer config triggers latent edge bug

### Turn 1

**User**

One customer made a normal configuration change. Shortly after that, a large part of our edge fleet started returning errors. The config was valid. It looks like it activated some bug.

**Assistant**

Initial hypotheses:

1. **Valid config triggered latent software bug**  
   The config was legal but activated an untested code path.

2. **Bad config validation**  
   The config may have been syntactically valid but semantically unsafe.

3. **Config propagation caused global blast radius**  
   The problem may be less the bug itself and more the uncontrolled rollout.

4. **Recent deployment introduced the latent bug**  
   The customer config may only be the trigger; the bug may have arrived earlier.

The closest practical precedent is **Fastly June 8, 2021**, where an undiscovered software bug was triggered by a valid customer configuration change and caused a broad outage.

**Next discriminating check**

Check whether the same config worked before a recent software deployment.

```text
config first created/changed at T1
edge software deployment at T0
bug begins at T2
```

If the config only became dangerous after a deployment, the root cause is likely a latent software bug introduced by deployment and triggered by config.

---

### Turn 2

**User**

The feature path existed before, but a deployment three weeks ago changed its behavior. The customer config included a rare combination of options that hit that path.

**Assistant**

That strongly supports:

> **H1 + H4: latent software bug introduced by deployment, triggered by valid config.**

Updated hypotheses:

| Hypothesis | Status | Why |
|---|---|---|
| H1: valid config triggered latent bug | Stronger | rare valid option combination hit bug path |
| H2: invalid/unsafe config | Weaker | config was valid |
| H3: excessive blast radius | Still important | large edge fleet affected |
| H4: recent deployment introduced bug | Stronger | behavior changed three weeks ago |

**Next discriminating check**

Check blast-radius control:

```text
Was the config applied globally?
Was there per-customer/per-POP canarying?
Could the specific config path be disabled centrally?
How quickly did errors drop after disabling the config or feature path?
```

This distinguishes a local customer-triggered bug from a global config propagation failure.

---

### Turn 3

**User**

The config propagated globally. Disabling the specific feature path dropped errors quickly. We did not need a full rollback.

**Assistant**

The closest precedent is now clearly **Fastly June 8, 2021**:

```text
valid customer configuration
  -> latent software bug
  -> broad/global impact
  -> mitigation by disabling the triggering configuration/path
  -> recovery without necessarily rolling back everything
```

**Immediate containment plan**

```text
1. Disable the triggering feature path globally or for affected customers.
2. Stop further propagation of configs using the rare option combination.
3. Keep the broader deployment stable unless evidence says rollback is needed.
4. Add alerting for edge error rate by config feature path.
5. Preserve the exact config diff and edge crash/error logs.
```

**Post-incident engineering actions**

```text
1. Add regression tests for rare valid config combinations.
2. Add staged config rollout / canary per POP or customer segment.
3. Add blast-radius limits for config-triggered code paths.
4. Add a kill switch for high-risk feature paths.
5. Add validation that checks runtime safety, not only syntax.
6. Add practice-corpus tag: valid_config_triggers_latent_bug.
```

**Current diagnosis**

> This is not best framed as “bad customer config”. It is a **latent software defect exposed by valid configuration**, with global blast radius due to propagation controls.

---

## What these dialogues prove architecturally

The assistant is useful only if it continues after the first report.

A weak version of the system does this:

```text
problem
  -> hypotheses
  -> missing data
  -> report
```

The stronger agentic version does this:

```text
problem
  -> hypotheses
  -> choose one discriminating check
  -> interpret observation
  -> update hypotheses
  -> choose next check
  -> identify closest precedent
  -> propose containment
  -> propose follow-up fixes
```

## Required runtime components

```text
problem_ingest
symptom_characterization
practice_case_retrieval
hypothesis_builder
check_selector
observation_interpreter
hypothesis_updater
precedent_matcher
containment_planner
mitigation_planner
policy_check
```

## Required RunState fields

```text
problem_case
active_hypotheses
weakened_hypotheses
eliminated_hypotheses
supporting_evidence
contradicting_evidence
requested_checks
user_observations
closest_practical_precedent
current_best_explanation
next_best_check
containment_actions
follow_up_actions
uncertainty_notes
```

## Key design rule

The assistant should not ask for “more data” generically.

It should ask for **one next discriminating observation**:

```text
Check X first.

Why:
X distinguishes H1 from H2.

If X is true:
strengthen H1 and do Y next.

If X is false:
weaken H1 and check Z next.
```

This is the difference between a RAG answer generator and an agentic diagnostic assistant.
