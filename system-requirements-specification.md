# System Requirements Specification (SRS)

**rexpipe - Regex Pipeline Processor**

**Version:** 1.0  
**Date:** August 2025  
**Document Classification:** Technical Specification

---

## 1. Introduction

### 1.1 Purpose
This document specifies the complete functional and non-functional requirements for rexpipe, a command-line regex pipeline processing tool. This SRS serves as the definitive specification for development, testing, and validation.

### 1.2 Scope
rexpipe is a single-binary command-line utility that processes text streams through configurable regular expression operations. The system shall:

- Process text streams from stdin to stdout with constant memory usage
- Execute sequential regex operations defined via CLI arguments or configuration files
- Provide detailed pattern matching inspection and debugging capabilities
- Integrate seamlessly with Unix pipeline workflows
- Support configuration-driven workflow management

**Out of Scope:**
- Graphical user interface
- Network-based processing
- Database integration
- Binary file processing

### 1.3 Definitions and Abbreviations

| Term | Definition |
|------|------------|
| CLI | Command-Line Interface |
| PCRE | Perl Compatible Regular Expressions |
| TOML | Tom's Obvious, Minimal Language configuration format |
| Pipeline | Sequence of regex operations applied to text stream |
| Step | Single regex operation within a pipeline |
| Stream | Continuous flow of text data through stdin/stdout |

### 1.4 References
- PCRE Specification (pcre.org)
- TOML v1.0.0 Specification
- POSIX.1-2017 (IEEE Std 1003.1-2017)
- Unix Philosophy (Doug McIlroy, 1978)

---

## 2. Overall Description

### 2.1 Product Perspective
rexpipe operates as a standalone command-line utility within the Unix ecosystem, designed to replace multi-tool regex processing chains with a single, high-performance process.

**System Context:**
```
Input Source → [rexpipe] → Output Destination
     ↓              ↓              ↓
   stdin         Processing     stdout
  Files          Pipeline       Files
 Network         Config         Network
  Pipes          Inspect        Pipes
```

### 2.2 Product Functions

**Primary Functions:**
- **F1:** Stream-based text processing with regex operations
- **F2:** Multi-step pipeline execution from configuration files
- **F3:** Interactive pattern matching inspection and debugging
- **F4:** Command-line argument-based operation chaining
- **F5:** Configuration file validation and management

### 2.3 User Characteristics

**Primary Users:**
- **DevOps Engineers:** Processing log files, configuration management
- **Data Engineers:** ETL pipeline text transformation
- **System Administrators:** Log analysis and system monitoring
- **Software Developers:** Text processing automation

**User Expertise:**
- **Minimum:** Basic command-line experience, understanding of regular expressions
- **Expected:** Intermediate Unix pipeline usage, regex debugging experience
- **Advanced:** Complex pattern writing, performance optimization awareness

### 2.4 Constraints

**Technical Constraints:**
- Must compile to single static binary
- Memory usage must remain constant regardless of input size
- Must maintain compatibility with POSIX shell environments
- Configuration files limited to TOML format

**Performance Constraints:**
- Process minimum 1GB/minute on standard hardware (4-core CPU, 8GB RAM)
- Memory usage not to exceed 100MB regardless of input size
- Startup time under 50ms for simple operations

---

## 3. Functional Requirements

### 3.1 Command-Line Interface (REQ-CLI)

#### 3.1.1 Basic Substitution (REQ-CLI-001)
**Description:** Execute single regex substitution operation  
**Input:** Text stream via stdin, pattern and replacement via arguments  
**Processing:** Apply PCRE pattern matching and substitution  
**Output:** Transformed text to stdout  

**Syntax:**
```bash
rexpipe --sub 'pattern' 'replacement' [flags]
rexpipe -s 'pattern' 'replacement' [flags]
```

**Flags:**
- `--global, -g`: Apply to all matches (default: first match only)
- `--case-insensitive, -i`: Ignore case in pattern matching
- `--multiline, -m`: Enable multiline mode

