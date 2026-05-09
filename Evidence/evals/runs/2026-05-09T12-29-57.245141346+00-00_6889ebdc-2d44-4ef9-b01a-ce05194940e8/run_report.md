# Eval Run Report

## Run Metadata

- eval_run_id: `6889ebdc-2d44-4ef9-b01a-ce05194940e8`
- run_type: `golden_dataset`
- status: `completed`
- started_at: `2026-05-09 12:29:57.245141346 UTC`
- completed_at: `2026-05-09 12:37:01.558939362 UTC`
- runtime_run_count: `5`
- iterations_evaluated_count: `15`
- judge_model: `openai/gpt-oss-20b`
- suite_count: `13`

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
| usable_first_response_rate | 0.8000 | Share of runs where the final answer can be shown as a first diagnostic response |
| gate_pass_rate | 0.6000 | Share of runs without critical gate failures |
| query_structuring_judge_score | 0.2667 | Judge-based semantic quality of query structuring |
| runtime_query_structuring_core_success_rate | 0.7000 | Gold-backed runtime success of structured query fields |
| runtime_retrieval_mean_ndcg | 0.7417 | Average ranking quality across retrieval targets and runs |
| runtime_retrieval_all_strict_recall_success_rate | 0.9000 | Average per-run share of retrieval targets where strict expected evidence was found |
| evidence_pack_judge_score | 0.5000 | Judge-based quality of selected evidence pack |
| final_answer_judge_score | 1.6533 | Judge-based quality of final diagnostic response |
| usable_continuation_response_rate | 0.5000 | Share of continuation iterations with usable update behavior |
| continuation_update_judge_score | 1.4333 | Judge-based quality of updating the diagnostic frame |
| continuation_input_judge_score | 2.0000 | Judge-based quality of reconstructing the new observation from context |
| continuation_update_strict_pass_rate | 0.3000 | Share of continuation iterations where CU1, CU2, CU3 all scored 2 |

## Judge-Based Aggregated Metrics

> initial iter-s = all initial iterations across runs; continuation iter-s = all continuation iterations across runs; total ignores n/a

| metric | initial iter-s | continuation iter-s | total | formula |
|---|---:|---:|---:|---|
| usable_first_response_rate | 0.6000 | n/a | 0.6000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 0.8000 | n/a | 0.8000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 1.6000 | 1.6800 | 1.6533 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0.4000 | n/a | 0.4000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1.0000 | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 0.6000 | 0.8000 | 0.7333 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0.4000 | 0.2000 | 0.2667 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0.0000 | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0.0000 | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0.2000 | 0.1000 | 0.1333 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 1.6000 | 1.6000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 1.3000 | 1.3000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 1.4000 | 1.4000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2.0000 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 0.5000 | 0.5000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 1.4333 | 1.4333 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 0.6000 | 0.6000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0.3000 | 0.3000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
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
| runtime_query_structuring_macro_precision_soft | 0.7333 | How many selected vocabulary terms are acceptable under soft relevance |
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
| system_properties | 0.8000 | 0.6000 | 0.6000 | 0.6000 | 0.6000 | 0.6000 |

### Retrieval Core Metrics

> Each value is averaged over runs where the target was evaluated.

| retrieval_target | evaluated_k | recall_strict | recall_soft | rr_strict | rr_soft | nDCG | frr_strict | frr_soft | n_strict | n_soft |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| candidate_cards | 8.0 | 1.0000 | 0.8444 | 1.0000 | 1.0000 | 0.9199 | 1.00 | 1.00 | 1.00 | 2.53 |
| incident_primary | 12.0 | 0.8667 | 0.8444 | 0.7689 | 0.9133 | 0.8053 | 1.46 | 1.33 | 1.27 | 2.93 |
| incident_alternatives | 12.0 | 0.7333 | 0.6222 | 0.5722 | 0.6278 | 0.5942 | 1.64 | 1.83 | 0.73 | 1.87 |
| theory_evidence | 12.0 | 1.0000 | 0.8333 | 0.5056 | 0.6222 | 0.6473 | 3.07 | 2.73 | 1.00 | 1.67 |

### Retrieval Summary

