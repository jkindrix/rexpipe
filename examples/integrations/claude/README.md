# rexpipe Claude API Integration

Use rexpipe with Claude's tool use feature for AI-powered text processing.

## Installation

```bash
# Install rexpipe
cargo install rexpipe

# Install Python SDK
pip install anthropic
```

## Quick Start

```python
import anthropic
from rexpipe_tools import REXPIPE_TOOLS, execute_tool

client = anthropic.Anthropic()

response = client.messages.create(
    model="claude-sonnet-4-20250514",
    max_tokens=1024,
    tools=REXPIPE_TOOLS,
    messages=[{
        "role": "user",
        "content": "Redact PII from: Contact john@example.com, SSN 123-45-6789"
    }]
)

# Handle tool use
if response.stop_reason == "tool_use":
    tool_use = next(b for b in response.content if b.type == "tool_use")
    result = execute_tool(tool_use.name, tool_use.input)
    print(result["output"])
```

## Available Tools

### `rexpipe_substitute`
Replace text matching regex patterns.

```python
execute_tool("rexpipe_substitute", {
    "text": "Hello 123 World",
    "pattern": r"\d+",
    "replacement": "NUM"
})
# Output: "Hello NUM World"
```

### `rexpipe_filter`
Filter lines by pattern matching.

```python
execute_tool("rexpipe_filter", {
    "text": "ERROR: failed\nINFO: ok\nERROR: again",
    "pattern": "ERROR",
    "keep": True
})
# Output: "ERROR: failed\nERROR: again"
```

### `rexpipe_extract`
Extract matching text.

```python
execute_tool("rexpipe_extract", {
    "text": "Emails: a@b.com, c@d.org",
    "pattern": r"[\w.]+@[\w.]+"
})
# Output: ["a@b.com", "c@d.org"]
```

### `rexpipe_transform`
Apply text transformations.

```python
execute_tool("rexpipe_transform", {
    "text": "helloWorld",
    "action": "snake_case"
})
# Output: "hello_world"
```

### `rexpipe_redact_pii`
Redact personally identifiable information.

```python
execute_tool("rexpipe_redact_pii", {
    "text": "Email: john@example.com, SSN: 123-45-6789",
    "replacement": "[HIDDEN]"
})
# Output: "Email: [HIDDEN], SSN: [HIDDEN]"
```

### `rexpipe_detect_secrets`
Scan for API keys and credentials.

```python
execute_tool("rexpipe_detect_secrets", {
    "text": "api_key = 'AKIAIOSFODNN7EXAMPLE'"
})
# Output: {"secrets_found": 1, "findings": [...]}
```

### `rexpipe_pipeline`
Execute multi-step pipelines.

```python
execute_tool("rexpipe_pipeline", {
    "text": "DEBUG: test\nERROR: failed",
    "pipeline": '''
[[steps]]
type = "filter"
pattern = "ERROR"
action = "keep_line"

[[steps]]
type = "substitute"
pattern = "ERROR"
replacement = "[ERR]"
'''
})
# Output: "[ERR]: failed"
```

## Full Conversation Example

```python
import anthropic
import json
from rexpipe_tools import REXPIPE_TOOLS, execute_tool

client = anthropic.Anthropic()

def chat_with_tools(user_message: str) -> str:
    messages = [{"role": "user", "content": user_message}]

    while True:
        response = client.messages.create(
            model="claude-sonnet-4-20250514",
            max_tokens=1024,
            tools=REXPIPE_TOOLS,
            messages=messages
        )

        if response.stop_reason == "end_turn":
            return response.content[0].text

        if response.stop_reason == "tool_use":
            tool_use = next(b for b in response.content if b.type == "tool_use")
            result = execute_tool(tool_use.name, tool_use.input)

            messages.append({"role": "assistant", "content": response.content})
            messages.append({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": tool_use.id,
                    "content": json.dumps(result)
                }]
            })

# Usage
result = chat_with_tools("""
Clean this log data:
1. Keep only ERROR lines
2. Redact any IP addresses
3. Convert to uppercase

Log:
INFO: Starting server at 192.168.1.1
ERROR: Connection failed from 10.0.0.5
DEBUG: Checking config
ERROR: Timeout at 172.16.0.1
""")
print(result)
```

## Batch Processing

For processing multiple items:

```python
def process_batch(items: list[str], operation: str) -> list[str]:
    results = []
    for item in items:
        response = client.messages.create(
            model="claude-sonnet-4-20250514",
            max_tokens=512,
            tools=REXPIPE_TOOLS,
            messages=[{
                "role": "user",
                "content": f"{operation}: {item}"
            }]
        )
        # Handle response...
        results.append(process_response(response))
    return results

# Redact PII from multiple texts
texts = ["email: a@b.com", "phone: 555-1234"]
clean_texts = process_batch(texts, "Redact all PII from")
```

## Error Handling

```python
result = execute_tool("rexpipe_substitute", {
    "text": "test",
    "pattern": "[invalid(",  # Bad regex
    "replacement": "x"
})

if not result["success"]:
    print(f"Error: {result['error']}")
    # Error will include suggestion for fixing
```

## Best Practices

### 1. Always Redact Before Logging

```python
# Before logging any user input
result = execute_tool("rexpipe_redact_pii", {"text": user_input})
logger.info(f"User input: {result['output']}")
```

### 2. Validate Tool Inputs

Claude may generate invalid regex. Handle gracefully:

```python
try:
    result = execute_tool(tool_name, tool_input)
except Exception as e:
    # Return error to Claude so it can retry
    result = {"success": False, "error": str(e)}
```

### 3. Use Pipelines for Complex Tasks

Single pipeline call is more efficient than multiple tool calls:

```python
# Instead of: filter -> substitute -> transform (3 calls)
# Use: pipeline with 3 steps (1 call)
```

## Security Notes

- rexpipe runs locally - no data sent to external services
- ReDoS-safe regex by default
- Use `--no-shell` flag to disable shell command execution
- All processing is deterministic and reproducible

## Related

- [MCP Integration](../mcp/) - For Claude Desktop and MCP-native apps
- [LangChain Integration](../langchain/) - For LangChain agents
- [AI Cookbook](../../AI_COOKBOOK.md) - Practical recipes
