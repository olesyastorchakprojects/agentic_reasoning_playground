# Eval Run Report

## Contents

- [Run Metadata](#run-metadata)
- [Metric Layers](#metric-layers)
- [Executive Summary](#executive-summary)
- [Runtime Gold Metrics](#runtime-gold-metrics)
- [Where Quality Was Lost](#where-quality-was-lost)
- [Judge-Based Aggregated Metrics](#judge-based-aggregated-metrics)
- [Suite Distributions](#suite-distributions)
- [Gate Breakdown](#gate-breakdown)
- [Failure Attribution](#failure-attribution)
- [Runtime vs Judge Interpretation](#runtime-vs-judge-interpretation)
- [Worst-Case Preview](#worst-case-preview)
- [Token Usage](#token-usage)
- [Appendix A: Full Query Structuring Diagnostics](#appendix-a-full-query-structuring-diagnostics)
- [Appendix B: Full Retrieval Diagnostics](#appendix-b-full-retrieval-diagnostics)
- [Appendix C: Judge Metrics Per Run](#appendix-c-judge-metrics-per-run)

- [Appendix D: Suite Overview](#appendix-d-suite-overview)

## Run Metadata

- eval_run_id: `c37e29e3-5726-402a-b707-b5264e28d74f`
- run_type: `golden_dataset`
- status: `completed`
- started_at: `2026-05-20 12:27:03.008919097 UTC`
- completed_at: `2026-05-20 15:39:46.480648988 UTC`
- runtime_run_count: `5`
- iterations_evaluated_count: `15`
- judge_model: `openai/gpt-oss-120b`
- query_structuring_model: `openai/gpt-oss-120b`
- observation_boundary_resolver_model: `openai/gpt-oss-120b`
- observation_extraction_model: `openai/gpt-oss-120b`
- llm_structured_generation_model: `openai/gpt-oss-120b`
- query_structuring_prompt_version: `v2`
- observation_boundary_resolver_prompt_version: `v1`
- observation_extraction_prompt_version: `v2`
- prompt_context_prompt_version: `v7`
- diagnostic_update_prompt_context_prompt_version: `v5`
- suite_count: `13`

### Token Pricing

| model | input_price_per_1m | output_price_per_1m |
|---|---:|---:|
| openai/gpt-oss-120b | $0.05/1M | $0.2/1M |

## Metric Layers

| layer | source | evaluates | interpretation |
|---|---|---|---|
| Judge-based quality metrics | judge model outputs | semantic quality of structuring, evidence pack, and final answer | answers whether the diagnostic behavior is good |
| Runtime gold metrics | runtime trace spans with golden labels | query structuring and retrieval against expected labels / evidence | answers whether upstream modules selected the expected terms and evidence |
| Runtime diagnostics | runtime trace attributes and events | low-level counters, hit counts, configuration, support-level issues | helps debug why a metric failed |

## Executive Summary

| metric | value | meaning |
|---|---:|---|
| usable_first_response_rate | 0.8667 | Share of runs where the final answer can be shown as a first diagnostic response |
| gate_pass_rate | 0.7333 | Share of runs without critical gate failures |
| query_structuring_judge_score | 0.2333 | Judge-based semantic quality of query structuring |
| runtime_query_structuring_macro_precision_soft | 0.6500 | Share of selected query-structuring terms that fall within the acceptable semantic set |
| runtime_query_structuring_grounded_strict_recall | 0.5500 | Coverage of canonical expected query-structuring terms with valid grounding |
| runtime_retrieval_mean_ndcg | 0.7997 | Average ranking quality across retrieval targets and runs |
| runtime_retrieval_all_strict_recall_success_rate | 0.9500 | Average per-run share of retrieval targets where strict expected evidence was found |
| evidence_pack_judge_score | 0.5333 | Judge-based quality of selected evidence pack |
| final_answer_judge_score | 1.8000 | Judge-based quality of final diagnostic response |
| usable_continuation_response_rate | 0.4000 | Share of continuation iterations with usable update behavior |
| continuation_update_judge_score | 1.7000 | Judge-based quality of updating the diagnostic frame |
| continuation_input_judge_score | 2.0000 | Judge-based quality of reconstructing the new observation from context |
| continuation_update_strict_pass_rate | 0.6000 | Share of continuation iterations where CU1, CU2, CU3 all scored 2 |

> QS1 = query_structuring_field_boundary_correctness ; QS2 = query_structuring_grounding_conservatism
> EP1 = evidence_pack_role_fit ; EP2 = evidence_pack_sufficiency
> FA1 = final_no_root_cause_claim ; FA2 = final_first_check_discriminates ; FA3 = final_hypothesis_source_alignment ; FA4 = final_alternative_context_handling ; FA5 = final_result_interpretation_usefulness
> CU1 = continuation_hypothesis_update_discipline ; CU2 = continuation_problem_understanding_update ; CU3 = continuation_next_check_progression ; CU4 = continuation_observation_resolution_context_recovery


> See [Appendix D: Suite Overview](#appendix-d-suite-overview) for the detailed suite description.

## Judge-Based Aggregated Metrics

> initial iter-s = all initial iterations across runs; continuation iter-s = all continuation iterations across runs; total ignores n/a

| metric | initial iter-s | continuation iter-s | total | formula |
|---|---:|---:|---:|---|
| usable_first_response_rate | 1.0000 | n/a | 1.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 0.7000 | n/a | 0.7000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.6000 | n/a | 1.6000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 1.8800 | 1.7600 | 1.8000 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0.4000 | n/a | 0.4000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1.0000 | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 0.8000 | 0.8000 | 0.8000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0.2000 | 0.2000 | 0.2000 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0.0000 | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0.2000 | n/a | 0.2000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0.8000 | 0.4000 | 0.5333 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 1.6000 | 1.6000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 1.9000 | 1.9000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 1.6000 | 1.6000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2.0000 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 0.4000 | 0.4000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 1.7000 | 1.7000 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 0.6000 | 0.6000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0.6000 | 0.6000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2.0000 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1.0000 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1.0000 | 1.0000 | frac(CU4=2) |

> QS1 = query_structuring_field_boundary_correctness ; QS2 = query_structuring_grounding_conservatism
> EP1 = evidence_pack_role_fit ; EP2 = evidence_pack_sufficiency
> FA1 = final_no_root_cause_claim ; FA2 = final_first_check_discriminates ; FA3 = final_hypothesis_source_alignment ; FA4 = final_alternative_context_handling ; FA5 = final_result_interpretation_usefulness
> CU1 = continuation_hypothesis_update_discipline ; CU2 = continuation_problem_understanding_update ; CU3 = continuation_next_check_progression ; CU4 = continuation_observation_resolution_context_recovery


> See [Appendix D: Suite Overview](#appendix-d-suite-overview) for the detailed suite description.

## Runtime Gold Metrics

These metrics are computed from runtime trace spans and compare structured query / retrieval outputs against golden labels.

### Query Structuring Quality Metrics

| metric | value | meaning |
|---|---:|---|
| runtime_query_structuring_macro_precision_soft | 0.6500 | Share of selected vocabulary terms that fall within the acceptable semantic set |
| runtime_query_structuring_macro_recall_soft | 0.4750 | Coverage of broader acceptable vocabulary terms |
| runtime_query_structuring_macro_recall_strict | 0.6500 | Coverage of canonical expected vocabulary terms |
| runtime_query_structuring_grounded_strict_recall | 0.5500 | Coverage of canonical expected terms with valid grounding |

#### Query Structuring Field Quality Profile

| field | precision_soft | recall_soft | recall_strict | grounded_strict_recall | field_core_success | field_grounded_success |
|---|---:|---:|---:|---:|---:|---:|
| symptoms | 0.7000 | 0.4000 | 0.6000 | 0.6000 | 0.6000 | 0.6000 |
| affected_subsystems | 0.4000 | 0.4000 | 0.6000 | 0.6000 | 0.6000 | 0.6000 |
| failure_modes | 1.0000 | 0.6000 | 1.0000 | 0.6000 | 1.0000 | 0.6000 |
| system_properties | 0.5000 | 0.5000 | 0.4000 | 0.4000 | 0.4000 | 0.4000 |

#### Query Structuring Field Overreach Signals

| field | zero_score_selection_count | unsupported_selected_term_rate | invalid_evidence_span_count |
|---|---:|---:|---:|
| symptoms | 0.40 | 0.0000 | 0.00 |
| affected_subsystems | 1.40 | 0.0000 | 0.00 |
| failure_modes | 0.00 | 0.4000 | 0.20 |
| system_properties | 0.80 | 0.2000 | 0.40 |

#### Query Structuring Strict Contract Checks

| metric | value | meaning |
|---|---:|---|
| runtime_query_structuring_core_success_rate | 0.6500 | Harsh all-fields contract pass rate against strict field expectations |

### Retrieval Core Metrics

> Each value is averaged over runs where the target was evaluated.

| retrieval_target | evaluated_k | recall_strict | recall_soft | rr_strict | rr_soft | nDCG | frr_strict | frr_soft | n_strict | n_soft |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| candidate_cards | 8.0 | 1.0000 | 0.8889 | 1.0000 | 1.0000 | 0.9440 | 1.00 | 1.00 | 1.00 | 2.67 |
| incident_primary | 12.0 | 1.0000 | 0.9556 | 0.8562 | 0.9111 | 0.8905 | 1.73 | 1.27 | 1.40 | 3.27 |
| incident_alternatives | 12.0 | 0.8000 | 0.6222 | 0.6889 | 0.7333 | 0.6488 | 1.33 | 1.17 | 0.80 | 1.87 |
| theory_evidence | 12.0 | 1.0000 | 0.8333 | 0.6383 | 0.7467 | 0.7156 | 2.87 | 2.13 | 1.00 | 1.67 |

### Retrieval Summary

| metric | value | formula | meaning |
|---|---:|---|---|
| runtime_retrieval_mean_ndcg | 0.7997 | avg_run(avg_target(ndcg)) | Average ranking quality across retrieval targets and runs |
| runtime_retrieval_all_strict_recall_success_rate | 0.9500 | avg_run(frac_target(recall_strict=1)) | Average per-run share of retrieval targets with strict recall success |
| runtime_retrieval_all_soft_recall_success_rate | 0.9500 | avg_run(frac_target(recall_soft>0)) | Average per-run share of retrieval targets with any soft recall |
| runtime_retrieval_penalized_first_relevant_rank_strict | 2.32 | avg_run(avg_target(frr_strict or k+1)) | Penalized rank; missing strict hit treated as k+1 |
| runtime_retrieval_zero_hit_rate | 0.0000 | avg_run(frac_target(hits_count=0)) | Average per-run share of retrieval calls with no hits |

## Suite Distributions

| suite | score_0 | score_1 | score_2 |
|---|---:|---:|---:|
| final_no_root_cause_claim | 2 | 0 | 13 |
| final_first_check_discriminates | 0 | 0 | 15 |
| final_alternative_context_handling | 2 | 1 | 12 |
| final_result_interpretation_usefulness | 0 | 0 | 15 |
| final_hypothesis_source_alignment | 1 | 4 | 10 |
| query_structuring_field_boundary_correctness | 11 | 3 | 1 |
| query_structuring_grounding_conservatism | 13 | 2 | 0 |
| evidence_pack_role_fit | 10 | 4 | 1 |
| evidence_pack_sufficiency | 10 | 0 | 5 |
| continuation_hypothesis_update_discipline | 2 | 0 | 8 |
| continuation_problem_understanding_update | 0 | 1 | 9 |
| continuation_next_check_progression | 2 | 0 | 8 |
| continuation_observation_resolution_context_recovery | 0 | 0 | 10 |

## Gate Breakdown

| gate | fail_count | fail_rate |
|---|---:|---:|
| final_no_root_cause_claim | 2 | 0.1333 |
| final_first_check_discriminates | 0 | 0.0000 |
| final_hypothesis_source_alignment | 1 | 0.0667 |
| query_structuring_field_boundary_correctness | 1 | 0.0667 |
| evidence_pack_sufficiency | 0 | 0.0000 |
| continuation_hypothesis_update_discipline | 2 | 0.2000 |
| continuation_problem_understanding_update | 0 | 0.0000 |
| continuation_next_check_progression | 2 | 0.2000 |
| continuation_observation_resolution_context_recovery | 0 | 0.0000 |

> Gate fails when suite score = 0. Pass threshold: score ≥ 1.
> Note: `Gate Breakdown` reflects critical standalone gates and may differ from composite no-hard-fail formulas (e.g., `final_hypothesis_source_alignment` is gated individually but excluded from `final_answer_no_hard_fail_rate`).

## Failure Attribution

### Initial / First-Response Attribution

| metric | value | formula |
|---|---:|---|
| bad_final_due_to_query_rate | 0.0000 | frac(!usable ∧ (QS1=0 ∨ QS2=0)) |
| bad_final_due_to_evidence_rate | 0.0000 | frac(!usable ∧ (EP1=0 ∨ EP2=0)) |
| bad_final_with_good_query_and_evidence_rate | 0.1333 | frac(!usable ∧ QS1>0 ∧ QS2>0 ∧ EP1>0 ∧ EP2>0) |

> usable = FA1≥1 ∧ FA2≥1 ∧ FA5≥1

### Continuation Attribution

> `frac(condition)` uses only continuation iterations where all fields referenced by that formula are present; missing values are excluded from both numerator and denominator.

| metric | value | formula |
|---|---:|---|
| bad_continuation_due_to_input_resolution_rate | 0.0000 | frac(!usable_continuation ∧ CU4=0) |
| bad_continuation_due_to_update_logic_rate | 0.4000 | frac(!usable_continuation ∧ CU4>0 ∧ (CU1=0 ∨ CU2=0 ∨ CU3=0)) |
| bad_continuation_despite_good_input_rate | 0.6000 | frac(!usable_continuation ∧ CU4=2) |
| good_continuation_despite_input_issue_rate | 0.0000 | frac(usable_continuation ∧ CU4<2) |

> usable_continuation = CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1

> QS1 = query_structuring_field_boundary_correctness ; QS2 = query_structuring_grounding_conservatism
> EP1 = evidence_pack_role_fit ; EP2 = evidence_pack_sufficiency
> FA1 = final_no_root_cause_claim ; FA2 = final_first_check_discriminates ; FA3 = final_hypothesis_source_alignment ; FA4 = final_alternative_context_handling ; FA5 = final_result_interpretation_usefulness
> CU1 = continuation_hypothesis_update_discipline ; CU2 = continuation_problem_understanding_update ; CU3 = continuation_next_check_progression ; CU4 = continuation_observation_resolution_context_recovery


> See [Appendix D: Suite Overview](#appendix-d-suite-overview) for the detailed suite description.

## Where Quality Was Lost

### Pipeline Stage Summary

| stage | signals | status | interpretation |
|---|---|---|---|
| query structuring | judge 0.23, no-hard-fail 80%, soft precision 65%, grounded strict 55% | mixed | no strict pass on any run; 1 field boundary gate fail(s); acceptable-selection precision 65%; grounded strict recall 55%; strict contract pass 65% |
| retrieval | strict recall 95%, nDCG 0.80 | strong | recall was present, but ranking quality remained weak (nDCG 0.80) |
| evidence packing | judge 0.53, no-hard-fail 100% | mixed | 10 run(s) with insufficient evidence pack (EP2=0) |
| final answer | usable 87%, judge 1.80, no-hard-fail 80% | strong | 2 premature certainty (FA1=0); 5 partial source alignment (FA3<2) |
| continuation | usable cont 40%, update score 1.70, input score 2.00, no-hard-fail 60% | weak | 2 hard fail(s) on hypothesis update discipline (CU1=0) |

### Failure Path

2 of 15 responses were unusable.

- 2 unusable despite good query + evidence → **final answer stage failure**
  - 2 × FA1=0: premature certainty or root cause claim

Continuation was the main observed degradation point:

- 2 hypothesis update discipline hard fail(s) (CU1=0)
- 2 next check progression hard fail(s) (CU3=0)

Quality degraded between initial and continuation iterations.

Main observed weakness: **query structuring** (composite 0.53). retrieval and final answer quality was strong.

## Runtime vs Judge Interpretation

| signal | interpretation |
|---|---|
| Good query structuring runtime metrics + bad query structuring judge score | Terms may match gold labels, but field semantics or conservatism are wrong |
| Bad query structuring runtime metrics + good final answer | Final model compensated for upstream errors; system may be unstable |
| Good retrieval metrics + bad evidence_pack judge score | Retrieved relevant chunks, but selected/packed chunks do not serve diagnostic roles |
| Good evidence_pack judge score + bad final answer | Final prompt/model likely needs work |
| Bad alternative retrieval + bad alternative handling | Alternative context is not giving the model useful competing evidence |

## Worst-Case Preview

| runtime_run_id | iteration_id | final_answer_score | usable_first_response |
|---|---|---:|---:|
| `387d547b-b0a8-4389-a97b-f41a83d2fdf1` | `30885e11-7cf4-40bc-81d4-a27d808a8ed7` | 1.2000 | false |
| `839542af-0d73-4d20-bd0c-b1872017ecb6` | `f42c1a24-2ffd-4e97-a1d6-073387316f23` | 1.4000 | false |
| `889f8164-5f81-4989-8292-842fc8e34993` | `ce556c18-9a12-49c0-b3b8-30708253c099` | 1.4000 | true |
| `839542af-0d73-4d20-bd0c-b1872017ecb6` | `7ea6a80a-ee63-4910-a215-5c5854e44c02` | 1.6000 | true |
| `28059867-57d6-4b8c-986b-0d0e52eb4a70` | `85edfe10-47c8-4f25-8f47-27e6eced035d` | 1.8000 | true |

## Token Usage

### Judge Calls by Suite

| suite | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| continuation_hypothesis_update_discipline | 16336 | 7265 | 23601 | 0.002270 |
| continuation_next_check_progression | 11307 | 5421 | 16728 | 0.001650 |
| continuation_observation_resolution_context_recovery | 9723 | 3892 | 13615 | 0.001265 |
| continuation_problem_understanding_update | 13971 | 5192 | 19163 | 0.001737 |
| evidence_pack_role_fit | 15567 | 4493 | 20060 | 0.001677 |
| evidence_pack_sufficiency | 22024 | 2184 | 24208 | 0.001538 |
| final_alternative_context_handling | 75516 | 6754 | 82270 | 0.005127 |
| final_first_check_discriminates | 76431 | 6032 | 82463 | 0.005028 |
| final_hypothesis_source_alignment | 64382 | 8550 | 72932 | 0.004929 |
| final_no_root_cause_claim | 75741 | 4171 | 79912 | 0.004621 |
| final_result_interpretation_usefulness | 13019 | 5177 | 18196 | 0.001686 |
| query_structuring_field_boundary_correctness | 3862 | 3570 | 7432 | 0.000907 |
| query_structuring_grounding_conservatism | 3712 | 4993 | 8705 | 0.001184 |
| judge_total | 401591 | 67694 | 469285 | 0.033618 |

| model | prompt_tokens_cost | completion_tokens_cost | total_cost_usd |
|---|---|---|---:|
| openai/gpt-oss-120b | 401591 * $0.05/1M = 0.020080 | 67694 * $0.2/1M = 0.013539 | 0.033618 |

### Runtime by Stage

| scope | model | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---|---:|---:|---:|---:|
| query_structuring | openai/gpt-oss-120b | 6912 | 5371 | 12283 | 0.004259 |
| observation_boundary_resolver | openai/gpt-oss-120b | 7270 | 2340 | 9610 | 0.002495 |
| observation_extraction | openai/gpt-oss-120b | 4205 | 5787 | 9992 | 0.004103 |
| llm_structured_generation | openai/gpt-oss-120b | 56823 | 21533 | 78356 | 0.021443 |
| runtime_total | — | 75210 | 35031 | 110241 | 0.032300 |

| model | prompt_tokens_cost | completion_tokens_cost | total_cost_usd |
|---|---|---|---:|
| openai/gpt-oss-120b | 75210 * $0.15/1M = 0.011281 | 35031 * $0.6/1M = 0.021019 | 0.032300 |
| runtime_total | sum(stage prompt costs) = 0.011281 | sum(stage completion costs) = 0.021019 | 0.032300 |

### Totals

| scope | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| runtime | 75210 | 35031 | 110241 | 0.032300 |
| judge_total | 401591 | 67694 | 469285 | 0.033618 |
| run_total | 476801 | 102725 | 579526 | 0.065918 |

Run total cost usd = runtime total cost usd + judge total cost usd = 0.032300 + 0.033618 = 0.065918

## Appendix A: Full Query Structuring Diagnostics

### A.1 Contract Diagnostics

| field | invalid_vocab_count | duplicate_term_count |
|---|---:|---:|
| symptoms | 0.00 | 0.00 |
| affected_subsystems | 0.00 | 0.00 |
| failure_modes | 0.00 | 0.00 |
| system_properties | 0.00 | 0.00 |

### A.2 Selection Diagnostics

| field | num_predicted_terms | num_false_positive | num_false_negative_strict | zero_score_selection_count |
|---|---:|---:|---:|---:|
| symptoms | 1.20 | 0.40 | 0.40 | 0.40 |
| affected_subsystems | 2.20 | 1.40 | 0.40 | 1.40 |
| failure_modes | 1.00 | 0.00 | 0.00 | 0.00 |
| system_properties | 1.60 | 0.80 | 0.60 | 0.80 |

### A.3 Graded Relevance Diagnostics

| field | graded_coverage | average_selected_score |
|---|---:|---:|
| symptoms | 0.4667 | 0.6500 |
| affected_subsystems | 0.4667 | 0.3500 |
| failure_modes | 0.7333 | 1.0000 |
| system_properties | 0.4667 | 0.4000 |

### A.4 Grounding Diagnostics

| field | unsupported_selected_term_rate | missing_evidence_span_count | invalid_evidence_span_count | evidence_span_near_substring_rate |
|---|---:|---:|---:|---:|
| symptoms | 0.0000 | 0.00 | 0.00 | 1.0000 |
| affected_subsystems | 0.0000 | 0.00 | 0.00 | 1.0000 |
| failure_modes | 0.4000 | 0.00 | 0.20 | 0.8000 |
| system_properties | 0.2000 | 0.00 | 0.40 | 0.8000 |

### A.5 Support-Level Diagnostics

| field | weak_inference_rate | strict_terms_weak_inference_rate | weak_false_positive_rate |
|---|---:|---:|---:|
| symptoms | 0.0000 | 0.0000 | 0.0000 |
| affected_subsystems | 0.0000 | 0.0000 | 0.0000 |
| failure_modes | 0.2000 | 0.2000 | 0.0000 |
| system_properties | 0.0000 | 0.0000 | 0.0000 |

### A.6 Field Success Diagnostics

| field | field_core_success | field_grounded_success | empty_when_gold_exists |
|---|---:|---:|---:|
| symptoms | 0.6000 | 0.6000 | 0.0000 |
| affected_subsystems | 0.6000 | 0.6000 | 0.0000 |
| failure_modes | 1.0000 | 0.6000 | 0.0000 |
| system_properties | 0.4000 | 0.4000 | 0.0000 |

### A.7 Query-Level Non-Vocabulary Diagnostics

| entities_count_avg | constraints_count_avg | triggers_count_avg | observability_signals_count_avg | unresolved_terms_count_avg | intent_present_rate | scenario_present_rate |
|---:|---:|---:|---:|---:|---:|---:|
| 2.60 | 0.20 | 0.20 | 0.80 | 0.40 | 1.0000 | 1.0000 |

## Appendix B: Full Retrieval Diagnostics

### B.1 Retrieval Configuration

| retrieval_target | collection | top_k |
|---|---|---:|
| candidate_cards | cards | 8 |
| incident_primary | practice_chunks | 12 |
| incident_alternatives | practice_chunks | 12 |
| theory_evidence | theory_chunks | 12 |

### B.2 Retrieval Hit Counts

| retrieval_target | hits_count_avg | selected_count_avg | top_score_avg | min_score_avg |
|---|---:|---:|---:|---:|
| candidate_cards | 7.2 | 3.0 | 0.9300 | 0.2387 |
| incident_primary | 7.9 | 7.9 | 0.7744 | 0.2662 |
| incident_alternatives | 8.6 | 8.6 | 0.5831 | 0.2000 |
| theory_evidence | 7.6 | 7.6 | 0.6679 | 0.2000 |

## Appendix C: Judge Metrics Per Run

### Run `16cae338-3773-4cd5-b4d9-4fedaf01f3f6`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 1 | 1 | 1.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 0.5000 | n/a | n/a | 0.5000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 2.0000 | 2.0000 | 2.0000 | 2.0000 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0 | n/a | n/a | 0.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 1 | 1 | 1.0000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 0 | 0 | 0.0000 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 1 | 1 | 1 | 1.0000 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 0 | 2 | 1.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 1 | 2 | 1.5000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2 | 2 | 2.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 0 | 1 | 0.5000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 1.0000 | 2.0000 | 1.5000 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 0 | 1 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0 | 1 | 0.5000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

### Run `28059867-57d6-4b8c-986b-0d0e52eb4a70`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 1 | 1 | 1.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 0.5000 | n/a | n/a | 0.5000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 2.0000 | 1.8000 | 2.0000 | 1.9333 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0 | n/a | n/a | 0.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 1 | 1 | 1.0000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 0 | 0 | 0.0000 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 1 | 0 | 1 | 0.6667 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 0 | 2 | 1.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 2 | 2.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2 | 2 | 2.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 0 | 1 | 0.5000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 1.3333 | 2.0000 | 1.6667 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 0 | 1 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0 | 1 | 0.5000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

### Run `387d547b-b0a8-4389-a97b-f41a83d2fdf1`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 0 | 1 | 0.6667 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 2.0000 | 1.2000 | 1.8000 | 1.6667 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 0 | 1 | 0.6667 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 1 | 0 | 0.3333 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 1 | 0 | 0 | 0.3333 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 2 | 2 | 2.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 2 | 2.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2 | 2 | 2.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 0 | 1 | 0.5000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 2.0000 | 2.0000 | 2.0000 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

### Run `839542af-0d73-4d20-bd0c-b1872017ecb6`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 1 | 0 | 0.6667 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 0.0000 | n/a | n/a | 0.0000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 2.0000 | 1.6000 | 1.4000 | 1.6667 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0 | n/a | n/a | 0.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 1 | 0 | 0.6667 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 0 | 1 | 0.3333 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 1 | 0 | 0 | 0.3333 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 2 | 2 | 2.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 2 | 2.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 0 | 2 | 1.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 0 | 0 | 0.0000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 1.3333 | 2.0000 | 1.6667 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 0 | 1 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0 | 1 | 0.5000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

### Run `889f8164-5f81-4989-8292-842fc8e34993`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 1 | 1 | 1.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 1.0000 | n/a | n/a | 1.0000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 2.0000 | n/a | n/a | 2.0000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 1.4000 | 2.0000 | 1.8000 | 1.7333 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 0 | 1 | 1 | 0.6667 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 1 | 0 | 0 | 0.3333 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 1 | n/a | n/a | 1.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0 | 1 | 0 | 0.3333 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 2 | 2 | 2.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 2 | 2.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2 | 0 | 1.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 1 | 0 | 0.5000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 2.0000 | 1.3333 | 1.6667 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 1 | 0 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 1 | 0 | 0.5000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

> QS1 = query_structuring_field_boundary_correctness ; QS2 = query_structuring_grounding_conservatism
> EP1 = evidence_pack_role_fit ; EP2 = evidence_pack_sufficiency
> FA1 = final_no_root_cause_claim ; FA2 = final_first_check_discriminates ; FA3 = final_hypothesis_source_alignment ; FA4 = final_alternative_context_handling ; FA5 = final_result_interpretation_usefulness
> CU1 = continuation_hypothesis_update_discipline ; CU2 = continuation_problem_understanding_update ; CU3 = continuation_next_check_progression ; CU4 = continuation_observation_resolution_context_recovery


> See [Appendix D: Suite Overview](#appendix-d-suite-overview) for the detailed suite description.

## Appendix D: Suite Overview

### query_structuring_field_boundary_correctness

| code | applies to | checks | why | inputs | score |
|---|---|---|---|---|---:|
| QS1 | initial only | Whether symptoms, affected_subsystems, failure_modes, and system_properties respect their intended meanings | Bad field separation poisons downstream retrieval and diagnosis — this is the most important semantic eval for query structuring | original user query, structured query output, controlled vocabulary definitions | 0/1/2 |

### query_structuring_grounding_conservatism

| code | applies to | checks | why | inputs | score |
|---|---|---|---|---|---:|
| QS2 | initial only | Whether selected vocabulary terms are sufficiently supported by the user query, and whether the model avoids weak over-inference | Protects against hallucinated or overly eager labels that make retrieval look precise while being wrong | original user query, structured query output, selected terms with evidence_span and support_level | 0/1/2 |

### evidence_pack_role_fit

| code | applies to | checks | why | inputs | score |
|---|---|---|---|---|---:|
| EP1 | initial only | Whether each selected chunk fits its assigned role: evidence_for_match, first_check_hint, supporting_explanation, alternative_context, mechanism_explanation | Chunks may be generally relevant but diagnostically misplaced; role fit is where evidence packing most often fails | user query, structured query, selected chunks with roles, role definitions | 0/1/2 |

### evidence_pack_sufficiency

| code | applies to | checks | why | inputs | score |
|---|---|---|---|---|---:|
| EP2 | initial only | Whether the selected evidence pack is enough to support a useful first diagnostic move | Evaluates the pack as a whole — good individual chunks can still leave the model unable to form hypotheses | user query, structured query, primary card, selected incident chunks, selected theory chunks | 0/1/2 |

### final_no_root_cause_claim

| code | applies to | checks | why | inputs | score |
|---|---|---|---|---|---:|
| FA1 | shared | Whether the answer avoids claiming or implying a final root cause | The assistant produces a first diagnostic frame, not a final diagnosis — premature certainty is an epistemic failure | JSON context, final answer | 0/1/2 |

### final_first_check_discriminates

| code | applies to | checks | why | inputs | score |
|---|---|---|---|---|---:|
| FA2 | shared | Whether first_check is exactly one actionable check that distinguishes between active hypotheses or primary vs competing interpretation | This is the core product value — a checklist or vague advice is not a first diagnostic move | JSON context, final answer, active hypotheses, result interpretation | 0/1/2 |

### final_hypothesis_source_alignment

| code | applies to | checks | why | inputs | score |
|---|---|---|---|---|---:|
| FA3 | shared | Whether each hypothesis is supported by its declared source: primary_incident, alternative_context, or theory_mechanism | Explicit source labels are only useful if they are honest — misaligned sources mislead the user about confidence | evidence topology, matched card, incident chunks, theory chunks, final answer | 0/1/2 |

### final_alternative_context_handling

| code | applies to | checks | why | inputs | score |
|---|---|---|---|---|---:|
| FA4 | shared | Whether alternative context is used when genuinely useful and not forced when weak | Protects against both premature convergence and fake symmetry — both are epistemic failures | evidence topology, alternative context chunks, final answer | 0/1/2 |

### final_result_interpretation_usefulness

| code | applies to | checks | why | inputs | score |
|---|---|---|---|---|---:|
| FA5 | shared | Whether supports_primary_if, supports_competing_if, and inconclusive_if explain how to interpret the first check result | Makes the first check operational — without interpretation guidance, the check is decorative | final answer, active hypotheses, first check | 0/1/2 |

### continuation_hypothesis_update_discipline

| code | applies to | checks | why | inputs | score |
|---|---|---|---|---|---:|
| CU1 | continuation only | Whether a continuation response updates hypotheses and the surrounding diagnostic frame in a disciplined way after a new observation | This is the core continuation behavior: the assistant must actually learn from the new observation rather than restating the previous answer | previous validated response, previous active hypotheses, previous first check, new resolved observation, structured extracted observations, current validated response | 0/1/2 |

### continuation_problem_understanding_update

| code | applies to | checks | why | inputs | score |
|---|---|---|---|---|---:|
| CU2 | continuation only | Whether problem_understanding is correctly updated to reflect the new observation without semantic inversion or loss of important state | A continuation response can sound plausible while still misframing the problem; this suite checks whether the updated framing stayed faithful | previous validated response, new resolved observation, structured extracted observations, current validated response | 0/1/2 |

### continuation_next_check_progression

| code | applies to | checks | why | inputs | score |
|---|---|---|---|---|---:|
| CU3 | continuation only | Whether the continuation response proposes a next check that genuinely advances diagnosis after the new observation | A continuation loop is only useful if it converts new evidence into a better next move rather than repeating prior advice | previous first check, previous active hypotheses, new resolved observation, structured extracted observations, current validated response | 0/1/2 |

### continuation_observation_resolution_context_recovery

| code | applies to | checks | why | inputs | score |
|---|---|---|---|---|---:|
| CU4 | continuation only | Whether a short or referential continuation observation was reconstructed into a faithful and useful standalone observation using prior context | Continuation loops often depend on resolving terse follow-up observations; bad reconstruction can poison downstream update quality | previous validated response, previous active hypotheses, raw new observation, resolved observation | 0/1/2 |

