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

- eval_run_id: `d2e574d4-ce0b-4471-9a42-76822f8f12dc`
- run_type: `golden_dataset`
- status: `completed`
- started_at: `2026-05-20 16:46:08.735070640 UTC`
- completed_at: `2026-05-20 17:13:02.545270122 UTC`
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
| usable_first_response_rate | 1.0000 | Share of runs where the final answer can be shown as a first diagnostic response |
| gate_pass_rate | 0.6667 | Share of runs without critical gate failures |
| query_structuring_judge_score | 1.0000 | Judge-based semantic quality of query structuring |
| runtime_query_structuring_macro_precision_soft | 0.8500 | Share of selected query-structuring terms that fall within the acceptable semantic set |
| runtime_query_structuring_grounded_strict_recall | 0.5500 | Coverage of canonical expected query-structuring terms with valid grounding |
| runtime_retrieval_mean_ndcg | 0.7768 | Average ranking quality across retrieval targets and runs |
| runtime_retrieval_all_strict_recall_success_rate | 0.9333 | Average per-run share of retrieval targets where strict expected evidence was found |
| evidence_pack_judge_score | 1.8000 | Judge-based quality of selected evidence pack |
| final_answer_judge_score | 1.7467 | Judge-based quality of final diagnostic response |
| usable_continuation_response_rate | 0.6000 | Share of continuation iterations with usable update behavior |
| continuation_update_judge_score | 1.6000 | Judge-based quality of updating the diagnostic frame |
| continuation_input_judge_score | 1.9000 | Judge-based quality of reconstructing the new observation from context |
| continuation_update_strict_pass_rate | 0.1000 | Share of continuation iterations where CU1, CU2, CU3 all scored 2 |

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
| query_structuring_judge_score | 1.0000 | n/a | 1.0000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.8000 | n/a | 1.8000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 1.9200 | 1.6600 | 1.7467 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0.4000 | n/a | 0.4000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1.0000 | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1.0000 | 0.6000 | 0.7333 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0.0000 | 0.4000 | 0.2667 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0.0000 | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0.6000 | n/a | 0.6000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0.6000 | 0.1000 | 0.2667 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 1.6000 | 1.6000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 1.8000 | 1.8000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 1.4000 | 1.4000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 1.9000 | 1.9000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 0.6000 | 0.6000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 1.6000 | 1.6000 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 0.7000 | 0.7000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0.1000 | 0.1000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 1.9000 | 1.9000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1.0000 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 0.9000 | 0.9000 | frac(CU4=2) |

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
| runtime_query_structuring_macro_precision_soft | 0.8500 | Share of selected vocabulary terms that fall within the acceptable semantic set |
| runtime_query_structuring_macro_recall_soft | 0.5033 | Coverage of broader acceptable vocabulary terms |
| runtime_query_structuring_macro_recall_strict | 0.6000 | Coverage of canonical expected vocabulary terms |
| runtime_query_structuring_grounded_strict_recall | 0.5500 | Coverage of canonical expected terms with valid grounding |

#### Query Structuring Field Quality Profile

| field | precision_soft | recall_soft | recall_strict | grounded_strict_recall | field_core_success | field_grounded_success |
|---|---:|---:|---:|---:|---:|---:|
| symptoms | 1.0000 | 0.4333 | 0.6000 | 0.6000 | 0.6000 | 0.6000 |
| affected_subsystems | 0.8000 | 0.6300 | 0.8000 | 0.8000 | 0.8000 | 0.8000 |
| failure_modes | 1.0000 | 0.5667 | 0.8000 | 0.8000 | 0.8000 | 0.8000 |
| system_properties | 0.6000 | 0.3833 | 0.2000 | 0.0000 | 0.2000 | 0.0000 |

#### Query Structuring Field Overreach Signals

| field | zero_score_selection_count | unsupported_selected_term_rate | invalid_evidence_span_count |
|---|---:|---:|---:|
| symptoms | 0.00 | 0.0000 | 0.00 |
| affected_subsystems | 0.20 | 0.0000 | 0.00 |
| failure_modes | 0.00 | 0.0000 | 0.00 |
| system_properties | 0.40 | 0.3000 | 0.40 |

