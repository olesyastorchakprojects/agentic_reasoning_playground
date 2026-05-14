# Eval Run Report

## Run Metadata

- eval_run_id: `a8dcfe2a-6205-4bd0-95b8-f5db0e398ac5`
- run_type: `golden_dataset`
- status: `completed`
- started_at: `2026-05-13 11:15:49.428743516 UTC`
- completed_at: `2026-05-13 12:55:13.544477506 UTC`
- runtime_run_count: `5`
- iterations_evaluated_count: `14`
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
| openai/gpt-oss-120b | $0.0/1M | $0.0/1M |
| openai/gpt-oss-20b | $0.05/1M | $0.2/1M |

## Suite Overview

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

## Metric Layers

| layer | source | evaluates | interpretation |
|---|---|---|---|
| Judge-based quality metrics | judge model outputs | semantic quality of structuring, evidence pack, and final answer | answers whether the diagnostic behavior is good |
| Runtime gold metrics | runtime trace spans with golden labels | query structuring and retrieval against expected labels / evidence | answers whether upstream modules selected the expected terms and evidence |
| Runtime diagnostics | runtime trace attributes and events | low-level counters, hit counts, configuration, support-level issues | helps debug why a metric failed |

## Executive Summary

| metric | value | meaning |
|---|---:|---|
| usable_first_response_rate | 0.7143 | Share of runs where the final answer can be shown as a first diagnostic response |
| gate_pass_rate | 0.5714 | Share of runs without critical gate failures |
| query_structuring_judge_score | 0.3214 | Judge-based semantic quality of query structuring |
| runtime_query_structuring_core_success_rate | 0.6964 | Gold-backed runtime success of structured query fields |
| runtime_retrieval_mean_ndcg | 0.8197 | Average ranking quality across retrieval targets and runs |
| runtime_retrieval_all_strict_recall_success_rate | 0.9464 | Average per-run share of retrieval targets where strict expected evidence was found |
| evidence_pack_judge_score | 0.4286 | Judge-based quality of selected evidence pack |
| final_answer_judge_score | 1.7714 | Judge-based quality of final diagnostic response |
| usable_continuation_response_rate | 0.4000 | Share of continuation iterations with usable update behavior |
| continuation_update_judge_score | 1.4667 | Judge-based quality of updating the diagnostic frame |
| continuation_input_judge_score | 2.0000 | Judge-based quality of reconstructing the new observation from context |
| continuation_update_strict_pass_rate | 0.4000 | Share of continuation iterations where CU1, CU2, CU3 all scored 2 |

## Judge-Based Aggregated Metrics

> initial iter-s = all initial iterations across runs; continuation iter-s = all continuation iterations across runs; total ignores n/a

| metric | initial iter-s | continuation iter-s | total | formula |
|---|---:|---:|---:|---|
| usable_first_response_rate | 0.7500 | n/a | 0.7500 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 1.1250 | n/a | 1.1250 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 1.9000 | 1.7200 | 1.7714 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0.7500 | n/a | 0.7500 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1.0000 | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 0.7500 | 0.7000 | 0.7143 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0.2500 | 0.3000 | 0.2857 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0.0000 | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0.0000 | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0.7500 | 0.3000 | 0.4286 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 1.4000 | 1.4000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 1.6000 | 1.6000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 1.4000 | 1.4000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2.0000 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 0.4000 | 0.4000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 1.4667 | 1.4667 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 0.5000 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0.4000 | 0.4000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2.0000 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1.0000 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1.0000 | 1.0000 | frac(CU4=2) |

> QS1 = query_structuring_field_boundary_correctness ; QS2 = query_structuring_grounding_conservatism
> EP1 = evidence_pack_role_fit ; EP2 = evidence_pack_sufficiency
> FA1 = final_no_root_cause_claim ; FA2 = final_first_check_discriminates ; FA3 = final_hypothesis_source_alignment ; FA4 = final_alternative_context_handling ; FA5 = final_result_interpretation_usefulness
> CU1 = continuation_hypothesis_update_discipline ; CU2 = continuation_problem_understanding_update ; CU3 = continuation_next_check_progression ; CU4 = continuation_observation_resolution_context_recovery

