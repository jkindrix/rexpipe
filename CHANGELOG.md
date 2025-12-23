# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.0.0] - 2024-12-21

### Changed

**Automation-First Redesign**

This release redesigns rexpipe as an automation-first text processor optimized for
scripting, pipelines, and programmatic use.

#### Behavior Changes
- **JSON output as default for pipes**: When stdout is not a terminal (piped output),
  JSON is now the default format. Use `--text` to force plain text output.
- **Safer in-place editing**: In non-interactive mode (piped/scripted), in-place
  editing (`-i`) now requires explicit `--apply` flag. Without it, a dry-run preview
  is shown instead. This prevents accidental file modifications by scripts.
- **Structured error output**: `--error-format json` provides machine-parseable
  errors with categories, exit codes, and suggestions.
- **Security**: Shell transforms now disabled by default
  - Requires `--allow-shell` flag to enable shell command execution
  - Prevents accidental command injection from untrusted configs
- Version now dynamically read from Cargo.toml

#### New Features
- `--explain`: Describe what a pipeline will do without processing data
- `--verify`: Output verification summary after processing
- `--apply`: Explicitly confirm in-place modifications
- `--text`: Force plain text output when piping
- `--validate-config`: Validate pipeline configuration without processing
- `--man`: Generate man page to stdout

### Added

- **CLI Integration Tests**: Comprehensive test suite for binary behavior
  - 37 tests covering substitution, filtering, JSON output, exit codes, file processing
  - Tests for shell completions, config validation, dry-run, context lines, man page
  - Platform path tests (spaces, special characters, nested directories)
  - Shell security warning tests
- **Fuzz Testing in CI**: Automated fuzz testing for config, pattern, and pipeline parsing
- **Feature Test Matrix**: CI tests for pcre, async, tree-sitter, fpe features
- **Shell Command Validation**: Security analysis for shell transforms with warnings
- **Rate Limiting**: Pattern learning now has configurable limits (max_examples, timeout)
- **FPE Security Documentation**: Best practices for key management
- **Benchmarking Documentation**: Guide for profiling and performance testing

### Advanced Features

- **Audit Trail & Provenance Tracking** (`audit.rs`): Compliance-first data pipeline support
  - Cryptographic verification with SHA-256 fingerprints of input/output data
  - Immutable provenance manifests in JSON format for transformation history
  - CLI: `--audit` flag enables audit trail, `--audit-dir` sets output directory
- **Bidirectional Pipelines** (`bidirectional.rs`): Reversible text transformations
  - Store transformation mappings for bidirectional recovery
  - Run pipelines in forward or reverse mode
  - CLI: `--reverse` runs in reverse mode, `--mapping-file` for mapping storage
- **Checkpoint/Incremental Processing** (`checkpoint.rs`): Resume interrupted processing
  - Save and restore processing state across runs
  - Git integration: `--git-diff REF` processes only changed lines since a commit
  - CLI: `--checkpoint FILE` enables incremental processing
- **Cross-File Relationships** (`crossfile.rs`): Semantic relationship processing
  - Define cross-file rules with trigger patterns and related file detection
  - Violation actions: warn, error, fix
- **Pattern Learning** (`learn.rs`): Infer regex patterns from examples
  - Example-based learning from positive and negative examples
  - Template matching for common patterns (email, URL, SSN, etc.)
  - CLI: `--learn` with `--positive` and `--negative` example flags
- **Pipeline Testing Framework** (`testing.rs`): First-class test support
  - Define test cases in pipeline configuration with `[[tests]]` sections
  - Multiple output formats: text, TAP, JUnit XML
  - CLI: `--test` runs tests, `--test-format` selects output format
- **Block Step Type**: Cross-line state machine processing for multi-line patterns
  - Define block boundaries with `pattern` and `until` patterns
  - Block actions: `keep_block`, `drop_block`, `collect_block`
