# Test Data Files

Sample files for testing rexpipe features. These contain fake/example data only.

## Files

| File | Purpose | Try This |
|------|---------|----------|
| `sample-log.txt` | Application log with mixed levels | `rexpipe -p 'ERROR' sample-log.txt` |
| `sample-secrets.txt` | Various secret patterns | `rexpipe --discover sample-secrets.txt` |
| `sample-pii.txt` | PII patterns (emails, SSNs, phones) | `rexpipe -c ../examples/pipelines/hipaa-deidentify.toml sample-pii.txt` |
| `sample-code.py` | Python with functions/strings/comments | `rexpipe -p 'old_function' --scope code --language python sample-code.py` |

## Quick Examples

### Discover patterns automatically

```bash
cd test-data
rexpipe --discover sample-secrets.txt
```

### Filter log levels

```bash
# Keep only errors
rexpipe -p '\[ERROR\]' sample-log.txt

# Remove debug lines
cat sample-log.txt | rexpipe -p 'DEBUG' --filter drop_line
```

### Redact secrets

```bash
rexpipe -c ../examples/pipelines/secrets-redact.toml sample-secrets.txt
```

### Redact PII

```bash
rexpipe -c ../examples/pipelines/hipaa-deidentify.toml sample-pii.txt
```

### Syntax-aware refactoring

```bash
# Only rename in code, not strings/comments (requires tree-sitter feature)
rexpipe -p 'old_function' -r 'new_function' \
  --scope code --language python \
  sample-code.py
```

## Creating Your Own Test Data

Generate log files:

```bash
# Create 10K line log file
for i in $(seq 1 10000); do
  echo "2024-12-21 10:15:$((i % 60)) [INFO] Request $i completed"
done > large-log.txt
```

Generate PII data:

```bash
# Create file with fake emails
for i in $(seq 1 100); do
  echo "user$i@example.com"
done > emails.txt
```
