# Eval Run Report

## Run Metadata

- eval_run_id: `e9907736-7fd8-48d4-b363-51b3b7c9f82f`
- run_type: `golden_dataset`
- status: `completed`
- started_at: `2026-05-02 13:13:34.046965141 UTC`
- completed_at: `2026-05-02 14:02:37.384767071 UTC`
- runtime_run_count: `1`
- iterations_evaluated_count: `1`
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

## Aggregated Metrics

| metric | value | formula |
|---|---:|---|
| usable_first_response_rate | 0.0000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 1.5000 | avg(QS1, QS2) over enabled suites |
| evidence_pack_judge_score | 0.5000 | avg(EP1, EP2) over enabled suites |
| final_answer_judge_score | 1.4000 | avg(FA1, FA2, FA3, FA4, FA5) over enabled suites |
| query_structuring_strict_pass_rate | 1.0000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_strict_pass_rate | 0.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_strict_pass_rate | 0.0000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 1.0000 | 1 − final_answer_strict_pass_rate |

> QS1 = query_structuring_field_boundary_correctness ; QS2 = query_structuring_grounding_conservatism
> EP1 = evidence_pack_role_fit ; EP2 = evidence_pack_sufficiency
> FA1 = final_no_root_cause_claim ; FA2 = final_first_check_discriminates ; FA3 = final_hypothesis_source_alignment ; FA4 = final_alternative_context_handling ; FA5 = final_result_interpretation_usefulness

## Suite Distributions

| suite | score_0 | score_1 | score_2 |
|---|---:|---:|---:|
| final_no_root_cause_claim | 1 | 0 | 0 |
| final_first_check_discriminates | 0 | 0 | 1 |
| final_alternative_context_handling | 0 | 1 | 0 |
| final_result_interpretation_usefulness | 0 | 0 | 1 |
| final_hypothesis_source_alignment | 0 | 0 | 1 |
| query_structuring_field_boundary_correctness | 0 | 0 | 1 |
| query_structuring_grounding_conservatism | 0 | 1 | 0 |
| evidence_pack_role_fit | 0 | 1 | 0 |
| evidence_pack_sufficiency | 1 | 0 | 0 |

## Gate Breakdown

| gate | fail_count | fail_rate |
|---|---:|---:|
| final_no_root_cause_claim | 1 | 1.0000 |
| final_first_check_discriminates | 0 | 0.0000 |
| final_hypothesis_source_alignment | 0 | 0.0000 |
| query_structuring_field_boundary_correctness | 0 | 0.0000 |
| evidence_pack_sufficiency | 1 | 1.0000 |

> Gate fails when suite score = 0. Pass threshold: score ≥ 1.

## Failure Attribution

| metric | value | formula |
|---|---:|---|
| bad_final_due_to_query_rate | 0.0000 | frac(!usable ∧ (QS1=0 ∨ QS2=0)) |
| bad_final_due_to_evidence_rate | 1.0000 | frac(!usable ∧ (EP1=0 ∨ EP2=0)) |
| bad_final_with_good_query_and_evidence_rate | 0.0000 | frac(!usable ∧ QS1>0 ∧ QS2>0 ∧ EP1>0 ∧ EP2>0) |

> usable = FA1≥1 ∧ FA2≥1 ∧ FA5≥1

> QS1 = query_structuring_field_boundary_correctness ; QS2 = query_structuring_grounding_conservatism
> EP1 = evidence_pack_role_fit ; EP2 = evidence_pack_sufficiency
> FA1 = final_no_root_cause_claim ; FA2 = final_first_check_discriminates ; FA3 = final_hypothesis_source_alignment ; FA4 = final_alternative_context_handling ; FA5 = final_result_interpretation_usefulness

## Worst-Case Preview

| runtime_run_id | iteration_id | final_answer_score | usable_first_response |
|---|---|---:|---:|
| `2a7ed93a-b0b6-4dba-a498-b3df911ed561` | `e82d2819-8519-4468-860d-da6a705e582d` | 1.4000 | false |

## Token Usage

### Judge Calls by Suite

| suite | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| evidence_pack_role_fit | 3028 | 1329 | 4357 | 0.000417 |
| evidence_pack_sufficiency | 4279 | 753 | 5032 | 0.000365 |
| final_alternative_context_handling | 4951 | 710 | 5661 | 0.000390 |
| final_first_check_discriminates | 5012 | 257 | 5269 | 0.000302 |
| final_hypothesis_source_alignment | 4405 | 581 | 4986 | 0.000336 |
| final_no_root_cause_claim | 4966 | 683 | 5649 | 0.000385 |
| final_result_interpretation_usefulness | 792 | 619 | 1411 | 0.000163 |
| query_structuring_field_boundary_correctness | 626 | 401 | 1027 | 0.000111 |
| query_structuring_grounding_conservatism | 596 | 656 | 1252 | 0.000161 |

### Totals

| scope | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| runtime | 5001 | 2486 | 7487 | 0.000747 |
| judge_total | 28655 | 5989 | 34644 | 0.002631 |
| run_total | 33656 | 8475 | 42131 | 0.003378 |

Run total cost usd = runtime total cost usd + judge total cost usd = 0.000747 + 0.002631 = 0.003378
