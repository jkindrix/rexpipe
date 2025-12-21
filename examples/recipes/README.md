# rexpipe Recipe Library

This directory contains 100+ pre-built pipeline recipes for common data processing tasks.

## Categories

| Category | Count | Description |
|----------|-------|-------------|
| `code-processing/` | 13 | Source code analysis and transformation |
| `compliance/` | 5 | GDPR, HIPAA, PCI-DSS, SOC2, CCPA compliance |
| `data-conversion/` | 3 | Format conversion helpers |
| `data-sanitization/` | 7 | PII redaction and anonymization |
| `devops/` | 10 | CI/CD and infrastructure processing |
| `log-processing/` | 14 | Log file parsing and filtering |
| `network/` | 9 | IP, URL, and network data processing |
| `reporting/` | 8 | Data extraction and formatting |
| `security/` | 12 | Secret detection and vulnerability scanning |
| `text-processing/` | 22 | General text transformation |

## Usage

```bash
# Use a recipe
rexpipe -c examples/recipes/data-sanitization/pii-redactor.toml < data.txt

# List available patterns in a recipe
rexpipe --validate examples/recipes/security/secrets-detector.toml

# Combine with other flags
rexpipe -c examples/recipes/log-processing/error-extractor.toml -R logs/

# Test a recipe
rexpipe -c examples/recipes/data-sanitization/pii-redactor.toml --test
```

## Popular Recipes

### Data Sanitization
- `pii-redactor.toml` - Redacts SSN, credit cards, phones, emails, IPs
- `password-remover.toml` - Removes passwords and secrets from configs
- `gdpr-compliance.toml` - Anonymizes personal data for GDPR

### Log Processing
- `error-extractor.toml` - Filters and highlights errors from logs
- `apache-parser.toml` - Parses Apache access logs
- `kubernetes-logs.toml` - Processes K8s pod logs

### Security
- `secrets-detector.toml` - Finds hardcoded secrets and API keys
- `sql-injection-finder.toml` - Detects SQL injection vulnerabilities
- `aws-key-detector.toml` - Finds AWS access keys

### Code Processing
- `todo-finder.toml` - Extracts TODO/FIXME comments
- `debug-remover.toml` - Removes console.log and print statements
- `remove-comments.toml` - Strips comments from code

## Creating Custom Recipes

Recipes are TOML files with pipeline step definitions:

```toml
name = "My Recipe"
description = "What this recipe does"

[[step]]
type = "substitute"
pattern = 'find-this'
replacement = "replace-with-this"

[[step]]
type = "filter"
pattern = 'keep-lines-matching'
action = "keep_line"

[[tests]]
name = "test_basic"
input = "test input"
expected_output = "expected output"
```

## Contributing

To add a new recipe:
1. Create a `.toml` file in the appropriate category directory
2. Include `name`, `description`, and at least one `[[step]]`
3. Add `[[tests]]` sections for validation
4. Test with `rexpipe -c your-recipe.toml --test`
