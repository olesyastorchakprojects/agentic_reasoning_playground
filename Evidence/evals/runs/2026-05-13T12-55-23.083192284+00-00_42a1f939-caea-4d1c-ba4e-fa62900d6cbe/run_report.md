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

- eval_run_id: `42a1f939-caea-4d1c-ba4e-fa62900d6cbe`
- run_type: `golden_dataset`
- status: `completed`
- started_at: `2026-05-13 12:55:23.083192284 UTC`
- completed_at: `2026-05-13 14:23:57.427917114 UTC`
- runtime_run_count: `5`
- iterations_evaluated_count: `15`
- judge_model: `openai/gpt-oss-20b`
- query_structuring_model: `openai/gpt-oss-120b`
- observation_boundary_resolver_model: `openai/gpt-oss-20b`
- observation_extraction_model: `openai/gpt-oss-120b`
- llm_structured_generation_model: `openai/gpt-oss-20b`
- query_structuring_prompt_version: `v2`
- observation_boundary_resolver_prompt_version: `v1`
- observation_extraction_prompt_version: `v2`
- prompt_context_prompt_version: `v7`
- diagnostic_update_prompt_context_prompt_version: `v5`
- suite_count: `13`

### Token Pricing

| model | input_price_per_1m | output_price_per_1m |
|---|---:|---:|
| openai/gpt-oss-120b | $0.15/1M | $0.6/1M |
| openai/gpt-oss-20b | $0.05/1M | $0.2/1M |

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
| query_structuring_judge_score | 0.3000 | Judge-based semantic quality of query structuring |
| runtime_query_structuring_core_success_rate | 0.7000 | Gold-backed runtime success of structured query fields |
| runtime_retrieval_mean_ndcg | 0.8310 | Average ranking quality across retrieval targets and runs |
| runtime_retrieval_all_strict_recall_success_rate | 0.9667 | Average per-run share of retrieval targets where strict expected evidence was found |
| evidence_pack_judge_score | 0.5333 | Judge-based quality of selected evidence pack |
| final_answer_judge_score | 1.7067 | Judge-based quality of final diagnostic response |
| usable_continuation_response_rate | 0.4000 | Share of continuation iterations with usable update behavior |
| continuation_update_judge_score | 1.3667 | Judge-based quality of updating the diagnostic frame |
| continuation_input_judge_score | 2.0000 | Judge-based quality of reconstructing the new observation from context |
| continuation_update_strict_pass_rate | 0.4000 | Share of continuation iterations where CU1, CU2, CU3 all scored 2 |

> QS1 = query_structuring_field_boundary_correctness ; QS2 = query_structuring_grounding_conservatism
> EP1 = evidence_pack_role_fit ; EP2 = evidence_pack_sufficiency
> FA1 = final_no_root_cause_claim ; FA2 = final_first_check_discriminates ; FA3 = final_hypothesis_source_alignment ; FA4 = final_alternative_context_handling ; FA5 = final_result_interpretation_usefulness
> CU1 = continuation_hypothesis_update_discipline ; CU2 = continuation_problem_understanding_update ; CU3 = continuation_next_check_progression ; CU4 = continuation_observation_resolution_context_recovery


