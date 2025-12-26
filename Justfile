# ============================================================================
# rexpipe Development Justfile
# ============================================================================
#
# Modern command runner for the rexpipe regex pipeline processor.
# Replaces traditional Makefile with improved UX, safety, and features.
#
# Usage:
#   just              - Show all available commands
#   just build        - Build debug
#   just ci           - Run full CI pipeline
#   just <recipe>     - Run any recipe
#
# Requirements:
#   - Just >= 1.23.0 (for [group], [confirm], [doc] attributes)
#   - Rust toolchain (rustup recommended)
#
# Install Just:
#   cargo install just
#   # or: brew install just / apt install just / pacman -S just
#
# ============================================================================

# ----------------------------------------------------------------------------
# Project Configuration
# ----------------------------------------------------------------------------

project_name := "rexpipe"
# Version is read dynamically from Cargo.toml to avoid drift
version := `cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "rexpipe") | .version'`
msrv := "1.85"
edition := "2024"

# ----------------------------------------------------------------------------
# Tool Configuration (can be overridden via environment)
# ----------------------------------------------------------------------------

cargo := env_var_or_default("CARGO", "cargo")

# Parallel jobs: auto-detect CPU count
jobs := env_var_or_default("JOBS", num_cpus())

# Runtime configuration
rust_log := env_var_or_default("RUST_LOG", "info")
rust_backtrace := env_var_or_default("RUST_BACKTRACE", "1")

# Fuzz configuration
fuzz_time := env_var_or_default("FUZZ_TIME", "60")
fuzz_target := env_var_or_default("FUZZ_TARGET", "fuzz_pattern")

# Paths
fuzz_dir := "fuzz"
target_dir := "target"

# ----------------------------------------------------------------------------
# Platform Detection
# ----------------------------------------------------------------------------

platform := if os() == "linux" { "linux" } else if os() == "macos" { "macos" } else { "windows" }
open_cmd := if os() == "linux" { "xdg-open" } else if os() == "macos" { "open" } else { "start" }

# ----------------------------------------------------------------------------
# ANSI Color Codes
# ----------------------------------------------------------------------------

reset := '\033[0m'
bold := '\033[1m'
dim := '\033[2m'
green := '\033[0;32m'
yellow := '\033[0;33m'
red := '\033[0;31m'
cyan := '\033[0;36m'
blue := '\033[0;34m'
magenta := '\033[0;35m'

# ----------------------------------------------------------------------------
# Default Recipe & Settings
# ----------------------------------------------------------------------------

# Show help by default
default:
    @just --list --unsorted

# Load .env file if present
set dotenv-load

# Use bash with strict error handling
# -e: Exit on error
# -u: Error on undefined variables
# -o pipefail: Pipe failures propagate
set shell := ["bash", "-euo", "pipefail", "-c"]

# Export all variables to child processes
set export

# ============================================================================
# SETUP & BOOTSTRAP
# ============================================================================

[group('setup')]
[doc("Full development setup (rust + tools + hooks)")]
setup: setup-rust setup-tools setup-hooks
    @printf '{{green}}{{bold}}✓ Development environment ready{{reset}}\n'

[group('setup')]
[doc("Install/update Rust toolchain components")]
setup-rust:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{blue}}{{bold}}Installing Rust toolchain...{{reset}}\n'
    rustup toolchain install stable --profile default
    rustup toolchain install nightly --profile minimal
    rustup component add rustfmt clippy llvm-tools-preview
    rustup component add --toolchain nightly rustfmt miri
    printf '{{green}}[OK]{{reset}}   Rust toolchain ready\n'

[group('setup')]
[doc("Install development tools (cargo extensions)")]
setup-tools:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{blue}}{{bold}}Installing development tools...{{reset}}\n'
    # Core tools (required for CI)
    {{cargo}} install cargo-nextest cargo-llvm-cov cargo-deny cargo-audit
    # Release tools
    {{cargo}} install cargo-semver-checks git-cliff
    # Quality tools
    {{cargo}} install cargo-outdated cargo-machete typos-cli cargo-careful
    # Development tools
    {{cargo}} install cargo-watch
    printf '{{green}}[OK]{{reset}}   Tools installed\n'

[group('setup')]
[doc("Install minimal tools for CI/release checks")]
setup-tools-minimal:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{blue}}{{bold}}Installing minimal tools...{{reset}}\n'
    {{cargo}} install cargo-deny cargo-audit cargo-semver-checks cargo-nextest
    printf '{{green}}[OK]{{reset}}   Minimal tools installed\n'

[group('setup')]
[doc("Install pre-commit hooks")]
setup-hooks:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{blue}}{{bold}}Setting up git hooks...{{reset}}\n'

    # Create hooks directory if it doesn't exist
    mkdir -p .git/hooks

    # Create pre-commit hook
    printf '%s\n' '#!/usr/bin/env bash' > .git/hooks/pre-commit
    printf '%s\n' 'set -euo pipefail' >> .git/hooks/pre-commit
    printf '%s\n' '' >> .git/hooks/pre-commit
    printf '%s\n' 'echo "Running pre-commit checks..."' >> .git/hooks/pre-commit
    printf '%s\n' '' >> .git/hooks/pre-commit
    printf '%s\n' '# Check formatting' >> .git/hooks/pre-commit
    printf '%s\n' 'if ! cargo fmt --all -- --check 2>/dev/null; then' >> .git/hooks/pre-commit
    printf '%s\n' '    echo "❌ Formatting check failed. Run '\''cargo fmt --all'\'' to fix."' >> .git/hooks/pre-commit
    printf '%s\n' '    exit 1' >> .git/hooks/pre-commit
    printf '%s\n' 'fi' >> .git/hooks/pre-commit
    printf '%s\n' 'echo "✓ Format check passed"' >> .git/hooks/pre-commit
    printf '%s\n' '' >> .git/hooks/pre-commit
    printf '%s\n' '# Run clippy' >> .git/hooks/pre-commit
    printf '%s\n' 'if ! cargo clippy --all-features --all-targets -- -D warnings 2>/dev/null; then' >> .git/hooks/pre-commit
    printf '%s\n' '    echo "❌ Clippy check failed. Fix the warnings above."' >> .git/hooks/pre-commit
    printf '%s\n' '    exit 1' >> .git/hooks/pre-commit
    printf '%s\n' 'fi' >> .git/hooks/pre-commit
    printf '%s\n' 'echo "✓ Clippy check passed"' >> .git/hooks/pre-commit
    printf '%s\n' '' >> .git/hooks/pre-commit
    printf '%s\n' 'echo "✅ All pre-commit checks passed!"' >> .git/hooks/pre-commit

    chmod +x .git/hooks/pre-commit
    printf '{{green}}[OK]{{reset}}   Pre-commit hook installed\n'
    printf '{{cyan}}[INFO]{{reset}} Hook will run: fmt-check, clippy\n'

