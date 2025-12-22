# Pattern Library Index

Quick reference for finding the right pattern for common tasks.

## Libraries

| Library | Patterns | Purpose |
|---------|----------|---------|
| `common.toml` | 40+ | General text processing: emails, URLs, dates, identifiers |
| `logs.toml` | 40+ | Log parsing: Apache, nginx, syslog, JSON logs, stack traces |

## Quick Reference by Task

### PII Detection & Redaction

```toml
patterns_include = ["patterns/common.toml"]

[[steps]]
type = "substitute"
pattern = '${email}'
replacement = "[EMAIL]"

[[steps]]
type = "substitute"
pattern = '${phone_us}'
replacement = "[PHONE]"
```

### Secret Detection

```toml
patterns_include = ["patterns/common.toml"]

[[steps]]
type = "filter"
mode = "lines"
pattern = '${security.api_key_generic}'
keep = true  # Find lines with secrets
```

**Secret patterns:**
- `${security.api_key_generic}` - Generic API keys (32+ chars)
- `${security.password_field}` - Password field patterns
- `${security.credit_card}` - Credit card numbers
- `${security.ssn}` - Social security numbers

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

### Data Extraction

```toml
patterns_include = ["patterns/common.toml"]

[[steps]]
type = "extract"
pattern = '${data.json_key}'
```

**Data patterns:**
- `${data.json_key}` - JSON key patterns
- `${data.semver}` - Semantic versions
- `${data.key_value}` - Key-value pairs

### Network Patterns

```toml
patterns_include = ["patterns/common.toml"]

[[steps]]
type = "extract"
pattern = '${net.ipv4}'
```

**Network patterns:**
- `${net.ipv4}` - IPv4 addresses
- `${net.ipv6}` - IPv6 addresses
- `${net.mac}` - MAC addresses
- `${net.cidr}` - CIDR notation

### Timestamp Patterns

```toml
patterns_include = ["patterns/common.toml"]

[[steps]]
type = "extract"
pattern = '${time.iso8601}'
```

**Time patterns:**
- `${time.iso8601}` - ISO 8601 timestamps
- `${time.date_iso}` - ISO dates
- `${time.timestamp_unix}` - Unix timestamps

## Pattern Naming Convention

Patterns use dot notation: `${category.name}`

| Category | Purpose |
|----------|---------|
| `net.*` | Network addresses |
| `time.*` | Timestamps and dates |
| `data.*` | Structured data |
| `code.*` | Source code structures |
| `security.*` | Credentials and sensitive data |
| `level.*` | Log severity levels |
| `apache.*` | Apache/nginx logs |
| `syslog.*` | Syslog formats |
| `app.*` | Application-specific logs |

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
patterns_include = ["patterns/common.toml"]

[patterns.myapp]
# Your custom patterns
user_token = '\bUSR_[a-zA-Z0-9]{24}\b'
order_id = '\bORD-\d{8}-[A-Z]{4}\b'
```
