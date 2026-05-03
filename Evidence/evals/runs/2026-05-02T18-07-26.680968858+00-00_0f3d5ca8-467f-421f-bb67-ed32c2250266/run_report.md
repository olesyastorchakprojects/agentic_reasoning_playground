# Eval Run Report

## Run Metadata

- eval_run_id: `0f3d5ca8-467f-421f-bb67-ed32c2250266`
- run_type: `golden_dataset`
- status: `completed`
- started_at: `2026-05-02 18:07:26.680968858 UTC`
- completed_at: `2026-05-02 21:42:48.008129732 UTC`
- runtime_run_count: `5`
- iterations_evaluated_count: `0`
- judge_model: `openai/gpt-oss-20b`
- suite_count: `9`

## Suite Overview

### query_structuring_field_boundary_correctness

| code | checks | why | inputs | score |
|---|---|---|---|---:|
| QS1 | Whether symptoms, affected_subsystems, failure_modes, and system_properties respect their intended meanings | Bad field separation poisons downstream retrieval and diagnosis — this is the most important semantic eval for query structuring | original user query, structured query output, controlled vocabulary definitions | 0/1/2 |

### query_structuring_grounding_conservatism

| code | checks | why | inputs | score |
|---|---|---|---|---:|
| QS2 | Whether selected vocabulary terms are sufficiently supported by the user query, and whether the model avoids weak over-inference | Protects against hallucinated or overly eager labels that make retrieval look precise while being wrong | original user query, structured query output, selected terms with evidence_span and support_level | 0/1/2 |

### evidence_pack_role_fit

| code | checks | why | inputs | score |
|---|---|---|---|---:|
| EP1 | Whether each selected chunk fits its assigned role: evidence_for_match, first_check_hint, supporting_explanation, alternative_context, mechanism_explanation | Chunks may be generally relevant but diagnostically misplaced; role fit is where evidence packing most often fails | user query, structured query, selected chunks with roles, role definitions | 0/1/2 |

### evidence_pack_sufficiency

| code | checks | why | inputs | score |
|---|---|---|---|---:|
| EP2 | Whether the selected evidence pack is enough to support a useful first diagnostic move | Evaluates the pack as a whole — good individual chunks can still leave the model unable to form hypotheses | user query, structured query, primary card, selected incident chunks, selected theory chunks | 0/1/2 |

### final_no_root_cause_claim

| code | checks | why | inputs | score |
|---|---|---|---|---:|
| FA1 | Whether the answer avoids claiming or implying a final root cause | The assistant produces a first diagnostic frame, not a final diagnosis — premature certainty is an epistemic failure | JSON context, final answer | 0/1/2 |

### final_first_check_discriminates

| code | checks | why | inputs | score |
|---|---|---|---|---:|
| FA2 | Whether first_check is exactly one actionable check that distinguishes between active hypotheses or primary vs competing interpretation | This is the core product value — a checklist or vague advice is not a first diagnostic move | JSON context, final answer, active hypotheses, result interpretation | 0/1/2 |

### final_hypothesis_source_alignment

| code | checks | why | inputs | score |
|---|---|---|---|---:|
| FA3 | Whether each hypothesis is supported by its declared source: primary_incident, alternative_context, or theory_mechanism | Explicit source labels are only useful if they are honest — misaligned sources mislead the user about confidence | evidence topology, matched card, incident chunks, theory chunks, final answer | 0/1/2 |

### final_alternative_context_handling

| code | checks | why | inputs | score |
|---|---|---|---|---:|
| FA4 | Whether alternative context is used when genuinely useful and not forced when weak | Protects against both premature convergence and fake symmetry — both are epistemic failures | evidence topology, alternative context chunks, final answer | 0/1/2 |

### final_result_interpretation_usefulness

| code | checks | why | inputs | score |
|---|---|---|---|---:|
| FA5 | Whether supports_primary_if, supports_competing_if, and inconclusive_if explain how to interpret the first check result | Makes the first check operational — without interpretation guidance, the check is decorative | final answer, active hypotheses, first check | 0/1/2 |

## Metric Layers

| layer | source | evaluates | interpretation |
|---|---|---|---|
| Judge-based quality metrics | judge model outputs | semantic quality of structuring, evidence pack, and final answer | answers whether the diagnostic behavior is good |
| Runtime gold metrics | runtime trace spans with golden labels | query structuring and retrieval against expected labels / evidence | answers whether upstream modules selected the expected terms and evidence |
| Runtime diagnostics | runtime trace attributes and events | low-level counters, hit counts, configuration, support-level issues | helps debug why a metric failed |

## Executive Summary

| metric | value | meaning |
|---|---:|---|
| usable_first_response_rate | 0.0000 | Share of runs where the final answer can be shown as a first diagnostic response |
| gate_pass_rate | 0.0000 | Share of runs without critical gate failures |
| query_structuring_judge_score | -0.0000 | Judge-based semantic quality of query structuring |
| runtime_query_structuring_core_success_rate | 0.0000 | Gold-backed runtime success of structured query fields |
| runtime_retrieval_mean_ndcg | 0.0000 | Average ranking quality across retrieval targets and runs |
| runtime_retrieval_all_strict_recall_success_rate | 0.0000 | Average per-run share of retrieval targets where strict expected evidence was found |
| evidence_pack_judge_score | -0.0000 | Judge-based quality of selected evidence pack |
| final_answer_judge_score | -0.0000 | Judge-based quality of final diagnostic response |

## Judge-Based Aggregated Metrics

| metric | value | formula |
|---|---:|---|
| usable_first_response_rate | 0.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | -0.0000 | mean over runs of avg(QS1, QS2) |
| evidence_pack_judge_score | -0.0000 | mean over runs of avg(EP1, EP2) |
| final_answer_judge_score | -0.0000 | mean over runs of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 0.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 0.0000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0.0000 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0.0000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0.0000 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |

> QS1 = query_structuring_field_boundary_correctness ; QS2 = query_structuring_grounding_conservatism
> EP1 = evidence_pack_role_fit ; EP2 = evidence_pack_sufficiency
> FA1 = final_no_root_cause_claim ; FA2 = final_first_check_discriminates ; FA3 = final_hypothesis_source_alignment ; FA4 = final_alternative_context_handling ; FA5 = final_result_interpretation_usefulness

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
| final_no_root_cause_claim | 0 | 0 | 0 |

## Gate Breakdown

| gate | fail_count | fail_rate |
|---|---:|---:|
| final_no_root_cause_claim | 0 | 0.0000 |
| final_first_check_discriminates | 0 | 0.0000 |
| final_hypothesis_source_alignment | 0 | 0.0000 |
| query_structuring_field_boundary_correctness | 0 | 0.0000 |
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

## Token Usage

### Totals

| scope | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| runtime | 0 | 0 | 0 | -0.000000 |
| judge_total | 0 | 0 | 0 | -0.000000 |
| run_total | 0 | 0 | 0 | -0.000000 |

Run total cost usd = runtime total cost usd + judge total cost usd = -0.000000 + -0.000000 = -0.000000

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

