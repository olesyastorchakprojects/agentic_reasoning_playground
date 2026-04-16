# Эволюция проекта: Agentic System over RAG + tools для разбора проблем в distributed systems

Ниже — обновлённая версия документа. Главное изменение: центральной единицей приложения становится не `SafeDiagnosticBrief`, а **итеративный diagnostic loop**.

Приложение должно не только находить похожие документы и предлагать первые проверки, а вести пользователя через расследование:

```text
problem
  -> hypotheses
  -> one discriminating check
  -> user observation
  -> hypothesis update
  -> next check or mitigation
```

Проект строится над уже существующим RAG-приложением и использует существующие retrieval-коллекции как теоретический слой. Новый practical layer добавляется постепенно.

---

## 1. Корпус знаний

На старте проект разделяет источники на несколько логических корпусов.

| Корпус | Содержание | Роль в системе | Статус для MVP |
|---|---|---|---|
| `theory_corpus` | Книги, учебные главы, conceptual explanations по distributed systems | Объясняет принципы, модели, trade-offs и базовые понятия | Уже частично есть в текущем RAG-приложении |
| `practice_corpus` | Incident reports, postmortems, Jepsen analyses, debugging case studies | Даёт реальные примеры отказов, симптомов, причин, mitigations и lessons learned | Входит в MVP первым |
| `procedure_corpus` | Runbooks, troubleshooting guides, incident response playbooks | Помогает предложить конкретные проверки и порядок действий | Добавляется после MVP |
| `pattern_corpus` | Reliability patterns, architecture guides, design mitigation docs | Помогает переходить от диагностики к design-level improvements | Добавляется позже |

MVP intentionally starts with `practice_corpus`, because именно он добавляет слой практичности: реальные случаи, реальные симптомы, реальные причины, реальные способы расследования и восстановления.

---

## 2. Главный сдвиг в архитектуре

Старая формула:

```text
question
  -> retrieval
  -> answer
  -> report
```

Промежуточная формула:

```text
problem
  -> symptoms
  -> similar incidents
  -> hypotheses
  -> checks
  -> diagnostic report
```

Новая правильная формула:

```text
problem
  -> symptoms
  -> hypotheses
  -> discriminating check
  -> observation
  -> hypothesis update
  -> next action
  -> mitigation or deeper investigation
```

То есть `missing_data` и `first checks` — не конец ответа. Это вход в настоящий agentic loop.

---

## 3. Таблица эволюции