## Runtime Gold Metrics

These metrics are computed from runtime trace spans and compare structured query / retrieval outputs against golden labels.

### Query Structuring Core Metrics

| metric | value | meaning |
|---|---:|---|
| runtime_query_structuring_macro_precision_soft | 0.6875 | How many selected vocabulary terms are acceptable under soft relevance |
| runtime_query_structuring_macro_recall_strict | 0.6964 | Whether strictly expected terms were recovered |
| runtime_query_structuring_macro_recall_soft | 0.5357 | Coverage of broader acceptable terms |
| runtime_query_structuring_grounded_strict_recall | 0.5357 | Whether strict terms are selected with valid grounding |
| runtime_query_structuring_core_success_rate | 0.6964 | Whether all vocab fields passed their core gold-backed checks |

#### Query Structuring Field Core Metrics

| field | precision_soft | recall_strict | recall_soft | grounded_strict_recall | field_core_success | field_grounded_success |
|---|---:|---:|---:|---:|---:|---:|
| symptoms | 0.6786 | 0.5714 | 0.3929 | 0.5714 | 0.5714 | 0.5714 |
| affected_subsystems | 0.5000 | 0.5714 | 0.5000 | 0.5714 | 0.5714 | 0.5714 |
| failure_modes | 1.0000 | 1.0000 | 0.6071 | 0.5714 | 1.0000 | 0.5714 |
| system_properties | 0.5714 | 0.6429 | 0.6429 | 0.4286 | 0.6429 | 0.4286 |

### Retrieval Core Metrics

> Each value is averaged over runs where the target was evaluated.

| retrieval_target | evaluated_k | recall_strict | recall_soft | rr_strict | rr_soft | nDCG | frr_strict | frr_soft | n_strict | n_soft |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| candidate_cards | 8.0 | 1.0000 | 0.9048 | 1.0000 | 1.0000 | 0.9544 | 1.00 | 1.00 | 1.00 | 2.71 |
| incident_primary | 12.0 | 1.0000 | 0.9762 | 0.8571 | 0.9286 | 0.9116 | 1.29 | 1.14 | 1.43 | 3.36 |
| incident_alternatives | 12.0 | 0.7857 | 0.6905 | 0.6071 | 0.7738 | 0.6678 | 1.82 | 1.25 | 0.79 | 2.07 |
| theory_evidence | 12.0 | 1.0000 | 0.8571 | 0.6799 | 0.7548 | 0.7451 | 2.64 | 1.93 | 1.00 | 1.71 |

### Retrieval Summary

| metric | value | formula | meaning |
|---|---:|---|---|
| runtime_retrieval_mean_ndcg | 0.8197 | avg_run(avg_target(ndcg)) | Average ranking quality across retrieval targets and runs |
| runtime_retrieval_all_strict_recall_success_rate | 0.9464 | avg_run(frac_target(recall_strict=1)) | Average per-run share of retrieval targets with strict recall success |
| runtime_retrieval_all_soft_recall_success_rate | 0.9643 | avg_run(frac_target(recall_soft>0)) | Average per-run share of retrieval targets with any soft recall |
| runtime_retrieval_penalized_first_relevant_rank_strict | 2.29 | avg_run(avg_target(frr_strict or k+1)) | Penalized rank; missing strict hit treated as k+1 |
| runtime_retrieval_zero_hit_rate | 0.0000 | avg_run(frac_target(hits_count=0)) | Average per-run share of retrieval calls with no hits |

## Suite Distributions

| suite | score_0 | score_1 | score_2 |
|---|---:|---:|---:|
| final_no_root_cause_claim | 4 | 0 | 10 |
| final_first_check_discriminates | 0 | 0 | 14 |
| final_alternative_context_handling | 1 | 0 | 13 |
| final_result_interpretation_usefulness | 0 | 0 | 14 |
| final_hypothesis_source_alignment | 2 | 2 | 10 |
| query_structuring_field_boundary_correctness | 10 | 2 | 2 |
| query_structuring_grounding_conservatism | 11 | 3 | 0 |
| evidence_pack_role_fit | 10 | 4 | 0 |
| evidence_pack_sufficiency | 10 | 0 | 4 |
| continuation_hypothesis_update_discipline | 2 | 2 | 6 |
| continuation_problem_understanding_update | 2 | 0 | 8 |
| continuation_next_check_progression | 3 | 0 | 7 |
| continuation_observation_resolution_context_recovery | 0 | 0 | 10 |

