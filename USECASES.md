# rexpipe Use Cases

Practical examples ranging from obvious to creative, general to niche.

## Implementation Status

All 200 use cases are designed to work with rexpipe's feature set. Key features:

| Feature | Status | Notes |
|---------|--------|-------|
| Core regex operations | ✅ Fully implemented | substitute, filter, extract, validate |
| Multi-file processing | ✅ Fully implemented | `-R` recursive, `-g` glob patterns |
| Pipeline configs (TOML) | ✅ Fully implemented | Complex multi-step pipelines |
| Checkpoint/Resume | ✅ Fully implemented | `--checkpoint FILE --resume` |
| Cross-file consistency | ✅ Fully implemented | `--cross-file RULES.toml` |
| Built-in plugins | ✅ Fully implemented | transpose, base64, hash, etc. |
| Data format conversion | ✅ Fully implemented | JSON, CSV, YAML, TOML, XML |
| Pattern learning | ✅ Fully implemented | `--learn` from examples |
| Pipeline testing | ✅ Fully implemented | `--test` with inline test cases |
| Bidirectional transforms | ✅ Fully implemented | `--reverse` support |
| Git-diff awareness | ✅ Fully implemented | `--git-diff REF` |
| Syntax-aware matching | ⚠️ Requires feature | `--features tree-sitter` |
| Format-preserving encryption | ⚠️ Requires feature | `--features fpe` |

---

## Obvious / General

### 1. Log Cleanup & Normalization

```bash
# Normalize timestamps, strip debug lines, redact IPs
rexpipe -c log-sanitize.toml < /var/log/app.log > clean.log
```

### 2. Bulk Code Refactoring

```bash
# Rename function across entire codebase
rexpipe -p 'oldFunction' -r 'newFunction' -i --apply -R src/
```

### 3. Data Format Conversion

```bash
# CSV to JSON
rexpipe --convert --input-format csv --output-format json < data.csv > data.json
```

### 4. Config File Templating

```bash
# Replace placeholders with environment-specific values
rexpipe -p '\$\{DB_HOST\}' -r 'prod-db.internal' -i --apply config/*.yaml
```

---

## Non-Obvious / Creative

### 5. Git Pre-Commit Hook for Secret Detection

```bash
#!/bin/bash
# .git/hooks/pre-commit
git diff --cached --name-only | xargs rexpipe -c patterns/secrets.toml --count
if [ $? -ne 0 ]; then
  echo "Potential secrets detected!"
  exit 1
fi
```

### 6. Reversible Dev/Prod Config Swapping

```toml
# dev-to-prod.toml
[bidirectional]
enabled = true
mapping_file = ".rexpipe-mappings.json"

[[step]]
type = "substitute"
pattern = "localhost:5432"
replacement = "prod-db.company.com:5432"
```

```bash
# Going to prod
rexpipe -c dev-to-prod.toml -i --apply config/

# Back to dev
rexpipe -c dev-to-prod.toml --reverse -i --apply config/
```

### 7. Syntax-Aware Renaming (Only in Code, Not Strings)

> **Requires:** `tree-sitter` feature (`cargo build --features tree-sitter`)

```bash
# Rename variable only in code, not in strings or comments
rexpipe -p 'userId' -r 'accountId' --scope code --language typescript -i --apply src/**/*.ts
```

### 8. Resume Processing Huge Log Files

```bash
# Process 50GB log, resume if interrupted
rexpipe -c pipeline.toml --checkpoint state.json < huge.log > processed.log

# Ctrl+C... later:
rexpipe -c pipeline.toml --checkpoint state.json --resume < huge.log >> processed.log
```

### 9. Cross-File Consistency Linting

```toml
# cross-file-rules.toml - Ensure every src/*.rs has a corresponding tests/*_test.rs
[[rule]]
name = "function-test-coverage"
trigger_files = "**/src/*.rs"
trigger_pattern = "pub fn (\\w+)"
related_files = "**/tests/*_test.rs"
ensure_pattern = "fn test_"
action = "warn"
```

```bash
# Run cross-file consistency check
rexpipe -p '.' --cross-file cross-file-rules.toml -R src/ tests/
```

### 10. Pattern Learning from Examples

```bash
# "I have these order IDs, give me a regex"
echo -e "ORD-2024-00123\nORD-2024-00456\nORD-2024-00789" | rexpipe --learn

# Output: ORD-\d{4}-\d{5} (confidence: 95%)
```

---

## Niche / Specialized

### 11. GDPR Data Anonymization with Deterministic Masking

```toml
# Same email always masks to same value (for JOIN operations)
[[step]]
type = "transform"
pattern = "[\\w.]+@[\\w.]+"
transform = { type = "mask_deterministic", seed_file = "/secrets/mask-seed" }
```

```bash
# user@example.com → u***@e******.com (consistently)
rexpipe -c gdpr-mask.toml < customer-data.csv > anonymized.csv
```

### 12. Format-Preserving Encryption of Credit Cards

> **Requires:** `fpe` feature (`cargo build --features fpe`)

```toml
# Encrypt but keep format (for legacy systems expecting 16 digits)
[[step]]
type = "transform"
pattern = "\\b(\\d{4})[- ]?(\\d{4})[- ]?(\\d{4})[- ]?(\\d{4})\\b"
transform = { type = "fpe_encrypt", key_file = "/secrets/fpe-key" }
```

```bash
# 4532-1234-5678-9012 → 7291-8834-2156-3847 (still valid format)
```

### 13. Inline Pipeline Regression Tests

```toml
name = "SSN Redaction Pipeline"

[[step]]
type = "substitute"
pattern = "\\d{3}-\\d{2}-\\d{4}"
replacement = "XXX-XX-XXXX"

[[test]]
name = "redacts SSN"
input = "SSN: 123-45-6789"
expected = "SSN: XXX-XX-XXXX"

[[test]]
name = "ignores phone numbers"
input = "Phone: 555-123-4567"
expected = "Phone: 555-123-4567"
```

```bash
rexpipe -c pipeline.toml --test
# ✓ redacts SSN
# ✓ ignores phone numbers
```

### 14. Git Diff-Aware Processing (Only Changed Lines)

```bash
# Only process lines changed since last release
rexpipe -c lint.toml --git-diff v1.2.0 src/
```

### 15. Multi-Format Log Aggregation

```bash
# Normalize JSON logs + text logs into unified format
cat app.jsonl | rexpipe --input-format jsonl -c normalize.toml > unified.log
cat legacy.log | rexpipe -c normalize.toml >> unified.log
```

### 16. CI/CD Pipeline Validation

```bash
# Validate all config files match expected patterns before deploy
rexpipe -c validation-rules.toml --validate deploy/*.yaml
if [ $? -ne 0 ]; then
  echo "Config validation failed"
  exit 1
fi
```

### 17. Streaming Log Tail with Transformation

```bash
# Tail a log, transform in real-time, output to another file
tail -f /var/log/app.log | rexpipe -c transform.toml >> /var/log/processed.log
```

### 18. Extract Structured Data from Unstructured Text

```toml
[[step]]
type = "extract"
pattern = "Order #(\\d+) placed by (\\w+) for \\$(\\d+\\.\\d{2})"
output_format = "json"
capture_names = ["order_id", "customer", "amount"]
```

```bash
# "Order #12345 placed by Alice for $99.95"
# → {"order_id": "12345", "customer": "Alice", "amount": "99.95"}
```

---

## Integration Ideas

### 19. Vim/Neovim Filter

```vim
" In visual mode, filter selection through rexpipe
:'<,'>!rexpipe -p 'TODO' -r 'DONE'
```

### 20. Shell Alias for Quick Transforms

```bash
alias snake='rexpipe --transform snake_case'
alias camel='rexpipe --transform camel_case'

echo "myVariableName" | snake  # my_variable_name
```

### 21. Structured Data Queries

```bash
# Query JSON with path expressions
cat data.json | rexpipe -Q '.users[*].email' --output-format text
```

---

## Development Workflows

