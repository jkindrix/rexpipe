# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.0.0] - 2024-12-21

### Changed

**AI-Native Text Processor Transformation**

This release transforms rexpipe from a general-purpose text processor into a focused,
AI-native tool optimized for use by AI agents and automated pipelines.

#### AI-Native Behavior Changes
- **JSON output as default for pipes**: When stdout is not a terminal (piped output),
  JSON is now the default format. Use `--text` to force plain text output.
- **Safer in-place editing**: In non-interactive mode (piped/scripted), in-place
  editing (`-i`) now requires explicit `--apply` flag. Without it, a dry-run preview
  is shown instead. This prevents accidental file modifications by AI agents.
- **Structured error output**: `--error-format json` provides machine-parseable
  errors with categories, exit codes, and suggestions.

#### New Features
- `--explain`: Describe what a pipeline will do without processing data
  - Lists each step, patterns, and transformations
  - JSON output with `--json` flag for AI consumption
- `--verify`: Output verification summary after processing
  - Confirms lines processed, matches found, transformations applied
  - JSON output with `--json` flag for programmatic verification
- `--apply`: Explicitly confirm in-place modifications
  - Required for `-i` in non-interactive mode
  - Prevents accidental file changes
- `--text`: Force plain text output when piping (override JSON default)

### Removed

**Focus on Core Primitives** - Removed ~2,500 lines of non-essential code:

- **Natural Language Interface** (`natural.rs`): AI agents don't need natural
  language interfaces - they can use the CLI directly
- **TUI Dashboard** (`tui.rs`): AI agents don't need interactive dashboards
- **Python Bindings** (`python.rs`): Premature optimization - can add later if needed
- **Apache Kafka Integration**: Broken functionality that distracted from core value
  - Removed Kafka source/sink from stream module
  - Removed `kafka` feature flag and rdkafka dependency
- **TUI Feature**: Removed ratatui and crossterm dependencies

### Technical Notes
- All 256 tests continue to pass
- Zero clippy warnings
- Schema version 1.0 included in all JSON responses for forward compatibility
- Existing core functionality (streaming, pipelines, patterns, inspection) unchanged

## [Unreleased]

### Added
- **Audit Trail & Provenance Tracking** (`audit.rs`): Compliance-first data pipeline support
  - Cryptographic verification with SHA-256 fingerprints of input/output data
  - Immutable provenance manifests in JSON format for transformation history
  - Optional digital signatures for audit records
  - Human-readable compliance reports for GDPR, HIPAA, PCI-DSS requirements
  - CLI: `--audit` flag enables audit trail, `--audit-dir` sets output directory
- **Bidirectional Pipelines** (`bidirectional.rs`): Reversible text transformations
  - Store transformation mappings for bidirectional recovery
  - Run pipelines in forward or reverse mode
  - Reversibility analysis for pipeline steps
  - CLI: `--reverse` runs in reverse mode, `--mapping-file` for mapping storage
- **Checkpoint/Incremental Processing** (`checkpoint.rs`): Resume interrupted processing
  - Save and restore processing state across runs
  - Track file positions and content hashes
  - Git integration: `--git-diff REF` processes only changed lines since a commit
  - CLI: `--checkpoint FILE` enables incremental processing
- **Cross-File Relationships** (`crossfile.rs`): Semantic relationship processing
  - Define cross-file rules with trigger patterns and related file detection
  - File grouping with glob patterns
  - Violation actions: warn, error, fix
  - Ensure consistency across related files (e.g., header and source files)
- **Pattern Learning** (`learn.rs`): Infer regex patterns from examples
  - Example-based learning from positive and negative examples
  - Pattern generalization with confidence scoring
  - Template matching for common patterns (email, URL, SSN, etc.)
  - CLI: `--learn` with `--positive` and `--negative` example flags
- **Pipeline Testing Framework** (`testing.rs`): First-class test support
  - Define test cases in pipeline configuration with `[[tests]]` sections
  - Test expected output, match counts, and transformation counts
  - Multiple output formats: text, TAP, JUnit XML
  - CLI: `--test` runs tests, `--test-format` selects output format
- **Block Step Type**: Cross-line state machine processing for multi-line patterns
  - Define block boundaries with trigger (`pattern`) and `until` patterns
  - Block actions: `keep_block`, `drop_block`, `mark_block`, `substitute_in_block`, `collect_block`
  - Enables extraction of stack traces, log entries, code blocks, or delimited records
