# rexpipe MCP Integration

Integration with [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) for AI agent frameworks.

## Overview

MCP is the open standard for connecting AI applications to external tools. This integration allows Claude, ChatGPT, Gemini, and other MCP-compatible AI systems to use rexpipe for text processing.

## Quick Start

### Option 1: stdio Transport (Recommended)

Add rexpipe as an MCP server in your client configuration:

```json
{
  "mcpServers": {
    "rexpipe": {
      "command": "rexpipe",
      "args": ["--mcp"],
      "env": {}
    }
  }
}
```

### Option 2: HTTP Transport

Run rexpipe as an HTTP server:

```bash
rexpipe --server --bind 127.0.0.1:8080
```

Configure your MCP client to connect via HTTP.

## Available Tools

| Tool | Description |
|------|-------------|
| `rexpipe_substitute` | Replace text matching regex patterns |
| `rexpipe_filter` | Keep or drop lines matching patterns |
| `rexpipe_extract` | Extract text matching patterns |
| `rexpipe_validate` | Validate text against patterns |
| `rexpipe_transform` | Apply text transformations |
| `rexpipe_pipeline` | Execute multi-step pipelines |
| `rexpipe_redact_pii` | Redact PII (emails, SSNs, etc.) |
| `rexpipe_detect_secrets` | Find API keys and credentials |

## Example Usage

### Redact PII

```json
{
  "tool": "rexpipe_redact_pii",
  "arguments": {
    "text": "Contact john@example.com or call 555-123-4567",
    "types": ["email", "phone"]
  }
}
```

**Result:**
```json
{
  "result": "Contact [REDACTED] or call [REDACTED]",
  "redactions": [
    {"type": "email", "original": "john@example.com"},
    {"type": "phone", "original": "555-123-4567"}
  ]
}
```

### Extract Code Blocks

```json
{
  "tool": "rexpipe_extract",
  "arguments": {
    "text": "Here's some code:\n```python\nprint('hello')\n```",
    "pattern": "```[a-z]*\\n([\\s\\S]*?)\\n```",
    "capture_group": 1
  }
}
```

### Multi-step Pipeline

```json
{
  "tool": "rexpipe_pipeline",
  "arguments": {
    "text": "ERROR: Connection failed at 192.168.1.1",
    "pipeline": "[[steps]]\ntype = \"filter\"\npattern = \"ERROR\"\naction = \"keep_line\"\n\n[[steps]]\ntype = \"substitute\"\npattern = \"\\\\d+\\\\.\\\\d+\\\\.\\\\d+\\\\.\\\\d+\"\nreplacement = \"[IP]\""
  }
}
```

## Resources

The integration exposes pattern libraries as MCP resources:

| Resource URI | Description |
|--------------|-------------|
| `rexpipe://patterns/common` | Email, URL, UUID, dates, etc. |
| `rexpipe://patterns/logs` | Apache, nginx, syslog parsing |
| `rexpipe://patterns/ai` | PII, secrets, code extraction |

Access patterns in your prompts:

```
Use the pattern from rexpipe://patterns/ai for detecting API keys
```

## Prompts

Pre-configured prompts for common tasks:

| Prompt | Description |
|--------|-------------|
| `redact_pii` | Remove all PII from text |
| `extract_code` | Extract code from markdown |
| `clean_text` | Normalize whitespace and formatting |

## Claude Desktop Integration

Add to `~/.config/claude/claude_desktop_config.json` (Linux) or `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS):

```json
{
  "mcpServers": {
    "rexpipe": {
      "command": "rexpipe",
      "args": ["--mcp"]
    }
  }
}
```

## Security Notes

- rexpipe processes text locally - no data leaves your machine
- Pattern matching is ReDoS-safe by default (linear-time regex)
- Use `--no-shell` to disable shell transform plugins
- Secrets detection is local pattern matching, not network-based

## Troubleshooting

### MCP Inspector

Test your setup with the MCP Inspector:

```bash
npx @anthropic-ai/mcp-inspector rexpipe --mcp
```

### Debug Mode

Enable verbose logging:

```bash
RUST_LOG=debug rexpipe --mcp
```

### Version Check

Ensure MCP compatibility:

```bash
rexpipe --version
# Should show 2.0.0 or higher
```