### 22. License Header Management

```toml
# Add/update copyright headers across all source files
[[step]]
type = "substitute"
pattern = "^(// Copyright \\d{4})"
replacement = "// Copyright 2024"
```

```bash
rexpipe -c update-copyright.toml -i --apply -R -g "*.rs" src/
```

### 23. TODO/FIXME Extraction with Context

```bash
# Extract all TODOs with surrounding context as JSON
rexpipe -p '(TODO|FIXME|HACK|XXX):?\s*(.*)' --extract --json -C 2 -R src/
# Output includes file, line, match, and 2 lines of context
```

### 24. Import Statement Deduplication

```toml
# Find duplicate imports in Python files
[[step]]
type = "extract"
pattern = "^(from .+ import .+|import .+)$"
deduplicate = true
```

### 25. API Version Migration

```toml
# Upgrade API calls from v1 to v2
[[step]]
type = "substitute"
pattern = "/api/v1/(users|orders|products)"
replacement = "/api/v2/$1"

[[step]]
type = "substitute"
pattern = 'api_version:\s*"1"'
replacement = 'api_version: "2"'
```

---

## Security & Compliance

### 26. URL Tracking Parameter Stripping

```toml
# Remove UTM and tracking params from URLs
[[step]]
type = "substitute"
pattern = "([?&])(utm_[^&]+|fbclid|gclid|mc_[^&]+)(&|$)"
replacement = "$1"

[[step]]
type = "substitute"
pattern = "\\?&"
replacement = "?"

[[step]]
type = "substitute"
pattern = "\\?$"
replacement = ""
```

### 27. JWT Token Expiry Extraction

```bash
# Extract and decode JWT expiry from logs
rexpipe -p 'Bearer (eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+)' \
  --extract < auth.log | while read token; do
    echo "$token" | cut -d. -f2 | base64 -d 2>/dev/null | jq .exp
  done
```

### 28. Hardcoded IP/Port Detection

```toml
# Find hardcoded IPs that should be config
[[step]]
type = "extract"
pattern = "\\b(?!127\\.0\\.0\\.1|0\\.0\\.0\\.0)(\\d{1,3}\\.\\d{1,3}\\.\\d{1,3}\\.\\d{1,3})(:\\d+)?\\b"

[[step]]
type = "filter"
pattern = "\\.(example\\.com|test|local)$"
action = "drop_line"
```

---

## Data Munging

### 29. Markdown Table to CSV

```toml
# Convert markdown tables to CSV
[[step]]
type = "filter"
pattern = "^\\|?[-:| ]+\\|?$"
action = "drop_line"

[[step]]
type = "substitute"
pattern = "^\\|\\s*"
replacement = ""

[[step]]
type = "substitute"
pattern = "\\s*\\|$"
replacement = ""

[[step]]
type = "substitute"
pattern = "\\s*\\|\\s*"
replacement = ","
```

### 30. Phone Number Normalization

```toml
# Normalize various phone formats to E.164
[[step]]
type = "transform"
pattern = "\\(?\\d{3}\\)?[-.\\s]?\\d{3}[-.\\s]?\\d{4}"
transform = { type = "shell", command = "sed 's/[^0-9]//g' | sed 's/^/+1/'" }
```

### 31. Currency Extraction with Normalization

```toml
[[step]]
type = "substitute"
pattern = "\\$([\\d,]+(?:\\.\\d{2})?)"
replacement = "USD:$1"

[[step]]
type = "substitute"
pattern = "€([\\d,]+(?:\\.\\d{2})?)"
replacement = "EUR:$1"

[[step]]
type = "substitute"
pattern = "([A-Z]{3}):(\\d+),(\\d+)"
replacement = "$1:$2$3"
```

---

## DevOps / Infrastructure

### 32. Kubernetes Secret Extraction

```bash
# Extract and decode all secrets from a namespace dump
kubectl get secrets -o yaml | rexpipe -p 'data:\n((?:  \w+: [A-Za-z0-9+/=]+\n)+)' --extract | \
  rexpipe -p ': ([A-Za-z0-9+/=]+)' --transform base64_decode
```

### 33. Prometheus Metric Extraction from Logs

```toml
# Convert log lines to Prometheus format
[[step]]
type = "extract"
pattern = "request_duration=([\\d.]+)s path=(/\\S+) status=(\\d+)"
capture_names = ["duration", "path", "status"]
output_template = "http_request_duration_seconds{path=\"$2\",status=\"$3\"} $1"
```

### 34. Terraform Output Parsing

```bash
# Extract all outputs as environment variables
terraform output -json | rexpipe --input-format json \
  -Q '.[*]' \
  -p '"(\\w+)":\\s*\\{[^}]*"value":\\s*"([^"]*)"' \
  -r 'export TF_$1="$2"'
```

---

## Documentation & Content

### 35. Broken Link Detection

```bash
# Extract all markdown links and check them
rexpipe -p '\\[([^\\]]+)\\]\\((https?://[^)]+)\\)' --extract -R docs/ | \
  while read url; do
    curl -s -o /dev/null -w "%{http_code} $url\n" "$url"
  done | grep -v "^200"
```

### 36. Glossary Auto-Linker

```toml
# Auto-link first occurrence of glossary terms
[[step]]
type = "substitute"
pattern = "\\b(Kubernetes|Docker|CI/CD)\\b"
replacement = "[$1](/glossary#$1)"
first_only = true
```

### 37. Translation Placeholder Extraction

```bash
# Extract all i18n strings for translation
rexpipe -p 't\\(["\x27]([^"\x27]+)["\x27]\\)' --extract -R src/ | sort -u > strings-to-translate.txt
```

---

## Unusual / Creative

### 38. Subtitle Timing Shift

```toml
# Shift all subtitle timestamps by 2.5 seconds
[[step]]
type = "transform"
pattern = "(\\d{2}):(\\d{2}):(\\d{2}),(\\d{3})"
transform = { type = "shell", command = "python3 -c \"import sys; h,m,s,ms = map(int, sys.stdin.read().strip().split(',')); t = h*3600000 + m*60000 + s*1000 + ms + 2500; print(f'{t//3600000:02d}:{(t//60000)%60:02d}:{(t//1000)%60:02d},{t%1000:03d}')\"" }
```

### 39. Chord Transposition for Musicians

```toml
# Transpose all chords up 2 semitones
[[step]]
type = "transform"
pattern = "[A-G][#b]?(?:m|maj|min|dim|aug|7|9|11|13)*"
flags = ["global"]
transform = { type = "plugin", name = "transpose", args = ["2"] }
```

```bash
# Input: "C Am F G" → Output: "D Bm G A" (transposed up 2 semitones)
```

### 40. Recipe Ingredient Extraction

```toml
# Extract ingredients list from recipe text
[[step]]
type = "extract"
pattern = "(\\d+(?:/\\d+)?(?:\\.\\d+)?)?\\s*(cups?|tbsp|tsp|oz|lbs?|g|kg|ml|L)?\\s+(?:of\\s+)?([a-zA-Z][a-zA-Z\\s]+?)(?:,|$|\\n)"
capture_names = ["amount", "unit", "ingredient"]
output_format = "json"
```

### 41. Git Commit Message Linting

```bash
# Validate commit messages match conventional commits
git log --oneline -20 | rexpipe -c - <<'EOF'
[[step]]
type = "validate"
pattern = "^[a-f0-9]+ (feat|fix|docs|style|refactor|test|chore)(\\(.+\\))?: .{1,50}$"
on_mismatch = "warn"
EOF
```

### 42. Stack Trace Deduplication

```toml
# Collapse repeated stack traces in logs
[settings]
block_mode = true

[[step]]
type = "block"
start_pattern = "^Exception|^Traceback"
end_pattern = "^\\S|^$"
action = "deduplicate"
```

### 43. Environment Variable Documentation Generator

