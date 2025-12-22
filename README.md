# rexpipe

**A transformation recipe system for repeatable, shareable text processing.**

rexpipe is not a sed replacement—it's a system for defining, sharing, and composing multi-stage text transformations as version-controlled configuration files. Where sed excels at one-liners, rexpipe excels at 10-50 step pipelines that need to be maintained, shared, and composed.

## The Core Idea

```bash
# Instead of this fragile, unmaintainable chain:
cat log.txt | grep -v DEBUG | sed 's/ERROR/ERR/g' | awk '{print $1,$3}' | ...

# Define a transformation recipe:
rexpipe -c pipelines/normalize-logs.toml < log.txt

# The recipe is:
# - Self-documenting (TOML with descriptions)
# - Version-controllable (commit it, review changes)
# - Shareable (drop into any project)
# - Composable (chain recipes together)
# - Testable (dry-run, explain modes)
```

## When to Use rexpipe

✅ **Use rexpipe for:**
- Multi-step transformations (5+ regex operations)
- Transformations you'll run repeatedly
- Pipelines shared across team/projects
- Audit-sensitive data processing
- Complex log normalization, data cleaning, redaction

❌ **Use sed/awk for:**
- Quick one-off substitutions
- Interactive exploration
- Simple single-pattern matches

## Why rexpipe?

| Traditional Tools | rexpipe |
|-------------------|---------|
| Cryptic one-liners | Self-documenting TOML recipes |
| Copy-paste to share | `git clone` + run |
| Regex from scratch | Pattern libraries with `${email}`, `${ipv4}` |
| Silent failures | Structured errors with fix suggestions |
| No audit trail | Verification and provenance tracking |

## Key Features

- **JSON output for scripting** - When stdout is not a TTY, output is JSON by default
- **Structured errors** - `--error-format json` for parseable error handling
- **Safe in-place editing** - Requires `--apply` in non-interactive mode to prevent accidents
- **Explain mode** - `--explain` describes what pipeline will do before running
- **Verify mode** - `--verify` confirms what transformations were applied
- **Schema versioning** - All JSON includes `schema_version` for stability

## Core Features

- **Streaming Architecture**: Constant memory usage regardless of file size
- **Pattern Libraries**: Reusable regex patterns with `${pattern.name}` syntax
- **Multi-File Processing**: Recursive search, in-place editing, grep-like modes
- **Atomic Operations**: Safe mutations with automatic backup
- **Dry-Run Preview**: See changes before applying
- **TOML Configuration**: Version-controllable, shareable pipeline definitions
- **Audit Trails**: Cryptographic provenance tracking for compliance
- **Block Processing**: Cross-line state machine for multi-line patterns
- **Syntax-Aware**: Tree-sitter integration for scope-limited matching (optional)
- **Data Protection**: FPE encryption and deterministic masking (optional)

## Installation

**Requirements:** Rust 1.85+ (Rust 2024 edition)

```bash
cargo install rexpipe
```

Or build from source:

```bash
git clone https://github.com/jkindrix/rexpipe
cd rexpipe
cargo build --release
```

### Minimum Supported Rust Version (MSRV)

rexpipe requires **Rust 1.85.0** or later. This version was chosen because:

- **Rust 2024 Edition**: Access to the latest language features and improved pattern matching ergonomics
- **Stable async features**: Full async/await support with recent improvements
- **Enhanced error handling**: Better `?` operator behavior and error display

The MSRV is enforced in `Cargo.toml` via `rust-version = "1.85"` and tested in CI.

## Quick Start

### Basic Pattern Replacement

```bash
echo "Test 123 and 456" | rexpipe --pattern '\d+' --replacement 'NUMBER'
# Output (JSON when piped): {"metadata":{"schema_version":"1.0",...},"data":{...}}
# Use --text for plain text: Test NUMBER and NUMBER
```

### Scripting Workflow

```bash
# 1. Explain what pipeline will do (before running)
rexpipe -c pipeline.toml --explain

# 2. Process with verification
echo "data 123" | rexpipe -p '\d+' -r 'X' --verify

# 3. Safe in-place editing (requires --apply)
rexpipe -p 'old' -r 'new' -i --apply *.txt

# 4. Get structured errors for parsing
rexpipe -p '[invalid' --error-format json 2>&1
```

### Configuration-Based Processing

```bash
# Process logs with predefined pipeline
rexpipe --config examples/log-cleanup.toml < access.log > cleaned.log

# Inspect patterns before processing
rexpipe --config examples/log-cleanup.toml --inspect < sample.log

# Preview in-place changes (dry-run)
rexpipe --config cleanup.toml -i --dry-run src/
```

## Configuration Format

Pipeline configurations use TOML format:

```toml
name = "Log Cleanup Pipeline"
description = "Standardizes and sanitizes server logs"
version = "1.0.0"

[[step]]
type = "substitute"
description = "Normalize error levels"
pattern = '\[ERROR\]'
replacement = '[ERR]'
flags = ["global"]
enabled = true

[[step]]
type = "filter"
description = "Remove debug messages"
pattern = 'DEBUG'
action = "drop_line"
enabled = true

[[step]]
type = "substitute"
description = "Standardize user ID format"
pattern = 'user_id=(\d+)'
replacement = 'uid=${1}'
flags = ["global"]
enabled = true
```

### Configuration Inheritance

Pipelines can extend base configurations using the `extends` field. This enables DRY configuration by defining common settings in a base file:

```toml
# base.toml - Base configuration with common settings
name = "Base Pipeline"
version = "1.0.0"

[settings]
pcre_mode = true
strict_mode = true

[[step]]
type = "substitute"
pattern = '\s+$'
replacement = ''
description = "Trim trailing whitespace"
```

```toml
# production.toml - Extends base with additional steps
extends = "base.toml"
name = "Production Pipeline"

[[step]]
type = "substitute"
pattern = 'DEBUG:.*\n'
replacement = ''
description = "Remove debug lines"
```

When loaded, the child config:
- Inherits settings from base (child settings override if explicitly set)
- Prepends base steps to its own steps
- Merges pattern includes
- Can override name, description, and version

## Pattern Libraries

Pattern libraries allow you to define reusable regex patterns in separate files and reference them across multiple pipelines using `${pattern.name}` syntax.

### Creating a Pattern Library

```toml
# patterns/common.toml
name = "Common Patterns"
description = "Reusable patterns for log processing"
version = "1.0.0"

[patterns.logs]
error = '\[ERROR\]'
warning = '\[WARN(ING)?\]'
timestamp = '\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2}:\d{2}'

[patterns.data]
email = '[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}'
ip_address = '\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}'
```

### Using Pattern Libraries in Pipelines

```toml
name = "Log Processor"
patterns_include = ["patterns/common.toml"]

[[step]]
type = "substitute"
pattern = '${logs.error}'
replacement = '[ERR]'

[[step]]
type = "filter"
pattern = '${data.ip_address}'
action = "keep_line"
```

