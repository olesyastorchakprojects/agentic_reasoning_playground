# First Iteration Design Note: Incident Card + Chunks + Model Prompt

Этот документ фиксирует текущую рабочую схему для **первой итерации** Agentic RAG diagnostic assistant.

Здесь мы рассматриваем только первый ответ системы:

```text
User describes a problem
  -> system finds one matched incident card
  -> retrieves a small evidence pack
  -> sends compact context to the model
  -> model produces the first diagnostic response
```

Мы пока **не рассматриваем полный diagnostic loop**:

```text
check -> user observation -> hypothesis update -> next check -> mitigation
```

Цель первой итерации — не поставить диагноз, а дать пользователю первую рамку расследования:

```text
problem understanding
+ similar practical context
+ active hypotheses
+ one first discriminating check
+ how to interpret the result
```

---

## 1. Core mechanism

У нас есть два разных типа данных.

### Incident card

Карточка — это structured memory об инциденте.

Она отвечает на вопрос:

```text
На какой тип инцидента это похоже?
```

Карточка даёт:

```text
- контекст;
- гипотезы;
- динамику;
- возможные проверки;
- known investigation pattern.
```

### Chunks

Chunks — это evidence-фрагменты из текстов.

Они отвечают на вопросы:

```text
Что именно подтверждает сходство?
Какая есть альтернатива?
Что может подсказать первую проверку?
Какой теоретический механизм здесь может работать?
```

---

## 2. First iteration flow

```text
UserProblem
  |
  v
Normalize user problem
  |
  v
Search incident cards
  |
  v
Select 1 matched incident card
  |
  v
Extract card context/hypotheses/checks
  |
  v
Search incident chunks
  |
  v
Search theory chunks
  |
  v
Select small evidence pack
  |
  v
Build RunState JSON / model context
  |
  v
Call model with first-response prompt
  |
  v
Show first diagnostic response to user
```

---

## 3. What we take from the incident card

From the matched card we extract three groups.

### 3.1. Context

```text
context =
  systems
  affected_components
  initial_symptoms
  later_symptoms
  timeline_events
```

These fields answer:

```text
Where did the incident happen?
What was affected?
How did the problem appear?
How did symptoms change over time?
```

Example:

```json
{
  "systems": ["R2", "Durable Objects"],
  "affected_components": ["R2 HTTP frontend", "R2 metadata layer"],
  "initial_symptoms": ["object operations impacted", "uploads/downloads failed"],
  "later_symptoms": ["clients reconnected during recovery", "metadata layer pressure"],
  "timeline_events": [
    "primary incident",
    "dependency impact",
    "reconnect surge",
    "stabilization"
  ]
}
```

---

### 3.2. Hypotheses

```text
hypotheses =
  failure_modes
  timeline_events[].hypothesis_signal
  investigation_steps[].hypothesis_update
  contributing_factors
```

These fields answer:

```text
Какие объяснения стоит держать активными?
Какие сигналы усиливали или ослабляли гипотезы в похожем инциденте?
Какие вторичные факторы могли усилить проблему?
```

Example:

```json
{
  "failure_modes": [
    "critical dependency outage",
    "recovery amplification",
    "reconnect storm",
    "metadata layer overload"
  ],
  "hypothesis_signals": [
    "critical dependency outage",
    "dependency blast radius",
    "recovery amplification"
  ],
  "contributing_factors": [
    "critical dependency for multiple products",
    "client reconnection created additional load"
  ]
}
```

---

### 3.3. Checks

```text
checks =
  investigation_steps[].question
  diagnostic_patterns[].discriminating_checks
```

These fields answer:

```text
Что можно проверить первым?
Какая проверка различает активные гипотезы?
```

Example:

```json
{
  "investigation_questions": [
    "Is the storage subsystem itself corrupted or unavailable?",
    "Are dependent services failing because they depend on R2?",
    "Did recovery create a second-order load problem?"
  ],
  "discriminating_checks": [
    "Compare reconnect rate with metadata-layer saturation during recovery.",
    "Check whether secondary errors begin after service restoration starts.",
    "Check whether metadata queue depth or CPU rises after client reconnects."
  ]
}
```

---

## 4. What the card should not do

The matched card must not become a diagnosis.

Wrong:

```text
Card matched -> root cause found.
```

Right:

```text
Card matched -> useful precedent -> candidate hypotheses -> first check.
```

The card is a structured prior, not proof.

---

## 5. Incident chunk retrieval

