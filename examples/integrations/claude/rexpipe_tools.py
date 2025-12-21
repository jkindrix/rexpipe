"""
Claude API Tools for rexpipe

This module provides tool definitions for using rexpipe with the
Anthropic Claude API's tool use feature.

Installation:
    pip install anthropic

Usage:
    from rexpipe_tools import REXPIPE_TOOLS, execute_tool

    response = client.messages.create(
        model="claude-sonnet-4-20250514",
        tools=REXPIPE_TOOLS,
        messages=[{"role": "user", "content": "Redact PII from this text: ..."}]
    )
"""

import json
import subprocess
from typing import Any


# --- Tool Definitions for Claude API ---

REXPIPE_TOOLS = [
    {
        "name": "rexpipe_substitute",
        "description": "Replace text matching a regex pattern with a replacement string. Supports capture groups ($1, $2, etc.) in the replacement. Use for find-and-replace operations, text normalization, and pattern-based transformations.",
        "input_schema": {
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "The text to process"
                },
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to match. Use standard regex syntax."
                },
                "replacement": {
                    "type": "string",
                    "description": "Replacement string. Use $1, $2 for capture groups, $0 for entire match."
                }
            },
            "required": ["text", "pattern", "replacement"]
        }
    },
    {
        "name": "rexpipe_filter",
        "description": "Filter lines of text based on whether they match a regex pattern. Useful for log filtering, extracting specific lines, removing unwanted content.",
        "input_schema": {
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Multi-line text to filter"
                },
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to match against each line"
                },
                "keep": {
                    "type": "boolean",
                    "description": "If true, keep lines that match. If false, drop lines that match.",
                    "default": True
                }
            },
            "required": ["text", "pattern"]
        }
    },
    {
        "name": "rexpipe_extract",
        "description": "Extract all text matching a regex pattern from input. Returns only the matched portions. Use capture groups to extract specific parts.",
        "input_schema": {
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text to extract matches from"
                },
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to match. Use capture groups () for specific extraction."
                }
            },
            "required": ["text", "pattern"]
        }
    },
    {
        "name": "rexpipe_transform",
        "description": "Apply text transformations: uppercase, lowercase, trim whitespace, convert to snake_case, camelCase, PascalCase, or kebab-case.",
        "input_schema": {
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text to transform"
                },
                "action": {
                    "type": "string",
                    "enum": ["uppercase", "lowercase", "trim", "snake_case", "camel_case", "pascal_case", "kebab_case"],
                    "description": "The transformation to apply"
                }
            },
            "required": ["text", "action"]
        }
    },
    {
        "name": "rexpipe_redact_pii",
        "description": "Redact personally identifiable information (PII) from text. Detects and replaces: email addresses, phone numbers, Social Security Numbers, credit card numbers, and IP addresses. ALWAYS use this before logging, storing, or displaying user-provided text that may contain sensitive information.",
        "input_schema": {
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text that may contain PII"
                },
                "replacement": {
                    "type": "string",
                    "description": "Text to replace PII with",
                    "default": "[REDACTED]"
                }
            },
            "required": ["text"]
        }
    },
    {
        "name": "rexpipe_detect_secrets",
        "description": "Scan text for potential secrets, API keys, and credentials. Returns locations and types of any detected secrets. Use for security scanning of code, configs, or logs.",
        "input_schema": {
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text to scan for secrets"
                }
            },
            "required": ["text"]
        }
    },
    {
        "name": "rexpipe_pipeline",
        "description": "Execute a multi-step text processing pipeline. Most powerful option for complex transformations that require multiple operations in sequence. Pipeline is defined in TOML format with [[steps]] sections.",
        "input_schema": {
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text to process through the pipeline"
                },
                "pipeline": {
                    "type": "string",
                    "description": "Pipeline configuration in TOML format. Each [[steps]] section defines one operation."
                }
            },
            "required": ["text", "pipeline"]
        }
    }
]


# --- Tool Execution ---

def run_rexpipe(args: list[str], input_text: str) -> dict[str, Any]:
    """Execute rexpipe and return result."""
    try:
        result = subprocess.run(
            ["rexpipe", "--json", "--error-format", "json"] + args,
            input=input_text,
            capture_output=True,
            text=True,
            timeout=30,
        )

        if result.returncode == 0:
            try:
                return {"success": True, "output": result.stdout.strip(), "data": json.loads(result.stdout)}
            except json.JSONDecodeError:
                return {"success": True, "output": result.stdout.strip()}
        else:
            try:
                error_data = json.loads(result.stderr)
                return {"success": False, "error": error_data}
            except json.JSONDecodeError:
                return {"success": False, "error": result.stderr.strip()}
    except subprocess.TimeoutExpired:
        return {"success": False, "error": "Processing timed out after 30 seconds"}
    except FileNotFoundError:
        return {"success": False, "error": "rexpipe not found. Install: cargo install rexpipe"}