### Library Location Resolution

Pattern libraries are resolved in this order:

1. **Absolute path**: If the path starts with `/` or contains a drive letter (Windows)
2. **Relative to pipeline config**: If a pipeline is loaded from `/path/to/pipeline.toml` and references `patterns/common.toml`, it looks for `/path/to/patterns/common.toml`
3. **Current working directory**: `./patterns/common.toml`
4. **User config directory**: `~/.config/rexpipe/patterns/` (Linux/macOS) or `%APPDATA%\rexpipe\patterns\` (Windows)
5. **Global patterns directory**: `~/.rexpipe/patterns/` (legacy)

**Environment variable override:**
```bash
# Set custom patterns directory
export REXPIPE_PATTERNS_DIR=/custom/patterns
```

**Resolution examples:**
```
# Pipeline in /home/user/project/config.toml
patterns_include = ["common.toml"]
# Checks: /home/user/project/common.toml → ~/.config/rexpipe/patterns/common.toml

patterns_include = ["./patterns/custom.toml"]
# Checks: /home/user/project/patterns/custom.toml

patterns_include = ["/opt/shared/patterns.toml"]
# Uses absolute path directly
```

**Circular include detection:**
rexpipe detects and reports circular library includes (A includes B, B includes A).

### Nested Libraries

Libraries can include other libraries:

```toml
# patterns/extended.toml
name = "Extended Patterns"
patterns_include = ["common.toml"]  # Include another library

[patterns.custom]
special = 'my-pattern'
```

### Remote Libraries

With the `remote` feature enabled, you can load pattern libraries from URLs:

```toml
# Include a remote library
patterns_include = [
    "https://example.com/patterns/common.toml",
    "./local-patterns.toml"
]
```

Install with remote support:

```bash
cargo install rexpipe --features remote
```

Remote libraries are cached for the duration of the process. They can include other remote libraries but cannot reference local files.

### CLI Commands for Libraries

```bash
# List all patterns in a library
rexpipe --list-patterns patterns/common.toml

# Validate a library file
rexpipe --validate-library patterns/common.toml
```

### Included Pattern Libraries

rexpipe ships with two pattern libraries in `examples/patterns/`:

**common.toml** (43 patterns):
- `email`, `url`, `uuid`, `phone_us`, `phone_intl`
- `net.ipv4`, `net.ipv6`, `net.mac`, `net.cidr`
- `time.iso8601`, `time.date_iso`, `time.timestamp_unix`
- `data.json_key`, `data.semver`, `data.key_value`
- `code.identifier`, `code.function_call`, `code.comment_hash`
- `security.api_key_generic`, `security.password_field`, `security.credit_card`, `security.ssn`

**logs.toml** (40 patterns, includes common.toml):
- `level.error`, `level.warning`, `level.info`, `level.debug`
- `apache.combined`, `apache.common`, `apache.status_5xx`
- `syslog.bsd`, `syslog.rfc5424`
- `json.json_line`, `json.message_field`
- `app.java_exception`, `app.python_traceback`, `app.request_id`
- `docker.container_prefix`, `docker.k8s_prefix`
- `nginx.error_log`, `nginx.access_log`

### Included Transformation Recipes

rexpipe ships with ready-to-use pipelines in `examples/pipelines/`:

| Recipe | Description |
|--------|-------------|
| `git-changelog.toml` | Transform git log into formatted changelog with conventional commit parsing |
| `secrets-redact.toml` | Redact API keys, tokens, passwords, PII from logs/configs |
| `csv-clean.toml` | Normalize whitespace, dates, null values in CSV data |
| `build-triage.toml` | Categorize compiler errors/warnings for easier debugging |
| `log-normalize.toml` | Unify timestamps and severity levels from mixed log sources |
| `markdown-toc.toml` | Generate table of contents from markdown headers |
| `stacktrace-clean.toml` | Remove framework noise from stack traces |
| `sql-to-json.toml` | Transform SQL table output to JSON lines |
| `env-to-docker.toml` | Convert .env files to Docker -e flags |
| `log-stats.toml` | **Aggregation:** Analyze logs and produce statistics summary |
| `api-audit.toml` | **Aggregation:** Audit API access logs with JSON report |
| `code-lens-errors.toml` | View codebase through error-handling lens |
| `code-lens-deps.toml` | Extract dependency graph from source imports |
| `code-lens-api.toml` | Extract public API surface from source files |
| `curl-to-api-doc.toml` | Transform curl verbose output into API documentation |
| `meeting-notes.toml` | Extract action items and decisions from meeting notes |
| `prose-stats.toml` | Analyze prose for readability issues |
| `crontab-explain.toml` | Transform crontab entries into human-readable schedules |
| `dependency-audit.toml` | Extract and normalize dependencies from package files |
| `diff-to-changelog.toml` | Transform git diff into changelog-style summary |
| `error-frequency.toml` | Extract unique error types for frequency analysis |
| `http-access-stats.toml` | Transform HTTP access logs into status code summary |
| `json-logs-to-text.toml` | Transform JSON log lines into human-readable format |
| `k8s-sanitize.toml` | Sanitize Kubernetes manifests for sharing |
| `secrets-redact-v2.toml` | Redact secrets using pattern library (cleaner version) |
| `shell-history-audit.toml` | Analyze shell history for security review |
| `sql-format.toml` | Format messy SQL queries for readability |
| `stats-collector.toml` | Collect and deduplicate statistics from logs |
| `todo-extract.toml` | Extract TODO/FIXME/HACK comments from source code |
| `code-lens-complexity.toml` | Find complexity hotspots - deeply nested code |

**Usage:**
```bash
# Generate changelog from recent commits
git log --oneline -20 | rexpipe -c examples/pipelines/git-changelog.toml

# Redact secrets before sharing logs
cat app.log | rexpipe -c examples/pipelines/secrets-redact.toml

# Chain recipes together
cat data.csv | rexpipe -c examples/pipelines/csv-clean.toml \
             | rexpipe -c examples/pipelines/secrets-redact.toml
