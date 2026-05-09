# Eval Run Report

## Run Metadata

- eval_run_id: `47d59658-7b1d-4a6f-81f9-87ded488d4e3`
- run_type: `golden_dataset`
- status: `completed`
- started_at: `2026-05-09 09:13:42.820521443 UTC`
- completed_at: `2026-05-09 09:14:51.869391053 UTC`
- runtime_run_count: `1`
- iterations_evaluated_count: `2`
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
| usable_first_response_rate | 1.0000 | Share of runs where the final answer can be shown as a first diagnostic response |
| gate_pass_rate | 0.0000 | Share of runs without critical gate failures |
| query_structuring_judge_score | 0.2500 | Judge-based semantic quality of query structuring |
| runtime_query_structuring_core_success_rate | 0.0000 | Gold-backed runtime success of structured query fields |
| runtime_retrieval_mean_ndcg | 0.0000 | Average ranking quality across retrieval targets and runs |
| runtime_retrieval_all_strict_recall_success_rate | 0.0000 | Average per-run share of retrieval targets where strict expected evidence was found |
| evidence_pack_judge_score | 0.7500 | Judge-based quality of selected evidence pack |
| final_answer_judge_score | 1.8000 | Judge-based quality of final diagnostic response |

## Judge-Based Aggregated Metrics

> initial iter-s = all initial iterations across runs; continuation iter-s = all continuation iterations across runs; total ignores n/a

| metric | initial iter-s | continuation iter-s | total | formula |
|---|---:|---:|---:|---|
| usable_first_response_rate | 1.0000 | 1.0000 | 1.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 0.5000 | n/a | 0.5000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 2.0000 | 1.6000 | 1.8000 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0.0000 | n/a | 0.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1.0000 | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1.0000 | 1.0000 | 1.0000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0.0000 | 0.0000 | 0.0000 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0.0000 | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0.0000 | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 1.0000 | 0.0000 | 0.5000 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 2.0000 | 2.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2.0000 | 2.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2.0000 | 2.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2.0000 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 1.0000 | 1.0000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 2.0000 | 2.0000 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 1.0000 | 1.0000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 1.0000 | 1.0000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
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
| runtime_query_structuring_macro_precision_soft | 0.0000 | How many selected vocabulary terms are acceptable under soft relevance |
| runtime_query_structuring_macro_recall_strict | 0.0000 | Whether strictly expected terms were recovered |
| runtime_query_structuring_macro_recall_soft | 0.0000 | Coverage of broader acceptable terms |
| runtime_query_structuring_grounded_strict_recall | 0.0000 | Whether strict terms are selected with valid grounding |
| runtime_query_structuring_core_success_rate | 0.0000 | Whether all vocab fields passed their core gold-backed checks |

#### Query Structuring Field Core Metrics

| field | precision_soft | recall_strict | recall_soft | grounded_strict_recall | field_core_success | field_grounded_success |
|---|---:|---:|---:|---:|---:|---:|
| symptoms | — | — | — | — | — | — |
| affected_subsystems | — | — | — | — | — | — |
| failure_modes | — | — | — | — | — | — |
| system_properties | — | — | — | — | — | — |

### Retrieval Core Metrics

> Each value is averaged over runs where the target was evaluated.

| retrieval_target | evaluated_k | recall_strict | recall_soft | rr_strict | rr_soft | nDCG | frr_strict | frr_soft | n_strict | n_soft |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| candidate_cards | — | — | — | — | — | — | — | — | — | — |
| incident_primary | — | — | — | — | — | — | — | — | — | — |
| incident_alternatives | — | — | — | — | — | — | — | — | — | — |
| theory_evidence | — | — | — | — | — | — | — | — | — | — |

### Retrieval Summary

| metric | value | formula | meaning |
|---|---:|---|---|
| runtime_retrieval_mean_ndcg | 0.0000 | avg_run(avg_target(ndcg)) | Average ranking quality across retrieval targets and runs |
| runtime_retrieval_all_strict_recall_success_rate | 0.0000 | avg_run(frac_target(recall_strict=1)) | Average per-run share of retrieval targets with strict recall success |
| runtime_retrieval_all_soft_recall_success_rate | 0.0000 | avg_run(frac_target(recall_soft>0)) | Average per-run share of retrieval targets with any soft recall |
| runtime_retrieval_penalized_first_relevant_rank_strict | — | avg_run(avg_target(frr_strict or k+1)) | Penalized rank; missing strict hit treated as k+1 |
| runtime_retrieval_zero_hit_rate | 0.0000 | avg_run(frac_target(hits_count=0)) | Average per-run share of retrieval calls with no hits |

## Suite Distributions

| suite | score_0 | score_1 | score_2 |
|---|---:|---:|---:|
| final_no_root_cause_claim | 0 | 0 | 2 |
| final_first_check_discriminates | 0 | 0 | 2 |
| final_alternative_context_handling | 0 | 0 | 2 |
| final_result_interpretation_usefulness | 0 | 0 | 2 |
| final_hypothesis_source_alignment | 1 | 0 | 1 |
| query_structuring_field_boundary_correctness | 2 | 0 | 0 |
| query_structuring_grounding_conservatism | 1 | 1 | 0 |
| evidence_pack_role_fit | 1 | 1 | 0 |
| evidence_pack_sufficiency | 1 | 0 | 1 |