#### Query Structuring Strict Contract Checks

| metric | value | meaning |
|---|---:|---|
| runtime_query_structuring_core_success_rate | 0.6000 | Harsh all-fields contract pass rate against strict field expectations |

### Retrieval Core Metrics

> Each value is averaged over runs where the target was evaluated.

| retrieval_target | evaluated_k | recall_strict | recall_soft | rr_strict | rr_soft | nDCG | frr_strict | frr_soft | n_strict | n_soft |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| candidate_cards | 8.0 | 1.0000 | 0.8667 | 1.0000 | 1.0000 | 0.9333 | 1.00 | 1.00 | 1.00 | 2.60 |
| incident_primary | 12.0 | 1.0000 | 0.9778 | 0.8244 | 0.9111 | 0.8827 | 1.73 | 1.27 | 1.40 | 3.33 |
| incident_alternatives | 12.0 | 0.8000 | 0.6000 | 0.6444 | 0.6556 | 0.6014 | 2.00 | 1.75 | 0.80 | 1.80 |
| theory_evidence | 12.0 | 0.9333 | 0.8000 | 0.5744 | 0.7611 | 0.6898 | 2.36 | 1.87 | 0.93 | 1.60 |

### Retrieval Summary

| metric | value | formula | meaning |
|---|---:|---|---|
| runtime_retrieval_mean_ndcg | 0.7768 | avg_run(avg_target(ndcg)) | Average ranking quality across retrieval targets and runs |
| runtime_retrieval_all_strict_recall_success_rate | 0.9333 | avg_run(frac_target(recall_strict=1)) | Average per-run share of retrieval targets with strict recall success |
| runtime_retrieval_all_soft_recall_success_rate | 0.9500 | avg_run(frac_target(recall_soft>0)) | Average per-run share of retrieval targets with any soft recall |
| runtime_retrieval_penalized_first_relevant_rank_strict | 2.50 | avg_run(avg_target(frr_strict or k+1)) | Penalized rank; missing strict hit treated as k+1 |
| runtime_retrieval_zero_hit_rate | 0.0000 | avg_run(frac_target(hits_count=0)) | Average per-run share of retrieval calls with no hits |

## Suite Distributions

| suite | score_0 | score_1 | score_2 |
|---|---:|---:|---:|
| final_no_root_cause_claim | 3 | 0 | 12 |
| final_first_check_discriminates | 0 | 0 | 15 |
| final_alternative_context_handling | 1 | 1 | 13 |
| final_result_interpretation_usefulness | 0 | 0 | 15 |
| final_hypothesis_source_alignment | 2 | 6 | 7 |
| query_structuring_field_boundary_correctness | 0 | 2 | 3 |
| query_structuring_grounding_conservatism | 3 | 2 | 0 |
| evidence_pack_role_fit | 0 | 2 | 3 |
| evidence_pack_sufficiency | 0 | 0 | 5 |
| continuation_hypothesis_update_discipline | 0 | 4 | 6 |
| continuation_problem_understanding_update | 1 | 0 | 9 |
| continuation_next_check_progression | 2 | 2 | 6 |
| continuation_observation_resolution_context_recovery | 0 | 1 | 9 |

## Gate Breakdown

| gate | fail_count | fail_rate |
|---|---:|---:|
| final_no_root_cause_claim | 3 | 0.2000 |
| final_first_check_discriminates | 0 | 0.0000 |
| final_hypothesis_source_alignment | 2 | 0.1333 |
| query_structuring_field_boundary_correctness | 0 | 0.0000 |
| evidence_pack_sufficiency | 0 | 0.0000 |
| continuation_hypothesis_update_discipline | 0 | 0.0000 |
| continuation_problem_understanding_update | 1 | 0.1000 |
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
| bad_final_with_good_query_and_evidence_rate | 0.2000 | frac(!usable ∧ QS1>0 ∧ QS2>0 ∧ EP1>0 ∧ EP2>0) |

