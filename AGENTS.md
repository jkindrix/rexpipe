# AGENTS.md - rexpipe

Machine-readable instructions for AI coding agents working on the rexpipe project.

## Project Overview

rexpipe is a modern regex pipeline processor optimized for scripting and automated text processing. Version 2.0 refocused the tool on core primitives with machine-readable defaults.

**Key design principle:** When stdout is not a TTY (piped/scripted), output JSON by default for machine consumption.

## Build & Test

```bash
# Build (development)
cargo build

# Build (release)
cargo build --release

# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test module
cargo test processor::tests

# Check for warnings
cargo clippy

# Format code
cargo fmt
```

## Code Style

- Rust 2024 edition with MSRV 1.85
- Use `cargo fmt` before committing
- Zero clippy warnings required
- Error handling: use `anyhow::Result` for functions, `thiserror` for error types
- Documentation: doc comments on all public items
- No AI branding in commits (OpenAI, Anthropic, Claude, ChatGPT, etc.)

## Module Structure

```
src/
├── main.rs          # CLI entry point and argument parsing
├── lib.rs           # Library root with public exports
├── processor.rs     # Core streaming text processor
├── pipeline.rs      # Pipeline configuration and step types
├── files.rs         # Multi-file processing with parallelism
├── inspector.rs     # Match inspection and debugging
├── library.rs       # Pattern library loading
├── error.rs         # Structured error types
├── json_schema.rs   # JSON output schemas (version 1.0)
├── stream.rs        # URI-based streaming sources/sinks
├── plugin.rs        # Plugin system for custom transforms
├── server.rs        # Pipeline server mode
├── audit.rs         # Cryptographic audit trails
├── bidirectional.rs # Reversible transformations
├── checkpoint.rs    # Incremental processing state
├── crossfile.rs     # Cross-file relationship rules
├── learn.rs         # Pattern inference from examples
├── testing.rs       # Pipeline test framework
└── data.rs          # Structured data format handling
```

## Key Patterns

### Adding a CLI Flag

1. Add argument in `build_cli()` function in `main.rs`
2. Handle flag in `run_application()` or appropriate handler
3. Update help text with clear description
4. Add long_help for complex flags

### Adding JSON Output

1. Define struct in `json_schema.rs` with `#[derive(Serialize)]`
2. Create output function using `JsonResponse::new(mode, data)`
3. Use `SCHEMA_VERSION` constant for forward compatibility
4. Output to stderr for metadata, stdout for data

### Pipeline Step Types

- `Substitute`: Replace pattern matches
- `Filter`: Keep/drop lines or matches
- `Extract`: Extract matched portions
- `Validate`: Ensure lines match pattern
- `Transform`: Apply text transformations
- `Block`: Process within block boundaries

## Testing

- Unit tests: in each module file
- Integration tests: `tests/` directory
- Doc tests: in documentation comments
- Property tests: using `proptest` crate

Test naming convention: `test_<function>_<scenario>`

## PR Instructions

1. Ensure `cargo test` passes
2. Ensure `cargo clippy` has zero warnings
3. Run `cargo fmt`
4. Write conventional commit messages (feat:, fix:, docs:, etc.)
5. Update CHANGELOG.md for user-facing changes

## Automation Features

These flags are designed for scripting and automation:

| Flag | Purpose |
|------|---------|
| `--json` | Force JSON output (auto for pipes) |
| `--text` | Force text output (override JSON default) |
| `--error-format json` | Structured error output |
| `--explain` | Describe pipeline without executing |
| `--verify` | Confirm what transformations were applied |
| `--apply` | Required for in-place edits when scripted |
| `--dry-run` | Preview changes without applying |

## Security Considerations

- `--no-shell` disables shell transforms (prevents command injection)
- `--strict` enables ReDoS pattern rejection
- Atomic file writes prevent corruption
- Audit trails available with `--audit`

## Common Tasks

### Process stdin and output JSON
```bash
echo "test 123" | rexpipe -p '\d+' -r 'NUM'
# Outputs JSON when piped
```

### Force text output
```bash
echo "test 123" | rexpipe -p '\d+' -r 'NUM' --text
```

### Explain before running
```bash
rexpipe -c pipeline.toml --explain
```

### Safe in-place edit
```bash
rexpipe -p 'foo' -r 'bar' -i --apply *.txt
```

### Verify processing
```bash
echo "data" | rexpipe -p '\d+' -r 'X' --verify
```