```

## Script-Based Plugins

Extend rexpipe with custom script-based plugins that can be used as transform actions.

### Plugin Directories

rexpipe automatically loads plugins from these directories (in order):

1. `./plugins/` (current directory)
2. `~/.config/rexpipe/plugins/`
3. `/usr/local/share/rexpipe/plugins/` (Unix)
4. `$REXPIPE_PLUGIN_DIR` (if set)

You can also specify a custom directory:

```bash
rexpipe --plugin-dir ./my-plugins -c config.toml < input.txt
```

### Creating a Plugin

Plugins are scripts that receive input via stdin and output the transformed result:

```bash
#!/bin/bash
# ~/.config/rexpipe/plugins/rot13.sh
# ROT13 cipher transform
tr 'A-Za-z' 'N-ZA-Mn-za-m'
```

```python
#!/usr/bin/env python3
# ~/.config/rexpipe/plugins/word_count.py
# Count words in the input
import sys
text = sys.stdin.read()
print(len(text.split()))
```

### Supported Script Types

| Extension | Interpreter |
|-----------|-------------|
| `.sh`     | `sh`        |
| `.py`     | `python3`   |
| `.rb`     | `ruby`      |
| `.pl`     | `perl`      |
| (none)    | executable  |

### Using Plugins in Pipelines

```toml
[[step]]
type = "transform"
pattern = '\w+'
transform_action = { plugin = { name = "rot13" } }
description = "Apply ROT13 cipher to words"
```

## Step Types

- **substitute**: Replace matched patterns with new text
- **filter**: Keep or drop lines based on pattern matching
- **extract**: Extract only the matched portions
- **validate**: Ensure lines match required patterns
- **transform**: Apply text transformations to matched content
- **block**: Cross-line state machine for multi-line pattern processing

### Transform Actions

The transform step type supports the following actions:

- **uppercase**: Convert matched text to uppercase
- **lowercase**: Convert matched text to lowercase
- **trim**: Remove whitespace from matched text
- **prepend**: Add text before matched content
- **append**: Add text after matched content
- **reverse**: Reverse the matched text
- **remove_whitespace**: Remove all whitespace from matched text
- **title_case**: Capitalize first letter of each word
- **base64_encode**: Encode matched text as Base64
- **base64_decode**: Decode Base64 matched text
- **url_encode**: URL-encode matched text
- **url_decode**: URL-decode matched text
- **normalize_whitespace**: Replace runs of whitespace with single space
- **fpe_encrypt**: Format-preserving encryption (requires `fpe` feature)
- **fpe_decrypt**: Format-preserving decryption (requires `fpe` feature)
- **mask_deterministic**: Consistent one-way masking with seed

## Filter Actions

- **keep_line**: Keep entire line if pattern matches
- **drop_line**: Drop entire line if pattern matches
- **keep_match**: Keep only if pattern matches
- **drop_match**: Drop only if pattern matches

## TOML Configuration Reference

This section provides a complete reference for all configuration fields.

### Pipeline Configuration

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `name` | string | No | `null` | Pipeline name for display |
| `description` | string | No | `null` | Pipeline description |
| `version` | string | No | `null` | Pipeline version (semver recommended) |
| `patterns_include` | array | No | `[]` | Pattern library files to include |
| `settings` | table | No | defaults | Global pipeline settings |
| `step` | array | **Yes** | - | Array of processing steps |

**Example:**
```toml
name = "My Pipeline"
description = "Processes log files"
version = "1.0.0"
patterns_include = ["patterns/common.toml"]

[settings]
timeout_ms = 5000

[[step]]
# ... step definitions
```

### Settings Table

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `pcre_mode` | bool | `false` | Use PCRE-compatible regex (lookahead/lookbehind) |
| `fixed_strings` | bool | `false` | Treat patterns as literal strings |
| `context_before` | int | `0` | Lines of context before matches |
| `context_after` | int | `0` | Lines of context after matches |
| `timeout_ms` | int | `0` | Shell command timeout in milliseconds (0 = no timeout) |
| `allow_shell` | bool | `true` | Allow shell transform execution |
| `strict_mode` | bool | `false` | Reject potentially dangerous ReDoS patterns |

**Example:**
```toml
[settings]
pcre_mode = true
timeout_ms = 5000
strict_mode = true
```

### Finalize Section (Aggregation)

The `[finalize]` section enables post-processing aggregation after all lines are processed. Use this to count matches, collect unique values, and produce summary reports.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `template` | string | `null` | Output template with `${count:NAME}` placeholders |
| `output_format` | string | `"text"` | Output format: `"text"` or `"json"` |
| `suppress_output` | bool | `false` | If true, only show finalize output (not processed lines) |
| `shell` | string | `null` | Shell command to run after processing (receives JSON input) |
| `counters` | array | `[]` | Counter definitions (see below) |

**Counter Definition:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | **Required** | Counter name (referenced as `${count:NAME}`) |
| `pattern` | string | **Required** | Regex pattern to match |
| `deduplicate` | bool | `false` | Only count unique matched values |
| `collect_values` | bool | `false` | Store matched values (available in JSON output) |
| `max_collected_values` | int | `1000` | Maximum values to collect |
| `description` | string | `null` | Description of what this counter tracks |

**Template Variables:**
- `${count:NAME}` - Value of counter named NAME
- `${lines}` - Total lines processed
- `${matches}` - Total pattern matches across all steps
- `${transformations}` - Total transformations applied

**Example:**
```toml
name = "log-stats"

[[step]]
type = "filter"
pattern = "."
action = "keep_line"

[finalize]
template = """
=== Summary ===
Errors: ${count:errors}
Unique IPs: ${count:ips}
Total lines: ${lines}
"""
suppress_output = true

[[finalize.counters]]
name = "errors"
pattern = "ERROR|FATAL"

[[finalize.counters]]
name = "ips"
pattern = "^(\\d+\\.\\d+\\.\\d+\\.\\d+)"
deduplicate = true
```

**JSON Output Mode:**
```toml
[finalize]
output_format = "json"
suppress_output = true