```bash
# Extract all env var usages and generate .env.example
rexpipe -p 'env::var\\(["\x27](\\w+)["\x27]\\)|std::env::var\\(["\x27](\\w+)["\x27]\\)|\\$\\{(\\w+)\\}|process\\.env\\.(\\w+)' \
  --extract -R src/ | sort -u | sed 's/^/# TODO: document\n/' > .env.example
```

### 44. SQL Schema Diffing Helper

```bash
# Extract CREATE TABLE statements for comparison
rexpipe -p 'CREATE TABLE[^;]+;' --extract < schema.sql | sort > schema-normalized.txt
```

### 45. Log Anomaly Flagging

```toml
# Flag unusual response times
[[step]]
type = "substitute"
pattern = "response_time=([5-9]\\d{3}|[1-9]\\d{4,})ms"
replacement = "⚠️ SLOW: response_time=$1ms"
```

---

## Summary

| Category | Use Cases |
|----------|-----------|
| **DevOps** | Log normalization, secret detection, config templating, K8s secrets, Terraform parsing |
| **Security** | PII redaction, FPE encryption, deterministic masking, JWT extraction, IP detection |
| **Refactoring** | Bulk rename, syntax-aware transforms, API migration, license headers |
| **Data Engineering** | Format conversion, structured extraction, markdown tables, currency normalization |
| **CI/CD** | Pre-commit hooks, config validation, diff-aware linting, commit message linting |
| **Compliance** | GDPR anonymization, reversible transforms, tracking param removal |
| **Documentation** | Broken link detection, glossary linking, i18n string extraction |
| **Creative** | Subtitle timing, chord transposition, recipe parsing, stack trace dedup |
| **ML Processing** | Prompt extraction, training data cleanup, LLM response sanitization |
| **Forensics** | IOC extraction, malware strings, suspicious command detection, event log parsing |
| **Finance** | IBAN extraction, invoice parsing, stock ticker detection |
| **Healthcare** | PHI detection, ICD-10 codes, drug dosage normalization |
| **Academic** | BibTeX citations, DOI extraction, author normalization, arXiv IDs |
| **Build/CI** | Compiler errors, dependency versions, changelog generation, version bumping |
| **Networking** | DNS logs, firewall rules, SSL certs, HAR parsing |
| **Legal** | Contract clauses, e-discovery redaction, compliance markers |
| **Gaming** | Save files, dialogue validation, asset paths, localization keys |
| **IoT** | Serial logs, firmware versions, MQTT topics |
| **Sysadmin** | Cron expressions, disk usage, systemd deps, SSH fingerprints |
| **Database** | SQL extraction, table names, connection string sanitization |
| **API Dev** | OpenAPI paths, GraphQL operations, REST endpoints |
| **Email** | Thread cleanup, meeting times, vCard parsing |
| **Media** | EXIF dates, video timestamps, podcast feeds |
| **Bioinformatics** | DNA sequences, FASTA parsing, ORF detection, protein motifs |
| **Automotive** | CAN bus logs, OBD-II codes, DBC files, vehicle telemetry |
| **Blockchain** | Solidity vulnerabilities, Ethereum addresses, smart contract events |
| **Manufacturing** | QC defect codes, SPC data, batch IDs, PLC alarms |
| **Ham Radio** | ADIF logs, grid squares, frequency bands, contest logs |
| **Genealogy** | GEDCOM parsing, birth dates, family relationships |
| **Observability** | Grok patterns, trace IDs, error rates, latency metrics |
| **NLP** | Entity-preserving cleanup, sentence splitting, URL defanging |
| **Scientific** | FITS headers, chemical formulas, lab notebooks |
| **Real Estate** | Address parsing, MLS numbers, square footage |
| **Aviation** | METAR weather, flight plans, NOTAMs |
| **Retail** | SKUs, price normalization, barcodes |
| **Telecom** | Phone formats, SIP URIs, IMEI numbers |
| **Education** | Student IDs, grades, course codes |
| **Utilities** | Smart meters, outage logs |
| **LLM Processing** | Prompt injection detection, token usage, tool calls, RAG deduplication, guardrails |
| **Cloud Native** | K8s container logs, OTel trace context, Prometheus validation, service mesh correlation |
| **Supply Chain** | SBOM parsing, license compliance, dependency confusion, CVE extraction, commit signatures |
| **IaC** | Terraform plan diff, Pulumi outputs, CloudFormation inventory, secret detection |
| **Protocol** | GraphQL introspection, gRPC errors, WebSocket frames, HTTP/2 headers, rate limits |
| **Privacy** | GDPR consent, PCI detection, SOC2 validation, data residency |
| **Mobile** | iOS crash logs, Android logcat, React Native bridge, Flutter widgets |
| **Profiling** | Flame graphs, memory leaks, GC analysis, Core Web Vitals |
| **Edge/Serverless** | Lambda cold starts, Cloudflare Workers, edge geolocation |
| **Emerging** | WASM analysis, eBPF output, vector DB queries, feature flags |
| **FinOps** | Cost allocation tags, carbon footprint, reserved instance utilization |

---

## ML Data Workflows

### 46. LLM Prompt Template Extraction

