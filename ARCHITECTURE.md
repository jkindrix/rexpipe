# rexpipe Architecture

This document describes the high-level architecture and design decisions of rexpipe, an AI-native regex pipeline processor for text transformation.

## Design Philosophy: AI-Native First

rexpipe v2.0 was redesigned with AI agents as the primary user persona. This informs several architectural decisions:

| Human Tool Design | AI-Native Tool Design |
|-------------------|----------------------|
| Terse syntax, muscle memory | Explicit, predictable semantics |
| Text output for reading | JSON output for parsing |
| Silent failures OK | Structured errors with suggestions |
| Trust the user | Safe-by-default operations |
| One-off commands | Composable, verifiable pipelines |

### AI-Native Features

1. **JSON by Default for Pipes**: When stdout is not a TTY, output is JSON. The `should_use_json()` function in `main.rs` implements this logic.

2. **Structured Errors**: `--error-format json` outputs errors with category, exit code, and suggestion fields for programmatic handling.

3. **Safe In-Place Editing**: In non-interactive mode, `--apply` is required for destructive operations. This prevents accidental file modifications by automated agents.

4. **Explain Mode**: `--explain` describes what a pipeline will do without executing it, allowing agents to validate configuration before running.

5. **Verify Mode**: `--verify` outputs a verification summary after processing, confirming what was done.

6. **Schema Versioning**: All JSON output includes `schema_version: "1.0"` for forward compatibility.

## Overview

rexpipe consolidates complex regex-based text processing workflows into a single, efficient tool. It transforms multi-tool pipelines (grep | sed | awk) into unified, configurable operations with better performance and maintainability.

```
┌─────────────────────────────────────────────────────────────────────┐
│                         CLI Layer (main.rs)                         │
│  - Argument parsing (clap)                                          │
│  - Exit code management (grep-compatible: 0=match, 1=no match)      │
│  - User interface coordination                                      │
└────────────────────┬────────────────────────────────────────────────┘
                     │
┌────────────────────▼────────────────────────────────────────────────┐
│                    Orchestration Layer                               │
├─────────────────┬──────────────────┬─────────────────────────────────┤
│   Inspector     │    Library       │     MultiFileProcessor          │
│   (debug mode)  │   (patterns)     │     (batch operations)          │
└─────────────────┴──────────────────┴─────────────────────────────────┘
                     │
┌────────────────────▼────────────────────────────────────────────────┐
│                    Core Processing Layer                             │
├─────────────────┬──────────────────┬─────────────────────────────────┤
│ StreamProcessor │  PluginRegistry  │      PipelineConfig             │
│ (line-by-line)  │  (transforms)    │   (configuration)               │
└─────────────────┴──────────────────┴─────────────────────────────────┘
                     │
┌────────────────────▼────────────────────────────────────────────────┐
│                    Foundation Layer                                  │
├──────────┬──────────────┬─────────────┬──────────────────────────────┤
│  Error   │   Pattern    │ JSON Schema │    Regex Engines             │
│  Types   │  Libraries   │  Validation │  (std/pcre/fixed)            │
└──────────┴──────────────┴─────────────┴──────────────────────────────┘
```

## Module Responsibilities

### `main.rs` - CLI Entry Point

**Responsibility**: Command-line interface, argument parsing, operation dispatch.

- Builds CLI using clap with 44+ options
- Manages exit codes (grep-compatible semantics)
- Coordinates between single-file, multi-file, and inspection modes
- Handles output formatting (text, JSON, JSONL)

### `processor.rs` - Core Stream Processor

**Responsibility**: Line-by-line text processing with constant memory usage.

Key components:
- `StreamProcessor`: Main processing engine
- `CompiledPattern`: Abstraction over regex engines (standard, PCRE, fixed-string)
- `MatchInfo`: Detailed match information for inspection mode
- `ProcessorStats`: Runtime performance metrics