> usable = FA1≥1 ∧ FA2≥1 ∧ FA5≥1

### Continuation Attribution

> `frac(condition)` uses only continuation iterations where all fields referenced by that formula are present; missing values are excluded from both numerator and denominator.

| metric | value | formula |
|---|---:|---|
| bad_continuation_due_to_input_resolution_rate | 0.0000 | frac(!usable_continuation ∧ CU4=0) |
| bad_continuation_due_to_update_logic_rate | 0.3000 | frac(!usable_continuation ∧ CU4>0 ∧ (CU1=0 ∨ CU2=0 ∨ CU3=0)) |
| bad_continuation_despite_good_input_rate | 0.4000 | frac(!usable_continuation ∧ CU4=2) |
| good_continuation_despite_input_issue_rate | 0.1000 | frac(usable_continuation ∧ CU4<2) |

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
| query structuring | judge 1.00, no-hard-fail 40%, soft precision 85%, grounded strict 55% | mixed | no strict pass on any run; grounded strict recall 55%; strict contract pass 60% |
| retrieval | strict recall 93%, nDCG 0.78 | strong | recall was present, but ranking quality remained weak (nDCG 0.78) |
| evidence packing | judge 1.80, no-hard-fail 100% | strong | selected evidence pack was sufficient and mostly role-appropriate |
| final answer | usable 100%, judge 1.75, no-hard-fail 73% | strong | 3 premature certainty (FA1=0); 8 partial source alignment (FA3<2) |
| continuation | usable cont 60%, update score 1.60, input score 1.90, no-hard-fail 70% | mixed | partial degradation: usable rate 60%, no-hard-fail 70% |

### Failure Path

3 of 15 responses were unusable.

- 3 unusable despite good query + evidence → **final answer stage failure**
  - 3 × FA1=0: premature certainty or root cause claim

Continuation was the main observed degradation point:

- 1 problem understanding update hard fail(s) (CU2=0)
- 2 next check progression hard fail(s) (CU3=0)

Quality degraded between initial and continuation iterations.

Main observed weakness: **query structuring**. retrieval and evidence packing and final answer quality was strong.

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
| `20d71364-f74c-46f9-a7a6-da260d3cd847` | `1a9fc53f-e1da-4a8a-b357-f16932b115c3` | 1.4000 | true |
| `fa4f888a-92e1-40c8-8b28-f7d020c5f39b` | `9835e09c-7eb3-49eb-aced-01908571c281` | 1.4000 | false |
| `20d71364-f74c-46f9-a7a6-da260d3cd847` | `dd0cd01b-0e99-4831-8b74-7e5e6352ed69` | 1.6000 | false |
| `5266db67-1df0-4a06-bde4-10712c688963` | `a4d9ce98-beeb-4535-9708-13a10f293294` | 1.6000 | true |
| `8bc933a8-dfff-4c72-97a8-591a02527834` | `f0010137-bf23-498f-8f71-07ae21b5a4b1` | 1.6000 | true |

## Token Usage

### Judge Calls by Suite

| suite | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| continuation_hypothesis_update_discipline | 17701 | 7851 | 25552 | 0.002455 |
| continuation_next_check_progression | 12294 | 6472 | 18766 | 0.001909 |
| continuation_observation_resolution_context_recovery | 11026 | 4200 | 15226 | 0.001391 |
| continuation_problem_understanding_update | 14812 | 4713 | 19525 | 0.001683 |
| evidence_pack_role_fit | 15742 | 3857 | 19599 | 0.001558 |
| evidence_pack_sufficiency | 22204 | 2327 | 24531 | 0.001576 |
| final_alternative_context_handling | 76185 | 6281 | 82466 | 0.005065 |
| final_first_check_discriminates | 77295 | 5503 | 82798 | 0.004965 |
| final_hypothesis_source_alignment | 65421 | 9211 | 74632 | 0.005113 |
| final_no_root_cause_claim | 76875 | 4931 | 81806 | 0.004830 |
| final_result_interpretation_usefulness | 14640 | 5944 | 20584 | 0.001921 |
| query_structuring_field_boundary_correctness | 4138 | 2597 | 6735 | 0.000726 |
| query_structuring_grounding_conservatism | 3998 | 4166 | 8164 | 0.001033 |
| judge_total | 412331 | 68053 | 480384 | 0.034227 |