| Уровень | Что добавляется | Где появляется ИИ | Как это можно сделать без ИИ | Чем вариант без ИИ хуже | Что это даёт пользователю |
|---|---|---|---|---|---|
| 0 | **Бейзлайн:** пользователь задаёт вопрос по distributed systems → система ищет ответ в `theory_corpus` | Retrieval + генерация ответа по найденным чанкам | Keyword search + ручное чтение книги | Плохо ловит смысловые совпадения, не связывает вопрос с близкими концептами | Пользователь быстро получает объяснение понятия или механизма |
| 1 | **Problem ingest:** пользователь описывает практическую проблему | ИИ помогает нормализовать свободный текст в структурированный `ProblemCase` | Форма с жёсткими полями | Пользователю сложнее описывать проблему; система хуже принимает неполный ввод | Проблема превращается в рабочий объект расследования |
| 2 | **Symptom characterization:** выделение симптомов: latency spike, stale reads, timeout, retry storm, split-brain, replication lag, inconsistent state | ИИ полезен для извлечения симптомов из естественного языка | Regex/rules по ключевым словам | Пропускает синонимы, неявные симптомы и смешанные случаи | Пользователь видит, как система поняла проблему |
| 3 | **Поиск релевантных концептов в `theory_corpus`** | RAG связывает симптомы с теоретическими понятиями | Keyword search по терминам | Работает только если пользователь уже знает правильные термины | Система объясняет, какие принципы могут быть задействованы |
| 4 | **Первый `practice_corpus`: поиск похожих incident reports / Jepsen cases / postmortems** | Семантический поиск по описаниям реальных случаев | Ручной поиск по тегам, названию системы или ключевым словам | Плохо находит похожие случаи при разной терминологии | Пользователь получает practical precedent memory, а не только теорию |
| 5 | **Evidence bundle:** теория + похожие практические случаи | ИИ помогает сгруппировать найденные фрагменты по симптомам, failure modes и evidence | Механическое объединение top-k chunks | Результаты шумные, связи между источниками не объяснены | Пользователь получает организованную evidence-карту |
| 6 | **Черновик гипотез:** система предлагает 2–5 возможных failure modes | ИИ синтезирует гипотезы из симптомов и evidence | Жёсткая таблица symptom → failure_mode | Плохо работает на комбинированных случаях и неполном описании | Пользователь получает стартовую структуру расследования |
| 7 | **Missing data detection:** система явно говорит, чего не хватает | ИИ помогает определить, какие данные нужны для различения гипотез | Жёсткий список обязательных полей | Слишком грубо: либо требует всё подряд, либо пропускает важные пробелы | Система не притворяется уверенной |
| 8 | **Discriminating check selection:** система выбирает одну следующую проверку, которая лучше всего различает гипотезы | ИИ становится полезен как механизм выбора следующего действия | Статический checklist | Проверок слишком много, пользователь не понимает, с чего начать | Пользователь получает не список всего возможного, а следующий лучший шаг |
| 9 | **Observation intake:** пользователь приносит результат проверки: метрику, лог, timeline, конфиг, факт rollback | ИИ помогает интерпретировать наблюдение в контексте активных гипотез | Ручная интерпретация пользователем | Пользователь сам должен понять, что результат усиливает или ослабляет | Расследование становится диалоговым |
| 10 | **Hypothesis update:** система усиливает, ослабляет или отбрасывает гипотезы | ИИ сопоставляет новое наблюдение с evidence и case memory | Жёсткие if/else правила | Плохо работает при неполных, неоднозначных или конфликтующих данных | Пространство причин постепенно сужается |
| 11 | **Next action planning:** система выбирает следующий check, deeper investigation или mitigation | ИИ управляет переходом между шагами | Фиксированный pipeline | Либо продолжает искать лишнее, либо слишком рано делает вывод | Пользователь идёт по расследованию, а не по случайному списку проверок |
| 12 | **Containment / mitigation planning:** система предлагает безопасные действия: rollback, rate limit, pause jobs, isolate dependency, reduce blast radius | ИИ связывает похожие incidents с практическими действиями | Общие runbook-шаблоны | Хуже учитывает конкретный failure mode и риск действия | Пользователь получает практическую помощь после диагностики |
| 13 | **Линейный diagnostic MVP:** problem → symptoms → theory → practice cases → hypotheses → check → observation → update → report | ИИ используется локально на каждом шаге | Полностью жёсткий workflow | Может быть приемлемо, но менее гибко | Появляется первый end-to-end troubleshooting assistant |
| 14 | **Policy layer:** ограничение ложной уверенности и unsupported claims | ИИ формулирует аккуратно, policy проверяет структуру | Жёсткие шаблоны отчёта | Шаблоны менее гибкие, но policy всё равно нужна | Отчёт не выдаёт догадки за root cause |
| 15 | **Session memory:** расследование продолжается в несколько сообщений | ИИ помогает удерживать релевантное состояние кейса | Хранить всю историю подряд | Контекст быстро зашумляется | Пользователь ведёт живое расследование |
| 16 | **Вариативный orchestrator:** система выбирает следующий шаг по состоянию расследования | ИИ впервые становится архитектурно важным: решает, искать теорию, искать cases, спросить уточнение, выбрать check, обновить гипотезы или предложить mitigation | Большое дерево правил | Дерево быстро разрастается и становится хрупким | Система адаптируется к разным типам проблем |
| 17 | **Несколько корпусов одновременно:** `theory_corpus`, `practice_corpus`, `procedure_corpus`, `pattern_corpus` | ИИ выбирает, какой корпус нужен сейчас | Всегда искать во всех корпусах | Больше шума, задержки, нерелевантных результатов | Ответ становится полнее, но не перегруженным |
| 18 | **Tool use:** вызовы специализированных инструментов поверх retrieval/evidence/state | ИИ выбирает tools: retrieve cases, compare hypotheses, choose check, interpret observation, plan mitigation | Фиксированная последовательность вызовов | Либо лишние вызовы, либо недостаток данных | Система становится рабочим assistant, а не answer generator |
| 19 | **Полноценный agentic diagnostic loop:** plan → action → observation → hypothesis update → next action | ИИ становится механизмом управления расследованием | Огромная машина правил + ручной workflow | Сложно поддерживать, плохо покрывает ветвящиеся случаи | Пользователь получает помощника, который ведёт расследование по evidence |

---

## 4. Поворотный момент

До уровней 5–7 проект остаётся сильным RAG/workflow-приложением:

- принимает проблему;
- извлекает симптомы;
- ищет теорию;
- ищет похожие практические случаи;
- собирает evidence;
- предлагает гипотезы;
- показывает missing data.

Это полезно, но ещё не настоящая помощь в решении проблемы.

Настоящая practical value начинается с уровня 8:

```text
choose one discriminating check
  -> interpret result
  -> update hypotheses
  -> choose next action
```

Настоящий agentic layer начинается с уровня 16, когда система не просто выполняет фиксированный pipeline, а выбирает следующий шаг по состоянию расследования.

