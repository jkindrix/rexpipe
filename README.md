# rexpipe

A unified regex pipeline processor with COMPASS framework integration.

## Overview

rexpipe transforms regex text processing from a fragmented, debugging-intensive, resource-heavy activity into a unified, transparent, and efficient workflow. Built with the COMPASS Strategic Collaboration Framework, it provides systematic analysis and implementation planning for complex text processing pipelines.

## Key Features

- **Unified Processing**: Single process handles multiple regex operations
- **Pattern Libraries**: Reusable regex patterns with `${pattern.name}` syntax
- **Multi-File Processing**: Recursive search, in-place editing, grep-like output modes
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

## Pattern Libraries

Pattern libraries allow you to define reusable regex patterns in separate files and reference them across multiple pipelines using `${pattern.name}` syntax.

### Creating a Pattern Library

```toml
# patterns/common.toml
name = "Common Patterns"
description = "Reusable patterns for log processing"
version = "1.0.0"

[patterns.logs]
error = '\[ERROR\]'
warning = '\[WARN(ING)?\]'
timestamp = '\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2}:\d{2}'

[patterns.data]
email = '[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}'
ip_address = '\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}'
```

### Using Pattern Libraries in Pipelines

```toml
name = "Log Processor"
patterns_include = ["patterns/common.toml"]

[[step]]
type = "substitute"
pattern = '${logs.error}'
replacement = '[ERR]'

[[step]]
type = "filter"
pattern = '${data.ip_address}'
action = "keep_line"
```

### Library Location Resolution

Pattern libraries are searched in order:
1. Relative to the pipeline configuration file
2. Global directory: `~/.rexpipe/patterns/`

### Nested Libraries

Libraries can include other libraries:

```toml
# patterns/extended.toml
name = "Extended Patterns"
patterns_include = ["common.toml"]  # Include another library

[patterns.custom]
special = 'my-pattern'
```

### CLI Commands for Libraries

```bash
# List all patterns in a library
rexpipe --list-patterns patterns/common.toml

# Validate a library file
rexpipe --validate-library patterns/common.toml
```

### Included Pattern Libraries

rexpipe ships with two pattern libraries in `examples/patterns/`:

**common.toml** (43 patterns):
- `email`, `url`, `uuid`, `phone_us`, `phone_intl`
- `net.ipv4`, `net.ipv6`, `net.mac`, `net.cidr`
- `time.iso8601`, `time.date_iso`, `time.timestamp_unix`
- `data.json_key`, `data.semver`, `data.key_value`
- `code.identifier`, `code.function_call`, `code.comment_hash`
- `security.api_key_generic`, `security.password_field`, `security.credit_card`, `security.ssn`

**logs.toml** (40 patterns, includes common.toml):
- `level.error`, `level.warning`, `level.info`, `level.debug`
- `apache.combined`, `apache.common`, `apache.status_5xx`
- `syslog.bsd`, `syslog.rfc5424`
- `json.json_line`, `json.message_field`
- `app.java_exception`, `app.python_traceback`, `app.request_id`
- `docker.container_prefix`, `docker.k8s_prefix`
- `nginx.error_log`, `nginx.access_log`

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
rexpipe [OPTIONS] [paths]...

ARGUMENTS:
    [paths]...                    Files or directories to process