```bash
# Extract all prompt templates from codebase
rexpipe -p '(system|user|assistant):\s*[`"]{1,3}([^`"]+)[`"]{1,3}' --extract -R src/
```

### 47. Training Data Deduplication

```toml
# Remove near-duplicate lines from training corpus
[settings]
block_mode = false

[[step]]
type = "filter"
pattern = "^(.{50}).*$"
action = "deduplicate_by_prefix"
```

### 48. LLM Response Cleanup

```toml
# Strip common LLM artifacts from generated text
[[step]]
type = "substitute"
pattern = "^(Sure|Certainly|Of course)[,!]?\\s*(I('d| would) be happy to |here('s| is) )?"
replacement = ""
flags = ["case_insensitive"]

[[step]]
type = "substitute"
pattern = "\\n*Is there anything else.*\\?$"
replacement = ""
```

### 49. Embedding Batch Preparation

```bash
# Split documents into chunks with overlap for embedding
rexpipe -c - <<'EOF' < corpus.txt
[[step]]
type = "block"
start_pattern = "^"
end_pattern = "(?=.{500})"
block_context = { overlap_chars = 100 }
EOF
```

---

## Forensics & Threat Hunting

### 50. IOC (Indicator of Compromise) Extraction

```toml
# Extract all potential IOCs from threat intel
[[step]]
type = "extract"
pattern = "\\b(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\b"
description = "IPv4 addresses"

[[step]]
type = "extract"
pattern = "\\b[a-fA-F0-9]{32}\\b|\\b[a-fA-F0-9]{40}\\b|\\b[a-fA-F0-9]{64}\\b"
description = "MD5/SHA1/SHA256 hashes"

[[step]]
type = "extract"
pattern = "\\b[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\\.(?:[a-z]{2,})+\\b"
description = "Domain names"
```

### 51. Suspicious Command Detection

```bash
# Find potentially malicious commands in bash history
rexpipe -p '(curl|wget).*\|.*sh|base64\s+-d|/dev/tcp/|nc\s+-e' --extract ~/.bash_history
```

### 52. Windows Event Log Parsing

```toml
# Extract failed login attempts
[[step]]
type = "extract"
pattern = "EventID=4625.*TargetUserName=([^\\s]+).*IpAddress=([\\d.]+)"
capture_names = ["username", "source_ip"]
output_format = "json"
```

### 53. Malware String Extraction

```bash
# Extract interesting strings from binary dumps
strings malware.bin | rexpipe -p '(https?://|\\\\\\\\|HKEY_|cmd\\.exe|powershell)' --extract
```

---

## Finance & Accounting

### 54. IBAN Validation and Extraction

```toml
[[step]]
type = "extract"
pattern = "\\b[A-Z]{2}\\d{2}[A-Z0-9]{4}\\d{7}([A-Z0-9]?){0,16}\\b"
description = "IBAN numbers"
```

### 55. Invoice Line Item Parsing

```toml
# Parse invoice line items to structured data
[[step]]
type = "extract"
pattern = "(\\d+)\\s+x\\s+(.+?)\\s+@\\s+\\$([\\d.]+)\\s+=\\s+\\$([\\d.]+)"
capture_names = ["quantity", "item", "unit_price", "total"]
output_format = "csv"
```

### 56. Stock Ticker Extraction

```bash
# Extract stock symbols from financial news
rexpipe -p '\\b[A-Z]{1,5}(?=\\s+(?:stock|shares|rose|fell|gained|dropped))' --extract < news.txt
```

---

## Healthcare

### 57. PHI Detection for HIPAA

```toml
# Detect Protected Health Information
[[step]]
type = "extract"
pattern = "\\b\\d{3}-\\d{2}-\\d{4}\\b"
description = "SSN"

[[step]]
type = "extract"
pattern = "\\b(?:DOB|born|birth[- ]?date)[:\\s]+\\d{1,2}[/-]\\d{1,2}[/-]\\d{2,4}\\b"
description = "Date of Birth"

[[step]]
type = "extract"
pattern = "\\bMRN[:#\\s]+\\d+\\b"
description = "Medical Record Number"
```

### 58. ICD-10 Code Extraction

```bash
# Extract diagnosis codes from clinical notes
rexpipe -p '\\b[A-Z]\\d{2}(?:\\.\\d{1,4})?\\b' --extract < clinical_notes.txt
```

### 59. Drug Dosage Normalization

```toml
# Normalize medication dosages
[[step]]
type = "substitute"
pattern = "(\\d+)\\s*(mg|mcg|g|ml|mL)"
replacement = "$1 $2"

[[step]]
type = "substitute"
pattern = "(\\d+)\\s*(?:milligrams?)"
replacement = "$1 mg"
```

---

## Academic & Research

### 60. BibTeX Citation Extraction

```bash
# Extract all citations from LaTeX documents
rexpipe -p '\\\\cite\\{([^}]+)\\}' --extract -R . -g "*.tex" | tr ',' '\n' | sort -u
```

### 61. DOI Extraction and Validation

```toml
[[step]]
type = "extract"
pattern = "\\b10\\.\\d{4,}/[^\\s]+"
description = "DOI identifiers"
```

### 62. Author Name Normalization

```toml
# Normalize author names to "Last, First" format
[[step]]
type = "substitute"
pattern = "([A-Z][a-z]+)\\s+([A-Z]\\.?\\s*)+([A-Z][a-z]+)"
replacement = "$3, $1"
```

### 63. arXiv ID Extraction

```bash
rexpipe -p 'arXiv:\\s*(\\d{4}\\.\\d{4,5}(?:v\\d+)?)' --extract < references.txt
```

---

## Build Systems & CI

### 64. Compiler Error Aggregation

```bash
# Extract unique error types from build log
cargo build 2>&1 | rexpipe -p 'error\\[E\\d+\\]: (.+)' --extract | sort | uniq -c | sort -rn
```

### 65. Dependency Version Extraction

```bash
# Extract all dependency versions from Cargo.lock
rexpipe -p '^name = "([^"]+)"\\nversion = "([^"]+)"' --extract < Cargo.lock
```

### 66. Changelog Entry Generator

```bash
# Generate changelog from conventional commits
git log --oneline v1.0.0..HEAD | rexpipe -c - <<'EOF'
[[step]]
type = "substitute"
pattern = "^[a-f0-9]+ feat(\\([^)]+\\))?: (.+)"
replacement = "- ✨ $2"

[[step]]
type = "substitute"
pattern = "^[a-f0-9]+ fix(\\([^)]+\\))?: (.+)"
replacement = "- 🐛 $2"

[[step]]
type = "filter"
pattern = "^-"
action = "keep_line"
EOF
```

### 67. Version Bump Automation

```bash
# Bump semver patch version across files
OLD="1.2.3"
NEW="1.2.4"
rexpipe -p "version\\s*=\\s*\"$OLD\"" -r "version = \"$NEW\"" -i --apply Cargo.toml package.json
```

---

## Networking & Protocols

### 68. DNS Query Log Analysis

```toml
# Extract queried domains from DNS logs
[[step]]
type = "extract"
pattern = "query:\\s+([^\\s]+)\\s+IN\\s+(A|AAAA|CNAME|MX)"
capture_names = ["domain", "record_type"]
output_format = "json"
```

### 69. Firewall Rule Extraction

```bash
# Extract allow/deny rules from iptables output
iptables -L -n | rexpipe -p '(ACCEPT|DROP|REJECT).*(?:spt|dpt):(\\d+)' --extract
```

### 70. SSL Certificate Expiry Check

```bash
# Extract cert expiry from openssl output
echo | openssl s_client -connect example.com:443 2>/dev/null | \
  openssl x509 -noout -dates | \
  rexpipe -p 'notAfter=(.+)' --extract
```

### 71. HTTP Header Extraction from HAR

```bash
# Extract all unique request headers from HAR file
cat trace.har | rexpipe -Q '.log.entries[*].request.headers[*].name' | sort -u
```

---

## Legal & Compliance

### 72. Contract Clause Extraction

```toml
# Extract numbered clauses from contracts
[[step]]
type = "extract"
pattern = "^\\d+\\.\\d+\\s+(.+?)(?=\\n\\d+\\.\\d+|\\n*$)"
```

### 73. Redaction for E-Discovery

```toml
# Redact privileged content markers
[[step]]
type = "substitute"
pattern = "(?i)(attorney[- ]client|work[- ]product|privileged)[^.]*\\."
replacement = "[REDACTED - PRIVILEGED]"
```

### 74. Policy Compliance Marker Detection

```bash
# Find files missing required headers
rexpipe -p '^(?!.*CONFIDENTIAL)' -L -R docs/
# Lists files WITHOUT "CONFIDENTIAL" marker
```

---

## Gaming & Creative

### 75. Game Save Hex Pattern Extraction

```bash
# Find player stats in save file hex dump
xxd savegame.dat | rexpipe -p '([0-9a-f]{8}).*PLAYER' --extract
```

### 76. Dialogue Tree Validation

```toml
# dialogue-rules.toml - Ensure all dialogue branches have responses
[[rule]]
name = "dialogue-branch-validation"
trigger_files = "**/dialogue/*.json"
trigger_pattern = '"next":\\s*"(\\w+)"'
related_files = "**/dialogue/*.json"
ensure_pattern = '"id":\\s*"'
action = "fail"
```

```bash
# Validate dialogue tree consistency
rexpipe -p '.' --cross-file dialogue-rules.toml -R dialogue/
```

### 77. Asset Path Normalization

```bash
# Normalize Windows paths to forward slashes
rexpipe -p '\\\\' -r '/' -i --apply -R assets/ -g "*.json"
```

### 78. Localization Key Extraction

```bash
# Extract all translation keys from Unity scripts
rexpipe -p 'Localize\\("([^"]+)"\\)' --extract -R Assets/ -g "*.cs" | sort -u
```

---

## IoT & Embedded

### 79. Serial Port Log Parsing

```toml
# Parse structured sensor data from serial output
[[step]]
type = "extract"
pattern = "SENSOR:(\\w+)\\s+VAL:(\\d+\\.?\\d*)\\s+TS:(\\d+)"
capture_names = ["sensor_id", "value", "timestamp"]
output_format = "csv"
```

### 80. Firmware Version Extraction

```bash
# Find version strings in firmware binary
strings firmware.bin | rexpipe -p 'v?\\d+\\.\\d+\\.\\d+(?:-[a-z0-9]+)?' --extract | head -5
```

### 81. MQTT Topic Extraction

```bash
# Extract all MQTT topics from codebase
rexpipe -p '(?:subscribe|publish)\\(["\x27](/[^"\x27]+)["\x27]' --extract -R src/
```

---

## System Administration

### 82. Cron Expression Extraction

```bash
# List all cron schedules across crontabs
cat /etc/cron.d/* | rexpipe -p '^([0-9*/,-]+\\s+){5}' --extract
```

### 83. Disk Usage Anomaly Detection

```bash
# Flag directories over 10GB
du -h --max-depth=2 / 2>/dev/null | rexpipe -p '(\\d+)G\\s+(.+)' --extract | \
  awk '$1 > 10 {print "⚠️ " $0}'
```

### 84. systemd Unit Dependency Mapping

```bash
# Extract service dependencies
systemctl show -p Wants,Requires,After nginx | \
  rexpipe -p '(?:Wants|Requires|After)=(.+)' --extract | tr ' ' '\n'
```

### 85. SSH Key Fingerprint Extraction

```bash
# Extract all SSH key fingerprints from known_hosts
ssh-keygen -l -f ~/.ssh/known_hosts 2>/dev/null | \
  rexpipe -p '(SHA256:[A-Za-z0-9+/]+)' --extract
```

---

## Database Operations

### 86. SQL Query Extraction from Logs

```bash
# Extract slow queries from PostgreSQL log
rexpipe -p 'duration: ([\\d.]+) ms\\s+statement: (.+)' --extract < postgres.log | \
  awk -F'\t' '$1 > 1000 {print}'
```

### 87. Table Name Extraction

```bash
# Find all referenced tables in SQL files
rexpipe -p '(?:FROM|JOIN|INTO|UPDATE)\\s+([a-z_][a-z0-9_]*)' --extract -R sql/ -g "*.sql" | sort -u
```

### 88. Connection String Sanitization

```toml
# Remove passwords from connection strings before logging
[[step]]
type = "substitute"
pattern = "(password|pwd)=([^;]+)"
replacement = "$1=*****"
flags = ["case_insensitive"]
```

---

## API Development

### 89. OpenAPI Path Extraction

```bash
# Extract all API paths from OpenAPI spec
rexpipe -Q '.paths | keys[]' < openapi.json
```

### 90. GraphQL Query Extraction

```bash
# Extract GraphQL operation names
rexpipe -p '(?:query|mutation|subscription)\\s+(\\w+)' --extract -R src/ -g "*.graphql"
```

### 91. REST Endpoint Documentation

```bash
# Extract route definitions with comments
rexpipe -p '(?://|#)\\s*(.+)\\n.*(?:@(?:Get|Post|Put|Delete)|router\\.)' --extract -R src/
```

---

## Email & Communication

### 92. Email Thread Extraction

```toml
# Strip quoted replies from email threads
[[step]]
type = "filter"
pattern = "^>|^On .* wrote:"
action = "drop_line"
```

### 93. Meeting Time Extraction

```bash
# Extract meeting times from calendar exports
rexpipe -p 'DTSTART[^:]*:(\\d{8}T\\d{6})' --extract < calendar.ics
```

### 94. Contact VCard Parsing

```toml
[[step]]
type = "extract"
pattern = "(?:FN|EMAIL|TEL)[^:]*:(.+)"
```

---

## Media & Content

### 95. EXIF Date Extraction

```bash
# Extract photo dates for organization
exiftool -DateTimeOriginal *.jpg | rexpipe -p ': (\\d{4}):(\\d{2})' --extract
```

### 96. Video Timestamp Extraction

```bash
# Extract chapter markers from YouTube descriptions
rexpipe -p '^(\\d{1,2}:\\d{2}(?::\\d{2})?)\\s+(.+)' --extract < description.txt
```

### 97. Podcast RSS Feed Parsing

```bash
# Extract episode URLs from podcast feed
curl -s "https://example.com/feed.xml" | \
  rexpipe -p '<enclosure[^>]+url="([^"]+)"' --extract
```

---

## Bioinformatics

### 98. DNA Sequence Pattern Extraction

```bash
# Find all start codons (ATG) in a sequence
rexpipe -p 'ATG' --extract < sequence.fasta
```

### 99. FASTA Header Parsing

```bash
# Extract gene IDs from FASTA headers
rexpipe -p '^>(\S+)' --extract < genes.fasta
```

### 100. Open Reading Frame Detection

```toml
# Find potential ORFs (ATG...stop codon, length multiple of 3)
[[step]]
type = "extract"
pattern = "ATG(?:[ATGC]{3})*?(?:TAA|TAG|TGA)"
description = "Potential open reading frames"
```

### 101. Restriction Enzyme Site Finder

```bash
# Find EcoRI cut sites (GAATTC)
rexpipe -p 'GAATTC' --extract < plasmid.fasta | wc -l
```

### 102. Protein Motif Search

```bash
# Find zinc finger motifs (C-x2-C-x3-F-x5-L-x2-H-x3-H pattern)
rexpipe -p 'C.{2}C.{3}F.{5}L.{2}H.{3}H' --extract < protein.fasta
```

### 103. FASTQ Quality Score Filtering

```toml
# Filter reads with low quality scores
[[step]]
type = "block"
start_pattern = "^@"
end_pattern = "^\\+"

[[step]]
type = "filter"
pattern = "[!-,]"  # Phred scores below 10
action = "drop_block"
```

---

## Automotive & Vehicle Data

### 104. CAN Bus Message Parsing

```bash
# Extract CAN IDs and data from candump logs
rexpipe -p '\\(([0-9.]+)\\)\\s+\\w+\\s+([0-9A-F]+)#([0-9A-F]+)' --extract < candump.log
```

### 105. OBD-II Trouble Code Extraction

```bash
# Find diagnostic trouble codes
rexpipe -p '\\b[PCBU][0-9A-F]{4}\\b' --extract < obd_scan.txt
```

### 106. Vehicle Speed Extraction from Logs

```toml
# Parse speed values from vehicle data logs
[[step]]
type = "extract"
pattern = "SPD[:\\s]+([\\d.]+)\\s*(mph|km/h)"
capture_names = ["speed", "unit"]
output_format = "csv"
```

### 107. DBC Signal Definition Extraction

```bash
# Extract signal definitions from CAN DBC files
rexpipe -p 'SG_\\s+(\\w+)\\s*:\\s*(\\d+)\\|(\\d+)' --extract < vehicle.dbc
```

---

## Blockchain & Smart Contracts

### 108. Solidity Vulnerability Pattern Detection

```toml
# Detect potential reentrancy vulnerabilities
[[step]]
type = "extract"
pattern = "\\.call\\{.*value.*\\}|call\\.value\\("
description = "Potential reentrancy vulnerability"

[[step]]
type = "extract"
pattern = "tx\\.origin"
description = "tx.origin usage (phishing risk)"
```

### 109. Ethereum Address Extraction

```bash
# Extract all Ethereum addresses from logs
rexpipe -p '0x[a-fA-F0-9]{40}' --extract < transactions.log
```

### 110. Smart Contract Event Signature Parsing

```bash
# Extract event definitions from Solidity
rexpipe -p 'event\\s+(\\w+)\\s*\\([^)]*\\)' --extract -R contracts/ -g "*.sol"
```

### 111. Gas Usage Pattern Analysis

```bash
# Find high-gas operations in Solidity
rexpipe -p '(for|while)\\s*\\([^)]*\\)\\s*\\{' --extract -R contracts/
```

---

## Manufacturing & Quality Control

### 112. Defect Code Extraction from QC Logs

```toml
# Parse quality control defect codes
[[step]]
type = "extract"
pattern = "DEF-([A-Z]{2})(\\d{3}):\\s*(.+)"
capture_names = ["category", "code", "description"]
output_format = "json"
```

### 113. SPC (Statistical Process Control) Data Parsing

```bash
# Extract measurements from SPC logs
rexpipe -p 'MEAS:\\s*([\\d.]+)\\s*(mm|in|μm)' --extract < spc_data.log
```

### 114. Production Batch ID Extraction

```bash
# Find batch IDs across production logs
rexpipe -p 'BATCH[:#]\\s*([A-Z]{2}\\d{6}-\\d{3})' --extract -R /var/log/production/
```

### 115. Machine Alarm Log Parsing

```toml
# Parse PLC alarm logs
[[step]]
type = "extract"
pattern = "(\\d{4}-\\d{2}-\\d{2}\\s+\\d{2}:\\d{2}:\\d{2})\\s+ALARM\\s+(\\d+):\\s+(.+)"
capture_names = ["timestamp", "alarm_code", "message"]
output_format = "jsonl"
```

---

## Ham Radio & Amateur Radio

### 116. ADIF QSO Record Extraction

```bash
# Extract callsigns from ADIF log
rexpipe -p '<CALL:\\d+>([A-Z0-9/]+)' --extract < logbook.adi
```

### 117. Maidenhead Grid Square Validation

```bash
# Find valid grid locators
rexpipe -p '\\b[A-R]{2}\\d{2}[a-x]{2}\\b' --extract < contacts.txt
```

### 118. Frequency Band Extraction

```toml
# Parse frequency from ham radio logs
[[step]]
type = "extract"
pattern = "<FREQ:[^>]+>([\\d.]+)"
description = "Frequency in MHz"

[[step]]
type = "filter"
pattern = "^(1\\.8|3\\.5|7\\.|14\\.|21\\.|28\\.)"
action = "keep_line"
description = "HF bands only"
```

### 119. Contest Log Normalization

```bash
# Convert mixed-case callsigns to uppercase
rexpipe -p '<CALL:\\d+>([a-zA-Z0-9/]+)' --transform uppercase -i --apply logbook.adi
```

---

## Genealogy

### 120. GEDCOM Individual Extraction

```bash
# Extract all individual names from GEDCOM
rexpipe -p '1 NAME (.+)' --extract < family.ged
```

### 121. Birth Date Extraction

```bash
# Find all birth dates in GEDCOM
rexpipe -p '2 DATE (.+)' -B 1 < family.ged | rexpipe -p 'BIRT.*\\n.*DATE (.+)' --extract
```

### 122. Family Relationship Parsing

```toml
# Extract parent-child relationships
[[step]]
type = "extract"
pattern = "1 CHIL @(I\\d+)@"
description = "Child references"
```

### 123. Place Name Normalization

```toml
# Standardize place names in genealogy data
[[step]]
type = "substitute"
pattern = ", USA$|, United States$|, U\\.S\\.A\\.$"
replacement = ", United States of America"
```

---

## Observability & SRE

### 124. Grok Pattern Application

```toml
# Parse Apache combined log format
[[step]]
type = "extract"
pattern = "^([\\d.]+) - - \\[([^\\]]+)\\] \"(\\w+) ([^ ]+) HTTP/[\\d.]+\" (\\d+) (\\d+|-)"
capture_names = ["client_ip", "timestamp", "method", "path", "status", "bytes"]
output_format = "json"
```

### 125. OpenTelemetry Trace ID Extraction

```bash
# Extract trace IDs from logs
rexpipe -p 'trace_id[=:]\\s*([a-f0-9]{32})' --extract < app.log
```

### 126. Error Rate Calculation Helper

```bash
# Count errors vs total requests
total=$(rexpipe -p 'HTTP/1\\.[01]" \\d+' --count < access.log)
errors=$(rexpipe -p 'HTTP/1\\.[01]" [45]\\d{2}' --count < access.log)
echo "Error rate: $(echo "scale=2; $errors / $total * 100" | bc)%"
```

### 127. Latency Percentile Extraction

```bash
# Extract response times for percentile calculation
rexpipe -p 'response_time=([\\d.]+)ms' --extract < app.log | sort -n | awk '
  {a[NR]=$1} END {print "p50:", a[int(NR*0.5)], "p99:", a[int(NR*0.99)]}'
```

---

## NLP & Text Preprocessing

### 128. Entity-Preserving Text Cleaning

```toml
# Clean text while preserving currencies and dates
[[step]]
type = "substitute"
pattern = "[^\\w\\s$€£¥.,/-]"
replacement = ""
description = "Remove special chars except currency/date markers"
```

### 129. Sentence Boundary Detection

```bash
# Split text into sentences
rexpipe -p '(?<=[.!?])\\s+(?=[A-Z])' -r '\n' < document.txt
```

### 130. Hashtag and Mention Extraction

```bash
# Extract social media entities
rexpipe -p '[@#]\\w+' --extract < tweets.jsonl
```

### 131. URL Defanging for Security Reports

```toml
# Defang URLs for safe sharing
[[step]]
type = "substitute"
pattern = "https?://"
replacement = "hxxp://"

[[step]]
type = "substitute"
pattern = "\\."
replacement = "[.]"
```

---

## Scientific Data

### 132. FITS Header Keyword Extraction

```bash
# Extract key FITS header values
rexpipe -p "^(OBJECT|EXPTIME|DATE-OBS)\\s*=\\s*'?([^'/]+)" --extract < header.txt
```

### 133. Chemical Formula Parsing

```bash
# Extract molecular formulas
rexpipe -p '\\b[A-Z][a-z]?\\d*(?:[A-Z][a-z]?\\d*)*\\b' --extract < chemistry.txt
```

### 134. Lab Notebook Entry Extraction

```toml
# Parse structured lab notes
[[step]]
type = "extract"
pattern = "(\\d{4}-\\d{2}-\\d{2})\\s+EXP-(\\d+):\\s+(.+)"
capture_names = ["date", "experiment_id", "notes"]
output_format = "csv"
```

---

## Real Estate & Property

### 135. Address Parsing

```toml
# Parse US street addresses
[[step]]
type = "extract"
pattern = "(\\d+)\\s+([NSEW]\\.?\\s+)?(\\w+(?:\\s+\\w+)*)\\s+(St|Ave|Blvd|Dr|Rd|Ln|Ct|Way)\\.?"
capture_names = ["number", "direction", "street", "type"]
```

### 136. MLS Number Extraction

```bash
# Find MLS listing numbers
rexpipe -p 'MLS#?\\s*:?\\s*([A-Z]?\\d{6,8})' --extract < listings.txt
```

### 137. Property Square Footage Normalization

```toml
# Normalize sq ft representations
[[step]]
type = "substitute"
pattern = "(\\d+(?:,\\d{3})*)\\s*(?:sq\\.?\\s*ft\\.?|square\\s+feet|SF)"
replacement = "$1 sqft"
flags = ["case_insensitive"]
```

---

## Aviation

### 138. METAR Weather Parsing

```bash
# Extract visibility from METAR
rexpipe -p '\\b(\\d+)SM\\b' --extract < metar.txt
```

### 139. Flight Plan Route Extraction

```bash
# Parse waypoints from flight plans
rexpipe -p '\\b[A-Z]{5}\\b|\\b[A-Z]{3}\\d{3}[A-Z]{3}\\b' --extract < flightplan.txt
```

### 140. NOTAM Parsing

```toml
# Extract NOTAMs by category
[[step]]
type = "extract"
pattern = "([A-Z])\\d{4}/\\d{2}\\s+NOTAM[NRC]\\s+(.+?)(?=\\n[A-Z]\\d{4}|$)"
capture_names = ["category", "content"]
```

---

## Retail & E-commerce

### 141. SKU Extraction and Validation

```bash
# Extract SKUs from product data
rexpipe -p '\\b[A-Z]{2,4}-\\d{4,8}(-[A-Z0-9]+)?\\b' --extract < products.csv
```

### 142. Price Normalization

```toml
# Normalize price formats to decimal
[[step]]
type = "substitute"
pattern = "\\$([\\d,]+)\\.?(\\d{2})?"
replacement = "$1.$2"

[[step]]
type = "substitute"
pattern = ","
replacement = ""
```

### 143. Barcode/UPC Extraction

```bash
# Extract UPC codes from inventory
rexpipe -p '\\b\\d{12,13}\\b' --extract < inventory.txt
```

---

## Telecommunications

### 144. Phone Number Format Detection

```toml
# Identify and categorize phone formats
[[step]]
type = "extract"
pattern = "\\+1[-. ]?\\(?\\d{3}\\)?[-. ]?\\d{3}[-. ]?\\d{4}"
description = "US format with country code"

[[step]]
type = "extract"
pattern = "\\+44[-. ]?\\d{4}[-. ]?\\d{6}"
description = "UK format"
```

### 145. SIP URI Extraction

```bash
# Extract SIP addresses from logs
rexpipe -p 'sip:[^@]+@[^;>\\s]+' --extract < voip.log
```

### 146. IMEI Number Detection

```bash
# Find IMEI numbers in device logs
rexpipe -p '\\b\\d{15}\\b' --extract < device_registry.txt
```

---

## Education

### 147. Student ID Extraction

```bash
# Extract student IDs from transcripts
rexpipe -p '\\bSTU-\\d{8}\\b|\\b[A-Z]{2}\\d{7}\\b' --extract < transcripts/
```

### 148. Grade Normalization

```toml
# Normalize letter grades
[[step]]
type = "substitute"
pattern = "\\b([ABCDF])\\s*[-+]?\\b"
replacement = "$1"
description = "Remove +/- modifiers"
```

### 149. Course Code Parsing

```bash
# Extract course codes with sections
rexpipe -p '\\b[A-Z]{2,4}\\s*\\d{3,4}[A-Z]?\\s*-\\s*\\d{2,3}\\b' --extract < schedule.txt
```

---

## Energy & Utilities

### 150. Smart Meter Reading Extraction

```toml
# Parse smart meter data
[[step]]
type = "extract"
pattern = "METER:(\\w+)\\s+KWH:([\\d.]+)\\s+TS:(\\d+)"
capture_names = ["meter_id", "reading", "timestamp"]
output_format = "csv"
```

### 151. Power Outage Log Parsing

```bash
# Extract outage events
rexpipe -p 'OUTAGE\\s+(\\d{4}-\\d{2}-\\d{2}T[\\d:]+)\\s+DURATION:(\\d+)min' --extract < grid.log
```

---

## LLM Output Processing

### 152. Prompt Injection Detection

```toml
# Detect potential prompt injection attempts
[[step]]
type = "extract"
pattern = "(?i)(ignore previous|disregard|forget|system prompt|you are now|act as|pretend to be)"
description = "Potential prompt injection phrases"

[[step]]
type = "extract"
pattern = "</?(?:system|user|assistant)>"
description = "Injected role markers"
```

### 153. LLM Response Confidence Extraction

```bash
# Extract confidence scores from LLM outputs
rexpipe -p '(?:confidence|certainty|probability)[:=\s]+(\d+(?:\.\d+)?%?)' --extract < llm_outputs.jsonl
```

### 154. Token Usage Parsing from API Responses

```toml
# Parse LLM API usage metadata
[[step]]
type = "extract"
pattern = '"(prompt_tokens|completion_tokens|total_tokens|input_tokens|output_tokens)":\s*(\d+)'
capture_names = ["token_type", "count"]
output_format = "csv"
```

### 155. Guardrail Violation Logging

```toml
# Extract and categorize safety violations
[[step]]
type = "extract"
pattern = '\[BLOCKED\]\s+category=(\w+)\s+severity=(\w+)\s+content="([^"]+)"'
capture_names = ["category", "severity", "content"]
output_format = "jsonl"
```

### 156. MCP Tool Call Extraction

```bash
# Extract MCP tool invocations from Claude logs
rexpipe -p 'tool_use.*?"name":\s*"([^"]+)".*?"input":\s*(\{[^}]+\})' --extract < mcp_session.log
```

### 157. RAG Context Window Deduplication

```toml
# Deduplicate retrieved chunks before sending to LLM
[settings]
block_mode = true

[[step]]
type = "block"
start_pattern = "^---CHUNK---"
end_pattern = "^---END---"
action = "deduplicate"
```

---

## Cloud Native & Observability

### 158. Kubernetes Container Log Parsing

```toml
# Parse containerd/CRI-O log format to structured data
[[step]]
type = "extract"
pattern = '^(\d{4}-\d{2}-\d{2}T[\d:.]+Z)\s+(stdout|stderr)\s+(\w)\s+(.+)$'
capture_names = ["timestamp", "stream", "partial", "message"]
output_format = "jsonl"
```

### 159. OpenTelemetry Trace Context Injection

```toml
# Add trace context to logs missing it
[[step]]
type = "filter"
pattern = "trace_id="
action = "drop_line"

[[step]]
type = "substitute"
pattern = '^(\d{4}-\d{2}-\d{2})'
replacement = '$1 trace_id=${TRACE_ID} span_id=${SPAN_ID}'
```

### 160. Prometheus Metric Name Validation

```bash
# Validate metric names follow naming conventions
rexpipe -c - <<'EOF' < metrics.txt
[[step]]
type = "validate"
pattern = "^[a-z][a-z0-9_]*_(total|count|sum|bucket|created|info)$"
on_mismatch = "warn"
EOF
```

### 161. Kubernetes Event Extraction

```bash
# Extract warning events from kubectl output
kubectl get events -o json | rexpipe -Q '.[*] | select(.type == "Warning")' \
  -p '"reason":\s*"([^"]+)".*"message":\s*"([^"]+)"' --extract
```

### 162. Helm Values Diff Analysis

```bash
# Compare helm values between environments
diff <(rexpipe -p '^\s*(\w+):' --extract < values-dev.yaml | sort) \
     <(rexpipe -p '^\s*(\w+):' --extract < values-prod.yaml | sort)
```

### 163. Service Mesh Sidecar Log Correlation

```toml
# Correlate Envoy/Istio sidecar logs with app logs
[[step]]
type = "extract"
pattern = 'x-request-id=([a-f0-9-]+)|"x-request-id":"([a-f0-9-]+)"'
description = "Request correlation IDs"
```

---

## Supply Chain Security

### 164. SBOM Component Extraction (CycloneDX)

```bash
# Extract all components from CycloneDX SBOM
rexpipe -Q '.components[*] | {name: .name, version: .version, purl: .purl}' < sbom.json
```

### 165. SPDX License Compliance Check

```toml
# Flag components with problematic licenses
[[step]]
type = "extract"
pattern = '"licenseConcluded":\s*"([^"]+)"'
capture_names = ["license"]

[[step]]
type = "filter"
pattern = "GPL|AGPL|SSPL"
action = "keep_line"
description = "Copyleft licenses requiring review"
```

### 166. Dependency Confusion Detection

```bash
# Find private package names that might conflict with public registries
rexpipe -p '@company/([a-z-]+)' --extract < package-lock.json | \
  while read pkg; do
    curl -s "https://registry.npmjs.org/$pkg" | grep -q '"name"' && echo "CONFLICT: $pkg"
  done
```

### 167. CVE ID Extraction from Security Advisories

```bash
# Extract all CVE references from security bulletins
rexpipe -p 'CVE-\d{4}-\d{4,}' --extract -R security-advisories/ | sort -u
```

### 168. Git Commit Signature Verification Parsing

```bash
# Parse GPG signature status from git log
git log --show-signature -10 2>&1 | \
  rexpipe -p 'gpg: (Good|BAD) signature from "([^"]+)"' --extract
```

---

## Infrastructure as Code

### 169. Terraform Plan Diff Extraction

```bash
# Extract resources being created/modified/destroyed
terraform plan -no-color | rexpipe -p '^\s*#\s*([\w.]+)\s+will be (created|destroyed|updated)' --extract
```

### 170. Pulumi Stack Output Parsing

```bash
# Extract outputs from Pulumi stack
pulumi stack output --json | rexpipe -Q '.[] | keys[]' --extract
```

### 171. CloudFormation Resource Type Inventory

```bash
# List all resource types in CloudFormation templates
rexpipe -p '"Type":\s*"(AWS::[^"]+)"' --extract -R cloudformation/ | sort | uniq -c | sort -rn
```

### 172. Terraform Provider Version Extraction

```bash
# Audit provider versions across modules
rexpipe -p 'source\s*=\s*"([^"]+)".*\n.*version\s*=\s*"([^"]+)"' --extract -R modules/
```

### 173. IaC Secret Detection

```toml
# Detect hardcoded secrets in IaC files
[[step]]
type = "extract"
pattern = '(?i)(password|secret|api_key|token)\s*[=:]\s*["'\'']([^"'\'']+)["'\'']'
description = "Potential hardcoded secrets"

[[step]]
type = "filter"
pattern = '\$\{|var\.|local\.'
action = "drop_line"
description = "Exclude variable references"
```

---

## API & Protocol Analysis

### 174. GraphQL Introspection Parsing

```bash
# Extract all type definitions from GraphQL schema
rexpipe -p 'type\s+(\w+)\s*\{' --extract < schema.graphql
```

### 175. gRPC Error Code Extraction

```bash
# Parse gRPC status codes from logs
rexpipe -p 'code\s*=\s*(OK|CANCELLED|UNKNOWN|INVALID_ARGUMENT|DEADLINE_EXCEEDED|NOT_FOUND|[A-Z_]+)' \
  --extract < grpc-server.log | sort | uniq -c
```

### 176. WebSocket Frame Analysis

```toml
# Extract WebSocket message types and payloads
[[step]]
type = "extract"
pattern = '(TEXT|BINARY)\s+frame.*payload:\s*(.+)'
capture_names = ["frame_type", "payload"]
output_format = "jsonl"
```

### 177. HTTP/2 HPACK Header Extraction

```bash
# Parse decoded HTTP/2 headers from network captures
rexpipe -p ':(\w+):\s*(.+)' --extract < h2_headers.txt
```

### 178. Rate Limit Header Parsing

```bash
# Extract rate limit info from API response headers
curl -sI api.example.com | rexpipe -p '(X-RateLimit-\w+|Retry-After):\s*(.+)' --extract
```

---

## Compliance & Privacy

### 179. GDPR Consent String Parsing (TCF v2)

```bash
# Decode and extract vendor consent from TCF strings
rexpipe -p 'euconsent-v2=([A-Za-z0-9_-]+)' --extract < cookies.log
```

### 180. PCI DSS Cardholder Data Detection

```toml
# Detect potential PCI data in logs
[[step]]
type = "extract"
pattern = '\b(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13})\b'
description = "Credit card numbers (Visa, MC, Amex)"

[[step]]
type = "extract"
pattern = '\b[0-9]{3,4}\b'
description = "CVV codes (requires context)"
```

### 181. SOC2 Audit Log Validation

```toml
# Validate audit logs contain required fields
[[step]]
type = "validate"
pattern = '"timestamp":.+"actor":.+"action":.+"resource":'
on_mismatch = "error"
description = "Required audit fields"
```

### 182. Data Residency Violation Detection

```bash
# Find references to non-compliant regions
rexpipe -p '(us-east|eu-west|ap-southeast)-\d' --extract < terraform.tfstate | \
  grep -v "eu-west" && echo "Non-EU region detected!"
```

---

## Mobile & Client-Side

### 183. iOS Crash Log Symbolication Prep

```bash
# Extract addresses for symbolication
rexpipe -p '^\d+\s+\w+\s+(0x[0-9a-f]+)\s+' --extract < crash.log
```

### 184. Android Logcat Filtering

```toml
# Filter and structure Android logs
[[step]]
type = "extract"
pattern = '^(\d{2}-\d{2})\s+([\d:.]+)\s+(\d+)\s+(\d+)\s+([VDIWEF])\s+([^:]+):\s*(.+)'
capture_names = ["date", "time", "pid", "tid", "level", "tag", "message"]
output_format = "jsonl"
```

### 185. React Native Bridge Message Parsing

```bash
# Extract native bridge calls
rexpipe -p 'NativeCall.*module=(\w+).*method=(\w+).*args=(\[.+\])' --extract < rn-debug.log
```

### 186. Flutter Widget Tree Extraction

```bash
# Parse widget hierarchy from Flutter debug output
rexpipe -p '^\s*(├|└)─+\s*(\w+)' --extract < flutter_debug.txt
```

---

## Performance & Profiling

### 187. Flame Graph Stack Extraction

```bash
# Convert perf output to flame graph format
rexpipe -p '^(\S+);.*?\s+(\d+)$' --extract < perf.out
```

### 188. Memory Leak Pattern Detection

```toml
# Identify growing memory allocations
[[step]]
type = "extract"
pattern = 'alloc.*size=(\d+).*addr=(0x[a-f0-9]+)'
capture_names = ["size", "address"]

[[step]]
type = "filter"
pattern = 'size=\d{7,}'
action = "keep_line"
description = "Large allocations (10MB+)"
```

### 189. Garbage Collection Log Analysis

```bash
# Parse GC pause times from JVM logs
rexpipe -p 'GC\(\d+\).*Pause.*?(\d+\.\d+)ms' --extract < gc.log | \
  awk '{sum+=$1; count++} END {print "Avg pause:", sum/count, "ms"}'
```

### 190. Browser Performance Timing Extraction

```bash
# Extract Core Web Vitals from performance logs
rexpipe -p '(LCP|FID|CLS|TTFB|FCP):\s*([\d.]+)' --extract < performance.json
```

---

## Edge & Serverless

### 191. Lambda Cold Start Detection

```toml
# Identify cold starts in CloudWatch logs
[[step]]
type = "extract"
pattern = 'REPORT RequestId:\s*([a-f0-9-]+).*Init Duration:\s*([\d.]+)\s*ms'
capture_names = ["request_id", "init_duration"]
output_format = "csv"
```

### 192. Cloudflare Workers Log Parsing

```bash
# Extract worker execution metrics
rexpipe -p 'cpu_time":(\d+).*wall_time":(\d+)' --extract < worker-logs.jsonl
```

### 193. Edge Function Geolocation Extraction

```bash
# Parse client location from edge logs
rexpipe -p 'cf-ipcountry:\s*(\w+).*cf-ipcity:\s*([^,]+)' --extract < edge-access.log
```

---

## Emerging Technologies

### 194. WebAssembly Module Analysis

```bash
# Extract exported functions from WASM text format
rexpipe -p '\(export\s+"([^"]+)"\s+\(func' --extract < module.wat
```

### 195. eBPF Program Output Parsing

```bash
# Parse bpftrace output for syscall analysis
rexpipe -p '@\[([^]]+)\]:\s*(\d+)' --extract < bpftrace.out
```

### 196. Vector Database Query Parsing

```bash
# Extract embedding dimensions and similarity scores
rexpipe -p 'similarity:\s*([\d.]+).*id:\s*"([^"]+)"' --extract < vector-results.json
```

### 197. Feature Flag Extraction from Code

```bash
# Find all feature flag checks in codebase
rexpipe -p 'isFeatureEnabled\(["\x27](\w+)["\x27]\)|getFeatureFlag\(["\x27](\w+)["\x27]\)' \
  --extract -R src/ | sort -u
```

---

## Sustainability & FinOps

### 198. Cloud Cost Allocation Tag Extraction

```bash
# Extract cost allocation tags from resources
rexpipe -p '"(cost-center|project|environment)":\s*"([^"]+)"' --extract < cloud-inventory.json
```

### 199. Carbon Footprint Metrics Parsing

```bash
# Parse carbon intensity data from sustainability APIs
rexpipe -p '"carbonIntensity":\s*([\d.]+).*"region":\s*"([^"]+)"' --extract < carbon-data.json
```

### 200. Reserved Instance Utilization Extraction

```bash
# Parse RI coverage from AWS Cost Explorer
rexpipe -p 'Coverage.*?(\d+\.?\d*)%.*Instance.*?([a-z]\d+\.\w+)' --extract < ri-report.csv
```

---

## The Sweet Spot

rexpipe excels at **repeatable, multi-stage text transformations** that you'd otherwise:
- Do manually in regex101.com
- Cobble together with fragile `sed | awk | grep` chains
- Write one-off Python scripts for

If you find yourself doing the same regex transformations repeatedly, capture them in a `.toml` pipeline and version control it.
