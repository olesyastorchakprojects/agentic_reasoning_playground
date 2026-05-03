# Eval Run Report

## Run Metadata

- eval_run_id: `45fb8e47-8672-476a-b7d3-7d4010955ff5`
- run_type: `golden_dataset`
- status: `completed`
- started_at: `2026-05-02 13:02:36.773887619 UTC`
- completed_at: `2026-05-02 13:08:32.547970539 UTC`
- runtime_run_count: `1`
- iterations_evaluated_count: `1`
- judge_model: `openai/gpt-oss-20b`
- suite_count: `2`

## Suite Overview

### query_structuring_field_boundary_correctness

| checks | why | inputs | score |
|---|---|---|---:|
| Whether symptoms, affected_subsystems, failure_modes, and system_properties respect their intended meanings | Bad field separation poisons downstream retrieval and diagnosis — this is the most important semantic eval for query structuring | original user query, structured query output, controlled vocabulary definitions | 0/0/1 |

### query_structuring_grounding_conservatism

| checks | why | inputs | score |
|---|---|---|---:|
| Whether selected vocabulary terms are sufficiently supported by the user query, and whether the model avoids weak over-inference | Protects against hallucinated or overly eager labels that make retrieval look precise while being wrong | original user query, structured query output, selected terms with evidence_span and support_level | 0/1/0 |

## Aggregated Metrics

| metric | value |
|---|---:|
| usable_first_response_rate | 0.0000 |
| query_structuring_judge_score | 1.5000 |
| evidence_pack_judge_score | 0.0000 |
| final_answer_judge_score | 0.0000 |
| query_structuring_strict_pass_rate | 1.0000 |
| evidence_pack_strict_pass_rate | 1.0000 |
| final_answer_strict_pass_rate | 0.0000 |
| diagnostic_move_hard_fail_rate | 1.0000 |

## Suite Distributions

| suite | score_0 | score_1 | score_2 |
|---|---:|---:|---:|
| final_no_root_cause_claim | 1 | 0 | 0 |
| final_first_check_discriminates | 1 | 0 | 0 |
| final_alternative_context_handling | 1 | 0 | 0 |
| final_result_interpretation_usefulness | 1 | 0 | 0 |
| final_hypothesis_source_alignment | 1 | 0 | 0 |
| query_structuring_field_boundary_correctness | 0 | 0 | 1 |
| query_structuring_grounding_conservatism | 0 | 1 | 0 |
| evidence_pack_role_fit | 1 | 0 | 0 |
| evidence_pack_sufficiency | 1 | 0 | 0 |

## Gate Breakdown

| gate | fail_count | fail_rate |
|---|---:|---:|
| no_root_cause_gate | 1 | 1.0000 |
| single_check_gate | 1 | 1.0000 |
| source_alignment_gate | 1 | 1.0000 |
| field_boundary_gate | 0 | 0.0000 |
| evidence_pack_gate | 0 | 0.0000 |

## Failure Attribution

| metric | value |
|---|---:|
| bad_final_due_to_query_rate | 0.0000 |
| bad_final_due_to_evidence_rate | 0.0000 |
| bad_final_with_good_query_and_evidence_rate | 1.0000 |

## Worst-Case Preview

| runtime_run_id | iteration_id | final_answer_score | usable_first_response |
|---|---|---:|---:|
| `2a7ed93a-b0b6-4dba-a498-b3df911ed561` | `e82d2819-8519-4468-860d-da6a705e582d` | 0.0000 | false |

## Token Usage

### Judge Calls by Suite

| suite | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| query_structuring_field_boundary_correctness | 626 | 469 | 1095 | 0.000125 |
| query_structuring_grounding_conservatism | 596 | 713 | 1309 | 0.000172 |

### Totals

| scope | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| runtime | 5001 | 2486 | 7487 | 0.000747 |
| judge_total | 1222 | 1182 | 2404 | 0.000298 |
| run_total | 6223 | 3668 | 9891 | 0.001045 |

Run total cost usd = runtime total cost usd + judge total cost usd = 0.000747 + 0.000298 = 0.001045