[[finalize.counters]]
name = "clients"
pattern = "^(\\d+\\.\\d+\\.\\d+\\.\\d+)"
deduplicate = true
collect_values = true
```

Output:
```json
{
  "lines_processed": 100,
  "total_matches": 42,
  "counters": {
    "clients": {
      "count": 15,
      "unique": true,
      "values": ["192.168.1.1", "192.168.1.2", ...]
    }
  }
}
```

### Step Definition

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `type` | string | No | `"substitute"` | Step type (see Step Types) |
| `pattern` | string | **Yes** | - | Regex pattern or `${ref}` reference |
| `replacement` | string | Conditional | `null` | Replacement text (required for substitute, prepend, append) |
| `action` | string | Conditional | `null` | Filter action (required for filter type) |
| `transform` | string/table | Conditional | `null` | Transform action (required for transform type) |
| `flags` | array | No | `[]` | Regex flags |
| `description` | string | No | `null` | Step description |
| `enabled` | bool | No | `true` | Whether step is active |

### Step Types

| Type | Description | Required Fields |
|------|-------------|-----------------|
| `substitute` | Replace matched text | `pattern`, `replacement` |
| `filter` | Keep or drop lines | `pattern`, `action` |
| `extract` | Extract matched portions | `pattern` |
| `validate` | Ensure lines match pattern | `pattern` |
| `transform` | Apply text transformation | `pattern`, `transform` |

### Filter Actions

| Action | Description |
|--------|-------------|
| `keep_line` | Keep entire line if pattern matches anywhere |
| `drop_line` | Drop entire line if pattern matches anywhere |
| `keep_match` | Keep only the matched portion of text |
| `drop_match` | Remove matched portions, keep rest |

### Transform Actions

**Simple transforms** (string value):

| Transform | Description |
|-----------|-------------|
| `uppercase` | Convert matched text to UPPERCASE |
| `lowercase` | Convert matched text to lowercase |
| `title_case` | Capitalize First Letter Of Each Word |
| `trim` | Remove leading/trailing whitespace |
| `remove_whitespace` | Remove all whitespace |
| `normalize_whitespace` | Replace runs of whitespace with single space |
| `reverse` | Reverse character order |
| `base64_encode` | Base64 encode matched text |
| `base64_decode` | Base64 decode matched text |
| `url_encode` | URL-encode special characters |
| `url_decode` | URL-decode escaped characters |
| `char_count` | Replace with character count |
| `word_count` | Replace with word count |
| `sort_chars` | Sort characters alphabetically |
| `deduplicate` | Remove duplicate lines |
| `prepend` | Add `replacement` text before match |
| `append` | Add `replacement` text after match |

**Shell transform** (table value):
```toml
transform.shell.command = "base64"
```

**Plugin transform** (table value):
```toml
transform.plugin.name = "my_plugin"
transform.plugin.args = ["arg1", "arg2"]
```

### Regex Flags

| Flag | Description |
|------|-------------|
| `global` | Replace all matches (not just first) |
| `case_insensitive` | Case-insensitive matching |
| `multiline` | `^` and `$` match line boundaries |
| `dot_all` | `.` matches newlines |
| `unicode` | Enable Unicode character classes |
| `extended` | Allow whitespace and comments in pattern |

### Replacement Syntax

Replacement strings support capture group references:

| Syntax | Description |
|--------|-------------|
| `${1}` | First capture group |
| `${2}` | Second capture group |
| `${name}` | Named capture group |
| `$0` or `${0}` | Entire match |

**Example:**
```toml
[[step]]
type = "substitute"
pattern = '(\w+)@(\w+\.com)'
replacement = '${2}/${1}'  # email@domain.com → domain.com/email
```

### Pattern Library Format

Pattern libraries use the same TOML format with a `patterns` table:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | No | Library name |
| `description` | string | No | Library description |
| `version` | string | No | Library version |
| `patterns_include` | array | No | Other libraries to include |
| `patterns` | table | **Yes** | Pattern definitions |

**Pattern definitions** can be flat or nested:
```toml
[patterns]
# Flat pattern
email = '[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}'

# Nested patterns (accessed as ${category.name})
[patterns.net]
ipv4 = '\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}'
mac = '([0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}'
```

### Complete Example

```toml
name = "Production Log Processor"
description = "Sanitizes and normalizes production logs"
version = "2.0.0"
patterns_include = ["patterns/common.toml"]

[settings]
pcre_mode = false
timeout_ms = 10000
strict_mode = true

# Step 1: Normalize timestamps
[[step]]
type = "substitute"
description = "Standardize ISO8601 timestamps"
pattern = '(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})'
replacement = '${1}/${2}/${3} ${4}:${5}:${6}'
flags = ["global"]
enabled = true

# Step 2: Remove debug noise
[[step]]
type = "filter"
description = "Drop debug messages"
pattern = '\[DEBUG\]'
action = "drop_line"
enabled = true

# Step 3: Uppercase error levels
[[step]]
type = "transform"
description = "Uppercase log levels"
pattern = '\[(error|warn|info)\]'
transform = "uppercase"
flags = ["global", "case_insensitive"]
enabled = true

# Step 4: Redact sensitive data
[[step]]
type = "substitute"
description = "Mask credit card numbers"
pattern = '\b\d{4}[-\s]?\d{4}[-\s]?\d{4}[-\s]?\d{4}\b'
replacement = '****-****-****-XXXX'
flags = ["global"]
enabled = true

# Step 5: Add timestamp prefix via shell
[[step]]
type = "transform"
description = "Add processing timestamp"
pattern = '^'
transform.shell.command = "date '+[%Y-%m-%d %H:%M:%S] '"
enabled = false  # Disabled by default
```

## Command Line Options

```bash
rexpipe [OPTIONS] [paths]...

ARGUMENTS:
    [paths]...                    Files or directories to process

OPTIONS:
    # Core Processing
    -c, --config <FILE>           TOML configuration file
    -p, --pattern <REGEX>         Inline regex pattern
    -r, --replacement <TEXT>      Replacement text for substitution
    -F, --fixed                   Treat pattern as fixed string (no regex)
    -P, --pcre                    Use PCRE-compatible regex (lookahead/lookbehind)

    # File Operations
    -i, --input <FILE>            Input file (default: stdin)
    -o, --output <FILE>           Output file (default: stdout)
    -I, --in-place                Edit files in-place
    -b, --backup <SUFFIX>         Create backup with suffix when editing in-place

    # Multi-File Processing
    -R, --recursive               Recursively process directories
    -g, --glob <PATTERN>          Only process files matching glob pattern
    -e, --exclude <PATTERN>       Exclude files matching glob pattern
        --no-ignore               Don't respect .gitignore files
        --hidden                  Include hidden files
        --max-depth <NUM>         Maximum directory recursion depth
    -j, --parallel                Process files in parallel
        --progress                Show progress indicator

    # Output Modes
        --count                   Only show count of matches per file
    -l, --files-with-matches      Only list files containing matches
    -L, --files-without-matches   Only list files not containing matches
    -q, --quiet                   Quiet mode - only set exit code
        --json                    Force JSON output (default when piped)
        --text                    Force plain text output (override JSON default)
        --error-format <FMT>      Error output format: text (default) or json

    # Safety & Verification
        --explain                 Describe what pipeline will do (no processing)
        --verify                  Output verification summary after processing
        --apply                   Confirm in-place edits (required when scripted)

    # Context Lines
    -B, --before-context <NUM>    Show NUM lines before each match
    -A, --after-context <NUM>     Show NUM lines after each match
    -C, --context <NUM>           Show NUM lines before and after each match

    # Inspection & Debugging
        --inspect                 Enable inspection mode
        --interactive             Enable interactive inspection
        --dry-run                 Validate config, or preview changes with -I
        --performance             Show performance metrics

    # Pattern Libraries
        --list-patterns <FILE>    List patterns from a pattern library
        --validate-library <FILE> Validate a pattern library file

    # Utilities
        --validate                Validate configuration only
        --validate-config         Validate pipeline configuration file
        --export <FORMAT>         Export configuration (toml or json)
        --completions <SHELL>     Generate shell completion script
        --man                     Generate man page to stdout
    -h, --help                    Print help information
    -V, --version                 Print version information
```

## Watch Mode

With the `watch` feature, rexpipe can monitor files for changes and automatically re-run the pipeline:

```bash
# Install with watch support
cargo install rexpipe --features watch