OPTIONS:
    # Core Processing
    -c, --config <FILE>           TOML configuration file
    -p, --pattern <REGEX>         Inline regex pattern
    -r, --replacement <TEXT>      Replacement text for substitution
    -F, --fixed                   Treat pattern as fixed string (no regex)
    -P, --pcre                    Use PCRE-compatible regex (lookahead/lookbehind)

    # File Operations
    -i, --input <FILE>            Input file (default: stdin)
    -o, --output <FILE>           Output file (default: stdout)
    -I, --in-place                Edit files in-place
    -b, --backup <SUFFIX>         Create backup with suffix when editing in-place

    # Multi-File Processing
    -R, --recursive               Recursively process directories
    -g, --glob <PATTERN>          Only process files matching glob pattern
    -e, --exclude <PATTERN>       Exclude files matching glob pattern
        --no-ignore               Don't respect .gitignore files
        --hidden                  Include hidden files
        --max-depth <NUM>         Maximum directory recursion depth
    -j, --parallel                Process files in parallel
        --progress                Show progress indicator

    # Output Modes
        --count                   Only show count of matches per file
    -l, --files-with-matches      Only list files containing matches
    -L, --files-without-matches   Only list files not containing matches
    -q, --quiet                   Quiet mode - only set exit code
        --json                    Output results as JSON

    # Context Lines
    -B, --before-context <NUM>    Show NUM lines before each match
    -A, --after-context <NUM>     Show NUM lines after each match
    -C, --context <NUM>           Show NUM lines before and after each match

    # Inspection & Debugging
        --inspect                 Enable inspection mode
        --interactive             Enable interactive inspection
        --dry-run                 Validate config, or preview changes with -I
        --performance             Show performance metrics

    # Pattern Libraries
        --list-patterns <FILE>    List patterns from a pattern library
        --validate-library <FILE> Validate a pattern library file

    # Utilities
        --compass                 Run COMPASS strategic analysis
        --validate                Validate configuration only
        --export <FORMAT>         Export configuration (toml or json)
        --completions <SHELL>     Generate shell completion script
    -h, --help                    Print help information
    -V, --version                 Print version information
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

## Multi-File Processing

rexpipe supports processing multiple files with grep-like functionality:

### Basic Multi-File Operations
```bash
# Process all .log files in current directory
rexpipe -p 'ERROR' *.log

# Recursively search directories
rexpipe -p 'TODO' -R src/

# Filter by glob pattern
rexpipe -p 'FIXME' -R -g '*.rs' .

# Exclude patterns
rexpipe -p 'debug' -R -e '*.test.js' -e 'node_modules/*' src/
```

### Grep-Like Output Modes
```bash
# List only files containing matches
rexpipe -p 'password' -l -R .

# List files NOT containing a pattern
rexpipe -p 'Copyright' -L -R src/

# Count matches per file
rexpipe -p 'TODO|FIXME' --count -R src/

# Quiet mode (just set exit code)
rexpipe -p 'ERROR' -q server.log && echo "Found errors"
```

### In-Place Editing
```bash
# Edit files in-place
rexpipe -p 'old_api' -r 'new_api' -I -R -g '*.py' src/

# Create backups before editing
rexpipe -p 'localhost' -r 'production.example.com' -I -b .bak config/

# Preview changes with dry-run
rexpipe -p 'v1' -r 'v2' -I --dry-run -R -g '*.json' .
```

### Parallel Processing
```bash
# Process files in parallel for large codebases
rexpipe -p 'deprecated' -R -j --progress src/
```

## Examples

### Log Processing
```bash
# Clean server logs
rexpipe -c examples/log-cleanup.toml < server.log

# Debug pattern matching
rexpipe -p 'ERROR.*user_id=(\d+)' --inspect < server.log

# Extract only errors (using pattern library)
rexpipe -c examples/log-errors.toml < application.log

# Sanitize logs (redact PII using pattern library)
rexpipe -c examples/log-sanitize.toml < application.log
```

### Data Transformation
```bash
# Process CSV data
rexpipe -c examples/data-transform.toml < customers.csv

# Interactive pattern testing
rexpipe -p '(\w+),(\w+@[\w.]+)' --inspect --interactive < data.csv
```

### Multi-File Search
```bash
# Find all TODO comments in Rust files
rexpipe -p 'TODO|FIXME|HACK' -R -g '*.rs' src/

# Search with context lines
rexpipe -p 'panic!' -C 3 -R -g '*.rs' src/

# JSON output for scripting
rexpipe -p 'unsafe' -R -g '*.rs' --json src/
```

### In-Place Refactoring
```bash
# Rename a function across codebase
rexpipe -p 'old_function_name' -r 'new_function_name' -I -R -g '*.py' src/

# Update API version in configs
rexpipe -p 'api/v1' -r 'api/v2' -I -R -g '*.yaml' config/
```

### Performance Analysis
```bash
# Show processing metrics
rexpipe -c pipeline.toml --performance < large-file.txt
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

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

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