For the first iteration we use one observed-behavior query.

Example:

```text
"object storage API unavailable, secondary errors during recovery, clients reconnecting, metadata layer saturation"
```

This query is used twice.

### 5.1. Search with document_id filter

```text
Search only inside the document linked to the matched incident card.
```

Purpose:

```text
Find evidence that explains why this matched incident is actually relevant.
```

This gives candidate chunks for:

```text
evidence_for_match
first_check_hint
```

---

### 5.2. Search without document_id filter

```text
Search across the whole incident/practice corpus.
```

Purpose:

```text
Find alternative context or competing patterns.
```

This gives candidate chunks for:

```text
alternative_context
possibly first_check_hint
```

---

## 6. Why we do not send all retrieved chunks to the model

Retrieval results are candidates, not final context.

Even if we retrieve:

```text
4 chunks from matched document
+ 4 chunks from global incident search
```

the model should not necessarily receive all 8.

The system must build a small evidence pack.

For the first response, the evidence pack should usually contain:

```text
3–4 incident chunks total
1–2 theory chunks total
```

The goal is not to maximize context.

The goal is to give the model just enough support to produce the first useful diagnostic response.

---

## 7. Incident chunk roles

For the first iteration we select incident chunks by function.

We need only three evidence roles.

```text
evidence_for_match
first_check_hint
alternative_context
```

---

### 7.1. evidence_for_match

Purpose:

```text
Show why the matched incident card is actually similar to the user problem.
```

Preferred chunk tags:

```text
symptom
impact
timeline
symptom_change
```

Why:

```text
This chunk should show similar observed behavior or similar symptom progression.
```

Example use:

```text
The user says: API unavailable + secondary recovery errors.
The chunk shows: primary outage + secondary recovery pressure.
```

---

### 7.2. first_check_hint

Purpose:

```text
Help choose the first discriminating check.
```

Preferred chunk tags:

```text
investigation
diagnostic_step
hypothesis_update
recovery
timeline
```

Why:

```text
The first check usually comes from what changed the understanding of the incident:
what was observed, compared, ruled out, or checked during recovery.
```

Example use:

```text
If the chunk shows reconnects preceded metadata saturation,
the first check can be:
compare reconnect rate with metadata saturation.
```

---

### 7.3. alternative_context

Purpose:

```text
Prevent premature narrowing to one incident pattern.
```

Preferred chunk tags:

```text
failure_mode
root_cause
contributing_factor
uncertainty
lesson
```

Why:

```text
This chunk should show that similar symptoms can arise in another context or from another mechanism.
```

Example use:

```text
A similar outage could be caused by bad config rollout rather than recovery amplification.
```

---

## 8. Minimal tag table

| Evidence role | Preferred chunk tags |
|---|---|
| `evidence_for_match` | `symptom`, `impact`, `timeline`, `symptom_change` |
| `first_check_hint` | `investigation`, `diagnostic_step`, `hypothesis_update`, `recovery`, `timeline` |
| `alternative_context` | `failure_mode`, `root_cause`, `contributing_factor`, `uncertainty`, `lesson` |

Simplified MVP version:

```text
evidence_for_match -> symptom / timeline
first_check_hint -> investigation / recovery
alternative_context -> failure_mode / root_cause
```

---

## 9. How chunks should be tagged

Chunk tags should be created during ingestion, not at runtime.

Recommended chunk metadata:

```json
{
  "chunk_id": "...",
  "document_id": "...",
  "section_title": "Recovery",
  "primary_role": "recovery",
  "chunk_roles": [
    "timeline",
    "symptom",
    "hypothesis_update",
    "recovery"
  ]
}
```

A chunk can have multiple roles.

Do not force one `chunk_type`.

Use:

```text
primary_role
chunk_roles[]
```

because one chunk can contain a mix of symptoms, timeline, mitigation and hypothesis updates.

---

## 10. How to assign chunk tags during ingestion

Start simple.

### 10.1. By section title

```text
Impact / Customer impact
  -> symptom, impact

Timeline
  -> timeline, symptom_change

Root cause
  -> root_cause, failure_mode

Mitigation / Resolution / Recovery
  -> mitigation, recovery

What went wrong
  -> contributing_factor, failure_mode

Investigation / Detection
  -> investigation, diagnostic_step

Follow-up / Action items
  -> prevention, action_items

Lessons learned
  -> lesson, uncertainty
```

### 10.2. From incident narrative extraction

If the ingestion pipeline extracts narrative structures:

