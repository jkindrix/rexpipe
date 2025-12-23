#!/usr/bin/env bash
# rexpipe Shell Aliases and Functions
# Source this file in your .bashrc or .zshrc:
#   source /path/to/rexpipe/integrations/shell-aliases.sh
#
# Or copy individual functions you find useful.

# ============================================================================
# Core Aliases
# ============================================================================

# Quick pattern replacement (like sed but with rexpipe)
alias rep='rexpipe --text'

# Recursive search with rexpipe (grep-like)
alias rg-pipe='rexpipe --recursive --text'

# Explain what a pipeline does without running it
alias rex-explain='rexpipe --explain'

# Validate a pipeline configuration
alias rex-validate='rexpipe --validate'

# ============================================================================
# Security Functions
# ============================================================================

# Detect secrets in files or stdin
secrets() {
    rexpipe \
        --pattern '(api[_-]?key|password|secret|token)\s*[:=]\s*["\x27][^\x27"]{8,}' \
        --pattern '\b(AKIA|ASIA)[A-Z0-9]{16}\b' \
        --pattern '\b(gh[ps]_[A-Za-z0-9]{36})\b' \
        --pattern '\bglpat-[A-Za-z0-9]{20}\b' \
        --pattern '\b(sk_live_|pk_live_)[A-Za-z0-9]{24,}\b' \
        --text \
        "$@"
}

# Detect PII (emails, SSNs, phone numbers)
pii() {
    rexpipe \
        --pattern '[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}' \
        --pattern '\b\d{3}-\d{2}-\d{4}\b' \
        --pattern '\b\d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}\b' \
        --text \
        "$@"
}

# Redact secrets from output (pipe-friendly)
redact-secrets() {
    rexpipe \
        --pattern '(api[_-]?key|password|secret|token)\s*[:=]\s*["\x27][^\x27"]+["\x27]' \
        --replacement '${1}="[REDACTED]"' \
        --pattern '\b(AKIA|ASIA)[A-Z0-9]{16}\b' \
        --replacement '[AWS_KEY]' \
        --pattern '\b(gh[ps]_[A-Za-z0-9]{36})\b' \
        --replacement '[GITHUB_TOKEN]' \
        --text
}

# Redact emails from output
redact-email() {
    rexpipe \
        --pattern '[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}' \
        --replacement '[EMAIL]' \
        --text
}

# Redact IPs from output
redact-ip() {
    rexpipe \
        --pattern '\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b' \
        --replacement '[IP]' \
        --text
}

# ============================================================================
# Log Processing Functions
# ============================================================================

# Extract only ERROR lines from logs
errors() {
    rexpipe --pattern '\[ERROR\]|ERROR:|level=error' --text "$@"
}

# Extract only WARN lines from logs
warnings() {
    rexpipe --pattern '\[WARN\]|WARN:|level=warn' --text "$@"
}

# Filter out DEBUG lines
no-debug() {
    rexpipe \
        --pattern '.*\[DEBUG\].*|.*DEBUG:.*|.*level=debug.*' \
        --replacement '' \
        --text
}

# Extract timestamps and messages from structured logs
log-simple() {
    rexpipe \
        --pattern '^(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2})[^\]]*\]\s*(.*)' \
        --replacement '${1} ${2}' \
        --text "$@"
}

# Anonymize logs for sharing
log-anon() {
    rexpipe \
        --pattern '[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}' \
        --replacement '[EMAIL]' \
        --pattern '\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b' \
        --replacement '[IP]' \
        --pattern '\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b' \
        --replacement '[UUID]' \
        --text
}

# ============================================================================
# Development Functions
# ============================================================================

# Extract TODO/FIXME comments from source files
todos() {
    local dir="${1:-.}"
    rexpipe \
        --pattern 'TODO|FIXME|HACK|XXX' \
        --recursive \
        --glob '*.py' \
        --glob '*.js' \
        --glob '*.ts' \
        --glob '*.rs' \
        --glob '*.go' \
        --glob '*.java' \
        --text \
        "$dir"
}

