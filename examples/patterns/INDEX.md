# Pattern Library Index

Quick reference for AI agents to find the right pattern for common tasks.

## Libraries

| Library | Patterns | Purpose |
|---------|----------|---------|
| `ai.toml` | 70+ | AI agent workflows: PII, secrets, code extraction, data cleaning |
| `common.toml` | 40+ | General text processing: emails, URLs, dates, identifiers |
| `logs.toml` | 40+ | Log parsing: Apache, nginx, syslog, JSON logs, stack traces |

## Quick Reference by Task

### PII Detection & Redaction

```toml
patterns_include = ["patterns/ai.toml"]

[[steps]]
type = "substitute"
pattern = '${pii.email}'
replacement = "[EMAIL]"

[[steps]]
type = "substitute"
pattern = '${pii.phone}'
replacement = "[PHONE]"

[[steps]]
type = "substitute"
pattern = '${pii.ssn}'
replacement = "[SSN]"

[[steps]]
type = "substitute"
pattern = '${pii.credit_card}'
replacement = "[CARD]"
```

### Secret Detection

```toml
patterns_include = ["patterns/ai.toml"]

[[steps]]
type = "filter"
mode = "lines"
pattern = '${secrets.api_key}'
keep = true  # Find lines with secrets
```

**Secret patterns:**
- `${secrets.api_key}` - Generic API keys (32+ chars)
- `${secrets.jwt}` - JSON Web Tokens
- `${secrets.aws_access_key}` - AWS access key IDs
- `${secrets.github_token}` - GitHub tokens
- `${secrets.private_key_begin}` - PEM private key markers

### Code Extraction from LLM Output

```toml
patterns_include = ["patterns/ai.toml"]

[[steps]]
type = "extract"
pattern = '${code.markdown_block}'
# Extracts: ```python\ncode here\n```
```

**Code patterns:**
- `${code.markdown_block}` - Fenced code blocks
- `${code.inline_code}` - Backtick inline code
- `${code.function_def_python}` - Python function definitions
- `${code.import_python}` - Python imports
- `${code.todo_marker}` - TODO/FIXME comments

### Log Processing

```toml
patterns_include = ["patterns/logs.toml"]

[[steps]]
type = "filter"
mode = "lines"
pattern = '${level.error}'
keep = true
```

**Log patterns:**
- `${level.error}` - Error/fatal/critical
- `${level.warning}` - Warning messages
- `${apache.combined}` - Apache combined log format
- `${syslog.bsd}` - BSD syslog format
- `${app.java_exception}` - Java stack traces

### Data Cleaning

```toml
patterns_include = ["patterns/ai.toml"]

[[steps]]
type = "substitute"
pattern = '${clean.multiple_spaces}'
replacement = " "

[[steps]]
type = "substitute"
pattern = '${clean.trailing_whitespace}'
replacement = ""
```

**Cleaning patterns:**
- `${clean.multiple_spaces}` - Collapse whitespace
- `${clean.multiple_newlines}` - Collapse blank lines
- `${clean.control_chars}` - Remove control characters
- `${clean.ansi_codes}` - Strip ANSI color codes
- `${clean.html_entities}` - HTML entities like `&nbsp;`

### Structured Data Extraction

```toml
patterns_include = ["patterns/ai.toml"]

[[steps]]
type = "extract"
pattern = '${data.json_key_value}'
```

**Data patterns:**
- `${data.json_key_value}` - JSON key-value pairs
- `${data.yaml_key_value}` - YAML key-value pairs
- `${data.markdown_link}` - Markdown links
- `${data.xml_tag}` - XML/HTML tags

### Validation

```toml
patterns_include = ["patterns/ai.toml"]

[[steps]]
type = "validate"
pattern = '${validate.valid_email}'
on_fail = "skip"
```

**Validation patterns:**
- `${validate.valid_email}` - Well-formed email
- `${validate.valid_url}` - Well-formed URL
- `${validate.valid_uuid}` - UUID format
- `${validate.valid_semver}` - Semantic version

## Pattern Naming Convention

Patterns use dot notation: `${category.name}`

| Category | Purpose |
|----------|---------|
| `pii.*` | Personally identifiable information |
| `secrets.*` | Credentials and tokens |
| `code.*` | Source code structures |
| `llm.*` | LLM-specific patterns |
| `data.*` | Structured data |
| `clean.*` | Text normalization |
| `validate.*` | Format validation |
| `level.*` | Log severity levels |
| `apache.*` | Apache/nginx logs |
| `syslog.*` | Syslog formats |
| `net.*` | Network addresses |
| `time.*` | Timestamps and dates |

## Combining Libraries

Libraries can include each other:

```toml
# logs.toml automatically includes common.toml
patterns_include = ["patterns/logs.toml"]

# Now you can use both log patterns and common patterns:
# ${level.error}  from logs.toml
# ${net.ipv4}     from common.toml (via include)
```

## Creating Custom Libraries

```toml
name = "My Custom Patterns"
description = "Project-specific patterns"
version = "1.0.0"

# Include base patterns
patterns_include = ["patterns/ai.toml"]

[patterns.myapp]
# Your custom patterns
user_token = '\bUSR_[a-zA-Z0-9]{24}\b'
order_id = '\bORD-\d{8}-[A-Z]{4}\b'
```