| metric | value | formula | meaning |
|---|---:|---|---|
| runtime_retrieval_mean_ndcg | 0.7417 | avg_run(avg_target(ndcg)) | Average ranking quality across retrieval targets and runs |
| runtime_retrieval_all_strict_recall_success_rate | 0.9000 | avg_run(frac_target(recall_strict=1)) | Average per-run share of retrieval targets with strict recall success |
| runtime_retrieval_all_soft_recall_success_rate | 0.9500 | avg_run(frac_target(recall_soft>0)) | Average per-run share of retrieval targets with any soft recall |
| runtime_retrieval_penalized_first_relevant_rank_strict | 2.93 | avg_run(avg_target(frr_strict or k+1)) | Penalized rank; missing strict hit treated as k+1 |
| runtime_retrieval_zero_hit_rate | 0.0000 | avg_run(frac_target(hits_count=0)) | Average per-run share of retrieval calls with no hits |

## Suite Distributions

| suite | score_0 | score_1 | score_2 |
|---|---:|---:|---:|
| final_no_root_cause_claim | 2 | 4 | 9 |
| final_first_check_discriminates | 1 | 0 | 14 |
| final_alternative_context_handling | 2 | 0 | 13 |
| final_result_interpretation_usefulness | 0 | 0 | 15 |
| final_hypothesis_source_alignment | 2 | 8 | 5 |
| query_structuring_field_boundary_correctness | 13 | 1 | 1 |
| query_structuring_grounding_conservatism | 10 | 5 | 0 |
| evidence_pack_role_fit | 10 | 5 | 0 |
| evidence_pack_sufficiency | 10 | 0 | 5 |
| continuation_hypothesis_update_discipline | 1 | 2 | 7 |
| continuation_problem_understanding_update | 2 | 3 | 5 |
| continuation_next_check_progression | 3 | 0 | 7 |
| continuation_observation_resolution_context_recovery | 0 | 0 | 10 |

## Gate Breakdown

| gate | fail_count | fail_rate |
|---|---:|---:|
| final_no_root_cause_claim | 2 | 0.1333 |
| final_first_check_discriminates | 1 | 0.0667 |
| final_hypothesis_source_alignment | 2 | 0.1333 |
| query_structuring_field_boundary_correctness | 3 | 0.2000 |
| evidence_pack_sufficiency | 0 | 0.0000 |
| continuation_hypothesis_update_discipline | 1 | 0.1000 |
| continuation_problem_understanding_update | 2 | 0.2000 |
| continuation_next_check_progression | 3 | 0.3000 |
| continuation_observation_resolution_context_recovery | 0 | 0.0000 |

> Gate fails when suite score = 0. Pass threshold: score ≥ 1.
> Note: `Gate Breakdown` reflects critical standalone gates and may differ from composite no-hard-fail formulas (e.g., `final_hypothesis_source_alignment` is gated individually but excluded from `final_answer_no_hard_fail_rate`).

## Failure Attribution

### Initial / First-Response Attribution

| metric | value | formula |
|---|---:|---|
| bad_final_due_to_query_rate | 0.0667 | frac(!usable ∧ (QS1=0 ∨ QS2=0)) |
| bad_final_due_to_evidence_rate | 0.0000 | frac(!usable ∧ (EP1=0 ∨ EP2=0)) |
| bad_final_with_good_query_and_evidence_rate | 0.1333 | frac(!usable ∧ QS1>0 ∧ QS2>0 ∧ EP1>0 ∧ EP2>0) |

> usable = FA1≥1 ∧ FA2≥1 ∧ FA5≥1

### Continuation Attribution

> `frac(condition)` uses only continuation iterations where all fields referenced by that formula are present; missing values are excluded from both numerator and denominator.

| metric | value | formula |
|---|---:|---|
| bad_continuation_due_to_input_resolution_rate | 0.0000 | frac(!usable_continuation ∧ CU4=0) |
| bad_continuation_due_to_update_logic_rate | 0.4000 | frac(!usable_continuation ∧ CU4>0 ∧ (CU1=0 ∨ CU2=0 ∨ CU3=0)) |
| bad_continuation_despite_good_input_rate | 0.5000 | frac(!usable_continuation ∧ CU4=2) |
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
| query structuring | judge 0.27, no-hard-fail 80%, runtime core 70% | mixed | no strict pass on any run; 3 field boundary gate fail(s); runtime core success 70% |
| retrieval | strict recall 90%, nDCG 0.74 | strong | recall was present, but ranking quality remained weak (nDCG 0.74) |
| evidence packing | judge 0.50, no-hard-fail 100% | mixed | 10 run(s) with insufficient evidence pack (EP2=0) |
| final answer | usable 80%, judge 1.65, no-hard-fail 73% | strong | 2 premature certainty (FA1=0); 1 vague first check (FA2=0); 10 partial source alignment (FA3<2) |
| continuation | usable cont 50%, update score 1.43, input score 2.00, no-hard-fail 60% | mixed | partial degradation: usable rate 50%, no-hard-fail 60% |