> See [Appendix D: Suite Overview](#appendix-d-suite-overview) for the detailed suite description.

## Judge-Based Aggregated Metrics

> initial iter-s = all initial iterations across runs; continuation iter-s = all continuation iterations across runs; total ignores n/a

| metric | initial iter-s | continuation iter-s | total | formula |
|---|---:|---:|---:|---|
| usable_first_response_rate | 0.8000 | n/a | 0.8000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 0.9000 | n/a | 0.9000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.6000 | n/a | 1.6000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 1.7600 | 1.6800 | 1.7067 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0.6000 | n/a | 0.6000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1.0000 | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 0.8000 | 0.6000 | 0.6667 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0.2000 | 0.4000 | 0.3333 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0.0000 | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0.2000 | n/a | 0.2000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0.4000 | 0.2000 | 0.2667 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 1.2000 | 1.2000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 1.6000 | 1.6000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 1.3000 | 1.3000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2.0000 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 0.4000 | 0.4000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 1.3667 | 1.3667 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 0.5000 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0.4000 | 0.4000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
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

### Query Structuring Core Metrics

| metric | value | meaning |
|---|---:|---|
| runtime_query_structuring_macro_precision_soft | 0.6583 | How many selected vocabulary terms are acceptable under soft relevance |
| runtime_query_structuring_macro_recall_strict | 0.7000 | Whether strictly expected terms were recovered |
| runtime_query_structuring_macro_recall_soft | 0.5500 | Coverage of broader acceptable terms |
| runtime_query_structuring_grounded_strict_recall | 0.7000 | Whether strict terms are selected with valid grounding |
| runtime_query_structuring_core_success_rate | 0.7000 | Whether all vocab fields passed their core gold-backed checks |

#### Query Structuring Field Core Metrics

| field | precision_soft | recall_strict | recall_soft | grounded_strict_recall | field_core_success | field_grounded_success |
|---|---:|---:|---:|---:|---:|---:|
| symptoms | 0.7000 | 0.6000 | 0.4000 | 0.6000 | 0.6000 | 0.6000 |
| affected_subsystems | 0.6333 | 0.8000 | 0.7000 | 0.8000 | 0.8000 | 0.8000 |
| failure_modes | 0.8000 | 0.8000 | 0.5000 | 0.8000 | 0.8000 | 0.8000 |
| system_properties | 0.5000 | 0.6000 | 0.6000 | 0.6000 | 0.6000 | 0.6000 |

### Retrieval Core Metrics

> Each value is averaged over runs where the target was evaluated.

| retrieval_target | evaluated_k | recall_strict | recall_soft | rr_strict | rr_soft | nDCG | frr_strict | frr_soft | n_strict | n_soft |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| candidate_cards | 8.0 | 1.0000 | 0.9333 | 1.0000 | 1.0000 | 0.9681 | 1.00 | 1.00 | 1.00 | 2.80 |
| incident_primary | 12.0 | 1.0000 | 0.9778 | 0.8689 | 0.9222 | 0.9106 | 1.47 | 1.20 | 1.40 | 3.33 |
| incident_alternatives | 12.0 | 0.8667 | 0.7333 | 0.7500 | 0.8500 | 0.7399 | 1.38 | 1.29 | 0.87 | 2.20 |
| theory_evidence | 12.0 | 1.0000 | 0.8667 | 0.5711 | 0.6822 | 0.7054 | 2.53 | 2.27 | 1.00 | 1.73 |

### Retrieval Summary

| metric | value | formula | meaning |
|---|---:|---|---|
| runtime_retrieval_mean_ndcg | 0.8310 | avg_run(avg_target(ndcg)) | Average ranking quality across retrieval targets and runs |
| runtime_retrieval_all_strict_recall_success_rate | 0.9667 | avg_run(frac_target(recall_strict=1)) | Average per-run share of retrieval targets with strict recall success |
| runtime_retrieval_all_soft_recall_success_rate | 0.9833 | avg_run(frac_target(recall_soft>0)) | Average per-run share of retrieval targets with any soft recall |
| runtime_retrieval_penalized_first_relevant_rank_strict | 1.98 | avg_run(avg_target(frr_strict or k+1)) | Penalized rank; missing strict hit treated as k+1 |
| runtime_retrieval_zero_hit_rate | 0.0000 | avg_run(frac_target(hits_count=0)) | Average per-run share of retrieval calls with no hits |

## Suite Distributions

| suite | score_0 | score_1 | score_2 |
|---|---:|---:|---:|
| final_no_root_cause_claim | 1 | 2 | 12 |
| final_first_check_discriminates | 1 | 0 | 14 |
| final_alternative_context_handling | 4 | 0 | 11 |
| final_result_interpretation_usefulness | 0 | 0 | 15 |
| final_hypothesis_source_alignment | 0 | 8 | 7 |
| query_structuring_field_boundary_correctness | 12 | 2 | 1 |
| query_structuring_grounding_conservatism | 10 | 5 | 0 |
| evidence_pack_role_fit | 10 | 4 | 1 |
| evidence_pack_sufficiency | 10 | 0 | 5 |
| continuation_hypothesis_update_discipline | 4 | 0 | 6 |
| continuation_problem_understanding_update | 2 | 0 | 8 |
| continuation_next_check_progression | 3 | 1 | 6 |
| continuation_observation_resolution_context_recovery | 0 | 0 | 10 |

## Gate Breakdown

| gate | fail_count | fail_rate |
|---|---:|---:|
| final_no_root_cause_claim | 1 | 0.0667 |
| final_first_check_discriminates | 1 | 0.0667 |
| final_hypothesis_source_alignment | 0 | 0.0000 |
| query_structuring_field_boundary_correctness | 2 | 0.1333 |
| evidence_pack_sufficiency | 0 | 0.0000 |
| continuation_hypothesis_update_discipline | 4 | 0.4000 |
| continuation_problem_understanding_update | 2 | 0.2000 |
| continuation_next_check_progression | 3 | 0.3000 |
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
| bad_continuation_due_to_update_logic_rate | 0.5000 | frac(!usable_continuation ∧ CU4>0 ∧ (CU1=0 ∨ CU2=0 ∨ CU3=0)) |
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
| query structuring | judge 0.30, no-hard-fail 87%, runtime core 70% | mixed | no strict pass on any run; 2 field boundary gate fail(s); runtime core success 70% |
| retrieval | strict recall 97%, nDCG 0.83 | strong | recall was present, but ranking quality remained weak (nDCG 0.83) |
| evidence packing | judge 0.53, no-hard-fail 100% | mixed | 10 run(s) with insufficient evidence pack (EP2=0) |
| final answer | usable 87%, judge 1.71, no-hard-fail 67% | strong | 1 premature certainty (FA1=0); 1 vague first check (FA2=0); 8 partial source alignment (FA3<2) |
| continuation | usable cont 40%, update score 1.37, input score 2.00, no-hard-fail 50% | weak | 4 hard fail(s) on hypothesis update discipline (CU1=0) |

### Failure Path

2 of 15 responses were unusable.

- 2 unusable despite good query + evidence → **final answer stage failure**
  - 1 × FA1=0: premature certainty or root cause claim
  - 1 × FA2=0: vague or non-discriminating first check

Continuation was the main observed degradation point:

- 4 hypothesis update discipline hard fail(s) (CU1=0)
- 2 problem understanding update hard fail(s) (CU2=0)
- 3 next check progression hard fail(s) (CU3=0)

Quality degraded between initial and continuation iterations.

Main observed weakness: **query structuring** (composite 0.57). retrieval and final answer quality was strong.

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
| `62935203-16f1-463c-b496-66e0f17c9a4c` | `10b6facb-9ebb-4c85-94b2-efc62f4f92c1` | 1.2000 | false |
| `e19bbabe-2038-41bc-bb43-da1c8f266640` | `b3521ea3-6f1e-4377-8226-95a42ed71a72` | 1.2000 | true |
| `2bd80389-3abc-4a94-9346-6320d4021394` | `1e28cca2-02c5-4b59-993b-bb2a2ca64b9c` | 1.4000 | false |
| `d70109a2-7433-4deb-acb2-3733d9b676a4` | `7a6b6a21-7e9e-420d-8fb1-f027993c0fdd` | 1.6000 | true |
| `d70109a2-7433-4deb-acb2-3733d9b676a4` | `f99b71ea-ddb8-4384-b522-c88b5320b8b7` | 1.6000 | true |

## Token Usage

### Judge Calls by Suite

| suite | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| continuation_hypothesis_update_discipline | 15819 | 7422 | 23241 | 0.002275 |
| continuation_next_check_progression | 10966 | 6099 | 17065 | 0.001768 |
| continuation_observation_resolution_context_recovery | 9625 | 4577 | 14202 | 0.001397 |
| continuation_problem_understanding_update | 13400 | 5727 | 19127 | 0.001815 |
| evidence_pack_role_fit | 15353 | 5296 | 20649 | 0.001827 |
| evidence_pack_sufficiency | 21810 | 6201 | 28011 | 0.002331 |
| final_alternative_context_handling | 76223 | 5794 | 82017 | 0.004970 |
| final_first_check_discriminates | 77138 | 5930 | 83068 | 0.005043 |
| final_hypothesis_source_alignment | 65851 | 9432 | 75283 | 0.005179 |
| final_no_root_cause_claim | 76448 | 5805 | 82253 | 0.004983 |
| final_result_interpretation_usefulness | 12734 | 8816 | 21550 | 0.002400 |
| query_structuring_field_boundary_correctness | 3658 | 3261 | 6919 | 0.000835 |
| query_structuring_grounding_conservatism | 3508 | 5136 | 8644 | 0.001203 |
| judge_total | 402533 | 79496 | 482029 | 0.036026 |

| model | prompt_tokens_cost | completion_tokens_cost | total_cost_usd |
|---|---|---|---:|
| openai/gpt-oss-20b | 402533 * $0.05/1M = 0.020127 | 79496 * $0.2/1M = 0.015899 | 0.036026 |

### Runtime by Stage

| scope | model | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---|---:|---:|---:|---:|
| query_structuring | openai/gpt-oss-120b | 6912 | 5030 | 11942 | 0.004055 |
| observation_boundary_resolver | openai/gpt-oss-20b | 7179 | 2971 | 10150 | 0.000953 |
| observation_extraction | openai/gpt-oss-120b | 4192 | 5274 | 9466 | 0.003793 |
| llm_structured_generation | openai/gpt-oss-20b | 58397 | 24401 | 82798 | 0.007800 |
| runtime_total | — | 76680 | 37676 | 114356 | 0.016601 |

| model | prompt_tokens_cost | completion_tokens_cost | total_cost_usd |
|---|---|---|---:|
| openai/gpt-oss-120b | 11104 * $0.15/1M = 0.001666 | 10304 * $0.6/1M = 0.006182 | 0.007848 |
| openai/gpt-oss-20b | 65576 * $0.05/1M = 0.003279 | 27372 * $0.2/1M = 0.005474 | 0.008753 |
| runtime_total | sum(stage prompt costs) = 0.004944 | sum(stage completion costs) = 0.011657 | 0.016601 |

### Totals

| scope | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| runtime | 76680 | 37676 | 114356 | 0.016601 |
| judge_total | 402533 | 79496 | 482029 | 0.036026 |
| run_total | 479213 | 117172 | 596385 | 0.052627 |

Run total cost usd = runtime total cost usd + judge total cost usd = 0.016601 + 0.036026 = 0.052627

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
| affected_subsystems | 2.00 | 0.80 | 0.20 | 0.80 |
| failure_modes | 1.00 | 0.20 | 0.20 | 0.20 |
| system_properties | 1.80 | 0.80 | 0.40 | 0.80 |

### A.3 Graded Relevance Diagnostics

| field | graded_coverage | average_selected_score |
|---|---:|---:|
| symptoms | 0.4667 | 0.6500 |
| affected_subsystems | 0.7333 | 0.5500 |
| failure_modes | 0.6000 | 0.8000 |
| system_properties | 0.6000 | 0.4000 |

### A.4 Grounding Diagnostics

| field | unsupported_selected_term_rate | missing_evidence_span_count | invalid_evidence_span_count | evidence_span_near_substring_rate |
|---|---:|---:|---:|---:|
| symptoms | 0.0000 | 0.00 | 0.00 | 1.0000 |
| affected_subsystems | 0.0000 | 0.00 | 0.00 | 1.0000 |
| failure_modes | 0.0000 | 0.00 | 0.00 | 1.0000 |
| system_properties | 0.0000 | 0.00 | 0.00 | 1.0000 |

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
| system_properties | 0.6000 | 0.6000 | 0.0000 |

### A.7 Query-Level Non-Vocabulary Diagnostics

| entities_count_avg | constraints_count_avg | triggers_count_avg | observability_signals_count_avg | unresolved_terms_count_avg | intent_present_rate | scenario_present_rate |
|---:|---:|---:|---:|---:|---:|---:|
| 2.80 | 0.00 | 0.20 | 0.60 | 0.00 | 1.0000 | 1.0000 |

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
| candidate_cards | 7.5 | 3.0 | 0.9778 | 0.2329 |
| incident_primary | 7.5 | 7.5 | 0.7711 | 0.2752 |
| incident_alternatives | 8.7 | 8.7 | 0.5484 | 0.2000 |
| theory_evidence | 7.7 | 7.7 | 0.6439 | 0.2000 |

## Appendix C: Judge Metrics Per Run

### Run `0d6b8f2c-f216-4182-b7d9-7098a324c3bb`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 1 | 1 | 1.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 1.0000 | n/a | n/a | 1.0000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 1.8000 | 1.8000 | 2.0000 | 1.8667 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 1 | 1 | 1.0000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 0 | 0 | 0.0000 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0 | 0 | 1 | 0.3333 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 2 | 0 | 1.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 2 | 2.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2 | 0 | 1.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 1 | 0 | 0.5000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 2.0000 | 0.6667 | 1.3333 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 1 | 0 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 1 | 0 | 0.5000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

### Run `2bd80389-3abc-4a94-9346-6320d4021394`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 0 | 1 | 1 | 0.6667 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 1.0000 | n/a | n/a | 1.0000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 1.4000 | 1.8000 | 1.8000 | 1.6667 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 0 | 1 | 1 | 0.6667 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 1 | 0 | 0 | 0.3333 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0 | 0 | 0 | 0.0000 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 0 | 2 | 1.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 2 | 2.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2 | 1 | 1.5000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 0 | 1 | 0.5000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 1.3333 | 1.6667 | 1.5000 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 0 | 1 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0 | 0 | 0.0000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

### Run `62935203-16f1-463c-b496-66e0f17c9a4c`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 0 | 1 | 0.6667 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 2.0000 | n/a | n/a | 2.0000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 2.0000 | 1.2000 | 2.0000 | 1.7333 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 0 | 1 | 0.6667 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 1 | 0 | 0.3333 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 1 | n/a | n/a | 1.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 1 | 0 | 1 | 0.6667 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 2 | 0 | 1.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 0 | 1.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2 | 2 | 2.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 0 | 0 | 0.0000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 2.0000 | 0.6667 | 1.3333 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 1 | 0 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 1 | 0 | 0.5000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

### Run `d70109a2-7433-4deb-acb2-3733d9b676a4`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 1 | 1 | 1.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 0.5000 | n/a | n/a | 0.5000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 2.0000 | 1.6000 | 1.6000 | 1.7333 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0 | n/a | n/a | 0.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 0 | 0 | 0.3333 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 1 | 1 | 0.6667 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 1 | 0 | 0 | 0.3333 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 0 | 2 | 1.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 0 | 2 | 1.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 0 | 2 | 1.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 0 | 1 | 0.5000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 0.0000 | 2.0000 | 1.0000 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 0 | 1 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0 | 1 | 0.5000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

### Run `e19bbabe-2038-41bc-bb43-da1c8f266640`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 1 | 1 | 1.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 0.5000 | n/a | n/a | 0.5000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 1.6000 | 1.8000 | 1.2000 | 1.5333 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0 | n/a | n/a | 0.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 1 | 0 | 0.6667 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 0 | 1 | 0.3333 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0 | 0 | 0 | 0.0000 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 2 | 2 | 2.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 2 | 2.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 0 | 2 | 1.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 0 | 1 | 0.5000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 1.3333 | 2.0000 | 1.6667 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 0 | 1 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0 | 1 | 0.5000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
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

