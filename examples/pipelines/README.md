# Example Pipelines

This directory contains example rexpipe pipelines demonstrating various use cases from simple transformations to complex multi-stage processing systems.

## Pipeline Categories

### Basic Transformations
Simple, focused pipelines for common text processing tasks.

| Pipeline | Description | Usage |
|----------|-------------|-------|
| [`csv-clean.toml`](csv-clean.toml) | Clean and normalize CSV data | `cat data.csv \| rexpipe -c csv-clean.toml` |
| [`json-logs-to-text.toml`](json-logs-to-text.toml) | Convert JSON logs to readable text | `cat app.log \| rexpipe -c json-logs-to-text.toml` |
| [`log-normalize.toml`](log-normalize.toml) | Normalize timestamp formats across logs | `cat *.log \| rexpipe -c log-normalize.toml` |
| [`log-stats.toml`](log-stats.toml) | Extract statistics from log files | `cat app.log \| rexpipe -c log-stats.toml` |
| [`markdown-toc.toml`](markdown-toc.toml) | Extract table of contents from markdown | `cat README.md \| rexpipe -c markdown-toc.toml` |
| [`prose-stats.toml`](prose-stats.toml) | Calculate text statistics (word count, etc.) | `cat document.txt \| rexpipe -c prose-stats.toml` |
| [`todo-extract.toml`](todo-extract.toml) | Extract TODO/FIXME comments from code | `rexpipe -c todo-extract.toml -R src/` |

### Security & Compliance
Pipelines for security scanning, secret detection, and compliance.

| Pipeline | Description | Usage |
|----------|-------------|-------|
| [`secrets-redact.toml`](secrets-redact.toml) | Redact API keys, passwords, tokens | `cat config.yaml \| rexpipe -c secrets-redact.toml` |
| [`secrets-redact-v2.toml`](secrets-redact-v2.toml) | Advanced secret redaction with more patterns | `rexpipe -c secrets-redact-v2.toml -R .` |
| [`gdpr-compliance-audit.toml`](gdpr-compliance-audit.toml) | **8-stage** GDPR compliance audit with lineage | `rexpipe -c gdpr-compliance-audit.toml -R data/` |
| [`hipaa-deidentify.toml`](hipaa-deidentify.toml) | HIPAA Safe Harbor de-identification (18 PHI types) | `cat patient-data.json \| rexpipe -c hipaa-deidentify.toml` |
| [`legal-anonymize.toml`](legal-anonymize.toml) | Anonymize legal documents | `cat contract.txt \| rexpipe -c legal-anonymize.toml` |
| [`ioc-extract.toml`](ioc-extract.toml) | Extract Indicators of Compromise for threat intel | `cat suspicious.log \| rexpipe -c ioc-extract.toml` |

### Code Analysis & Migration
Pipelines for analyzing, transforming, and migrating codebases.

| Pipeline | Description | Usage |
|----------|-------------|-------|
| [`codebase-intelligence.toml`](codebase-intelligence.toml) | **10-pass** codebase analyzer (architecture, patterns, smells) | `rexpipe -c codebase-intelligence.toml -R src/` |
| [`python2-to-python3.toml`](python2-to-python3.toml) | **10-stage** Python 2→3 migration with review markers | `rexpipe -c python2-to-python3.toml -R src/` |
| [`api-spec-generator.toml`](api-spec-generator.toml) | Generate OpenAPI specs from route definitions | `rexpipe -c api-spec-generator.toml -R src/` |
| [`dependency-audit.toml`](dependency-audit.toml) | Audit dependencies in package files | `cat package.json \| rexpipe -c dependency-audit.toml` |
| [`cobol-to-csv.toml`](cobol-to-csv.toml) | Convert fixed-width COBOL to CSV | `cat legacy.dat \| rexpipe -c cobol-to-csv.toml` |

### Observability & Logging
Pipelines for log processing, metrics extraction, and alerting.