## Gate Breakdown

| gate | fail_count | fail_rate |
|---|---:|---:|
| final_no_root_cause_claim | 4 | 0.2857 |
| final_first_check_discriminates | 0 | 0.0000 |
| final_hypothesis_source_alignment | 2 | 0.1429 |
| query_structuring_field_boundary_correctness | 0 | 0.0000 |
| evidence_pack_sufficiency | 0 | 0.0000 |
| continuation_hypothesis_update_discipline | 2 | 0.2000 |
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
| bad_final_with_good_query_and_evidence_rate | 0.2857 | frac(!usable ∧ QS1>0 ∧ QS2>0 ∧ EP1>0 ∧ EP2>0) |

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

## Where Quality Was Lost

### Pipeline Stage Summary

| stage | signals | status | interpretation |
|---|---|---|---|
| query structuring | judge 0.32, no-hard-fail 93%, runtime core 70% | mixed | no strict pass on any run; runtime core success 70% |
| retrieval | strict recall 95%, nDCG 0.82 | strong | recall was present, but ranking quality remained weak (nDCG 0.82) |
| evidence packing | judge 0.43, no-hard-fail 100% | mixed | 10 run(s) with insufficient evidence pack (EP2=0) |
| final answer | usable 71%, judge 1.77, no-hard-fail 71% | strong | 4 premature certainty (FA1=0); 4 partial source alignment (FA3<2) |
| continuation | usable cont 40%, update score 1.47, input score 2.00, no-hard-fail 50% | weak | 2 hard fail(s) on hypothesis update discipline (CU1=0) |

### Failure Path

4 of 14 responses were unusable.

- 4 unusable despite good query + evidence → **final answer stage failure**
  - 4 × FA1=0: premature certainty or root cause claim

Continuation was the main observed degradation point:

- 2 hypothesis update discipline hard fail(s) (CU1=0)
- 2 problem understanding update hard fail(s) (CU2=0)
- 3 next check progression hard fail(s) (CU3=0)

Quality degraded between initial and continuation iterations.

Main observed weakness: **query structuring** (composite 0.60). retrieval and final answer quality was strong.

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
| `db80b5d4-da8d-48c3-8566-a542c48ffa41` | `09b48f1e-b081-4a0f-86c6-8fd57d702935` | 1.2000 | false |
| `d62e4d7f-37ff-4def-8d43-df865219c589` | `38426ad8-ca56-4c10-bc37-d6db93274bbf` | 1.6000 | false |
| `d62e4d7f-37ff-4def-8d43-df865219c589` | `5ee95c73-1be9-4807-ad66-8d5e594eed10` | 1.6000 | false |
| `d62e4d7f-37ff-4def-8d43-df865219c589` | `641048cb-8ec9-49f6-97cc-533fb14d22a9` | 1.6000 | false |
| `db80b5d4-da8d-48c3-8566-a542c48ffa41` | `a9d35fe2-f04d-4777-a188-4725807d87e2` | 1.6000 | true |

## Token Usage

### Judge Calls by Suite

| suite | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| continuation_hypothesis_update_discipline | 15488 | 6637 | 22125 | 0.002102 |
| continuation_next_check_progression | 10765 | 5694 | 16459 | 0.001677 |
| continuation_observation_resolution_context_recovery | 9409 | 6861 | 16270 | 0.001843 |
| continuation_problem_understanding_update | 13180 | 7867 | 21047 | 0.002232 |
| evidence_pack_role_fit | 15424 | 4927 | 20351 | 0.001757 |
| evidence_pack_sufficiency | 21881 | 4367 | 26248 | 0.001967 |
| final_alternative_context_handling | 75808 | 5108 | 80916 | 0.004812 |
| final_first_check_discriminates | 76723 | 5389 | 82112 | 0.004914 |
| final_hypothesis_source_alignment | 65535 | 8738 | 74273 | 0.005024 |
| final_no_root_cause_claim | 76033 | 5680 | 81713 | 0.004938 |
| final_result_interpretation_usefulness | 12340 | 9215 | 21555 | 0.002460 |
| query_structuring_field_boundary_correctness | 3659 | 2992 | 6651 | 0.000781 |
| query_structuring_grounding_conservatism | 3509 | 5529 | 9038 | 0.001281 |
| judge_total | 399754 | 79004 | 478758 | 0.035788 |