Design decisions:
- **Streaming architecture**: Processes input line-by-line to maintain O(1) memory regardless of input size
- **Multiple regex engines**: Standard Rust regex (fast, ReDoS-safe), fancy-regex for PCRE features, literal matching for fixed strings
- **Context line buffering**: VecDeque-based buffer for before/after context lines (like grep -B/-A/-C)

### `pipeline.rs` - Configuration

**Responsibility**: Pipeline configuration structures, validation, serialization.

Key types:
- `PipelineConfig`: Root configuration containing steps and settings
- `PipelineSettings`: Processing options (PCRE mode, timeouts, line limits)
- `PipelineStep`: Individual processing step (substitute, filter, extract, validate, transform)

### `files.rs` - Multi-File Processing

**Responsibility**: Batch file operations, directory recursion, parallel processing.

Key components:
- `MultiFileProcessor`: Batch file processing with parallel support
- `FileProcessingOptions`: Configuration for file operations
- `ShutdownSignal`: Graceful termination coordination

Design decisions:
- **Parallel processing threshold**: Only parallelize when file count exceeds threshold (default: 4) to avoid overhead
- **VCS awareness**: Respects .gitignore by default using the `ignore` crate
- **Symlink security**: Does not follow symlink directories to prevent traversal attacks
- **Atomic writes**: In-place editing uses write-to-temp-then-rename for crash safety

### `library.rs` - Pattern Libraries

**Responsibility**: Reusable pattern definitions with hierarchical organization.

Features:
- TOML-based pattern definitions
- Nested categorization with dot-notation (`${category.pattern}`)
- Include mechanism for composing libraries
- Circular dependency detection

### `inspector.rs` - Debug Mode

**Responsibility**: Interactive pattern debugging and match visualization.

Features:
- Colored match highlighting
- Capture group display
- Performance profiling
- Interactive step-through mode

### `plugin.rs` - Transform Plugins

**Responsibility**: Extensible transformation functions.

Built-in plugins:
- Case transformations (snake_case, camelCase, PascalCase, kebab-case)
- String manipulation (reverse, repeat, slice, pad, truncate)
- Encoding (hex, base64, URL)
- Text analysis (length, word count, character frequency)

Shell integration:
- External command execution with stdin/stdout piping
- Timeout protection (configurable, default 30s)
- Security: Input passed via stdin, not interpolated into command

### `error.rs` - Error Handling

**Responsibility**: Structured error types with actionable suggestions.

Design:
- Typed errors using `thiserror` for each error category
- Every error includes a `.suggestion()` method for user guidance
- Context-aware hints (e.g., TOML validation links, regex syntax help)

## Data Flow

### Single File Processing

```
Input Stream
     │
     ▼
┌─────────────────┐
│ Read Line       │ ◄── Detect line ending (LF/CRLF)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Check Max       │ ◄── Skip/error/truncate if too long
│ Line Length     │
└────────┬────────┘
         │
         ▼
┌─────────────────────────────────┐
│ For Each Pipeline Step:         │
│  ├─ Substitute: regex replace  │
│  ├─ Filter: keep/drop lines    │
│  ├─ Extract: capture matches   │
│  ├─ Validate: pattern check    │
│  └─ Transform: apply action    │
└────────┬───────────────────────┘
         │
         ▼
┌─────────────────┐
│ Context Buffer  │ ◄── Manage before/after context lines
└────────┬────────┘
         │
         ▼
Output Stream
```

### Multi-File Processing

```
Input Paths
     │
     ▼
┌─────────────────┐
│ Discover Files  │ ◄── Directory walk, gitignore, glob patterns
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Filter Binary   │ ◄── Skip binary files (auto-detect)
└────────┬────────┘
         │
         ▼
┌─────────────────────────────────┐
│ Process Files                   │
│  ├─ Sequential (< threshold)   │
│  └─ Parallel (>= threshold)    │ ◄── Rayon parallel iterators
└────────┬───────────────────────┘
         │
         ▼
┌─────────────────┐
│ Aggregate       │
│ Results         │
└─────────────────┘
```

## Key Design Decisions

### 1. Streaming Architecture