# Find console.log/print statements
debug-stmts() {
    local dir="${1:-.}"
    rexpipe \
        --pattern 'console\.(log|debug|warn)|print\(|println!|fmt\.Print|System\.out' \
        --recursive \
        --glob '*.py' \
        --glob '*.js' \
        --glob '*.ts' \
        --glob '*.rs' \
        --glob '*.go' \
        --glob '*.java' \
        --text \
        "$dir"
}

# ============================================================================
# Git Functions
# ============================================================================

# Check git diff for secrets before commit
git-check-secrets() {
    git diff --cached | secrets
}

# Sanitize git log output for sharing
git-log-safe() {
    git log --oneline -20 "$@" | redact-email
}

# ============================================================================
# Docker Functions
# ============================================================================

# Sanitize docker logs
docker-logs-safe() {
    docker logs "$@" 2>&1 | log-anon
}

# ============================================================================
# Cloud Functions
# ============================================================================

# Sanitize AWS CLI output
aws-safe() {
    aws "$@" 2>&1 | rexpipe \
        --pattern '\d{12}' \
        --replacement '[ACCOUNT]' \
        --pattern 'arn:aws[a-z-]*:[a-z0-9-]+:[a-z0-9-]*:\d{12}:[^\s]+' \
        --replacement '[ARN]' \
        --text
}

# ============================================================================
# Discovery Functions
# ============================================================================

# Discover patterns in a file
discover() {
    rexpipe --discover "$@"
}

# Learn a pattern from examples
# Usage: learn-pattern "match1" "match2" -- "nomatch1" "nomatch2"
learn-pattern() {
    local positives=()
    local negatives=()
    local is_negative=false

    for arg in "$@"; do
        if [ "$arg" = "--" ]; then
            is_negative=true
            continue
        fi
        if $is_negative; then
            negatives+=("--negative" "$arg")
        else
            positives+=("--positive" "$arg")
        fi
    done

    rexpipe --learn "${positives[@]}" "${negatives[@]}"
}

# ============================================================================
# Pipeline Functions
# ============================================================================

# Run a pipeline from examples directory
pipeline() {
    local name="$1"
    shift
    local pipeline_file=""

    # Check common locations
    for dir in \
        "$HOME/.config/rexpipe/pipelines" \
        "/usr/local/share/rexpipe/pipelines" \
        "$(dirname "$(which rexpipe 2>/dev/null)")/../share/rexpipe/pipelines" \
        "./pipelines" \
        "./examples/pipelines"; do
        if [ -f "$dir/$name.toml" ]; then
            pipeline_file="$dir/$name.toml"
            break
        fi
    done

    if [ -z "$pipeline_file" ]; then
        echo "Pipeline not found: $name" >&2
        echo "Searched in: ~/.config/rexpipe/pipelines, ./pipelines, ./examples/pipelines" >&2
        return 1
    fi

    rexpipe --config "$pipeline_file" --text "$@"
}

# List available pipelines
pipelines() {
    echo "Available pipelines:"
    for dir in \
        "$HOME/.config/rexpipe/pipelines" \
        "./pipelines" \
        "./examples/pipelines"; do
        if [ -d "$dir" ]; then
            echo "  $dir:"
            ls -1 "$dir"/*.toml 2>/dev/null | sed 's|.*/||; s|\.toml$||; s|^|    |'
        fi
    done
}

# ============================================================================
# Completion (bash)
# ============================================================================

# Generate completions if not already present
if [ -n "$BASH_VERSION" ]; then
    if command -v rexpipe &>/dev/null; then
        eval "$(rexpipe --completions bash 2>/dev/null)"
    fi
fi

# ============================================================================
# Zsh-specific completions
# ============================================================================

if [ -n "$ZSH_VERSION" ]; then
    if command -v rexpipe &>/dev/null; then
        eval "$(rexpipe --completions zsh 2>/dev/null)"
    fi
fi

echo "rexpipe aliases loaded. Try: secrets, pii, redact-secrets, todos, discover"