| Pipeline | Description | Usage |
|----------|-------------|-------|
| [`observability-pipeline.toml`](observability-pipeline.toml) | **8-stage** logs→metrics→anomalies→alerts | `cat *.log \| rexpipe -c observability-pipeline.toml` |
| [`unified-event-stream.toml`](unified-event-stream.toml) | **7-stage** multi-source log fusion to NDJSON | `cat *.log \| rexpipe -c unified-event-stream.toml` |
| [`http-access-stats.toml`](http-access-stats.toml) | HTTP access log statistics | `cat access.log \| rexpipe -c http-access-stats.toml` |
| [`error-frequency.toml`](error-frequency.toml) | Analyze error frequency and patterns | `cat app.log \| rexpipe -c error-frequency.toml` |
| [`stacktrace-clean.toml`](stacktrace-clean.toml) | Clean and format stack traces | `cat crash.log \| rexpipe -c stacktrace-clean.toml` |
| [`build-triage.toml`](build-triage.toml) | Triage build failures from CI logs | `cat build.log \| rexpipe -c build-triage.toml` |

### Data Quality & Transformation
Pipelines for data validation, cleaning, and format conversion.

| Pipeline | Description | Usage |
|----------|-------------|-------|
| [`data-quality-pipeline.toml`](data-quality-pipeline.toml) | **7-stage** data quality framework with scoring | `cat data.csv \| rexpipe -c data-quality-pipeline.toml` |
| [`schema-evolution-engine.toml`](schema-evolution-engine.toml) | **8-phase** schema diff → migration generator | `cat v1.sql v2.sql \| rexpipe -c schema-evolution-engine.toml` |
| [`sql-format.toml`](sql-format.toml) | Format and clean SQL queries | `cat query.sql \| rexpipe -c sql-format.toml` |
| [`sql-to-json.toml`](sql-to-json.toml) | Convert SQL results to JSON | `cat output.sql \| rexpipe -c sql-to-json.toml` |

### Protocol & Format Parsing
Pipelines that implement parsers and protocol decoders.

| Pipeline | Description | Usage |
|----------|-------------|-------|
| [`http-protocol-decoder.toml`](http-protocol-decoder.toml) | **State machine** HTTP protocol parser | `cat http-capture.txt \| rexpipe -c http-protocol-decoder.toml` |
| [`crontab-explain.toml`](crontab-explain.toml) | Parse and explain cron expressions | `cat crontab \| rexpipe -c crontab-explain.toml` |
| [`curl-to-api-doc.toml`](curl-to-api-doc.toml) | Extract API documentation from curl commands | `cat curls.txt \| rexpipe -c curl-to-api-doc.toml` |

### DevOps & Infrastructure
Pipelines for DevOps workflows, Git, Docker, Kubernetes.

| Pipeline | Description | Usage |
|----------|-------------|-------|
| [`git-changelog.toml`](git-changelog.toml) | Generate changelog from git commits | `git log --oneline \| rexpipe -c git-changelog.toml` |
| [`diff-to-changelog.toml`](diff-to-changelog.toml) | Convert diffs to changelog entries | `git diff \| rexpipe -c diff-to-changelog.toml` |
| [`env-to-docker.toml`](env-to-docker.toml) | Convert .env to Dockerfile ENV | `cat .env \| rexpipe -c env-to-docker.toml` |
| [`k8s-sanitize.toml`](k8s-sanitize.toml) | Sanitize Kubernetes manifests | `cat deploy.yaml \| rexpipe -c k8s-sanitize.toml` |
| [`shell-history-audit.toml`](shell-history-audit.toml) | Audit shell history for security | `cat ~/.bash_history \| rexpipe -c shell-history-audit.toml` |

### Publishing & Documentation
Pipelines for content creation and document processing.

| Pipeline | Description | Usage |
|----------|-------------|-------|
| [`manuscript-cleanup.toml`](manuscript-cleanup.toml) | Clean and format manuscripts for publishing | `cat manuscript.txt \| rexpipe -c manuscript-cleanup.toml` |
| [`bibtex-to-apa.toml`](bibtex-to-apa.toml) | Convert BibTeX citations to APA format | `cat refs.bib \| rexpipe -c bibtex-to-apa.toml` |
| [`meeting-notes.toml`](meeting-notes.toml) | Parse and structure meeting transcripts | `cat meeting.txt \| rexpipe -c meeting-notes.toml` |

### Audit & Analytics
Pipelines with finalize sections that produce reports.

| Pipeline | Description | Usage |
|----------|-------------|-------|
| [`api-audit.toml`](api-audit.toml) | Audit API access logs with JSON output | `cat access.log \| rexpipe -c api-audit.toml` |
| [`stats-collector.toml`](stats-collector.toml) | Collect and aggregate statistics | `cat data.log \| rexpipe -c stats-collector.toml` |
| [`code-lens-*.toml`](.) | Code metrics extraction (4 variants) | `rexpipe -c code-lens-complexity.toml -R src/` |