```text
timeline_events
investigation_steps
mitigation_steps
diagnostic_patterns
```

then related chunks can receive matching roles:

```text
timeline_events -> timeline
investigation_steps -> investigation, diagnostic_step, hypothesis_update
mitigation_steps -> mitigation, recovery
diagnostic_patterns -> failure_mode, diagnostic_step
```

### 10.3. LLM tagging as fallback

If the document has poor structure or mixed sections, an LLM can assign:

```text
primary_role
chunk_roles[]
confidence
```

But for MVP, start with:

```text
structural chunks + metadata + fixed evidence roles
```

This is cheaper, more reproducible and easier to debug.

---

## 11. Theory corpus retrieval

Theory chunks are not used to find the practical incident.

They are used to explain the mechanism.

For first iteration, theory chunks answer questions like:

```text
Why can retries/reconnects amplify load?
Why can dependency outage create cascading failure?
Why can metadata/control plane saturation create secondary errors?
```

Budget:

```text
1–2 theory chunks
```

Theory chunks are optional in the first response.

If the user wants only a practical next check, theory can be omitted.

---

## 12. What we send to the model

The model should receive compact prepared context, not raw conversation history and not all retrieved chunks.

Recommended first-call context:

```json
{
  "task": "draft_initial_diagnostic_response",
  "user_problem": "...",
  "normalized_incident_query": {
    "recognized_canonical_symptoms": [],
    "unmapped_user_symptoms": [],
    "affected_components": [],
    "failure_mode_candidates": [],
    "observed_phase": [],
    "signals_present": [],
    "missing_signals": []
  },
  "matched_incident_card": {
    "case_id": "...",
    "source_name": "...",
    "title": "...",
    "context": {
      "systems": [],
      "affected_components": [],
      "initial_symptoms": [],
      "later_symptoms": [],
      "timeline_events": []
    },
    "hypotheses": {
      "failure_modes": [],
      "hypothesis_signals": [],
      "hypothesis_updates": [],
      "contributing_factors": []
    },
    "checks": {
      "investigation_questions": [],
      "discriminating_checks": []
    }
  },
  "incident_evidence_chunks": [
    {
      "role": "evidence_for_match",
      "source_document_id": "...",
      "chunk_tags": ["symptom", "timeline"],
      "text": "..."
    },
    {
      "role": "first_check_hint",
      "source_document_id": "...",
      "chunk_tags": ["investigation", "recovery"],
      "text": "..."
    },
    {
      "role": "alternative_context",
      "source_document_id": "...",
      "chunk_tags": ["failure_mode", "root_cause"],
      "text": "..."
    }
  ],
  "theory_chunks": [
    {
      "role": "mechanism_explanation",
      "source_document_id": "...",
      "text": "..."
    }
  ],
  "policy_constraints": [
    "do not claim final root cause",
    "give exactly one first check",
    "do not suggest destructive actions",
    "state uncertainty"
  ]
}
```

---

## 13. What the model should return

The model should return the first diagnostic response to the user.

It should contain exactly these sections.

```text
1. Problem understanding
2. Similar practical context
3. Active hypotheses
4. First check
5. How to interpret the result
```

The first response is not a final report.

It is a starting frame for investigation.

---

## 14. First model prompt

```text
You are a diagnostic assistant for distributed systems incidents.

Your task:
Produce the first diagnostic response to the user.

Do not give a final root cause.
Do not list many checks.
Give exactly one first discriminating check.

Input contains:
1. user_problem
2. normalized_incident_query
3. matched_incident_card
4. incident_evidence_chunks
5. theory_chunks
6. policy_constraints

Use the matched incident card as a structured precedent, not as proof.
Use chunks as supporting evidence and nuance.
If evidence is insufficient, say so.
If the evidence pack contains `alternative_context`, preserve that uncertainty:
- do not collapse too early to one incident story;
- keep at least one plausible competing interpretation active when the match is not unique;
- use the single first check to help separate the primary precedent from the competing interpretation.

Your response must contain exactly these sections:

1. Problem understanding
Briefly restate how you understood the user's problem.

2. Similar practical context
Explain what kind of incident pattern this resembles.

3. Active hypotheses
List 2–3 hypotheses. Keep them tentative.
If a plausible competing interpretation exists, at least one hypothesis should preserve it in compact form.

4. First check
Give exactly one next check.

5. How to interpret the result
Explain how different outcomes strengthen or weaken the hypotheses.
If there is a competing interpretation, make it clear which outcome strengthens the primary precedent and which outcome keeps the competing interpretation alive.

6. Competing interpretation
If a plausible competing interpretation exists, give one compact alternative explanation.
If no plausible competing interpretation exists, return `null` in structured form or omit this section in prose form.

Constraints:
- Do not claim root cause.
- Do not suggest destructive actions.
- Do not overload the user with a checklist.
- Prefer uncertainty-reducing reasoning.
- Keep the answer compact.
- If evidence is not unique, do not present the primary card as the only plausible explanation.
```

