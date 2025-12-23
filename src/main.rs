use anyhow::{Error as AnyhowError, Result, anyhow};
use clap::{Arg, ArgAction, Command, ValueHint, value_parser};
use clap_complete::{Generator, Shell, generate};
use clap_mangen::Man;
use log::{debug, info};
use std::fs::File;
#[cfg(feature = "tree-sitter")]
use std::io::Read;
use std::io::{self, BufReader, IsTerminal};
use std::path::{Path, PathBuf};

// Import from the library crate
use rexpipe::checkpoint::{Checkpoint, CheckpointConfig, GitDiff};
use rexpipe::crossfile::{CrossFileConfig, CrossFileManager, ViolationAction, format_check_report};
use rexpipe::error::{ConfigError, LibraryError, PatternError, RexpipeError, ValidationError};
use rexpipe::files::{FileProcessingOptions, MultiFileProcessor, MultiFileResult};
use rexpipe::inspector::{Inspector, InspectorOptions};
use rexpipe::json_schema;
use rexpipe::library;
use rexpipe::library::LibraryResolver;
use rexpipe::pipeline::{
    MaxLineAction, PipelineConfig, PipelineSettings, PipelineStep, RegexFlag, StepType,
    TransformAction,
};
use rexpipe::plugin::PluginRegistry;
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
    if std::env::var("NO_COLOR")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
    {
        return false;
    }

    // Otherwise, use color if stdout is a terminal
    std::io::stdout().is_terminal()
}