| model | prompt_tokens_cost | completion_tokens_cost | total_cost_usd |
|---|---|---|---:|
| openai/gpt-oss-120b | 412331 * $0.05/1M = 0.020617 | 68053 * $0.2/1M = 0.013611 | 0.034227 |

### Runtime by Stage

| scope | model | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---|---:|---:|---:|---:|
| query_structuring | openai/gpt-oss-120b | 6912 | 5259 | 12171 | 0.004192 |
| observation_boundary_resolver | openai/gpt-oss-120b | 7442 | 2352 | 9794 | 0.002528 |
| observation_extraction | openai/gpt-oss-120b | 4202 | 5442 | 9644 | 0.003895 |
| llm_structured_generation | openai/gpt-oss-120b | 56884 | 21261 | 78145 | 0.021289 |
| runtime_total | — | 75440 | 34314 | 109754 | 0.031904 |

| model | prompt_tokens_cost | completion_tokens_cost | total_cost_usd |
|---|---|---|---:|
| openai/gpt-oss-120b | 75440 * $0.15/1M = 0.011316 | 34314 * $0.6/1M = 0.020588 | 0.031904 |
| runtime_total | sum(stage prompt costs) = 0.011316 | sum(stage completion costs) = 0.020588 | 0.031904 |

### Totals

| scope | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| runtime | 75440 | 34314 | 109754 | 0.031904 |
| judge_total | 412331 | 68053 | 480384 | 0.034227 |
| run_total | 487771 | 102367 | 590138 | 0.066132 |

Run total cost usd = runtime total cost usd + judge total cost usd = 0.031904 + 0.034227 = 0.066132

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
| symptoms | 1.20 | 0.00 | 0.40 | 0.00 |
| affected_subsystems | 2.20 | 0.20 | 0.20 | 0.20 |
| failure_modes | 1.00 | 0.00 | 0.20 | 0.00 |
| system_properties | 1.40 | 0.40 | 0.80 | 0.40 |

### A.3 Graded Relevance Diagnostics

| field | graded_coverage | average_selected_score |
|---|---:|---:|
| symptoms | 0.4967 | 0.8000 |
| affected_subsystems | 0.6600 | 0.5667 |
| failure_modes | 0.6500 | 0.9000 |
| system_properties | 0.3400 | 0.3500 |

### A.4 Grounding Diagnostics

| field | unsupported_selected_term_rate | missing_evidence_span_count | invalid_evidence_span_count | evidence_span_near_substring_rate |
|---|---:|---:|---:|---:|
| symptoms | 0.0000 | 0.00 | 0.00 | 1.0000 |
| affected_subsystems | 0.0000 | 0.00 | 0.00 | 1.0000 |
| failure_modes | 0.0000 | 0.00 | 0.00 | 1.0000 |
| system_properties | 0.3000 | 0.00 | 0.40 | 0.7000 |

### A.5 Support-Level Diagnostics

| field | weak_inference_rate | strict_terms_weak_inference_rate | weak_false_positive_rate |
|---|---:|---:|---:|
| symptoms | 0.0000 | 0.0000 | 0.0000 |
| affected_subsystems | 0.0000 | 0.0000 | 0.0000 |
| failure_modes | 0.0000 | 0.0000 | 0.0000 |
| system_properties | 0.0000 | 0.0000 | 0.0000 |

### A.6 Field Success Diagnostics

| field | field_core_success | field_grounded_success | empty_when_gold_exists |
|---|---:|---:|---:|
| symptoms | 0.6000 | 0.6000 | 0.0000 |
| affected_subsystems | 0.8000 | 0.8000 | 0.0000 |
| failure_modes | 0.8000 | 0.8000 | 0.0000 |
| system_properties | 0.2000 | 0.0000 | 0.0000 |