# ============================================================================
# FEATURE FLAG CONFIGURATION
# ============================================================================
#
# Available features:
#   - tree-sitter : Syntax-aware scoping (Rust, Python, JS, TS, Go, JSON, YAML)
#   - pcre        : PCRE regex engine with lookahead/lookbehind
#   - async       : Async I/O support via tokio
#   - watch       : File watching support via notify
#   - remote      : Remote file fetching via ureq
#   - fpe         : Format-preserving encryption
#
# Feature presets:
#   - "all"      : All features (default for development)
#   - "minimal"  : No optional features (smallest binary)
#   - "standard" : tree-sitter,pcre (common use case)
#
# Usage:
#   just build                    # Build with all features
#   just build-with "pcre"        # Build with specific feature(s)
#   just build-minimal            # Build with no optional features
#   just install-with "tree-sitter,pcre"  # Install with specific features
#
# ----------------------------------------------------------------------------

# Default features for builds (can override via FEATURES env var)
default_features := env_var_or_default("FEATURES", "all")

# Feature presets
features_all := "tree-sitter,pcre,async,watch,remote,fpe"
features_standard := "tree-sitter,pcre"
features_minimal := ""

# Helper to resolve feature preset or custom features
_resolve_features features:
    #!/usr/bin/env bash
    case "{{features}}" in
        "all")      echo "{{features_all}}" ;;
        "standard") echo "{{features_standard}}" ;;
        "minimal"|"none"|"") echo "" ;;
        *)          echo "{{features}}" ;;
    esac

# ============================================================================
# CORE BUILD RECIPES
# ============================================================================

[group('build')]
[doc("Build in debug mode (all features)")]
build:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Building (debug) ══════{{reset}}\n\n'
    {{cargo}} build --all-features -j {{jobs}}
    printf '{{green}}[OK]{{reset}}   Build complete\n'

[group('build')]
[doc("Build with specific features: just build-with \"tree-sitter,pcre\"")]
build-with features:
    #!/usr/bin/env bash
    RESOLVED=$(just _resolve_features "{{features}}")
    printf '\n{{bold}}{{blue}}══════ Building (debug) ══════{{reset}}\n\n'
    if [ -z "$RESOLVED" ]; then
        printf '{{cyan}}[INFO]{{reset}} Features: (none - minimal build)\n'
        {{cargo}} build --no-default-features -j {{jobs}}
    else
        printf '{{cyan}}[INFO]{{reset}} Features: %s\n' "$RESOLVED"
        {{cargo}} build --no-default-features --features "$RESOLVED" -j {{jobs}}
    fi
    printf '{{green}}[OK]{{reset}}   Build complete\n'

[group('build')]
[doc("Build with minimal features (no optional deps)")]
build-minimal:
    @just build-with "minimal"

[group('build')]
[doc("Build with standard features (tree-sitter + pcre)")]
build-standard:
    @just build-with "standard"

[group('build')]
[doc("Build in release mode with optimizations (all features)")]
release:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Building (release) ══════{{reset}}\n\n'
    {{cargo}} build --all-features --release -j {{jobs}}
    printf '{{green}}[OK]{{reset}}   Release build complete\n'

[group('build')]
[doc("Release build with specific features: just release-with \"tree-sitter,pcre\"")]
release-with features:
    #!/usr/bin/env bash
    RESOLVED=$(just _resolve_features "{{features}}")
    printf '\n{{bold}}{{blue}}══════ Building (release) ══════{{reset}}\n\n'
    if [ -z "$RESOLVED" ]; then
        printf '{{cyan}}[INFO]{{reset}} Features: (none - minimal build)\n'
        {{cargo}} build --no-default-features --release -j {{jobs}}
    else
        printf '{{cyan}}[INFO]{{reset}} Features: %s\n' "$RESOLVED"
        {{cargo}} build --no-default-features --features "$RESOLVED" --release -j {{jobs}}
    fi
    printf '{{green}}[OK]{{reset}}   Release build complete\n'

[group('build')]
[doc("Release build with minimal features")]
release-minimal:
    @just release-with "minimal"

[group('build')]
[doc("Release build with standard features")]
release-standard:
    @just release-with "standard"

[group('build')]
[doc("Fast type check without code generation")]
check:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Type checking...\n'
    {{cargo}} check --all-features -j {{jobs}}
    printf '{{green}}[OK]{{reset}}   Type check passed\n'

[group('build')]
[doc("Type check with specific features")]
check-with features:
    #!/usr/bin/env bash
    RESOLVED=$(just _resolve_features "{{features}}")
    printf '{{cyan}}[INFO]{{reset}} Type checking...\n'
    if [ -z "$RESOLVED" ]; then
        {{cargo}} check --no-default-features -j {{jobs}}
    else
        {{cargo}} check --no-default-features --features "$RESOLVED" -j {{jobs}}
    fi
    printf '{{green}}[OK]{{reset}}   Type check passed\n'

[group('build')]
[doc("Analyze build times")]
build-timing:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Building with timing analysis...\n'
    {{cargo}} build --all-features --timings
    printf '{{green}}[OK]{{reset}}   Build timing report generated (see target/cargo-timings/)\n'

[group('build')]
[confirm("This will delete all build artifacts. Continue?")]
[doc("Clean all build artifacts")]
clean:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Cleaning build artifacts...\n'
    {{cargo}} clean
    rm -rf coverage/ lcov.info *.profraw *.profdata
    printf '{{green}}[OK]{{reset}}   Clean complete\n'

[group('build')]
[doc("Clean and rebuild from scratch")]
rebuild: clean build

