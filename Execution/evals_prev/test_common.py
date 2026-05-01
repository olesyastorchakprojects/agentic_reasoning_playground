from __future__ import annotations

import unittest

from Execution.evals.common import (
    extract_text_from_openai_compatible_response,
    parse_json_object_from_model_text,
)


class ParseJsonObjectFromModelTextTests(unittest.TestCase):
    def test_parses_valid_json_object(self) -> None:
        parsed = parse_json_object_from_model_text(
            '{"label":"correct_refusal","explanation":"short explanation"}'
        )

        self.assertEqual(parsed["label"], "correct_refusal")
        self.assertEqual(parsed["explanation"], "short explanation")

    def test_repairs_unescaped_quotes_inside_string_values(self) -> None:
        parsed = parse_json_object_from_model_text(
            '{"label": "correct_refusal", "explanation": "The answer correctly identifies TCP as a layering mechanism that creates a reliable communication channel over an unreliable IP network and explains how it compensates for packet loss, duplicates, and out-of-order delivery using acknowledgments, checksums, retransmission timers, and segment numbering. This is appropriate behavior for the question about why TCP is considered a "layering mechanism" and what mechanisms it uses to ensure reliability."}'
        )

        self.assertEqual(parsed["label"], "correct_refusal")
        self.assertIn('a "layering mechanism"', parsed["explanation"])
        self.assertTrue(parsed["explanation"].endswith("reliability."))


class ExtractTextFromOpenAiCompatibleResponseTests(unittest.TestCase):
    def test_extracts_message_content_string(self) -> None:
        raw_response = {
            "choices": [
                {
                    "message": {
                        "content": '{"label":"grounded","explanation":"ok"}',
                    }
                }
            ]
        }

        content = extract_text_from_openai_compatible_response(raw_response)

        self.assertEqual(content, '{"label":"grounded","explanation":"ok"}')

    def test_extracts_message_content_array(self) -> None:
        raw_response = {
            "choices": [
                {
                    "message": {
                        "content": [
                            {"type": "output_text", "text": '{"label":"grounded","explanation":"ok"}'}
                        ],
                    }
                }
            ]
        }

        content = extract_text_from_openai_compatible_response(raw_response)

        self.assertEqual(content, '{"label":"grounded","explanation":"ok"}')

    def test_extracts_choice_text_fallback(self) -> None:
        raw_response = {
            "choices": [
                {
                    "text": '{"label":"relevant","explanation":"ok"}',
                }
            ]
        }

        content = extract_text_from_openai_compatible_response(raw_response)

        self.assertEqual(content, '{"label":"relevant","explanation":"ok"}')

    def test_extracts_output_text_fallback(self) -> None:
        raw_response = {
            "output_text": '{"label":"relevant","explanation":"ok"}',
        }

        content = extract_text_from_openai_compatible_response(raw_response)

        self.assertEqual(content, '{"label":"relevant","explanation":"ok"}')

    def test_extracts_nested_text_value_from_message_content_array(self) -> None:
        raw_response = {
            "choices": [
                {
                    "message": {
                        "content": [
                            {
                                "type": "text",
                                "text": {"value": '{"label":"grounded","explanation":"ok"}'},
                            }
                        ]
                    }
                }
            ]
        }

        content = extract_text_from_openai_compatible_response(raw_response)

        self.assertEqual(content, '{"label":"grounded","explanation":"ok"}')

    def test_extracts_message_output_text_when_content_is_empty(self) -> None:
        raw_response = {
            "choices": [
                {
                    "message": {
                        "content": None,
                        "output_text": '{"label":"grounded","explanation":"ok"}',
                    }
                }
            ]
        }

        content = extract_text_from_openai_compatible_response(raw_response)

        self.assertEqual(content, '{"label":"grounded","explanation":"ok"}')


if __name__ == "__main__":
    unittest.main()
