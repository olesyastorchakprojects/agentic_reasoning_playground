# ASCII Architecture Evolution V2: Diagnostic Assistant over Cards + Chunks + Multi-Step Loop

Этот документ — версия 2 архитектурной эволюции.

Он учитывает не только ранние идеи, но и то, что уже удалось проверить вручную:

- first response на основе `incident card + chunk pack` работает;
- жесткий маленький chunk budget для первого ответа выглядит жизнеспособным;
- `one primary card + competing chunks` работает лучше, чем передача нескольких полных карточек;
- `Qdrant` лучше подходит для card ranking;
- `Postgres` лучше подходит для canonical card storage;
- user-facing JSON — хороший формат ответа;
- схема уже выглядит жизнеспособной не только для first response, но и для раннего multi-step diagnostic loop.

---

## 1. Main architecture idea

В версии 2 система уже не выглядит как “один большой RAG”.

Она распадается на несколько слоев:

```text
1. Card retrieval layer
2. Chunk packing layer
3. Theory retrieval layer
4. Response generation layer
5. Multi-step diagnostic loop layer
```

Ключевая идея:

```text
Cards give structure.
Chunks give evidence.
Theory gives mechanism.
Loop gives diagnostic progression.
```

---

## 2. Current best first-response architecture

Это текущий strongest supported slice.

```text
UserProblem
    |
    v
+-----------------------------+
| normalization layer         |
|-----------------------------|
| extract symptom hints       |
| component hints             |
| phase / failure-mode hints  |
+--------------+--------------+
               |
               v
+-----------------------------+
| Qdrant card retrieval       |
|-----------------------------|
| rank candidate cards        |
| return scores               |
+--------------+--------------+
               |
               v
+-----------------------------+
| candidate selection         |
|-----------------------------|
| choose 1 primary card       |
| choose 1-2 competing cards  |
+--------------+--------------+
               |
               v
+-----------------------------+
| Postgres card hydration     |
|-----------------------------|
| fetch canonical card body   |
| by case_id                  |
+--------------+--------------+
               |
               v
+-----------------------------+
| practical chunk selection   |
|-----------------------------|
| 2 primary-card chunks       |
| 1 chunk per competing card  |
| tags shape the roles        |
+--------------+--------------+
               |
               +-------------------+
               |                   |
               v                   v
+-----------------------------+   +-----------------------------+
| theory retrieval            |   | chunk packing               |
|-----------------------------|   |-----------------------------|
| optional 1 theory chunk     |   | evidence_for_match          |
| mechanism explanation       |   | first_check_hint            |
+--------------+--------------+   | alternative_context         |
               |                  | mechanism_explanation       |
               +---------+--------+-------------+---------------+
                         |                      |
                         v                      v
                   +---------------------------------------------+
                   | prepared prompt context                     |
                   |---------------------------------------------|
                   | primary card                               |
                   | compact evidence pack                      |
                   | optional theory chunk                      |
                   | response schema                            |
                   +-------------------+-------------------------+
                                       |
                                       v
                   +---------------------------------------------+
                   | model generates user-facing JSON            |
                   |---------------------------------------------|
                   | problem_understanding                       |
                   | similar_practical_context                   |
                   | active_hypotheses                           |
                   | first_check                                 |
                   | result_interpretation                       |
                   | competing_interpretation                    |
                   +-------------------+-------------------------+
                                       |
                                       v
                               First Diagnostic Response
```

---

## 3. Why this is different from “search directly in report chunks”

Система теперь опирается не на один retrieval слой, а на два разных semantic levels:

```text
incident cards  -> find incident family and main frame
report chunks   -> support / nuance / competing context
```

ASCII comparison:

```text
Reports only:

UserProblem
   |
   v
Chunk Retrieval
   |
   v
Top N text fragments
   |
   v
Model tries to infer:
- what family of incident this is
- what hypotheses matter
- what check to choose


Cards + reports:

UserProblem
   |
   v
Card Retrieval
   |
   v
Primary / competing cards
   |
   v
Chunk Retrieval within selected card neighborhood
   |
   v
Model receives:
- incident frame
- candidate hypotheses
- check structure
- supporting evidence
```

