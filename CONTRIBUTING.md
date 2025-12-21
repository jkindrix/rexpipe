# Contributing to rexpipe

Thank you for your interest in contributing to rexpipe! This document provides guidelines and information for contributors.

## Getting Started

### Development Environment

1. **Install Rust** (stable toolchain):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Clone the repository**:
   ```bash
   git clone https://github.com/jkindrix/rexpipe.git
   cd rexpipe
   ```

3. **Build the project**:
   ```bash
   cargo build
   ```

4. **Run tests**:
   ```bash
   cargo test
   ```

### Recommended Tools

- **rustfmt**: Code formatting (`cargo fmt`)
- **clippy**: Linting (`cargo clippy`)
- **cargo-audit**: Security audit (install with `cargo install cargo-audit`)

## How to Contribute

### Finding Issues

- Look for issues labeled `good first issue` for beginner-friendly tasks
- Issues labeled `help wanted` are open for community contributions
- Check the project's TODO comments for improvement opportunities

### Before You Start

1. **Check existing issues** to avoid duplicate work
2. **Comment on the issue** you'd like to work on
3. **Ask questions** if requirements are unclear

### Making Changes

1. **Fork the repository** and create a feature branch:
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Make your changes** following the code style guidelines below

3. **Write tests** for new functionality

4. **Run the test suite** to ensure nothing is broken:
   ```bash
   cargo test
   cargo clippy -- -D warnings
   cargo fmt -- --check
   ```

5. **Commit your changes** with clear, descriptive messages

6. **Push your branch** and open a Pull Request

## Code Style Guidelines

### Formatting

- Use `cargo fmt` to format code before committing
- Follow the standard Rust style conventions

### Linting

- Code should pass `cargo clippy` without warnings
- Address clippy suggestions or document why they don't apply

### Commit Messages

Follow the [Conventional Commits](https://www.conventionalcommits.org/) format:

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `refactor`: Code refactoring
- `test`: Adding or updating tests
- `chore`: Maintenance tasks

**Examples:**
```
feat(processor): add context line support for matches
fix(library): handle circular include detection
docs(readme): add pattern library examples
```

### Documentation

- Add rustdoc comments for public APIs
- Include examples in documentation where helpful
- Update README.md for user-facing changes

### Testing

- Write tests for new functionality
- Tests should be in the `tests/` directory for integration tests
- Unit tests can be inline with `#[cfg(test)]` modules
- Ensure tests are deterministic and don't rely on external state

### Fuzz Testing

The project includes fuzz testing infrastructure using `cargo-fuzz`. Fuzz tests help find edge cases and potential panics in parsing and processing code.

**Setup:**
```bash
cargo install cargo-fuzz
```

**Available fuzz targets:**
- `fuzz_pattern`: Tests regex pattern compilation
- `fuzz_config`: Tests TOML configuration parsing
- `fuzz_pipeline`: Tests pipeline processing with structured input

**Running fuzz tests:**
```bash
cd fuzz
cargo +nightly fuzz run fuzz_pattern -- -max_len=1000
cargo +nightly fuzz run fuzz_config -- -max_len=10000
cargo +nightly fuzz run fuzz_pipeline
```

**Note:** Fuzz testing requires the nightly Rust toolchain.

## Pull Request Process

1. **Create a descriptive PR title** using conventional commit format
2. **Fill out the PR template** with:
   - Summary of changes
   - Related issue numbers
   - Testing performed
3. **Respond to review feedback** promptly
4. **Keep commits focused** - one logical change per commit
5. **Rebase if needed** to maintain a clean history

## Architecture Overview

```
src/
├── main.rs        # CLI entry point and argument parsing
├── lib.rs         # Library crate root with module exports
├── pipeline.rs    # Configuration structures
├── processor.rs   # Core streaming text processor
├── files.rs       # Multi-file processing
├── library.rs     # Pattern library resolution
├── inspector.rs   # Interactive debugging
├── json_schema.rs # JSON output schemas
└── error.rs       # Error type definitions

tests/
├── integration_tests.rs  # End-to-end tests
└── library_tests.rs      # Pattern library tests

fuzz/
└── fuzz_targets/  # Fuzz testing targets
    ├── fuzz_pattern.rs   # Pattern compilation fuzzer
    ├── fuzz_config.rs    # TOML config fuzzer
    └── fuzz_pipeline.rs  # Pipeline processing fuzzer

examples/
├── *.toml         # Example pipeline configurations
└── patterns/      # Built-in pattern libraries
```

## Feature Development

When adding new features:

1. **Consider backwards compatibility** - avoid breaking existing configs
2. **Add CLI flags** for new options following existing patterns
3. **Update documentation** including README and help text
4. **Add configuration support** if applicable (TOML format)
5. **Include tests** covering common and edge cases

## Reporting Issues

When reporting bugs, please include:

- Rust version (`rustc --version`)
- Operating system
- Steps to reproduce
- Expected vs. actual behavior
- Relevant configuration or input files

## Questions?

- Open a [GitHub Issue](https://github.com/jkindrix/rexpipe/issues) for questions
- Check existing issues and discussions first

## License

By contributing, you agree that your contributions will be licensed under the same terms as the project (MIT OR Apache-2.0).