**Example:**
```bash
echo "Hello World" | rexpipe --sub 'World' 'Universe'
# Output: Hello Universe
```

#### 3.1.2 Multiple Operations Chaining (REQ-CLI-002)
**Description:** Execute multiple regex operations in sequence  
**Input:** Multiple `--sub` arguments processed left-to-right  
**Processing:** Apply operations sequentially to text stream  
**Output:** Final transformed text  

**Syntax:**
```bash
rexpipe --sub 'pattern1' 'replacement1' --sub 'pattern2' 'replacement2'
```

**Requirements:**
- Operations must execute in argument order
- Each operation processes output of previous operation
- Failure in any operation terminates processing with error

#### 3.1.3 Filter Operations (REQ-CLI-003)
**Description:** Filter lines based on regex patterns  
**Syntax:**
```bash
rexpipe --filter 'pattern' [--action keep|drop]
rexpipe --keep 'pattern'      # Shorthand for --filter 'pattern' --action keep
rexpipe --drop 'pattern'      # Shorthand for --filter 'pattern' --action drop
```

**Behavior:**
- `keep`: Output only lines matching pattern
- `drop`: Output only lines not matching pattern
- Default action: `keep`

#### 3.1.4 Extract Mode (REQ-CLI-004)
**Description:** Extract only matching portions of text  
**Syntax:**
```bash
rexpipe --extract 'pattern' [--group N]
```

**Behavior:**
- Output only text matching the pattern
- `--group N`: Output only capture group N (default: entire match)
- Non-matching lines produce no output

#### 3.1.5 Pattern Inspection Mode (REQ-CLI-005)
**Description:** Analyze pattern matches without transformation  
**Syntax:**
```bash
rexpipe --pattern 'pattern' --inspect [--format json|table|detailed]
```

**Output Format (detailed - default):**
```
Match 1 (line 15, chars 45-56):
  Full match: "user_id=1234"
  Group 1: "1234"
  
Match 2 (line 23, chars 12-22):
  Full match: "user_id=5678"
  Group 1: "5678"
  
Summary:
  Total matches: 2
  Lines processed: 150
  Processing time: 0.023s
```

### 3.2 Configuration File Processing (REQ-CONFIG)

#### 3.2.1 TOML Configuration Loading (REQ-CONFIG-001)
**Description:** Load and execute multi-step pipelines from TOML files  
**Syntax:**
```bash
rexpipe --config pipeline.toml
rexpipe -c pipeline.toml
```

**Configuration File Structure:**
```toml
[metadata]
name = "Pipeline Name"
description = "Pipeline description"
version = "1.0.0"
author = "Author Name"

[[step]]
type = "substitute"
description = "Step description"
pattern = "regex_pattern"
replacement = "replacement_string"
flags = ["global", "case_insensitive", "multiline"]

[[step]]
type = "filter"
pattern = "regex_pattern"  
action = "keep" # or "drop"

[[step]]
type = "extract"
pattern = "regex_pattern"
group = 1 # optional, defaults to 0 (full match)
```

#### 3.2.2 Configuration Validation (REQ-CONFIG-002)
**Description:** Validate configuration file syntax and semantics  
**Syntax:**
```bash
rexpipe --config pipeline.toml --validate
```

**Validation Requirements:**
- TOML syntax validation
- Required field presence validation
- Regex pattern compilation validation
- Flag compatibility validation
- Circular dependency detection

**Error Output:**
```
Configuration validation failed:
  Line 15: Invalid regex pattern in step 'normalize-emails'
  Line 23: Unknown flag 'invalid_flag' in step 'filter-debug'
  Line 30: Missing required field 'pattern' in step 'extract-dates'
```

#### 3.2.3 Dry Run Mode (REQ-CONFIG-003)
**Description:** Execute pipeline without producing output  
**Syntax:**
```bash
rexpipe --config pipeline.toml --dry-run
```