### Failure Path

3 of 15 responses were unusable.

- 1 unusable → **query structuring hard fail** (QS1=0 or QS2=0)
- 2 unusable despite good query + evidence → **final answer stage failure**
  - 2 × FA1=0: premature certainty or root cause claim
  - 1 × FA2=0: vague or non-discriminating first check

Continuation was the main observed degradation point:

- 1 hypothesis update discipline hard fail(s) (CU1=0)
- 2 problem understanding update hard fail(s) (CU2=0)
- 3 next check progression hard fail(s) (CU3=0)

Quality degraded between initial and continuation iterations.

Main observed weakness: **query structuring** (composite 0.54). retrieval and final answer quality was strong.

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
| `26f3a8c4-f3fe-4914-8523-ac8b237306be` | `7ce72cc6-305c-4ec2-adca-e8498c4482ee` | 0.8000 | false |
| `b75c532a-6018-4446-b594-5cf7dd5f8ed7` | `bc1033d2-021a-4d4e-9244-4f71a3b1d8a3` | 1.4000 | false |
| `26f3a8c4-f3fe-4914-8523-ac8b237306be` | `1ac9b959-6d13-4068-858c-0092f8ce5ad0` | 1.6000 | true |
| `26f3a8c4-f3fe-4914-8523-ac8b237306be` | `5b098e03-17de-4870-8a96-506a361909f1` | 1.6000 | true |
| `4310da2c-d094-4acb-a985-eb650b5ee00e` | `40747451-541a-44e7-ba8b-5cbc1938f77c` | 1.6000 | true |

## Token Usage

### Judge Calls by Suite

| suite | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| continuation_hypothesis_update_discipline | 16386 | 7166 | 23552 | 0.002253 |
| continuation_next_check_progression | 11317 | 5717 | 17034 | 0.001709 |
| continuation_observation_resolution_context_recovery | 9932 | 4205 | 14137 | 0.001338 |
| continuation_problem_understanding_update | 13895 | 6369 | 20264 | 0.001969 |
| evidence_pack_role_fit | 15389 | 5194 | 20583 | 0.001808 |
| evidence_pack_sufficiency | 21846 | 5331 | 27177 | 0.002158 |
| final_alternative_context_handling | 76667 | 5300 | 81967 | 0.004893 |
| final_first_check_discriminates | 77582 | 6551 | 84133 | 0.005189 |
| final_hypothesis_source_alignment | 65819 | 10536 | 76355 | 0.005398 |
| final_no_root_cause_claim | 76892 | 6805 | 83697 | 0.005206 |
| final_result_interpretation_usefulness | 13183 | 9597 | 22780 | 0.002579 |
| query_structuring_field_boundary_correctness | 3697 | 2736 | 6433 | 0.000732 |
| query_structuring_grounding_conservatism | 3547 | 4319 | 7866 | 0.001041 |

### Totals

| scope | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| runtime | 64558 | 29788 | 94346 | 0.004028 |
| judge_total | 406152 | 79826 | 485978 | 0.036273 |
| run_total | 470710 | 109614 | 580324 | 0.040301 |

Run total cost usd = runtime total cost usd + judge total cost usd = 0.004028 + 0.036273 = 0.040301

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
| affected_subsystems | 2.20 | 1.00 | 0.20 | 1.00 |
| failure_modes | 1.00 | 0.20 | 0.20 | 0.20 |
| system_properties | 1.20 | 0.20 | 0.40 | 0.20 |

### A.3 Graded Relevance Diagnostics

