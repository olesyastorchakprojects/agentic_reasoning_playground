# Eval Run Report

## Run Metadata

- eval_run_id: `d474f6db-80be-4a48-a5a0-b5ffc6e16a28`
- run_type: `golden_dataset`
- status: `completed`
- started_at: `2026-05-02 12:41:36.705582958 UTC`
- completed_at: `2026-05-02 12:41:48.976687355 UTC`
- runtime_run_count: `1`
- iterations_evaluated_count: `1`
- judge_model: `openai/gpt-oss-20b`
- suite_count: `2`

## Aggregated Metrics

| metric | value |
|---|---:|
| usable_first_response_rate | 0.0000 |
| query_structuring_judge_score | 0.5000 |
| evidence_pack_judge_score | 0.0000 |
| final_answer_judge_score | 0.0000 |
| query_structuring_strict_pass_rate | 0.0000 |
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
| query_structuring_field_boundary_correctness | 1 | 0 | 0 |
| query_structuring_grounding_conservatism | 0 | 1 | 0 |
| evidence_pack_role_fit | 1 | 0 | 0 |
| evidence_pack_sufficiency | 1 | 0 | 0 |

## Gate Breakdown

| gate | fail_count | fail_rate |
|---|---:|---:|
| no_root_cause_gate | 1 | 1.0000 |
| single_check_gate | 1 | 1.0000 |
| source_alignment_gate | 1 | 1.0000 |
| field_boundary_gate | 1 | 1.0000 |
| evidence_pack_gate | 0 | 0.0000 |

## Failure Attribution

| metric | value |
|---|---:|
| bad_final_due_to_query_rate | 1.0000 |
| bad_final_due_to_evidence_rate | 0.0000 |
| bad_final_with_good_query_and_evidence_rate | 0.0000 |

## Worst-Case Preview

| runtime_run_id | iteration_id | final_answer_score | usable_first_response |
|---|---|---:|---:|
| `2a7ed93a-b0b6-4dba-a498-b3df911ed561` | `e82d2819-8519-4468-860d-da6a705e582d` | 0.0000 | false |

## Token Usage

### Judge Calls by Suite

| suite | prompt_tokens | completion_tokens | total_cost_usd |
|---|---:|---:|---:|
| query_structuring_field_boundary_correctness | 626 | 548 | 0.000141 |
| query_structuring_grounding_conservatism | 596 | 673 | 0.000164 |

### Totals

| scope | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| runtime | 5001 | 2486 | 7487 | 0.000747 |
| judge_total | 1222 | 1221 | 2443 | 0.000305 |
| run_total | 6223 | 3707 | 9930 | 0.001053 |

Run total cost usd = runtime total cost usd + judge total cost usd = 0.000747 + 0.000305 = 0.001053
