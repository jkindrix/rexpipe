use anyhow::{Error as AnyhowError, Result, anyhow};
use clap::{Arg, ArgAction, Command, ValueHint, value_parser};
use clap_complete::{Generator, Shell, generate};
use log::{debug, info};
use std::fs::File;
use std::io::{self, BufReader, IsTerminal};
use std::path::{Path, PathBuf};

// Import from the library crate
use rexpipe::checkpoint::{Checkpoint, CheckpointConfig};
use rexpipe::crossfile::{CrossFileConfig, CrossFileManager, ViolationAction, format_check_report};
use rexpipe::data::{DataFormat, DataValue};
use rexpipe::error::{ConfigError, LibraryError, PatternError, RexpipeError, ValidationError};
use rexpipe::files::{FileProcessingOptions, MultiFileProcessor, MultiFileResult};
use rexpipe::inspector::{Inspector, InspectorOptions};
use rexpipe::json_schema;
use rexpipe::library;
use rexpipe::library::LibraryResolver;
use rexpipe::pipeline::{MaxLineAction, PipelineConfig, PipelineSettings, PipelineStep, StepType, TransformAction, RegexFlag};
use rexpipe::processor::StreamProcessor;

/// Exit codes for different error conditions.
///
/// These follow the standard Unix/grep convention:
/// - 0: Success (matches found, or operation completed successfully)
/// - 1: No matches found (not an error, but nothing matched)
/// - 2+: Various error conditions
///
/// This allows scripts to distinguish between "no matches" (exit 1) and
/// "something went wrong" (exit 2+):
///
/// ```bash
/// rexpipe -p 'pattern' file.txt
/// case $? in
///     0) echo "Matches found" ;;
///     1) echo "No matches" ;;
///     *) echo "Error occurred" ;;
/// esac
/// ```
mod exit_codes {
    /// Success - operation completed normally with matches found.
    /// Note: This constant exists for documentation completeness. In practice,
    /// success is indicated by main() completing normally (implicit exit 0).
    #[allow(dead_code)]
    pub const SUCCESS: i32 = 0;
    /// No matches found (grep-like behavior: not an error, just no results)
    pub const NO_MATCHES: i32 = 1;
    /// General/unspecified error
    pub const GENERAL_ERROR: i32 = 2;
    /// Invalid command line usage or missing arguments
    pub const USAGE_ERROR: i32 = 2;
    /// Configuration file error (not found, invalid TOML, etc.)
    pub const CONFIG_ERROR: i32 = 3;
    /// Invalid regex pattern
    pub const PATTERN_ERROR: i32 = 4;
    /// File I/O error (file not found, permission denied, etc.)
    pub const IO_ERROR: i32 = 5;
    /// Validation error (pipeline configuration validation failed)
    pub const VALIDATION_ERROR: i32 = 6;
}

/// Determine if colored output should be used.
/// Respects the NO_COLOR environment variable (https://no-color.org/)
/// and the --no-color CLI flag.
fn should_use_color(matches: &clap::ArgMatches) -> bool {
    // --no-color flag explicitly disables color
    if matches.get_flag("no-color") {
        return false;
    }

    // NO_COLOR env var (any non-empty value disables color)
    if std::env::var("NO_COLOR").map(|v| !v.is_empty()).unwrap_or(false) {
        return false;
    }

    // Otherwise, use color if stdout is a terminal
    std::io::stdout().is_terminal()
}

/// Determine if JSON output should be used.
/// AI-native behavior: JSON is default when stdout is not a terminal (piped output).
/// This makes rexpipe ideal for AI agent consumption without explicit flags.
///
/// Priority:
/// 1. --text flag forces plain text output (returns false)
/// 2. --json flag forces JSON output (returns true)
/// 3. Default: JSON when stdout is NOT a terminal (AI-native)
fn should_use_json(matches: &clap::ArgMatches) -> bool {
    // --text forces plain text output
    if matches.get_flag("text") {
        return false;
    }

    // --json forces JSON output
    if matches.get_flag("json") {
        return true;
    }

    // AI-native default: JSON when stdout is not a terminal
    !io::stdout().is_terminal()
}

/// Categorize error type using structured error types for exit code selection.
/// Falls back to string matching for errors that weren't wrapped in our types.
fn categorize_error(error: &AnyhowError) -> i32 {
    // Try to downcast to our structured error types first
    if let Some(rexpipe_err) = error.downcast_ref::<RexpipeError>() {
        return match rexpipe_err {
            RexpipeError::Config(_) => exit_codes::CONFIG_ERROR,
            RexpipeError::Pattern(_) => exit_codes::PATTERN_ERROR,
            RexpipeError::Io(_) => exit_codes::IO_ERROR,
            RexpipeError::Library(_) => exit_codes::CONFIG_ERROR, // Library errors are config-related
            RexpipeError::Validation(_) => exit_codes::VALIDATION_ERROR,
            RexpipeError::Processing(_) => exit_codes::GENERAL_ERROR,
        };
    }

    // Check for specific nested error types
    if error.downcast_ref::<ConfigError>().is_some() {
        return exit_codes::CONFIG_ERROR;
    }
    if error.downcast_ref::<PatternError>().is_some() {
        return exit_codes::PATTERN_ERROR;
    }
    if error.downcast_ref::<LibraryError>().is_some() {
        return exit_codes::CONFIG_ERROR;
    }
    if error.downcast_ref::<ValidationError>().is_some() {
        return exit_codes::VALIDATION_ERROR;
    }
    if error.downcast_ref::<std::io::Error>().is_some() {
        return exit_codes::IO_ERROR;
    }

    // Fallback to string matching for errors from external crates
    let error_msg = error.to_string().to_lowercase();

    if error_msg.contains("missing required")
        || error_msg.contains("must specify")
        || error_msg.contains("invalid argument")
    {
        exit_codes::USAGE_ERROR
    } else if error_msg.contains("no such file")
        || error_msg.contains("not found")
        || error_msg.contains("permission denied")
    {
        exit_codes::IO_ERROR
    } else if error_msg.contains("invalid regex") || error_msg.contains("regex parse error") {
        exit_codes::PATTERN_ERROR
    } else if error_msg.contains("toml") || error_msg.contains("parse error") {
        exit_codes::CONFIG_ERROR
    } else if error_msg.contains("validation") {
        exit_codes::VALIDATION_ERROR
    } else {
        exit_codes::GENERAL_ERROR
    }
}