**Why**: Constant memory usage regardless of input size.

The processor reads input line-by-line, never loading the entire file into memory. This allows processing of arbitrarily large files (multi-GB log files) without memory exhaustion.

### 2. Multiple Regex Engines

**Why**: Balance between performance, safety, and features.

- **Standard (default)**: Rust `regex` crate with linear-time guarantees - immune to ReDoS
- **PCRE**: `fancy-regex` for lookahead/lookbehind when needed - user opts in with `--pcre`
- **Fixed**: Literal string matching - fastest option when regex features aren't needed

### 3. TOML Configuration

**Why**: Human-readable, version-controllable pipeline definitions.

TOML provides clear structure for complex pipelines while being easy to read and edit. Pipelines can be shared, versioned, and documented alongside code.

### 4. Parallel Processing with Threshold

**Why**: Avoid overhead on small file sets.

Parallel processing has setup costs (thread pool, synchronization). For small file counts, sequential processing is faster. The configurable threshold (default: 4 files) ensures parallelization only when beneficial.

### 5. VCS-Aware File Discovery

**Why**: Professional workflow integration.

Respecting `.gitignore` by default prevents processing of generated files, dependencies, and build artifacts - matching the behavior of modern tools like ripgrep.

### 6. Atomic In-Place Editing

**Why**: Crash safety.

In-place edits write to a temporary file first, then atomically rename. This prevents file corruption if the process is interrupted mid-write.

### 7. Graceful Shutdown

**Why**: Data integrity under interruption.

When receiving SIGINT/SIGTERM, rexpipe completes the current file before exiting. This prevents partial writes and ensures clean state even when cancelled.

## Extension Points

### Adding a Transform Action

1. Add variant to `TransformAction` enum in `pipeline.rs`
2. Handle the new action in `apply_transform()` in `processor.rs`
3. Add serialization support (the enum uses serde rename_all)

### Adding a Plugin

1. Register in `PluginRegistry::register_builtins()` in `plugin.rs`
2. Plugin signature: `Fn(&str, &[String]) -> String`

### Supporting a New Output Format

1. Add format option to CLI in `main.rs`
2. Implement formatting in the output section
3. Consider adding to `json_schema.rs` if structured

## Performance Characteristics

| Operation | Memory | Time Complexity |
|-----------|--------|-----------------|
| Single file | O(1) | O(n) lines |
| Multi-file sequential | O(1) | O(n) files × O(m) lines |
| Multi-file parallel | O(k) threads | O(n/k) files × O(m) lines |
| Pattern compilation | O(p) | Once per pattern |
| Context lines | O(c) buffer | O(1) per line |

## Security Considerations

1. **ReDoS Protection**: Standard regex engine uses linear-time matching
2. **Symlink Safety**: Directory symlinks not followed during recursion
3. **Shell Transform Safety**: Input piped to stdin, not interpolated into commands
4. **Binary Detection**: Prevents corruption from processing binary files as text
5. **Timeout Support**: Per-line and per-command timeouts prevent hangs

## Settings Architecture

