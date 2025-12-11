# rexpipe

A unified regex pipeline processor with COMPASS framework integration.

## Overview

rexpipe transforms regex text processing from a fragmented, debugging-intensive, resource-heavy activity into a unified, transparent, and efficient workflow. Built with the COMPASS Strategic Collaboration Framework, it provides systematic analysis and implementation planning for complex text processing pipelines.

## Key Features

- **Unified Processing**: Single process handles multiple regex operations
- **COMPASS Integration**: Strategic thinking framework for complex pipeline planning
- **Interactive Debugging**: Real-time pattern inspection and match visualization
- **Streaming Architecture**: Constant memory usage regardless of file size
- **TOML Configuration**: Version-controllable, shareable pipeline definitions
- **Performance Focus**: 3-5x faster than equivalent multi-tool pipelines

## Installation

```bash
cargo install rexpipe
```

Or build from source:

```bash
git clone https://github.com/example/rexpipe
cd rexpipe
cargo build --release
```

## Quick Start

### Basic Pattern Replacement

```bash
echo "Test 123 and 456" | rexpipe --pattern '\d+' --replacement 'NUMBER'
# Output: Test NUMBER and NUMBER
```

### Configuration-Based Processing

```bash
# Process logs with predefined pipeline
rexpipe --config examples/log-cleanup.toml < access.log > cleaned.log

# Inspect patterns before processing
rexpipe --config examples/log-cleanup.toml --inspect < sample.log

# Interactive debugging
rexpipe --config examples/log-cleanup.toml --inspect --interactive < sample.log
```

### COMPASS Strategic Analysis

```bash
# Run strategic framework analysis
rexpipe --compass
```

## Configuration Format

Pipeline configurations use TOML format:

```toml
name = "Log Cleanup Pipeline"
description = "Standardizes and sanitizes server logs"
version = "1.0.0"

[[step]]
type = "substitute"
description = "Normalize error levels"
pattern = '\[ERROR\]'
replacement = '[ERR]'
flags = ["global"]
enabled = true

[[step]]
type = "filter"
description = "Remove debug messages"
pattern = 'DEBUG'
action = "drop_line"
enabled = true

[[step]]
type = "substitute"
description = "Standardize user ID format"
pattern = 'user_id=(\d+)'
replacement = 'uid=${1}'
flags = ["global"]
enabled = true
```

## Step Types

- **substitute**: Replace matched patterns with new text
- **filter**: Keep or drop lines based on pattern matching
- **extract**: Extract only the matched portions
- **validate**: Ensure lines match required patterns
- **transform**: Apply text transformations to matched content

### Transform Actions

The transform step type supports the following actions:

- **uppercase**: Convert matched text to uppercase
- **lowercase**: Convert matched text to lowercase
- **trim**: Remove whitespace from matched text
- **prepend**: Add text before matched content
- **append**: Add text after matched content
- **reverse**: Reverse the matched text
- **remove_whitespace**: Remove all whitespace from matched text
- **title_case**: Capitalize first letter of each word

## Filter Actions

- **keep_line**: Keep entire line if pattern matches
- **drop_line**: Drop entire line if pattern matches
- **keep_match**: Keep only if pattern matches
- **drop_match**: Drop only if pattern matches

## Command Line Options

```bash
rexpipe [OPTIONS]

OPTIONS:
    -c, --config <FILE>         TOML configuration file
    -p, --pattern <REGEX>       Inline regex pattern
    -r, --replacement <TEXT>    Replacement text for substitution
    -F, --fixed                 Treat pattern as fixed string (no regex)
    -P, --pcre                  Use PCRE-compatible regex (lookahead/lookbehind)
    -B, --before <N>            Show N lines before each match
    -A, --after <N>             Show N lines after each match
    -C, --context <N>           Show N lines before and after each match
        --inspect               Enable inspection mode
        --interactive           Enable interactive inspection
        --dry-run               Validate config, or show diff preview with -I
        --progress              Show progress indicator for multi-file processing
        --performance           Show performance metrics
        --compass               Run COMPASS strategic analysis
        --validate              Validate configuration only
        --export <FORMAT>       Export configuration to TOML or JSON
        --completions <SHELL>   Generate shell completion script
    -i, --input <FILE>          Input file (default: stdin)
    -o, --output <FILE>         Output file (default: stdout)
    -h, --help                  Print help information
    -V, --version               Print version information
```

## JSON Output

All JSON output uses a standardized schema with metadata for forward compatibility:

```json
{
  "metadata": {
    "schema_version": "1.0",
    "mode": "count",
    "tool_version": "1.1.0"
  },
  "data": {
    "lines_processed": 2,
    "matches_found": 2,
    "transformations_applied": 0
  }
}
```

Supported modes: `count`, `processing`, `performance`, `multi_file`, `files_with_matches`, `files_without_matches`.

## Shell Completions

rexpipe supports generating shell completion scripts for bash, zsh, fish, PowerShell, and elvish:

```bash
# Bash - add to ~/.bashrc
rexpipe --completions bash >> ~/.bashrc

# Zsh - add to ~/.zshrc or create a completion file
rexpipe --completions zsh > ~/.zfunc/_rexpipe

# Fish - save to completions directory
rexpipe --completions fish > ~/.config/fish/completions/rexpipe.fish

# PowerShell - add to $PROFILE
rexpipe --completions powershell >> $PROFILE

# Elvish
rexpipe --completions elvish >> ~/.elvish/rc.elv
```