/// Determine if JSON output should be used.
/// JSON is the default when stdout is not a terminal (piped output).
/// This makes rexpipe ideal for scripting and automation.
///
/// Priority:
/// 1. --text flag forces plain text output (returns false)
/// 2. --json flag forces JSON output (returns true)
/// 3. Default: JSON when stdout is NOT a terminal (scripting-friendly)
fn should_use_json(matches: &clap::ArgMatches) -> bool {
    // --text forces plain text output
    if matches.get_flag("text") {
        return false;
    }

    // --json forces JSON output
    if matches.get_flag("json") {
        return true;
    }

    // Default: JSON when stdout is not a terminal
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

SHORTHAND SYNTAX:
    Use [[filter]], [[substitute]], etc. instead of [[step]] + type = "...":

    # Before (verbose):
    [[step]]
    type = "filter"
    pattern = "^\\[OK\\]"
    action = "drop_line"

    # After (concise):
    [[filter]]
    pattern = "^\\[OK\\]"
    action = "drop_line"

    Available shorthand sections:
      [[filter]]      Filter steps (keep_line, drop_line, etc.)
      [[substitute]]  Substitution steps
      [[extract]]     Extraction steps
      [[validate]]    Validation steps
      [[transform]]   Transform steps
      [[block]]       Block-scoped steps

    You can mix [[step]] with shorthand sections. [[step]] runs first.

FILTER ACTIONS:
    For type = "filter" steps, use these actions:
      keep_line             Keep lines matching the pattern (whitelist)
      drop_line             Drop lines matching the pattern (blacklist)
      keep_match            Output only the matched text from each line
      drop_match            Remove matched text, keeping the rest of the line
      deduplicate_by_prefix Deduplicate based on pattern capture group

    For type = "block" steps (multi-line matching):
      keep_block            Keep lines within matching blocks
      drop_block            Drop lines within matching blocks
      collect_block         Collect and output block contents together
      deduplicate           Output each unique block only once

PER-STEP FLAGS:
    Individual steps can specify regex flags without affecting other steps:

    [[step]]
    type = "filter"
    pattern = "(?<=user=)\\w+"
    flags = ["pcre"]              # PCRE for this step only
    action = "keep_line"

    Available flags:
      global            Apply replacement to all matches (not just first)
      case_insensitive  Case-insensitive matching
      multiline         ^ and $ match line boundaries
      dot_all           . matches newlines
      unicode           Enable Unicode support
      extended          Allow whitespace and comments in pattern
      pcre              Use PCRE engine (lookahead/lookbehind) for this step

CONFIG COMPOSITION:
    Pipelines can inherit from base configs and include pattern libraries:

    # patterns/common.toml - Pattern library file
    [patterns.logs]
    error = "(ERROR|FATAL|CRITICAL)"
    warn = "(WARN|WARNING)"
    info = "^\\[INFO\\]"

    # pipeline.toml - Uses the library
    patterns_include = ["patterns/common.toml"]

    [[filter]]
    pattern = "${logs.error}"   # Expands to (ERROR|FATAL|CRITICAL)
    action = "keep_line"

    # Inherit from base config
    extends = "base.toml"       # Steps/settings from base are applied first

    See README for full pattern library and inheritance documentation.

DEBUGGING:
    Control logging verbosity with -q and -v flags:
      rexpipe -q ...      # errors only (also suppresses output)
      rexpipe ...         # warnings (default)
      rexpipe -v ...      # info level
      rexpipe -vv ...     # debug level
      rexpipe -vvv ...    # trace level (line-by-line processing details)

    Note: -v flags override -q's log effect (e.g., -q -v shows info logs)

    Or use RUST_LOG for fine-grained control (takes precedence):
      RUST_LOG=rexpipe=debug rexpipe -c config.toml < input.txt

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
        .version(env!("CARGO_PKG_VERSION"))
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
            Arg::new("validate-config")
                .long("validate-config")
                .help("Validate configuration file without processing")
                .long_help(
                    "Parse and validate the pipeline configuration file, checking for:\n\
                     - TOML syntax errors\n\
                     - Invalid regex patterns\n\
                     - Missing required fields\n\
                     - Invalid option values\n\
                     - Pattern library references\n\n\
                     Exits with 0 if valid, 1 if invalid."
                )
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("apply")
                .long("apply")
                .help("Actually apply changes (required for in-place edits when piping/scripting)")
                .long_help(
                    "Explicitly confirm that file modifications should be applied. \
                     In non-interactive mode (piped/scripted), this flag is required for \
                     destructive operations like in-place editing (-i). This prevents \
                     accidental file modifications when rexpipe is used in automated \
                     pipelines."
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
                .help("Quiet mode - suppress output and reduce logs to errors only")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Increase logging verbosity (repeat for more: -v=info, -vv=debug, -vvv=trace)")
                .action(ArgAction::Count)
                .global(true),
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
                .help("Force plain text output even when piping (override JSON default)")
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
                .help("Error output format: text (default) or json for machine-parseable errors")
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
        .arg(
            Arg::new("checkpoint-info")
                .long("checkpoint-info")
                .value_name("FILE")
                .help("Display checkpoint file information and exit")
                .long_help(
                    "Read and display the contents of a checkpoint file in human-readable format. \
                     Shows tracked files, byte offsets, timestamps, and file states. \
                     Useful for debugging and verifying checkpoint integrity."
                )
                .value_hint(ValueHint::FilePath),
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
        .arg(
            Arg::new("positive-file")
                .long("positive-file")
                .value_name("FILE")
                .help("File containing positive examples (one per line)")
                .long_help(
                    "Load positive examples from a file, one example per line. \
                     Empty lines and lines starting with # are ignored. \
                     Can be combined with --positive for additional examples."
                ),
        )
        .arg(
            Arg::new("negative-file")
                .long("negative-file")
                .value_name("FILE")
                .help("File containing negative examples (one per line)")
                .long_help(
                    "Load negative examples from a file, one example per line. \
                     Empty lines and lines starting with # are ignored. \
                     Can be combined with --negative for additional examples."
                ),
        )
        .arg(
            Arg::new("learn-output")
                .long("learn-output")
                .value_name("FILE")
                .help("Save learned patterns to a TOML pipeline file")
                .long_help(
                    "Instead of printing the learned pipeline to stdout, save it directly \
                     to the specified file. The file can then be used with -c/--config."
                )
                .value_hint(ValueHint::FilePath),
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
                     it applies. Useful for understanding a pipeline before running it. \
                     Output can be JSON with --json flag."
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
                     and transformation counts. Useful for confirming that processing \
                     completed as expected. Output can be JSON with --json flag."
                )
                .action(ArgAction::SetTrue),
        )
        // === Security ===
        .arg(
            Arg::new("allow-shell")
                .long("allow-shell")
                .help("Enable shell command execution in transforms")
                .long_help(
                    "Enable shell transforms in the pipeline. By default, shell command execution \
                     is disabled for security when processing untrusted input. Use this flag when \
                     you trust the pipeline configuration and need shell transforms."
                )
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-shell")
                .long("no-shell")
                .help("Disable shell command execution (default, kept for compatibility)")
                .action(ArgAction::SetTrue)
                .hide(true), // Hidden since it's now the default
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
        .arg(
            Arg::new("man")
                .long("man")
                .help("Generate man page to stdout")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("init")
                .long("init")
                .value_name("FILE")
                .help("Generate a starter pipeline configuration")
                .long_help(
                    "Generate a starter pipeline configuration file with example steps.\n\n\
                     Templates available:\n\
                       basic    - Simple substitution pipeline (default)\n\
                       log      - Log processing with filtering\n\
                       security - Data sanitization and masking\n\
                       validate - Input validation pipeline\n\n\
                     Examples:\n\
                       rexpipe --init pipeline.toml\n\
                       rexpipe --init my-config.toml:log\n\
                       rexpipe --init config.toml:security"
                ),
        )
        .arg(
            Arg::new("watch")
                .short('w')
                .long("watch")
                .help("Watch input files for changes and re-run")
                .long_help(
                    "Watch mode: re-runs the pipeline when input files change.\n\n\
                     Requires the 'watch' feature to be enabled.\n\n\
                     Example:\n\
                       rexpipe -c config.toml --watch ./logs/*.log"
                )
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("plugin-dir")
                .long("plugin-dir")
                .value_name("DIR")
                .help("Load plugins from directory")
                .long_help(
                    "Load script-based plugins from a directory.\n\n\
                     Scripts are registered as plugins with names derived from filenames.\n\
                     Supported types: .sh (shell), .py (Python), .rb (Ruby), .pl (Perl)\n\n\
                     Example:\n\
                       rexpipe --plugin-dir ./my-plugins -c config.toml < input\n\n\
                     Default plugin directories (checked automatically):\n\
                       - ./plugins/\n\
                       - ~/.config/rexpipe/plugins/\n\
                       - /usr/local/share/rexpipe/plugins/\n\
                       - $REXPIPE_PLUGIN_DIR (if set)"
                )
                .value_hint(ValueHint::DirPath),
        )
        // === Git Integration ===
        .arg(
            Arg::new("conventional-commits")
                .long("conventional-commits")
                .help("Validate commit messages against Conventional Commits specification")
                .long_help(
                    "Validate input as a commit message against the Conventional Commits specification.\n\n\
                     Format: <type>[optional scope]: <description>\n\n\
                     Valid types: feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert\n\n\
                     Exit codes:\n\
                     - 0: Valid commit message\n\
                     - 1: Invalid commit message (with explanation)\n\n\
                     Use in git hooks:\n\
                       rexpipe --conventional-commits < .git/COMMIT_EDITMSG"
                )
                .action(ArgAction::SetTrue),
        )
        // === Streaming Mode ===
        .arg(
            Arg::new("stream")
                .long("stream")
                .help("Real-time streaming mode with live aggregation")
                .long_help(
                    "Process input in real-time streaming mode with live aggregation.\n\n\
                     Useful for tailing logs and getting immediate feedback:\n\
                       tail -f app.log | rexpipe -c errors.toml --stream\n\n\
                     Aggregation shows:\n\
                     - Running match counts per pattern\n\
                     - Error rates and trends\n\
                     - Periodic summary output"
                )
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("stream-interval")
                .long("stream-interval")
                .value_name("SECS")
                .help("Interval in seconds for streaming aggregation summary (default: 5)")
                .value_parser(value_parser!(u64))
                .default_value("5"),
        )
        // === Atomic Multi-File ===
        .arg(
            Arg::new("atomic")
                .long("atomic")
                .help("Atomic multi-file transforms with rollback on failure")
                .long_help(
                    "Enable atomic mode for multi-file transforms.\n\n\
                     In atomic mode:\n\
                     - All changes are staged to temporary files first\n\
                     - If any file fails to process, all changes are rolled back\n\
                     - Changes are only committed if all files succeed\n\n\
                     Requires --in-place or --output-dir.\n\n\
                     Example:\n\
                       rexpipe -c migrate.toml -i --atomic -R src/"
                )
                .action(ArgAction::SetTrue),
        )
        // === Test Data Generation ===
        .arg(
            Arg::new("generate")
                .long("generate")
                .value_name("COUNT")
                .help("Generate test data from pipeline patterns")
                .long_help(
                    "Generate test data that matches the patterns in the pipeline.\n\n\
                     Uses pipeline patterns to generate sample data for testing.\n\
                     Useful for creating test fixtures or fuzzing.\n\n\
                     Example:\n\
                       rexpipe -c email-validator.toml --generate 10\n\n\
                     This will generate 10 samples that match the email pattern."
                )
                .value_parser(value_parser!(u32)),
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

/// Generate man page and write to stdout
fn print_man_page(cmd: Command) -> Result<()> {
    let man = Man::new(cmd);
    man.render(&mut io::stdout())
        .map_err(|e| anyhow!("Failed to render man page: {}", e))
}

/// Generate a starter pipeline configuration file
fn generate_starter_pipeline(init_arg: &str) -> Result<()> {
    // Parse FILE:TEMPLATE format
    let (file_path, template) = if let Some(idx) = init_arg.rfind(':') {
        let path = &init_arg[..idx];
        let tmpl = &init_arg[idx + 1..];
        // Handle Windows paths like C:\path
        if path.len() == 1
            && path
                .chars()
                .next()
                .map(|c| c.is_ascii_alphabetic())
                .unwrap_or(false)
        {
            (init_arg, "basic")
        } else {
            (path, tmpl)
        }
    } else {
        (init_arg, "basic")
    };

    // Check if file already exists
    if std::path::Path::new(file_path).exists() {
        return Err(anyhow!(
            "File '{}' already exists. Use a different name or delete the existing file.",
            file_path
        ));
    }

    let content = match template {
        "basic" => TEMPLATE_BASIC,
        "log" => TEMPLATE_LOG,
        "security" => TEMPLATE_SECURITY,
        "validate" => TEMPLATE_VALIDATE,
        _ => {
            return Err(anyhow!(
                "Unknown template '{}'. Available: basic, log, security, validate",
                template
            ));
        }
    };

    std::fs::write(file_path, content)?;
    println!("Created pipeline configuration: {}", file_path);
    println!("Edit the file to customize your pipeline, then run:");
    println!("  rexpipe -c {} < input.txt", file_path);

    Ok(())
}

const TEMPLATE_BASIC: &str = r#"# rexpipe Pipeline Configuration
# Generated with: rexpipe --init
#
# Run with: rexpipe -c this_file.toml < input.txt

name = "My Pipeline"
description = "A basic text processing pipeline"
version = "1.0.0"

# Step 1: Simple substitution
[[step]]
pattern = 'foo'
replacement = "bar"
description = "Replace foo with bar"

# Step 2: Another substitution with regex
[[step]]
pattern = '\d{4}-\d{2}-\d{2}'
replacement = "[DATE]"
description = "Redact dates"
flags = ["global"]

# Uncomment to add more steps:
# [[step]]
# type = "filter"
# pattern = 'DEBUG'
# action = "drop_line"
# description = "Remove debug lines"
"#;

const TEMPLATE_LOG: &str = r#"# rexpipe Log Processing Pipeline
# Generated with: rexpipe --init pipeline.toml:log
#
# Run with: rexpipe -c this_file.toml < application.log

name = "Log Processor"
description = "Filter and transform log files"
version = "1.0.0"

# Step 1: Keep only ERROR and WARN lines
[[step]]
type = "filter"
pattern = '\[(ERROR|WARN)\]'
action = "keep_line"
description = "Keep error and warning lines"

# Step 2: Normalize log levels
[[step]]
pattern = '\[WARNING\]'
replacement = "[WARN]"
description = "Standardize WARNING to WARN"

# Step 3: Anonymize IP addresses
[[step]]
pattern = '\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}'
replacement = "[IP_REDACTED]"
flags = ["global"]
description = "Redact IP addresses"

# Step 4: Anonymize email addresses
[[step]]
pattern = '[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}'
replacement = "[EMAIL_REDACTED]"
flags = ["global"]
description = "Redact email addresses"
"#;

const TEMPLATE_SECURITY: &str = r#"# rexpipe Security/Sanitization Pipeline
# Generated with: rexpipe --init pipeline.toml:security
#
# Run with: rexpipe -c this_file.toml < sensitive_data.txt

name = "Data Sanitizer"
description = "Redact sensitive information for safe sharing"
version = "1.0.0"

# Step 1: Redact credit card numbers
[[step]]
pattern = '\b\d{4}[-\s]?\d{4}[-\s]?\d{4}[-\s]?\d{4}\b'
replacement = "[CREDIT_CARD]"
flags = ["global"]
description = "Redact credit card numbers"

# Step 2: Redact Social Security Numbers
[[step]]
pattern = '\b\d{3}-\d{2}-\d{4}\b'
replacement = "[SSN]"
flags = ["global"]
description = "Redact SSN"

# Step 3: Redact API keys (generic pattern)
[[step]]
pattern = '(?i)(api[_-]?key|apikey|secret|token)\s*[=:]\s*["\']?([a-zA-Z0-9_-]{20,})["\']?'
replacement = "${1}=[REDACTED]"
flags = ["global"]
description = "Redact API keys and secrets"

# Step 4: Redact phone numbers
[[step]]
pattern = '\b\d{3}[-.]?\d{3}[-.]?\d{4}\b'
replacement = "[PHONE]"
flags = ["global"]
description = "Redact phone numbers"

# Step 5: Redact email addresses
[[step]]
pattern = '[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}'
replacement = "[EMAIL]"
flags = ["global"]
description = "Redact email addresses"
"#;

const TEMPLATE_VALIDATE: &str = r#"# rexpipe Validation Pipeline
# Generated with: rexpipe --init pipeline.toml:validate
#
# Run with: rexpipe -c this_file.toml < input.txt

name = "Input Validator"
description = "Validate input format and flag issues"
version = "1.0.0"

# Step 1: Validate that each line has a timestamp
[[step]]
type = "validate"
pattern = '^\d{4}-\d{2}-\d{2}'
description = "Each line must start with a date (YYYY-MM-DD)"
on_mismatch = "warn"

# Step 2: Flag lines without proper log level
[[step]]
type = "validate"
pattern = '\[(DEBUG|INFO|WARN|ERROR|FATAL)\]'
description = "Each line must have a valid log level"
on_mismatch = "warn"

# Step 3: Extract only valid entries
[[step]]
type = "filter"
pattern = '^\d{4}-\d{2}-\d{2}.*\[(DEBUG|INFO|WARN|ERROR|FATAL)\]'
action = "keep_line"
description = "Keep only properly formatted lines"
"#;

fn main() {
    // Parse arguments first so we can use -v flags for logging
    let matches = build_cli().get_matches();

    // Initialize logger: RUST_LOG takes precedence, otherwise use -v/-q flags
    // -q reduces to error-only, -v increases verbosity, -v overrides -q's log effect
    let verbose_count = matches.get_count("verbose");
    let quiet = matches.get_flag("quiet");
    let log_level = match (verbose_count, quiet) {
        (0, true) => "error",  // -q alone: errors only
        (0, false) => "warn",  // default: warnings
        (1, _) => "info",      // -v (overrides -q)
        (2, _) => "debug",     // -vv
        _ => "trace",          // -vvv+
    };

    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(format!("rexpipe={}", log_level)),
    )
    .format_timestamp(None)
    .init();

    debug!("Starting rexpipe");

    // Handle completions generation first (before any other processing)
    if let Some(shell) = matches.get_one::<Shell>("completions").copied() {
        let mut cmd = build_cli();
        print_completions(shell, &mut cmd);
        return;
    }

    // Handle man page generation
    if matches.get_flag("man") {
        if let Err(e) = print_man_page(build_cli()) {
            eprintln!("Error: {}", e);
            std::process::exit(exit_codes::IO_ERROR);
        }
        return;
    }

    // Handle --init to generate starter pipeline
    if let Some(init_arg) = matches.get_one::<String>("init") {
        if let Err(e) = generate_starter_pipeline(init_arg) {
            eprintln!("Error: {}", e);
            std::process::exit(exit_codes::IO_ERROR);
        }
        return;
    }

    // Load plugins from default directories
    let default_loaded = PluginRegistry::load_default_plugins_to_global();
    if default_loaded > 0 {
        debug!("Loaded {} plugins from default directories", default_loaded);
    }

    // Load plugins from --plugin-dir if specified
    if let Some(plugin_dir) = matches.get_one::<String>("plugin-dir") {
        let plugin_path = std::path::Path::new(plugin_dir);
        match PluginRegistry::load_plugins_to_global(plugin_path) {
            Ok(count) => {
                if count > 0 {
                    debug!("Loaded {} plugins from {}", count, plugin_dir);
                }
            }
            Err(e) => {
                eprintln!("Warning: Failed to load plugins from {}: {}", plugin_dir, e);
            }
        }
    }

    if let Err(e) = run_application(&matches) {
        let exit_code = categorize_error(&e);

        // Check if JSON error output is requested
        let use_json_errors = matches
            .get_one::<String>("error-format")
            .map(|s| s == "json")
            .unwrap_or(false);

        if use_json_errors {
            // Output structured JSON error for machine consumption
            match json_schema::output_error_json(&e.to_string(), exit_code, None) {
                Ok(json) => eprintln!("{}", json),
                Err(_) => eprintln!("Error: {}", e), // Fallback to plain text
            }
        } else {
            // Print full error chain for better debugging
            // {:#} shows the error and all its causes
            eprintln!("Error: {:#}", e);
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

    // Handle config validation (--validate-config)
    if matches.get_flag("validate-config") {
        return validate_config_file(matches);
    }

    // Handle checkpoint info display
    if let Some(checkpoint_path) = matches.get_one::<String>("checkpoint-info") {
        return display_checkpoint_info(checkpoint_path);
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

    // Handle conventional commits validation mode
    if matches.get_flag("conventional-commits") {
        return run_conventional_commits_validation(matches);
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

    // Handle test data generation mode
    if let Some(count) = matches.get_one::<u32>("generate") {
        return run_test_data_generation(&config, *count, matches);
    }

    // Check if we're in multi-file mode
    let paths: Vec<PathBuf> = matches
        .get_many::<String>("paths")
        .map(|v| v.map(PathBuf::from).collect())
        .unwrap_or_default();

    let is_multi_file =
        matches.get_flag("recursive") || matches.get_flag("in-place") || !paths.is_empty();

    // Safety: require --apply for in-place edits in non-interactive mode
    // This prevents accidental file modifications when used in scripts or pipelines
    let in_place = matches.get_flag("in-place");
    let has_apply = matches.get_flag("apply");
    let is_interactive = io::stdin().is_terminal() && io::stdout().is_terminal();

    if in_place && !is_interactive && !has_apply && !matches.get_flag("dry-run") {
        // Non-interactive in-place edit without --apply: show dry-run preview
        eprintln!("Safety: In-place editing requires --apply flag in non-interactive mode.");
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

    // Handle watch mode
    if matches.get_flag("watch") {
        if paths.is_empty() {
            return Err(anyhow::anyhow!(
                "Watch mode requires file paths. Usage: rexpipe -c config.toml --watch ./files/*.log"
            ));
        }
        let path_strings: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
        return run_watch_mode(matches, &path_strings, &config);
    }

    if is_multi_file {
        return run_multi_file_mode(&config, matches, paths);
    }

    // Single file/stdin mode
    let input: Box<dyn io::BufRead> = if let Some(input_file) = matches.get_one::<String>("input") {
        Box::new(BufReader::new(File::open(input_file)?))
    } else {
        // Warn if stdin is a TTY (user might be expecting file input)
        if io::stdin().is_terminal() && !matches.get_flag("quiet") {
            eprintln!("Reading from stdin (press Ctrl+D when done, or Ctrl+C to cancel)...");
            eprintln!("Tip: Pipe input or use -f <file> to read from a file.");
        }
        Box::new(io::stdin().lock())
    };

    // Handle streaming mode
    if matches.get_flag("stream") {
        return run_streaming_mode(&config, input, matches);
    }

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
        // --allow-shell enables shell transforms (disabled by default for security)
        // --no-shell kept for backwards compatibility (explicitly disables)
        allow_shell: matches.get_flag("allow-shell") && !matches.get_flag("no-shell"),
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
                    debug!(
                        "Skipping {} (unchanged since last checkpoint)",
                        file.display()
                    );
                }
                Err(e) => {
                    debug!(
                        "Error checking checkpoint for {}: {}, will process",
                        file.display(),
                        e
                    );
                    to_process.push(file.clone());
                }
            }
        }

        if skipped > 0 && !quiet {
            eprintln!(
                "Checkpoint: skipping {} unchanged files, processing {}",
                skipped,
                to_process.len()
            );
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

    // Filter by git-diff if enabled
    let files_to_process: Vec<PathBuf> =
        if let Some(git_ref) = matches.get_one::<String>("git-diff") {
            match GitDiff::discover(".", git_ref) {
                Ok(git_diff) => {
                    match git_diff.changed_files() {
                        Ok(changed_files) => {
                            let changed_set: std::collections::HashSet<_> =
                                changed_files.into_iter().collect();
                            let filtered: Vec<PathBuf> = files_to_process
                                .into_iter()
                                .filter(|f| {
                                    // Check if file is in changed set (handle both absolute and relative paths)
                                    let abs_path =
                                        std::fs::canonicalize(f).unwrap_or_else(|_| f.clone());
                                    changed_set.iter().any(|changed| {
                                        let changed_abs = std::fs::canonicalize(changed)
                                            .unwrap_or_else(|_| changed.clone());
                                        abs_path == changed_abs
                                    })
                                })
                                .collect();

                            if !quiet && filtered.len() < files.len() {
                                eprintln!(
                                    "Git diff: processing {} of {} files (changed since {})",
                                    filtered.len(),
                                    files.len(),
                                    git_ref
                                );
                            }

                            if filtered.is_empty() && !quiet {
                                eprintln!("Git diff: no files changed since {}", git_ref);
                                return Ok(());
                            }

                            filtered
                        }
                        Err(e) => {
                            if !quiet {
                                eprintln!("Warning: Could not get changed files from git: {}", e);
                            }
                            files_to_process
                        }
                    }
                }
                Err(e) => {
                    if !quiet {
                        eprintln!("Warning: Could not initialize git diff: {}", e);
                    }
                    files_to_process
                }
            }
        } else {
            files_to_process
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
                        eprintln!(
                            "Warning: Could not load {} for cross-file check: {}",
                            file.display(),
                            e
                        );
                    }
                }
            }

            // Scan for triggers and check rules
            manager
                .scan_triggers()
                .map_err(|e| anyhow!("Failed to scan triggers: {}", e))?;

            let results = manager
                .check_all()
                .map_err(|e| anyhow!("Failed to check cross-file rules: {}", e))?;

            // Report results
            let has_violations = results.iter().any(|r| !r.passed);

            if has_violations || !quiet {
                if json_output {
                    // Output as JSON
                    let json_results: Vec<serde_json::Value> = results
                        .iter()
                        .map(|r| {
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
                        })
                        .collect();
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
                        // Apply auto-fixes for violations
                        // Dry-run if --dry-run flag is set, or if not using in-place+apply
                        let explicit_dry_run = matches.get_flag("dry-run");
                        let fix_in_place = matches.get_flag("in-place");
                        let fix_apply = matches.get_flag("apply");
                        let dry_run = explicit_dry_run || !fix_in_place || !fix_apply;

                        if !quiet && dry_run {
                            eprintln!("\n--- Cross-File Fix Preview (dry-run) ---");
                            for result in &results {
                                if !result.passed {
                                    for violation in &result.violations {
                                        eprintln!(
                                            "Would add pattern '{}' to {}",
                                            result.trigger_pattern,
                                            violation.file.display()
                                        );
                                    }
                                }
                            }
                            eprintln!();
                        }

                        match manager.apply_fixes(&results, dry_run) {
                            Ok((files_modified, fixes_applied)) => {
                                if !quiet {
                                    if dry_run {
                                        eprintln!(
                                            "Dry-run: Would apply {} fix(es) to {} file(s)",
                                            fixes_applied, files_modified
                                        );
                                        if explicit_dry_run {
                                            eprintln!(
                                                "Remove --dry-run and use --apply -i to apply fixes"
                                            );
                                        } else {
                                            eprintln!("Use --apply with -i to apply fixes");
                                        }
                                    } else {
                                        eprintln!(
                                            "Applied {} fix(es) to {} file(s)",
                                            fixes_applied, files_modified
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                return Err(anyhow!("Failed to apply cross-file fixes: {}", e));
                            }
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
            checkpoint
                .save()
                .map_err(|e| anyhow!("Failed to save checkpoint: {}", e))?;
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
            checkpoint
                .save()
                .map_err(|e| anyhow!("Failed to save checkpoint: {}", e))?;
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
                checkpoint
                    .save()
                    .map_err(|e| anyhow!("Failed to save checkpoint: {}", e))?;
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
            checkpoint
                .save()
                .map_err(|e| anyhow!("Failed to save checkpoint: {}", e))?;
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
            checkpoint
                .save()
                .map_err(|e| anyhow!("Failed to save checkpoint: {}", e))?;
        }

        // Output summary as final JSONL line
        if let Ok(summary) = json_schema::output_streaming_summary_jsonl(&result) {
            println!("{}", summary);
        }

        if !result.has_matches() {
            std::process::exit(exit_codes::NO_MATCHES);
        }
        return Ok(());
    } else if matches.get_flag("atomic") && options.in_place {
        // Atomic mode: process to temp files first, then commit or rollback
        return run_atomic_multi_file_processing(
            config,
            &processor,
            &files_to_process,
            &options,
            &mut checkpoint,
            quiet,
            json_output,
        );
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
        checkpoint
            .save()
            .map_err(|e| anyhow!("Failed to save checkpoint: {}", e))?;
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
        // Apply shell transform setting from CLI
        // --allow-shell enables, --no-shell explicitly disables (overrides config)
        if settings.allow_shell {
            config.settings.allow_shell = true;
        } else if !settings.allow_shell {
            // Default is false, but config might have allow_shell = true
            // CLI --allow-shell was not passed, so respect CLI default (disabled)
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
                "Shell transforms are disabled by default for security, but config contains shell commands:\n  {}\n\
                Use --allow-shell to enable shell command execution.",
                shell_commands.join("\n  ")
            ));
        }

        // Warn about shell transforms when enabled
        if config.settings.allow_shell && config.has_shell_transforms() {
            let shell_commands = config.get_shell_commands();
            eprintln!(
                "Warning: This pipeline executes shell commands (--allow-shell enabled):\n  {}",
                shell_commands.join("\n  ")
            );

            // Analyze each command for potentially dangerous patterns
            let mut all_warnings = Vec::new();
            for cmd in &shell_commands {
                let warnings = PluginRegistry::validate_shell_command(cmd);
                for warning in warnings {
                    all_warnings.push(format!("  - {} in: {}", warning, cmd));
                }
            }
            if !all_warnings.is_empty() {
                eprintln!("\nSecurity analysis warnings:\n{}", all_warnings.join("\n"));
                eprintln!(
                    "\nThese commands may have security implications. Review carefully before proceeding."
                );
            }
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
        let replacement = matches
            .get_one::<String>("replacement")
            .map(|s| s.to_string());
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
                _ => Some(TransformAction::Plugin {
                    name: name.clone(),
                    args: vec![],
                }),
            };
            (StepType::Transform, None, transform_action)
        } else if replacement.is_some() {
            (StepType::Substitute, None, None)
        } else {
            (
                StepType::Filter,
                Some(rexpipe::pipeline::StepAction::KeepMatch),
                None,
            )
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
            settings,
            step: vec![step],
            bidirectional,
            ..Default::default()
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
    use rexpipe::pipeline::{StepAction, StepType, TransformAction};

    let json_output = should_use_json(matches);

    if json_output {
        // JSON output for machine consumption
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

        let steps: Vec<StepExplanation> = config
            .step
            .iter()
            .enumerate()
            .map(|(i, step)| {
                let (effect, action_str) = match step.step_type {
                    StepType::Substitute => {
                        let repl = step
                            .replacement
                            .clone()
                            .unwrap_or_else(|| "(none)".to_string());
                        (format!("Replace matches with '{}'", repl), None)
                    }
                    StepType::Filter => {
                        let action_desc = step
                            .action
                            .as_ref()
                            .map(|a| match a {
                                StepAction::KeepLine => "Keep only matching lines".to_string(),
                                StepAction::DropLine => "Remove matching lines".to_string(),
                                StepAction::KeepMatch => "Keep only the match text".to_string(),
                                StepAction::DropMatch => "Remove the match from line".to_string(),
                                StepAction::DeduplicateByPrefix => {
                                    "Deduplicate by prefix from capture group".to_string()
                                }
                                _ => "Filter action".to_string(),
                            })
                            .unwrap_or_else(|| "Filter lines".to_string());
                        (
                            action_desc,
                            step.action
                                .as_ref()
                                .map(|a| format!("{:?}", a).to_lowercase()),
                        )
                    }
                    StepType::Extract => ("Extract matching text".to_string(), None),
                    StepType::Validate => ("Validate lines match pattern".to_string(), None),
                    StepType::Transform => {
                        let t = step
                            .transform
                            .as_ref()
                            .map(|t| match t {
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
                            })
                            .unwrap_or("Transform matches");
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
            })
            .collect();

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
                            TransformAction::NormalizeWhitespace => {
                                "Normalize whitespace".to_string()
                            }
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

        println!(
            "Summary: Pipeline processes input through {} step(s).",
            config.step.len()
        );
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

/// Display checkpoint file information.
fn display_checkpoint_info(checkpoint_path: &str) -> Result<()> {
    use chrono::{DateTime, Utc};
    use rexpipe::checkpoint::CheckpointState;

    let path = Path::new(checkpoint_path);
    if !path.exists() {
        return Err(anyhow!("Checkpoint file not found: {}", checkpoint_path));
    }

    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow!("Failed to read checkpoint file: {}", e))?;

    let state: CheckpointState =
        serde_json::from_str(&content).map_err(|e| anyhow!("Invalid checkpoint format: {}", e))?;

    // Format timestamps
    let format_time = |ts: u64| -> String {
        DateTime::<Utc>::from_timestamp(ts as i64, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| format!("{}", ts))
    };

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║                    CHECKPOINT INFORMATION                        ║");
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    println!("File: {}", checkpoint_path);
    println!("Version: {}", state.version);
    if let Some(ref pipeline_id) = state.pipeline_id {
        println!("Pipeline ID: {}", pipeline_id);
    }
    println!("Created: {}", format_time(state.created_at));
    println!("Updated: {}", format_time(state.updated_at));
    println!();

    // Display file statistics
    println!("Tracked Files: {}", state.files.len());
    let total_bytes: u64 = state.files.values().map(|f| f.byte_offset).sum();
    let total_lines: u64 = state.files.values().map(|f| f.line_number).sum();
    println!(
        "Total Progress: {} bytes processed, {} lines tracked",
        total_bytes, total_lines
    );
    println!();

    // Display each tracked file with staleness detection
    if !state.files.is_empty() {
        println!("File Details:");
        println!("{:-<70}", "");

        let mut stale_count = 0;
        let mut missing_count = 0;
        let mut grown_count = 0;

        for (path, file_state) in &state.files {
            println!("  Path: {}", path.display());

            // Check file status
            let status = match std::fs::metadata(path) {
                Ok(meta) => {
                    let current_size = meta.len();
                    let current_mtime = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs());

                    if current_size > file_state.size {
                        grown_count += 1;
                        let new_bytes = current_size - file_state.size;
                        format!("📈 GROWN (+{} bytes to process)", new_bytes)
                    } else if let (Some(ckpt_mtime), Some(curr_mtime)) =
                        (file_state.mtime, current_mtime)
                    {
                        if curr_mtime > ckpt_mtime {
                            stale_count += 1;
                            "⚠️ STALE (modified since checkpoint)".to_string()
                        } else {
                            "✓ Current".to_string()
                        }
                    } else {
                        "✓ Current".to_string()
                    }
                }
                Err(_) => {
                    missing_count += 1;
                    "❌ MISSING (file deleted)".to_string()
                }
            };

            println!("    Status: {}", status);
            println!(
                "    Offset: {} bytes (line {})",
                file_state.byte_offset, file_state.line_number
            );
            println!("    Size at checkpoint: {} bytes", file_state.size);
            if let Some(mtime) = file_state.mtime {
                println!("    Modified at checkpoint: {}", format_time(mtime));
            }
            if let Some(ref hash) = file_state.content_hash {
                println!("    Hash: {}...", &hash[..hash.len().min(16)]);
            }
            println!(
                "    Last Processed: {}",
                format_time(file_state.last_processed)
            );
            println!();
        }

        // Summary of file status
        if stale_count > 0 || missing_count > 0 || grown_count > 0 {
            println!("File Status Summary:");
            if grown_count > 0 {
                println!("  📈 {} file(s) have new content to process", grown_count);
            }
            if stale_count > 0 {
                println!("  ⚠️  {} file(s) modified since checkpoint", stale_count);
            }
            if missing_count > 0 {
                println!("  ❌ {} file(s) no longer exist", missing_count);
            }
            println!();
        }
    }

    // Display metadata
    if !state.metadata.is_empty() {
        println!("Metadata:");
        for (key, value) in &state.metadata {
            println!("  {}: {}", key, value);
        }
    }

    Ok(())
}

/// Validate a pipeline configuration file without processing.
fn validate_config_file(matches: &clap::ArgMatches) -> Result<()> {
    let config_path = matches
        .get_one::<String>("config")
        .ok_or_else(|| anyhow!("--validate-config requires a config file (-c/--config)"))?;

    let path = Path::new(config_path);
    if !path.exists() {
        return Err(anyhow!("Config file not found: {}", config_path));
    }

    // Parse the config file
    let content =
        std::fs::read_to_string(path).map_err(|e| anyhow!("Failed to read config file: {}", e))?;

    let config: PipelineConfig =
        toml::from_str(&content).map_err(|e| anyhow!("TOML parsing error: {}", e))?;

    // Validate the config
    config.validate().map_err(|errors| {
        anyhow!(
            "Configuration validation errors:\n  {}",
            errors.join("\n  ")
        )
    })?;

    // Try to compile the processor to catch regex errors
    match StreamProcessor::new(config.clone()) {
        Ok(_) => {
            println!("✓ Configuration '{}' is valid", config_path);

            // Show summary
            if let Some(name) = &config.name {
                println!("  Name: {}", name);
            }
            println!("  Steps: {}", config.step.len());

            // Check for shell transforms
            if config.has_shell_transforms() {
                let shell_count = config.get_shell_commands().len();
                println!(
                    "  Shell transforms: {} (requires --allow-shell)",
                    shell_count
                );
            }

            // Check for pattern library references
            let patterns_with_refs: usize = config
                .step
                .iter()
                .filter(|s| s.pattern.starts_with("${"))
                .count();
            if patterns_with_refs > 0 {
                println!("  Pattern library references: {}", patterns_with_refs);
            }

            // Check for tests
            if !config.tests.is_empty() {
                println!("  Inline tests: {}", config.tests.len());
            }

            Ok(())
        }
        Err(e) => {
            println!("✗ Configuration '{}' is invalid:", config_path);
            println!("  {}", e);
            Err(anyhow!("Configuration validation failed: {}", e))
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

    // Check if syntax-aware processing is needed (requires tree-sitter feature)
    #[cfg(feature = "tree-sitter")]
    let use_syntax_aware = processor.has_syntax_aware_steps();
    #[cfg(not(feature = "tree-sitter"))]
    let use_syntax_aware = false;

    // Get language from CLI for syntax-aware processing
    #[cfg(feature = "tree-sitter")]
    let cli_language: Option<rexpipe::syntax::Language> = matches
        .get_one::<String>("language")
        .and_then(|s| s.parse().ok());

    if quiet {
        // Quiet mode: process but don't output anything
        let result = if use_syntax_aware {
            #[cfg(feature = "tree-sitter")]
            {
                let content = read_input_to_string(input)?;
                let (_, result) = processor.process_file_content(&content, cli_language)?;
                result
            }
            #[cfg(not(feature = "tree-sitter"))]
            {
                let mut output = std::io::sink();
                processor.process_stream(input, &mut output)?
            }
        } else {
            let mut output = std::io::sink();
            processor.process_stream(input, &mut output)?
        };
        if result.matches_found == 0 {
            std::process::exit(exit_codes::NO_MATCHES);
        }
        return Ok(());
    }

    if count_only {
        // Count mode: just count matches
        let result = if use_syntax_aware {
            #[cfg(feature = "tree-sitter")]
            {
                let content = read_input_to_string(input)?;
                let (_, result) = processor.process_file_content(&content, cli_language)?;
                result
            }
            #[cfg(not(feature = "tree-sitter"))]
            {
                let mut output = std::io::sink();
                processor.process_stream(input, &mut output)?
            }
        } else {
            let mut output = std::io::sink();
            processor.process_stream(input, &mut output)?
        };

        if json_output {
            println!("{}", json_schema::output_count_json(&result)?);
        } else {
            println!("{}", result.matches_found);
        }
        return Ok(());
    }

    // Note: `mut` is required when tree-sitter feature is enabled for write_all()
    #[allow(unused_mut)]
    let mut output: Box<dyn io::Write> =
        if let Some(output_file) = matches.get_one::<String>("output") {
            Box::new(File::create(output_file)?)
        } else {
            Box::new(io::stdout())
        };

    let result = if use_syntax_aware {
        #[cfg(feature = "tree-sitter")]
        {
            // Syntax-aware processing: buffer input, process with AST, write output
            let content = read_input_to_string(input)?;
            let (processed, result) = processor.process_file_content(&content, cli_language)?;
            output.write_all(processed.as_bytes())?;
            result
        }
        #[cfg(not(feature = "tree-sitter"))]
        {
            processor.process_stream(input, output)?
        }
    } else {
        // Standard stream processing
        processor.process_stream(input, output)?
    };

    if matches.get_flag("performance") {
        eprintln!("{}", result.performance_summary());
        eprintln!("{}", processor.performance_report());
    }

    if json_output && matches.get_flag("performance") {
        // If both json and performance are requested, output performance as JSON
        eprintln!("{}", json_schema::output_performance_json(&result)?);
    }

    // Output verification summary if requested (includes bidirectional stats)
    if matches.get_flag("verify") {
        // Include bidirectional mapping statistics in verification output
        let bidir_stats = processor.get_bidirectional_stats();
        output_verification_summary(&result, json_output, bidir_stats)?;
    }

    Ok(())
}

/// Read all input from a BufRead into a String.
///
/// Only compiled when the `tree-sitter` feature is enabled, as syntax-aware
/// processing requires buffering the entire file content for AST analysis.
#[cfg(feature = "tree-sitter")]
fn read_input_to_string(mut input: Box<dyn io::BufRead>) -> Result<String> {
    let mut content = String::new();
    input.read_to_string(&mut content)?;
    Ok(content)
}

/// Output a verification summary confirming what transformations were applied
fn output_verification_summary(
    result: &rexpipe::pipeline::PipelineResult,
    json_output: bool,
    bidir_stats: Option<rexpipe::bidirectional::MappingStats>,
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
            #[serde(skip_serializing_if = "Option::is_none")]
            bidirectional: Option<BidirStats>,
        }

        #[derive(serde::Serialize)]
        struct BidirStats {
            total_mappings: usize,
            unique_originals: usize,
            unique_transformed: usize,
            steps_with_mappings: usize,
        }

        let verified = result.lines_processed > 0;
        let status = if result.transformations_applied > 0 {
            "transformations_applied"
        } else if result.matches_found > 0 {
            "matches_found_no_transformations"
        } else {
            "no_matches"
        };

        let bidirectional = bidir_stats
            .as_ref()
            .filter(|s| s.total_mappings > 0)
            .map(|s| BidirStats {
                total_mappings: s.total_mappings,
                unique_originals: s.unique_originals,
                unique_transformed: s.unique_transformed,
                steps_with_mappings: s.steps_with_mappings,
            });

        let verification = VerificationResult {
            status: status.to_string(),
            lines_processed: result.lines_processed,
            matches_found: result.matches_found,
            transformations_applied: result.transformations_applied,
            success_rate: result.success_rate(),
            verified,
            bidirectional,
        };

        let response = json_schema::JsonResponse::new("verify", verification);
        eprintln!("{}", response.to_json()?);
    } else {
        eprintln!("\n--- Verification Summary ---");
        eprintln!("Lines processed: {}", result.lines_processed);
        eprintln!("Matches found: {}", result.matches_found);
        eprintln!(
            "Transformations applied: {}",
            result.transformations_applied
        );
        eprintln!("Success rate: {:.1}%", result.success_rate() * 100.0);

        // Show bidirectional stats if available
        if let Some(ref stats) = bidir_stats {
            if stats.total_mappings > 0 {
                eprintln!("\nBidirectional: {}", stats);
            }
        }

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
        println!(
            "# NOTE: No config file specified. Add -c <config.toml> for transformation rules."
        );
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
    println!(
        "# git config --global filter.{}.clean '{}'",
        filter_name, rexpipe_cmd
    );
    println!("# git config --global filter.{}.smudge 'cat'", filter_name);

    Ok(())
}

/// Run pattern discovery/learning mode
fn run_pattern_discovery(matches: &clap::ArgMatches) -> Result<()> {
    use regex::Regex;
    use std::collections::HashMap;
    use std::io::BufRead;

    // Common pattern templates to search for
    // Note: All patterns are hardcoded and validated at compile time
    let pattern_templates: Vec<(&str, &str, Regex)> = vec![
        (
            "email",
            r"Email addresses",
            Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}")
                .expect("static email pattern"),
        ),
        (
            "ipv4",
            r"IPv4 addresses",
            Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").expect("static ipv4 pattern"),
        ),
        (
            "phone_us",
            r"US phone numbers",
            Regex::new(r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b").expect("static phone pattern"),
        ),
        (
            "date_iso",
            r"ISO dates (YYYY-MM-DD)",
            Regex::new(r"\b\d{4}-\d{2}-\d{2}\b").expect("static date_iso pattern"),
        ),
        (
            "date_us",
            r"US dates (MM/DD/YYYY)",
            Regex::new(r"\b\d{1,2}/\d{1,2}/\d{4}\b").expect("static date_us pattern"),
        ),
        (
            "time_24h",
            r"24-hour time",
            Regex::new(r"\b\d{1,2}:\d{2}(:\d{2})?\b").expect("static time_24h pattern"),
        ),
        (
            "uuid",
            r"UUIDs",
            Regex::new(
                r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b",
            )
            .expect("static uuid pattern"),
        ),
        (
            "hex_id",
            r"Hex identifiers (8+ chars)",
            Regex::new(r"\b[0-9a-fA-F]{8,}\b").expect("static hex_id pattern"),
        ),
        (
            "url",
            r"URLs",
            Regex::new(r#"https?://[^\s<>"']+"#).expect("static url pattern"),
        ),
        (
            "ssn",
            r"SSN-like patterns",
            Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").expect("static ssn pattern"),
        ),
        (
            "credit_card",
            r"Credit card patterns",
            Regex::new(r"\b\d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}\b")
                .expect("static credit_card pattern"),
        ),
        (
            "api_key",
            r"API key patterns",
            Regex::new(r"\b[A-Za-z0-9_-]{20,}\b").expect("static api_key pattern"),
        ),
        (
            "base64_blob",
            r"Base64 blobs (20+ chars)",
            Regex::new(r"\b[A-Za-z0-9+/]{20,}={0,2}\b").expect("static base64_blob pattern"),
        ),
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
                // Use entry API for safe HashMap access
                let entry = pattern_counts.entry(name).or_insert((0, Vec::new()));
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
        return Err(anyhow!(
            "No input provided. Pipe data to stdin or use -f <file>"
        ));
    }

    // Report findings
    println!("Pattern Discovery Report");
    println!("========================");
    println!("Analyzed {} lines\n", total_lines);

    // Sort by count descending
    let mut findings: Vec<_> = pattern_templates
        .iter()
        .filter_map(|(name, desc, regex)| {
            // Safe: only include patterns that have matches
            pattern_counts.get(name).and_then(|(count, examples)| {
                if *count > 0 {
                    Some((name, desc, regex.as_str(), *count, examples.clone()))
                } else {
                    None
                }
            })
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

    // Load positive examples from file
    if let Some(file_path) = matches.get_one::<String>("positive-file") {
        let content = std::fs::read_to_string(file_path).map_err(|e| {
            anyhow!(
                "Failed to read positive examples file '{}': {}",
                file_path,
                e
            )
        })?;
        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') {
                learner.add_positive(line);
            }
        }
    }

    // Load negative examples from file
    if let Some(file_path) = matches.get_one::<String>("negative-file") {
        let content = std::fs::read_to_string(file_path).map_err(|e| {
            anyhow!(
                "Failed to read negative examples file '{}': {}",
                file_path,
                e
            )
        })?;
        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') {
                learner.add_negative(line);
            }
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
        return Err(anyhow!(
            "No examples provided. Use --positive and --negative flags or provide examples via stdin."
        ));
    }

    // Collect examples for testing patterns
    let positive_examples: Vec<String> =
        learner.positive_examples().map(|s| s.to_string()).collect();
    let negative_examples: Vec<String> =
        learner.negative_examples().map(|s| s.to_string()).collect();

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

                    // Show test results for this pattern
                    if let Ok(regex) = regex::Regex::new(&pattern.pattern) {
                        let pos_matches: Vec<_> = positive_examples
                            .iter()
                            .filter(|ex| regex.is_match(ex))
                            .collect();
                        let neg_matches: Vec<_> = negative_examples
                            .iter()
                            .filter(|ex| regex.is_match(ex))
                            .collect();

                        println!(
                            "   Test: ✓ {}/{} positive, ✗ {}/{} negative (false positives)",
                            pos_matches.len(),
                            positive_examples.len(),
                            neg_matches.len(),
                            negative_examples.len()
                        );

                        // Show false positives (negatives that match)
                        if !neg_matches.is_empty() {
                            println!(
                                "   ⚠ False positives: {}",
                                neg_matches
                                    .iter()
                                    .map(|s| format!("\"{}\"", s))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            );
                        }
                    }
                    println!();
                }

                // Generate pipeline config
                let config_toml = rexpipe::learn::generate_pipeline_config(&patterns);

                // Save to file or print
                if let Some(output_path) = matches.get_one::<String>("learn-output") {
                    std::fs::write(output_path, &config_toml)
                        .map_err(|e| anyhow!("Failed to write to '{}': {}", output_path, e))?;
                    println!("Pipeline saved to: {}", output_path);
                    println!("Use with: rexpipe -c {}", output_path);
                } else {
                    println!("Suggested pipeline configuration:");
                    println!("{}", config_toml);
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
        return Err(anyhow!(
            "No tests defined in pipeline configuration. Add [[tests]] sections to define test cases."
        ));
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
                Ok((
                    output_str,
                    result.matches_found,
                    result.transformations_applied,
                ))
            }
            Err(e) => Err(format!("Processing error: {}", e)),
        }
    };

    let summary = runner.run_all(processor);

    // Get output format
    let format = matches
        .get_one::<String>("test-format")
        .map(|s| s.as_str())
        .unwrap_or("text");

    match format {
        "tap" => {
            println!("{}", rexpipe::testing::format_tap_output(&summary));
        }
        "junit" => {
            println!(
                "{}",
                rexpipe::testing::format_junit_xml(&summary, "rexpipe")
            );
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

/// Watch mode implementation - re-runs pipeline when input files change.
///
/// Requires the `watch` feature to be enabled.
#[cfg(feature = "watch")]
fn run_watch_mode(
    matches: &clap::ArgMatches,
    paths: &[String],
    config: &PipelineConfig,
) -> Result<()> {
    use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
    use std::sync::mpsc::channel;
    use std::time::Duration;

    println!("Watch mode enabled. Watching for changes...");
    println!("Press Ctrl+C to exit.\n");

    // Create a channel to receive file system events
    let (tx, rx) = channel();

    // Create a watcher
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;

    // Watch each path
    for path in paths {
        let path = std::path::Path::new(path);
        if path.exists() {
            watcher.watch(path, RecursiveMode::NonRecursive)?;
            println!("Watching: {}", path.display());
        } else {
            eprintln!("Warning: Path does not exist: {}", path.display());
        }
    }

    println!();

    // Run initial processing
    let _ = process_files_with_config(matches, paths, config);

    // Wait for events
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                // Only react to modify/create events
                match event.kind {
                    notify::EventKind::Modify(_) | notify::EventKind::Create(_) => {
                        println!("\n--- File changed, re-running pipeline ---\n");
                        // Small delay to let file writes complete
                        std::thread::sleep(Duration::from_millis(100));
                        let _ = process_files_with_config(matches, paths, config);
                    }
                    _ => {}
                }
            }
            Ok(Err(e)) => {
                eprintln!("Watch error: {:?}", e);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Check for Ctrl+C
                continue;
            }
            Err(e) => {
                eprintln!("Channel error: {:?}", e);
                break;
            }
        }
    }

    Ok(())
}

/// Stub for when watch feature is disabled.
#[cfg(not(feature = "watch"))]
fn run_watch_mode(
    _matches: &clap::ArgMatches,
    _paths: &[String],
    _config: &PipelineConfig,
) -> Result<()> {
    Err(anyhow::anyhow!(
        "Watch mode requires the 'watch' feature.\n\
         Install with: cargo install rexpipe --features watch"
    ))
}

/// Helper to process files with a given config (for watch mode).
#[cfg(feature = "watch")]
fn process_files_with_config(
    matches: &clap::ArgMatches,
    paths: &[String],
    config: &PipelineConfig,
) -> Result<()> {
    let quiet = matches.get_flag("quiet");
    let in_place = matches.get_flag("in-place");

    for path_str in paths {
        let path = std::path::Path::new(path_str);
        if !path.exists() {
            continue;
        }

        if path.is_file() {
            let mut processor = rexpipe::processor::StreamProcessor::new(config.clone())?;
            let content = std::fs::read_to_string(path)?;
            let reader = std::io::BufReader::new(content.as_bytes());
            let mut output = Vec::new();
            let _ = processor.process_stream(reader, &mut output)?;
            let result = String::from_utf8_lossy(&output);

            if in_place {
                std::fs::write(path, result.as_bytes())?;
                if !quiet {
                    eprintln!("Updated: {}", path.display());
                }
            } else {
                print!("{}", result);
            }
        }
    }

    Ok(())
}

/// Run atomic multi-file processing with rollback on failure.
///
/// This processes all files to temporary files first, then only commits
/// the changes if all files succeed. If any file fails, all temp files
/// are deleted (rollback).
fn run_atomic_multi_file_processing(
    config: &PipelineConfig,
    _processor: &MultiFileProcessor,
    files: &[PathBuf],
    options: &FileProcessingOptions,
    checkpoint: &mut Checkpoint,
    quiet: bool,
    json_output: bool,
) -> Result<()> {
    use std::io::Write;

    if !quiet {
        eprintln!(
            "Atomic mode: processing {} files to staging...",
            files.len()
        );
    }

    // Create temp directory for staging
    let temp_dir = std::env::temp_dir().join(format!("rexpipe_atomic_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir)?;

    // Track staged files: (original_path, temp_path)
    let mut staged: Vec<(PathBuf, PathBuf)> = Vec::with_capacity(files.len());
    let mut results: Vec<(PathBuf, Result<String>)> = Vec::new();
    let mut all_success = true;

    // Stage 1: Process all files to temp directory
    for file in files {
        let temp_name = format!("{}", file.file_name().unwrap_or_default().to_string_lossy());
        let temp_path = temp_dir.join(&temp_name);

        // Read and process file
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                results.push((file.clone(), Err(anyhow!("Failed to read: {}", e))));
                all_success = false;
                continue;
            }
        };

        // Process through pipeline
        let mut proc = StreamProcessor::new(config.clone())?;
        let mut output = Vec::new();
        let cursor = std::io::Cursor::new(content.as_bytes());
        let reader = std::io::BufReader::new(cursor);

        match proc.process_stream(reader, &mut output) {
            Ok(_result) => {
                // Write to temp file
                match std::fs::File::create(&temp_path) {
                    Ok(mut f) => {
                        if let Err(e) = f.write_all(&output) {
                            results
                                .push((file.clone(), Err(anyhow!("Failed to write temp: {}", e))));
                            all_success = false;
                            continue;
                        }
                        staged.push((file.clone(), temp_path.clone()));
                        results.push((
                            file.clone(),
                            Ok(String::from_utf8_lossy(&output).to_string()),
                        ));
                    }
                    Err(e) => {
                        results.push((file.clone(), Err(anyhow!("Failed to create temp: {}", e))));
                        all_success = false;
                    }
                }
            }
            Err(e) => {
                results.push((file.clone(), Err(e)));
                all_success = false;
            }
        }
    }

    // Stage 2: Commit or rollback
    if all_success && !staged.is_empty() {
        if !quiet {
            eprintln!(
                "Atomic mode: all {} files processed successfully, committing...",
                staged.len()
            );
        }

        // Create backups if requested
        let backup_suffix = options.backup_suffix.as_deref();

        for (original, temp) in &staged {
            // Create backup if suffix provided
            if let Some(suffix) = backup_suffix {
                let backup_path = PathBuf::from(format!("{}{}", original.display(), suffix));
                if let Err(e) = std::fs::copy(original, &backup_path) {
                    eprintln!(
                        "Warning: Failed to create backup for {}: {}",
                        original.display(),
                        e
                    );
                }
            }

            // Move temp to original
            if std::fs::rename(temp, original).is_err() {
                // If rename fails (cross-device), try copy+delete
                if let Err(e) = std::fs::copy(temp, original) {
                    eprintln!("Error: Failed to commit {}: {}", original.display(), e);
                } else {
                    let _ = std::fs::remove_file(temp);
                }
            }
        }

        if !quiet {
            eprintln!("Atomic mode: {} files committed successfully", staged.len());
        }

        // Update checkpoint
        if checkpoint.is_enabled() {
            for file in files {
                if let Ok(metadata) = std::fs::metadata(file) {
                    checkpoint.update_file_state(file, metadata.len(), 0, metadata.len());
                }
            }
            checkpoint
                .save()
                .map_err(|e| anyhow!("Failed to save checkpoint: {}", e))?;
        }
    } else {
        // Rollback: delete all temp files
        if !quiet {
            eprintln!("Atomic mode: errors detected, rolling back...");
        }

        for (_, temp) in &staged {
            let _ = std::fs::remove_file(temp);
        }

        // Clean up temp directory
        let _ = std::fs::remove_dir(&temp_dir);

        // Report errors
        if json_output {
            let errors: Vec<_> = results
                .iter()
                .filter_map(|(path, result)| {
                    result.as_ref().err().map(|e| {
                        serde_json::json!({
                            "file": path.display().to_string(),
                            "error": e.to_string()
                        })
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::json!({
                    "status": "rollback",
                    "errors": errors
                })
            );
        } else {
            for (path, result) in &results {
                if let Err(e) = result {
                    eprintln!("  Error in {}: {}", path.display(), e);
                }
            }
        }

        return Err(anyhow!("Atomic operation failed, changes rolled back"));
    }

    // Clean up temp directory
    let _ = std::fs::remove_dir(&temp_dir);

    Ok(())
}

/// Validate a commit message against the Conventional Commits specification.
///
/// Format: <type>[optional scope]: <description>
///
/// Valid types: feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert
fn run_conventional_commits_validation(matches: &clap::ArgMatches) -> Result<()> {
    use std::io::BufRead;

    // Read commit message from input
    let input: Box<dyn io::BufRead> = if let Some(input_file) = matches.get_one::<String>("input") {
        Box::new(BufReader::new(File::open(input_file)?))
    } else {
        Box::new(io::stdin().lock())
    };

    let mut commit_msg = String::new();
    for line in input.lines() {
        let line = line?;
        // Skip comment lines (git commit message comments)
        if line.starts_with('#') {
            continue;
        }
        if !commit_msg.is_empty() {
            commit_msg.push('\n');
        }
        commit_msg.push_str(&line);
    }

    let commit_msg = commit_msg.trim();

    if commit_msg.is_empty() {
        eprintln!("Error: Empty commit message");
        std::process::exit(exit_codes::VALIDATION_ERROR);
    }

    // Parse the first line (subject line)
    let first_line = commit_msg.lines().next().unwrap_or("");

    // Conventional Commits pattern:
    // ^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(\(.+\))?(!)?: .+$
    let valid_types = [
        "feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore",
        "revert",
    ];

    // Parse type
    let type_end = first_line.find(['(', ':', '!']);
    let commit_type = match type_end {
        Some(idx) => &first_line[..idx],
        None => {
            eprintln!("Error: Invalid commit message format");
            eprintln!("Expected: <type>[scope]: <description>");
            eprintln!("Got: {}", first_line);
            std::process::exit(exit_codes::VALIDATION_ERROR);
        }
    };

    if !valid_types.contains(&commit_type) {
        eprintln!("Error: Invalid commit type: '{}'", commit_type);
        eprintln!("Valid types: {}", valid_types.join(", "));
        std::process::exit(exit_codes::VALIDATION_ERROR);
    }

    let rest = &first_line[commit_type.len()..];

    // Parse optional scope
    let (scope, rest) = if rest.starts_with('(') {
        if let Some(close_paren) = rest.find(')') {
            let scope = &rest[1..close_paren];
            if scope.is_empty() {
                eprintln!("Error: Empty scope in parentheses");
                std::process::exit(exit_codes::VALIDATION_ERROR);
            }
            (Some(scope), &rest[close_paren + 1..])
        } else {
            eprintln!("Error: Unclosed parenthesis in scope");
            std::process::exit(exit_codes::VALIDATION_ERROR);
        }
    } else {
        (None, rest)
    };

    // Check for breaking change indicator
    let (is_breaking, rest) = match rest.strip_prefix('!') {
        Some(stripped) => (true, stripped),
        None => (false, rest),
    };

    // Require colon and space
    if !rest.starts_with(": ") {
        eprintln!("Error: Missing ': ' after type/scope");
        eprintln!("Expected: <type>[scope]: <description>");
        eprintln!("Got: {}", first_line);
        std::process::exit(exit_codes::VALIDATION_ERROR);
    }

    let description = rest[2..].trim();

    if description.is_empty() {
        eprintln!("Error: Empty description");
        std::process::exit(exit_codes::VALIDATION_ERROR);
    }

    // Check for breaking change in footer
    let has_breaking_footer =
        commit_msg.contains("BREAKING CHANGE:") || commit_msg.contains("BREAKING-CHANGE:");

    // Success - output parsed information in JSON if requested
    let use_json = matches.get_flag("json") || !io::stdout().is_terminal();

    if use_json {
        let output = serde_json::json!({
            "valid": true,
            "type": commit_type,
            "scope": scope,
            "description": description,
            "breaking": is_breaking || has_breaking_footer,
            "subject": first_line,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if !matches.get_flag("quiet") {
        println!("✓ Valid conventional commit");
        println!("  Type: {}", commit_type);
        if let Some(s) = scope {
            println!("  Scope: {}", s);
        }
        println!("  Description: {}", description);
        if is_breaking || has_breaking_footer {
            println!("  ⚠ Breaking change");
        }
    }

    Ok(())
}

/// Run streaming mode with real-time aggregation.
fn run_streaming_mode(
    config: &PipelineConfig,
    input: Box<dyn io::BufRead>,
    matches: &clap::ArgMatches,
) -> Result<()> {
    use std::collections::HashMap;
    use std::io::{BufRead, Write};
    use std::time::{Duration, Instant};

    let interval_secs = matches
        .get_one::<u64>("stream-interval")
        .copied()
        .unwrap_or(5);
    let interval = Duration::from_secs(interval_secs);
    let use_json = matches.get_flag("json") || matches.get_flag("jsonl");
    let quiet = matches.get_flag("quiet");

    // Aggregation counters
    let mut match_counts: HashMap<String, u64> = HashMap::new();
    let mut total_lines: u64 = 0;
    let mut total_matches: u64 = 0;
    let mut last_summary = Instant::now();
    let start_time = Instant::now();

    // Initialize counters for each step
    for (idx, step) in config.step.iter().enumerate() {
        let step_name = step.description.clone().unwrap_or_else(|| {
            if !step.pattern.is_empty() {
                step.pattern.clone()
            } else {
                format!("step-{}", idx)
            }
        });
        match_counts.insert(step_name, 0);
    }

    // Build compiled patterns for matching
    let patterns: Vec<(String, Option<regex::Regex>)> = config
        .step
        .iter()
        .map(|step| {
            let name = step
                .description
                .clone()
                .unwrap_or_else(|| step.pattern.clone());
            let re = if !step.pattern.is_empty() {
                regex::Regex::new(&step.pattern).ok()
            } else {
                None
            };
            (name, re)
        })
        .collect();

    // Process lines manually for streaming with aggregation
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();

    for line_result in input.lines() {
        let line = line_result?;
        total_lines += 1;

        // Process line through a fresh processor for each line
        // (for simplicity - in production you'd want to batch)
        let mut processor = StreamProcessor::new(config.clone())?;
        let mut output = Vec::new();
        let cursor = std::io::Cursor::new(line.as_bytes());
        let reader = std::io::BufReader::new(cursor);
        let _ = processor.process_stream(reader, &mut output)?;
        let result = String::from_utf8_lossy(&output);
        let result = result.trim_end();

        // Count matches
        if result != line {
            total_matches += 1;
            // Try to identify which step matched
            for (name, re_opt) in &patterns {
                if let Some(re) = re_opt {
                    if re.is_match(&line) {
                        *match_counts.entry(name.clone()).or_insert(0) += 1;
                    }
                }
            }
        }

        // Output the processed line
        if !quiet {
            writeln!(stdout_lock, "{}", result)?;
        }

        // Periodic summary
        if last_summary.elapsed() >= interval {
            let elapsed = start_time.elapsed();

            if use_json {
                let summary = serde_json::json!({
                    "type": "summary",
                    "elapsed_secs": elapsed.as_secs(),
                    "total_lines": total_lines,
                    "total_matches": total_matches,
                    "match_rate": if total_lines > 0 {
                        (total_matches as f64 / total_lines as f64) * 100.0
                    } else { 0.0 },
                    "matches_by_pattern": match_counts,
                });
                eprintln!("{}", serde_json::to_string(&summary)?);
            } else {
                eprintln!("--- Streaming Summary ({:?} elapsed) ---", elapsed);
                eprintln!("  Lines processed: {}", total_lines);
                eprintln!(
                    "  Total matches: {} ({:.1}%)",
                    total_matches,
                    if total_lines > 0 {
                        (total_matches as f64 / total_lines as f64) * 100.0
                    } else {
                        0.0
                    }
                );
                if !match_counts.is_empty() {
                    eprintln!("  By pattern:");
                    for (name, count) in &match_counts {
                        if *count > 0 {
                            eprintln!("    {}: {}", name, count);
                        }
                    }
                }
                eprintln!("---");
            }

            last_summary = Instant::now();
        }
    }

    // Final summary
    let elapsed = start_time.elapsed();

    if use_json {
        let summary = serde_json::json!({
            "type": "final_summary",
            "elapsed_secs": elapsed.as_secs_f64(),
            "total_lines": total_lines,
            "total_matches": total_matches,
            "match_rate": if total_lines > 0 {
                (total_matches as f64 / total_lines as f64) * 100.0
            } else { 0.0 },
            "matches_by_pattern": match_counts,
        });
        eprintln!("{}", serde_json::to_string_pretty(&summary)?);
    } else if !quiet {
        eprintln!("\n=== Final Summary ===");
        eprintln!("  Total time: {:.2}s", elapsed.as_secs_f64());
        eprintln!("  Lines processed: {}", total_lines);
        eprintln!(
            "  Total matches: {} ({:.1}%)",
            total_matches,
            if total_lines > 0 {
                (total_matches as f64 / total_lines as f64) * 100.0
            } else {
                0.0
            }
        );
        if total_lines > 0 {
            eprintln!(
                "  Lines/sec: {:.0}",
                total_lines as f64 / elapsed.as_secs_f64().max(0.001)
            );
        }
    }

    Ok(())
}

/// Generate test data from pipeline patterns.
fn run_test_data_generation(
    config: &PipelineConfig,
    count: u32,
    matches: &clap::ArgMatches,
) -> Result<()> {
    use rand::Rng;

    let use_json = matches.get_flag("json");
    let mut generated: Vec<String> = Vec::with_capacity(count as usize);
    let mut rng = rand::thread_rng();

    // Extract patterns from steps (only steps with non-empty patterns)
    let patterns: Vec<(&str, Option<&str>)> = config
        .step
        .iter()
        .filter(|step| !step.pattern.is_empty())
        .map(|step| (step.pattern.as_str(), step.description.as_deref()))
        .collect();

    if patterns.is_empty() {
        return Err(anyhow!("No patterns found in pipeline configuration"));
    }

    for i in 0..count {
        // Pick a random pattern to generate from
        let (pattern, description) = patterns[rng.gen_range(0..patterns.len())];

        // Generate a sample that matches the pattern
        // This is a simplified generator - for complex patterns, we generate approximations
        let sample = generate_sample_from_pattern(pattern, i, &mut rng);

        if use_json {
            generated.push(
                serde_json::json!({
                    "index": i,
                    "pattern": pattern,
                    "description": description,
                    "sample": sample,
                })
                .to_string(),
            );
        } else {
            generated.push(sample);
        }
    }

    if use_json {
        println!("[{}]", generated.join(",\n"));
    } else {
        for sample in generated {
            println!("{}", sample);
        }
    }

    Ok(())
}

/// Generate a sample string that approximately matches a regex pattern.
/// This is a simplified generator for common pattern types.
fn generate_sample_from_pattern(pattern: &str, index: u32, rng: &mut impl rand::Rng) -> String {
    // Common pattern substitutions
    let sample = pattern
        // Digit patterns
        .replace(r"\d+", &format!("{}", rng.gen_range(100..9999)))
        .replace(r"\d{3}", &format!("{:03}", rng.gen_range(0..999)))
        .replace(r"\d{4}", &format!("{:04}", rng.gen_range(0..9999)))
        .replace(r"\d{2}", &format!("{:02}", rng.gen_range(0..99)))
        .replace(r"\d", &format!("{}", rng.gen_range(0..9)))
        // Word patterns
        .replace(r"\w+", &format!("word{}", index))
        .replace(r"\w*", &format!("text{}", rng.gen_range(0..100)))
        // Space patterns
        .replace(r"\s+", " ")
        .replace(r"\s*", " ")
        .replace(r"\s", " ")
        // Common anchors
        .replace("^", "")
        .replace("$", "")
        // Character classes
        .replace(r"[a-zA-Z]+", &format!("Sample{}", index))
        .replace(r"[a-z]+", "example")
        .replace(r"[A-Z]+", "EXAMPLE")
        .replace(r"[0-9]+", &format!("{}", rng.gen_range(1000..9999)))
        // Email-like patterns
        .replace(
            r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}",
            &format!("user{}@example.com", index),
        )
        // IP-like patterns
        .replace(
            r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}",
            &format!(
                "192.168.{}.{}",
                rng.gen_range(0..255),
                rng.gen_range(1..255)
            ),
        )
        // UUID-like patterns
        .replace(
            r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
            &format!(
                "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
                rng.gen_range(0u32..u32::MAX),
                rng.gen_range(0u16..u16::MAX),
                rng.gen_range(0u16..u16::MAX),
                rng.gen_range(0u16..u16::MAX),
                rng.gen_range(0u64..0xffffffffffff)
            ),
        )
        // Quantifiers
        .replace("+", "")
        .replace("*", "")
        .replace("?", "")
        // Groups
        .replace("(", "")
        .replace(")", "")
        .replace("|", "")
        // Escape sequences
        .replace(r"\\", "\\")
        .replace(r"\.", ".")
        .replace(r"\-", "-");

    // If the result still contains regex metacharacters, return a simpler sample
    if sample.contains(r"\") || sample.contains('[') || sample.contains('{') {
        format!("sample_data_{}", index)
    } else {
        sample
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
}