---

## 5. MVP scope

В MVP входит:

- использование существующего `theory_corpus` из текущего RAG-приложения;
- добавление первого `practice_corpus`;
- ingestion практических документов;
- chunking practical cases;
- semantic/hybrid retrieval по практическим случаям;
- structured extraction минимальных case records;
- evidence-backed hypotheses;
- missing data detection;
- discriminating check selection;
- intake пользовательских наблюдений;
- hypothesis update после наблюдения;
- 2–4 итерации diagnostic loop на один кейс;
- final diagnostic summary;
- safe containment / mitigation suggestions;
- policy check для ограничения ложной уверенности.

В MVP не входит:

- полноценный autonomous production agent;
- автоматическое исправление проблем;
- production incident response без человека;
- интеграция с живыми telemetry/logs/traces;
- полная `procedure_corpus` и `pattern_corpus` база;
- автоматическое определение root cause;
- выполнение destructive actions;
- claims с высокой уверенностью без evidence.

---

## 6. Минимальный runtime flow для MVP

```text
RawUserProblem
  -> problem_ingest
  -> symptom_characterization
  -> theory_retrieval
  -> practice_case_retrieval
  -> evidence_grouping
  -> hypothesis_builder
  -> missing_data_detector
  -> check_selector
  -> user_observation_intake
  -> observation_interpreter
  -> hypothesis_updater
  -> next_action_planner
  -> mitigation_planner
  -> diagnostic_report_builder
  -> policy_check
  -> SafeDiagnosticBrief
```

Для первой версии можно реализовать цикл так:

```text
for step in 1..=4:
    choose best discriminating check
    ask user for observation
    interpret observation
    update hypotheses
    if one hypothesis is strong enough:
        move to mitigation planning
        break
```

---

## 7. Orchestrator model

Общая схема остаётся совместимой с прежним вариантом:

```text
RawInput
   |
   v
Orchestrator
   |
   v
TransitionPolicy
   |
   v
StepExecutor
   |
   v
StepResult
   |
   v
RunState update
   |
   +--> next step
```

Но `TransitionPolicy` теперь должна уметь выбирать не только следующий линейный stage, а один из вариантов:

```text
retrieve_more_theory
retrieve_more_practice_cases
build_or_update_hypotheses
ask_for_missing_data
select_discriminating_check
interpret_observation
update_hypotheses
plan_mitigation
finalize_report
stop_due_to_low_confidence
```

---

## 8. Минимальные сущности MVP

```text
RawUserProblem
ProblemCase
SymptomSummary
TheoryConceptHit
PracticeCaseHit
EvidenceBundle
DiagnosticHypothesis
DiscriminatingCheck
DiagnosticObservation
HypothesisUpdate
SuggestedCheck
MissingDataItem
NextAction
MitigationOption
DiagnosticReport
SafeDiagnosticBrief
RunState
```

---

## 9. Ключевая сущность: DiagnosticHypothesis

```json
{
  "id": "h1",
  "claim": "The outage is caused by reconnect storm overloading the metadata layer",
  "status": "active",
  "confidence": "medium",
  "supporting_evidence": [
    "Observed second wave of errors during recovery",
    "Similar pattern in Cloudflare R2 incident"
  ],
  "contradicting_evidence": [],
  "checks_to_confirm": [
    "Compare reconnect rate with metadata CPU saturation during recovery window"
  ],
  "checks_to_disprove": [
    "If reconnect rate stayed normal while metadata layer was saturated earlier, weaken this hypothesis"
  ],
  "source_refs": [
    "practice_case:cloudflare_r2_2025_02_06"
  ]
}
```

---

## 10. Ключевая сущность: DiscriminatingCheck

Проверка должна не просто “добавлять данные”, а различать гипотезы.

```json
{
  "id": "check_1",
  "question": "Did reconnect rate spike before metadata layer saturation?",
  "why_this_check": "Distinguishes reconnect-storm amplification from independent metadata-layer degradation",
  "hypotheses_it_distinguishes": ["h1", "h2"],
  "expected_observations": [
    {
      "observation": "Reconnect rate spikes first, metadata saturation follows",
      "effect": "strengthen h1"
    },
    {
      "observation": "Metadata saturation starts before reconnect spike",
      "effect": "strengthen h2"
    }
  ],
  "requested_data": [
    "reconnect rate time series",
    "metadata CPU or queue depth",
    "incident timeline"
  ]
}
```

---

## 11. Ключевая сущность: DiagnosticObservation

```json
{
  "id": "obs_1",
  "provided_by": "user",
  "raw_text": "Reconnects increased 8x two minutes before metadata CPU hit 95%",
  "normalized_facts": [
    "reconnect_rate_spike_before_metadata_saturation",
    "metadata_cpu_peak_95_percent"
  ],
  "time_window": "recovery phase",
  "source_type": "metric_summary"
}
```

