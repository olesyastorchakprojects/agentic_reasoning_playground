# Eval Run Report

## Run Metadata

- eval_run_id: `c0012711-4586-4c3b-9205-f71cdb8f6817`
- run_type: `golden_dataset`
- status: `completed`
- started_at: `2026-05-02 07:28:55.701858493 UTC`
- completed_at: `2026-05-02 07:30:28.486445942 UTC`
- runtime_run_count: `5`
- iterations_evaluated_count: `5`
- judge_model: `openai/gpt-oss-20b`
- suite_count: `4`

## Aggregated Metrics

| metric | value |
|---|---:|
| usable_first_response_rate | 1.0000 |
| query_structuring_judge_score | 0.0000 |
| evidence_pack_judge_score | 0.0000 |
| final_answer_judge_score | 1.9000 |
| query_structuring_strict_pass_rate | 1.0000 |
| evidence_pack_strict_pass_rate | 1.0000 |
| final_answer_strict_pass_rate | 1.0000 |
| diagnostic_move_hard_fail_rate | 0.0000 |

## Suite Distributions

| suite | score_0 | score_1 | score_2 |
|---|---:|---:|---:|
| final_no_root_cause_claim | 0 | 2 | 3 |
| final_first_check_discriminates | 0 | 0 | 5 |
| final_alternative_context_handling | 0 | 0 | 5 |
| final_result_interpretation_usefulness | 0 | 0 | 5 |
| final_hypothesis_source_alignment | 5 | 0 | 0 |
| query_structuring_field_boundary_correctness | 5 | 0 | 0 |
| query_structuring_grounding_conservatism | 5 | 0 | 0 |
| evidence_pack_role_fit | 5 | 0 | 0 |
| evidence_pack_sufficiency | 5 | 0 | 0 |

## Gate Breakdown

| gate | fail_count | fail_rate |
|---|---:|---:|
| no_root_cause_gate | 0 | 0.0000 |
| single_check_gate | 0 | 0.0000 |
| source_alignment_gate | 5 | 1.0000 |
| field_boundary_gate | 0 | 0.0000 |
| evidence_pack_gate | 0 | 0.0000 |

## Failure Attribution

| metric | value |
|---|---:|
| bad_final_due_to_query_rate | 0.0000 |
| bad_final_due_to_evidence_rate | 0.0000 |
| bad_final_with_good_query_and_evidence_rate | 0.0000 |

## Worst-Case Preview

| runtime_run_id | iteration_id | final_answer_score | usable_first_response |
|---|---|---:|---:|
| `4fa3e103-85ee-49ad-9e86-fa5a24a2dfb5` | `5cebf72a-37c3-4350-86c7-604c27cf5bb9` | 1.7500 | true |
| `7eaa8e22-a7da-4085-9754-ca1bad95f3fa` | `01203fbc-9a1c-4d76-93f9-1d838a2d1d11` | 1.7500 | true |
| `1a952a67-a47e-48d7-97c8-35574e152a62` | `24c29a95-c63a-409a-a021-9dd13da33a13` | 2.0000 | true |
| `cc7003a7-8a24-41c7-839e-16f3e41d2f87` | `59e8cb61-fef4-4049-af85-913bb9719987` | 2.0000 | true |
| `dedc6b3b-e881-49e6-b6c6-93a3e6e669f0` | `44385465-4950-47e5-827e-d710803574c9` | 2.0000 | true |

## Token Usage

| scope | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |
|---|---:|---:|---:|---:|
| runtime | 24770 | 12871 | 37641 | 0.000000 |
| judge_total | 79270 | 9090 | 88360 | 0.005782 |
| run_total | 104040 | 21961 | 126001 | 0.005782 |

Run total cost usd = runtime total cost usd + judge total cost usd = 0.000000 + 0.005782 = 0.005782