Главное отличие:

```text
Reports tell the story.
Cards extract the diagnostic logic.
```

---

## 4. Evolution path

Ниже — не абстрактная roadmap, а более реалистичная эволюция, исходя из того, что уже сработало.

---

### Stage 0. Plain RAG assistant

Это точка, от которой проект идейно стартует.

```text
UserProblem
   |
   v
Theory / report retrieval
   |
   v
Generic answer
```

Проблема этого слоя:

- нет устойчивого incident frame;
- нет structured hypotheses;
- нет одного discriminating check;
- слишком большая зависимость от случайного top-N.

---

### Stage 1. First-response diagnostic assistant

Это то, что уже подтверждено как working direction.

```text
UserProblem
   |
   v
Card Ranking (Qdrant)
   |
   v
Primary + competing cards
   |
   v
Canonical card load (Postgres)
   |
   v
Tiny evidence pack
   |
   v
User-facing JSON first response
```

Что уже выглядит жизнеспособно:

- one primary card;
- tiny role-balanced chunk pack;
- optional theory chunk;
- competing interpretation in response;
- structured interpretation of first check.

---

### Stage 2. First-response with ambiguity management

Этот слой уже частично протестирован на ambiguous transactional case.

```text
                 +------------------------------+
                 | top card candidates          |
                 |------------------------------|
                 | primary card                 |
                 | competing card A             |
                 | competing card B             |
                 +---------------+--------------+
                                 |
                                 v
                    +---------------------------+
                    | tiny mixed evidence pack  |
                    |---------------------------|
                    | 2 chunks from primary     |
                    | 1 chunk from competitor A |
                    | 1 chunk from competitor B |
                    | optional theory chunk     |
                    +---------------+-----------+
                                    |
                                    v
                        +-----------------------+
                        | first response JSON   |
                        |-----------------------|
                        | active_hypotheses     |
                        | competing_interpret.  |
                        +-----------------------+
```

Ключевой результат:

```text
chunk count does not need to grow linearly
with the number of plausible cards
```

Но только если:

- primary card anchors the response;
- competing cards contribute chunks, not full card bodies;
- prompt explicitly preserves uncertainty.

---

### Stage 3. Early multi-step diagnostic loop

Этот слой уже выглядит `promising`, хотя еще требует prompt refinement.

```text
                     +----------------------+
                     | current diagnostic   |
                     | state                |
                     |----------------------|
                     | problem frame        |
                     | active hypotheses    |
                     | current check        |
                     | interpretation rules |
                     +----------+-----------+
                                |
                                v
                          Ask user / observe
                                |
                                v
                     +----------------------+
                     | observation update   |
                     |----------------------|
                     | strengthen some      |
                     | weaken others        |
                     | keep alternatives    |
                     +----------+-----------+
                                |
                                v
                     +----------------------+
                     | next-check selector   |
                     |----------------------|
                     | choose exactly one   |
                     | next discriminating  |
                     | check                |
                     +----------+-----------+
                                |
                                v
                        Updated Diagnostic State
```

Что уже подтвердилось:

- after one observation, the model can update hypotheses;
- it can keep one next check;
- after partial support for the primary explanation, it can pivot to a still-active secondary explanation.

Что еще нужно улучшить:

- stronger plain-JSON enforcement;
- better prompt behavior after strong supporting observations;
- less conservative next-check progression.

---

### Stage 4. Mature diagnostic assistant

Это следующий логичный слой, еще не проверенный полностью.

```text
UserProblem
   |
   v
Normalization
   |
   v
Card Ranking
   |
   v
Primary + competing cards
   |
   v
Prompted first response
   |
   v
Observation loop
   |
   +--> update hypotheses
   +--> choose one next check
   +--> request missing signal
   +--> decide whether more retrieval is needed
   |
   v
Safe diagnostic guidance
```