### A.7 Query-Level Non-Vocabulary Diagnostics

| entities_count_avg | constraints_count_avg | triggers_count_avg | observability_signals_count_avg | unresolved_terms_count_avg | intent_present_rate | scenario_present_rate |
|---:|---:|---:|---:|---:|---:|---:|
| 2.80 | 0.20 | 0.40 | 1.00 | 0.00 | 1.0000 | 1.0000 |

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
| candidate_cards | 7.3 | 3.0 | 0.9444 | 0.2309 |
| incident_primary | 8.2 | 8.2 | 0.7672 | 0.2717 |
| incident_alternatives | 8.5 | 8.5 | 0.5707 | 0.2000 |
| theory_evidence | 7.5 | 7.5 | 0.7167 | 0.2000 |

## Appendix C: Judge Metrics Per Run

### Run `20d71364-f74c-46f9-a7a6-da260d3cd847`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 1 | 0 | 0.6667 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 2.0000 | n/a | n/a | 2.0000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 2.0000 | 1.4000 | 1.6000 | 1.6667 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 0 | 0 | 0.3333 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 1 | 1 | 0.6667 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 1 | n/a | n/a | 1.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 1 | 0 | 0 | 0.3333 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 1 | 2 | 1.5000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 2 | 2.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2 | 0 | 1.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 1 | 0 | 0.5000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 1.6667 | 1.3333 | 1.5000 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 1 | 0 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0 | 0 | 0.0000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

### Run `5266db67-1df0-4a06-bde4-10712c688963`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 1 | 1 | 1.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 1.8000 | 1.8000 | 1.6000 | 1.7333 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 1 | 1 | 1.0000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 0 | 0 | 0.0000 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0 | 0 | 0 | 0.0000 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 2 | 1 | 1.5000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 2 | 2.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2 | 2 | 2.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 1 | 1.5000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 1 | 1 | 1.0000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 2.0000 | 1.6667 | 1.8333 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 1 | 0 | 0.5000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 1 | 1.5000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 0 | 0.5000 | frac(CU4=2) |

### Run `8bc933a8-dfff-4c72-97a8-591a02527834`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 1 | 1 | 1.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 1.0000 | n/a | n/a | 1.0000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 2.0000 | n/a | n/a | 2.0000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 2.0000 | 1.8000 | 1.6000 | 1.8000 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0 | n/a | n/a | 0.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 1 | 1 | 1.0000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 0 | 0 | 0.0000 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 1 | n/a | n/a | 1.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 1 | 0 | 0 | 0.3333 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 2 | 2 | 2.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 2 | 2.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 1 | 0 | 0.5000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 1 | 0 | 0.5000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 1.6667 | 1.3333 | 1.5000 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 1 | 0 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0 | 0 | 0.0000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

### Run `8ebc52eb-4a9a-40af-a3d4-16f4845c90bf`

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
| continuation_hypothesis_update_discipline_score | n/a | 1 | 2 | 1.5000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 2 | 2.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2 | 1 | 1.5000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 1 | 1 | 1.0000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 1.6667 | 1.6667 | 1.6667 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0 | 0 | 0.0000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

### Run `fa4f888a-92e1-40c8-8b28-f7d020c5f39b`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 0 | 0 | 0.3333 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 0.5000 | n/a | n/a | 0.5000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 2.0000 | n/a | n/a | 2.0000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 1.8000 | 1.6000 | 1.4000 | 1.6000 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0 | n/a | n/a | 0.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 0 | 0 | 0.3333 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 1 | 1 | 0.6667 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 1 | n/a | n/a | 1.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0 | 0 | 0 | 0.0000 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 1 | 2 | 1.5000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 0 | 1.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2 | 2 | 2.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 0 | 0 | 0.0000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 1.6667 | 1.3333 | 1.5000 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 1 | 0 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0 | 0 | 0.0000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
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