### Meta-Programming (Level 5)
**Advanced pipelines that generate other pipelines or demonstrate self-referential capabilities.**

| Pipeline | Description | Usage |
|----------|-------------|-------|
| [`pipeline-generator.toml`](pipeline-generator.toml) | **Meta-pipeline** that generates redaction pipelines from DSL | See example below |

**Pipeline Generator Example:**
```bash
# Generate a custom redaction pipeline
echo "name: customer-cleaner
redact: email, ssn, phone" | rexpipe -c pipeline-generator.toml > customer-cleaner.toml

# Use the generated pipeline
cat customer-data.txt | rexpipe -c customer-cleaner.toml
```

This Level 5 capability demonstrates rexpipe's potential as a **meta-programming engine** — it doesn't just process data, it generates the tools that process data.

---

## Advanced Pipeline Patterns

### Multi-Stage Progressive Transformation

The most powerful pipelines use **progressive transformation** where each stage builds on the previous:

```
Input → [Stage 1: Parse] → [Stage 2: Annotate] → [Stage 3: Analyze] →
        [Stage 4: Enrich] → [Stage 5: Format] → Output
```

**Key Examples:**
- `observability-pipeline.toml` - 8 stages: parse → extract → classify → metrics → anomalies → correlate → alerts → report
- `gdpr-compliance-audit.toml` - 8 stages: discover → classify → assess risk → map lineage → check consent → retention → remediate → audit
- `codebase-intelligence.toml` - 10 passes: detect language → extract structure → map dependencies → recognize patterns → analyze complexity → mine docs → scan security → infer architecture → synthesize → report

### Internal Marker Pattern

Progressive pipelines use `@@MARKER:value@@` syntax for inter-stage communication:

```toml
# Stage 1: Mark discoveries
[[step]]
pattern = "[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}"
replacement = "@@PII:EMAIL@@$0"

# Stage 3: Use markers for classification
[[step]]
pattern = "@@PII:(EMAIL|PHONE|NAME)@@"
replacement = "$0@@RISK:MEDIUM@@"

# Final stage: Convert to human-readable format
[[step]]
pattern = "@@RISK:([A-Z]+)@@"
replacement = "[RISK:$1]"
```

### State Machine Implementation

Some pipelines implement actual state machines:

```toml
# HTTP Protocol Decoder - State transitions
pattern = "@@STATE:HEADERS@@(.*)\\n\\n"
replacement = "@@STATE:BODY@@$1\\n\\n"
```

### Finalize Section for Aggregation

Use `[finalize]` to aggregate results and generate reports:

```toml
[finalize]
enabled = true

[[finalize.counters]]
name = "errors"
pattern = "\\[ERROR\\]"
description = "Error count"

[[finalize.counters]]
name = "warnings"
pattern = "\\[WARN\\]"
description = "Warning count"

template = """# Report
- Errors: {{counters.errors}}
- Warnings: {{counters.warnings}}

{{output}}
"""
```

---

## Running Pipelines

### Basic Usage

```bash
# Pipe input
cat file.txt | rexpipe -c pipeline.toml

# Process files directly
rexpipe -c pipeline.toml input.txt

# Recursive directory processing
rexpipe -c pipeline.toml -R --include '*.py' src/

# Preview changes (dry-run)
rexpipe -c pipeline.toml --dry-run input.txt

# In-place editing (with backup)
rexpipe -c pipeline.toml -i -b input.txt
```

### Output Formats

```bash
# Plain text (default for terminal)
rexpipe -c pipeline.toml --text

# JSON output (default when piped)
rexpipe -c pipeline.toml --json

# Quiet mode (just counts)
rexpipe -c pipeline.toml -q
```

### Including Pattern Libraries

```bash
# Pipelines can include pattern libraries
# patterns_include = ["patterns/common.toml", "patterns/security.toml"]

# Or pass via CLI
rexpipe -c pipeline.toml --patterns patterns/custom.toml
```

---

## Creating New Pipelines

1. Start with a simple pipeline and add stages incrementally
2. Use descriptive `description` fields for each step
3. Test with `--dry-run` before applying changes
4. Use `[finalize]` for pipelines that aggregate results
5. Include comments explaining the transformation logic

See the [main README](../../README.md) for complete pipeline configuration reference.