- **Git Filter Integration**: `--git-filter-setup <name>` generates git clean/smudge configuration
  - Automatic file transformation on commit/checkout
  - Integration with .gitattributes for pattern-based file matching
- **Pattern Discovery Mode**: `--discover` analyzes input to detect common patterns
  - Frequency analysis for 13 pattern types (email, IP, dates, URLs, etc.)
  - Generates suggested pipeline configuration for detected patterns
  - Bootstrap configuration from unknown log formats
- **Format-Preserving Encryption** (requires `--features fpe`):
  - `fpe_encrypt` transform using NIST FF1 algorithm (AES-128/192/256)
  - `fpe_decrypt` transform for reversible encryption
  - Preserves data format (encrypted digits remain digits)
  - Configurable radix (character set) for encryption
  - Support for external key files via `key_file` and `tweak_file` parameters
- **Deterministic Masking**: `mask_deterministic` transform for consistent masking
  - Same input+seed always produces same output
  - Preserve prefix/suffix characters (e.g., first 4 and last 4)
  - Support for external seed files via `seed_file` parameter
  - Useful for joining masked datasets or consistent test data
- **Syntax-Aware Processing** (requires `--features tree-sitter`):
  - Structure-aware pattern matching using tree-sitter parsing
  - Basic scopes: `code`, `strings`, `comments`, `functions`
  - Fine-grained scopes: `function_calls`, `imports`, `types`, `identifiers`, `macros`, `control_flow`
  - **Tests scope**: Language-aware test detection (`scope = "tests"` or `exclude_scopes = ["tests"]`)
    - Rust: `#[test]` attributes, `mod tests` blocks
    - Python: `test_` prefix functions, `Test` prefix classes
    - JavaScript/TypeScript: `describe()`, `it()`, `test()` blocks
    - Go: `Test`, `Benchmark`, `Example` prefix functions
  - 7 languages: Rust, Python, JavaScript, TypeScript, Go, JSON, YAML
  - Multi-language steps: `languages = ["rust", "python", "typescript"]`
  - Exclude scopes: `exclude_scopes = ["comments", "strings", "tests"]`
  - Refactor code without changing strings or comments
  - Example: `scope = "code"` with `language = "rust"` to only match in code
- **Streaming Pipeline Server**: `--server` mode for network-based processing
  - TCP server that accepts pipeline configurations and text to process
  - Line-based JSON protocol for easy integration
  - Support for default pipeline configuration
  - Async mode available with `--features async`
- **Continuous Streaming Mode**: `--stream` with URI-based sources and sinks
  - Input sources: `stdin://`, `file:///path`, `tcp://host:port`, `udp://host:port`
  - Output sinks: `stdout://`, `stderr://`, `file:///path`, `tcp://host:port`, `udp://host:port`
  - Example: `rexpipe --config pipeline.toml --input tcp://0.0.0.0:5140 --output file:///var/log/processed.log`
- **Apache Kafka Integration** (requires `--features kafka`):
  - Consume messages from Kafka topics as input source
  - Produce processed messages to Kafka topics as output sink
  - URI format: `kafka://broker:port/topic?group_id=consumer-group`
  - Built on rdkafka/librdkafka for production-grade reliability
  - Example: `rexpipe --stream --input kafka://localhost:9092/raw-logs --output kafka://localhost:9092/processed-logs`
- Crate-level documentation with usage examples and doc-tests
- Working doctests for `ResolvedLibrary::contains` and `ResolvedLibrary::pattern_names`

### Changed
- Updated all dependencies to latest compatible versions
- Migrated to `anyhow` for application error handling with rich context
- Added `thiserror` for structured error types (foundation for future refinement)
- Fixed all clippy warnings (Entry API usage, recursion parameter, iterator idioms)
- Improved `BinaryMode` to implement `FromStr` trait for idiomatic parsing
- Improved documentation with proper intra-doc links and HTML escaping

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

[2.0.0]: https://github.com/jkindrix/rexpipe/compare/v1.1.0...v2.0.0
[Unreleased]: https://github.com/jkindrix/rexpipe/compare/v2.0.0...HEAD
[1.1.0]: https://github.com/jkindrix/rexpipe/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/jkindrix/rexpipe/compare/v0.1.0...v1.0.0
[0.1.0]: https://github.com/jkindrix/rexpipe/releases/tag/v0.1.0
