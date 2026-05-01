from __future__ import annotations

import os
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Mapping


class JudgeConfigError(RuntimeError):
    """Raised when judge transport configuration is invalid."""


@dataclass(frozen=True)
class JudgeSettings:
    provider: str
    model_name: str
    tokenizer_source: str
    base_url: str
    api_key: str
    timeout_sec: int
    input_cost_per_million_tokens: float
    output_cost_per_million_tokens: float


def load_judge_settings(
    eval_config_path: Path,
    env: Mapping[str, str] | None = None,
) -> JudgeSettings:
    env_values = env or os.environ
    eval_data = _load_toml(eval_config_path)
    judge = _require_dict(eval_data.get("judge"), "judge")

    provider = _require_non_empty_string(judge.get("provider"), "judge.provider")
    model_name = _require_non_empty_string(judge.get("model_name"), "judge.model_name")
    tokenizer_source = _require_non_empty_string(
        judge.get("tokenizer_source"),
        "judge.tokenizer_source",
    )
    input_cost_per_million_tokens = _require_non_negative_number(
        judge.get("input_cost_per_million_tokens"),
        "judge.input_cost_per_million_tokens",
    )
    output_cost_per_million_tokens = _require_non_negative_number(
        judge.get("output_cost_per_million_tokens"),
        "judge.output_cost_per_million_tokens",
    )

    if provider == "ollama":
        ollama = _require_dict(judge.get("ollama"), "judge.ollama")
        timeout_sec = _require_positive_int(ollama.get("timeout_sec"), "judge.ollama.timeout_sec")
        return JudgeSettings(
            provider="ollama",
            model_name=model_name,
            tokenizer_source=tokenizer_source,
            base_url=_normalize_openai_base_url(_require_env(env_values, "OLLAMA_URL")),
            api_key="unused",
            timeout_sec=timeout_sec,
            input_cost_per_million_tokens=input_cost_per_million_tokens,
            output_cost_per_million_tokens=output_cost_per_million_tokens,
        )

    if provider == "together":
        together = _require_dict(judge.get("together"), "judge.together")
        timeout_sec = _require_positive_int(
            together.get("timeout_sec"), "judge.together.timeout_sec"
        )
        return JudgeSettings(
            provider="together",
            model_name=model_name,
            tokenizer_source=tokenizer_source,
            base_url=_normalize_openai_base_url(
                _require_env(env_values, "OPENAI_COMPATIBLE_URL")
            ),
            api_key=_require_env(env_values, "TOGETHER_API_KEY"),
            timeout_sec=timeout_sec,
            input_cost_per_million_tokens=input_cost_per_million_tokens,
            output_cost_per_million_tokens=output_cost_per_million_tokens,
        )

    raise JudgeConfigError("judge.provider must be either 'ollama' or 'together'")


def build_judge_client(settings: JudgeSettings) -> Any:
    from openai import OpenAI

    return OpenAI(
        base_url=settings.base_url,
        api_key=settings.api_key,
        timeout=settings.timeout_sec,
    )


def create_chat_completion_with_retry(
    client: Any,
    settings: JudgeSettings,
    *,
    messages: list[dict[str, str]],
    temperature: float = 0.0,
) -> Any:
    last_error: Exception | None = None
    max_attempts = 3

    for attempt in range(1, max_attempts + 1):
        try:
            return client.chat.completions.create(
                model=settings.model_name,
                temperature=temperature,
                messages=messages,
            )
        except Exception as error:  # pragma: no cover - exercised via tests/fake clients
            last_error = error
            if attempt >= max_attempts or not _is_retryable_judge_error(error):
                raise
            time.sleep(_retry_delay_seconds(attempt))

    if last_error is not None:  # pragma: no cover - defensive
        raise last_error
    raise RuntimeError("judge completion retry loop exited without response or error")


def _retry_delay_seconds(attempt: int) -> float:
    return float(2 ** (attempt - 1))


def _is_retryable_judge_error(error: Exception) -> bool:
    status_code = getattr(error, "status_code", None)
    if isinstance(status_code, int):
        return status_code == 429 or status_code >= 500

    error_name = type(error).__name__
    if error_name in {"APIConnectionError", "APITimeoutError", "RateLimitError", "InternalServerError"}:
        return True

    return False


def _normalize_openai_base_url(url: str) -> str:
    normalized = url.strip().rstrip("/")
    if normalized.endswith("/v1"):
        return normalized
    return normalized + "/v1"


def _require_env(values: Mapping[str, str], key: str) -> str:
    value = values.get(key)
    if value is None or not value.strip():
        raise JudgeConfigError(f"missing required environment variable {key}")
    return value.strip().strip('"').strip("'")


def _load_toml(path: Path) -> dict[str, Any]:
    try:
        import tomllib as parser  # type: ignore
    except ModuleNotFoundError:
        import tomli as parser  # type: ignore
    with path.open("rb") as handle:
        data = parser.load(handle)
    if not isinstance(data, dict):
        raise JudgeConfigError(f"TOML root must be an object: {path}")
    return data


def _require_dict(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise JudgeConfigError(f"{label} must be a table/object")
    return value


def _require_non_empty_string(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise JudgeConfigError(f"{label} must be a non-empty string")
    return value.strip()


def _require_positive_int(value: Any, label: str) -> int:
    if not isinstance(value, int) or value < 1:
        raise JudgeConfigError(f"{label} must be an integer >= 1")
    return value


def _require_non_negative_number(value: Any, label: str) -> float:
    if not isinstance(value, (int, float)) or value < 0:
        raise JudgeConfigError(f"{label} must be a number >= 0")
    return float(value)