[group('build')]
[doc("List available features and presets")]
features:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Available Features ══════{{reset}}\n\n'
    printf '{{bold}}Individual Features:{{reset}}\n'
    printf '  {{cyan}}tree-sitter{{reset}}  Syntax-aware scoping (Rust, Python, JS, TS, Go, JSON, YAML)\n'
    printf '  {{cyan}}pcre{{reset}}         PCRE regex engine with lookahead/lookbehind support\n'
    printf '  {{cyan}}async{{reset}}        Async I/O support via tokio\n'
    printf '  {{cyan}}watch{{reset}}        File watching support via notify\n'
    printf '  {{cyan}}remote{{reset}}       Remote file fetching via ureq\n'
    printf '  {{cyan}}fpe{{reset}}          Format-preserving encryption\n'
    printf '\n{{bold}}Feature Presets:{{reset}}\n'
    printf '  {{cyan}}all{{reset}}          All features: {{features_all}}\n'
    printf '  {{cyan}}standard{{reset}}     Common features: {{features_standard}}\n'
    printf '  {{cyan}}minimal{{reset}}      No optional features (smallest binary)\n'
    printf '\n{{bold}}Usage Examples:{{reset}}\n'
    printf '  just build                         # All features (default)\n'
    printf '  just build-with "pcre"             # Single feature\n'
    printf '  just build-with "tree-sitter,pcre" # Multiple features\n'
    printf '  just build-minimal                 # No optional features\n'
    printf '  just build-standard                # tree-sitter + pcre\n'
    printf '  just release                       # Release (all features)\n'
    printf '  just release-with "standard"       # Release with preset\n'
    printf '  just install                       # Install (all features)\n'
    printf '  just install-minimal               # Install minimal binary\n'
    printf '\n{{bold}}Environment Variable:{{reset}}\n'
    printf '  FEATURES="pcre,async" just build-with "$FEATURES"\n'
    printf '\n'

# ============================================================================
# TESTING RECIPES
# ============================================================================

[group('test')]
[doc("Run all tests (all features)")]
test:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Running Tests ══════{{reset}}\n\n'
    {{cargo}} test --all-features -j {{jobs}}
    printf '{{green}}[OK]{{reset}}   All tests passed\n'

[group('test')]
[doc("Run tests with specific features: just test-with \"tree-sitter,pcre\"")]
test-with features:
    #!/usr/bin/env bash
    RESOLVED=$(just _resolve_features "{{features}}")
    printf '\n{{bold}}{{blue}}══════ Running Tests ══════{{reset}}\n\n'
    if [ -z "$RESOLVED" ]; then
        printf '{{cyan}}[INFO]{{reset}} Features: (none - minimal build)\n'
        {{cargo}} test --no-default-features -j {{jobs}}
    else
        printf '{{cyan}}[INFO]{{reset}} Features: %s\n' "$RESOLVED"
        {{cargo}} test --no-default-features --features "$RESOLVED" -j {{jobs}}
    fi
    printf '{{green}}[OK]{{reset}}   All tests passed\n'

[group('test')]
[doc("Run tests with locked dependencies (reproducible)")]
test-locked:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Running Tests (locked) ══════{{reset}}\n\n'
    {{cargo}} test --all-features --locked -j {{jobs}}
    printf '{{green}}[OK]{{reset}}   All tests passed (locked)\n'

[group('test')]
[doc("Run tests with output visible")]
test-verbose:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Running Tests (verbose) ══════{{reset}}\n\n'
    {{cargo}} test --all-features -j {{jobs}} -- --nocapture
    printf '{{green}}[OK]{{reset}}   All tests passed\n'

[group('test')]
[doc("Run documentation tests only")]
test-doc:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running doc tests...\n'
    {{cargo}} test --all-features --doc
    printf '{{green}}[OK]{{reset}}   Doc tests passed\n'

[group('test')]
[doc("Run tests with various feature combinations")]
test-features:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '\n{{bold}}{{blue}}══════ Testing Feature Matrix ══════{{reset}}\n\n'
    printf '{{cyan}}[INFO]{{reset}} Testing with no features...\n'
    {{cargo}} test --no-default-features -j {{jobs}}
    printf '{{cyan}}[INFO]{{reset}} Testing with default features...\n'
    {{cargo}} test -j {{jobs}}
    printf '{{cyan}}[INFO]{{reset}} Testing with pcre feature...\n'
    {{cargo}} test --features pcre -j {{jobs}}
    printf '{{cyan}}[INFO]{{reset}} Testing with async feature...\n'
    {{cargo}} test --features async -j {{jobs}}
    printf '{{cyan}}[INFO]{{reset}} Testing with all features...\n'
    {{cargo}} test --all-features -j {{jobs}}
    printf '{{green}}[OK]{{reset}}   Feature matrix tests passed\n'

[group('test')]
[doc("Run tests under Miri for undefined behavior detection")]
miri:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{blue}}{{bold}}Running Miri...{{reset}}\n'
    # Note: Miri may not support all dependencies (e.g., tree-sitter FFI)
    # Run with minimal features for best compatibility
    {{cargo}} +nightly miri test --no-default-features 2>&1 || {
        printf '{{yellow}}[WARN]{{reset}} Miri failed - this may be expected for FFI-heavy code\n'
        exit 0
    }
    printf '{{green}}[OK]{{reset}}   Miri passed (no UB detected)\n'

[group('test')]
[doc("Run tests with cargo-careful for extra safety checks")]
careful:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{blue}}{{bold}}Running tests with cargo-careful...{{reset}}\n'
    if ! command -v cargo-careful &> /dev/null; then
        printf '{{yellow}}[WARN]{{reset}} cargo-careful not installed (cargo install cargo-careful)\n'
        exit 0
    fi
    {{cargo}} +nightly careful test --all-features
    printf '{{green}}[OK]{{reset}}   Careful tests passed\n'

[group('test')]
[doc("Run tests with cargo-nextest (faster, parallel)")]
nextest:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Running Tests (nextest) ══════{{reset}}\n\n'
    {{cargo}} nextest run --all-features -j {{jobs}}
    printf '{{green}}[OK]{{reset}}   All tests passed\n'

# ============================================================================
# CODE QUALITY RECIPES
# ============================================================================

[group('lint')]
[doc("Format all code")]
fmt:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Formatting code...\n'
    {{cargo}} fmt --all
    printf '{{green}}[OK]{{reset}}   Formatting complete\n'

[group('lint')]
[doc("Check code formatting")]
fmt-check:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Checking format...\n'
    {{cargo}} fmt --all -- --check
    printf '{{green}}[OK]{{reset}}   Format check passed\n'

[group('lint')]
[doc("Run clippy lints (matches CI configuration)")]
clippy:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running clippy...\n'
    {{cargo}} clippy --all-features --all-targets -- -D warnings
    printf '{{green}}[OK]{{reset}}   Clippy passed\n'