**Behavior:**
- Process input through all pipeline steps
- Count matches and transformations
- Report would-be changes without outputting transformed text
- Validate all patterns work with real data

### 3.3 Advanced Features (REQ-ADVANCED)

#### 3.3.1 Field-Based Processing (REQ-ADVANCED-001)
**Description:** Apply operations to specific fields in delimited text  
**Syntax:**
```bash
rexpipe --field N --delimiter 'char' --sub 'pattern' 'replacement'
```

**Requirements:**
- Support comma, tab, pipe, semicolon, and custom delimiters
- Field numbering starts at 1
- Preserve field structure in output
- Handle quoted fields with embedded delimiters

#### 3.3.2 Named Capture Groups (REQ-ADVANCED-002)
**Description:** Support PCRE named capture group syntax  
**Pattern Example:** `(?P<year>\d{4})-(?P<month>\d{2})-(?P<day>\d{2})`  
**Replacement Example:** `${month}/${day}/${year}`

#### 3.3.3 Conditional Processing (REQ-ADVANCED-003)
**Description:** Apply operations based on pattern matching conditions  
**Configuration Example:**
```toml
[[step]]
type = "conditional"
condition = "ERROR"
if_match = [
  {type = "substitute", pattern = "ERROR", replacement = "[ERR]"},
  {type = "substitute", pattern = "(\d{4}-\d{2}-\d{2})", replacement = "DATE: ${1}"}
]
if_no_match = [
  {type = "filter", pattern = "DEBUG", action = "drop"}
]
```

---

## 4. Non-Functional Requirements

### 4.1 Performance Requirements (REQ-PERF)

#### 4.1.1 Processing Speed (REQ-PERF-001)
- **Requirement:** Process minimum 1GB/minute of text data
- **Test Conditions:** 4-core CPU (2.5GHz), 8GB RAM, SSD storage
- **Measurement:** Throughput measured with 10GB file containing mixed log entries
- **Acceptance:** Sustained 1GB/min for duration of processing

#### 4.1.2 Memory Usage (REQ-PERF-002)
- **Requirement:** Constant memory usage regardless of input file size
- **Maximum:** 100MB RAM consumption for any operation
- **Test Conditions:** Files from 1MB to 100GB
- **Measurement:** Peak memory usage via system monitoring
- **Acceptance:** Memory usage remains below 100MB threshold

#### 4.1.3 Startup Time (REQ-PERF-003)
- **Requirement:** Application startup under 50ms for simple operations
- **Test Conditions:** Simple substitution operation
- **Measurement:** Time from process start to first output byte
- **Acceptance:** 95th percentile under 50ms

#### 4.1.4 Configuration Loading (REQ-PERF-004)
- **Requirement:** Configuration files up to 100 steps load under 100ms
- **Measurement:** Time from config argument to processing start
- **Acceptance:** Linear scaling with step count, max 100ms for 100 steps

### 4.2 Reliability Requirements (REQ-REL)

#### 4.2.1 Error Handling (REQ-REL-001)
- **Requirement:** Graceful handling of all input error conditions
- **Error Conditions:**
  - Invalid regex patterns
  - Malformed configuration files
  - Insufficient memory
  - Disk full conditions
  - Interrupted input streams
- **Behavior:** Return appropriate exit codes, clear error messages to stderr

#### 4.2.2 Data Integrity (REQ-REL-002)
- **Requirement:** No data loss or corruption during processing
- **Test:** Round-trip operations must be reversible where applicable
- **Verification:** Checksum validation on known datasets

#### 4.2.3 Resource Cleanup (REQ-REL-003)
- **Requirement:** Proper cleanup on interruption (SIGINT, SIGTERM)
- **Behavior:** Close file handles, flush buffers, exit cleanly

### 4.3 Usability Requirements (REQ-USE)

