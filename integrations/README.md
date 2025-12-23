# rexpipe Integrations

Ready-to-use integration configurations for popular development tools and CI/CD platforms.

## Contents

| File | Description |
|------|-------------|
| [`github-actions.yml`](github-actions.yml) | GitHub Actions workflow for secret detection, PII scanning, and log sanitization |
| [`gitlab-ci.yml`](gitlab-ci.yml) | GitLab CI configuration with security scanning and artifact sanitization |
| [`pre-commit-config.example.yaml`](pre-commit-config.example.yaml) | Example pre-commit configuration for local development |
| [`shell-aliases.sh`](shell-aliases.sh) | Shell functions and aliases for common rexpipe operations |

## Quick Start

### Pre-commit Hooks

1. Install pre-commit:
   ```bash
   pip install pre-commit
   ```

2. Add to your project's `.pre-commit-config.yaml`:
   ```yaml
   repos:
     - repo: https://github.com/jkindrix/rexpipe
       rev: v2.0.0
       hooks:
         - id: rexpipe-secrets
         - id: rexpipe-pii
   ```

3. Install the hooks:
   ```bash
   pre-commit install
   ```

### GitHub Actions

Copy `github-actions.yml` to `.github/workflows/rexpipe.yml` in your project.

### GitLab CI

Include in your `.gitlab-ci.yml`:
```yaml
include:
  - project: 'your-group/rexpipe'
    file: '/integrations/gitlab-ci.yml'
    ref: v2.0.0
```

Or copy `gitlab-ci.yml` directly to your project.

### Shell Aliases

Add to your `.bashrc` or `.zshrc`:
```bash
source /path/to/rexpipe/integrations/shell-aliases.sh
```

Available functions:
- `secrets <files>` - Detect hardcoded secrets
- `pii <files>` - Detect personally identifiable information
- `redact-secrets` - Pipe through to redact secrets
- `redact-email` - Pipe through to redact emails
- `redact-ip` - Pipe through to redact IP addresses
- `errors <files>` - Extract ERROR lines from logs
- `warnings <files>` - Extract WARN lines from logs
- `todos [dir]` - Extract TODO/FIXME from source code
- `discover <file>` - Auto-discover patterns in data
- `pipeline <name>` - Run a named pipeline

## Editor Integrations

### VSCode

Create `.vscode/tasks.json` in your project:
```json
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "rexpipe: Check for secrets",
      "type": "shell",
      "command": "rexpipe",
      "args": [
        "--pattern", "(api[_-]?key|password|secret)[:=]",
        "--recursive", "--glob", "*.{js,ts,py,rs}",
        "--dry-run", "."
      ],
      "problemMatcher": []
    },
    {
      "label": "rexpipe: Discover patterns",
      "type": "shell",
      "command": "rexpipe",
      "args": ["--discover", "${file}"],
      "problemMatcher": []
    }
  ]
}
```

### Vim/Neovim

Add to your config:
```vim
" Run rexpipe on visual selection
vnoremap <leader>rp :!rexpipe --text<CR>

" Discover patterns in current file
nnoremap <leader>rd :!rexpipe --discover %<CR>
```

## Creating Custom Integrations

### Pipeline for CI

Create a project-specific pipeline in `.rexpipe/project.toml`:
```toml
name = "project-security-check"
description = "Project-specific security patterns"

[[step]]
type = "filter"
pattern = "INTERNAL_API_KEY"
action = "keep_line"
description = "Flag internal API keys"

[[step]]
type = "filter"
pattern = "localhost:\\d+"
action = "keep_line"
description = "Flag hardcoded localhost ports"
```

### Custom Pre-commit Hook

```yaml
- repo: local
  hooks:
    - id: project-security
      name: Project security check
      entry: rexpipe --config .rexpipe/project.toml --dry-run --quiet
      language: system
      types: [text]
      pass_filenames: true
```

## Troubleshooting

### Pre-commit is slow

The first run compiles rexpipe from source. Subsequent runs use the cached binary.
For faster cold starts, use a local hook with a pre-installed binary:

```yaml
- repo: local
  hooks:
    - id: rexpipe-local
      name: rexpipe (local)
      entry: rexpipe
      language: system  # Uses system-installed rexpipe
      # ...
```

### False positives

Use `exclude` patterns to skip test fixtures and documentation:
```yaml
- id: rexpipe-secrets
  exclude: |
    (?x)^(
      tests/fixtures/.*|
      docs/.*
    )$
```

### Exit codes

rexpipe uses grep-compatible exit codes:
- `0`: Matches found (or no matches with `--dry-run`)
- `1`: No matches found
- `2+`: Error occurred

For CI, you may want to invert the logic:
```bash
rexpipe --pattern 'SECRET' . && exit 1 || exit 0
```
