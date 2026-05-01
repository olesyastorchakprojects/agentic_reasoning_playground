from __future__ import annotations

import json
import urllib.request
import uuid
from dataclasses import dataclass
from decimal import Decimal, ROUND_HALF_UP
from functools import lru_cache
from typing import Any

from psycopg import Connection
from psycopg.types.json import Json

from .judge_transport import JudgeSettings


_USD_QUANT = Decimal("0.00000001")


class JudgeUsageError(RuntimeError):
    """Raised when judge token usage or cost cannot be determined."""


@dataclass(frozen=True)
class JudgeLlmCallRecord:
    call_id: str
    request_id: str
    run_id: str
    trace_id: str
    stage_name: str
    suite_name: str | None
    chunk_id: str | None
    judge_provider: str
    judge_model: str
    judge_prompt_version: str
    token_count_source: str
    raw_response: dict[str, Any]
    prompt_tokens: int
    completion_tokens: int
    total_tokens: int
    input_cost_per_million_tokens: Decimal
    output_cost_per_million_tokens: Decimal
    total_cost_usd: Decimal


def build_judge_llm_call_record(
    *,
    request_id: str,
    run_id: str,
    trace_id: str,
    stage_name: str,
    suite_name: str | None,
    chunk_id: str | None,
    judge_prompt_version: str,
    judge_settings: JudgeSettings,
    prompt_text: str,
    raw_response: dict[str, Any],
    completion_text: str | None = None,
) -> JudgeLlmCallRecord:
    prompt_tokens: int
    completion_tokens: int
    token_count_source: str

    usage = raw_response.get("usage")
    if isinstance(usage, dict):
        prompt = usage.get("prompt_tokens")
        completion = usage.get("completion_tokens")
        if isinstance(prompt, int) and prompt >= 0 and isinstance(completion, int) and completion >= 0:
            prompt_tokens = prompt
            completion_tokens = completion
            token_count_source = "provider_usage"
        else:
            raise JudgeUsageError("response.usage must include non-negative prompt_tokens and completion_tokens")
    elif (
        isinstance(raw_response.get("prompt_eval_count"), int)
        and raw_response["prompt_eval_count"] >= 0
        and isinstance(raw_response.get("eval_count"), int)
        and raw_response["eval_count"] >= 0
    ):
        prompt_tokens = int(raw_response["prompt_eval_count"])
        completion_tokens = int(raw_response["eval_count"])
        token_count_source = "ollama_native_usage"
    else:
        if completion_text is None:
            raise JudgeUsageError(
                "local token estimate requires extracted completion text when provider usage is absent"
            )
        tokenizer = _load_tokenizer(judge_settings.tokenizer_source)
        prompt_tokens = _count_tokens(tokenizer, prompt_text)
        completion_tokens = _count_tokens(tokenizer, completion_text)
        token_count_source = "local_estimate"

    total_tokens = prompt_tokens + completion_tokens
    input_cost_per_million_tokens = Decimal(str(judge_settings.input_cost_per_million_tokens))
    output_cost_per_million_tokens = Decimal(str(judge_settings.output_cost_per_million_tokens))
    total_cost_usd = (
        (Decimal(prompt_tokens) * input_cost_per_million_tokens / Decimal("1000000"))
        + (Decimal(completion_tokens) * output_cost_per_million_tokens / Decimal("1000000"))
    ).quantize(_USD_QUANT, rounding=ROUND_HALF_UP)

    return JudgeLlmCallRecord(
        call_id=str(uuid.uuid4()),
        request_id=request_id,
        run_id=run_id,
        trace_id=trace_id,
        stage_name=stage_name,
        suite_name=suite_name,
        chunk_id=chunk_id,
        judge_provider=judge_settings.provider,
        judge_model=judge_settings.model_name,
        judge_prompt_version=judge_prompt_version,
        token_count_source=token_count_source,
        raw_response=raw_response,
        prompt_tokens=prompt_tokens,
        completion_tokens=completion_tokens,
        total_tokens=total_tokens,
        input_cost_per_million_tokens=input_cost_per_million_tokens,
        output_cost_per_million_tokens=output_cost_per_million_tokens,
        total_cost_usd=total_cost_usd,
    )


def insert_judge_llm_call(connection: Connection[Any], record: JudgeLlmCallRecord) -> None:
    connection.execute(
        """
        insert into judge_llm_calls (
            call_id,
            request_id,
            run_id,
            trace_id,
            stage_name,
            suite_name,
            chunk_id,
            judge_provider,
            judge_model,
            judge_prompt_version,
            token_count_source,
            raw_response,
            prompt_tokens,
            completion_tokens,
            total_tokens,
            input_cost_per_million_tokens,
            output_cost_per_million_tokens,
            total_cost_usd
        ) values (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
        """,
        (
            record.call_id,
            record.request_id,
            record.run_id,
            record.trace_id,
            record.stage_name,
            record.suite_name,
            record.chunk_id,
            record.judge_provider,
            record.judge_model,
            record.judge_prompt_version,
            record.token_count_source,
            Json(record.raw_response),
            record.prompt_tokens,
            record.completion_tokens,
            record.total_tokens,
            record.input_cost_per_million_tokens,
            record.output_cost_per_million_tokens,
            record.total_cost_usd,
        ),
    )


def tokenizer_url(repo_id: str) -> str:
    return f"https://huggingface.co/{repo_id}/resolve/main/tokenizer.json"


@lru_cache(maxsize=8)
def _load_tokenizer(repo_id: str) -> Any:
    try:
        from tokenizers import Tokenizer
    except ModuleNotFoundError as exc:  # pragma: no cover - environment boundary
        raise JudgeUsageError(f"missing required dependency 'tokenizers': {exc}") from exc

    url = tokenizer_url(repo_id)
    try:
        with urllib.request.urlopen(url, timeout=10) as response:
            payload = response.read()
    except Exception as exc:  # pragma: no cover - network boundary
        raise JudgeUsageError(f"failed to download tokenizer artifact from {url}: {exc}") from exc

    try:
        json.loads(payload.decode("utf-8"))
    except Exception as exc:  # pragma: no cover - defensive boundary
        raise JudgeUsageError(f"tokenizer artifact from {url} is not valid json: {exc}") from exc

    try:
        return Tokenizer.from_str(payload.decode("utf-8"))
    except Exception as exc:  # pragma: no cover - parser boundary
        raise JudgeUsageError(f"failed to construct tokenizer from {url}: {exc}") from exc


def _count_tokens(tokenizer: Any, text: str) -> int:
    try:
        encoding = tokenizer.encode(text)
    except Exception as exc:  # pragma: no cover - tokenizer boundary
        raise JudgeUsageError(f"failed to encode text with judge tokenizer: {exc}") from exc
    return len(encoding.ids)
