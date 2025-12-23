# Advanced Features Guide

This guide covers rexpipe's advanced capabilities that go beyond basic pattern matching and substitution.

## Table of Contents

1. [Pattern Learning](#pattern-learning)
2. [Pattern Discovery](#pattern-discovery)
3. [Bidirectional Transformations](#bidirectional-transformations)
4. [Syntax-Aware Matching](#syntax-aware-matching)
5. [Cross-File Operations](#cross-file-operations)
6. [Checkpointing & Incremental Processing](#checkpointing--incremental-processing)
7. [Pipeline Networks](#pipeline-networks)

---

## Pattern Learning

**Learn regex patterns from examples instead of writing them manually.**

When you know *what* you want to match but not *how* to write the regex, pattern learning infers patterns from positive and negative examples.

### Basic Usage

```bash
# Provide examples of what should match (--positive) and what shouldn't (--negative)
rexpipe --learn \
  --positive "user@example.com" \
  --positive "admin@company.org" \
  --positive "test123@domain.net" \
  --negative "not-an-email" \
  --negative "@invalid"
```

**Output:**
```
Learned patterns:

1. Pattern: [a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}
   Confidence: 100%
   Description: Matches 3/3 positive examples

Suggested pipeline configuration:
[[step]]
type = "substitute"
pattern = "[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}"
replacement = "[REDACTED]"
```

### Learning Phone Numbers

```bash
rexpipe --learn \
  --positive "555-123-4567" \
  --positive "(555) 123-4567" \
  --positive "555.123.4567" \
  --negative "12345" \
  --negative "phone"
```

### Learning Custom ID Formats

```bash
rexpipe --learn \
  --positive "USR-12345-A" \
  --positive "USR-67890-B" \
  --positive "USR-11111-C" \
  --negative "USER12345" \
  --negative "12345-A"
```

### Programmatic Usage

```rust
use rexpipe::learn::PatternLearner;

let mut learner = PatternLearner::new();
learner.add_positive("user@example.com");
learner.add_positive("admin@company.org");
learner.add_negative("not-an-email");

let patterns = learner.learn().unwrap();
println!("Best pattern: {}", patterns[0].pattern);
```

---

## Pattern Discovery

**Automatically detect patterns in your data without providing examples.**

Pattern discovery analyzes input text and identifies common structures like emails, IPs, dates, phone numbers, API keys, and more.

### Basic Usage

```bash
cat data.txt | rexpipe --discover
```

### Example

```bash
echo "Contact: john@example.com, Phone: 555-123-4567
Server: 192.168.1.100, Date: 2024-01-15
User ID: usr_12345, API Key: sk_live_abc123def456" | rexpipe --discover
```

**Output:**
```
Pattern Discovery Report
========================
Analyzed 3 lines

Detected Patterns:

  email (1 matches)
    Pattern: [a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}
    Examples: john@example.com

  ipv4 (1 matches)
    Pattern: \b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b
    Examples: 192.168.1.100

  phone_us (1 matches)
    Pattern: \b\d{3}[-.]?\d{3}[-.]?\d{4}\b
    Examples: 555-123-4567

  date_iso (1 matches)
    Pattern: \b\d{4}-\d{2}-\d{2}\b
    Examples: 2024-01-15

  api_key (1 matches)
    Pattern: \b[A-Za-z0-9_-]{20,}\b
    Examples: sk_live_abc123def456
```

### Use Cases

- **Data Audit**: Discover what sensitive data exists in files
- **Pipeline Bootstrap**: Generate initial pipeline from discovered patterns
- **Compliance Check**: Find PII that needs redaction

---

## Bidirectional Transformations

**Transform data forward, then reverse the transformation later.**

Bidirectional mode records mappings during transformation, allowing you to restore original values.

### Recording Mappings (Forward)

```bash
# Transform and record mappings
cat data.txt | rexpipe -c redact.toml --mapping-file mappings.json > redacted.txt
```

### Restoring Original Values (Reverse)

```bash
# Reverse the transformation using recorded mappings
cat redacted.txt | rexpipe -c redact.toml --reverse --mapping-file mappings.json > restored.txt
```

### Example: Reversible Redaction

```toml
# redact-reversible.toml
name = "reversible-redact"

[bidirectional]
enabled = true
mapping_file = "mappings.json"

[[step]]
type = "substitute"
pattern = "[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}"
replacement = "[EMAIL_${seq}]"
flags = ["global"]

[[step]]
type = "substitute"
pattern = "\\b\\d{3}-\\d{2}-\\d{4}\\b"
replacement = "[SSN_${seq}]"
flags = ["global"]
```

**Variables available in replacements:**
- `${seq}` - Per-step sequence counter (resets for each step)
- `${count}` - Global match counter across all steps

**Workflow:**
```bash
# Step 1: Redact for sharing (forward direction is default)
echo "Contact: john@acme.com, SSN: 123-45-6789" | \
  rexpipe -c redact-reversible.toml

# Output: Contact: [EMAIL_1], SSN: [SSN_1]
# mappings.json is automatically saved with original → redacted mappings

# Step 2: Create a reverse config (same steps, direction = reverse)
cat > restore.toml << 'EOF'
name = "restore-redact"

[bidirectional]
enabled = true
direction = "reverse"
mapping_file = "mappings.json"

[[step]]
type = "substitute"
pattern = "[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}"
replacement = "[EMAIL_${seq}]"
flags = ["global"]

[[step]]
type = "substitute"
pattern = "\\b\\d{3}-\\d{2}-\\d{4}\\b"
replacement = "[SSN_${seq}]"
flags = ["global"]
EOF

# Step 3: Restore original values
echo "Contact: [EMAIL_1], SSN: [SSN_1]" | \
  rexpipe -c restore.toml

# Output: Contact: john@acme.com, SSN: 123-45-6789
```

### Use Cases

- **Secure Data Sharing**: Redact for external sharing, restore for internal use
- **Testing**: Anonymize production data, restore after testing
- **Audit Trail**: Track all transformations for compliance

---

## Syntax-Aware Matching

**Match patterns only in specific code constructs (code, strings, comments).**

Using tree-sitter parsing, rexpipe can limit matches to specific syntax scopes, avoiding false positives.

### Scopes

| Scope | Description |
|-------|-------------|
| `code` | Match only in executable code, not strings or comments |
| `string` | Match only within string literals |
| `comment` | Match only within comments |
| `function` | Match only within function bodies |
| `test` | Match only within test functions/blocks |

### Supported Languages

`rust`, `python`, `javascript`, `typescript`, `go`, `c`, `cpp`, `java`, `ruby`

### Example: Rename Function Only in Code

```bash
# Rename 'oldFunc' to 'newFunc' only in code, not in strings or comments
rexpipe -p 'oldFunc' -r 'newFunc' --scope code --language python src/*.py
```

### Example: Find TODOs Only in Comments

```bash
# Extract TODOs from comments only
rexpipe -p 'TODO:.*' --scope comment --language rust src/*.rs
```

### Pipeline Configuration

```toml
[[step]]
type = "substitute"
pattern = "deprecated_api"
replacement = "new_api"
scope = "code"
language = "javascript"
```

### Use Cases

- **Safe Refactoring**: Rename symbols without changing string content
- **Documentation Extraction**: Pull comments for doc generation
- **Code Analysis**: Analyze only executable code

---

## Cross-File Operations

**Ensure consistency across related files.**

Cross-file rules define patterns that should be synchronized across file sets (e.g., API versions in source and tests).

### Configuration

```toml
# cross-file-rules.toml

[[cross_file_rule]]
name = "api-version-sync"
description = "Ensure API version matches in source and tests"
trigger_pattern = "api/v2/"
trigger_files = "src/**/*.ts"
related_files = "test/**/*.test.ts"
ensure_pattern = "api/v2/"
action = "fail"  # or "warn", "fix", "skip"

[[cross_file_rule]]
name = "config-env-sync"
description = "Environment variables should match between configs"
trigger_pattern = "DATABASE_URL"
trigger_files = ".env.example"
related_files = ".env.production"
ensure_pattern = "DATABASE_URL"
action = "warn"
```

### Usage

```bash
# Check cross-file consistency
rexpipe --cross-file cross-file-rules.toml -R .

# Output:
# ✓ api-version-sync: All 15 related files match
# ⚠ config-env-sync: .env.production missing DATABASE_URL
```

### Actions

| Action | Behavior |
|--------|----------|
| `warn` | Log warning, continue processing |
| `fail` | Stop pipeline with error |
| `fix` | Auto-apply transformation to related files |
| `skip` | Skip file, continue with others |

### Use Cases

- **API Consistency**: Ensure API versions match across codebase
- **Config Sync**: Verify environment configs are complete
- **Test Coverage**: Ensure all features have corresponding tests
- **Monorepo Coordination**: Keep shared dependencies aligned

---

## Checkpointing & Incremental Processing

**Resume interrupted processing and handle growing files efficiently.**

### Basic Checkpointing

```bash
# First run - processes entire file, saves checkpoint
rexpipe -c pipeline.toml --checkpoint state.json < large.log > output.txt

# Second run - only processes new content since checkpoint
rexpipe -c pipeline.toml --checkpoint state.json --resume < large.log >> output.txt
```

### Watching Growing Log Files

```bash
# Process new log entries as they arrive
tail -f /var/log/app.log | rexpipe -c alerts.toml --checkpoint /var/lib/rexpipe/app.json
```

### Git-Diff Aware Processing

```bash
# Only process lines changed since last commit
rexpipe -c style-check.toml --git-diff HEAD~1 src/

# Only process lines changed compared to main branch
rexpipe -c lint.toml --git-diff main src/
```

### Checkpoint File Format

```json
{
  "version": "1.0",
  "files": {
    "/var/log/app.log": {
      "position": 1048576,
      "hash": "sha256:abc123...",
      "last_modified": "2024-01-15T10:30:00Z"
    }
  },
  "pipeline_hash": "sha256:def456..."
}
```

### Use Cases

- **Log Processing**: Efficiently process growing log files
- **CI/CD**: Only check changed files in PRs
- **Disaster Recovery**: Resume after interruption
- **Real-time Monitoring**: Continuous stream processing

---

## Pipeline Networks

**Beyond linear chains: fan-out, fan-in, and conditional routing.**

### Fan-Out Pattern

One input feeds multiple parallel pipelines:

```bash
# Process same input through multiple analyzers in parallel
cat data.txt | tee \
  >(rexpipe -c security-audit.toml > security.report) \
  >(rexpipe -c performance-analysis.toml > performance.report) \
  >(rexpipe -c compliance-check.toml > compliance.report) \
  > /dev/null

# Wait for all to complete
wait
```

### Fan-In Pattern

Multiple inputs merge into one pipeline:

```bash
# Merge multiple log sources into unified analysis
cat /var/log/app/*.log | rexpipe -c normalize.toml | \
  rexpipe -c unified-analysis.toml > merged.report
```

### Conditional Routing

Route to different pipelines based on content:

```bash
# Route based on log level
cat app.log | while IFS= read -r line; do
  if echo "$line" | grep -q '\[ERROR\]'; then
    echo "$line" | rexpipe -c error-handler.toml >> errors.txt
  elif echo "$line" | grep -q '\[WARN\]'; then
    echo "$line" | rexpipe -c warn-handler.toml >> warnings.txt
  else
    echo "$line" >> other.txt
  fi
done
```

### Progressive Pipeline with Branching

```
                        ┌─→ [Security Patterns] ─→ security.md
Input → [Extract] → Graph ┼─→ [Performance Patterns] ─→ performance.md
                        └─→ [Architecture Patterns] ─→ architecture.md
```

```bash
# Stage 1: Extract to shared intermediate
cat src/*.py | rexpipe -c extract-symbols.toml > /tmp/symbols.intermediate

# Stage 2: Parallel analysis from same intermediate
rexpipe -c security-patterns.toml < /tmp/symbols.intermediate > security.md &
rexpipe -c performance-patterns.toml < /tmp/symbols.intermediate > performance.md &
rexpipe -c architecture-patterns.toml < /tmp/symbols.intermediate > architecture.md &
wait
```

### Pipeline Composition Script

```bash
#!/bin/bash
# analyze-codebase.sh - Comprehensive codebase analysis

INPUT_DIR="${1:-.}"
OUTPUT_DIR="${2:-./reports}"
INTERMEDIATE_DIR="/tmp/rexpipe-$$"

mkdir -p "$OUTPUT_DIR" "$INTERMEDIATE_DIR"

# Stage 1: Extract (single pass over source files)
find "$INPUT_DIR" -name "*.py" -exec cat {} + | \
  rexpipe -c pipelines/01-extract-symbols.toml > "$INTERMEDIATE_DIR/symbols"

# Stage 2: Build graph
rexpipe -c pipelines/02-build-graph.toml < "$INTERMEDIATE_DIR/symbols" > "$INTERMEDIATE_DIR/graph"

# Stage 3: Parallel analysis (fan-out from graph)
rexpipe -c pipelines/analyze-security.toml < "$INTERMEDIATE_DIR/graph" > "$OUTPUT_DIR/security.md" &
rexpipe -c pipelines/analyze-patterns.toml < "$INTERMEDIATE_DIR/graph" > "$OUTPUT_DIR/patterns.md" &
rexpipe -c pipelines/analyze-complexity.toml < "$INTERMEDIATE_DIR/graph" > "$OUTPUT_DIR/complexity.md" &
wait

# Stage 4: Merge reports (fan-in)
cat "$OUTPUT_DIR"/*.md | rexpipe -c pipelines/merge-reports.toml > "$OUTPUT_DIR/full-report.md"

# Cleanup
rm -rf "$INTERMEDIATE_DIR"

echo "Analysis complete: $OUTPUT_DIR/full-report.md"
```

---

## Combining Features

These features can be combined for powerful workflows:

### Example: Intelligent Codebase Migration

```bash
#!/bin/bash
# migrate-api.sh - Migrate from API v1 to v2 with full safety

# 1. Discover what patterns exist
rexpipe --discover < src/**/*.ts > patterns-found.txt

# 2. Learn any custom patterns from examples
rexpipe --learn \
  --positive "api/v1/users" \
  --positive "api/v1/orders" \
  --negative "api/v2/users" > learned-patterns.txt

# 3. Transform with syntax awareness (only in code, not strings)
rexpipe -c migration.toml \
  --scope code \
  --language typescript \
  --mapping-file rollback-mappings.json \
  -i --apply \
  src/**/*.ts

# 4. Verify cross-file consistency
rexpipe --cross-file consistency-rules.toml -R src/ test/

# 5. If issues found, rollback using bidirectional mappings
# rexpipe -c migration.toml --reverse --mapping-file rollback-mappings.json -i --apply src/**/*.ts
```

---

## Summary

| Feature | CLI Flag | Use Case |
|---------|----------|----------|
| Pattern Learning | `--learn --positive --negative` | Generate regex from examples |
| Pattern Discovery | `--discover` | Find patterns in data automatically |
| Bidirectional | `--reverse --mapping-file` | Reversible transformations |
| Syntax-Aware | `--scope --language` | Match only in code/strings/comments |
| Cross-File | `--cross-file` | Ensure consistency across files |
| Checkpointing | `--checkpoint --resume` | Incremental processing |
| Git-Diff | `--git-diff` | Process only changed lines |

rexpipe is not just a regex tool—it's a **text transformation platform** for building sophisticated processing systems.