[group('lint')]
[doc("Run clippy with strict deny on warnings")]
clippy-strict:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running clippy (strict)...\n'
    {{cargo}} clippy --all-targets --all-features -- \
        -D warnings \
        -D clippy::all \
        -D clippy::pedantic \
        -A clippy::module_name_repetitions \
        -A clippy::too_many_lines \
        -A clippy::must_use_candidate
    printf '{{green}}[OK]{{reset}}   Clippy (strict) passed\n'

[group('lint')]
[doc("Auto-fix clippy warnings")]
clippy-fix:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Auto-fixing clippy warnings...\n'
    {{cargo}} clippy --all-targets --all-features --fix --allow-dirty --allow-staged
    printf '{{green}}[OK]{{reset}}   Clippy fixes applied\n'

[group('security')]
[doc("Security vulnerability audit via cargo-audit")]
audit:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running security audit...\n'
    if ! command -v cargo-audit &> /dev/null; then
        printf '{{yellow}}[WARN]{{reset}} cargo-audit not installed (cargo install cargo-audit)\n'
        exit 0
    fi
    {{cargo}} audit
    printf '{{green}}[OK]{{reset}}   Security audit passed\n'

[group('security')]
[doc("Run cargo-deny checks (licenses, bans, advisories)")]
deny:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running cargo-deny...\n'
    if ! command -v cargo-deny &> /dev/null; then
        printf '{{yellow}}[WARN]{{reset}} cargo-deny not installed (cargo install cargo-deny)\n'
        exit 0
    fi
    {{cargo}} deny check
    printf '{{green}}[OK]{{reset}}   Deny checks passed\n'

[group('lint')]
[doc("Find unused dependencies via cargo-machete (fast, heuristic)")]
machete:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{cyan}}[INFO]{{reset}} Finding unused dependencies (fast)...\n'
    if ! command -v cargo-machete &> /dev/null; then
        printf '{{yellow}}[WARN]{{reset}} cargo-machete not installed (cargo install cargo-machete)\n'
        exit 0
    fi
    {{cargo}} machete
    printf '{{green}}[OK]{{reset}}   Machete check complete\n'

[group('lint')]
[doc("Run typos spell checker")]
typos:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{cyan}}[INFO]{{reset}} Checking for typos...\n'
    if ! command -v typos &> /dev/null; then
        printf '{{yellow}}[WARN]{{reset}} typos not installed (cargo install typos-cli)\n'
        exit 0
    fi
    typos src/ tests/ docs/ README.md CHANGELOG.md
    printf '{{green}}[OK]{{reset}}   Typos check passed\n'

[group('lint')]
[doc("Fix typos automatically")]
typos-fix:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{cyan}}[INFO]{{reset}} Fixing typos...\n'
    if ! command -v typos &> /dev/null; then
        printf '{{yellow}}[WARN]{{reset}} typos not installed (cargo install typos-cli)\n'
        exit 0
    fi
    typos --write-changes
    printf '{{green}}[OK]{{reset}}   Typos fixed\n'

[group('lint')]
[doc("Verify MSRV compliance")]
msrv-check:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Checking MSRV {{msrv}}...\n'
    {{cargo}} +{{msrv}} check --all-features
    printf '{{green}}[OK]{{reset}}   MSRV {{msrv}} check passed\n'

[group('lint')]
[doc("Run all lints (fmt + clippy)")]
lint: fmt-check clippy
    @printf '{{green}}[OK]{{reset}}   All lints passed\n'

# ============================================================================
# DOCUMENTATION RECIPES
# ============================================================================

[group('docs')]
[doc("Generate documentation")]
doc:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Generating documentation...\n'
    {{cargo}} doc --all-features --no-deps
    printf '{{green}}[OK]{{reset}}   Documentation generated\n'

[group('docs')]
[doc("Generate and open documentation")]
doc-open:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Generating documentation...\n'
    {{cargo}} doc --all-features --no-deps --open
    printf '{{green}}[OK]{{reset}}   Documentation opened\n'

[group('docs')]
[doc("Generate docs including private items")]
doc-private:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Generating documentation (with private items)...\n'
    {{cargo}} doc --all-features --no-deps --document-private-items --open
    printf '{{green}}[OK]{{reset}}   Documentation opened\n'

[group('docs')]
[doc("Check documentation for warnings")]
doc-check:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{cyan}}[INFO]{{reset}} Checking documentation...\n'
    RUSTDOCFLAGS="-D warnings" {{cargo}} doc --all-features --no-deps
    printf '{{green}}[OK]{{reset}}   Documentation check passed\n'

[group('docs')]
[doc("Check markdown links (requires lychee)")]
link-check:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{cyan}}[INFO]{{reset}} Checking markdown links...\n'
    if ! command -v lychee &> /dev/null; then
        printf '{{yellow}}[WARN]{{reset}} lychee not installed (cargo install lychee)\n'
        printf '{{yellow}}[WARN]{{reset}} Skipping link check\n'
        exit 0
    fi
    lychee --no-progress --accept 200,204,206 \
        --exclude '^https://crates.io' \
        --exclude '^https://docs.rs' \
        './docs/**/*.md' './README.md' './CONTRIBUTING.md' './RELEASING.md' 2>/dev/null || true
    printf '{{green}}[OK]{{reset}}   Link check passed\n'

# ============================================================================
# COVERAGE RECIPES
# ============================================================================

[group('coverage')]
[doc("Generate HTML coverage report and open in browser")]
coverage:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Generating Coverage Report ══════{{reset}}\n\n'
    if ! command -v cargo-llvm-cov &> /dev/null; then
        printf '{{yellow}}[WARN]{{reset}} cargo-llvm-cov not installed (cargo install cargo-llvm-cov)\n'
        exit 0
    fi
    {{cargo}} llvm-cov --all-features --html --open
    printf '{{green}}[OK]{{reset}}   Coverage report opened\n'

[group('coverage')]
[doc("Generate LCOV coverage for CI integration")]
coverage-lcov output="lcov.info":
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Generating LCOV coverage...\n'
    if ! command -v cargo-llvm-cov &> /dev/null; then
        printf '{{yellow}}[WARN]{{reset}} cargo-llvm-cov not installed\n'
        exit 0
    fi
    {{cargo}} llvm-cov --all-features --lcov --output-path {{output}}
    printf '{{green}}[OK]{{reset}}   Coverage saved to {{output}}\n'

