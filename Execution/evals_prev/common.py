from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class SuitePrompt:
    suite_name: str
    prompt_id: str
    version: str
    prompt_template: str
    input_variables: tuple[str, ...]


def project_root() -> Path:
    return Path(__file__).resolve().parents[2]


def prompts_json_path() -> Path:
    return project_root() / "Specification" / "codegen" / "evals" / "prompts.json"


def load_prompt_catalog() -> dict[str, Any]:
    with prompts_json_path().open("r", encoding="utf-8") as handle:
        return json.load(handle)


def load_generation_suite_prompts() -> dict[str, SuitePrompt]:
    catalog = load_prompt_catalog()
    suites = catalog["generation_suites"]
    return {
        suite_name: SuitePrompt(
            suite_name=suite_name,
            prompt_id=entry["id"],
            version=entry["version"],
            prompt_template=entry["prompt_template"],
            input_variables=tuple(entry["input_variables"]),
        )
        for suite_name, entry in suites.items()
    }


def load_retrieval_suite_prompts() -> dict[str, SuitePrompt]:
    catalog = load_prompt_catalog()
    suites = catalog["retrieval_suites"]
    return {
        suite_name: SuitePrompt(
            suite_name=suite_name,
            prompt_id=entry["id"],
            version=entry["version"],
            prompt_template=entry["prompt_template"],
            input_variables=tuple(entry["input_variables"]),
        )
        for suite_name, entry in suites.items()
    }


def render_prompt_template(prompt_template: str, variables: dict[str, str]) -> str:
    rendered = prompt_template
    for key, value in variables.items():
        rendered = rendered.replace(f"{{{{{key}}}}}", value)
    if "{{" in rendered or "}}" in rendered:
        raise ValueError("prompt template contains unresolved placeholders")
    return rendered


def load_chunk_index(chunks_path: str) -> dict[str, dict[str, Any]]:
    chunk_index: dict[str, dict[str, Any]] = {}
    with Path(chunks_path).open("r", encoding="utf-8") as handle:
        for line in handle:
            record = json.loads(line)
            chunk_index[record["chunk_id"]] = record
    return chunk_index


def ensure_json_object(value: Any) -> dict[str, Any]:
    if isinstance(value, dict):
        return value
    if isinstance(value, str):
        parsed = json.loads(value)
        if isinstance(parsed, dict):
            return parsed
    raise ValueError("expected JSON object")


def ensure_json_array(value: Any) -> list[Any]:
    if isinstance(value, list):
        return value
    if isinstance(value, str):
        parsed = json.loads(value)
        if isinstance(parsed, list):
            return parsed
    raise ValueError("expected JSON array")


def extract_text_from_openai_compatible_response(raw_response: dict[str, Any]) -> str:
    choices = raw_response.get("choices")
    if isinstance(choices, list) and choices:
        first_choice = ensure_json_object(choices[0])
        message = first_choice.get("message")
        if isinstance(message, dict):
            for field_name in (
                "content",
                "output_text",
                "text",
                "reasoning_content",
                "refusal",
            ):
                content_text = _extract_text_from_content_field(message.get(field_name))
                if content_text:
                    return content_text
        text = first_choice.get("text")
        if isinstance(text, str) and text.strip():
            return text.strip()
        for field_name in ("content", "output_text"):
            content_text = _extract_text_from_content_field(first_choice.get(field_name))
            if content_text:
                return content_text
        delta = first_choice.get("delta")
        if isinstance(delta, dict):
            for field_name in ("content", "text"):
                content_text = _extract_text_from_content_field(delta.get(field_name))
                if content_text:
                    return content_text

    output_text = raw_response.get("output_text")
    if isinstance(output_text, str) and output_text.strip():
        return output_text.strip()

    output = raw_response.get("output")
    if isinstance(output, list):
        text_parts: list[str] = []
        for item in output:
            if not isinstance(item, dict):
                continue
            content_text = _extract_text_from_content_field(item.get("content"))
            if content_text:
                text_parts.append(content_text)
        joined = "\n".join(part for part in text_parts if part.strip()).strip()
        if joined:
            return joined

    raise ValueError(
        "response missing extractable text content"
    )


def parse_json_object_from_model_text(content: str) -> dict[str, Any]:
    text = content.strip()
    candidates = [text]

    if text.startswith("```"):
        fence_lines = text.splitlines()
        if len(fence_lines) >= 3 and fence_lines[-1].strip() == "```":
            inner = "\n".join(fence_lines[1:-1]).strip()
            candidates.append(inner)

    repaired_candidates: list[str] = []
    for candidate in candidates:
        open_braces = candidate.count("{")
        close_braces = candidate.count("}")
        if open_braces > close_braces and candidate.lstrip().startswith("{"):
            repaired_candidates.append(candidate + ("}" * (open_braces - close_braces)))
    candidates.extend(repaired_candidates)

    for candidate in candidates:
        try:
            parsed = json.loads(candidate)
        except json.JSONDecodeError:
            continue
        if isinstance(parsed, dict):
            return parsed

    repaired_candidates = [_repair_unescaped_quotes(candidate) for candidate in candidates]
    for candidate in repaired_candidates:
        try:
            parsed = json.loads(candidate)
        except json.JSONDecodeError:
            continue
        if isinstance(parsed, dict):
            return parsed

    raise ValueError("model content does not contain a valid JSON object")


def _extract_text_from_content_field(content: Any) -> str:
    if isinstance(content, str) and content.strip():
        return content.strip()
    if isinstance(content, list):
        text_parts: list[str] = []
        for item in content:
            if not isinstance(item, dict):
                continue
            for field_name in ("text", "content", "output_text"):
                text = _extract_text_from_content_field(item.get(field_name))
                if text:
                    text_parts.append(text)
                    break
        joined = "\n".join(text_parts).strip()
        if joined:
            return joined
    if isinstance(content, dict):
        for field_name in ("text", "value", "content", "output_text"):
            text = _extract_text_from_content_field(content.get(field_name))
            if text:
                return text
    return ""


def _repair_unescaped_quotes(text: str) -> str:
    repaired: list[str] = []
    in_string = False
    escaped = False
    string_role = "key"
    object_state = "expect_key"

    for index, char in enumerate(text):
        if not in_string:
            repaired.append(char)
            if char == '"':
                in_string = True
                string_role = "value" if object_state == "expect_value" else "key"
            continue

        if escaped:
            repaired.append(char)
            escaped = False
            continue

        if char == "\\":
            repaired.append(char)
            escaped = True
            continue

        if char == '"':
            next_non_ws = index + 1
            while next_non_ws < len(text) and text[next_non_ws].isspace():
                next_non_ws += 1
            if string_role == "key":
                should_close = next_non_ws < len(text) and text[next_non_ws] == ":"
            else:
                should_close = next_non_ws == len(text) or text[next_non_ws] in {",", "}", "]"}
            if should_close:
                repaired.append(char)
                in_string = False
                object_state = "expect_value" if string_role == "key" else "after_value"
            else:
                repaired.append("\\")
                repaired.append('"')
            continue

        repaired.append(char)
        if char == "{":
            object_state = "expect_key"
        elif char == ":" and not in_string:
            object_state = "expect_value"
        elif char == "," and not in_string:
            object_state = "expect_key"

    return "".join(repaired)
