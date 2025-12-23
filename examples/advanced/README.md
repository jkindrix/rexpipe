# Advanced rexpipe Examples

These examples demonstrate rexpipe's advanced capabilities beyond simple text transformation.

## Examples

### 1. Pattern Learning (`--learn`)

Learn regex patterns from positive and negative examples:

```bash
# Create examples file
cat > emails.txt << 'EOF'
user@example.com
admin@company.org
test.user@domain.co.uk
EOF

# Learn pattern from examples
rexpipe --learn --positive emails.txt

# Output: Inferred pattern for email addresses
```

### 2. Pattern Discovery (`--discover`)

Automatically detect patterns in data:

```bash
# Sample data with various patterns
cat > data.txt << 'EOF'
Contact: john@example.com
IP: 192.168.1.1
Phone: 555-123-4567
API Key: sk-1234567890abcdef
EOF

# Discover patterns
rexpipe --discover < data.txt

# Output: Detected email, IPv4, phone, API key patterns
```

### 3. Reversible Redaction

**File:** `reversible-redaction.toml`

Redact PII with the ability to restore original values:

```bash
# Redact (forward)
cat data.txt | rexpipe -c reversible-redaction.toml --mapping-file mappings.json

# Restore (reverse)
cat redacted.txt | rexpipe -c reversible-redaction.toml --reverse --mapping-file mappings.json
```

The mapping file preserves original values for restoration.

### 4. Syntax-Aware Refactoring

**File:** `syntax-aware-refactor.toml`

Refactor code symbols only in specific syntax contexts (requires tree-sitter):

```bash
# Only rename in code, not strings or comments
rexpipe -c syntax-aware-refactor.toml --scope code --language python src/*.py
```

Scopes:
- `code` - Function names, variables, class names
- `string` - String literal contents
- `comment` - Comment text

### 5. Cross-File Consistency

**File:** `cross-file-rules.toml`

Ensure patterns remain synchronized across related files:

```bash
# Check consistency across project
rexpipe --cross-file cross-file-rules.toml -R .
```

Use cases:
- API version sync between source and tests
- Environment variable completeness
- Package version consistency
- Import/export validation

### 6. Incremental Log Processing

**File:** `incremental-log-processor.toml`

Process only new log entries using checkpoints:

```bash
# First run - processes entire file
rexpipe -c incremental-log-processor.toml --checkpoint app.json < app.log

# Subsequent runs - only new entries
rexpipe -c incremental-log-processor.toml --checkpoint app.json --resume < app.log

# Real-time monitoring
tail -f app.log | rexpipe -c incremental-log-processor.toml --checkpoint app.json
```

### 7. Pipeline Networks

**File:** `pipeline-network.sh`

Demonstrates fan-out/fan-in patterns for parallel processing:

```bash
./pipeline-network.sh ./src ./reports
```

Pipeline stages:
1. **Extract** - Single pass symbol extraction
2. **Build Graph** - Dependency graph construction
3. **Fan-Out** - Parallel analysis (security, patterns, smells, opportunities)
4. **Generate Reports** - Individual report generation
5. **Fan-In** - Merge into comprehensive report

## Feature Requirements

| Feature | Requirement |
|---------|-------------|
| Pattern Learning | `--learn` CLI flag |
| Pattern Discovery | `--discover` CLI flag |
| Bidirectional | `bidirectional = true` in config |
| Syntax-Aware | Tree-sitter, `--scope` and `--language` flags |
| Cross-File | `--cross-file` flag with rule config |
| Checkpointing | `--checkpoint` and `--resume` flags |
| Pipeline Networks | Bash orchestration with multiple rexpipe calls |

## See Also

- [ADVANCED_FEATURES.md](../../ADVANCED_FEATURES.md) - Comprehensive feature documentation
- [Progressive System Pipelines](../pipelines/progressive-system/) - Multi-stage transformation examples