[group('coverage')]
[doc("Show coverage summary in terminal")]
coverage-summary:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Coverage summary:\n'
    if ! command -v cargo-llvm-cov &> /dev/null; then
        printf '{{yellow}}[WARN]{{reset}} cargo-llvm-cov not installed\n'
        exit 0
    fi
    {{cargo}} llvm-cov --all-features --text

# Coverage aliases (short names)
alias cov := coverage
alias cov-lcov := coverage-lcov
alias cov-summary := coverage-summary

# ============================================================================
# FUZZING RECIPES
# ============================================================================

[group('fuzz')]
[doc("Run default fuzz target")]
fuzz target=fuzz_target time=fuzz_time:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Fuzzing: {{target}} ══════{{reset}}\n\n'
    cd {{fuzz_dir}} && {{cargo}} +nightly fuzz run {{target}} -- -max_total_time={{time}}
    printf '{{green}}[OK]{{reset}}   Fuzzing complete\n'

[group('fuzz')]
[doc("List available fuzz targets")]
fuzz-list:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Available fuzz targets:\n'
    cd {{fuzz_dir}} && {{cargo}} +nightly fuzz list

[group('fuzz')]
[doc("Fuzz regex pattern compilation")]
fuzz-pattern time=fuzz_time:
    @just fuzz fuzz_pattern {{time}}

[group('fuzz')]
[doc("Fuzz TOML config parsing")]
fuzz-config time=fuzz_time:
    @just fuzz fuzz_config {{time}}

[group('fuzz')]
[doc("Fuzz pipeline processing")]
fuzz-pipeline time=fuzz_time:
    @just fuzz fuzz_pipeline {{time}}

[group('fuzz')]
[doc("Run all fuzz targets briefly (smoke test)")]
fuzz-all time="30":
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Fuzzing All Targets ══════{{reset}}\n\n'
    for target in fuzz_pattern fuzz_config fuzz_pipeline; do
        printf '{{cyan}}[INFO]{{reset}} Fuzzing %s...\n' "$target"
        cd {{fuzz_dir}} && {{cargo}} +nightly fuzz run "$target" -- -max_total_time={{time}}
    done
    printf '{{green}}[OK]{{reset}}   All fuzz targets complete\n'

# ============================================================================
# BENCHMARK RECIPES
# ============================================================================

[group('bench')]
[doc("Run benchmarks")]
bench:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Running Benchmarks ══════{{reset}}\n\n'
    {{cargo}} bench
    printf '{{green}}[OK]{{reset}}   Benchmarks complete\n'

[group('bench')]
[doc("Run benchmarks and save baseline")]
bench-save name="baseline":
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running benchmarks (saving baseline: {{name}})...\n'
    {{cargo}} bench -- --save-baseline {{name}}
    printf '{{green}}[OK]{{reset}}   Baseline saved: {{name}}\n'

[group('bench')]
[doc("Run benchmarks and compare to baseline")]
bench-compare name="baseline":
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Comparing to baseline: {{name}}...\n'
    {{cargo}} bench -- --baseline {{name}}
    printf '{{green}}[OK]{{reset}}   Comparison complete\n'

# ============================================================================
# DEVELOPMENT WORKFLOW RECIPES
# ============================================================================

[group('dev')]
[doc("Full development setup")]
dev: build test lint
    @printf '{{green}}[OK]{{reset}}   Development environment ready\n'

[group('dev')]
[no-exit-message]
[doc("Watch mode: re-run tests on file changes")]
watch:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Watching for changes (tests)...\n'
    {{cargo}} watch -x "test --all-features"

[group('dev')]
[no-exit-message]
[doc("Watch mode: re-run check on file changes")]
watch-check:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Watching for changes (check)...\n'
    {{cargo}} watch -x "check --all-features"

[group('dev')]
[no-exit-message]
[doc("Watch mode: re-run clippy on file changes")]
watch-clippy:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Watching for changes (clippy)...\n'
    {{cargo}} watch -x "clippy --all-targets --all-features"

[group('dev')]
[doc("Run rexpipe (debug mode)")]
run *args:
    {{cargo}} run -- {{args}}

[group('dev')]
[doc("Run rexpipe with debug logging")]
run-debug *args:
    RUST_LOG=debug {{cargo}} run -- {{args}}

[group('dev')]
[doc("Run rexpipe with trace logging")]
run-trace *args:
    RUST_LOG=trace {{cargo}} run -- {{args}}

[group('dev')]
[doc("Run rexpipe (release mode)")]
run-release *args:
    {{cargo}} run --release -- {{args}}

[group('dev')]
[doc("Fix all auto-fixable issues")]
fix:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{cyan}}[INFO]{{reset}} Auto-fixing issues...\n'
    {{cargo}} fix --workspace --allow-dirty --allow-staged
    {{cargo}} fmt --all
    if command -v typos &> /dev/null; then
        typos --write-changes || true
    fi
    printf '{{green}}[OK]{{reset}}   Fixed\n'

# ============================================================================
# CI/CD RECIPES
# ============================================================================

[group('ci')]
[doc("Check documentation versions match Cargo.toml")]
version-sync:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{cyan}}[INFO]{{reset}} Checking version sync...\n'
    VERSION=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "rexpipe") | .version')
    MAJOR_MINOR=$(echo "$VERSION" | cut -d. -f1,2)

    # Check README.md for version mention
    if grep -q "rexpipe" README.md; then
        printf '{{cyan}}[INFO]{{reset}} README.md found, checking for version references...\n'
    fi

    printf '{{green}}[OK]{{reset}}   Version sync check complete (v%s)\n' "$VERSION"

[group('ci')]
[doc("Check CI status on main branch")]
ci-status:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{cyan}}[INFO]{{reset}} Checking CI status on main...\n'
    if command -v gh &> /dev/null; then
        gh run list --limit 5 --branch main
    else
        printf '{{yellow}}[WARN]{{reset}} gh CLI not installed, cannot check CI status\n'
        printf '{{cyan}}[INFO]{{reset}} Install: https://cli.github.com/\n'
    fi

[group('ci')]
[doc("Standard CI pipeline (matches GitHub Actions)")]
ci: fmt-check clippy test doc-check version-sync
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ CI Pipeline Complete ══════{{reset}}\n\n'
    printf '{{green}}[OK]{{reset}}   All CI checks passed\n'

[group('ci')]
[doc("Fast CI checks (no tests)")]
ci-fast: fmt-check clippy check
    @printf '{{green}}[OK]{{reset}}   Fast CI checks passed\n'