## Gate Breakdown

| gate | fail_count | fail_rate |
|---|---:|---:|
| final_no_root_cause_claim | 0 | 0.0000 |
| final_first_check_discriminates | 0 | 0.0000 |
| final_hypothesis_source_alignment | 1 | 0.5000 |
| query_structuring_field_boundary_correctness | 1 | 0.5000 |
| evidence_pack_sufficiency | 0 | 0.0000 |

> Gate fails when suite score = 0. Pass threshold: score ≥ 1.

## Failure Attribution

| metric | value | formula |
|---|---:|---|
| bad_final_due_to_query_rate | 0.0000 | frac(!usable ∧ (QS1=0 ∨ QS2=0)) |
| bad_final_due_to_evidence_rate | 0.0000 | frac(!usable ∧ (EP1=0 ∨ EP2=0)) |
| bad_final_with_good_query_and_evidence_rate | 0.0000 | frac(!usable ∧ QS1>0 ∧ QS2>0 ∧ EP1>0 ∧ EP2>0) |

> usable = FA1≥1 ∧ FA2≥1 ∧ FA5≥1

> QS1 = query_structuring_field_boundary_correctness ; QS2 = query_structuring_grounding_conservatism
> EP1 = evidence_pack_role_fit ; EP2 = evidence_pack_sufficiency
> FA1 = final_no_root_cause_claim ; FA2 = final_first_check_discriminates ; FA3 = final_hypothesis_source_alignment ; FA4 = final_alternative_context_handling ; FA5 = final_result_interpretation_usefulness
> CU1 = continuation_hypothesis_update_discipline ; CU2 = continuation_problem_understanding_update ; CU3 = continuation_next_check_progression ; CU4 = continuation_observation_resolution_context_recovery

## Where Quality Was Lost

### Pipeline Stage Summary

| stage | signals | status | interpretation |
|---|---|---|---|
| query structuring | judge 0.25, no-hard-fail 50%, runtime core 0% | weak | no strict pass on any run; 1 field boundary gate fail(s); runtime core success 0% |
| retrieval | strict recall 0%, nDCG 0.00 | weak | recall strong but ranking quality below threshold (nDCG 0.00) |
| evidence packing | judge 0.75, no-hard-fail 100% | mixed | 1 run(s) with insufficient evidence pack (EP2=0) |
| final answer | usable 100%, judge 1.80, no-hard-fail 100% | strong | 1 partial source alignment (FA3<2) |

### Failure Path

All 2 responses were usable. No hard failures at the final answer stage.

Soft weaknesses observed:

- **Query structuring**: strict pass rate 0%, no-hard-fail 50%, runtime core 0%
- **Evidence packing**: judge score 0.75/2, no-hard-fail 100%

Main observed weakness: **retrieval** (composite 0.00). final answer were strong.

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
| `d541773c-a7e2-42e0-bd59-44ddbfb8b29f` | `671dc57b-a589-4d1c-962a-2ba8564c97fc` | 1.6000 | true |
| `d541773c-a7e2-42e0-bd59-44ddbfb8b29f` | `b5bdb5ea-1e56-42d3-829a-8482e562825d` | 2.0000 | true |

## Token Usage

### Judge Calls by Suite

| suite | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| continuation_hypothesis_update_discipline | 1689 | 544 | 2233 | 0.000193 |
| continuation_next_check_progression | 1172 | 443 | 1615 | 0.000147 |
| continuation_observation_resolution_context_recovery | 1059 | 586 | 1645 | 0.000170 |
| continuation_problem_understanding_update | 1389 | 383 | 1772 | 0.000146 |
| evidence_pack_role_fit | 2884 | 763 | 3647 | 0.000297 |
| evidence_pack_sufficiency | 4104 | 676 | 4780 | 0.000340 |
| final_alternative_context_handling | 9736 | 704 | 10440 | 0.000628 |
| final_first_check_discriminates | 9858 | 768 | 10626 | 0.000647 |
| final_hypothesis_source_alignment | 8338 | 1947 | 10285 | 0.000806 |
| final_no_root_cause_claim | 9766 | 705 | 10471 | 0.000629 |
| final_result_interpretation_usefulness | 1775 | 1362 | 3137 | 0.000361 |
| query_structuring_field_boundary_correctness | 713 | 468 | 1181 | 0.000129 |
| query_structuring_grounding_conservatism | 683 | 893 | 1576 | 0.000213 |

### Totals

| scope | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| runtime | 8516 | 4531 | 13047 | 0.000796 |
| judge_total | 53166 | 10242 | 63408 | 0.004707 |
| run_total | 61682 | 14773 | 76455 | 0.005503 |

Run total cost usd = runtime total cost usd + judge total cost usd = 0.000796 + 0.004707 = 0.005503

