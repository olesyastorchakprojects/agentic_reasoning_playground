# Eval Run Report

## Run Metadata

- eval_run_id: `1bfc954f-e1d1-4e85-9070-bea54bcf242e`
- run_type: `golden_dataset`
- status: `completed`
- started_at: `2026-05-01 21:24:33.637011839 UTC`
- completed_at: `2026-05-01 21:29:16.634480638 UTC`
- runtime_run_count: `5`
- iterations_evaluated_count: `5`
- judge_model: `openai/gpt-oss-20b`
- suite_versions: `1`

## Aggregated Metrics

| metric | value |
|---|---:|
| usable_first_response_rate | 1.0000 |
| final_answer_judge_score | 2.0000 |
| final_answer_strict_pass_rate | 1.0000 |
| diagnostic_move_hard_fail_rate | 0.0000 |

## Suite Distributions

| suite | score_0 | score_1 | score_2 |
|---|---:|---:|---:|
| final_no_root_cause_claim | 0 | 0 | 5 |

## Gate Breakdown

| gate | fail_count | fail_rate |
|---|---:|---:|
| no_root_cause_gate_passed | 0 | 0.0000 |

## Failure Attribution

| metric | value |
|---|---:|
| bad_final_due_to_query_rate | 0.0000 |
| bad_final_due_to_evidence_rate | 0.0000 |
| bad_final_with_good_query_and_evidence_rate | 0.0000 |

## Worst-Case Preview

| runtime_run_id | iteration_id | final_score | usable_first_response |
|---|---|---:|---:|
| `06954dd1-2512-4d1d-9d72-17e4822dc9ad` | `9b7f81ee-1914-4623-af01-fac8130436a1` | 2 | true |
| `098f0ae3-5323-47cc-9035-47fa45075776` | `28e4a969-d0b4-4d23-9293-7e0ce4e3662d` | 2 | true |
| `4fc639f2-7d29-4b64-a4f6-7ecf84874438` | `1aa7a364-8b40-4a11-9cc8-db9fa9961cc5` | 2 | true |
| `81fe7cf0-226f-49a1-8165-4060f357c3f5` | `a7a6d08b-98ad-4a71-8311-8c5e1d0521d2` | 2 | true |
| `8590b0ea-4acc-4f54-9dae-df979df02447` | `cc99049c-a6d5-4234-aaf2-b8c4a81a94c5` | 2 | true |

## Token Usage

| scope | total_tokens | total_cost_usd |
|---|---:|---:|
| runtime | 37898 | 0.000000 |
| judge_total | 26925 | 0.001597 |
| run_total | 64823 | 0.001597 |

Run total cost usd = runtime total cost usd + judge total cost usd = 0.000000 + 0.001597 = 0.001597