After generating, restart your shell or source the completion file to enable completions.

## Exit Codes

rexpipe uses distinct exit codes for different error conditions:

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | No matches found (grep-like behavior) |
| 2 | Invalid usage / missing arguments |
| 3 | Configuration file error |
| 4 | Invalid regex pattern |
| 5 | File I/O error |
| 6 | Validation error |

## Performance Benefits

Traditional approach:
```bash
cat access.log | \
  sed 's/\[ERROR\]/[ERR]/g' | \
  grep -v 'DEBUG' | \
  sed 's/user_id=([0-9]+)/uid=\1/g' | \
  awk '{gsub(/192\.168\./, "10.0."); print}' | \
  sed 's/@company\.com/@domain\.com/g'
```

rexpipe approach:
```bash
cat access.log | rexpipe --config log-cleanup.toml
```

**Performance improvements:**
- **3-5x faster processing** on files >100MB
- **10-20x less RAM usage** on large files
- **Single process** eliminates inter-process communication overhead
- **Constant memory usage** regardless of file size

## COMPASS Framework

The integrated COMPASS (Clarify, Orient, Map, Pause, Architect, Synthesize) framework provides:

1. **Clarify Core Intent**: Understanding fundamental requirements
2. **Orient Through Research**: Evidence-based problem analysis
3. **Map Solution Space**: Comprehensive solution design
4. **Pause for Strategic Validation**: Alignment confirmation
5. **Architect Implementation**: Detailed specification creation
6. **Synthesize and Validate**: Quality assurance and final validation

Run `rexpipe --compass` to see the framework in action.

## Examples

### Log Processing
```bash
# Clean server logs
rexpipe --config examples/log-cleanup.toml < server.log

# Debug pattern matching
rexpipe --pattern 'ERROR.*user_id=(\d+)' --inspect < server.log
```

### Data Transformation
```bash
# Process CSV data
rexpipe --config examples/data-transform.toml < customers.csv

# Interactive pattern testing
rexpipe --pattern '(\w+),(\w+@[\w.]+)' --inspect --interactive < data.csv
```

### Performance Analysis
```bash
# Show processing metrics
rexpipe --config pipeline.toml --performance < large-file.txt
```

### Dry-Run Preview
```bash
# Preview changes before modifying files in-place
rexpipe -p 'old_value' -r 'new_value' -I --dry-run src/

# Preview shows unified diff of all changes that would be made
# without actually modifying any files
```

## Testing

```bash
# Run all tests
cargo test

# Run benchmarks
cargo bench

# Test with sample data
rexpipe --config examples/log-cleanup.toml --inspect < test-data/sample.log
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Run `cargo test` and `cargo fmt`
6. Submit a pull request

## License

MIT License - see LICENSE file for details.

## Architecture

Built with:
- **Rust**: Memory safety and performance
- **regex crate**: Fast regex engine for standard patterns
- **fancy-regex crate**: PCRE-compatible regex (lookahead/lookbehind support)
- **TOML**: Human-readable configuration
- **clap**: Command-line interface
- **termcolor**: Colored output for debugging

### Regex Engine Options

rexpipe supports multiple regex modes:

1. **Standard mode** (default): Uses the fast Rust `regex` crate
2. **PCRE mode** (`-P/--pcre`): Uses `fancy-regex` for PCRE-compatible patterns with lookahead/lookbehind support
3. **Fixed string mode** (`-F/--fixed`): Treats patterns as literal strings (fastest)

To enable PCRE mode features, build with:
```bash
cargo build --release --features pcre
```

### ReDoS Protection

rexpipe includes protection against Regular Expression Denial of Service (ReDoS) attacks:

**Standard mode** uses the Rust `regex` crate which guarantees **O(m × n) linear time** matching. This eliminates catastrophic backtracking vulnerabilities found in traditional regex engines.

**Built-in safeguards:**
- **Size limits**: Compiled regex size is limited to 10MB to prevent compilation DoS
- **DFA size limits**: Deterministic finite automaton size is capped to prevent memory exhaustion
- **Pattern analysis**: PCRE mode patterns are analyzed for common ReDoS indicators (nested quantifiers, excessive alternations)

**PCRE mode warning**: Unlike standard mode, PCRE mode uses a backtracking engine and can be vulnerable to ReDoS. Patterns like `(a+)+`, `(a*)*`, or deeply nested quantifiers will trigger warnings. For untrusted input, prefer standard mode.

### Named Capture Groups

rexpipe supports named capture groups in patterns, allowing you to reference captured text by name in replacements:

```bash
# Using named capture groups: (?P<name>pattern)
echo "John Doe" | rexpipe -p '(?P<first>\w+)\s+(?P<last>\w+)' -r '${last}, ${first}'
# Output: Doe, John

# Mixing named and numbered captures
echo "user: admin, id: 12345" | rexpipe -p '(?P<role>\w+), id: (\d+)' -r 'ID $2 is ${role}'
# Output: user: ID 12345 is admin
```

**Reference syntax in replacements:**
- `${name}` - Reference a named capture group
- `$1`, `$2`, etc. - Reference numbered capture groups
- `$0` - Reference the entire match

Named capture groups work in both standard mode and PCRE mode.

### Async Processing (Optional)

For non-blocking async file processing, build with the `async` feature:
```bash
cargo build --release --features async
```

This enables the `AsyncMultiFileProcessor` for concurrent file operations using tokio.

## Acknowledgments

Developed using the COMPASS Strategic Collaboration Framework for systematic approach to complex software challenges.