## Appendix A: Full Query Structuring Diagnostics

### A.1 Contract Diagnostics

| field | invalid_vocab_count | duplicate_term_count |
|---|---:|---:|
| symptoms | — | — |
| affected_subsystems | — | — |
| failure_modes | — | — |
| system_properties | — | — |

### A.2 Selection Diagnostics

| field | num_predicted_terms | num_false_positive | num_false_negative_strict | zero_score_selection_count |
|---|---:|---:|---:|---:|
| symptoms | — | — | — | — |
| affected_subsystems | — | — | — | — |
| failure_modes | — | — | — | — |
| system_properties | — | — | — | — |

### A.3 Graded Relevance Diagnostics

| field | graded_coverage | average_selected_score |
|---|---:|---:|
| symptoms | — | — |
| affected_subsystems | — | — |
| failure_modes | — | — |
| system_properties | — | — |

### A.4 Grounding Diagnostics

| field | unsupported_selected_term_rate | missing_evidence_span_count | invalid_evidence_span_count | evidence_span_near_substring_rate |
|---|---:|---:|---:|---:|
| symptoms | — | — | — | — |
| affected_subsystems | — | — | — | — |
| failure_modes | — | — | — | — |
| system_properties | — | — | — | — |

### A.5 Support-Level Diagnostics

| field | weak_inference_rate | strict_terms_weak_inference_rate | weak_false_positive_rate |
|---|---:|---:|---:|
| symptoms | — | — | — |
| affected_subsystems | — | — | — |
| failure_modes | — | — | — |
| system_properties | — | — | — |

### A.6 Field Success Diagnostics

| field | field_core_success | field_grounded_success | empty_when_gold_exists |
|---|---:|---:|---:|
| symptoms | — | — | — |
| affected_subsystems | — | — | — |
| failure_modes | — | — | — |
| system_properties | — | — | — |

### A.7 Query-Level Non-Vocabulary Diagnostics

| entities_count_avg | constraints_count_avg | triggers_count_avg | observability_signals_count_avg | unresolved_terms_count_avg | intent_present_rate | scenario_present_rate |
|---:|---:|---:|---:|---:|---:|---:|
| — | — | — | — | — | — | — |

## Appendix B: Full Retrieval Diagnostics

### B.1 Retrieval Configuration

| retrieval_target | collection | top_k |
|---|---|---:|
| candidate_cards | cards | — |
| incident_primary | practice_chunks | — |
| incident_alternatives | practice_chunks | — |
| theory_evidence | theory_chunks | — |

### B.2 Retrieval Hit Counts

| retrieval_target | hits_count_avg | selected_count_avg | top_score_avg | min_score_avg |
|---|---:|---:|---:|---:|
| candidate_cards | — | — | — | — |
| incident_primary | — | — | — | — |
| incident_alternatives | — | — | — | — |
| theory_evidence | — | — | — | — |

## Appendix C: Judge Metrics Per Run

### Run `d541773c-a7e2-42e0-bd59-44ddbfb8b29f`

| metric | initial iter-s | continuation iter-s | total | formula |
|---|---:|---:|---:|---|
| usable_first_response_rate | 1.0000 | 1.0000 | 1.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 0.5000 | n/a | 0.5000 | mean of avg(QS1, QS2) over initial iter-s |
| evidence_pack_judge_score | 1.5000 | n/a | 1.5000 | mean of avg(EP1, EP2) over initial iter-s |
| final_answer_judge_score | 2.0000 | 1.6000 | 1.8000 | mean of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0.0000 | n/a | 0.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1.0000 | n/a | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1.0000 | 1.0000 | 1.0000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0.0000 | 0.0000 | 0.0000 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0.0000 | n/a | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0.0000 | n/a | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 1.0000 | 0.0000 | 0.5000 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |
| continuation_hypothesis_update_discipline_score | n/a | 2.0000 | 2.0000 | mean(CU1) over continuation iter-s |
| continuation_problem_understanding_update_score | n/a | 2.0000 | 2.0000 | mean(CU2) over continuation iter-s |
| continuation_next_check_progression_score | n/a | 2.0000 | 2.0000 | mean(CU3) over continuation iter-s |
| continuation_observation_resolution_context_recovery_score | n/a | 2.0000 | 2.0000 | mean(CU4) over continuation iter-s |
| usable_continuation_response_rate | n/a | 1.0000 | 1.0000 | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| continuation_update_judge_score | n/a | 2.0000 | 2.0000 | mean of avg(CU1, CU2, CU3) over continuation iter-s |
| continuation_update_no_hard_fail_rate | n/a | 1.0000 | 1.0000 | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |
| continuation_update_strict_pass_rate | n/a | 1.0000 | 1.0000 | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |
| continuation_input_judge_score | n/a | 2.0000 | 2.0000 | mean(CU4) over continuation iter-s |
| continuation_input_no_hard_fail_rate | n/a | 1.0000 | 1.0000 | frac(CU4>0) |
| continuation_input_strict_pass_rate | n/a | 1.0000 | 1.0000 | frac(CU4=2) |