| model | prompt_tokens_cost | completion_tokens_cost | total_cost_usd |
|---|---|---|---:|
| openai/gpt-oss-20b | 399754 * $0.05/1M = 0.019988 | 79004 * $0.2/1M = 0.015801 | 0.035788 |

### Runtime by Stage

| scope | model | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---|---:|---:|---:|---:|
| query_structuring | openai/gpt-oss-120b | 0 | 0 | 10043 | 0.003535 |
| observation_boundary_resolver | openai/gpt-oss-20b | 0 | 0 | 10058 | 0.000945 |
| observation_extraction | openai/gpt-oss-120b | 0 | 0 | 9636 | 0.003897 |
| llm_structured_generation | openai/gpt-oss-20b | 0 | 0 | 77908 | 0.007431 |
| runtime_total | — | 0 | 0 | 107645 | 0.015807 |

| model | prompt_tokens_cost | completion_tokens_cost | total_cost_usd |
|---|---|---|---:|
| openai/gpt-oss-120b | 0 * $0.0/1M = 0.000000 | 0 * $0.0/1M = 0.000000 | 0.007431 |
| openai/gpt-oss-20b | 0 * $0.0/1M = 0.000000 | 0 * $0.0/1M = 0.000000 | 0.008375 |
| runtime_total | sum(stage prompt costs) = 0.000000 | sum(stage completion costs) = 0.000000 | 0.015807 |

### Totals

| scope | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| runtime | 71177 | 36468 | 107645 | 0.015807 |
| judge_total | 371952 | 73698 | 445650 | 0.033337 |
| run_total | 443129 | 110166 | 553295 | 0.049144 |

Run total cost usd = runtime total cost usd + judge total cost usd = 0.015807 + 0.033337 = 0.049144

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
| symptoms | 1.21 | 0.43 | 0.43 | 0.43 |
| affected_subsystems | 2.29 | 1.29 | 0.43 | 1.29 |
| failure_modes | 1.00 | 0.00 | 0.00 | 0.00 |
| system_properties | 1.64 | 0.57 | 0.36 | 0.57 |

### A.3 Graded Relevance Diagnostics

| field | graded_coverage | average_selected_score |
|---|---:|---:|
| symptoms | 0.4524 | 0.6250 |
| affected_subsystems | 0.5238 | 0.4107 |
| failure_modes | 0.7381 | 1.0000 |
| system_properties | 0.6429 | 0.4821 |

### A.4 Grounding Diagnostics

| field | unsupported_selected_term_rate | missing_evidence_span_count | invalid_evidence_span_count | evidence_span_near_substring_rate |
|---|---:|---:|---:|---:|
| symptoms | 0.0000 | 0.00 | 0.00 | 1.0000 |
| affected_subsystems | 0.0000 | 0.00 | 0.00 | 1.0000 |
| failure_modes | 0.4286 | 0.00 | 0.21 | 0.7857 |
| system_properties | 0.2143 | 0.00 | 0.64 | 0.7857 |

### A.5 Support-Level Diagnostics

| field | weak_inference_rate | strict_terms_weak_inference_rate | weak_false_positive_rate |
|---|---:|---:|---:|
| symptoms | 0.0000 | 0.0000 | 0.0000 |
| affected_subsystems | 0.0000 | 0.0000 | 0.0000 |
| failure_modes | 0.2143 | 0.2143 | 0.0000 |
| system_properties | 0.0000 | 0.0000 | 0.0000 |

### A.6 Field Success Diagnostics