На этом слое уже возможны:

- step-specific retrieval refresh;
- stronger state memory;
- explicit uncertainty tracking;
- transition to mitigation guidance.

---

## 5. Best current card/chunk split

Это одна из самых важных практических архитектурных идей, которые появились после тестов.

```text
Qdrant   = ranking layer for cards
Postgres = canonical card storage
Tags     = chunk packing layer
Theory   = optional mechanism layer
```

Расшифровка:

### Qdrant

Нужен для:

- ranked card retrieval;
- primary / competing candidate selection;
- score gap analysis;
- ambiguity management.

### Postgres

Нужен для:

- canonical card body;
- structured fields;
- schema evolution;
- easy reindexing of retrieval layer;
- operational maintenance of cards.

### Tags

Нужны для:

- role-aware chunk selection;
- balanced evidence packing;
- avoiding random top-N chunk trimming.

### Theory

Нужен для:

- mechanism explanation;
- generalization beyond one vendor-specific case;
- stabilizing competing interpretation.

---

## 6. Best current response shape

Текущий strongest-supported output format:

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

Что важно:

- это JSON;
- но он уже должен читаться как ответ пользователю;
- значения не должны быть internal labels;
- hypotheses должны быть user-facing English sentences;
- competing interpretation должна быть explicit and machine-readable.

---

## 7. Best current update-step shape

Для multi-step loop текущая рабочая форма ближе к такой:

```json
{
  "updated_problem_understanding": "string",
  "hypothesis_update": {
    "strengthened": ["string"],
    "weakened": ["string"],
    "still_active": ["string"]
  },
  "next_check": "string",
  "why_this_check_now": "string",
  "result_interpretation": {
    "supports_primary_if": "string",
    "supports_competing_if": "string",
    "inconclusive_if": "string | null"
  }
}
```

Это еще не финальная стабильная схема, но она уже показывает правильную логику:

```text
observation
  -> hypothesis update
  -> one next check
  -> interpretation of the next check
```

---

## 8. Current best chunk budget

Для first response:

```text
1 primary card
+ 2 primary-card chunks
+ 1 chunk from each of up to 2 competing cards
+ 0-1 theory chunk
```

Для update step:

```text
reuse current state first
add fresh retrieval only if the loop gets stuck
or if the user observation opens a new branch
```

То есть в версии 2 архитектуры важная идея такая:

```text
Do not re-run broad retrieval on every turn by default.
State should carry most of the loop.
Retrieval should refresh when needed.
```

---

## 9. Why this architecture is stronger than the original v1 idea

В первой версии эволюции система была более “общим agentic RAG loop”.

В версии 2 она стала более operationally grounded.

Главные изменения:

- cards moved to the center;
- retrieval split into card ranking vs chunk packing;
- ambiguity is explicit, not accidental;
- user-facing JSON became a design target;
- multi-step loop became a tested direction, not only a future idea.

Именно поэтому v2 выглядит ближе к реальной реализации, а не только к красивой conceptual diagram.

---

## 10. Current implementation target

Если собирать ближайший полезный slice, он должен выглядеть так:

```text
UserProblem
   |
   v
Normalization
   |
   v
Qdrant card ranking
   |
   v
Primary + competing card selection
   |
   v
Postgres card hydration
   |
   v
Tiny role-balanced chunk pack
   |
   +--> practical chunks
   +--> optional theory chunk
   |
   v
User-facing first-response JSON
   |
   v
User observation
   |
   v
Hypothesis update JSON
   |
   v
One next discriminating check
```

Это уже выглядит:

- useful;
- buildable;
- explainable;
- extensible into a fuller diagnostic assistant.

---

## 11. Final short formula

```text
Cards choose the incident family.
Chunks support and challenge it.
Theory explains the mechanism.
JSON makes the reasoning consumable.
The loop turns first response into diagnosis.
```