rexpipe uses a **layered settings composition** pattern with three distinct configuration types, each serving a specific architectural layer:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    User Interaction Layer                           │
├─────────────────────────────────────────────────────────────────────┤
│  InspectorOptions                                                   │
│  - Output visualization (colors, line numbers)                      │
│  - Display limits (max matches per line)                            │
│  - Interactive mode settings                                        │
│  - Performance metric display                                       │
└─────────────────────────────────────────────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────────────┐
│                    Batch Processing Layer                           │
├─────────────────────────────────────────────────────────────────────┤
│  FileProcessingOptions                                              │
│  - File discovery (gitignore, patterns, depth)                      │
│  - Parallelization (parallel, threshold)                            │
│  - In-place editing (backup, atomic writes)                         │
│  - Binary handling mode                                             │
│  - Output modes (quiet, progress, streaming)                        │
│  - Graceful shutdown coordination                                   │
└─────────────────────────────────────────────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────────────┐
│                    Pipeline Processing Layer                        │
├─────────────────────────────────────────────────────────────────────┤
│  PipelineSettings                                                   │
│  - Regex engine selection (pcre, fixed_strings)                     │
│  - Pattern safety (strict_mode, regex_size_limit, timeout_ms)       │
│  - Line handling (max_line_length, preserve_line_endings)           │
│  - Context lines (context_before, context_after)                    │
│  - Shell execution (allow_shell, shell_timeout_secs)                │
└─────────────────────────────────────────────────────────────────────┘
```

### Design Rationale

The three settings types are intentionally separated:

1. **PipelineSettings** (TOML-serializable)
   - Purpose: Configuration embedded in pipeline TOML files
   - Scope: Affects regex matching and text processing behavior
   - Lifecycle: Loaded once from config, immutable during processing
   - Serialization: `serde::Serialize + Deserialize` for TOML/JSON

2. **FileProcessingOptions** (runtime-only)
   - Purpose: CLI-driven batch operation configuration
   - Scope: Affects file discovery, I/O behavior, and parallelization
   - Lifecycle: Built from CLI args, may vary per invocation
   - Contains: Non-serializable types (ShutdownSignal, callbacks)

3. **InspectorOptions** (runtime-only)
   - Purpose: Debug/inspection output configuration
   - Scope: Affects only the inspection visualization layer
   - Lifecycle: Built from CLI args for inspect mode only
   - Independence: Does not affect processing behavior, only display

### Composition Pattern

Each layer can operate independently:

```
// Pipeline-only (single file, no inspection)
let processor = StreamProcessor::new(config)?;
processor.process_stream(reader, writer)?;

// Batch processing (multi-file, no inspection)
let processor = MultiFileProcessor::new(config, file_options);
processor.process_files(&files)?;

// Inspection (debug mode)
let inspector = Inspector::new(config)?.with_options(inspector_options);
inspector.inspect_stream(reader)?;
```

### Why Not a Unified Settings Struct?

A unified configuration struct was considered but rejected for these reasons:

1. **Coupling**: Batch processing shouldn't depend on inspection settings
2. **Serialization**: PipelineSettings must be TOML-serializable; FileProcessingOptions contains non-serializable types
3. **Lifecycle**: Pipeline settings are immutable; file options may be rebuilt for different operations
4. **Clear boundaries**: Each struct has a single responsibility

The current design follows the Interface Segregation Principle - clients only depend on the settings they actually use.

## Logging and Diagnostics

rexpipe uses the standard Rust logging ecosystem: `log` for macros and `env_logger` for output.

### Why log + env_logger (Not tracing)

The `tracing` crate was evaluated but not adopted for these reasons:

1. **Synchronous workload**: rexpipe is primarily synchronous. The optional async feature is
   for I/O-bound batch operations, not complex concurrent spans.

2. **No nested contexts**: Processing is strictly line-by-line with no nested execution
   contexts that would benefit from tracing's span model.

3. **CLI simplicity**: Users expect standard environment variable control (`RUST_LOG=debug`)
   which env_logger provides out of the box.

4. **Dependency weight**: `log` is a facade with minimal overhead; `tracing` adds more
   complexity for features we don't use.

### Logging Levels

- **error**: Unrecoverable failures
- **warn**: Recoverable issues (e.g., shell command failure with fallback)
- **info**: High-level operation progress (file discovery, processing start)
- **debug**: Detailed operation flow (per-file processing, pattern compilation)
- **trace**: Verbose internal state (per-line processing, match details)

### Usage

```bash
# Enable debug logging
RUST_LOG=debug rexpipe -c config.toml < input.txt

# Filter to specific module
RUST_LOG=rexpipe::processor=trace rexpipe -c config.toml < input.txt
```

## Testing Strategy

- **Unit tests**: Core processing logic in each module
- **Integration tests**: End-to-end pipeline testing
- **Property tests**: Invariant checking with proptest
- **Fuzz tests**: Edge case discovery with cargo-fuzz
- **Benchmarks**: Performance regression detection with criterion