| field | graded_coverage | average_selected_score |
|---|---:|---:|
| symptoms | 0.4667 | 0.6500 |
| affected_subsystems | 0.7333 | 0.5500 |
| failure_modes | 0.6000 | 0.8000 |
| system_properties | 0.6000 | 0.6500 |

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
| 2.00 | 0.40 | 0.60 | 1.00 | 0.00 | 1.0000 | 1.0000 |

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
| candidate_cards | 7.3 | 3.0 | 0.9300 | 0.2425 |
| incident_primary | 6.7 | 6.7 | 0.7500 | 0.2380 |
| incident_alternatives | 8.1 | 8.1 | 0.5624 | 0.2033 |
| theory_evidence | 7.9 | 7.9 | 0.5744 | 0.2000 |

## Appendix C: Judge Metrics Per Run

### Run `26f3a8c4-f3fe-4914-8523-ac8b237306be`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 0 | 1 | 1 | 0.6667 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 0.8000 | 1.6000 | 1.6000 | 1.3333 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 0 | 1 | 0 | 0.3333 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 1 | 0 | 1 | 0.6667 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0 | 0 | 0 | 0.0000 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 2 | 0 | 1.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 1 | 0 | 0.5000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2 | 2 | 2.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 1 | 0 | 0.5000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 1.6667 | 0.6667 | 1.1667 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 1 | 0 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0 | 0 | 0.0000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

### Run `4310da2c-d094-4acb-a985-eb650b5ee00e`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 1 | 1 | 1.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 0.5000 | n/a | n/a | 0.5000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 2.0000 | 1.6000 | 1.6000 | 1.7333 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0 | n/a | n/a | 0.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 1 | 1 | 1.0000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 0 | 0 | 0.0000 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 1 | 0 | 0 | 0.3333 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
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

### Run `54dea932-1e52-4746-98f9-6cb2060b6916`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 1 | 1 | 1.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 0.5000 | n/a | n/a | 0.5000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 1.8000 | 1.6000 | 1.8000 | 1.7333 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0 | n/a | n/a | 0.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 1 | 1 | 1.0000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 0 | 0 | 0.0000 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0 | 0 | 0 | 0.0000 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 1 | 2 | 1.5000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 2 | 2.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2 | 2 | 2.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 1 | 1 | 1.0000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 1.6667 | 2.0000 | 1.8333 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0 | 1 | 0.5000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

### Run `8482a873-a19f-4fdc-8e19-bfd58cb97e3b`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 1 | 1 | 1 | 1.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 1.0000 | n/a | n/a | 1.0000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 1.8000 | 1.8000 | 1.8000 | 1.8000 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1 | 1 | 1 | 1.0000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0 | 0 | 0 | 0.0000 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0 | 0 | 0 | 0.0000 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 2 | 1 | 1.5000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2 | 2 | 2.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2 | 0 | 1.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 1 | 0 | 0.5000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 2.0000 | 1.0000 | 1.5000 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 1 | 0 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 1 | 0 | 0.5000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

### Run `b75c532a-6018-4446-b594-5cf7dd5f8ed7`

| metric | iter_1 | iter_2 | iter_3 | total | formula |
|--- |---: |---: |---: |---:|---|
| usable_first_response_rate | 0 | 1 | 0 | 0.3333 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 0.5000 | n/a | n/a | 0.5000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 1.6000 | 2.0000 | 1.4000 | 1.6667 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0 | n/a | n/a | 0.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1 | n/a | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 0 | 1 | 0 | 0.3333 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 1 | 0 | 1 | 0.6667 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0 | n/a | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0 | 1 | 0 | 0.3333 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 2 | 2 | 2.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 1 | 1 | 1.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 0 | 2 | 1.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 0 | 0 | 0.0000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 1.0000 | 1.6667 | 1.3333 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 0 | 1 | 0.5000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 0 | 0 | 0.0000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2 | 2 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1 | 1 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1 | 1 | 1.0000 | frac(CU4=2) |

> QS1 = query_structuring_field_boundary_correctness ; QS2 = query_structuring_grounding_conservatism
> EP1 = evidence_pack_role_fit ; EP2 = evidence_pack_sufficiency
> FA1 = final_no_root_cause_claim ; FA2 = final_first_check_discriminates ; FA3 = final_hypothesis_source_alignment ; FA4 = final_alternative_context_handling ; FA5 = final_result_interpretation_usefulness
> CU1 = continuation_hypothesis_update_discipline ; CU2 = continuation_problem_understanding_update ; CU3 = continuation_next_check_progression ; CU4 = continuation_observation_resolution_context_recovery