| field | field_core_success | field_grounded_success | empty_when_gold_exists |
|---|---:|---:|---:|
| symptoms | 0.5714 | 0.5714 | 0.0000 |
| affected_subsystems | 0.5714 | 0.5714 | 0.0000 |
| failure_modes | 1.0000 | 0.5714 | 0.0000 |
| system_properties | 0.6429 | 0.4286 | 0.0000 |

### A.7 Query-Level Non-Vocabulary Diagnostics

| entities_count_avg | constraints_count_avg | triggers_count_avg | observability_signals_count_avg | unresolved_terms_count_avg | intent_present_rate | scenario_present_rate |
|---:|---:|---:|---:|---:|---:|---:|
| 2.07 | 0.00 | 0.21 | 0.21 | 0.00 | 1.0000 | 1.0000 |

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
| candidate_cards | 7.5 | 3.0 | 0.9524 | 0.2356 |
| incident_primary | 8.2 | 8.2 | 0.7833 | 0.2470 |
| incident_alternatives | 8.4 | 8.4 | 0.5466 | 0.2000 |
| theory_evidence | 7.8 | 7.8 | 0.6095 | 0.2000 |

## Appendix C: Judge Metrics Per Run

### Run `185d0128-218d-41fc-adc6-2c1e40aef04a`

| metric | iter_1 | iter_2 | total | formula |
|--- |---: |---: |---:|---|
| usable_first_response_rate | 1 | 1 | 1.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | n/a | n/a | — | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | n/a | n/a | — | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 1.8000 | 1.8000 | 1.8000 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | n/a | n/a | — | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | n/a | n/a | — | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 1 | 1.0000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 0 | 0.0000 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | n/a | n/a | — | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | n/a | n/a | — | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0 | 0 | 0.0000 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | 0 | 2 | 1.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | 0 | 2 | 1.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | 2 | 2 | 2.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | 0 | 1 | 0.5000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | 0.6667 | 2.0000 | 1.3333 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | 0 | 1 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | 0 | 1 | 0.5000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | 1 | 1 | 1.0000 | frac(CU4=2) |

### Run `29942251-4612-429f-a1cc-b637f82733a4`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 1 | 1 | 1.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 1.0000 | n/a | n/a | 1.0000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 2.0000 | 2.0000 | 2.0000 | 2.0000 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0 | n/a | n/a | 0.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 1 | 1 | 1.0000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 0 | 0 | 0.0000 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 1 | 1 | 1 | 1.0000 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 2 | 2 | 2.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 0 | 1.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2 | 0 | 1.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 1 | 0 | 0.5000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 2.0000 | 0.6667 | 1.3333 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 1 | 0 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 1 | 0 | 0.5000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

### Run `d62e4d7f-37ff-4def-8d43-df865219c589`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 0 | 0 | 0 | 0.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 1.6000 | 1.6000 | 1.6000 | 1.6000 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 0 | 0 | 0 | 0.0000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 1 | 1 | 1 | 1.0000 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0 | 0 | 0 | 0.0000 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 2 | 2 | 2.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 2 | 2.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2 | 0 | 1.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 0 | 0 | 0.0000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 2.0000 | 1.3333 | 1.6667 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 1 | 0 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 1 | 0 | 0.5000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

### Run `db80b5d4-da8d-48c3-8566-a542c48ffa41`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 0 | 1 | 0.6667 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 1.0000 | n/a | n/a | 1.0000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 2.0000 | 1.2000 | 1.6000 | 1.6000 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 0 | 1 | 0.6667 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 1 | 0 | 0.3333 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 1 | 0 | 0 | 0.3333 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 1 | 1 | 1.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 2 | 2.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 0 | 2 | 1.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 0 | 1 | 0.5000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 1.0000 | 1.6667 | 1.3333 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 0 | 1 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0 | 0 | 0.0000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

### Run `dcdeb125-a417-4725-80b3-bf3221630c56`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 1 | 1 | 1.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 1.0000 | n/a | n/a | 1.0000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 2.0000 | 1.6000 | 2.0000 | 1.8667 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 1 | 1 | 1.0000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 0 | 0 | 0.0000 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 1 | 0 | 1 | 0.6667 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 2 | 0 | 1.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 2 | 2.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2 | 2 | 2.0000 | mean(CU3) over continuation iter-s |
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