/// Help text with examples shown at the end of --help output
const EXAMPLES_HELP: &str = r#"
EXAMPLES:
    Basic substitution (stdin):
        echo "Hello 123 World" | rexpipe -p '\d+' -r 'NUM'
        # Output: Hello NUM World

    Find and replace in files:
        rexpipe -p 'TODO' -r 'DONE' -i *.txt
        # Edits files in-place

    With backup before editing:
        rexpipe -p 'old' -r 'new' -i -b .bak src/*.rs
        # Creates .bak backups

    Using a config file:
        rexpipe -c pipeline.toml < input.txt

    Debug/preview pattern matching:
        rexpipe -p 'ERROR.*code=(\d+)' --inspect < logs.txt
        # Shows matches with highlighting

    Preview changes before applying (dry-run):
        rexpipe -p 'foo' -r 'bar' --dry-run -i *.txt
        # Shows diff without modifying files

    Process all files recursively:
        rexpipe -p '\d{4}-\d{2}-\d{2}' -R src/
        # Finds date patterns in src/

    Only show files with matches:
        rexpipe -p 'FIXME' -l -R .
        # Lists files containing FIXME

    Count matches per file:
        rexpipe -p 'TODO' --count -R .

    PCRE mode (lookahead/lookbehind):
        rexpipe -P -p '(?<=user=)\w+' < logs.txt
        # Uses PCRE regex engine

    Fixed string mode (no regex):
        rexpipe -F -p '*.txt' -r '[files]' < input.txt
        # Matches literal *.txt

    Use pattern library:
        rexpipe -p '${email}' --library patterns/common.toml < data.txt
        # Uses predefined email pattern

    List available patterns:
        rexpipe --list-patterns patterns/common.toml

    JSON output:
        rexpipe -p '\w+@\w+\.\w+' --json < emails.txt

    Extract emails from text:
        rexpipe -p '\w+@\w+\.\w+' --extract < data.txt
        # Outputs only matching patterns, one per line

    Transform case:
        echo "myVariableName" | rexpipe -p '\w+' --transform snake_case
        # Output: my_variable_name

CONFIGURATION FILE EXAMPLE:
    [[step]]
    type = "filter"
    pattern = "ERROR"
    action = "keep_line"

    [[step]]
    type = "substitute"
    pattern = "password=\\w+"
    replacement = "password=***"

EXIT CODES:
    0    Success (matches found or operation completed)
    1    No matches found
    2    General error
    3    Configuration error
    4    Pattern/regex error
    5    I/O error
    6    Validation error

For more information, see: https://github.com/rexpipe/rexpipe
"#;

/// Build the CLI command structure
/// Separated for use with clap_complete shell completion generation
fn build_cli() -> Command {
    Command::new("rexpipe")
        .version("1.1.0")
        .about("Unified regex pipeline processor for text transformation")
        .after_long_help(EXAMPLES_HELP)
        // === Pattern and Config ===
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("TOML configuration file")
                .value_hint(ValueHint::FilePath),
        )
        .arg(
            Arg::new("pattern")
                .short('p')
                .long("pattern")
                .value_name("REGEX")
                .help("Inline regex pattern"),
        )
        .arg(
            Arg::new("replacement")
                .short('r')
                .long("replacement")
                .value_name("TEXT")
                .help("Replacement text for substitution"),
        )
        // === Regex Engine Options ===
        .arg(
            Arg::new("fixed")
                .short('F')
                .long("fixed")
                .help("Treat pattern as fixed string (no regex interpretation)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("pcre")
                .short('P')
                .long("pcre")
                .help("Use PCRE-compatible regex via fancy-regex (supports lookahead/lookbehind)")
                .action(ArgAction::SetTrue),
        )
        // === File Operations ===
        .arg(
            Arg::new("in-place")
                .short('i')
                .long("in-place")
                .help("Edit files in-place (like sed -i)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("backup")
                .short('b')
                .long("backup")
                .value_name("SUFFIX")
                .help("Create backup with given suffix when editing in-place (e.g., .bak)"),
        )
        .arg(
            Arg::new("recursive")
                .short('R')
                .long("recursive")
                .help("Recursively process directories")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("glob")
                .short('g')
                .long("glob")
                .value_name("PATTERN")
                .help("Only process files matching glob pattern (e.g., '*.txt')")
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new("exclude")
                .short('e')
                .long("exclude")
                .value_name("PATTERN")
                .help("Exclude files matching glob pattern")
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new("no-ignore")
                .long("no-ignore")
                .help("Don't respect .gitignore files")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("hidden")
                .long("hidden")
                .help("Include hidden files")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("max-depth")
                .long("max-depth")
                .value_name("NUM")
                .help("Maximum directory recursion depth"),
        )
        .arg(
            Arg::new("binary")
                .long("binary")
                .value_name("MODE")
                .help("Binary file handling: 'auto' (skip, default), 'text' (process as text), 'skip' (always skip)")
                .value_parser(["auto", "text", "skip"])
                .default_value("auto"),
        )
        // === Processing Modes ===
        .arg(
            Arg::new("parallel")
                .short('j')
                .long("parallel")
                .help("Process files in parallel")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("timeout")
                .long("timeout")
                .value_name("MS")
                .help("Timeout in milliseconds per line (0 = no timeout)")
                .value_parser(value_parser!(u64)),
        )
        .arg(
            Arg::new("async")
                .long("async")
                .help("Use async I/O for file processing (requires async feature)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("progress")
                .long("progress")
                .help("Show progress indicator for multi-file processing")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("inspect")
                .long("inspect")
                .help("Enable inspection mode")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("interactive")
                .long("interactive")
                .help("Enable interactive inspection")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .help("Preview changes without modifying (works with stdin or -i mode)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("apply")
                .long("apply")
                .help("Actually apply changes (required for in-place edits when piping/scripting)")
                .long_help(
                    "Explicitly confirm that file modifications should be applied. \
                     In AI-native mode (non-interactive), this flag is required for \
                     destructive operations like in-place editing (-i). This prevents \
                     accidental file modifications when rexpipe is used by AI agents \
                     or in automated pipelines."
                )
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("extract")
                .long("extract")
                .help("Extract matching patterns instead of substituting")
                .long_help(
                    "Extract mode: output only the matched patterns, one per line. \
                     Useful for extracting structured data like emails, URLs, IDs. \
                     Combined with --json, outputs matches as JSON arrays."
                )
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("transform")
                .long("transform")
                .value_name("NAME")
                .help("Apply a named transform to matches (e.g., snake_case, camel_case)")
                .long_help(
                    "Apply a built-in transformation to all matched text. Available transforms:\n\n  \
                     Case transforms:\n  \
                     - snake_case: Convert to snake_case\n  \
                     - camel_case: Convert to camelCase\n  \
                     - pascal_case: Convert to PascalCase\n  \
                     - kebab_case: Convert to kebab-case\n  \
                     - uppercase: Convert to UPPERCASE\n  \
                     - lowercase: Convert to lowercase\n  \
                     - title_case: Convert to Title Case\n\n  \
                     String manipulation:\n  \
                     - reverse: Reverse the text\n  \
                     - trim: Remove leading/trailing whitespace\n  \
                     - remove_whitespace: Remove all whitespace\n  \
                     - normalize_whitespace: Collapse runs of whitespace to single space\n  \
                     - deduplicate: Remove duplicate lines\n  \
                     - sort_chars: Sort characters alphabetically\n  \
                     - char_count: Replace with character count\n  \
                     - word_count: Replace with word count\n\n  \
                     Encoding:\n  \
                     - base64_encode: Encode as base64\n  \
                     - base64_decode: Decode from base64\n  \
                     - url_encode: URL-encode special characters\n  \
                     - url_decode: Decode URL-encoded text"
                ),
        )
        // === Syntax-Aware Processing ===
        .arg(
            Arg::new("scope")
                .long("scope")
                .value_name("SCOPE")
                .help("Limit matches to syntax scope (code, string, comment)")
                .long_help(
                    "Only match patterns within the specified syntax scope. Requires tree-sitter feature.\n  \
                     - code: Match only in code, not strings or comments\n  \
                     - string: Match only within string literals\n  \
                     - comment: Match only within comments\n\n\
                     Combine with --language to specify the source language."
                )
                .value_parser(["code", "string", "comment"]),
        )
        .arg(
            Arg::new("language")
                .long("language")
                .value_name("LANG")
                .help("Source language for syntax-aware processing")
                .long_help(
                    "Specify the programming language for syntax-aware matching. \
                     Used with --scope to limit matches to specific syntax contexts.\n\n\
                     Supported languages: rust, python, javascript, typescript, go, c, cpp, java, ruby"
                ),
        )
        // === Streaming Mode ===
        .arg(
            Arg::new("stream")
                .long("stream")
                .help("Run in continuous streaming mode (requires --source)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("input-uri")
                .long("source")
                .value_name("URI")
                .help("Streaming input source URI (stdin://, file:///path, tcp://host:port, udp://host:port)")
                .long_help("Specify input source using URI format for streaming mode:\n\
                     - stdin://           Read from standard input\n\
                     - file:///path       Read from a file\n\
                     - tcp://host:port    Accept TCP connections\n\
                     - udp://host:port    Receive UDP datagrams"),
        )
        .arg(
            Arg::new("output-uri")
                .long("sink")
                .value_name("URI")
                .help("Streaming output sink URI (stdout://, file:///path, tcp://host:port, udp://host:port)")
                .long_help("Specify output sink using URI format for streaming mode:\n\
                     - stdout://          Write to standard output\n\
                     - stderr://          Write to standard error\n\
                     - file:///path       Write to a file\n\
                     - tcp://host:port    Send to TCP socket\n\
                     - udp://host:port    Send UDP datagrams"),
        )
        // === Output Modes ===
        .arg(
            Arg::new("count")
                .long("count")
                .help("Only show count of matches per file")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("files-with-matches")
                .short('l')
                .long("files-with-matches")
                .help("Only list files containing matches")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("files-without-matches")
                .short('L')
                .long("files-without-matches")
                .help("Only list files not containing matches")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .help("Quiet mode - only set exit code")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output results as JSON (default when stdout is not a terminal)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("text")
                .long("text")
                .help("Force plain text output even when piping (override AI-native JSON default)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("jsonl")
                .long("jsonl")
                .help("Output results as streaming JSON Lines (one JSON object per line)")
                .action(ArgAction::SetTrue)
                .conflicts_with("json"),
        )
        .arg(
            Arg::new("no-color")
                .long("no-color")
                .help("Disable colored output (also respects NO_COLOR env var)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("error-format")
                .long("error-format")
                .value_name("FORMAT")
                .help("Error output format: text (default) or json for AI-parseable errors")
                .value_parser(["text", "json"])
                .default_value("text"),
        )
        // === Context Lines (for inspection) ===
        .arg(
            Arg::new("context-before")
                .short('B')
                .long("before-context")
                .value_name("NUM")
                .help("Show NUM lines before each match"),
        )
        .arg(
            Arg::new("context-after")
                .short('A')
                .long("after-context")
                .value_name("NUM")
                .help("Show NUM lines after each match"),
        )
        .arg(
            Arg::new("context")
                .short('C')
                .long("context")
                .value_name("NUM")
                .help("Show NUM lines before and after each match"),
        )
        // === Pattern Library ===
        .arg(
            Arg::new("list-patterns")
                .long("list-patterns")
                .value_name("LIBRARY")
                .help("List all patterns in a pattern library file")
                .value_hint(ValueHint::FilePath),
        )
        .arg(
            Arg::new("validate-library")
                .long("validate-library")
                .value_name("LIBRARY")
                .help("Validate a pattern library file")
                .value_hint(ValueHint::FilePath),
        )
        // === Git Filter Integration ===
        .arg(
            Arg::new("git-filter-setup")
                .long("git-filter-setup")
                .value_name("FILTER_NAME")
                .help("Generate git filter configuration for clean/smudge operations")
                .long_help(
                    "Generate configuration for using rexpipe as a git clean/smudge filter. \
                     This outputs git config commands and .gitattributes entries. Use with a \
                     pipeline config to automatically transform files on commit (clean) and \
                     checkout (smudge). Example: rexpipe --git-filter-setup sanitize -c sanitize.toml"
                ),
        )
        // === Pattern Discovery ===
        .arg(
            Arg::new("discover")
                .long("discover")
                .help("Discover potential patterns in input (frequency analysis)")
                .long_help(
                    "Analyze input to discover potential patterns not covered by current pipeline. \
                     Uses frequency analysis to find repeated structures like IDs, dates, emails, \
                     phone numbers, etc. Outputs suggested patterns with match counts."
                )
                .action(ArgAction::SetTrue),
        )
        // === Data Processing ===
        .arg(
            Arg::new("convert")
                .long("convert")
                .help("Convert between data formats (json, csv, yaml, xml, toml)")
                .long_help(
                    "Convert data from one format to another. Use --input-format and --output-format \
                     to specify formats explicitly, or let rexpipe detect the input format.\n\n\
                     Supported formats: json, jsonl, csv, tsv, yaml, xml, toml\n\n\
                     Examples:\n  \
                     rexpipe --convert --output-format yaml < data.json\n  \
                     rexpipe --convert --input-format csv --output-format json < data.csv"
                )
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("data-query")
                .long("query")
                .short('Q')
                .value_name("EXPR")
                .help("Query data with jq-like expressions (e.g., '.users[0].name')")
                .long_help(
                    "Query structured data using path expressions similar to jq.\n\n\
                     Path syntax:\n  \
                     .key       - Access object key\n  \
                     [0]        - Access array index\n  \
                     .[*]       - Access all array elements\n  \
                     .key1.key2 - Chain accessors\n\n\
                     Examples:\n  \
                     rexpipe -Q '.name' < user.json\n  \
                     rexpipe -Q '.users[0].email' < data.json\n  \
                     rexpipe -Q '.[*].id' < items.json"
                ),
        )
        .arg(
            Arg::new("input-format")
                .long("input-format")
                .value_name("FORMAT")
                .help("Explicit input data format")
                .value_parser(["text", "json", "jsonl", "csv", "tsv", "yaml", "xml", "toml"]),
        )
        .arg(
            Arg::new("output-format")
                .long("output-format")
                .value_name("FORMAT")
                .help("Output data format for conversion")
                .value_parser(["text", "json", "jsonl", "csv", "tsv", "yaml", "xml", "toml"]),
        )
        .arg(
            Arg::new("pretty")
                .long("pretty")
                .help("Pretty print output (for JSON, XML, etc.)")
                .action(ArgAction::SetTrue),
        )
        // === Bidirectional Pipelines ===
        .arg(
            Arg::new("reverse")
                .long("reverse")
                .help("Run pipeline in reverse mode (requires bidirectional config or mapping file)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("mapping-file")
                .long("mapping-file")
                .value_name("FILE")
                .help("File to store/load bidirectional transformation mappings")
                .long_help(
                    "Store transformation mappings for bidirectional pipelines. \
                     In forward mode, mappings are recorded. In reverse mode with --reverse, \
                     mappings are used to restore original values."
                )
                .value_hint(ValueHint::FilePath),
        )
        // === Checkpoint/Incremental Processing ===
        .arg(
            Arg::new("checkpoint")
                .long("checkpoint")
                .value_name("FILE")
                .help("Enable incremental processing with checkpoint file")
                .long_help(
                    "Resume processing from saved position. Tracks file offsets and content hashes \
                     to only process new or changed content. Useful for growing log files."
                )
                .value_hint(ValueHint::FilePath),
        )
        .arg(
            Arg::new("resume")
                .long("resume")
                .help("Resume processing from checkpoint (requires --checkpoint)")
                .long_help(
                    "Resume processing from the last saved checkpoint position. \
                     Must be used with --checkpoint to specify the checkpoint file. \
                     Only processes new content since the last checkpoint was saved."
                )
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("git-diff")
                .long("git-diff")
                .value_name("REF")
                .help("Only process lines changed since git ref (e.g., HEAD~1, main)")
                .num_args(0..=1)
                .default_missing_value("HEAD"),
        )
        // === Cross-file Consistency ===
        .arg(
            Arg::new("cross-file")
                .long("cross-file")
                .value_name("FILE")
                .help("Load cross-file consistency rules from a TOML file")
                .long_help(
                    "Load cross-file relationship rules from a TOML configuration file. \
                     Rules define patterns that should be consistent across related files, \
                     such as API version strings in source and test files."
                )
                .value_hint(ValueHint::FilePath),
        )
        // === Pipeline Testing ===
        .arg(
            Arg::new("test")
                .long("test")
                .help("Run tests defined in pipeline configuration")
                .long_help(
                    "Execute test cases defined in the [tests] section of the pipeline config. \
                     Validates that the pipeline produces expected outputs for given inputs."
                )
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("test-format")
                .long("test-format")
                .value_name("FORMAT")
                .help("Test output format: text, tap, or junit")
                .value_parser(["text", "tap", "junit"])
                .default_value("text"),
        )
        // === Pattern Learning ===
        .arg(
            Arg::new("learn")
                .long("learn")
                .help("Learn patterns from input examples")
                .long_help(
                    "Infer regex patterns from positive and negative examples. \
                     Use with --positive and --negative to provide examples. \
                     Outputs suggested patterns with confidence scores."
                )
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("positive")
                .long("positive")
                .value_name("EXAMPLE")
                .help("Positive example for pattern learning (should match)")
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new("negative")
                .long("negative")
                .value_name("EXAMPLE")
                .help("Negative example for pattern learning (should not match)")
                .action(ArgAction::Append),
        )
        // === Misc ===
        .arg(
            Arg::new("performance")
                .long("performance")
                .help("Show performance metrics")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("validate")
                .long("validate")
                .help("Validate configuration only")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("explain")
                .long("explain")
                .help("Explain what the pipeline will do without processing data")
                .long_help(
                    "Output a human-readable description of the pipeline's behavior. \
                     Lists each step, what patterns it matches, and what transformations \
                     it applies. Useful for AI agents to understand a pipeline before \
                     running it. Output can be JSON with --json flag."
                )
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("verify")
                .long("verify")
                .help("Output verification summary after processing")
                .long_help(
                    "After processing, output a verification summary confirming what \
                     transformations were applied. Shows line counts, match counts, \
                     and transformation counts. Useful for AI agents to confirm that \
                     processing completed as expected. Output can be JSON with --json flag."
                )
                .action(ArgAction::SetTrue),
        )
        // === Security ===
        .arg(
            Arg::new("no-shell")
                .long("no-shell")
                .help("Disable shell command execution in transforms (security)")
                .action(ArgAction::SetTrue),
        )
        // === Line Endings ===
        .arg(
            Arg::new("crlf")
                .long("crlf")
                .visible_alias("preserve-line-endings")
                .help("Preserve CRLF (Windows) line endings in output")
                .long_help(
                    "Preserve original line endings when processing files. By default, all \
                     output uses Unix-style LF line endings. With this flag, lines that had \
                     CRLF (Windows) endings in the input will have CRLF endings in the output."
                )
                .action(ArgAction::SetTrue),
        )
        // === Large Line Handling ===
        .arg(
            Arg::new("max-line-length")
                .long("max-line-length")
                .short('M')
                .value_name("SIZE")
                .help("Maximum line length (e.g., 1M, 512K, 10000)")
                .long_help(
                    "Maximum line length in bytes. Lines exceeding this limit will be handled \
                     according to --max-line-action. Supports suffixes: K (kilobytes), M (megabytes), \
                     G (gigabytes). Default: unlimited. Use this to prevent memory issues with \
                     minified files or binary content misidentified as text."
                ),
        )
        .arg(
            Arg::new("max-line-action")
                .long("max-line-action")
                .value_name("ACTION")
                .value_parser(["skip", "error", "truncate"])
                .default_value("skip")
                .help("Action for lines exceeding --max-line-length")
                .long_help(
                    "Action to take when a line exceeds --max-line-length:\n\
                     - skip: Output the line unchanged without processing (default)\n\
                     - error: Exit with an error\n\
                     - truncate: Truncate the line at the limit and process"
                ),
        )
        .arg(
            Arg::new("strict")
                .long("strict")
                .help("Reject potentially dangerous ReDoS regex patterns")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("export")
                .long("export")
                .value_name("FORMAT")
                .help("Export configuration (toml or json)"),
        )
        .arg(
            Arg::new("completions")
                .long("completions")
                .value_name("SHELL")
                .help("Generate shell completion script")
                .value_parser(value_parser!(Shell)),
        )
        // === I/O ===
        .arg(
            Arg::new("input")
                .short('f')
                .long("input")
                .value_name("FILE")
                .help("Input file (default: stdin)")
                .value_hint(ValueHint::FilePath),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("FILE")
                .help("Output file (default: stdout)")
                .value_hint(ValueHint::FilePath),
        )
        // === Positional Args ===
        .arg(
            Arg::new("paths")
                .help("Files or directories to process")
                .action(ArgAction::Append)
                .num_args(0..)
                .value_hint(ValueHint::AnyPath),
        )
}

/// Generate shell completion script for the given shell
fn print_completions<G: Generator>(generator: G, cmd: &mut Command) {
    generate(
        generator,
        cmd,
        cmd.get_name().to_string(),
        &mut io::stdout(),
    );
}

fn main() {
    // Initialize logger from RUST_LOG environment variable
    // Example: RUST_LOG=rexpipe=debug rexpipe --config my.toml < input.txt
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .format_timestamp(None)
        .init();

    debug!("Starting rexpipe");

    let matches = build_cli().get_matches();

    // Handle completions generation first (before any other processing)
    if let Some(shell) = matches.get_one::<Shell>("completions").copied() {
        let mut cmd = build_cli();
        print_completions(shell, &mut cmd);
        return;
    }

    if let Err(e) = run_application(&matches) {
        let exit_code = categorize_error(&e);

        // Check if JSON error output is requested
        let use_json_errors = matches
            .get_one::<String>("error-format")
            .map(|s| s == "json")
            .unwrap_or(false);

        if use_json_errors {
            // Output structured JSON error for AI consumption
            match json_schema::output_error_json(&e.to_string(), exit_code, None) {
                Ok(json) => eprintln!("{}", json),
                Err(_) => eprintln!("Error: {}", e), // Fallback to plain text
            }
        } else {
            eprintln!("Error: {}", e);
        }

        std::process::exit(exit_code);
    }
}

fn run_application(matches: &clap::ArgMatches) -> Result<()> {
    // Handle pattern library commands first (don't require pipeline config)
    if let Some(library_path) = matches.get_one::<String>("list-patterns") {
        return list_library_patterns(library_path);
    }

    if let Some(library_path) = matches.get_one::<String>("validate-library") {
        return validate_library_file(library_path);
    }

    // Handle git filter setup
    if let Some(filter_name) = matches.get_one::<String>("git-filter-setup") {
        return print_git_filter_setup(filter_name, matches);
    }

    // Handle pattern discovery mode
    if matches.get_flag("discover") {
        return run_pattern_discovery(matches);
    }

    // Handle pattern learning mode
    if matches.get_flag("learn") {
        return run_pattern_learning(matches);
    }

    // Handle data conversion mode
    if matches.get_flag("convert") {
        return run_data_conversion(matches);
    }

    // Handle data query mode
    if matches.get_one::<String>("data-query").is_some() {
        return run_data_query(matches);
    }

    // Handle streaming mode
    if matches.get_flag("stream") || matches.contains_id("input-uri") {
        return run_streaming_mode(matches);
    }

    // Build pipeline settings from CLI flags
    let settings = build_pipeline_settings(matches);

    // Load or create pipeline configuration
    let config = load_pipeline_config(matches, settings)?;

    // Handle export mode
    if let Some(format) = matches.get_one::<String>("export") {
        return export_configuration(&config, format);
    }

    // Handle pipeline test mode
    if matches.get_flag("test") {
        return run_pipeline_tests(&config, matches);
    }

    // Validate configuration if requested (unless we have files to preview)
    if matches.get_flag("validate") {
        return validate_configuration(&config);
    }

    // Handle explain mode: describe what pipeline will do
    if matches.get_flag("explain") {
        return explain_pipeline(&config, matches);
    }

    // Check if we're in multi-file mode
    let paths: Vec<PathBuf> = matches
        .get_many::<String>("paths")
        .map(|v| v.map(PathBuf::from).collect())
        .unwrap_or_default();

    let is_multi_file =
        matches.get_flag("recursive") || matches.get_flag("in-place") || !paths.is_empty();

    // AI-native safety: require --apply for in-place edits in non-interactive mode
    // This prevents accidental file modifications when used by AI agents or scripts
    let in_place = matches.get_flag("in-place");
    let has_apply = matches.get_flag("apply");
    let is_interactive = io::stdin().is_terminal() && io::stdout().is_terminal();

    if in_place && !is_interactive && !has_apply && !matches.get_flag("dry-run") {
        // Non-interactive in-place edit without --apply: show dry-run preview
        eprintln!("AI-native safety: In-place editing requires --apply flag in non-interactive mode.");
        eprintln!("Showing dry-run preview instead. Add --apply to actually modify files.\n");
        return run_dry_run_preview(&config, matches, paths);
    }

    // Handle dry-run: show preview for in-place mode, stdin preview, or just validate
    if matches.get_flag("dry-run") {
        if is_multi_file && matches.get_flag("in-place") {
            return run_dry_run_preview(&config, matches, paths);
        }
        if !is_multi_file {
            // Stdin mode dry-run: show what would change
            let input: Box<dyn io::BufRead> =
                if let Some(input_file) = matches.get_one::<String>("input") {
                    Box::new(BufReader::new(File::open(input_file)?))
                } else {
                    Box::new(io::stdin().lock())
                };
            return run_stdin_dry_run_preview(&config, input, matches);
        }
        return validate_configuration(&config);
    }

    if is_multi_file {
        return run_multi_file_mode(&config, matches, paths);
    }

    // Single file/stdin mode
    let input: Box<dyn io::BufRead> = if let Some(input_file) = matches.get_one::<String>("input") {
        Box::new(BufReader::new(File::open(input_file)?))
    } else {
        Box::new(io::stdin().lock())
    };

    // Handle inspection mode
    if matches.get_flag("inspect") {
        return run_inspection_mode(&config, input, matches);
    }

    // Run standard processing mode
    run_processing_mode(&config, input, matches)
}

fn build_pipeline_settings(matches: &clap::ArgMatches) -> PipelineSettings {
    let context = matches
        .get_one::<String>("context")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let context_before = matches
        .get_one::<String>("context-before")
        .and_then(|s| s.parse().ok())
        .unwrap_or(context);

    let context_after = matches
        .get_one::<String>("context-after")
        .and_then(|s| s.parse().ok())
        .unwrap_or(context);

    let timeout_ms = matches.get_one::<u64>("timeout").copied().unwrap_or(0);

    // Parse max-line-length
    let max_line_length = matches
        .get_one::<String>("max-line-length")
        .and_then(|s| parse_size(s))
        .unwrap_or(0);

    // Parse max-line-action
    let max_line_action = matches
        .get_one::<String>("max-line-action")
        .map(|s| match s.to_lowercase().as_str() {
            "skip" => MaxLineAction::Skip,
            "error" => MaxLineAction::Error,
            "truncate" => MaxLineAction::Truncate,
            _ => MaxLineAction::Skip,
        })
        .unwrap_or_default();

    PipelineSettings {
        pcre_mode: matches.get_flag("pcre"),
        fixed_strings: matches.get_flag("fixed"),
        context_before,
        context_after,
        timeout_ms,
        // --no-shell disables shell transforms
        allow_shell: !matches.get_flag("no-shell"),
        // --strict enables ReDoS pattern rejection
        strict_mode: matches.get_flag("strict"),
        // --crlf preserves Windows line endings in in-place editing
        preserve_line_endings: matches.get_flag("crlf"),
        // --max-line-length limits line length
        max_line_length,
        max_line_action,
        // Use defaults for new configurable settings
        ..Default::default()
    }
}

/// Parse a size string like "1M", "512K", "1024"
fn parse_size(s: &str) -> Option<usize> {
    let s = s.trim().to_uppercase();
    if s.ends_with('K') {
        s[..s.len() - 1].parse::<usize>().ok().map(|n| n * 1024)
    } else if s.ends_with('M') {
        s[..s.len() - 1]
            .parse::<usize>()
            .ok()
            .map(|n| n * 1024 * 1024)
    } else if s.ends_with('G') {
        s[..s.len() - 1]
            .parse::<usize>()
            .ok()
            .map(|n| n * 1024 * 1024 * 1024)
    } else {
        s.parse().ok()
    }
}

fn export_configuration(config: &PipelineConfig, format: &str) -> Result<()> {
    let output = match format.to_lowercase().as_str() {
        "toml" => config.to_toml()?,
        "json" => config.to_json()?,
        _ => {
            return Err(anyhow!(
                "Unknown export format: {}. Use 'toml' or 'json'",
                format
            ));
        }
    };
    println!("{}", output);
    Ok(())
}

/// Build file processing options from command-line arguments.
///
/// Extracts configuration for file operations including:
/// - In-place editing settings
/// - Gitignore and hidden file handling
/// - Parallelization options
/// - Output modes (count, files-with-matches, etc.)
/// - Glob and exclude patterns
fn build_file_processing_options(matches: &clap::ArgMatches) -> Result<FileProcessingOptions> {
    // Parse binary mode
    let binary_mode = matches
        .get_one::<String>("binary")
        .map(|s| s.parse::<rexpipe::BinaryMode>())
        .transpose()
        .map_err(|e| anyhow!("Invalid binary mode: {}", e))?
        .unwrap_or_default();

    // Set up graceful shutdown handling
    let shutdown_signal = rexpipe::ShutdownSignal::new();
    if let Err(e) = shutdown_signal.install_handlers() {
        if !matches.get_flag("quiet") {
            eprintln!("Warning: Could not install signal handlers: {}", e);
        }
    }

    // Build base options
    let mut options = FileProcessingOptions::new()
        .in_place(matches.get_flag("in-place"))
        .backup_suffix(matches.get_one::<String>("backup").cloned())
        .respect_gitignore(!matches.get_flag("no-ignore"))
        .include_hidden(matches.get_flag("hidden"))
        .parallel(matches.get_flag("parallel"))
        .count_only(matches.get_flag("count"))
        .files_with_matches(matches.get_flag("files-with-matches"))
        .files_without_matches(matches.get_flag("files-without-matches"))
        .quiet(matches.get_flag("quiet"))
        .show_progress(matches.get_flag("progress"))
        .shutdown_signal(shutdown_signal)
        .binary_mode(binary_mode);

    // Add max depth
    if let Some(depth) = matches.get_one::<String>("max-depth") {
        options = options.max_depth(Some(depth.parse()?));
    }

    // Add glob patterns
    if let Some(globs) = matches.get_many::<String>("glob") {
        for glob in globs {
            options = options.include_pattern(glob.clone());
        }
    }

    // Add exclude patterns
    if let Some(excludes) = matches.get_many::<String>("exclude") {
        for exclude in excludes {
            options = options.exclude_pattern(exclude.clone());
        }
    }

    Ok(options)
}

/// Discover files and emit warnings if none are found.
///
/// Returns the discovered files, or Ok(empty) with warnings if nothing matches.
fn discover_files_with_warnings(
    processor: &MultiFileProcessor,
    paths: &[PathBuf],
    options: &FileProcessingOptions,
    quiet: bool,
) -> Result<Vec<PathBuf>> {
    let paths_to_process = if paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        paths.to_vec()
    };

    let files = processor.discover_files(&paths_to_process)?;

    info!("Discovered {} files to process", files.len());

    if files.is_empty() && !quiet {
        eprintln!("Warning: No files found matching criteria");
        if !options.include_patterns.is_empty() {
            eprintln!(
                "  Glob patterns specified: {}",
                options.include_patterns.join(", ")
            );
            eprintln!("  Hint: Check that your glob patterns match existing files");
        }
        if !options.exclude_patterns.is_empty() {
            eprintln!(
                "  Exclude patterns: {}",
                options.exclude_patterns.join(", ")
            );
        }
    }

    Ok(files)
}

fn run_multi_file_mode(
    config: &PipelineConfig,
    matches: &clap::ArgMatches,
    paths: Vec<PathBuf>,
) -> Result<()> {
    let quiet = matches.get_flag("quiet");
    let json_output = should_use_json(matches);
    let jsonl_output = matches.get_flag("jsonl");

    debug!(
        "Entering multi-file mode with {} paths",
        if paths.is_empty() { 1 } else { paths.len() }
    );

    // Build options and create processor
    let options = build_file_processing_options(matches)?;
    let processor = MultiFileProcessor::new(config.clone(), options.clone());

    // Discover files with warnings
    let files = discover_files_with_warnings(&processor, &paths, &options, quiet)?;
    if files.is_empty() {
        return Ok(());
    }

    // Handle checkpoint/resume functionality
    let checkpoint_path = matches.get_one::<String>("checkpoint");
    let resume_mode = matches.get_flag("resume");

    let mut checkpoint = if let Some(path) = checkpoint_path {
        let checkpoint_config = CheckpointConfig::new()
            .enabled(true)
            .with_checkpoint_file(path)
            .with_auto_save(true);

        if resume_mode {
            // Load existing checkpoint
            Checkpoint::load_or_create(&checkpoint_config)
                .map_err(|e| anyhow!("Failed to load checkpoint: {}", e))?
        } else {
            // Create new checkpoint (will overwrite existing)
            Checkpoint::new(checkpoint_config)
        }
    } else {
        // No checkpoint - create disabled one
        Checkpoint::new(CheckpointConfig::default())
    };

    // Filter files based on checkpoint if in resume mode
    let files_to_process: Vec<PathBuf> = if resume_mode && checkpoint.is_enabled() {
        let mut to_process = Vec::new();
        let mut skipped = 0;

        for file in &files {
            match checkpoint.needs_processing(file) {
                Ok(true) => to_process.push(file.clone()),
                Ok(false) => {
                    skipped += 1;
                    debug!("Skipping {} (unchanged since last checkpoint)", file.display());
                }
                Err(e) => {
                    debug!("Error checking checkpoint for {}: {}, will process", file.display(), e);
                    to_process.push(file.clone());
                }
            }
        }

        if skipped > 0 && !quiet {
            eprintln!("Checkpoint: skipping {} unchanged files, processing {}", skipped, to_process.len());
        }

        if to_process.is_empty() {
            if !quiet {
                eprintln!("Checkpoint: all files are up to date");
            }
            return Ok(());
        }

        to_process
    } else {
        files.clone()
    };

    // Handle cross-file consistency checking
    if let Some(cross_file_path) = matches.get_one::<String>("cross-file") {
        let cross_file_config = CrossFileConfig::load_rules_file(cross_file_path)
            .map_err(|e| anyhow!("Failed to load cross-file rules: {}", e))?;

        if !cross_file_config.rules.is_empty() {
            let mut manager = CrossFileManager::new();
            manager.add_rules(cross_file_config.rules.clone());

            // Load all files to process
            for file in &files_to_process {
                if let Err(e) = manager.load_file(file) {
                    if !quiet {
                        eprintln!("Warning: Could not load {} for cross-file check: {}", file.display(), e);
                    }
                }
            }

            // Scan for triggers and check rules
            manager.scan_triggers()
                .map_err(|e| anyhow!("Failed to scan triggers: {}", e))?;

            let results = manager.check_all()
                .map_err(|e| anyhow!("Failed to check cross-file rules: {}", e))?;

            // Report results
            let has_violations = results.iter().any(|r| !r.passed);

            if has_violations || !quiet {
                if json_output {
                    // Output as JSON
                    let json_results: Vec<serde_json::Value> = results.iter().map(|r| {
                        serde_json::json!({
                            "rule_name": r.rule_name,
                            "trigger_file": r.trigger_file.display().to_string(),
                            "passed": r.passed,
                            "violations": r.violations.iter().map(|v| {
                                serde_json::json!({
                                    "file": v.file.display().to_string(),
                                    "description": v.description,
                                    "expected_pattern": v.expected_pattern
                                })
                            }).collect::<Vec<_>>()
                        })
                    }).collect();
                    println!("{}", serde_json::to_string_pretty(&json_results)?);
                } else {
                    eprintln!("{}", format_check_report(&results));
                }
            }

            // Handle violations based on default action
            if has_violations {
                match cross_file_config.default_action {
                    ViolationAction::Fail => {
                        return Err(anyhow!("Cross-file consistency check failed"));
                    }
                    ViolationAction::Warn => {
                        // Already warned above, continue processing
                    }
                    ViolationAction::Skip => {
                        if !quiet {
                            eprintln!("Cross-file violations found, skipping processing");
                        }
                        return Ok(());
                    }
                    ViolationAction::Fix => {
                        // TODO: Implement auto-fix functionality
                        if !quiet {
                            eprintln!("Note: Auto-fix for cross-file violations not yet implemented");
                        }
                    }
                }
            }
        }
    }

    // Check if async mode is requested
    let use_async = matches.get_flag("async");

    // Warn if async flag is used without async feature
    #[cfg(not(feature = "async"))]
    if use_async {
        eprintln!("Warning: --async flag requires the 'async' feature.");
        eprintln!("Rebuild with: cargo build --features async");
        eprintln!("Falling back to synchronous processing.");
    }

    // Process based on mode
    let result = if options.files_with_matches {
        let matching = processor.files_with_matches(&files_to_process)?;
        // Update checkpoint for processed files
        if checkpoint.is_enabled() {
            for file in &matching {
                if let Ok(metadata) = std::fs::metadata(file) {
                    checkpoint.update_file_state(file, metadata.len(), 0, metadata.len());
                }
            }
            checkpoint.save().map_err(|e| anyhow!("Failed to save checkpoint: {}", e))?;
        }
        output_file_list(&matching, quiet, json_output, "files_with_matches")?;
        return Ok(());
    } else if options.files_without_matches {
        let non_matching = processor.files_without_matches(&files_to_process)?;
        // Update checkpoint for processed files
        if checkpoint.is_enabled() {
            for file in &files_to_process {
                if let Ok(metadata) = std::fs::metadata(file) {
                    checkpoint.update_file_state(file, metadata.len(), 0, metadata.len());
                }
            }
            checkpoint.save().map_err(|e| anyhow!("Failed to save checkpoint: {}", e))?;
        }
        output_file_list(&non_matching, quiet, json_output, "files_without_matches")?;
        return Ok(());
    } else if options.count_only {
        #[cfg(feature = "async")]
        if use_async {
            let rt = tokio::runtime::Runtime::new()?;
            let result = rt
                .block_on(
                    rexpipe::files::AsyncMultiFileProcessor::new(config.clone(), options.clone())
                        .count_matches_async(&files_to_process),
                )
                .map_err(|e| anyhow!(e))?;
            // Update checkpoint for processed files
            if checkpoint.is_enabled() {
                for file in &files_to_process {
                    if let Ok(metadata) = std::fs::metadata(file) {
                        checkpoint.update_file_state(file, metadata.len(), 0, metadata.len());
                    }
                }
                checkpoint.save().map_err(|e| anyhow!("Failed to save checkpoint: {}", e))?;
            }
            output_count_results(&result, quiet, json_output)?;
            return Ok(());
        }
        let result = processor.count_matches(&files_to_process)?;
        // Update checkpoint for processed files
        if checkpoint.is_enabled() {
            for file in &files_to_process {
                if let Ok(metadata) = std::fs::metadata(file) {
                    checkpoint.update_file_state(file, metadata.len(), 0, metadata.len());
                }
            }
            checkpoint.save().map_err(|e| anyhow!("Failed to save checkpoint: {}", e))?;
        }
        output_count_results(&result, quiet, json_output)?;
        return Ok(());
    } else if jsonl_output {
        // JSONL streaming mode: output each file result as it's processed
        let result = processor.process_files_streaming(&files_to_process, |file_result| {
            if let Ok(jsonl) = json_schema::output_file_result_jsonl(file_result) {
                println!("{}", jsonl);
            }
        })?;

        // Update checkpoint for processed files
        if checkpoint.is_enabled() {
            for file in &files_to_process {
                if let Ok(metadata) = std::fs::metadata(file) {
                    checkpoint.update_file_state(file, metadata.len(), 0, metadata.len());
                }
            }
            checkpoint.save().map_err(|e| anyhow!("Failed to save checkpoint: {}", e))?;
        }

        // Output summary as final JSONL line
        if let Ok(summary) = json_schema::output_streaming_summary_jsonl(&result) {
            println!("{}", summary);
        }

        if !result.has_matches() {
            std::process::exit(exit_codes::NO_MATCHES);
        }
        return Ok(());
    } else {
        #[cfg(feature = "async")]
        if use_async {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(
                rexpipe::files::AsyncMultiFileProcessor::new(config.clone(), options.clone())
                    .process_files_async(&files_to_process),
            )
            .map_err(|e| anyhow!(e))?
        } else {
            processor.process_files(&files_to_process)?
        }
        #[cfg(not(feature = "async"))]
        processor.process_files(&files_to_process)?
    };

    // Update checkpoint for processed files (main processing path)
    if checkpoint.is_enabled() {
        for file in &files_to_process {
            if let Ok(metadata) = std::fs::metadata(file) {
                checkpoint.update_file_state(file, metadata.len(), 0, metadata.len());
            }
        }
        checkpoint.save().map_err(|e| anyhow!("Failed to save checkpoint: {}", e))?;
    }

    // Output results
    if !quiet {
        if json_output {
            output_multi_file_json(&result)?;
        } else {
            output_multi_file_summary(&result)?;
        }
    }

    // Set exit code based on matches (exit 1 = no matches, for grep-like behavior)
    if !result.has_matches() {
        std::process::exit(exit_codes::NO_MATCHES);
    }

    Ok(())
}

fn run_dry_run_preview(
    config: &PipelineConfig,
    matches: &clap::ArgMatches,
    paths: Vec<PathBuf>,
) -> Result<()> {
    // Build options (same infrastructure as multi-file mode)
    let options = build_file_processing_options(matches)?;
    let processor = MultiFileProcessor::new(config.clone(), options.clone());

    // Discover files with warnings
    let files = discover_files_with_warnings(&processor, &paths, &options, false)?;
    if files.is_empty() {
        return Ok(());
    }

    // Determine if we should use color (respects --no-color and NO_COLOR env)
    let use_color = should_use_color(matches);

    // Generate preview
    let preview = processor.preview_changes(&files, use_color)?;
    print!("{}", preview);

    Ok(())
}

/// Show a preview of what would change when processing stdin.
/// Displays a diff-like view showing original lines vs transformed lines.
fn run_stdin_dry_run_preview(
    config: &PipelineConfig,
    input: Box<dyn io::BufRead>,
    matches: &clap::ArgMatches,
) -> Result<()> {
    use std::io::BufRead;

    let use_color = should_use_color(matches);
    let mut processor = StreamProcessor::new(config.clone())?;

    // ANSI color codes
    let (red, green, reset) = if use_color {
        ("\x1b[31m", "\x1b[32m", "\x1b[0m")
    } else {
        ("", "", "")
    };

    println!("Dry-run preview (no changes will be made):");
    println!("---");

    let mut total_lines = 0u64;
    let mut changed_lines = 0u64;

    for (line_num, line_result) in input.lines().enumerate() {
        let original = line_result?;
        total_lines += 1;

        // Process the line through the pipeline
        let mut output_buffer = Vec::new();
        {
            let single_line = std::io::Cursor::new(format!("{}\n", original));
            processor.process_stream(single_line, &mut output_buffer)?;
        }

        let transformed = String::from_utf8_lossy(&output_buffer)
            .trim_end_matches('\n')
            .to_string();

        if original != transformed {
            changed_lines += 1;
            println!("{}{}: - {}{}", red, line_num + 1, original, reset);
            println!("{}{}: + {}{}", green, line_num + 1, transformed, reset);
        }
    }

    println!("---");
    println!(
        "Summary: {} of {} lines would change",
        changed_lines, total_lines
    );

    Ok(())
}

fn output_file_list(files: &[PathBuf], quiet: bool, json: bool, mode: &str) -> Result<()> {
    if quiet {
        return Ok(());
    }

    if json {
        println!("{}", json_schema::output_file_list_json(files, mode)?);
    } else {
        for file in files {
            println!("{}", file.display());
        }
    }
    Ok(())
}

fn output_count_results(result: &MultiFileResult, quiet: bool, json: bool) -> Result<()> {
    if quiet {
        return Ok(());
    }

    if json {
        println!("{}", json_schema::output_multi_file_json(result)?);
    } else {
        for file_result in &result.file_results {
            println!(
                "{}:{}",
                file_result.path.display(),
                file_result.matches_found
            );
        }
        println!("---");
        println!(
            "Total: {} matches in {} files",
            result.total_matches, result.files_matched
        );
    }
    Ok(())
}

fn output_multi_file_json(result: &MultiFileResult) -> Result<()> {
    println!("{}", json_schema::output_multi_file_json(result)?);
    Ok(())
}

fn output_multi_file_summary(result: &MultiFileResult) -> Result<()> {
    println!("{}", result.summary());
    if !result.errors.is_empty() {
        eprintln!("\nErrors:");
        for error in &result.errors {
            eprintln!("  {}", error);
        }
    }
    Ok(())
}

fn load_pipeline_config(
    matches: &clap::ArgMatches,
    settings: PipelineSettings,
) -> Result<PipelineConfig> {
    debug!("Loading pipeline configuration");

    if let Some(config_file) = matches.get_one::<String>("config") {
        info!("Loading config from file: {}", config_file);
        let config_path = Path::new(config_file);
        let mut config = PipelineConfig::from_file(config_file)?;

        // Load and resolve pattern libraries if specified
        if config.uses_pattern_libraries() {
            let mut resolver = LibraryResolver::new(config_path.parent());
            let library = resolver.load_libraries(&config.patterns_include)?;

            if let Err(errors) = config.resolve_pattern_references(&library) {
                return Err(anyhow!(
                    "Failed to resolve pattern references:\n  {}",
                    errors.join("\n  ")
                ));
            }
        }

        // Merge CLI settings with config file settings (CLI takes precedence)
        if settings.pcre_mode {
            config.settings.pcre_mode = true;
        }
        if settings.fixed_strings {
            config.settings.fixed_strings = true;
        }
        if settings.context_before > 0 {
            config.settings.context_before = settings.context_before;
        }
        if settings.context_after > 0 {
            config.settings.context_after = settings.context_after;
        }
        if settings.timeout_ms > 0 {
            config.settings.timeout_ms = settings.timeout_ms;
        }
        // --no-shell flag disables shell transforms
        if !settings.allow_shell {
            config.settings.allow_shell = false;
        }

        // --strict flag enables ReDoS pattern rejection
        if settings.strict_mode {
            config.settings.strict_mode = true;
        }

        // Validate shell transform usage if disabled
        if !config.settings.allow_shell && config.has_shell_transforms() {
            let shell_commands = config.get_shell_commands();
            return Err(anyhow!(
                "Shell transforms are disabled (--no-shell flag), but config contains shell transforms:\n  {}",
                shell_commands.join("\n  ")
            ));
        }

        // Warn about shell transforms when not disabled
        if config.has_shell_transforms() {
            let shell_commands = config.get_shell_commands();
            eprintln!(
                "Warning: This pipeline uses shell transforms that will execute external commands:\n  {}\n\
                Use --no-shell to disable shell command execution.",
                shell_commands.join("\n  ")
            );
        }

        // Handle bidirectional mode flags
        if matches.get_flag("reverse") {
            config.bidirectional.enabled = true;
            config.bidirectional.direction = rexpipe::bidirectional::Direction::Reverse;
        }
        if let Some(mapping_file) = matches.get_one::<String>("mapping-file") {
            config.bidirectional.enabled = true;
            config.bidirectional.mapping_file = Some(std::path::PathBuf::from(mapping_file));
        }

        Ok(config)
    } else if let Some(pattern) = matches.get_one::<String>("pattern") {
        debug!("Using inline pattern: {}", pattern);
        let replacement = matches.get_one::<String>("replacement").map(|s| s.to_string());
        let is_extract = matches.get_flag("extract");
        let transform_name = matches.get_one::<String>("transform").cloned();
        let scope = matches.get_one::<String>("scope").cloned();
        let language = matches.get_one::<String>("language").cloned();

        // Determine step type and configuration
        let (step_type, action, transform) = if is_extract {
            (StepType::Extract, None, None)
        } else if let Some(ref name) = transform_name {
            // Map transform name to TransformAction
            let transform_action = match name.as_str() {
                // Case transforms
                "uppercase" => Some(TransformAction::Uppercase),
                "lowercase" => Some(TransformAction::Lowercase),
                "title_case" => Some(TransformAction::TitleCase),
                // String manipulation
                "reverse" => Some(TransformAction::Reverse),
                "trim" => Some(TransformAction::Trim),
                "remove_whitespace" => Some(TransformAction::RemoveWhitespace),
                "normalize_whitespace" => Some(TransformAction::NormalizeWhitespace),
                "deduplicate" => Some(TransformAction::Deduplicate),
                "sort_chars" => Some(TransformAction::SortChars),
                "char_count" => Some(TransformAction::CharCount),
                "word_count" => Some(TransformAction::WordCount),
                // Encoding transforms
                "base64_encode" => Some(TransformAction::Base64Encode),
                "base64_decode" => Some(TransformAction::Base64Decode),
                "url_encode" => Some(TransformAction::UrlEncode),
                "url_decode" => Some(TransformAction::UrlDecode),
                // For plugin transforms (snake_case, camel_case, kebab_case, pascal_case, etc.)
                _ => Some(TransformAction::Plugin { name: name.clone(), args: vec![] }),
            };
            (StepType::Transform, None, transform_action)
        } else if replacement.is_some() {
            (StepType::Substitute, None, None)
        } else {
            (StepType::Filter, Some(rexpipe::pipeline::StepAction::KeepMatch), None)
        };

        let step = PipelineStep {
            step_type,
            pattern: pattern.to_string(),
            replacement,
            action,
            transform,
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
            start_pattern: None,
            end_pattern: None,
            block_context: None,
            on_mismatch: None,
            language,
            languages: None,
            scope,
            exclude_scopes: None,
            capture_names: None,
            output_format: None,
            output_template: None,
            first_only: None,
            deduplicate: None,
        };

        // Build bidirectional config from CLI flags
        let mut bidirectional = rexpipe::bidirectional::BidirectionalConfig::default();
        if matches.get_flag("reverse") {
            bidirectional.enabled = true;
            bidirectional.direction = rexpipe::bidirectional::Direction::Reverse;
        }
        if let Some(mapping_file) = matches.get_one::<String>("mapping-file") {
            bidirectional.enabled = true;
            bidirectional.mapping_file = Some(std::path::PathBuf::from(mapping_file));
        }

        Ok(PipelineConfig {
            name: Some("Inline Pipeline".to_string()),
            description: Some("Generated from command line pattern".to_string()),
            version: Some("1.0.0".to_string()),
            patterns_include: Vec::new(),
            settings,
            step: vec![step],
            bidirectional,
            checkpoint: Default::default(),
            cross_file: Default::default(),
            tests: Vec::new(),
        })
    } else {
        Err(anyhow!(
            "Missing required input.\n\n\
             You must specify either:\n  \
             - A config file:  rexpipe --config pipeline.toml < input.txt\n  \
             - An inline pattern:  rexpipe --pattern '\\d+' < input.txt\n\n\
             Examples:\n  \
             rexpipe -p 'ERROR' < log.txt              # Match lines with ERROR\n  \
             rexpipe -p '\\d+' -r 'NUM' < data.txt     # Replace numbers with NUM\n  \
             rexpipe -c config.toml --inspect < test   # Preview matches\n\n\
             Run 'rexpipe --help' for full usage information."
        ))
    }
}

fn validate_configuration(config: &PipelineConfig) -> Result<()> {
    match config.validate() {
        Ok(()) => {
            println!("✓ Configuration is valid");
            println!("{}", config.summary());

            // Test compilation
            match StreamProcessor::new(config.clone()) {
                Ok(_) => {
                    println!("✓ All patterns compile successfully");
                    Ok(())
                }
                Err(e) => {
                    println!("✗ Pattern compilation failed:");
                    println!("{}", e);
                    Err(e)
                }
            }
        }
        Err(errors) => {
            println!("✗ Configuration validation failed:\n");
            for (i, error) in errors.iter().enumerate() {
                println!("  {}. {}", i + 1, error);
            }
            println!("\nSuggestions:");
            println!("  - Check that all substitute steps have 'replacement' defined");
            println!(
                "  - Check that all filter steps have 'action' defined (keep_line, drop_line, etc.)"
            );
            println!("  - Verify all patterns are valid regex syntax");
            println!("  - Use 'rexpipe --inspect' to test patterns interactively");
            Err(anyhow!("Configuration is invalid"))
        }
    }
}

/// Explain what a pipeline will do without processing data.
/// Outputs human-readable or JSON description of each step.
fn explain_pipeline(config: &PipelineConfig, matches: &clap::ArgMatches) -> Result<()> {
    use rexpipe::pipeline::{StepType, StepAction, TransformAction};

    let json_output = should_use_json(matches);

    if json_output {
        // JSON output for AI consumption
        #[derive(serde::Serialize)]
        struct StepExplanation {
            step_number: usize,
            step_type: String,
            pattern: String,
            description: String,
            effect: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            replacement: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            action: Option<String>,
        }

        #[derive(serde::Serialize)]
        struct PipelineExplanation {
            name: Option<String>,
            step_count: usize,
            steps: Vec<StepExplanation>,
            summary: String,
        }

        let steps: Vec<StepExplanation> = config.step.iter().enumerate().map(|(i, step)| {
            let (effect, action_str) = match step.step_type {
                StepType::Substitute => {
                    let repl = step.replacement.clone().unwrap_or_else(|| "(none)".to_string());
                    (format!("Replace matches with '{}'", repl), None)
                }
                StepType::Filter => {
                    let action_desc = step.action.as_ref().map(|a| match a {
                        StepAction::KeepLine => "Keep only matching lines".to_string(),
                        StepAction::DropLine => "Remove matching lines".to_string(),
                        StepAction::KeepMatch => "Keep only the match text".to_string(),
                        StepAction::DropMatch => "Remove the match from line".to_string(),
                        StepAction::DeduplicateByPrefix => {
                            "Deduplicate by prefix from capture group".to_string()
                        }
                        _ => "Filter action".to_string(),
                    }).unwrap_or_else(|| "Filter lines".to_string());
                    (action_desc, step.action.as_ref().map(|a| format!("{:?}", a).to_lowercase()))
                }
                StepType::Extract => ("Extract matching text".to_string(), None),
                StepType::Validate => ("Validate lines match pattern".to_string(), None),
                StepType::Transform => {
                    let t = step.transform.as_ref().map(|t| match t {
                        TransformAction::Uppercase => "Convert to uppercase",
                        TransformAction::Lowercase => "Convert to lowercase",
                        TransformAction::TitleCase => "Convert to title case",
                        TransformAction::Trim => "Trim whitespace",
                        TransformAction::Reverse => "Reverse text",
                        TransformAction::Base64Encode => "Encode to base64",
                        TransformAction::Base64Decode => "Decode from base64",
                        TransformAction::UrlEncode => "URL encode",
                        TransformAction::UrlDecode => "URL decode",
                        TransformAction::Prepend => "Prepend text",
                        TransformAction::Append => "Append text",
                        TransformAction::RemoveWhitespace => "Remove whitespace",
                        TransformAction::NormalizeWhitespace => "Normalize whitespace",
                        TransformAction::Deduplicate => "Remove duplicates",
                        TransformAction::SortChars => "Sort characters",
                        TransformAction::CharCount => "Count characters",
                        TransformAction::WordCount => "Count words",
                        TransformAction::Shell { .. } => "Run shell command",
                        TransformAction::Plugin { .. } => "Run plugin",
                        _ => "Custom transformation",
                    }).unwrap_or("Transform matches");
                    (t.to_string(), None)
                }
                StepType::Block => ("Process within block boundaries".to_string(), None),
            };

            StepExplanation {
                step_number: i + 1,
                step_type: format!("{:?}", step.step_type).to_lowercase(),
                pattern: step.pattern.clone(),
                description: step.description.clone().unwrap_or_default(),
                effect,
                replacement: step.replacement.clone(),
                action: action_str,
            }
        }).collect();

        let summary = if steps.is_empty() {
            "Empty pipeline (passthrough)".to_string()
        } else {
            format!("Pipeline with {} step(s)", steps.len())
        };

        let explanation = PipelineExplanation {
            name: config.name.clone(),
            step_count: steps.len(),
            steps,
            summary,
        };

        let response = json_schema::JsonResponse::new("explain", explanation);
        println!("{}", response.to_json()?);
    } else {
        // Human-readable output
        println!("Pipeline Explanation");
        println!("====================\n");

        if let Some(name) = &config.name {
            println!("Name: {}\n", name);
        }

        if config.step.is_empty() {
            println!("Empty pipeline - input passes through unchanged.");
            return Ok(());
        }

        println!("Steps ({}):\n", config.step.len());

        for (i, step) in config.step.iter().enumerate() {
            println!("  Step {}: {:?}", i + 1, step.step_type);
            println!("    Pattern: '{}'", step.pattern);

            if let Some(desc) = &step.description {
                println!("    Description: {}", desc);
            }

            match step.step_type {
                StepType::Substitute => {
                    if let Some(repl) = &step.replacement {
                        println!("    Replaces with: '{}'", repl);
                    }
                }
                StepType::Filter => {
                    if let Some(action) = &step.action {
                        let action_desc = match action {
                            StepAction::KeepLine => "Keep only lines matching pattern".to_string(),
                            StepAction::DropLine => "Remove lines matching pattern".to_string(),
                            StepAction::KeepMatch => "Keep only matched text".to_string(),
                            StepAction::DropMatch => "Remove matched text from line".to_string(),
                            StepAction::DeduplicateByPrefix => {
                                "Deduplicate by prefix from capture group".to_string()
                            }
                            _ => "Action".to_string(),
                        };
                        println!("    Action: {}", action_desc);
                    }
                }
                StepType::Transform => {
                    if let Some(transform) = &step.transform {
                        let transform_desc: String = match transform {
                            TransformAction::Uppercase => "Convert to uppercase".to_string(),
                            TransformAction::Lowercase => "Convert to lowercase".to_string(),
                            TransformAction::TitleCase => "Convert to title case".to_string(),
                            TransformAction::Trim => "Trim whitespace".to_string(),
                            TransformAction::Reverse => "Reverse text".to_string(),
                            TransformAction::Base64Encode => "Encode to base64".to_string(),
                            TransformAction::Base64Decode => "Decode from base64".to_string(),
                            TransformAction::UrlEncode => "URL encode".to_string(),
                            TransformAction::UrlDecode => "URL decode".to_string(),
                            TransformAction::Prepend => "Prepend text".to_string(),
                            TransformAction::Append => "Append text".to_string(),
                            TransformAction::RemoveWhitespace => "Remove whitespace".to_string(),
                            TransformAction::NormalizeWhitespace => "Normalize whitespace".to_string(),
                            TransformAction::Deduplicate => "Remove duplicates".to_string(),
                            TransformAction::SortChars => "Sort characters".to_string(),
                            TransformAction::CharCount => "Count characters".to_string(),
                            TransformAction::WordCount => "Count words".to_string(),
                            TransformAction::Shell { command } => format!("Execute: {}", command),
                            TransformAction::Plugin { name, .. } => format!("Plugin: {}", name),
                            _ => "Custom transformation".to_string(),
                        };
                        println!("    Transform: {}", transform_desc);
                    }
                }
                _ => {}
            }
            println!();
        }

        println!("Summary: Pipeline processes input through {} step(s).", config.step.len());
    }

    Ok(())
}

fn list_library_patterns(library_path: &str) -> Result<()> {
    let path = Path::new(library_path);
    let patterns = library::list_patterns(path)?;

    if patterns.is_empty() {
        println!("No patterns found in library '{}'", library_path);
        return Ok(());
    }

    println!(
        "Patterns in '{}' ({} total):\n",
        library_path,
        patterns.len()
    );

    // Group by category (prefix before last dot)
    let mut current_category = String::new();
    for (name, pattern) in &patterns {
        let category = if let Some(pos) = name.rfind('.') {
            &name[..pos]
        } else {
            ""
        };

        if category != current_category {
            if !current_category.is_empty() {
                println!();
            }
            if !category.is_empty() {
                println!("[{}]", category);
            }
            current_category = category.to_string();
        }

        // Truncate long patterns for display
        let display_pattern = if pattern.len() > 60 {
            format!("{}...", &pattern[..57])
        } else {
            pattern.clone()
        };

        println!("  {} = '{}'", name, display_pattern);
    }

    Ok(())
}

fn validate_library_file(library_path: &str) -> Result<()> {
    let path = Path::new(library_path);

    match library::LibraryResolver::validate_library(path) {
        Ok(lib) => {
            println!("✓ Library '{}' is valid", library_path);
            if let Some(name) = &lib.name {
                println!("  Name: {}", name);
            }
            if let Some(version) = &lib.version {
                println!("  Version: {}", version);
            }
            if let Some(desc) = &lib.description {
                println!("  Description: {}", desc);
            }

            // Count patterns
            let patterns = library::list_patterns(path)?;
            println!("  Patterns: {}", patterns.len());

            if !lib.patterns_include.is_empty() {
                println!("  Includes: {}", lib.patterns_include.join(", "));
            }

            Ok(())
        }
        Err(e) => {
            println!("✗ Library '{}' is invalid:", library_path);
            println!("  {}", e);
            Err(e)
        }
    }
}

fn run_inspection_mode(
    config: &PipelineConfig,
    input: Box<dyn io::BufRead>,
    matches: &clap::ArgMatches,
) -> Result<()> {
    let use_color = should_use_color(matches);

    let options = InspectorOptions::new()
        .interactive(matches.get_flag("interactive"))
        .show_performance(matches.get_flag("performance"))
        .show_line_numbers(true)
        .show_captures(true);

    let mut inspector = Inspector::new(config.clone())?
        .with_options(options)
        .with_color(use_color);
    let result = inspector.inspect_stream(input)?;
    inspector.display_results(&result)?;

    Ok(())
}

fn run_processing_mode(
    config: &PipelineConfig,
    input: Box<dyn io::BufRead>,
    matches: &clap::ArgMatches,
) -> Result<()> {
    let quiet = matches.get_flag("quiet");
    let json_output = should_use_json(matches);
    let count_only = matches.get_flag("count");

    let mut processor = StreamProcessor::new(config.clone())?;

    if quiet {
        // Quiet mode: process but don't output anything
        let mut output = std::io::sink();
        let result = processor.process_stream(input, &mut output)?;
        if result.matches_found == 0 {
            std::process::exit(exit_codes::NO_MATCHES);
        }
        return Ok(());
    }

    if count_only {
        // Count mode: just count matches
        let mut output = std::io::sink();
        let result = processor.process_stream(input, &mut output)?;

        if json_output {
            println!("{}", json_schema::output_count_json(&result)?);
        } else {
            println!("{}", result.matches_found);
        }
        return Ok(());
    }

    let output: Box<dyn io::Write> = if let Some(output_file) = matches.get_one::<String>("output")
    {
        Box::new(File::create(output_file)?)
    } else {
        Box::new(io::stdout())
    };

    let result = processor.process_stream(input, output)?;

    if matches.get_flag("performance") {
        eprintln!("{}", result.performance_summary());
        eprintln!("{}", processor.performance_report());
    }

    if json_output && matches.get_flag("performance") {
        // If both json and performance are requested, output performance as JSON
        eprintln!("{}", json_schema::output_performance_json(&result)?);
    }

    // Output verification summary if requested
    if matches.get_flag("verify") {
        output_verification_summary(&result, json_output)?;
    }

    Ok(())
}

/// Output a verification summary confirming what transformations were applied
fn output_verification_summary(
    result: &rexpipe::pipeline::PipelineResult,
    json_output: bool,
) -> Result<()> {
    if json_output {
        #[derive(serde::Serialize)]
        struct VerificationResult {
            status: String,
            lines_processed: u64,
            matches_found: u64,
            transformations_applied: u64,
            success_rate: f64,
            verified: bool,
        }

        let verified = result.lines_processed > 0;
        let status = if result.transformations_applied > 0 {
            "transformations_applied"
        } else if result.matches_found > 0 {
            "matches_found_no_transformations"
        } else {
            "no_matches"
        };

        let verification = VerificationResult {
            status: status.to_string(),
            lines_processed: result.lines_processed,
            matches_found: result.matches_found,
            transformations_applied: result.transformations_applied,
            success_rate: result.success_rate(),
            verified,
        };

        let response = json_schema::JsonResponse::new("verify", verification);
        eprintln!("{}", response.to_json()?);
    } else {
        eprintln!("\n--- Verification Summary ---");
        eprintln!("Lines processed: {}", result.lines_processed);
        eprintln!("Matches found: {}", result.matches_found);
        eprintln!("Transformations applied: {}", result.transformations_applied);
        eprintln!("Success rate: {:.1}%", result.success_rate() * 100.0);

        if result.transformations_applied > 0 {
            eprintln!("Status: ✓ Transformations applied successfully");
        } else if result.matches_found > 0 {
            eprintln!("Status: ⚠ Matches found but no transformations (filter-only mode?)");
        } else {
            eprintln!("Status: ⚠ No matches found");
        }
    }

    Ok(())
}

/// Print git filter setup instructions
fn print_git_filter_setup(filter_name: &str, matches: &clap::ArgMatches) -> Result<()> {
    let config_path = matches.get_one::<String>("config");

    println!("# Git Filter Setup: {}", filter_name);
    println!("#");
    println!("# This configures rexpipe as a git clean/smudge filter.");
    println!("# - 'clean' runs on commit (working directory → repository)");
    println!("# - 'smudge' runs on checkout (repository → working directory)");
    println!();

    // Generate the command
    let rexpipe_cmd = if let Some(cfg) = config_path {
        format!("rexpipe -c {}", cfg)
    } else {
        println!("# NOTE: No config file specified. Add -c <config.toml> for transformation rules.");
        "rexpipe -c .rexpipe/filter.toml".to_string()
    };

    println!("# Step 1: Add to your git config (run in repository root):");
    println!();
    println!("git config filter.{}.clean '{}'", filter_name, rexpipe_cmd);
    println!("git config filter.{}.smudge 'cat'", filter_name);
    println!("git config filter.{}.required true", filter_name);
    println!();

    println!("# Step 2: Add to .gitattributes (patterns to filter):");
    println!();
    println!("# Example: sanitize all log files");
    println!("*.log filter={}", filter_name);
    println!("# Example: sanitize environment files");
    println!("*.env filter={}", filter_name);
    println!("# Example: sanitize specific config files");
    println!("config/*.json filter={}", filter_name);
    println!();

    println!("# Step 3: Create pipeline config (.rexpipe/filter.toml):");
    println!();
    println!("# Example sanitization pipeline:");
    println!(r#"[[step]]"#);
    println!(r#"type = "substitute""#);
    println!(r#"pattern = 'password\s*=\s*"[^"]*"'"#);
    println!(r#"replacement = 'password = "***REDACTED***"'"#);
    println!();
    println!(r#"[[step]]"#);
    println!(r#"type = "substitute""#);
    println!(r#"pattern = 'api_key\s*=\s*"[^"]*"'"#);
    println!(r#"replacement = 'api_key = "***REDACTED***"'"#);
    println!();

    println!("# For bidirectional filters (reversible), consider using deterministic masking");
    println!("# (feature in development).");
    println!();

    println!("# Global setup (applies to all repositories):");
    println!("# git config --global filter.{}.clean '{}'", filter_name, rexpipe_cmd);
    println!("# git config --global filter.{}.smudge 'cat'", filter_name);

    Ok(())
}

/// Run pattern discovery/learning mode
fn run_pattern_discovery(matches: &clap::ArgMatches) -> Result<()> {
    use std::collections::HashMap;
    use regex::Regex;
    use std::io::BufRead;

    // Common pattern templates to search for
    let pattern_templates: Vec<(&str, &str, Regex)> = vec![
        ("email", r"Email addresses", Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap()),
        ("ipv4", r"IPv4 addresses", Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap()),
        ("phone_us", r"US phone numbers", Regex::new(r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b").unwrap()),
        ("date_iso", r"ISO dates (YYYY-MM-DD)", Regex::new(r"\b\d{4}-\d{2}-\d{2}\b").unwrap()),
        ("date_us", r"US dates (MM/DD/YYYY)", Regex::new(r"\b\d{1,2}/\d{1,2}/\d{4}\b").unwrap()),
        ("time_24h", r"24-hour time", Regex::new(r"\b\d{1,2}:\d{2}(:\d{2})?\b").unwrap()),
        ("uuid", r"UUIDs", Regex::new(r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b").unwrap()),
        ("hex_id", r"Hex identifiers (8+ chars)", Regex::new(r"\b[0-9a-fA-F]{8,}\b").unwrap()),
        ("url", r"URLs", Regex::new(r#"https?://[^\s<>"']+"#).unwrap()),
        ("ssn", r"SSN-like patterns", Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap()),
        ("credit_card", r"Credit card patterns", Regex::new(r"\b\d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}\b").unwrap()),
        ("api_key", r"API key patterns", Regex::new(r"\b[A-Za-z0-9_-]{20,}\b").unwrap()),
        ("base64_blob", r"Base64 blobs (20+ chars)", Regex::new(r"\b[A-Za-z0-9+/]{20,}={0,2}\b").unwrap()),
    ];

    // Count matches
    let mut pattern_counts: HashMap<&str, (u64, Vec<String>)> = HashMap::new();
    for (name, _, _) in &pattern_templates {
        pattern_counts.insert(name, (0, Vec::new()));
    }

    let mut total_lines = 0u64;

    // Helper to process a line
    fn process_line<'a>(
        line: &str,
        pattern_templates: &[(&'a str, &str, Regex)],
        pattern_counts: &mut HashMap<&'a str, (u64, Vec<String>)>,
    ) {
        for (name, _, regex) in pattern_templates {
            for cap in regex.find_iter(line) {
                let entry = pattern_counts.get_mut(name).unwrap();
                entry.0 += 1;
                // Store up to 3 examples
                if entry.1.len() < 3 && !entry.1.contains(&cap.as_str().to_string()) {
                    entry.1.push(cap.as_str().to_string());
                }
            }
        }
    }

    // Read input
    if let Some(input_file) = matches.get_one::<String>("input") {
        let reader = BufReader::new(File::open(input_file)?);
        for line_result in reader.lines() {
            let line = line_result?;
            total_lines += 1;
            process_line(&line, &pattern_templates, &mut pattern_counts);
        }
    } else if !io::stdin().is_terminal() {
        let reader = BufReader::new(io::stdin());
        for line_result in reader.lines() {
            let line = line_result?;
            total_lines += 1;
            process_line(&line, &pattern_templates, &mut pattern_counts);
        }
    } else {
        return Err(anyhow!("No input provided. Pipe data to stdin or use -f <file>"));
    }

    // Report findings
    println!("Pattern Discovery Report");
    println!("========================");
    println!("Analyzed {} lines\n", total_lines);

    // Sort by count descending
    let mut findings: Vec<_> = pattern_templates
        .iter()
        .filter_map(|(name, desc, regex)| {
            let (count, examples) = pattern_counts.get(name).unwrap();
            if *count > 0 {
                Some((name, desc, regex.as_str(), *count, examples.clone()))
            } else {
                None
            }
        })
        .collect();

    findings.sort_by(|a, b| b.3.cmp(&a.3));

    if findings.is_empty() {
        println!("No common patterns detected.");
    } else {
        println!("Detected Patterns:");
        println!();
        for (name, desc, pattern, count, examples) in &findings {
            println!("  {} ({} matches)", name, count);
            println!("    Description: {}", desc);
            println!("    Pattern: {}", pattern);
            if !examples.is_empty() {
                println!("    Examples: {}", examples.join(", "));
            }
            println!();
        }

        // Generate suggested config
        println!("Suggested Pipeline Config:");
        println!("--------------------------");
        for (name, _desc, pattern, count, _examples) in &findings {
            if *count >= 5 {
                println!();
                println!("[[step]]");
                println!("# {} occurrences", count);
                println!("type = \"substitute\"");
                println!("pattern = '{}'", pattern.replace('\'', "\\'"));
                println!("replacement = '[{}]'", name.to_uppercase());
            }
        }
    }

    Ok(())
}

/// Run continuous streaming mode with URI-based sources and sinks.
fn run_streaming_mode(matches: &clap::ArgMatches) -> Result<()> {
    use rexpipe::stream::{StreamUri, create_source, create_sink};
    use rexpipe::processor::StreamProcessor;

    // Parse input URI (default to stdin if not specified)
    let input_uri_str = matches
        .get_one::<String>("input-uri")
        .map(|s| s.as_str())
        .unwrap_or("stdin://");
    let input_uri = StreamUri::parse(input_uri_str)
        .map_err(|e| anyhow!("Invalid input URI: {}", e))?;

    // Parse output URI (default to stdout if not specified)
    let output_uri_str = matches
        .get_one::<String>("output-uri")
        .map(|s| s.as_str())
        .unwrap_or("stdout://");
    let output_uri = StreamUri::parse(output_uri_str)
        .map_err(|e| anyhow!("Invalid output URI: {}", e))?;

    // Load pipeline configuration
    let settings = build_pipeline_settings(matches);
    let config = load_pipeline_config(matches, settings)?;

    // Create processor
    let mut processor = StreamProcessor::new(config)?;

    // Create source and sink
    let mut source = create_source(&input_uri)
        .map_err(|e| anyhow!("Failed to create input source: {}", e))?;
    let mut sink = create_sink(&output_uri)
        .map_err(|e| anyhow!("Failed to create output sink: {}", e))?;

    info!("Streaming: {} -> pipeline -> {}", input_uri_str, output_uri_str);
    if !matches.get_flag("quiet") {
        eprintln!("Streaming from {} to {}", input_uri_str, output_uri_str);
        eprintln!("Press Ctrl+C to stop...");
    }

    // Process lines continuously
    loop {
        match source.read_line() {
            Ok(Some(line)) => {
                // Process the line through the pipeline
                let input = format!("{}\n", line);
                let mut output = Vec::new();

                match processor.process_stream(std::io::Cursor::new(input.as_bytes()), &mut output) {
                    Ok(_) => {
                        // Write output (remove trailing newline since sink adds one)
                        let output_str = String::from_utf8_lossy(&output);
                        let output_trimmed = output_str.trim_end_matches('\n');
                        if !output_trimmed.is_empty() {
                            sink.write_line(output_trimmed)?;
                            sink.flush()?;
                        }
                    }
                    Err(e) => {
                        log::error!("Processing error: {}", e);
                    }
                }
            }
            Ok(None) => {
                // Source exhausted (EOF for files, but TCP/UDP continue)
                if input_uri.scheme == "stdin" || input_uri.scheme == "file" {
                    break;
                }
                // For network sources, this shouldn't happen
            }
            Err(e) => {
                log::error!("Read error: {}", e);
                // For transient errors, we might want to continue
                // For fatal errors, we should break
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Run pattern learning mode to infer regex patterns from examples.
fn run_pattern_learning(matches: &clap::ArgMatches) -> Result<()> {
    use rexpipe::learn::PatternLearner;

    let mut learner = PatternLearner::new();

    // Add positive examples
    if let Some(positives) = matches.get_many::<String>("positive") {
        for example in positives {
            learner.add_positive(example);
        }
    }

    // Add negative examples
    if let Some(negatives) = matches.get_many::<String>("negative") {
        for example in negatives {
            learner.add_negative(example);
        }
    }

    // If no examples provided via flags, try to read from stdin
    if learner.example_count() == 0 {
        // Only show instructions if stdin is interactive (TTY)
        if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
            eprintln!("Reading examples from stdin (prefix with + for positive, - for negative):");
            eprintln!("Example: +user@example.com");
            eprintln!("Example: -not-an-email");
            eprintln!("Press Ctrl+D when done.");
        }

        let stdin = io::stdin();
        for line in stdin.lines() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(example) = line.strip_prefix('+') {
                learner.add_positive(example.trim());
            } else if let Some(example) = line.strip_prefix('-') {
                learner.add_negative(example.trim());
            } else {
                // Default to positive
                learner.add_positive(line);
            }
        }
    }

    if learner.example_count() == 0 {
        return Err(anyhow!("No examples provided. Use --positive and --negative flags or provide examples via stdin."));
    }

    // Learn patterns
    match learner.learn() {
        Ok(patterns) => {
            if patterns.is_empty() {
                println!("No patterns could be learned from the provided examples.");
            } else {
                println!("Learned patterns:\n");
                for (i, pattern) in patterns.iter().enumerate() {
                    println!("{}. Pattern: {}", i + 1, pattern.pattern);
                    println!("   Confidence: {}%", pattern.confidence);
                    if !pattern.description.is_empty() {
                        println!("   Description: {}", pattern.description);
                    }
                    println!();
                }

                // Generate pipeline config suggestion
                if let Some(best) = patterns.first() {
                    println!("Suggested pipeline configuration:");
                    println!("[[step]]");
                    println!("type = \"substitute\"");
                    println!("pattern = \"{}\"", best.pattern.replace('\\', "\\\\").replace('"', "\\\""));
                    println!("replacement = \"[REDACTED]\"");
                }
            }
        }
        Err(e) => {
            return Err(anyhow!("Pattern learning failed: {}", e));
        }
    }

    Ok(())
}

/// Run pipeline tests defined in configuration.
fn run_pipeline_tests(config: &PipelineConfig, matches: &clap::ArgMatches) -> Result<()> {
    use rexpipe::testing::{TestConfig, TestRunner};
    use std::io::Cursor;

    if config.tests.is_empty() {
        return Err(anyhow!("No tests defined in pipeline configuration. Add [[tests]] sections to define test cases."));
    }

    // Create test runner
    let test_config = TestConfig::new();
    let mut runner = TestRunner::new(test_config);
    runner.add_tests(config.tests.clone());

    // Create processor function that uses the pipeline
    let pipeline_config = config.clone();
    let processor = move |input: &str| -> std::result::Result<(String, u64, u64), String> {
        let mut processor = match StreamProcessor::new(pipeline_config.clone()) {
            Ok(p) => p,
            Err(e) => return Err(format!("Failed to create processor: {}", e)),
        };

        let reader = Cursor::new(input);
        let mut output = Vec::new();

        match processor.process_stream(reader, &mut output) {
            Ok(result) => {
                let output_str = String::from_utf8_lossy(&output).to_string();
                Ok((output_str, result.matches_found, result.transformations_applied))
            }
            Err(e) => Err(format!("Processing error: {}", e)),
        }
    };

    let summary = runner.run_all(processor);

    // Get output format
    let format = matches.get_one::<String>("test-format").map(|s| s.as_str()).unwrap_or("text");

    match format {
        "tap" => {
            println!("{}", rexpipe::testing::format_tap_output(&summary));
        }
        "junit" => {
            println!("{}", rexpipe::testing::format_junit_xml(&summary, "rexpipe"));
        }
        _ => {
            // Default text format
            println!("{}", rexpipe::testing::format_test_report(&summary));
        }
    }

    // Exit with appropriate code
    if summary.failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

/// Run data format conversion mode.
fn run_data_conversion(matches: &clap::ArgMatches) -> Result<()> {
    use std::io::Read;

    // Read input
    let mut input_content = String::new();
    if let Some(input_file) = matches.get_one::<String>("input") {
        File::open(input_file)?.read_to_string(&mut input_content)?;
    } else {
        io::stdin().read_to_string(&mut input_content)?;
    }

    // Determine input format
    let input_format = if let Some(fmt) = matches.get_one::<String>("input-format") {
        parse_data_format(fmt)?
    } else {
        DataFormat::detect(&input_content)
    };

    // Determine output format
    let output_format = if let Some(fmt) = matches.get_one::<String>("output-format") {
        parse_data_format(fmt)?
    } else {
        return Err(anyhow!(
            "Output format is required for conversion. Use --output-format (json, csv, yaml, xml, toml)"
        ));
    };

    // Parse input
    let data = DataValue::parse(&input_content, input_format)
        .map_err(|e| anyhow!("Failed to parse input as {:?}: {}", input_format, e))?;

    // Convert to output format
    let pretty = matches.get_flag("pretty");
    let output = data.to_format_with_options(output_format, pretty)
        .map_err(|e| anyhow!("Failed to convert to {:?}: {}", output_format, e))?;

    // Write output
    if let Some(output_file) = matches.get_one::<String>("output") {
        std::fs::write(output_file, &output)?;
    } else {
        print!("{}", output);
    }

    Ok(())
}

/// Run data query mode with jq-like expressions.
fn run_data_query(matches: &clap::ArgMatches) -> Result<()> {
    use std::io::Read;

    let query = matches
        .get_one::<String>("data-query")
        .ok_or_else(|| anyhow!("Query expression is required"))?;

    // Read input
    let mut input_content = String::new();
    if let Some(input_file) = matches.get_one::<String>("input") {
        File::open(input_file)?.read_to_string(&mut input_content)?;
    } else {
        io::stdin().read_to_string(&mut input_content)?;
    }

    // Determine input format
    let input_format = if let Some(fmt) = matches.get_one::<String>("input-format") {
        parse_data_format(fmt)?
    } else {
        DataFormat::detect(&input_content)
    };

    // Parse input
    let data = DataValue::parse(&input_content, input_format)
        .map_err(|e| anyhow!("Failed to parse input as {:?}: {}", input_format, e))?;

    // Execute query
    let result = data
        .query(query)
        .map_err(|e| anyhow!("Query failed: {}", e))?;

    // Determine output format
    let output_format = if let Some(fmt) = matches.get_one::<String>("output-format") {
        parse_data_format(fmt)?
    } else {
        // Default to JSON for query results
        DataFormat::Json
    };

    // Format and output results
    let pretty = matches.get_flag("pretty");
    let output = result.to_format_with_options(output_format, pretty)
        .map_err(|e| anyhow!("Failed to format result: {}", e))?;
    println!("{}", output.trim_end());

    Ok(())
}

/// Parse a data format string into a DataFormat enum.
fn parse_data_format(s: &str) -> Result<DataFormat> {
    match s.to_lowercase().as_str() {
        "text" => Ok(DataFormat::Text),
        "json" => Ok(DataFormat::Json),
        "jsonl" | "jsonlines" | "ndjson" => Ok(DataFormat::JsonLines),
        "csv" => Ok(DataFormat::Csv),
        "tsv" => Ok(DataFormat::Tsv),
        "yaml" | "yml" => Ok(DataFormat::Yaml),
        "xml" => Ok(DataFormat::Xml),
        "toml" => Ok(DataFormat::Toml),
        _ => Err(anyhow!(
            "Unknown data format: {}. Supported: text, json, jsonl, csv, tsv, yaml, xml, toml",
            s
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_loading_from_pattern() {
        let mut _matches: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        // This would normally be created by clap, but for testing we simulate it

        let config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUMBER"));
        assert_eq!(config.step.len(), 1);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_parse_data_format() {
        assert!(matches!(parse_data_format("json").unwrap(), DataFormat::Json));
        assert!(matches!(parse_data_format("CSV").unwrap(), DataFormat::Csv));
        assert!(matches!(parse_data_format("yaml").unwrap(), DataFormat::Yaml));
        assert!(matches!(parse_data_format("yml").unwrap(), DataFormat::Yaml));
        assert!(matches!(parse_data_format("xml").unwrap(), DataFormat::Xml));
        assert!(matches!(parse_data_format("toml").unwrap(), DataFormat::Toml));
        assert!(parse_data_format("unknown").is_err());
    }
}
