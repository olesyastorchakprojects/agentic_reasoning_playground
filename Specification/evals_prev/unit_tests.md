## 1) Purpose / Scope

This document defines the mandatory generated unit-test contract for the eval engine.

This document is the single source of truth for:

- shared unit-test generation rules;
- required module-level unit-test cases;
- required coverage targets.

Code generation for the eval engine is incomplete if the generated Python source omits any required unit tests defined by this document.

## 2) General Generation Rules

Generated unit tests must satisfy all of the following rules:

- unit tests must be deterministic;
- unit tests must not depend on external network access;
- unit tests must not depend on running PostgreSQL instances, Docker containers, or external services;
- unit tests must execute locally inside the Python test process;
- unit tests must assert exact contract-relevant outcomes;
- success-path tests must assert exact returned values or exact output structure;
- failure-path tests must assert the specific failure behavior or omission behavior defined by the contract;
- comments, TODO items, prose test plans, pseudo-tests, placeholder test functions without assertions, and empty test classes do not satisfy any required unit-test case from this document;
- tests must not introduce new public module APIs solely for testability;
- database-calling functions must be tested with in-memory or fake data passed directly, not through live database connections.

Required test placement rules:

- unit tests for `eval_orchestrator` internal helpers must live in `Execution/evals/test_eval_orchestrator_report.py`;
- each test file must be independently runnable with `pytest`.

## 3) Required Unit Tests: `_build_retrieval_quality_section`

These tests cover the internal helper responsible for constructing the Retrieval Quality section of `run_report.md`.

The helper accepts one argument: a `dict` (or `None`) representing the single aggregated row returned by the `request_run_summaries` query (keys are column names, values are Python `Decimal` or `int` or `None`). It returns either a non-empty list of Markdown strings or an empty list when the section must be omitted.

### 3.1) Full data — table and scalars rendered

**Input**: aggregated row with all metric columns non-null:

- `retrieval_evaluated_k = 12`, `reranking_evaluated_k = 4`
- `retrieval_recall_soft = 0.5100`, `retrieval_recall_strict = 0.7533`
- `retrieval_rr_soft = 1.0000`, `retrieval_rr_strict = 1.0000`
- `retrieval_ndcg = 0.6605`
- `reranking_recall_soft = 0.2733`, `reranking_recall_strict = 0.5933`
- `reranking_rr_soft = 1.0000`, `reranking_rr_strict = 1.0000`
- `reranking_ndcg = 0.7875`
- `retrieval_context_loss_soft = 0.2367`, `retrieval_context_loss_strict = 0.1600`
- `retrieval_num_relevant_soft = 3.5`, `retrieval_num_relevant_strict = 2.1`
- `reranking_num_relevant_soft = 1.8`, `reranking_num_relevant_strict = 1.2`

**Expected**:

- output is non-empty;
- contains a markdown table header row with columns: `set`, `Recall soft`, `Recall strict`, `MRR soft`, `MRR strict`, `nDCG`;
- contains a row starting with `| top12 |` with values `0.5100`, `0.7533`, `1.0000`, `1.0000`, `0.6605`;
- contains a row starting with `| top4 |` with values `0.2733`, `0.5933`, `1.0000`, `1.0000`, `0.7875`;
- contains `retrieval_context_loss_soft: 0.2367`;
- contains `retrieval_context_loss_strict: 0.1600`;
- contains `avg_num_relevant_in_top12_soft: 3.5000`;
- contains `avg_num_relevant_in_top12_strict: 2.1000`;
- contains `avg_num_relevant_in_top4_soft: 1.8000`;
- contains `avg_num_relevant_in_top4_strict: 1.2000`.

### 3.2) All metric columns NULL — section omitted

**Input**: aggregated row where all metric columns are `None` / NULL (no requests in the run had retrieval quality metrics).

**Expected**:

- output is an empty list;
- the Retrieval Quality section heading must not appear in the output.

### 3.3) k values reflected in row labels and scalar labels

**Input**: `retrieval_evaluated_k = 20`, `reranking_evaluated_k = 5`, with non-null metric values.

**Expected**:

- table contains a row starting with `| top20 |`;
- table contains a row starting with `| top5 |`;
- scalar list contains label `avg_num_relevant_in_top20_soft`;
- scalar list contains label `avg_num_relevant_in_top20_strict`;
- scalar list contains label `avg_num_relevant_in_top5_soft`;
- scalar list contains label `avg_num_relevant_in_top5_strict`.

### 3.4) Negative context loss rendered correctly

**Input**: `retrieval_context_loss_soft = -0.0500` (reranker improved recall beyond retrieval baseline — acceptable edge case).

**Expected**:

- output contains `retrieval_context_loss_soft: -0.0500`;
- section is not omitted.

### 3.5) Partial NULLs — section rendered with available values

**Input**: `reranking_ndcg = None`, all other metric columns non-null.

**Expected**:

- section is not omitted;
- the `top{reranking_evaluated_k}` row renders `n/a` in the `nDCG` column position rather than crashing;
- all other values render correctly.

## 4) Required Unit Tests: `_build_conditional_retrieval_generation_section`

The helper accepts one argument: a dict (or None) representing the single
aggregated row from the 20-expression query. Returns either a non-empty list
of Markdown strings or an empty list when the section must be omitted.

### 4.1) Worked example — exact values verified