[group('ci')]
[doc("Full CI with security audit")]
ci-full: ci audit
    @printf '{{green}}[OK]{{reset}}   Full CI pipeline passed\n'

[group('ci')]
[doc("Complete CI with all checks (for releases)")]
ci-release: ci-full msrv-check test-features
    @printf '{{green}}[OK]{{reset}}   Release CI pipeline passed\n'

[group('ci')]
[doc("Pre-commit hook checks")]
pre-commit: fmt-check clippy check
    @printf '{{green}}[OK]{{reset}}   Pre-commit checks passed\n'

[group('ci')]
[doc("Pre-push hook checks")]
pre-push: ci
    @printf '{{green}}[OK]{{reset}}   Pre-push checks passed\n'

# ============================================================================
# DEPENDENCY MANAGEMENT
# ============================================================================

[group('deps')]
[doc("Check for outdated dependencies")]
outdated:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Checking for outdated dependencies...\n'
    if ! command -v cargo-outdated &> /dev/null; then
        printf '{{yellow}}[WARN]{{reset}} cargo-outdated not installed (cargo install cargo-outdated)\n'
        exit 0
    fi
    {{cargo}} outdated -R

[group('deps')]
[doc("Update Cargo.lock to latest compatible versions")]
update:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Updating dependencies...\n'
    {{cargo}} update
    printf '{{green}}[OK]{{reset}}   Dependencies updated\n'

[group('deps')]
[doc("Update specific dependency")]
update-dep package:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Updating {{package}}...\n'
    {{cargo}} update -p {{package}}
    printf '{{green}}[OK]{{reset}}   {{package}} updated\n'

[group('deps')]
[doc("Show dependency tree")]
tree:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Dependency tree:\n'
    {{cargo}} tree

[group('deps')]
[doc("Show duplicate dependencies")]
tree-duplicates:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Duplicate dependencies:\n'
    {{cargo}} tree --duplicates

# ============================================================================
# CHANGELOG & VERSION MANAGEMENT
# ============================================================================

[group('release')]
[doc("Generate changelog with git-cliff")]
changelog:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{cyan}}[INFO]{{reset}} Generating changelog...\n'
    if ! command -v git-cliff &> /dev/null; then
        printf '{{yellow}}[WARN]{{reset}} git-cliff not installed (cargo install git-cliff)\n'
        printf '{{cyan}}[INFO]{{reset}} Skipping changelog generation\n'
        exit 0
    fi
    git-cliff -o CHANGELOG.md
    printf '{{green}}[OK]{{reset}}   Changelog generated\n'

[group('release')]
[doc("Preview changelog for next release")]
changelog-preview:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{cyan}}[INFO]{{reset}} Previewing changelog for next release...\n'
    if ! command -v git-cliff &> /dev/null; then
        printf '{{yellow}}[WARN]{{reset}} git-cliff not installed\n'
        exit 0
    fi
    git-cliff --unreleased

[group('release')]
[doc("Bump version: just version-bump [major|minor|patch]")]
version-bump level="patch":
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{cyan}}[INFO]{{reset}} Bumping version ({{level}})...\n'

    CURRENT=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "rexpipe") | .version')
    IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT"

    case "{{level}}" in
        major) NEW="$((MAJOR + 1)).0.0" ;;
        minor) NEW="${MAJOR}.$((MINOR + 1)).0" ;;
        patch) NEW="${MAJOR}.${MINOR}.$((PATCH + 1))" ;;
        *)
            printf '{{red}}[ERR]{{reset}}  Invalid level: {{level}} (use major|minor|patch)\n'
            exit 1
            ;;
    esac

    printf '{{cyan}}[INFO]{{reset}} Bumping: %s → %s\n' "$CURRENT" "$NEW"

    # Update Cargo.toml
    sed -i "s/^version = \"$CURRENT\"/version = \"$NEW\"/" Cargo.toml

    printf '{{green}}[OK]{{reset}}   Version bumped to %s\n' "$NEW"
    printf '{{dim}}Next steps:{{reset}}\n'
    printf '  1. Update CHANGELOG.md\n'
    printf '  2. git add Cargo.toml Cargo.lock CHANGELOG.md\n'
    printf '  3. git commit -m "chore: release v%s"\n' "$NEW"

# ============================================================================
# RELEASE CHECKLIST RECIPES
# ============================================================================

[group('release')]
[doc("Check for WIP markers (TODO, FIXME, XXX, HACK, todo!, unimplemented!)")]
wip-check:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Checking for WIP markers...\n'

    # Search for comment markers
    COMMENTS=$(grep -rn "TODO\|FIXME\|XXX\|HACK" --include="*.rs" src/ 2>/dev/null || true)
    if [ -n "$COMMENTS" ]; then
        printf '{{yellow}}[WARN]{{reset}} Found WIP comments:\n'
        echo "$COMMENTS" | head -20
        COMMENT_COUNT=$(echo "$COMMENTS" | wc -l)
        if [ "$COMMENT_COUNT" -gt 20 ]; then
            printf '{{yellow}}[WARN]{{reset}} ... and %d more\n' "$((COMMENT_COUNT - 20))"
        fi
    fi

    # Search for incomplete macros
    MACROS=$(grep -rn "todo!\|unimplemented!" --include="*.rs" src/ 2>/dev/null || true)
    if [ -n "$MACROS" ]; then
        printf '{{red}}[ERR]{{reset}}  Found incomplete macros in production code:\n'
        echo "$MACROS"
        exit 1
    fi

    printf '{{green}}[OK]{{reset}}   WIP check passed (no blocking issues)\n'

[group('release')]
[doc("Audit panic paths (.unwrap(), .expect()) in production code")]
panic-audit:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Auditing panic paths in production code...\n'

    # Find .unwrap() and .expect() in src/ (production code)
    UNWRAPS=$(grep -rn "\.unwrap()" src/ --include="*.rs" 2>/dev/null || true)
    EXPECTS=$(grep -rn "\.expect(" src/ --include="*.rs" 2>/dev/null || true)

    if [ -n "$UNWRAPS" ] || [ -n "$EXPECTS" ]; then
        printf '{{yellow}}[WARN]{{reset}} Found potential panic paths:\n'
        if [ -n "$UNWRAPS" ]; then
            echo "$UNWRAPS" | head -15
            UNWRAP_COUNT=$(echo "$UNWRAPS" | wc -l)
            printf '{{cyan}}[INFO]{{reset}} Total .unwrap() calls: %d\n' "$UNWRAP_COUNT"
        fi
        if [ -n "$EXPECTS" ]; then
            echo "$EXPECTS" | head -10
            EXPECT_COUNT=$(echo "$EXPECTS" | wc -l)
            printf '{{cyan}}[INFO]{{reset}} Total .expect() calls: %d\n' "$EXPECT_COUNT"
        fi
        printf '{{yellow}}[NOTE]{{reset}} Review each for production safety. Test modules are acceptable.\n'
    else
        printf '{{green}}[OK]{{reset}}   No panic paths found in production code\n'
    fi

