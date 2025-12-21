# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed
- Updated all dependencies to latest compatible versions
- Migrated to `anyhow` for application error handling with rich context
- Added `thiserror` for structured error types (foundation for future refinement)
- Fixed clippy warnings (Entry API usage, recursion parameter)

### Added
- Crate-level documentation with usage examples and doc-tests

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

[Unreleased]: https://github.com/jkindrix/rexpipe/compare/v1.1.0...HEAD
[1.1.0]: https://github.com/jkindrix/rexpipe/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/jkindrix/rexpipe/compare/v0.1.0...v1.0.0
[0.1.0]: https://github.com/jkindrix/rexpipe/releases/tag/v0.1.0