Input dict derived by pre-aggregating 3 requests (top4_soft column verified):

  Request A: num_relevant_in_top4_soft=2, first_relevant_rank_top4_soft=1,
             groundedness=1.0, answer_completeness=1.0, answer_relevance=1.0
  Request B: num_relevant_in_top4_soft=1, first_relevant_rank_top4_soft=2,
             groundedness=0.5, answer_completeness=1.0, answer_relevance=1.0
  Request C: num_relevant_in_top4_soft=0, first_relevant_rank_top4_soft=None,
             groundedness=0.0, answer_completeness=0.5, answer_relevance=1.0

The test input is a pre-aggregated dict (as returned by the SQL query), not
the request-level rows above. Dict keys are the fixed SQL aliases defined in
eval_orchestrator.md (no k values embedded). The dict values for the
reranking_soft condition (reranking_evaluated_k=4, rendered as top4_soft) are:

  groundedness_reranking_soft            = Decimal("0.7500")   # mean(1.0, 0.5), A+B
  answer_completeness_reranking_soft     = Decimal("1.0000")   # mean(1.0, 1.0), A+B
  answer_relevance_reranking_soft        = Decimal("1.0000")   # mean(1.0, 1.0), A+B
  hallucination_reranking_soft           = Decimal("1.0000")   # rate(B,C), 2/2
  success_reranking_soft                 = Decimal("0.5000")   # rate(A), 1/2

  retrieval_evaluated_k = 12
  reranking_evaluated_k = 4

**Expected**:

- output is non-empty;
- section title contains "Conditional Retrieval";
- rendered table header contains: metric, top12_soft, top12_strict, top4_soft, top4_strict
  (k values substituted from retrieval_evaluated_k=12 and reranking_evaluated_k=4);
- the top4_soft column in groundedness row contains "0.7500";
- the top4_soft column in answer_completeness row contains "1.0000";
- the top4_soft column in hallucination_rate row contains "1.0000";
- the top4_soft column in success_rate row contains "0.5000".

### 4.2) All cells NULL — section omitted

**Input**: dict with all 20 aggregate values = None; `retrieval_evaluated_k` and
`reranking_evaluated_k` may be any value (e.g. 12 and 4) — the omission check
must inspect only the 20 aggregate keys, not the k-value keys.

**Expected**:

- output is empty list;
- section title must not appear.

### 4.3) None input — section omitted

**Input**: None (query returned zero rows).

**Expected**:

- output is empty list.

### 4.4) Zero-denominator renders n/a, not 0.0000

**Input**: dict where all reranking_strict aggregate values are None
(keys: `groundedness_reranking_strict`, `answer_completeness_reranking_strict`,
`answer_relevance_reranking_strict`, `hallucination_reranking_strict`,
`success_reranking_strict` all set to None), other conditions non-null.

**Expected**:

- each cell in the reranking_strict column (rendered as top{reranking_k}_strict) renders "n/a";
- section is not omitted (other columns have data);
- no cell in that column renders "0.0000".

### 4.5) top1_irrelevant condition with None first_relevant_rank treated as irrelevant

**Input**: a pre-aggregated dict where `hallucination_reranking_soft` is non-null
(e.g. `Decimal("1.0000")`), derived from a filter condition where NULL
`first_relevant_rank` was counted as `top1_irrelevant` by the SQL query.
The test verifies the helper renders the value correctly, not the aggregation logic.

**Expected**:

- hallucination_reranking_soft cell is not "n/a" (denominator > 0);
- the rendered value matches the non-null dict value.

### 4.6) k values substituted into column headers

**Note (deviation from original task):** Column headers use the shorter form
`top{k}_soft` / `top{k}_strict` instead of `top{k}_soft_conditioned`.
This was chosen for table width. Any reference implementation must match
these exact shorter names.

**Input**: dict with fixed-alias keys (e.g. `groundedness_retrieval_soft`, `groundedness_reranking_soft`, etc.),
retrieval_evaluated_k=20, reranking_evaluated_k=5, all values non-null.

**Expected**:

- rendered table header contains "top20_soft" (k=20 substituted for retrieval columns);
- rendered table header contains "top5_soft" (k=5 substituted for reranking columns);
- rendered header does not contain literal "retrieval_evaluated_k" or "reranking_evaluated_k";
- rendered header does not contain "_conditioned" suffix;
- dict keys passed to the helper use fixed aliases (e.g. "groundedness_retrieval_soft"),
  not k-embedded names (e.g. "groundedness_top20_soft").

## 5) Required Unit Tests: `_build_run_report` integration

These tests verify that `_build_run_report` includes or excludes the Retrieval Quality section and the Conditional Retrieval→Generation Aggregates section based on data availability, without requiring a live database.

### 5.1) Report includes Retrieval Quality section when data is available

**Setup**: mock the `request_run_summaries` query to return one aggregated row with non-null metric values.

**Expected**:

- the returned report string contains `Retrieval Quality`;
- the string contains `top` followed by an integer k value.

### 5.2) Report omits Retrieval Quality section when no data

**Setup**: mock the `request_run_summaries` query to return zero rows or a row with all metric columns NULL.

**Expected**:

- the returned report string does not contain `Retrieval Quality`.

### 5.3) Report omits Conditional Retrieval→Generation section when no data available

**Setup**: mock the 20-expression query to return a row where all 20 values are NULL.

**Expected**:

- returned report string does not contain "Conditional Retrieval".