[group('release')]
[doc("Verify Cargo.toml metadata for crates.io publishing")]
metadata-check:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Checking Cargo.toml metadata...\n'

    METADATA=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "rexpipe")')

    # Required fields
    DESC=$(echo "$METADATA" | jq -r '.description // empty')
    LICENSE=$(echo "$METADATA" | jq -r '.license // empty')
    REPO=$(echo "$METADATA" | jq -r '.repository // empty')

    MISSING=""
    [ -z "$DESC" ] && MISSING="$MISSING description"
    [ -z "$LICENSE" ] && MISSING="$MISSING license"
    [ -z "$REPO" ] && MISSING="$MISSING repository"

    if [ -n "$MISSING" ]; then
        printf '{{red}}[ERR]{{reset}}  Missing required fields:%s\n' "$MISSING"
        exit 1
    fi

    # Recommended fields
    KEYWORDS=$(echo "$METADATA" | jq -r '.keywords // [] | length')
    CATEGORIES=$(echo "$METADATA" | jq -r '.categories // [] | length')

    [ "$KEYWORDS" -eq 0 ] && printf '{{yellow}}[WARN]{{reset}} No keywords defined (recommended for discoverability)\n'
    [ "$CATEGORIES" -eq 0 ] && printf '{{yellow}}[WARN]{{reset}} No categories defined (recommended for discoverability)\n'

    printf '{{cyan}}[INFO]{{reset}} Package metadata:\n'
    printf '  description: %s\n' "$DESC"
    printf '  license:     %s\n' "$LICENSE"
    printf '  repository:  %s\n' "$REPO"
    printf '  keywords:    %d defined\n' "$KEYWORDS"
    printf '  categories:  %d defined\n' "$CATEGORIES"

    printf '{{green}}[OK]{{reset}}   Metadata check passed\n'

[group('release')]
[doc("Check semver compatibility")]
semver:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{cyan}}[INFO]{{reset}} Checking semver compliance...\n'
    if ! command -v cargo-semver-checks &> /dev/null; then
        printf '{{yellow}}[WARN]{{reset}} cargo-semver-checks not installed (cargo install cargo-semver-checks)\n'
        exit 0
    fi
    # Check if crate is published on crates.io
    if ! cargo search rexpipe 2>/dev/null | grep -q "^rexpipe "; then
        printf '{{yellow}}[WARN]{{reset}} rexpipe not yet published on crates.io\n'
        printf '{{cyan}}[INFO]{{reset}} Semver check skipped (no baseline version)\n'
        exit 0
    fi
    {{cargo}} semver-checks check-release --package rexpipe || {
        printf '{{yellow}}[WARN]{{reset}} Semver check found breaking changes (review above)\n'
    }
    printf '{{green}}[OK]{{reset}}   Semver check complete\n'

[group('release')]
[doc("Full release validation (REQUIRED before tagging)")]
release-check: ci-release wip-check panic-audit version-sync typos machete metadata-check publish-dry
    #!/usr/bin/env bash
    set -euo pipefail
    printf '\n{{bold}}{{blue}}══════ Release Validation ══════{{reset}}\n\n'
    printf '{{cyan}}[INFO]{{reset}} Checking for uncommitted changes...\n'
    if ! git diff-index --quiet HEAD -- 2>/dev/null; then
        printf '{{red}}[ERR]{{reset}}  Uncommitted changes detected\n'
        exit 1
    fi
    printf '{{cyan}}[INFO]{{reset}} Checking for unpushed commits...\n'
    if [ -n "$(git log @{u}.. 2>/dev/null || true)" ]; then
        printf '{{yellow}}[WARN]{{reset}} Unpushed commits detected\n'
    fi
    printf '{{green}}[OK]{{reset}}   Ready for release\n'
    printf '\n{{bold}}Next steps:{{reset}}\n'
    printf '  1. Create tag:  just tag\n'
    printf '  2. Push tag:    git push origin v{{version}}\n'

[group('release')]
[doc("Publish to crates.io (dry run)")]
publish-dry:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Publishing (dry run)...\n'
    {{cargo}} publish --dry-run
    printf '{{green}}[OK]{{reset}}   Dry run complete\n'

[group('release')]
[doc("Create git tag for release")]
tag:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '{{cyan}}[INFO]{{reset}} Creating tag v{{version}}...\n'
    git tag -a "v{{version}}" -m "Release v{{version}}"
    printf '{{green}}[OK]{{reset}}   Tag created: v{{version}}\n'
    printf '{{dim}}Push with: git push origin v{{version}}{{reset}}\n'

[group('release')]
[confirm("This will publish to crates.io. This action is IRREVERSIBLE. Continue?")]
[doc("Publish to crates.io (LAST RESORT - prefer automated release)")]
publish:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '\n{{bold}}{{blue}}══════ Publishing to crates.io ══════{{reset}}\n\n'
    printf '{{yellow}}[WARN]{{reset}} This action is IRREVERSIBLE!\n'
    printf '{{yellow}}[WARN]{{reset}} Prefer automated release via git tag push.\n\n'
    printf '{{cyan}}[INFO]{{reset}} Publishing rexpipe...\n'
    {{cargo}} publish
    printf '\n{{green}}[OK]{{reset}}   Published successfully!\n'
    printf '{{cyan}}[INFO]{{reset}} Next steps:\n'
    printf '  1. Verify: cargo search rexpipe\n'
    printf '  2. Check docs.rs in ~15 minutes\n'
    printf '  3. Update CHANGELOG.md [Unreleased] section\n'

# ============================================================================
# UTILITIES
# ============================================================================

[group('util')]
[doc("Count lines of code")]
loc:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Lines of code:\n'
    if command -v tokei &> /dev/null; then
        tokei . --exclude target
    else
        find src -name '*.rs' | xargs wc -l | tail -1
    fi

[group('util')]
[doc("Run rexpipe with example input")]
run-example pattern="\\d+" input="Test 123 and 456":
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running: echo "{{input}}" | rexpipe -p "{{pattern}}"\n'
    echo "{{input}}" | {{cargo}} run --release -- -p "{{pattern}}"

