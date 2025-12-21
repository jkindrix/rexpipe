# AI Agent Cookbook

Practical recipes for AI agents using rexpipe for text processing tasks.

## Quick Start

rexpipe is designed for AI agents. Key behaviors:

1. **JSON output by default** when piped (not TTY)
2. **Structured errors** with `--error-format json`
3. **Preview before modify** with `--explain` and `--dry-run`
4. **Explicit confirmation** with `--apply` for in-place edits

## Recipe 1: Extract Code from LLM Response

**Task**: Extract code blocks from markdown-formatted LLM output.

```bash
# Extract all fenced code blocks
echo "$llm_response" | rexpipe -p '```[a-z]*\n([\s\S]*?)\n```' --extract

# Using pattern library
echo "$llm_response" | rexpipe -c - <<'EOF'
patterns_include = ["patterns/ai.toml"]
[[steps]]
type = "extract"
pattern = '${code.markdown_block}'
EOF
```

**JSON Output:**
```json
{
  "schema_version": "1.0",
  "matches": [
    {"line": 3, "text": "def hello():\n    print(\"Hello\")"}
  ]
}
```

## Recipe 2: Redact PII from Text

**Task**: Remove personally identifiable information before processing.

```bash
# Multi-step PII redaction
cat document.txt | rexpipe -c - <<'EOF'
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
EOF
```

**Verify what was redacted:**
```bash
cat document.txt | rexpipe -c pipeline.toml --verify
```

## Recipe 3: Detect Secrets in Code

**Task**: Scan files for exposed credentials.

```bash
# Scan for secrets, output as JSON
rexpipe -r -p '\b(AKIA[0-9A-Z]{16}|ghp_[a-zA-Z0-9]{36}|sk-[a-zA-Z0-9]{48})\b' src/

# Using pattern library for comprehensive scan
rexpipe -c - -r src/ <<'EOF'
patterns_include = ["patterns/ai.toml"]
[[steps]]
type = "filter"
mode = "lines"
pattern = '${secrets.api_key}'
keep = true
EOF
```

**JSON Output:**
```json
{
  "schema_version": "1.0",
  "files_with_matches": ["src/config.js"],
  "matches": [
    {"file": "src/config.js", "line": 42, "text": "apiKey = 'AKIA..."}
  ]
}
```

## Recipe 4: Parse and Filter Logs

**Task**: Extract error entries from mixed log output.

```bash
# Get only error-level log lines
cat app.log | rexpipe -c - <<'EOF'
patterns_include = ["patterns/logs.toml"]
[[steps]]
type = "filter"
mode = "lines"
pattern = '${level.error}'
keep = true
EOF
```

**Task**: Extract stack traces from logs.

```bash
# Filter Java exceptions
cat app.log | rexpipe -p '^(?:\s+at\s+|Caused by:|Exception|Error).*' --filter
```

## Recipe 5: Clean and Normalize Text

**Task**: Prepare text for LLM input (normalize whitespace, remove artifacts).

```bash
cat messy.txt | rexpipe -c - <<'EOF'
patterns_include = ["patterns/ai.toml"]

# Collapse multiple spaces to single space
[[steps]]
type = "substitute"
pattern = '${clean.multiple_spaces}'
replacement = " "

# Collapse multiple newlines to double
[[steps]]
type = "substitute"
pattern = '${clean.multiple_newlines}'
replacement = "\n\n"

# Remove trailing whitespace
[[steps]]
type = "substitute"
pattern = '${clean.trailing_whitespace}'
replacement = ""

# Remove ANSI color codes
[[steps]]
type = "substitute"
pattern = '${clean.ansi_codes}'
replacement = ""
EOF
```

## Recipe 6: Extract Structured Data

**Task**: Extract key-value pairs from configuration output.

```bash
# Extract environment variable assignments
cat .env | rexpipe -p '([A-Z_]+)=(.+)' --extract --json

# Extract JSON fields
cat data.json | rexpipe -p '"(\w+)":\s*"([^"]*)"' --extract
```

## Recipe 7: Validate Data Format

**Task**: Check if all lines match expected format.

