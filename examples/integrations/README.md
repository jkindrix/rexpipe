# rexpipe AI Integrations

Integration guides for using rexpipe with AI agent frameworks.

## Available Integrations

| Framework | Directory | Description |
|-----------|-----------|-------------|
| [MCP](./mcp/) | Model Context Protocol | Claude Desktop, ChatGPT, Gemini, OpenAI Agents SDK |
| [LangChain](./langchain/) | LangChain Tools | Python agents with LangChain/LangGraph |
| [Claude API](./claude/) | Anthropic Tool Use | Direct Claude API integration |

## Quick Comparison

| Feature | MCP | LangChain | Claude API |
|---------|-----|-----------|------------|
| Setup complexity | Low | Medium | Low |
| Language | Any (stdio) | Python | Python |
| Best for | Desktop apps | Complex agents | Simple tools |
| Streaming | Yes | Yes | Yes |
| Multi-model | Yes | Yes | Claude only |

## Which Should I Use?

### Use MCP if:
- You're using Claude Desktop, ChatGPT, or Gemini
- You want plug-and-play integration
- You need cross-platform support

### Use LangChain if:
- You're building custom Python agents
- You need to combine with other LangChain tools
- You want LangGraph workflow integration

### Use Claude API if:
- You're integrating directly with Anthropic's API
- You want minimal dependencies
- You need fine-grained control

## Common Patterns

### Redact PII Before Processing

All integrations provide PII redaction. Use it before logging or storing user data:

```python
# LangChain
from rexpipe_tools import RexpipeRedactPII
safe_text = RexpipeRedactPII()._run(user_input)

# Claude API
result = execute_tool("rexpipe_redact_pii", {"text": user_input})
safe_text = result["output"]
```

### Multi-Step Pipelines

For complex transformations, use the pipeline tool:

```python
pipeline = '''
[[steps]]
type = "filter"
pattern = "ERROR"
action = "keep_line"

[[steps]]
type = "substitute"
pattern = "\\d{1,3}\\.\\d{1,3}\\.\\d{1,3}\\.\\d{1,3}"
replacement = "[IP]"
'''
```

### Pattern Libraries

Reference pre-tested patterns instead of writing regex:

```python
# Use patterns from the AI library
pipeline = '''
patterns_include = ["patterns/ai.toml"]

[[steps]]
type = "substitute"
pattern = "${pii.email}"
replacement = "[EMAIL]"
'''
```

## Installation

1. Install rexpipe:
   ```bash
   cargo install rexpipe
   # or download from releases
   ```

2. Install framework-specific dependencies:
   ```bash
   # For LangChain
   pip install langchain langchain-openai

   # For Claude API
   pip install anthropic

   # For MCP (no Python needed)
   # Just configure your MCP client
   ```

3. Copy the integration files to your project.

## Support

- [rexpipe Documentation](https://github.com/jkindrix/rexpipe)
- [AI Cookbook](../AI_COOKBOOK.md) - Practical recipes
- [Pattern Library](../patterns/INDEX.md) - Available patterns