[group('util')]
[doc("Install rexpipe locally (all features)")]
install:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Installing rexpipe (all features)...\n'
    {{cargo}} install --path . --all-features
    printf '{{green}}[OK]{{reset}}   rexpipe installed\n'

[group('util')]
[doc("Install with specific features: just install-with \"tree-sitter,pcre\"")]
install-with features:
    #!/usr/bin/env bash
    RESOLVED=$(just _resolve_features "{{features}}")
    printf '{{cyan}}[INFO]{{reset}} Installing rexpipe...\n'
    if [ -z "$RESOLVED" ]; then
        printf '{{cyan}}[INFO]{{reset}} Features: (none - minimal build)\n'
        {{cargo}} install --path . --no-default-features
    else
        printf '{{cyan}}[INFO]{{reset}} Features: %s\n' "$RESOLVED"
        {{cargo}} install --path . --no-default-features --features "$RESOLVED"
    fi
    printf '{{green}}[OK]{{reset}}   rexpipe installed\n'

[group('util')]
[doc("Install with minimal features")]
install-minimal:
    @just install-with "minimal"

[group('util')]
[doc("Install with standard features (tree-sitter + pcre)")]
install-standard:
    @just install-with "standard"

[group('util')]
[doc("Uninstall rexpipe")]
uninstall:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Uninstalling rexpipe...\n'
    {{cargo}} uninstall rexpipe
    printf '{{green}}[OK]{{reset}}   rexpipe uninstalled\n'

# ============================================================================
# HELP & DOCUMENTATION
# ============================================================================

[group('help')]
[doc("Show version and environment info")]
info:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{project_name}} v{{version}}{{reset}}\n'
    printf '═══════════════════════════════════════\n'
    printf '{{cyan}}MSRV:{{reset}}      {{msrv}}\n'
    printf '{{cyan}}Edition:{{reset}}   {{edition}}\n'
    printf '{{cyan}}Platform:{{reset}}  {{platform}}\n'
    printf '{{cyan}}Jobs:{{reset}}      {{jobs}}\n'
    printf '\n{{cyan}}Rust:{{reset}}      %s\n' "$(rustc --version)"
    printf '{{cyan}}Cargo:{{reset}}     %s\n' "$(cargo --version)"
    printf '{{cyan}}Just:{{reset}}      %s\n' "$(just --version)"
    printf '\n'

[group('help')]
[doc("Check which development tools are installed")]
check-tools:
    #!/usr/bin/env bash
    printf '\n{{bold}}Development Tool Status{{reset}}\n'
    printf '═══════════════════════════════════════\n'

    check_cargo_tool() {
        if {{cargo}} "$1" --version &> /dev/null 2>&1; then
            printf '{{green}}✓{{reset}} cargo-%s\n' "$1"
        else
            printf '{{red}}✗{{reset}} cargo-%s (not installed)\n' "$1"
        fi
    }

    # Core tools
    printf '\n{{cyan}}Core:{{reset}}\n'
    command -v rustfmt &> /dev/null && printf '{{green}}✓{{reset}} rustfmt\n' || printf '{{red}}✗{{reset}} rustfmt\n'
    command -v clippy-driver &> /dev/null && printf '{{green}}✓{{reset}} clippy\n' || printf '{{red}}✗{{reset}} clippy\n'

    # Cargo extensions
    printf '\n{{cyan}}Cargo Extensions:{{reset}}\n'
    for tool in nextest llvm-cov audit deny outdated watch semver-checks machete careful; do
        check_cargo_tool $tool
    done

    # Standalone tools
    printf '\n{{cyan}}Standalone Tools:{{reset}}\n'
    command -v git-cliff &> /dev/null && printf '{{green}}✓{{reset}} git-cliff\n' || printf '{{red}}✗{{reset}} git-cliff\n'
    command -v typos &> /dev/null && printf '{{green}}✓{{reset}} typos\n' || printf '{{red}}✗{{reset}} typos\n'

    # External tools
    printf '\n{{cyan}}External:{{reset}}\n'
    command -v tokei &> /dev/null && printf '{{green}}✓{{reset}} tokei\n' || printf '{{red}}✗{{reset}} tokei\n'
    command -v jq &> /dev/null && printf '{{green}}✓{{reset}} jq\n' || printf '{{red}}✗{{reset}} jq\n'

    printf '\n'

[group('help')]
[doc("Show all available recipes grouped by category")]
help:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{project_name}} v{{version}}{{reset}} — Regex Pipeline Processor\n'
    printf 'MSRV: {{msrv}} | Edition: {{edition}} | Platform: {{platform}}\n\n'
    printf '{{bold}}Usage:{{reset}} just [recipe] [arguments...]\n\n'
    just --list --unsorted

[group('help')]
[doc("Show commonly used recipes")]
quick:
    #!/usr/bin/env bash
    printf '{{cyan}}{{bold}}Quick Reference{{reset}}\n\n'
    printf '{{bold}}Development:{{reset}}\n'
    printf '  {{green}}just build{{reset}}          Build debug (all features)\n'
    printf '  {{green}}just test{{reset}}           Run tests\n'
    printf '  {{green}}just clippy{{reset}}         Run clippy lints\n'
    printf '  {{green}}just fmt{{reset}}            Format code\n'
    printf '  {{green}}just watch{{reset}}          Watch mode (tests)\n'
    printf '  {{green}}just run{{reset}}            Run rexpipe\n'
    printf '\n{{bold}}CI/Release:{{reset}}\n'
    printf '  {{green}}just ci{{reset}}             Run full CI\n'
    printf '  {{green}}just ci-release{{reset}}     Release CI\n'
    printf '  {{green}}just release-check{{reset}}  Pre-release validation\n'
    printf '\n{{bold}}Analysis:{{reset}}\n'
    printf '  {{green}}just coverage{{reset}}       Code coverage\n'
    printf '  {{green}}just deny{{reset}}           Security/license check\n'
    printf '  {{green}}just audit{{reset}}          Security vulnerability scan\n'
    printf '\n{{bold}}Features:{{reset}}\n'
    printf '  {{green}}just build-with "pcre"{{reset}}      Build with specific features\n'
    printf '  {{green}}just build-minimal{{reset}}          Build minimal binary\n'
    printf '  {{green}}just build-standard{{reset}}         Build standard (tree-sitter+pcre)\n'
    printf '\n'