```bash
# Validate email list
cat emails.txt | rexpipe -c - <<'EOF'
patterns_include = ["patterns/ai.toml"]
[[steps]]
type = "validate"
pattern = '${validate.valid_email}'
on_fail = "error"
EOF

# Exit code: 0 = all valid, 1 = validation failures
echo $?
```

## Recipe 8: Safe File Modification

**Task**: Modify files with verification and rollback capability.

```bash
# Step 1: Explain what will happen
rexpipe -p 'oldFunction' -r 'newFunction' -i src/*.js --explain

# Step 2: Preview changes (dry-run)
rexpipe -p 'oldFunction' -r 'newFunction' -i src/*.js --dry-run

# Step 3: Apply with verification
rexpipe -p 'oldFunction' -r 'newFunction' -i src/*.js --apply --verify
```

**Explanation output (JSON):**
```json
{
  "schema_version": "1.0",
  "description": "Replace pattern 'oldFunction' with 'newFunction'",
  "files_affected": ["src/main.js", "src/utils.js"],
  "estimated_replacements": 12
}
```

## Recipe 9: Process Multiple File Types

**Task**: Apply different transformations based on file extension.

```bash
# Process only Python files
rexpipe -p 'print\(' -r 'logger.info(' -i -r --include '*.py' src/ --apply

# Exclude test files
rexpipe -p 'DEBUG' -r 'INFO' -i -r --exclude '*_test.py' src/ --apply
```

## Recipe 10: Chain with Other Tools

**Task**: Integrate rexpipe into larger pipelines.

```bash
# Git diff → rexpipe → count changes
git diff --name-only | rexpipe -p '\.py$' --filter | wc -l

# Find files → rexpipe → JSON output for further processing
find . -name '*.log' | xargs rexpipe -p 'ERROR' --count --json

# jq + rexpipe for JSON log processing
cat logs.jsonl | jq -r '.message' | rexpipe -p 'timeout' --filter
```

## Error Handling

**Always use structured errors for programmatic handling:**

```bash
rexpipe -p '[invalid(' input.txt --error-format json 2>&1
```

**Error output:**
```json
{
  "schema_version": "1.0",
  "error": {
    "category": "regex",
    "message": "Invalid regex pattern",
    "details": "unclosed group at position 8",
    "suggestion": "Check for unmatched parentheses or brackets"
  },
  "exit_code": 2
}
```

**Exit codes:**
| Code | Meaning |
|------|---------|
| 0 | Success, matches found |
| 1 | Success, no matches |
| 2 | Error (invalid regex, file not found, etc.) |

## Best Practices for AI Agents

### 1. Always Preview First

```bash
# Before modifying files, understand the scope
rexpipe -p 'pattern' -r 'replacement' files --explain
rexpipe -p 'pattern' -r 'replacement' files --dry-run
```

### 2. Use Pattern Libraries

```bash
# Reference tested patterns instead of writing regex
pattern = '${pii.email}'  # Not '[a-zA-Z0-9...]+'
```

### 3. Capture Verification Output

```bash
# Confirm what was done
result=$(rexpipe -c pipeline.toml input.txt --verify --json)
matches=$(echo "$result" | jq '.matches_found')
```

### 4. Handle Errors Gracefully

```bash
# Check exit code and parse error JSON
if ! output=$(rexpipe -p 'pattern' file.txt 2>&1); then
  error=$(echo "$output" | jq -r '.error.message')
  suggestion=$(echo "$output" | jq -r '.error.suggestion')
fi
```

### 5. Prefer Explicit Over Implicit

```bash
# Explicit: --apply confirms intent
rexpipe -i file.txt -p 'old' -r 'new' --apply

# Implicit: might preview instead of modify (safe default)
rexpipe -i file.txt -p 'old' -r 'new'
```

## Common Patterns Reference

| Task | Pattern |
|------|---------|
| Email | `${pii.email}` |
| Phone | `${pii.phone}` |
| SSN | `${pii.ssn}` |
| Credit Card | `${pii.credit_card}` |
| IPv4 | `${net.ipv4}` |
| UUID | `${common.uuid}` |
| URL | `${common.url}` |
| API Key | `${secrets.api_key}` |
| JWT | `${secrets.jwt}` |
| Code Block | `${code.markdown_block}` |
| Error Log | `${level.error}` |

See `patterns/INDEX.md` for complete pattern reference.