# Watch log files for changes
rexpipe -c config.toml --watch ./logs/*.log

# Watch and apply in-place edits
rexpipe -c config.toml --watch --in-place ./data/*.txt
```

Press Ctrl+C to exit watch mode.

## JSON Output

All JSON output uses a standardized schema with metadata for forward compatibility:

```json
{
  "metadata": {
    "schema_version": "1.0",
    "mode": "count",
    "tool_version": "1.1.0"
  },
  "data": {
    "lines_processed": 2,
    "matches_found": 2,
    "transformations_applied": 0
  }
}
```

Supported modes: `count`, `processing`, `performance`, `multi_file`, `files_with_matches`, `files_without_matches`.

## Shell Completions

rexpipe supports generating shell completion scripts for bash, zsh, fish, PowerShell, and elvish:

```bash
# Bash - add to ~/.bashrc
rexpipe --completions bash >> ~/.bashrc

# Zsh - add to ~/.zshrc or create a completion file
rexpipe --completions zsh > ~/.zfunc/_rexpipe

# Fish - save to completions directory
rexpipe --completions fish > ~/.config/fish/completions/rexpipe.fish

# PowerShell - add to $PROFILE
rexpipe --completions powershell >> $PROFILE

# Elvish
rexpipe --completions elvish >> ~/.elvish/rc.elv
```

After generating, restart your shell or source the completion file to enable completions.

## Man Page

Generate and install the man page:

```bash
# Generate man page and save to file
rexpipe --man > rexpipe.1

# Install to system man pages (requires sudo)
sudo install -m 644 rexpipe.1 /usr/local/share/man/man1/

# Or install to user man pages
mkdir -p ~/.local/share/man/man1
install -m 644 rexpipe.1 ~/.local/share/man/man1/

# Update man database
sudo mandb  # System-wide
# or
mandb -c ~/.local/share/man  # User-specific

# View the man page
man rexpipe
```

## Exit Codes

rexpipe uses distinct exit codes for different error conditions:

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | No matches found (grep-like behavior) |
| 2 | Invalid usage / missing arguments |
| 3 | Configuration file error |
| 4 | Invalid regex pattern |
| 5 | File I/O error |
| 6 | Validation error |

## Performance Benefits

Traditional approach:
```bash
cat access.log | \
  sed 's/\[ERROR\]/[ERR]/g' | \
  grep -v 'DEBUG' | \
  sed 's/user_id=([0-9]+)/uid=\1/g' | \
  awk '{gsub(/192\.168\./, "10.0."); print}' | \
  sed 's/@company\.com/@domain\.com/g'
```

rexpipe approach:
```bash
cat access.log | rexpipe --config log-cleanup.toml
```

**Performance improvements:**
- **3-5x faster processing** on files >100MB
- **10-20x less RAM usage** on large files
- **Single process** eliminates inter-process communication overhead
- **Constant memory usage** regardless of file size

## Multi-File Processing

rexpipe supports processing multiple files with grep-like functionality:

### Basic Multi-File Operations
```bash
# Process all .log files in current directory
rexpipe -p 'ERROR' *.log

# Recursively search directories
rexpipe -p 'TODO' -R src/

# Filter by glob pattern
rexpipe -p 'FIXME' -R -g '*.rs' .

# Exclude patterns
rexpipe -p 'debug' -R -e '*.test.js' -e 'node_modules/*' src/
```

### Grep-Like Output Modes
```bash
# List only files containing matches
rexpipe -p 'password' -l -R .

# List files NOT containing a pattern
rexpipe -p 'Copyright' -L -R src/

# Count matches per file
rexpipe -p 'TODO|FIXME' --count -R src/

# Quiet mode (just set exit code)
rexpipe -p 'ERROR' -q server.log && echo "Found errors"
```

### In-Place Editing
```bash
# Edit files in-place
rexpipe -p 'old_api' -r 'new_api' -I -R -g '*.py' src/

# Create backups before editing
rexpipe -p 'localhost' -r 'production.example.com' -I -b .bak config/

# Preview changes with dry-run
rexpipe -p 'v1' -r 'v2' -I --dry-run -R -g '*.json' .
```

### Parallel Processing
```bash
# Process files in parallel for large codebases
rexpipe -p 'deprecated' -R -j --progress src/
```

## Examples

### Log Processing
```bash
# Clean server logs
rexpipe -c examples/log-cleanup.toml < server.log

# Debug pattern matching
rexpipe -p 'ERROR.*user_id=(\d+)' --inspect < server.log

# Extract only errors (using pattern library)
rexpipe -c examples/log-errors.toml < application.log

# Sanitize logs (redact PII using pattern library)
rexpipe -c examples/log-sanitize.toml < application.log
```

### Data Transformation
```bash
# Process CSV data
rexpipe -c examples/data-transform.toml < customers.csv

# Interactive pattern testing
rexpipe -p '(\w+),(\w+@[\w.]+)' --inspect --interactive < data.csv
```

### Multi-File Search
```bash
# Find all TODO comments in Rust files
rexpipe -p 'TODO|FIXME|HACK' -R -g '*.rs' src/

# Search with context lines
rexpipe -p 'panic!' -C 3 -R -g '*.rs' src/

# JSON output for scripting
rexpipe -p 'unsafe' -R -g '*.rs' --json src/
```

### In-Place Refactoring
```bash
# Rename a function across codebase
rexpipe -p 'old_function_name' -r 'new_function_name' -I -R -g '*.py' src/

# Update API version in configs
rexpipe -p 'api/v1' -r 'api/v2' -I -R -g '*.yaml' config/
```

### Performance Analysis
```bash
# Show processing metrics
rexpipe -c pipeline.toml --performance < large-file.txt
```

### Benchmarking and Profiling

Run benchmarks to measure performance:

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark group
cargo bench "simple_substitution"

# Generate HTML report (requires gnuplot)
cargo bench -- --noplot

# Benchmarks are in benches/processing_benchmark.rs
```

### Comparison with Other Tools

Compare rexpipe against sed, awk, and ripgrep:

```bash
# Requires hyperfine: cargo install hyperfine
./benches/compare_tools.sh

# The script tests:
# 1. Simple substitution (replacing digits)
# 2. Line filtering (grep-like matching)
# 3. Multi-step pipelines (3 transformations)
# 4. IP anonymization (complex patterns)
# 5. Capture group substitution (date reformatting)
```

**Performance characteristics:**

| Scenario | rexpipe vs alternatives |
|----------|------------------------|
| Simple patterns | Comparable (I/O bound) |
| Multi-step pipelines | Faster (single process, no pipes) |
| Complex regex | Comparable to ripgrep (Rust regex crate) |
| Large files | Constant memory (streaming) |
| Many small files | Faster with `-j` (parallel processing) |

For profiling with perf (Linux):

```bash
# Build with debug symbols in release mode
RUSTFLAGS="-C debuginfo=2" cargo build --release

# Profile with perf
perf record --call-graph dwarf ./target/release/rexpipe -c pipeline.toml < large-file.txt
perf report

# Flamegraph (requires cargo-flamegraph)
cargo flamegraph -- -c pipeline.toml < large-file.txt
```

### Dry-Run Preview
```bash
# Preview changes before modifying files in-place
rexpipe -p 'old_value' -r 'new_value' -I --dry-run src/

# Preview shows unified diff of all changes that would be made
# without actually modifying any files
```

### Debugging with Structured Logging

rexpipe uses the `RUST_LOG` environment variable for structured logging. This is helpful for debugging pipeline execution, understanding file discovery, and diagnosing issues.

```bash
# Enable debug-level logging for rexpipe
RUST_LOG=rexpipe=debug rexpipe -c pipeline.toml < input.txt

# Enable trace-level logging (most verbose, includes per-line processing)
RUST_LOG=rexpipe=trace rexpipe -c pipeline.toml < input.txt

# Enable debug for specific modules
RUST_LOG=rexpipe::files=debug,rexpipe::processor=trace rexpipe -c pipeline.toml src/

# Combine with quiet mode to see only logs, not output
RUST_LOG=rexpipe=debug rexpipe -c pipeline.toml -q < input.txt
```

**Log levels:**
- `error`: Critical failures only
- `warn`: Warnings and errors (default)
- `info`: High-level operations (file discovery counts, processing summaries)
- `debug`: Detailed operations (configuration loading, file-by-file processing)
- `trace`: Per-line processing details (most verbose)

Logs are written to stderr, so they don't interfere with pipeline output on stdout.

## Edge Cases and Behavior

This section documents how rexpipe handles various edge cases.

### Empty Files

- Empty files (0 bytes) are processed without error
- Processing returns with 0 matches found
- In-place editing on empty files creates an empty output
- Empty lines within files are processed normally (filters can match `^$`)

### Large Files and Long Lines

- **Streaming architecture**: Files are processed line-by-line, so memory usage remains constant regardless of file size
- **Very long lines**: Lines of any length are supported, limited only by available memory
- **Recommendation**: For files with extremely long lines (>10MB per line), consider preprocessing to add line breaks

### Timeout Behavior

- **Shell transforms** have a configurable timeout (default: varies by setting)
- **Timeout setting**: Use `settings.timeout_ms` in TOML configs to control shell command timeout
- **On timeout**: Shell transform returns the original matched text unchanged
- **Progress feedback**: Long-running operations show progress when `--progress` is used

### Binary Files

- rexpipe is designed for text processing
- Binary files may produce unexpected results
- Use `--glob` patterns to exclude binary files (e.g., `-e '*.exe' -e '*.bin'`)

### Unicode and Encoding

- Input is expected to be valid UTF-8
- Invalid UTF-8 sequences are replaced with the Unicode replacement character (U+FFFD)
- Unicode patterns are fully supported in both standard and PCRE modes
- Named character classes (e.g., `\pL` for letters) work in standard mode

### In-Place Editing Safety

- **Atomic writes**: Files are written to a temporary file first, then renamed
- **No partial writes**: If the process is interrupted, original files remain intact
- **Backup option**: Use `--backup <suffix>` to keep original files
- **Dry-run preview**: Use `--dry-run` with `-I` to see what would change before committing

### Circular Library Includes

- rexpipe detects circular library includes (A includes B, B includes A)
- Deep nesting is limited (default max depth: 32)
- Clear error messages indicate the include chain when circular references are detected

### Zero Matches

- Exit code 1 is returned when no matches are found (grep-like behavior)
- Use `-q` (quiet) to suppress output and only check exit code
- When processing multiple files, the exit code reflects whether any file had matches

### Parallel Processing Thresholds

- Parallel processing (`-j`) is only used when file count exceeds a threshold (default: 4 files)
- This avoids overhead for small file sets where sequential is faster

### Streaming Architecture

rexpipe uses a **file-level parallelism** model rather than intra-file streaming:

- **Line-by-line processing**: Each file is processed line-by-line with constant memory
- **Parallel at file level**: Multiple files can be processed concurrently with `-j`
- **Sequential within files**: Lines within a single file are processed sequentially

This design provides:
- **Predictable memory usage**: O(max_line_length), not O(file_size)
- **Simple error handling**: Errors are scoped to individual files
- **Correctness for multi-line patterns**: Block processing requires seeing lines in order
- **Efficient for typical workloads**: Most text processing is I/O-bound, not CPU-bound

For extremely large single files (multi-GB), consider:
- Using `split` to divide the file if patterns don't span lines
- Using `--stream` mode for continuous processing
- Piping through `parallel` for chunk-based processing

### Symlinks and Path Traversal

- **Symlinks are not followed**: For security, rexpipe does not follow symbolic links during directory traversal
- **No path traversal attacks**: Malicious symlinks pointing outside the intended directory tree are ignored
- **Symlink files**: Symlinks to files are listed but not dereferenced (the symlink itself is matched, not the target)
- **In-place editing**: If a path passed directly is a symlink, rexpipe operates on the symlink's target (standard behavior for `File::open`)

This design prevents:
- Infinite loops from circular symlinks
- Accidental modification of files outside the working tree
- Directory escape attacks via crafted symlinks

### Stdin Behavior

When reading from stdin without piped input:
- rexpipe shows a helpful message: "Reading from stdin..."
- Press Ctrl+D (Unix) or Ctrl+Z (Windows) to signal end of input
- Press Ctrl+C to cancel

**For scripts that need to fail fast on empty stdin:**
```bash
# Use timeout command (Unix)
timeout 5s rexpipe -p '\d+' -r 'X' < /dev/stdin || echo "No input received"

# Check if stdin has data before processing
if [ -t 0 ]; then
    echo "Error: No input piped to stdin" >&2
    exit 1
fi
rexpipe -p '\d+' -r 'X'
```

### Graceful Shutdown

- rexpipe handles Ctrl+C (SIGINT) and SIGTERM signals gracefully
- When interrupted, in-progress files complete normally to avoid leaving files in a partial state
- Files that haven't started processing are skipped
- Progress bar shows "Interrupted" status with counts of completed and remaining files
- In-place edits use atomic writes, so even interrupted operations won't corrupt files

## Testing

```bash
# Run all tests
cargo test

# Run benchmarks
cargo bench

# Test with sample data
rexpipe --config examples/log-cleanup.toml --inspect < test-data/sample.log
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Run `cargo test` and `cargo fmt`
6. Submit a pull request

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Architecture

Built with:
- **Rust**: Memory safety and performance
- **regex crate**: Fast regex engine for standard patterns
- **fancy-regex crate**: PCRE-compatible regex (lookahead/lookbehind support)
- **TOML**: Human-readable configuration
- **clap**: Command-line interface
- **termcolor**: Colored output for debugging

### Regex Engine Options

rexpipe supports multiple regex modes:

1. **Standard mode** (default): Uses the fast Rust `regex` crate
2. **PCRE mode** (`-P/--pcre`): Uses `fancy-regex` for PCRE-compatible patterns with lookahead/lookbehind support
3. **Fixed string mode** (`-F/--fixed`): Treats patterns as literal strings (fastest)

To enable PCRE mode features, build with:
```bash
cargo build --release --features pcre
```

### ReDoS Protection

rexpipe includes protection against Regular Expression Denial of Service (ReDoS) attacks:

**Standard mode** uses the Rust `regex` crate which guarantees **O(m × n) linear time** matching. This eliminates catastrophic backtracking vulnerabilities found in traditional regex engines.

**Built-in safeguards:**
- **Size limits**: Compiled regex size is limited to 10MB to prevent compilation DoS
- **DFA size limits**: Deterministic finite automaton size is capped to prevent memory exhaustion
- **Pattern analysis**: PCRE mode patterns are analyzed for common ReDoS indicators (nested quantifiers, excessive alternations)

**PCRE mode warning**: Unlike standard mode, PCRE mode uses a backtracking engine and can be vulnerable to ReDoS. Patterns like `(a+)+`, `(a*)*`, or deeply nested quantifiers will trigger warnings. For untrusted input, prefer standard mode.

### Named Capture Groups

rexpipe supports named capture groups in patterns, allowing you to reference captured text by name in replacements:

```bash
# Using named capture groups: (?P<name>pattern)
echo "John Doe" | rexpipe -p '(?P<first>\w+)\s+(?P<last>\w+)' -r '${last}, ${first}'
# Output: Doe, John

# Mixing named and numbered captures
echo "user: admin, id: 12345" | rexpipe -p '(?P<role>\w+), id: (\d+)' -r 'ID $2 is ${role}'
# Output: user: ID 12345 is admin
```

**Reference syntax in replacements:**
- `${name}` - Reference a named capture group
- `$1`, `$2`, etc. - Reference numbered capture groups
- `$0` - Reference the entire match

Named capture groups work in both standard mode and PCRE mode.

### Async Processing (Optional)

For non-blocking async file processing, build with the `async` feature:
```bash
cargo build --release --features async
```

This enables the `AsyncMultiFileProcessor` for concurrent file operations using tokio.

## Block Step Type

The `block` step type enables cross-line pattern matching using a state machine approach. This is useful for extracting or processing multi-line patterns like stack traces, log entries, or delimited records.

### Block Step Configuration

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `pattern` | string | Yes | - | Trigger pattern that starts a block |
| `until` | string | No | - | Pattern that ends a block |
| `block_action` | string/table | Yes | - | Action to apply to lines in the block |
| `block_context` | integer | No | 0 | Number of context lines after trigger |

### Block Actions

- **keep_block**: Keep only lines within matching blocks
- **drop_block**: Drop lines within matching blocks
- **mark_block**: Prepend a marker to lines within blocks
- **substitute_in_block**: Apply a substitution only within blocks
- **collect_block**: Collect and output block contents together

### Example: Extract Stack Traces

```toml
[[step]]
type = "block"
pattern = "^Exception:"
until = "^\\s*at\\s+.*\\)$"
block_action = "keep_block"
description = "Extract Java stack traces"
```

### Example: Mark Log Sections

```toml
[[step]]
type = "block"
pattern = "^=== START TRANSACTION ==="
until = "^=== END TRANSACTION ==="
block_action = { mark_block = { marker = ">>> " } }
description = "Mark transaction log entries"
```

## Pattern Discovery Mode

Pattern discovery analyzes input to detect common patterns and suggests pipeline configurations.

```bash
# Analyze input for patterns
cat data.txt | rexpipe --discover

# Example output:
# Pattern Discovery Results:
# ========================
# email (12 matches): [a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}
# ipv4 (5 matches): \b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b
# uuid (3 matches): [0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}
#
# Suggested pipeline:
# [[step]]
# type = "substitute"
# pattern = "[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}"
# replacement = "[REDACTED_EMAIL]"
```

Detected pattern types include: email, IPv4, IPv6, UUID, dates, URLs, phone numbers, credit cards, SSN, API keys, and more.

## Git Filter Integration

rexpipe can be configured as a git clean/smudge filter for automatic file transformation on commit/checkout.

### Setup

```bash
# Generate git filter configuration
rexpipe --git-filter-setup my-filter

# This outputs:
# Add to .git/config:
#   [filter "my-filter"]
#     clean = rexpipe --config .rexpipe/my-filter.toml
#     smudge = rexpipe --config .rexpipe/my-filter.toml --reverse
#
# Add to .gitattributes:
#   *.log filter=my-filter
```

### Use Cases

- **Sanitize logs before commit**: Remove sensitive data automatically
- **Format normalization**: Consistent line endings, whitespace
- **Environment substitution**: Replace placeholders with environment-specific values

## Format-Preserving Encryption (FPE)

FPE allows reversible encryption that preserves the format of the original data. Requires the `fpe` feature.

```bash
cargo build --release --features fpe
```

### FPE Configuration Options

| Option | Type | Description |
|--------|------|-------------|
| `key` | string | Hex-encoded AES key (inline) |
| `key_file` | string | Path to file containing the key |
| `tweak` | string | Optional tweak value (inline) |
| `tweak_file` | string | Path to file containing the tweak |
| `radix` | string | Character set for encryption (default: `"0123456789"`) |

**Note:** Use either `key` or `key_file`, not both. Same for `tweak`/`tweak_file`.

### FPE Encrypt

```toml
# Using inline key
[[step]]
type = "transform"
pattern = '\b(\d{4})-(\d{4})-(\d{4})-(\d{4})\b'
transform = { fpe_encrypt = {
    key = "0123456789ABCDEF0123456789ABCDEF",  # Hex-encoded AES key
    tweak = "",                                 # Optional tweak
    radix = "0123456789"                        # Character set
}}
description = "Encrypt credit card numbers (digits remain digits)"

# Using external key file (recommended for production)
[[step]]
type = "transform"
pattern = '\b(\d{4})-(\d{4})-(\d{4})-(\d{4})\b'
transform = { fpe_encrypt = {
    key_file = "/etc/rexpipe/fpe.key",         # Path to key file
    tweak_file = "/etc/rexpipe/fpe.tweak",     # Optional tweak file
    radix = "0123456789"
}}
description = "Encrypt credit card numbers using external key"
```

### FPE Decrypt

```toml
[[step]]
type = "transform"
pattern = '\b(\d{4})-(\d{4})-(\d{4})-(\d{4})\b'
transform = { fpe_decrypt = {
    key_file = "/etc/rexpipe/fpe.key",
    radix = "0123456789"
}}
description = "Decrypt credit card numbers"
```

### FPE Security Best Practices

**Key Management:**

1. **Never hardcode keys in config files that are committed to version control.** Use `key_file` instead of inline `key` for any non-development environment.

2. **Set restrictive file permissions on key files:**
   ```bash
   chmod 600 /etc/rexpipe/fpe.key
   chown root:root /etc/rexpipe/fpe.key
   ```

3. **Generate cryptographically secure keys:**
   ```bash
   # Generate 256-bit key (32 bytes = 64 hex characters)
   openssl rand -hex 32 > /etc/rexpipe/fpe.key

   # Generate 128-bit key (16 bytes = 32 hex characters)
   openssl rand -hex 16 > /etc/rexpipe/fpe.key
   ```

4. **Key length requirements:**
   - AES-128: 16 bytes (32 hex characters)
   - AES-192: 24 bytes (48 hex characters)
   - AES-256: 32 bytes (64 hex characters)

**Tweak Usage:**

- Tweaks add domain separation - use different tweaks for different data types
- Tweaks can be public (unlike keys) but should be consistent per data domain
- Missing or empty tweak is valid but reduces security isolation

**Operational Security:**

- **Rotate keys periodically** - maintain old keys for decryption during transition
- **Audit key access** - log when key files are read
- **Backup keys securely** - encrypted backups with tested recovery procedures
- **Environment separation** - use different keys for dev/staging/production

**What NOT to do:**

```toml
# BAD: Hardcoded key in committed config
transform = { fpe_encrypt = { key = "0123456789ABCDEF..." }}

# BAD: Key file in repository
transform = { fpe_encrypt = { key_file = "./keys/fpe.key" }}

# BAD: World-readable key file
# chmod 644 /etc/rexpipe/fpe.key  # DON'T DO THIS
```

**Recommended pattern:**

```toml
# GOOD: External key file with restricted access
transform = { fpe_encrypt = {
    key_file = "/etc/rexpipe/fpe.key",
    tweak_file = "/etc/rexpipe/fpe.tweak",
    radix = "0123456789"
}}
```

## Deterministic Masking

Deterministic masking produces consistent output for the same input, allowing masked data to be joined across datasets.

```toml
# Using inline seed
[[step]]
type = "transform"
pattern = '\d{3}-\d{2}-\d{4}'
transform = { mask_deterministic = {
    seed = "my-secret-seed",
    preserve_prefix = 0,
    preserve_suffix = 4,
    mask_char = "X"
}}
description = "Mask SSN, keeping last 4 digits"
# Input:  123-45-6789
# Output: XXX-XX-6789

# Using external seed file (recommended for production)
[[step]]
type = "transform"
pattern = '\d{3}-\d{2}-\d{4}'
transform = { mask_deterministic = {
    seed_file = "/etc/rexpipe/masking.seed",
    preserve_suffix = 4,
    mask_char = "X"
}}
description = "Mask SSN using external seed file"
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `seed` | string | - | Seed for deterministic hashing (inline) |
| `seed_file` | string | - | Path to file containing the seed |
| `preserve_prefix` | integer | 0 | Keep first N characters unchanged |
| `preserve_suffix` | integer | 0 | Keep last N characters unchanged |
| `mask_char` | char | '*' | Character to use for masking |

**Note:** Use either `seed` or `seed_file`, not both. One must be specified.

**Security:** The seed acts as a secret key - identical input + identical seed = identical output. Protect the seed like a password:
- Use `seed_file` with restricted permissions (chmod 600) for production
- Never commit seeds to version control
- Use different seeds for different environments (dev/prod)
- Treat seed compromise as requiring re-masking of all data

## Syntax-Aware Processing (Optional)

Syntax-aware processing uses tree-sitter to parse code and apply patterns only within specific scopes (code, strings, comments, functions). Requires the `tree-sitter` feature.

```bash
cargo build --release --features tree-sitter
```

### Configuration

| Field | Type | Description |
|-------|------|-------------|
| `language` | string | Single language for parsing: rust, python, javascript, typescript, go, json, yaml |
| `languages` | array | Multiple languages: `["rust", "python", "typescript"]` |
| `scope` | string | Where to apply patterns (see Supported Scopes below) |
| `exclude_scopes` | array | Scopes to exclude: `["comments", "strings"]` |

### Example: Rename Function in Code Only

```toml
[[step]]
type = "substitute"
pattern = "old_function"
replacement = "new_function"
language = "rust"
scope = "code"
description = "Rename function in code, not in strings or comments"
```

### Example: Multi-Language Refactoring

```toml
[[step]]
type = "substitute"
pattern = "deprecated_api"
replacement = "new_api"
languages = ["rust", "python", "typescript"]
scope = "function_calls"
description = "Update API calls across multiple languages"
```

### Example: Exclude Specific Scopes

```toml
[[step]]
type = "substitute"
pattern = "TODO"
replacement = "FIXME"
language = "rust"
exclude_scopes = ["strings", "comments"]
description = "Replace TODO with FIXME, but only in code"
```

Given this Rust code:
```rust
fn old_function() {
    // Call old_function here
    let s = "old_function";
    old_function();
}
```

Result:
```rust
fn new_function() {           // <- renamed
    // Call old_function here  // <- unchanged (comment)
    let s = "old_function";    // <- unchanged (string)
    new_function();            // <- renamed
}
```

### Supported Scopes

| Scope | Description |
|-------|-------------|
| `all` | Match anywhere (default) |
| `code` | Match only in code (exclude strings and comments) |
| `strings` | Match only in string literals |
| `comments` | Match only in comments |
| `functions` | Match only in function/method definitions |
| `function_calls` | Match only in function/method calls |
| `imports` | Match only in import/use statements |
| `types` | Match only in type annotations |
| `identifiers` | Match only in identifiers |
| `macros` | Match only in macro invocations |
| `control_flow` | Match only in control flow (if, for, while, match) |
| `tests` | Match only in test code (see below) |

### Tests Scope

The `tests` scope identifies test-related code in a language-aware manner:

| Language | Detection Method |
|----------|------------------|
| **Rust** | Functions with `#[test]`, `#[tokio::test]` attributes; `mod tests` blocks |
| **Python** | Functions starting with `test_`; classes starting with `Test` |
| **JavaScript/TypeScript** | `describe()`, `it()`, `test()`, `beforeEach()`, etc. |
| **Go** | Functions starting with `Test`, `Benchmark`, or `Example` |

```toml
# Exclude test code from refactoring
[[step]]
type = "substitute"
pattern = "old_api"
replacement = "new_api"
language = "rust"
exclude_scopes = ["tests", "comments", "strings"]
description = "Update API calls in production code only"
```

## Acknowledgments

Thank you to all contributors and the Rust community.