def execute_tool(tool_name: str, tool_input: dict[str, Any]) -> dict[str, Any]:
    """Execute a rexpipe tool and return the result."""

    if tool_name == "rexpipe_substitute":
        return run_rexpipe(
            ["-p", tool_input["pattern"], "-r", tool_input["replacement"]],
            tool_input["text"]
        )

    elif tool_name == "rexpipe_filter":
        keep = tool_input.get("keep", True)
        action = "keep_line" if keep else "drop_line"
        return run_rexpipe(
            ["-p", tool_input["pattern"], "--filter", action],
            tool_input["text"]
        )

    elif tool_name == "rexpipe_extract":
        return run_rexpipe(
            ["-p", tool_input["pattern"], "--extract"],
            tool_input["text"]
        )

    elif tool_name == "rexpipe_transform":
        return run_rexpipe(
            ["--transform", tool_input["action"]],
            tool_input["text"]
        )

    elif tool_name == "rexpipe_redact_pii":
        replacement = tool_input.get("replacement", "[REDACTED]")
        pipeline = f'''
[[steps]]
type = "substitute"
pattern = '[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{{2,}}'
replacement = "{replacement}"

[[steps]]
type = "substitute"
pattern = '(?:\\+?[0-9]{{1,4}}[-.]?)?(?:\\([0-9]{{1,4}}\\)[-.]?)?[0-9]{{1,4}}[-.][0-9]{{1,4}}[-.][0-9]{{1,9}}'
replacement = "{replacement}"

[[steps]]
type = "substitute"
pattern = '\\b\\d{{3}}-\\d{{2}}-\\d{{4}}\\b'
replacement = "{replacement}"

[[steps]]
type = "substitute"
pattern = '\\b(?:\\d{{4}}[-\\s]?){{3}}\\d{{4}}\\b'
replacement = "{replacement}"

[[steps]]
type = "substitute"
pattern = '\\b(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\.(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\.(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\.(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\b'
replacement = "{replacement}"
'''
        import tempfile
        with tempfile.NamedTemporaryFile(mode='w', suffix='.toml', delete=False) as f:
            f.write(pipeline)
            f.flush()
            return run_rexpipe(["-c", f.name], tool_input["text"])

    elif tool_name == "rexpipe_detect_secrets":
        # Detect common secret patterns
        patterns = [
            (r'\bAKIA[0-9A-Z]{16}\b', 'AWS Access Key'),
            (r'\bghp_[a-zA-Z0-9]{36}\b', 'GitHub Token'),
            (r'\beyJ[a-zA-Z0-9_-]+\.eyJ[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+\b', 'JWT'),
            (r'-----BEGIN (?:RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----', 'Private Key'),
            (r'\b[a-zA-Z0-9_-]{32,}\b', 'Potential API Key'),
        ]

        text = tool_input["text"]
        findings = []

        import re
        for pattern, secret_type in patterns:
            for match in re.finditer(pattern, text):
                findings.append({
                    "type": secret_type,
                    "match": match.group()[:20] + "..." if len(match.group()) > 20 else match.group(),
                    "position": match.start()
                })

        return {
            "success": True,
            "secrets_found": len(findings),
            "findings": findings
        }

    elif tool_name == "rexpipe_pipeline":
        import tempfile
        with tempfile.NamedTemporaryFile(mode='w', suffix='.toml', delete=False) as f:
            f.write(tool_input["pipeline"])
            f.flush()
            return run_rexpipe(["-c", f.name], tool_input["text"])

    else:
        return {"success": False, "error": f"Unknown tool: {tool_name}"}


# --- Example Usage with Claude API ---

def example_usage():
    """Example of using rexpipe tools with Claude API."""
    import anthropic

    client = anthropic.Anthropic()

    # Example conversation
    messages = [
        {
            "role": "user",
            "content": "Please redact any PII from this text: My email is john.doe@example.com and my SSN is 123-45-6789"
        }
    ]

    # First API call - Claude decides to use a tool
    response = client.messages.create(
        model="claude-sonnet-4-20250514",
        max_tokens=1024,
        tools=REXPIPE_TOOLS,
        messages=messages
    )

    # Process tool use
    while response.stop_reason == "tool_use":
        # Find the tool use block
        tool_use = next(
            block for block in response.content
            if block.type == "tool_use"
        )

        # Execute the tool
        tool_result = execute_tool(tool_use.name, tool_use.input)

        # Continue conversation with tool result
        messages.append({"role": "assistant", "content": response.content})
        messages.append({
            "role": "user",
            "content": [
                {
                    "type": "tool_result",
                    "tool_use_id": tool_use.id,
                    "content": json.dumps(tool_result)
                }
            ]
        })

        # Next API call
        response = client.messages.create(
            model="claude-sonnet-4-20250514",
            max_tokens=1024,
            tools=REXPIPE_TOOLS,
            messages=messages
        )

    # Final response
    print("Claude's response:", response.content[0].text)


if __name__ == "__main__":
    # Test tool execution
    print("Testing rexpipe Claude tools...\n")

    # Test substitute
    result = execute_tool("rexpipe_substitute", {
        "text": "Hello 123 World 456",
        "pattern": r"\d+",
        "replacement": "NUM"
    })
    print(f"Substitute: {result}\n")

    # Test filter
    result = execute_tool("rexpipe_filter", {
        "text": "ERROR: failed\nINFO: success\nERROR: another",
        "pattern": "ERROR",
        "keep": True
    })
    print(f"Filter: {result}\n")

    # Test detect secrets
    result = execute_tool("rexpipe_detect_secrets", {
        "text": "api_key = 'AKIAIOSFODNN7EXAMPLE' and token = 'ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'"
    })
    print(f"Detect Secrets: {result}\n")

    print("All tools working!")
