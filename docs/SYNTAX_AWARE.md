# Syntax-Aware Processing Guide

rexpipe can use tree-sitter to parse source code and apply patterns only within specific syntax scopes. This enables precise refactoring without affecting strings, comments, or other contexts.

## Enabling Syntax-Aware Mode

Build rexpipe with the `tree-sitter` feature:

```bash
cargo install rexpipe --features tree-sitter
```

Or build from source:

```bash
cargo build --release --features tree-sitter
```

## Supported Languages

| Language | Grammar | File Extensions |
|----------|---------|-----------------|
| Rust | `tree-sitter-rust` 0.24 | `.rs` |
| Python | `tree-sitter-python` 0.23 | `.py` |
| JavaScript | `tree-sitter-javascript` 0.23 | `.js`, `.mjs` |
| TypeScript | `tree-sitter-typescript` 0.23 | `.ts`, `.tsx` |
| Go | `tree-sitter-go` 0.23 | `.go` |
| JSON | `tree-sitter-json` 0.24 | `.json` |
| YAML | `tree-sitter-yaml` 0.7 | `.yaml`, `.yml` |

## Scopes

### Basic Scopes

| Scope | Description | Example Match |
|-------|-------------|---------------|
| `all` | Match anywhere (default) | Everything |
| `code` | Match in code, not strings/comments | `fn foo()` but not `"fn foo()"` |
| `strings` | Match only in string literals | `"hello"` content |
| `comments` | Match only in comments | `// comment` content |

### Advanced Scopes

| Scope | Description | Languages |
|-------|-------------|-----------|
| `functions` | Function/method definitions | All |
| `function_calls` | Function call expressions | All |
| `imports` | Import/use statements | All |
| `types` | Type annotations | Rust, TS, Python (hints) |
| `identifiers` | Variable/function names | All |
| `macros` | Macro invocations | Rust |
| `control_flow` | if/for/while/match | All |
| `tests` | Test functions/blocks | All (see below) |

### Test Scope Detection

The `tests` scope recognizes language-specific test patterns:

| Language | What's Matched |
|----------|----------------|
| **Rust** | `#[test]`, `#[tokio::test]` attrs; `mod tests` blocks |
| **Python** | `def test_*`, `class Test*` |
| **JavaScript/TypeScript** | `describe()`, `it()`, `test()`, `beforeEach()` |
| **Go** | `func Test*`, `func Benchmark*`, `func Example*` |

## Usage Examples

### CLI Usage

```bash
# Rename function only in code (not strings/comments)
rexpipe -p 'oldFunc' -r 'newFunc' --scope code --language python src/*.py

# Find TODOs only in comments
rexpipe -p 'TODO:.*' --scope comments --language rust src/*.rs

# Extract API endpoints from function calls only
rexpipe -p '/api/v[12]/' --scope function_calls --language javascript src/*.js
```

### Pipeline Configuration

```toml
# syntax-refactor.toml
name = "safe-rename"
description = "Rename symbol only in code context"

[[step]]
type = "substitute"
pattern = "deprecated_function"
replacement = "new_function"
language = "python"
scope = "code"
description = "Rename in code only, preserve strings and comments"
```

### Multi-Language Support

```toml
[[step]]
type = "substitute"
pattern = "TODO"
replacement = "FIXME"
languages = ["rust", "python", "typescript"]
scope = "comments"
description = "Update TODOs across multiple languages"
```

### Excluding Scopes

```toml
[[step]]
type = "substitute"
pattern = "old_api"
replacement = "new_api"
language = "rust"
exclude_scopes = ["strings", "comments", "tests"]
description = "Update API in production code only"
```

## Practical Examples

### Safe Function Rename

Given this Rust code:

```rust
fn old_function() {
    // Call old_function here
    let s = "old_function";
    old_function();
}
```

Pipeline:

```toml
[[step]]
type = "substitute"
pattern = "old_function"
replacement = "new_function"
language = "rust"
scope = "code"
```

Result:

```rust
fn new_function() {           // <- renamed
    // Call old_function here  // <- unchanged (comment)
    let s = "old_function";    // <- unchanged (string)
    new_function();            // <- renamed
}
```

### Extract Deprecated API Usage (Excluding Tests)

```bash
rexpipe \
  -p 'deprecated_v1_api' \
  --scope function_calls \
  --language python \
  --exclude-scope tests \
  -R src/
```

### Update Logging Calls

```toml
[[step]]
type = "substitute"
pattern = 'console\.log\('
replacement = "logger.debug("
language = "javascript"
scope = "function_calls"
description = "Replace console.log with logger"
```

## File Processing

When processing files directly, rexpipe auto-detects language from extension:

```bash
# Auto-detects .py files as Python
rexpipe -p 'class\s+\w+' --scope code -R src/
```

Override with `--language`:

```bash
# Force TypeScript parsing for .mjs files
rexpipe -p 'interface' --scope types --language typescript src/*.mjs
```

## Limitations

1. **Performance:** Syntax-aware parsing is slower than regex-only matching (tree must be built)
2. **Large files:** Very large files may use more memory during parsing
3. **Embedded languages:** HTML with embedded JS/CSS is not fully supported
4. **Custom languages:** Only bundled grammars are available

## Adding Custom Languages

Currently, languages must be compiled into rexpipe. To request additional languages:

1. Open an issue at https://github.com/jkindrix/rexpipe/issues
2. Specify the tree-sitter grammar crate (e.g., `tree-sitter-ruby`)
3. Describe the use case

## Troubleshooting

### "Language not supported"

Ensure you built with `--features tree-sitter`:

```bash
rexpipe --version
# Should show: rexpipe 2.0.0 [tree-sitter]
```

### No matches with scope filter

1. Verify the language is correct for your file
2. Check that the pattern exists in the expected scope
3. Use `--explain` to see what would match without scope filtering

### Incorrect scope detection

Tree-sitter grammars may have edge cases. Report issues with:
1. The input code
2. Expected vs actual behavior
3. Language and scope used
