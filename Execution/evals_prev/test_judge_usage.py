from __future__ import annotations

from decimal import Decimal
from unittest.mock import MagicMock, patch

import pytest
from psycopg.types.json import Json

from .judge_transport import JudgeSettings
from . import judge_usage
from .judge_usage import build_judge_llm_call_record, insert_judge_llm_call


def _judge_settings() -> JudgeSettings:
    return JudgeSettings(
        provider="together",
        model_name="openai/gpt-oss-20b",
        tokenizer_source="Qwen/Qwen2.5-1.5B-Instruct",
        base_url="https://api.together.xyz/v1",
        api_key="test-key",
        timeout_sec=30,
        input_cost_per_million_tokens=0.05,
        output_cost_per_million_tokens=0.20,
    )


def test_build_record_uses_provider_usage_when_available() -> None:
    record = build_judge_llm_call_record(
        request_id="req-1",
        run_id="run-1",
        trace_id="trace-1",
        stage_name="judge_generation",
        suite_name="groundedness",
        chunk_id=None,
        judge_prompt_version="v1",
        judge_settings=_judge_settings(),
        prompt_text="prompt",
        raw_response={
            "usage": {"prompt_tokens": 200, "completion_tokens": 50},
            "choices": [{"message": {"content": '{"label":"grounded","explanation":"ok"}'}}],
        },
        completion_text='{"label":"grounded","explanation":"ok"}',
    )

    assert record.token_count_source == "provider_usage"
    assert record.raw_response["usage"]["prompt_tokens"] == 200
    assert record.prompt_tokens == 200
    assert record.completion_tokens == 50
    assert record.total_tokens == 250
    assert record.total_cost_usd == Decimal("0.00002000")


def test_build_record_uses_provider_usage_without_completion_text() -> None:
    record = build_judge_llm_call_record(
        request_id="req-1",
        run_id="run-1",
        trace_id="trace-1",
        stage_name="judge_generation",
        suite_name="groundedness",
        chunk_id=None,
        judge_prompt_version="v1",
        judge_settings=_judge_settings(),
        prompt_text="prompt",
        raw_response={"usage": {"prompt_tokens": 200, "completion_tokens": 50}},
        completion_text=None,
    )

    assert record.token_count_source == "provider_usage"
    assert record.raw_response["usage"]["completion_tokens"] == 50
    assert record.prompt_tokens == 200
    assert record.completion_tokens == 50


def test_build_record_uses_ollama_native_usage_when_available() -> None:
    settings = _judge_settings()
    settings = JudgeSettings(
        provider="ollama",
        model_name=settings.model_name,
        tokenizer_source=settings.tokenizer_source,
        base_url="http://localhost:11434/v1",
        api_key="unused",
        timeout_sec=30,
        input_cost_per_million_tokens=0.0,
        output_cost_per_million_tokens=0.0,
    )
    record = build_judge_llm_call_record(
        request_id="req-1",
        run_id="run-1",
        trace_id="trace-1",
        stage_name="judge_retrieval",
        suite_name="retrieval_relevance",
        chunk_id="chunk-1",
        judge_prompt_version="v1",
        judge_settings=settings,
        prompt_text="prompt",
        raw_response={
            "prompt_eval_count": 120,
            "eval_count": 30,
            "choices": [{"message": {"content": '{"label":"relevant","explanation":"ok"}'}}],
        },
        completion_text='{"label":"relevant","explanation":"ok"}',
    )

    assert record.token_count_source == "ollama_native_usage"
    assert record.raw_response["prompt_eval_count"] == 120
    assert record.prompt_tokens == 120
    assert record.completion_tokens == 30
    assert record.total_tokens == 150
    assert record.total_cost_usd == Decimal("0E-8")


def test_build_record_falls_back_to_local_estimate() -> None:
    with patch.object(judge_usage, "_load_tokenizer", return_value="tokenizer"), patch.object(
        judge_usage,
        "_count_tokens",
        side_effect=[17, 5],
    ):
        record = build_judge_llm_call_record(
            request_id="req-1",
            run_id="run-1",
            trace_id="trace-1",
            stage_name="judge_generation",
            suite_name="answer_relevance",
            chunk_id=None,
            judge_prompt_version="v1",
            judge_settings=_judge_settings(),
            prompt_text="prompt",
            raw_response={"choices": [{"message": {"content": '{"label":"relevant","explanation":"ok"}'}}]},
            completion_text='{"label":"relevant","explanation":"ok"}',
        )

    assert record.token_count_source == "local_estimate"
    assert record.raw_response["choices"][0]["message"]["content"] == '{"label":"relevant","explanation":"ok"}'
    assert record.prompt_tokens == 17
    assert record.completion_tokens == 5
    assert record.total_tokens == 22


def test_insert_judge_llm_call_executes_insert() -> None:
    connection = MagicMock()
    record = build_judge_llm_call_record(
        request_id="req-1",
        run_id="run-1",
        trace_id="trace-1",
        stage_name="judge_generation",
        suite_name="groundedness",
        chunk_id=None,
        judge_prompt_version="v1",
        judge_settings=_judge_settings(),
        prompt_text="prompt",
        raw_response={
            "usage": {"prompt_tokens": 10, "completion_tokens": 2},
            "choices": [{"message": {"content": '{"label":"grounded","explanation":"ok"}'}}],
        },
        completion_text='{"label":"grounded","explanation":"ok"}',
    )

    insert_judge_llm_call(connection, record)

    assert connection.execute.call_count == 1
    sql = connection.execute.call_args.args[0]
    params = connection.execute.call_args.args[1]
    assert "insert into judge_llm_calls" in sql
    assert params[0] == record.call_id
    assert params[1] == "req-1"
    assert isinstance(params[11], Json)
