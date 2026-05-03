# Eval Run Report

## Run Metadata

- eval_run_id: `2bc12c2e-9d1b-4505-971b-d6c6da6d09cf`
- run_type: `golden_dataset`
- status: `completed`
- started_at: `2026-05-02 21:45:47.530484054 UTC`
- completed_at: `2026-05-02 22:00:23.065074615 UTC`
- runtime_run_count: `5`
- iterations_evaluated_count: `5`
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
| usable_first_response_rate | 1.0000 | Share of runs where the final answer can be shown as a first diagnostic response |
| gate_pass_rate | 0.8000 | Share of runs without critical gate failures |
| query_structuring_judge_score | 1.0000 | Judge-based semantic quality of query structuring |
| runtime_query_structuring_core_success_rate | 0.7000 | Gold-backed runtime success of structured query fields |
| runtime_retrieval_mean_ndcg | 0.9419 | Average ranking quality across retrieval targets and runs |
| runtime_retrieval_all_strict_recall_success_rate | 1.0000 | Average per-run share of retrieval targets where strict expected evidence was found |
| evidence_pack_judge_score | 1.5000 | Judge-based quality of selected evidence pack |
| final_answer_judge_score | 1.8800 | Judge-based quality of final diagnostic response |

## Judge-Based Aggregated Metrics

| metric | value | formula |
|---|---:|---|
| usable_first_response_rate | 1.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 1.0000 | mean over runs of avg(QS1, QS2) |
| evidence_pack_judge_score | 1.5000 | mean over runs of avg(EP1, EP2) |
| final_answer_judge_score | 1.8800 | mean over runs of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0.6000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 0.8000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 1.0000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0.0000 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0.2000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0.6000 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |

> QS1 = query_structuring_field_boundary_correctness ; QS2 = query_structuring_grounding_conservatism
> EP1 = evidence_pack_role_fit ; EP2 = evidence_pack_sufficiency
> FA1 = final_no_root_cause_claim ; FA2 = final_first_check_discriminates ; FA3 = final_hypothesis_source_alignment ; FA4 = final_alternative_context_handling ; FA5 = final_result_interpretation_usefulness

## Runtime Gold Metrics

These metrics are computed from runtime trace spans and compare structured query / retrieval outputs against golden labels.

### Query Structuring Core Metrics

| metric | value | meaning |
|---|---:|---|
| runtime_query_structuring_macro_precision_soft | 0.7583 | How many selected vocabulary terms are acceptable under soft relevance |
| runtime_query_structuring_macro_recall_strict | 0.7000 | Whether strictly expected terms were recovered |
| runtime_query_structuring_macro_recall_soft | 0.5000 | Coverage of broader acceptable terms |
| runtime_query_structuring_grounded_strict_recall | 0.7000 | Whether strict terms are selected with valid grounding |
| runtime_query_structuring_core_success_rate | 0.7000 | Whether all vocab fields passed their core gold-backed checks |

#### Query Structuring Field Core Metrics

| field | precision_soft | recall_strict | recall_soft | grounded_strict_recall | field_core_success | field_grounded_success |
|---|---:|---:|---:|---:|---:|---:|
| symptoms | 0.7333 | 0.8000 | 0.5000 | 0.8000 | 0.8000 | 0.8000 |
| affected_subsystems | 0.7000 | 0.6000 | 0.4000 | 0.6000 | 0.6000 | 0.6000 |
| failure_modes | 0.8000 | 0.8000 | 0.5000 | 0.8000 | 0.8000 | 0.8000 |
| system_properties | 0.8000 | 0.6000 | 0.6000 | 0.6000 | 0.6000 | 0.6000 |

### Retrieval Core Metrics

> Each value is averaged over runs where the target was evaluated.

| retrieval_target | evaluated_k | recall_strict | recall_soft | rr_strict | rr_soft | nDCG | frr_strict | frr_soft | n_strict | n_soft |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| candidate_cards | 8.0 | 1.0000 | 1.0000 | 1.0000 | 1.0000 | 1.0000 | 1.00 | 1.00 | 1.00 | 3.00 |
| incident_primary | 12.0 | 1.0000 | 0.9333 | 1.0000 | 1.0000 | 0.9681 | 1.00 | 1.00 | 1.40 | 3.20 |
| incident_alternatives | 12.0 | 1.0000 | 0.8667 | 1.0000 | 1.0000 | 0.9233 | 1.00 | 1.00 | 1.00 | 2.60 |
| theory_evidence | 12.0 | 1.0000 | 0.9000 | 0.8667 | 0.8667 | 0.8760 | 1.40 | 1.40 | 1.00 | 1.80 |

### Retrieval Summary

| metric | value | formula | meaning |
|---|---:|---|---|
| runtime_retrieval_mean_ndcg | 0.9419 | avg_run(avg_target(ndcg)) | Average ranking quality across retrieval targets and runs |
| runtime_retrieval_all_strict_recall_success_rate | 1.0000 | avg_run(frac_target(recall_strict=1)) | Average per-run share of retrieval targets with strict recall success |
| runtime_retrieval_all_soft_recall_success_rate | 1.0000 | avg_run(frac_target(recall_soft>0)) | Average per-run share of retrieval targets with any soft recall |
| runtime_retrieval_penalized_first_relevant_rank_strict | 1.10 | avg_run(avg_target(frr_strict or k+1)) | Penalized rank; missing strict hit treated as k+1 |
| runtime_retrieval_zero_hit_rate | 0.0000 | avg_run(frac_target(hits_count=0)) | Average per-run share of retrieval calls with no hits |

## Suite Distributions