#### 4.3.1 Error Messages (REQ-USE-001)
- **Requirement:** Clear, actionable error messages with context
- **Format:** 
```
Error: Invalid regex pattern at line 23 in config file
  Pattern: '(?P<invalid'
  Error: Missing closing parenthesis for named group
  Suggestion: Add closing ')' after group name
```

#### 4.3.2 Help Documentation (REQ-USE-002)
- **Requirement:** Comprehensive built-in help system
- **Commands:**
  - `rexpipe --help`: General usage
  - `rexpipe --help examples`: Common usage examples
  - `rexpipe --help config`: Configuration file format
- **Content:** Examples, syntax reference, common patterns

#### 4.3.3 Progress Reporting (REQ-USE-003)
- **Requirement:** Progress indication for long-running operations
- **Trigger:** Operations expected to take >5 seconds
- **Format:** `Processing... 2.3GB processed (45MB/s) [ETA: 2m15s]`
- **Output:** To stderr to avoid interfering with stdout

### 4.4 Compatibility Requirements (REQ-COMPAT)

#### 4.4.1 Platform Support (REQ-COMPAT-001)
- **Primary:** Linux x86_64 (Ubuntu 20.04+, RHEL 8+)
- **Secondary:** macOS x86_64/ARM64 (macOS 11+)
- **Tertiary:** Windows x86_64 (Windows 10+)

#### 4.4.2 Shell Integration (REQ-COMPAT-002)
- **Requirement:** Seamless integration with bash, zsh, fish shells
- **Behavior:** Proper exit codes, signal handling, stdout/stderr usage
- **Test:** Common pipeline patterns work identically to traditional tools

#### 4.4.3 Character Encoding (REQ-COMPAT-003)
- **Requirement:** UTF-8 input/output support
- **Behavior:** Preserve character encoding through pipeline
- **Error Handling:** Graceful handling of invalid UTF-8 sequences

---

## 5. Interface Requirements

### 5.1 Command-Line Interface Specification (REQ-INT-CLI)

#### 5.1.1 Argument Structure
```
rexpipe [GLOBAL_OPTIONS] OPERATION [OPERATION_OPTIONS] [OPERATION ...]

GLOBAL_OPTIONS:
  --config, -c FILE       Load pipeline from configuration file
  --help, -h             Show help message
  --version, -V          Show version information
  --verbose, -v          Enable verbose output
  --quiet, -q            Suppress progress and non-error output
  --dry-run             Process input without producing output
  --validate            Validate configuration without processing

OPERATIONS:
  --sub, -s PATTERN REPLACEMENT    Substitute pattern with replacement
  --filter PATTERN [--action ACTION]  Filter lines by pattern
  --keep PATTERN                   Keep only matching lines
  --drop PATTERN                   Drop matching lines
  --extract PATTERN [--group N]   Extract matching text
  --pattern PATTERN --inspect      Inspect pattern matches

OPERATION_OPTIONS:
  --global, -g          Apply to all matches (default: first only)
  --case-insensitive, -i    Ignore case in pattern matching
  --multiline, -m       Enable multiline mode
  --field N             Apply to field N in delimited text
  --delimiter CHAR      Field delimiter (default: auto-detect)
```

#### 5.1.2 Exit Codes
```
0   Success
1   General error (invalid arguments, processing failure)
2   Configuration error (invalid config file, validation failure)
3   Input/Output error (file not found, permission denied)
4   Pattern error (invalid regex, compilation failure)
64  Usage error (invalid command line arguments)
```

### 5.2 Configuration File Interface (REQ-INT-CONFIG)