Then append the JSON context described above.

---

## 15. Expected first response shape

Example response shape:

```text
Problem understanding

I understood this as: object storage API was mostly unavailable, and during recovery a second wave of errors appeared while the metadata layer became saturated.

Similar practical context

This resembles a critical dependency outage followed by recovery amplification: the primary service begins to recover, clients reconnect or retry, and the recovery traffic creates secondary pressure on the metadata/control layer.

Active hypotheses

H1: The primary API/storage layer was unavailable.
H2: Recovery traffic amplified the incident through reconnects/retries/backlog drain.
H3: The metadata layer may also have been an independent bottleneck rather than only a recovery-induced effect.

First check

Compare reconnect/retry rate with metadata-layer saturation during the recovery window.

How to interpret the result

If reconnect/retry rate spikes before metadata CPU, queue depth or latency rises, H2 becomes stronger.
If metadata saturation starts before reconnect/retry traffic increases, H3 becomes stronger.
If both begin right after service restoration, recovery amplification plus backlog drain is likely, but not yet a final root cause.

Competing interpretation

The metadata/control layer may have been an independent bottleneck that the recovery only exposed more clearly, rather than a purely recovery-induced overload.
```

Important reading of this shape:

```text
The first response should still have one primary practical precedent.

But if the evidence pack includes alternative_context,
the answer should preserve one compact competing interpretation
instead of speaking as if the primary precedent is already confirmed.
```

Recommended JSON response schema for implementation:

```json
{
  "problem_understanding": "string",
  "similar_practical_context": "string",
  "active_hypotheses": ["string", "string"],
  "first_check": "string",
  "result_interpretation": {
    "supports_primary_if": "string",
    "supports_competing_if": "string",
    "inconclusive_if": "string | null"
  },
  "competing_interpretation": "string | null"
}
```

Notes:

```text
- `competing_interpretation` is not a second full diagnosis.
- It is one compact alternative explanation worth keeping alive.
- If evidence is strongly one-sided, `competing_interpretation` can be null.
- If `alternative_context` is present in the evidence pack, the default expectation is that this field should be non-null unless the competing context is clearly ruled out by the prepared evidence.
- `result_interpretation` is better as a structured object than a single paragraph, because downstream code can separately surface:
  - what outcome strengthens the primary precedent;
  - what outcome keeps the competing interpretation alive;
  - what outcome is still inconclusive.
- These fields should use uncertainty-preserving language such as:
  - `strengthens the primary explanation`
  - `keeps the competing interpretation alive`
  - `remains inconclusive`
- Avoid phrases like `confirms the root cause` or `proves the diagnosis` in the first response.
```

---

## 16. What we intentionally do not do in the first iteration

We do not:

```text
- diagnose final root cause;
- run full multi-turn diagnostic loop;
- retrieve chunks from all corpora;
- send dozens of chunks to the model;
- list many checks;
- suggest destructive actions;
- use the matched card as proof;
- make the model remember state from raw chat history.
```

We do:

```text
- normalize the user problem;
- find one useful matched incident card;
- extract structured context/hypotheses/checks from it;
- retrieve a small number of incident chunks;
- retrieve optional theory chunks;
- send compact context to the model;
- ask for one first discriminating check.
```

---

## 17. Key budgets for first iteration

```text
matched cards:
  1 primary card

incident chunks:
  3–4 total

theory chunks:
  0–2 total

model context:
  compact RunState JSON
  selected evidence chunks
  first-response prompt
```

If more than this is needed, it probably means the system has moved beyond the first iteration and should start the diagnostic loop.

---

## 18. Final short formula

```text
Card = structured frame.

Incident chunks =
  evidence for match
  first-check hint
  alternative context.

Theory chunks =
  mechanism explanation.

Model =
  turns prepared context into the first diagnostic response.

First response =
  understanding
  similar context
  hypotheses
  one check
  interpretation rules
  + preserved uncertainty when alternative_context exists
  + one compact competing interpretation when needed.
```