- **Format-Preserving Encryption** (requires `--features fpe`):
  - `fpe_encrypt`/`fpe_decrypt` transforms using NIST FF1 algorithm
  - Preserves data format (encrypted digits remain digits)
- **Syntax-Aware Processing** (requires `--features tree-sitter`):
  - Structure-aware pattern matching using tree-sitter parsing
  - Scopes: `code`, `strings`, `comments`, `functions`, `tests`
  - 7 languages: Rust, Python, JavaScript, TypeScript, Go, JSON, YAML

### Removed

**Focus on Core Primitives** - Removed ~2,500 lines of non-essential code:

- **Natural Language Interface** (`natural.rs`): Unnecessary complexity
- **TUI Dashboard** (`tui.rs`): Out of scope for a CLI pipeline tool
- **Python Bindings** (`python.rs`): Premature optimization
- **Apache Kafka Integration**: Broken functionality
- **TUI Feature**: Removed ratatui and crossterm dependencies

### Technical Notes
- 421 tests passing (250 unit + 66 integration + 27 property + 37 CLI + 41 doc tests)
- Zero clippy warnings
- Schema version 1.0 included in all JSON responses for forward compatibility
- Release profile optimized (LTO, strip, single codegen unit)

## [1.1.0] - 2024-12-15

### Added
- **Pattern Library Support**: Reusable regex pattern definitions in TOML format
  - Nested pattern categories with dot notation access (`${category.name}`)
  - Library nesting via `patterns_include`
  - Built-in libraries: `common.toml` (43+ patterns), `logs.toml` (40+ patterns)
  - `--list-patterns` and `--validate-library` commands
- **Shell Completions**: Generated completions for Bash, Zsh, Fish, PowerShell, and Elvish
- **Progress Indicator**: Visual progress bar for multi-file processing (`--progress`)
- **Dry-Run Preview**: Unified diff output before applying changes (`--dry-run`)
- **JSON Output Schema**: Versioned JSON output for scripting (`--json`)
- **ReDoS Protection**: Automatic detection and warnings for potentially dangerous patterns

### Changed
- Enhanced documentation with complete CLI reference and examples

## [1.0.0] - 2024-12-14

### Added
- **Multi-file Processing**: Recursive directory traversal with parallel processing
  - VCS-aware file discovery (respects `.gitignore`)
  - In-place editing with optional backup
  - Glob include/exclude patterns
  - Files with/without matches modes
- **PCRE Support**: Optional `fancy-regex` backend for lookahead/lookbehind patterns
  - Enable with `--features pcre` or `-P` flag
- **Enhanced CLI**: Comprehensive command-line interface
  - Context lines (`-B`, `-A`, `-C`)
  - Quiet mode (`-q`)
  - Count-only mode (`-c`)
  - Fixed string matching (`-F`)

### Changed
- Improved error messages with helpful suggestions
- Better exit code categorization

## [0.1.0] - 2024-12-13

### Added
- **Core Pipeline Processing**: Unified regex pipeline processor
  - Streaming architecture with constant memory usage
  - Multiple step types: Substitute, Filter, Extract, Validate, Transform
  - Named and numbered capture group support
  - TOML-based pipeline configuration
- **Step Types**:
  - `substitute`: Replace pattern matches with replacement text
  - `filter`: Keep or drop lines/matches based on patterns
  - `extract`: Extract only matched portions
  - `validate`: Ensure lines match required patterns
  - `transform`: Apply text transformations (uppercase, lowercase, trim, etc.)
- **Inspection Mode**: Interactive debugging with match visualization
- **Performance Metrics**: Processing statistics and throughput reporting

[Unreleased]: https://github.com/jkindrix/rexpipe/compare/v2.0.0...HEAD
[2.0.0]: https://github.com/jkindrix/rexpipe/compare/v1.1.0...v2.0.0
[1.1.0]: https://github.com/jkindrix/rexpipe/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/jkindrix/rexpipe/compare/v0.1.0...v1.0.0
[0.1.0]: https://github.com/jkindrix/rexpipe/releases/tag/v0.1.0
