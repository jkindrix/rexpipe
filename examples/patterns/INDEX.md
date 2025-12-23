# Pattern Library Index

This directory contains reusable regex pattern libraries for rexpipe pipelines.

## Available Libraries

| Library | Description | Pattern Count |
|---------|-------------|---------------|
| [`common.toml`](common.toml) | General-purpose patterns (email, URLs, dates, etc.) | 30+ |
| [`logs.toml`](logs.toml) | Log parsing patterns (Apache, nginx, syslog, levels) | 40+ |
| [`security.toml`](security.toml) | Secret detection, credentials, API keys | 25+ |
| [`pii.toml`](pii.toml) | PII/PHI detection for GDPR, HIPAA, PCI compliance | 30+ |

## Usage

Include a pattern library in your pipeline:

```toml
patterns_include = ["patterns/common.toml"]

[[step]]
type = "substitute"
pattern = "${email}"
replacement = "[REDACTED_EMAIL]"
```

Patterns support nested categories with dot notation:

```toml
pattern = "${net.ipv4}"      # From [patterns.net] section
pattern = "${time.iso8601}"  # From [patterns.time] section
```

## Pattern Reference

### common.toml

#### Root Patterns
| Pattern | Description | Example Match |
|---------|-------------|---------------|
| `email` | Email addresses | `user@example.com` |
| `url` | HTTP(S) URLs | `https://example.com/path` |
| `url_strict` | Validated URLs | `https://www.example.com/` |
| `phone_us` | US phone numbers | `(555) 123-4567` |
| `phone_intl` | International phones | `+44 20 7946 0958` |
| `uuid` | UUIDs | `123e4567-e89b-12d3-a456-426614174000` |
| `hex_color` | Hex colors | `#FF5733` |

#### Network (`${net.*}`)
| Pattern | Description | Example Match |
|---------|-------------|---------------|
| `net.ipv4` | IPv4 (validated) | `192.168.1.1` |
| `net.ipv4_simple` | IPv4 (fast) | `10.0.0.1` |
| `net.ipv6` | IPv6 addresses | `2001:0db8:85a3::8a2e:0370:7334` |
| `net.mac` | MAC addresses | `00:1A:2B:3C:4D:5E` |
| `net.port` | Port numbers | `:8080` |
| `net.cidr` | CIDR notation | `10.0.0.0/8` |

#### Time (`${time.*}`)
| Pattern | Description | Example Match |
|---------|-------------|---------------|
| `time.iso8601` | ISO 8601 datetime | `2024-12-22T10:30:00Z` |
| `time.date_iso` | ISO date | `2024-12-22` |
| `time.date_us` | US date format | `12/22/2024` |
| `time.date_eu` | EU date format | `22.12.2024` |
| `time.time_24h` | 24-hour time | `14:30:00` |
| `time.time_12h` | 12-hour time | `2:30 PM` |
| `time.timestamp_unix` | Unix timestamp | `1703246400` |

#### Data (`${data.*}`)
| Pattern | Description | Example Match |
|---------|-------------|---------------|
| `data.json_key` | JSON key | `"name":` |
| `data.json_string` | JSON string value | `"hello world"` |
| `data.key_value` | Key=value pair | `level=info` |
| `data.integer` | Integer numbers | `-42` |
| `data.decimal` | Decimal numbers | `3.14159` |
| `data.semver` | Semantic version | `v2.1.0-beta.1` |
| `data.path_unix` | Unix file path | `/usr/local/bin` |
| `data.path_windows` | Windows path | `C:\Users\admin` |

#### Code (`${code.*}`)
| Pattern | Description | Example Match |
|---------|-------------|---------------|
| `code.identifier` | Variable/function name | `myFunction` |
| `code.constant` | SCREAMING_CASE constant | `MAX_SIZE` |
| `code.function_call` | Function call | `print(` |
| `code.comment_hash` | Hash comments | `# comment` |
| `code.comment_slash` | Slash comments | `// comment` |
| `code.string_single` | Single-quoted string | `'hello'` |
| `code.string_double` | Double-quoted string | `"hello"` |

#### Security (`${security.*}`)
| Pattern | Description | Example Match |
|---------|-------------|---------------|
| `security.api_key_generic` | Generic API keys | `api_key=abc123...` |
| `security.password_field` | Password assignments | `password="secret"` |
| `security.aws_access_key` | AWS access key ID | `AKIAIOSFODNN7EXAMPLE` |
| `security.aws_secret_key` | AWS secret key | (40-char base64) |
| `security.credit_card` | Credit card numbers | `4111-1111-1111-1111` |
| `security.ssn` | US Social Security | `123-45-6789` |

---

### logs.toml

#### Severity Levels (`${level.*}`)
| Pattern | Description | Example Match |
|---------|-------------|---------------|
| `level.error` | Error indicators | `ERROR`, `error`, `SEVERE` |
| `level.warning` | Warning indicators | `WARN`, `warning` |
| `level.info` | Info indicators | `INFO`, `info` |
| `level.debug` | Debug indicators | `DEBUG`, `TRACE` |
| `level.any` | Any log level | `INFO`, `ERROR`, etc. |
| `level.any_bracket` | Bracketed levels | `[INFO]`, `[ERROR]` |

#### Apache/nginx (`${apache.*}`)
| Pattern | Description | Example Match |
|---------|-------------|---------------|
| `apache.combined` | Combined log format | Full access log line |
| `apache.common` | Common log format | Basic access log line |
| `apache.status` | HTTP status codes | `200`, `404`, `500` |

---

### security.toml

See [`security.toml`](security.toml) for comprehensive secret detection patterns including:
- Cloud provider credentials (AWS, GCP, Azure)
- API tokens (GitHub, Slack, Stripe, etc.)
- Database connection strings
- Private keys and certificates
- JWT tokens

---

### pii.toml

See [`pii.toml`](pii.toml) for PII/PHI detection patterns supporting:
- GDPR compliance (Article 4 personal data identifiers)
- HIPAA Safe Harbor (18 PHI identifiers)
- PCI DSS (payment card data)
- General PII (names, addresses, government IDs)

## Creating Custom Libraries

Create a TOML file with the following structure:

```toml
name = "My Custom Patterns"
description = "Patterns for specific use case"
version = "1.0.0"

# Optional: include other libraries
patterns_include = ["common.toml"]

[patterns]
# Simple patterns
my_pattern = 'regex here'

# Categorized patterns
[patterns.category]
specific_pattern = 'another regex'
```

## Best Practices

1. **Use appropriate escaping**: TOML single quotes for simple patterns, double quotes when escaping is needed
2. **Group related patterns**: Use nested `[patterns.category]` sections
3. **Document patterns**: Add comments explaining what each pattern matches
4. **Test patterns**: Use `rexpipe --validate-library patterns/mylib.toml` to verify
5. **Version libraries**: Include a `version` field for tracking changes