---

## 12. Ключевая сущность: HypothesisUpdate

```json
{
  "observation_id": "obs_1",
  "updates": [
    {
      "hypothesis_id": "h1",
      "change": "strengthen",
      "reason": "Observation matches expected reconnect-storm amplification pattern"
    },
    {
      "hypothesis_id": "h2",
      "change": "weaken",
      "reason": "Metadata layer did not degrade independently before reconnect spike"
    }
  ],
  "next_recommended_step": "plan containment for reconnect storm and metadata overload"
}
```

---

## 13. Как хранить practice corpus

Практические документы лучше хранить в двух формах одновременно.

### 13.1. RAG chunks

Нужны для semantic/hybrid retrieval.

Минимальные metadata:

```text
document_id
chunk_id
source_type
source_name
source_url
system_or_product
failure_modes
symptoms
affected_components
published_at
section_title
provenance_ref
```

### 13.2. Structured case records

Нужны для фильтрации, сравнения и построения гипотез.

Минимальная карточка:

```text
case_id
title
source_type
system_or_product
short_summary
symptoms
failure_modes
root_causes
contributing_factors
detection_signals
mitigations
prevention_actions
source_chunk_refs
confidence_notes
```

RAG отвечает на вопрос:

```text
Какие фрагменты похожи по смыслу?
```

Structured case records отвечают на вопрос:

```text
Какие случаи похожи по симптомам, failure mode, компонентам и последствиям?
```

---

## 14. Practice corpus для MVP

Минимальный стартовый набор:

```text
practice_corpus/
  raw/
    jepsen/
    cloudflare/
    github/
    slack/
    fastly/

  normalized/
    documents.jsonl
    case_records.jsonl

  chunks/
    practice_chunks.jsonl

  manifests/
    source_inventory.json
```

Рекомендуемые источники для первого MVP:

```text
1. Jepsen analyses
2. Cloudflare postmortems
3. GitHub post-incident analyses
4. Slack engineering incident reports
5. Fastly outage report
```

---

## 15. Инварианты качества

Система не должна:

- выдавать похожий incident как доказанный root cause;
- предлагать destructive action без предупреждения;
- скрывать неопределённость;
- смешивать theory evidence и practice precedent;
- делать final diagnosis после одного retrieval step;
- перечислять все проверки вместо выбора следующей лучшей;
- продолжать расследование, если данных явно недостаточно.

Система должна:

- явно показывать активные гипотезы;
- показывать supporting и contradicting evidence;
- выбирать одну следующую discriminating check;
- объяснять, какие гипотезы эта проверка различает;
- обновлять гипотезы после observation;
- отделять containment от root cause fix;
- сохранять provenance;
- завершать с честным confidence level.

---

## 16. Evaluation для MVP

Нужно оценивать не только качество ответа, но и качество диагностического цикла.

Минимальные eval dimensions:

```text
retrieval_relevance
practice_case_match_quality
hypothesis_quality
check_discriminativeness
observation_interpretation_quality
hypothesis_update_correctness
mitigation_safety
evidence_coverage
unsupported_claim_rate
policy_conformance
```

Особенно важная новая метрика:

```text
check_discriminativeness
```

Она отвечает на вопрос:

```text
Выбрала ли система проверку, которая реально различает активные гипотезы?
```

Пример плохого поведения:

```text
Check logs, metrics, traces, configs.
```

Пример хорошего поведения:

```text
First compare reconnect rate with metadata-layer saturation during recovery.
This distinguishes reconnect-storm amplification from independent metadata degradation.
```

---

## 17. Сжатая формула эволюции

```text
Question answering over theory
  -> practical case retrieval
  -> evidence grouping
  -> hypothesis generation
  -> missing data detection
  -> discriminating check selection
  -> observation interpretation
  -> hypothesis update
  -> next action planning
  -> containment / mitigation planning
  -> adaptive orchestration
  -> agentic diagnostic loop
```

---

## 18. Главный смысл перехода

Текущий RAG-проект уже показывает, что система умеет искать, ранжировать, оценивать и наблюдать retrieval/generation pipeline.

Новый слой делает следующий шаг:

не просто отвечать на вопросы по distributed systems,
а помогать разбирать практические проблемы через связку:

```text
theory
  + practical incidents
  + evidence
  + hypotheses
  + discriminating checks
  + user observations
  + iterative narrowing
  + mitigation
```

Именно добавление `practice_corpus` превращает проект из учебного RAG-приложения в основу для настоящего diagnostic assistant.

Но именно `diagnostic loop` превращает его из красивого evidence-backed summarizer в рабочую agentic system.
