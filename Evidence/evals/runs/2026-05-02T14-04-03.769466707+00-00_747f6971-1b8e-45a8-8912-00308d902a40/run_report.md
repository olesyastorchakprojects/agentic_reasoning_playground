# Eval Run Report

## Run Metadata

- eval_run_id: `747f6971-1b8e-45a8-8912-00308d902a40`
- run_type: `golden_dataset`
- status: `completed`
- started_at: `2026-05-02 14:04:03.769466707 UTC`
- completed_at: `2026-05-02 16:55:01.307384981 UTC`
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

## Aggregated Metrics

| metric | value | formula |
|---|---:|---|
| usable_first_response_rate | 0.8000 | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |
| query_structuring_judge_score | 0.9000 | mean over runs of avg(QS1, QS2) |
| evidence_pack_judge_score | 1.6000 | mean over runs of avg(EP1, EP2) |
| final_answer_judge_score | 1.8400 | mean over runs of avg(FA1, FA2, FA3, FA4, FA5) |
| query_structuring_no_hard_fail_rate | 0.6000 | frac(QS1>0 ∧ QS2>0) |
| evidence_pack_no_hard_fail_rate | 1.0000 | frac(EP1>0 ∧ EP2>0) |
| final_answer_no_hard_fail_rate | 0.8000 | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |
| diagnostic_move_hard_fail_rate | 0.2000 | 1 − final_answer_no_hard_fail_rate |
| query_structuring_strict_pass_rate | 0.0000 | frac(QS1=2 ∧ QS2=2) |
| evidence_pack_strict_pass_rate | 0.2000 | frac(EP1=2 ∧ EP2=2) |
| final_answer_strict_pass_rate | 0.4000 | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |

> QS1 = query_structuring_field_boundary_correctness ; QS2 = query_structuring_grounding_conservatism
> EP1 = evidence_pack_role_fit ; EP2 = evidence_pack_sufficiency
> FA1 = final_no_root_cause_claim ; FA2 = final_first_check_discriminates ; FA3 = final_hypothesis_source_alignment ; FA4 = final_alternative_context_handling ; FA5 = final_result_interpretation_usefulness

## Suite Distributions

| suite | score_0 | score_1 | score_2 |
|---|---:|---:|---:|
| final_no_root_cause_claim | 1 | 0 | 4 |
| final_first_check_discriminates | 0 | 1 | 4 |
| final_alternative_context_handling | 0 | 0 | 5 |
| final_result_interpretation_usefulness | 0 | 0 | 5 |
| final_hypothesis_source_alignment | 0 | 1 | 4 |
| query_structuring_field_boundary_correctness | 2 | 2 | 1 |
| query_structuring_grounding_conservatism | 0 | 5 | 0 |
| evidence_pack_role_fit | 0 | 4 | 1 |
| evidence_pack_sufficiency | 0 | 0 | 5 |

## Gate Breakdown

| gate | fail_count | fail_rate |
|---|---:|---:|
| final_no_root_cause_claim | 1 | 0.2000 |
| final_first_check_discriminates | 0 | 0.0000 |
| final_hypothesis_source_alignment | 0 | 0.0000 |
| query_structuring_field_boundary_correctness | 2 | 0.4000 |
| evidence_pack_sufficiency | 0 | 0.0000 |

> Gate fails when suite score = 0. Pass threshold: score ≥ 1.

## Failure Attribution

| metric | value | formula |
|---|---:|---|
| bad_final_due_to_query_rate | 0.0000 | frac(!usable ∧ (QS1=0 ∨ QS2=0)) |
| bad_final_due_to_evidence_rate | 0.0000 | frac(!usable ∧ (EP1=0 ∨ EP2=0)) |
| bad_final_with_good_query_and_evidence_rate | 0.2000 | frac(!usable ∧ QS1>0 ∧ QS2>0 ∧ EP1>0 ∧ EP2>0) |

> usable = FA1≥1 ∧ FA2≥1 ∧ FA5≥1

> QS1 = query_structuring_field_boundary_correctness ; QS2 = query_structuring_grounding_conservatism
> EP1 = evidence_pack_role_fit ; EP2 = evidence_pack_sufficiency
> FA1 = final_no_root_cause_claim ; FA2 = final_first_check_discriminates ; FA3 = final_hypothesis_source_alignment ; FA4 = final_alternative_context_handling ; FA5 = final_result_interpretation_usefulness

## Worst-Case Preview

| runtime_run_id | iteration_id | final_answer_score | usable_first_response |
|---|---|---:|---:|
| `a11d8069-39ec-465c-956e-d91504f275d8` | `312939b5-f25b-47e6-9901-169a27828a6f` | 1.6000 | false |
| `22446d38-a8c2-40fb-9241-7fd4b1b271df` | `b2b1e00f-f5cc-4498-b32f-01449bf718ac` | 1.8000 | true |
| `648dc603-eba3-4970-b867-d8559605f55b` | `1f0bda38-7d75-4a7f-a571-d7803148f567` | 1.8000 | true |
| `2a7ed93a-b0b6-4dba-a498-b3df911ed561` | `e82d2819-8519-4468-860d-da6a705e582d` | 2.0000 | true |
| `51a08d66-f1aa-4903-9516-cc587c5ddd5b` | `cbd6932c-13e3-4b8d-b1e3-79df9980f1c4` | 2.0000 | true |

## Token Usage

### Judge Calls by Suite

| suite | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| evidence_pack_role_fit | 15529 | 4933 | 20462 | 0.001763 |
| evidence_pack_sufficiency | 21986 | 4129 | 26115 | 0.001925 |
| final_alternative_context_handling | 25428 | 2298 | 27726 | 0.001731 |
| final_first_check_discriminates | 25733 | 2361 | 28094 | 0.001759 |
| final_hypothesis_source_alignment | 21956 | 3504 | 25460 | 0.001799 |
| final_no_root_cause_claim | 25503 | 1718 | 27221 | 0.001619 |
| final_result_interpretation_usefulness | 4055 | 3094 | 7149 | 0.000822 |
| query_structuring_field_boundary_correctness | 3837 | 3386 | 7223 | 0.000869 |
| query_structuring_grounding_conservatism | 3687 | 4207 | 7894 | 0.001026 |

### Totals

| scope | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| runtime | 24929 | 12564 | 37493 | 0.003759 |
| judge_total | 147714 | 29630 | 177344 | 0.013312 |
| run_total | 172643 | 42194 | 214837 | 0.017071 |

Run total cost usd = runtime total cost usd + judge total cost usd = 0.003759 + 0.013312 = 0.017071