#### 5.2.1 Complete TOML Schema
```toml
# Metadata section (optional)
[metadata]
name = "string"           # Pipeline name
description = "string"    # Pipeline description  
version = "string"        # Version identifier
author = "string"         # Author information

# Global settings (optional)
[settings]
field_delimiter = "string"    # Default field delimiter
case_sensitive = boolean      # Default case sensitivity
multiline = boolean          # Default multiline mode
max_memory = "string"        # Memory limit (e.g., "500MB")

# Pipeline steps (required, minimum 1)
[[step]]
type = "substitute" | "filter" | "extract" | "conditional"
description = "string"        # Step description (optional)
pattern = "string"           # Regex pattern (required)
replacement = "string"       # Replacement string (substitute only)
action = "keep" | "drop"     # Filter action (filter only)
group = integer             # Capture group number (extract only)
field = integer             # Field number for processing (optional)
flags = [string]            # Array of flags (optional)

# Conditional step structure
[[step]]
type = "conditional"
condition = "string"        # Pattern to test
[[step.if_match]]           # Steps to execute if pattern matches
type = "substitute" | "filter" | "extract"
# ... step configuration ...
[[step.if_no_match]]        # Steps to execute if pattern doesn't match
type = "substitute" | "filter" | "extract"  
# ... step configuration ...
```

#### 5.2.2 Flag Values
```toml
flags = [
  "global",              # Apply to all matches (g)
  "case_insensitive",    # Ignore case (i)  
  "multiline",          # Multiline mode (m)
  "dotall",             # . matches newlines (s)
  "extended",           # Extended regex syntax (x)
  "ungreedy"            # Non-greedy quantifiers (U)
]
```

### 5.3 Input/Output Specifications (REQ-INT-IO)

#### 5.3.1 Standard Input Processing
- **Format:** UTF-8 text stream
- **Behavior:** Line-buffered processing for interactive use
- **Buffer Size:** 64KB read buffer for optimal performance
- **EOF Handling:** Process final line even without trailing newline

#### 5.3.2 Standard Output Format
- **Default:** Transformed text maintaining original line structure
- **Inspect Mode:** Structured analysis output
- **Progress Mode:** Progress information to stderr only

#### 5.3.3 Standard Error Usage
- **Error Messages:** All error and diagnostic output
- **Progress Information:** Processing status and statistics
- **Verbose Output:** Debug information when --verbose enabled

---

## 6. Quality Attributes

### 6.1 Maintainability (REQ-QUAL-MAINT)
- **Code Coverage:** Minimum 90% test coverage
- **Documentation:** All public APIs documented with examples
- **Code Style:** Enforced via rustfmt and clippy
- **Dependency Management:** Minimal external dependencies, regular security updates

### 6.2 Portability (REQ-QUAL-PORT)
- **Static Compilation:** Single binary with no external dependencies
- **Cross-Platform:** Identical behavior across supported platforms
- **Architecture Support:** Native binaries for x86_64 and ARM64

### 6.3 Security (REQ-QUAL-SEC)
- **Input Validation:** All regex patterns validated before compilation
- **Resource Limits:** Protection against ReDoS (Regular Expression Denial of Service)
- **Memory Safety:** Rust's memory safety guarantees prevent buffer overflows
- **Configuration Security:** Validation of all configuration file inputs

---

## 7. Verification and Validation

### 7.1 Test Categories

#### 7.1.1 Unit Tests
- Individual regex operation correctness
- Configuration file parsing
- Error handling for invalid inputs
- Memory management verification

#### 7.1.2 Integration Tests  
- End-to-end pipeline processing
- CLI argument parsing and execution
- Configuration file workflow execution
- Performance benchmark validation

#### 7.1.3 System Tests
- Large file processing (1GB+)
- Memory usage verification
- Cross-platform compatibility
- Shell integration testing

### 7.2 Acceptance Criteria
Each requirement must pass:
- **Functional Testing:** Feature works as specified
- **Performance Testing:** Meets performance requirements
- **Error Testing:** Handles error conditions gracefully
- **Usability Testing:** Provides clear feedback and documentation

---

This SRS serves as the complete specification for rexpipe development and validation. All requirements are designed to be testable, measurable, and verifiable through automated testing and performance benchmarking.