| suite | score_0 | score_1 | score_2 |
|---|---:|---:|---:|
| final_no_root_cause_claim | 0 | 0 | 5 |
| final_first_check_discriminates | 0 | 0 | 5 |
| final_alternative_context_handling | 0 | 1 | 4 |
| final_result_interpretation_usefulness | 0 | 0 | 5 |
| final_hypothesis_source_alignment | 0 | 2 | 3 |
| query_structuring_field_boundary_correctness | 1 | 2 | 2 |
| query_structuring_grounding_conservatism | 1 | 4 | 0 |
| evidence_pack_role_fit | 1 | 3 | 1 |
| evidence_pack_sufficiency | 0 | 0 | 5 |

## Gate Breakdown

| gate | fail_count | fail_rate |
|---|---:|---:|
| final_no_root_cause_claim | 0 | 0.0000 |
| final_first_check_discriminates | 0 | 0.0000 |
| final_hypothesis_source_alignment | 0 | 0.0000 |
| query_structuring_field_boundary_correctness | 1 | 0.2000 |
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

## Where Quality Was Lost

### Pipeline Stage Summary

| stage | signals | status | interpretation |
|---|---|---|---|
| query structuring | judge 1.00, no-hard-fail 60%, runtime core 70% | mixed | no strict pass on any run; 1 field boundary gate fail(s); runtime core success 70% |
| retrieval | strict recall 100%, nDCG 0.94 | strong | expected evidence found in all runs across all targets |
| evidence packing | judge 1.50, no-hard-fail 80% | strong | selected evidence pack was sufficient and mostly role-appropriate |
| final answer | usable 100%, judge 1.88, no-hard-fail 100% | strong | 2 partial source alignment (FA3<2) |

### Failure Path

All 5 responses were usable. No hard failures at the final answer stage.

Soft weaknesses observed:

- **Query structuring**: strict pass rate 0%, no-hard-fail 60%, runtime core 70%

Main observed weakness: **query structuring** (composite 0.60). retrieval, evidence packing, final answer were strong.

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
| `14af2f6a-35ad-4816-b8ed-f189106088db` | `cffff2e9-f043-4090-9d46-99eb48b56a11` | 1.6000 | true |
| `48f2268a-73af-409d-b0db-0b2ae61f0f1e` | `9ea400d5-17b3-4691-8a2f-319f525ad11c` | 1.8000 | true |
| `1e5c19cb-baeb-4bb4-a5c8-8f2890edabe5` | `f98e0893-9269-49be-9443-694681b40483` | 2.0000 | true |
| `4b268668-9fca-4898-81b2-710ec74a4489` | `f689c406-13d4-433d-bcce-9c09f1fa7f9d` | 2.0000 | true |
| `bd6f8d33-7452-43a0-b06d-1355d4dfba96` | `d4e53bc5-a245-4514-916a-72a4c276f47f` | 2.0000 | true |

## Token Usage

### Judge Calls by Suite

| suite | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| evidence_pack_role_fit | 15223 | 5591 | 20814 | 0.001879 |
| evidence_pack_sufficiency | 21680 | 4308 | 25988 | 0.001946 |
| final_alternative_context_handling | 24856 | 2489 | 27345 | 0.001741 |
| final_first_check_discriminates | 25161 | 2201 | 27362 | 0.001698 |
| final_hypothesis_source_alignment | 21984 | 3156 | 25140 | 0.001730 |
| final_no_root_cause_claim | 24931 | 1057 | 25988 | 0.001458 |
| final_result_interpretation_usefulness | 3807 | 2852 | 6659 | 0.000761 |
| query_structuring_field_boundary_correctness | 3375 | 3051 | 6426 | 0.000779 |
| query_structuring_grounding_conservatism | 3225 | 4306 | 7531 | 0.001022 |

### Totals

| scope | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| runtime | 25071 | 12474 | 37545 | 0.003748 |
| judge_total | 144242 | 29011 | 173253 | 0.013014 |
| run_total | 169313 | 41485 | 210798 | 0.016763 |

Run total cost usd = runtime total cost usd + judge total cost usd = 0.003748 + 0.013014 = 0.016763

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
| symptoms | 1.40 | 0.40 | 0.20 | 0.40 |
| affected_subsystems | 1.20 | 0.40 | 0.40 | 0.40 |
| failure_modes | 1.00 | 0.20 | 0.20 | 0.20 |
| system_properties | 1.20 | 0.20 | 0.40 | 0.20 |

### A.3 Graded Relevance Diagnostics

| field | graded_coverage | average_selected_score |
|---|---:|---:|
| symptoms | 0.6000 | 0.7000 |
| affected_subsystems | 0.4667 | 0.6500 |
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
| symptoms | 0.8000 | 0.8000 | 0.0000 |
| affected_subsystems | 0.6000 | 0.6000 | 0.0000 |
| failure_modes | 0.8000 | 0.8000 | 0.0000 |
| system_properties | 0.6000 | 0.6000 | 0.0000 |

### A.7 Query-Level Non-Vocabulary Diagnostics

| entities_count_avg | constraints_count_avg | triggers_count_avg | observability_signals_count_avg | unresolved_terms_count_avg | intent_present_rate | scenario_present_rate |
|---:|---:|---:|---:|---:|---:|---:|
| 1.40 | 0.60 | 1.00 | 1.40 | 0.00 | 1.0000 | 1.0000 |

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
| candidate_cards | 6.8 | 3.0 | 0.9667 | 0.2419 |
| incident_primary | 7.4 | 7.4 | 0.7167 | 0.2739 |
| incident_alternatives | 8.4 | 8.4 | 0.5786 | 0.2000 |
| theory_evidence | 7.6 | 7.6 | 0.6500 | 0.2000 |

