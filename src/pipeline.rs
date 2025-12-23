//! Pipeline configuration and step definitions.
//!
//! This module defines the core data structures for rexpipe pipelines:
//! - `PipelineConfig`: The root configuration containing steps and settings
//! - `PipelineStep`: Individual processing steps (substitute, filter, etc.)
//! - `PipelineSettings`: Global options like timeouts and regex modes
//!
//! # Configuration Formats
//!
//! Pipelines can be defined in TOML or JSON format, or created programmatically.
//!
//! # Examples
//!
//! ## Creating a Pipeline Programmatically
//!
//! ```rust
//! use rexpipe::pipeline::{PipelineConfig, PipelineStep, StepType};
//!
//! // Create from inline pattern (simplest approach)
//! let config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUMBER"));
//! assert_eq!(config.step.len(), 1);
//! ```
//!
//! ## Loading from TOML
//!
//! ```rust
//! use rexpipe::pipeline::PipelineConfig;
//!
//! let toml = r#"
//! name = "example"
//! description = "Replace numbers with NUM"
//!
//! [[step]]
//! type = "substitute"
//! pattern = '\d+'
//! replacement = "NUM"
//! "#;
//!
//! let config: PipelineConfig = toml::from_str(toml).unwrap();
//! assert_eq!(config.name, Some("example".to_string()));
//! ```
//!
//! ## Multi-Step Pipeline
//!
//! ```rust
//! use rexpipe::pipeline::PipelineConfig;
//!
//! let toml = r#"
//! [[step]]
//! type = "filter"
//! pattern = "DEBUG"
//! action = "drop_line"
//!
//! [[step]]
//! type = "substitute"
//! pattern = "ERROR"
//! replacement = "[ERROR]"
//! "#;
//!
//! let config: PipelineConfig = toml::from_str(toml).unwrap();
//! assert_eq!(config.step.len(), 2);
//! ```
//!
//! # Step Types
//!
//! | Type | Description | Key Fields |
//! |------|-------------|------------|
//! | `substitute` | Replace pattern matches | `pattern`, `replacement` |
//! | `filter` | Keep/drop lines | `pattern`, `action` |
//! | `extract` | Extract matched portions | `pattern`, `output_format` |
//! | `validate` | Assert pattern presence | `pattern`, `on_mismatch` |
//! | `transform` | Transform matched text | `pattern`, `transform` |
//! | `block` | Multi-line processing | `pattern`, `end_pattern`, `action` |

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::bidirectional::BidirectionalConfig;
use crate::checkpoint::CheckpointConfig;
use crate::crossfile::CrossFileConfig;
use crate::testing::TestCase;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    /// Base pipeline configuration to extend (inherits steps and settings)
    /// Steps from the base are prepended to this config's steps
    #[serde(default)]
    pub extends: Option<String>,
    /// Pattern libraries to include (supports ${pattern_name} references in steps)
    #[serde(default)]
    pub patterns_include: Vec<String>,
    #[serde(default)]
    pub settings: PipelineSettings,
    #[serde(default)]
    pub step: Vec<PipelineStep>,

    // === Advanced feature configurations ===
    /// Bidirectional (reversible) pipeline configuration
    #[serde(default)]
    pub bidirectional: BidirectionalConfig,

    /// Checkpoint configuration for incremental processing
    #[serde(default)]
    pub checkpoint: CheckpointConfig,

    /// Cross-file relationship processing configuration
    #[serde(default)]
    pub cross_file: CrossFileConfig,

    /// Inline test cases for pipeline validation
    #[serde(default, rename = "test")]
    pub tests: Vec<TestCase>,

    /// Finalize section for post-processing aggregation and summary.
    /// Runs after all input lines have been processed.
    #[serde(default)]
    pub finalize: FinalizeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineSettings {
    /// Use PCRE-compatible regex engine via fancy-regex (requires pcre feature)
    #[serde(default)]
    pub pcre_mode: bool,
    /// Treat patterns as fixed strings (no regex interpretation)
    #[serde(default)]
    pub fixed_strings: bool,
    /// Number of context lines to show before matches
    #[serde(default)]
    pub context_before: usize,
    /// Number of context lines to show after matches
    #[serde(default)]
    pub context_after: usize,
    /// Timeout in milliseconds for regex matching per line (0 = no timeout)
    #[serde(default)]
    pub timeout_ms: u64,
    /// Allow shell command execution in transforms.
    ///
    /// **Security**: Defaults to `false` to prevent command injection when processing
    /// untrusted input. Set to `true` explicitly in config or via CLI to enable shell
    /// transforms. When enabled, shell commands receive input via stdin (not interpolation)
    /// for safety, but the command string itself from the config is executed.
    #[serde(default = "default_allow_shell")]
    pub allow_shell: bool,
    /// Strict mode - reject patterns with potential ReDoS vulnerabilities
    #[serde(default)]
    pub strict_mode: bool,
    /// Enable block processing mode.
    /// When true, the pipeline operates on multi-line blocks rather than individual lines.
    #[serde(default)]
    pub block_mode: bool,
    /// Preserve CRLF line endings in in-place editing mode
    ///
    /// When true, the processor detects and preserves the original line ending
    /// style (LF or CRLF) for each line. When false (default), all output uses
    /// LF line endings regardless of input.
    #[serde(default)]
    pub preserve_line_endings: bool,
    /// Maximum line length in bytes (0 = no limit)
    ///
    /// Lines exceeding this limit will be handled according to `max_line_action`.
    /// This prevents memory issues when processing files with very long lines
    /// (e.g., minified JavaScript). Default: 0 (no limit).
    #[serde(default)]
    pub max_line_length: usize,
    /// Action to take when a line exceeds `max_line_length`
    ///
    /// - "skip": Skip the line entirely (default)
    /// - "error": Return an error
    /// - "truncate": Truncate the line at the limit
    #[serde(default)]
    pub max_line_action: MaxLineAction,
    /// Timeout in seconds for shell transform commands (0 = no timeout).
    ///
    /// Shell transforms execute external commands which may hang. This timeout
    /// prevents indefinite hangs. Default: 30 seconds.
    #[serde(default = "default_shell_timeout")]
    pub shell_timeout_secs: u64,
    /// Maximum regex pattern size in bytes for ReDoS protection.
    ///
    /// Patterns exceeding this size will be rejected to prevent memory exhaustion.
    /// Default: 10MB (10 * 1024 * 1024 bytes).
    #[serde(default = "default_regex_size_limit")]
    pub regex_size_limit: usize,
}

impl Default for PipelineSettings {
    fn default() -> Self {
        Self {
            pcre_mode: false,
            fixed_strings: false,
            context_before: 0,
            context_after: 0,
            timeout_ms: 0,
            allow_shell: default_allow_shell(),
            strict_mode: false,
            block_mode: false,
            preserve_line_endings: false,
            max_line_length: 0,
            max_line_action: MaxLineAction::default(),
            shell_timeout_secs: default_shell_timeout(),
            regex_size_limit: default_regex_size_limit(),
        }
    }
}

impl PipelineSettings {
    /// Merge with a base config - this config's non-default values override base
    pub fn merge_with_base(self, base: PipelineSettings) -> PipelineSettings {
        let default = PipelineSettings::default();
        PipelineSettings {
            pcre_mode: if self.pcre_mode != default.pcre_mode {
                self.pcre_mode
            } else {
                base.pcre_mode
            },
            fixed_strings: if self.fixed_strings != default.fixed_strings {
                self.fixed_strings
            } else {
                base.fixed_strings
            },
            context_before: if self.context_before != default.context_before {
                self.context_before
            } else {
                base.context_before
            },
            context_after: if self.context_after != default.context_after {
                self.context_after
            } else {
                base.context_after
            },
            timeout_ms: if self.timeout_ms != default.timeout_ms {
                self.timeout_ms
            } else {
                base.timeout_ms
            },
            allow_shell: if self.allow_shell != default.allow_shell {
                self.allow_shell
            } else {
                base.allow_shell
            },
            strict_mode: if self.strict_mode != default.strict_mode {
                self.strict_mode
            } else {
                base.strict_mode
            },
            block_mode: if self.block_mode != default.block_mode {
                self.block_mode
            } else {
                base.block_mode
            },
            preserve_line_endings: if self.preserve_line_endings != default.preserve_line_endings {
                self.preserve_line_endings
            } else {
                base.preserve_line_endings
            },
            max_line_length: if self.max_line_length != default.max_line_length {
                self.max_line_length
            } else {
                base.max_line_length
            },
            max_line_action: if self.max_line_action != default.max_line_action {
                self.max_line_action
            } else {
                base.max_line_action
            },
            shell_timeout_secs: if self.shell_timeout_secs != default.shell_timeout_secs {
                self.shell_timeout_secs
            } else {
                base.shell_timeout_secs
            },
            regex_size_limit: if self.regex_size_limit != default.regex_size_limit {
                self.regex_size_limit
            } else {
                base.regex_size_limit
            },
        }
    }
}

fn default_shell_timeout() -> u64 {
    30
}

fn default_regex_size_limit() -> usize {
    10 * 1024 * 1024
}

fn default_allow_shell() -> bool {
    // Security: Shell transforms are disabled by default to prevent command injection
    // when processing untrusted input. Enable explicitly with allow_shell = true in config.
    false
}

// =============================================================================
// Finalize Configuration - Post-processing aggregation and summary
// =============================================================================

/// Configuration for the finalize section - runs after all input is processed.
///
/// The finalize section enables aggregation operations that require seeing all input
/// before producing output, such as counting matches, computing statistics, or
/// producing summaries.
///
/// # Example
///
/// ```toml
/// [finalize]
/// template = """
/// === Summary ===
/// Total errors: ${count:errors}
/// Unique IPs: ${count:unique_ips}
/// """
///
/// [[finalize.counters]]
/// name = "errors"
/// pattern = "ERROR|FATAL"
///
/// [[finalize.counters]]
/// name = "unique_ips"
/// pattern = "^(\\d+\\.\\d+\\.\\d+\\.\\d+)"
/// deduplicate = true
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FinalizeConfig {
    /// Output template with counter references like ${count:COUNTER_NAME}
    ///
    /// Available template variables:
    /// - `${count:NAME}` - Value of counter named NAME
    /// - `${lines}` - Total lines processed
    /// - `${matches}` - Total pattern matches across all steps
    /// - `${transformations}` - Total transformations applied
    #[serde(default)]
    pub template: Option<String>,

    /// Shell command to run after processing, receiving accumulated output via stdin.
    /// Requires `settings.allow_shell = true` in the pipeline config.
    #[serde(default)]
    pub shell: Option<String>,

    /// Counters to track during processing
    #[serde(default)]
    pub counters: Vec<CounterConfig>,

    /// Whether to suppress normal line output and only show finalize output.
    /// When true, lines are processed but not output; only the finalize template is shown.
    /// Default: false (normal output plus finalize summary)
    #[serde(default)]
    pub suppress_output: bool,

    /// Output format for finalize: "text" (default), "json"
    #[serde(default)]
    pub output_format: Option<FinalizeOutputFormat>,
}

/// Output format for finalize section
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FinalizeOutputFormat {
    #[default]
    Text,
    Json,
}

/// Configuration for a counter that tracks pattern matches during processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterConfig {
    /// Name of the counter (used in template as ${count:NAME})
    pub name: String,

    /// Pattern to match for incrementing the counter.
    /// Each line matching this pattern increments the counter.
    pub pattern: String,

    /// If true, only count unique matched values (deduplication).
    /// The first capture group is used for uniqueness; if no capture group,
    /// the entire match is used.
    /// Default: false (count all matches)
    #[serde(default)]
    pub deduplicate: bool,

    /// Optional description of what this counter tracks
    #[serde(default)]
    pub description: Option<String>,

    /// Regex flags for the pattern
    #[serde(default)]
    pub flags: Option<Vec<RegexFlag>>,

    /// If true, extract and store matched values (available in JSON output).
    /// Default: false (only count, don't store values)
    #[serde(default)]
    pub collect_values: bool,

    /// Maximum number of values to collect when collect_values is true.
    /// Default: 1000 (prevents memory exhaustion)
    #[serde(default = "default_max_collected_values")]
    pub max_collected_values: usize,
}

fn default_max_collected_values() -> usize {
    1000
}

impl FinalizeConfig {
    /// Check if finalize is configured (has any meaningful configuration)
    pub fn is_configured(&self) -> bool {
        self.template.is_some() || self.shell.is_some() || !self.counters.is_empty()
    }

    /// Get counter names
    pub fn counter_names(&self) -> Vec<&str> {
        self.counters.iter().map(|c| c.name.as_str()).collect()
    }
}

/// Action to take when a line exceeds the maximum length
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MaxLineAction {
    /// Skip lines exceeding the limit (output unchanged, log warning)
    #[default]
    Skip,
    /// Return an error when a line exceeds the limit
    Error,
    /// Truncate lines at the limit and continue processing
    Truncate,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PipelineStep {
    #[serde(rename = "type", default)]
    pub step_type: StepType,
    /// Pattern to match (for non-block steps: substitute, filter, extract, validate, transform)
    #[serde(default)]
    pub pattern: String,
    #[serde(default)]
    pub replacement: Option<String>,
    /// Unified action field - works for Filter, Block, and other step types.
    /// For Filter steps: "keep_line", "drop_line", "keep_match", "drop_match", "deduplicate_by_prefix"
    /// For Block steps: "keep_block", "drop_block", "collect_block", "deduplicate"
    #[serde(default)]
    pub action: Option<StepAction>,
    /// Transform action for Transform step type
    #[serde(default)]
    pub transform: Option<TransformAction>,
    #[serde(default)]
    pub flags: Option<Vec<RegexFlag>>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    // === Block step fields ===
    /// Start pattern for Block step type (marks beginning of block)
    #[serde(default)]
    pub start_pattern: Option<String>,
    /// End pattern for Block step type (marks end of block)
    #[serde(default)]
    pub end_pattern: Option<String>,
    /// Block context configuration - can be a simple number (lines) or structured config
    /// Examples: `block_context = 5` or `block_context = { overlap_chars = 100 }`
    #[serde(default)]
    pub block_context: Option<BlockContextValue>,
    // === Validation step fields ===
    /// Action to take when validation fails: "error" (default), "warn", "skip"
    #[serde(default)]
    pub on_mismatch: Option<OnMismatch>,
    // === Syntax-aware processing fields (requires tree-sitter feature) ===
    /// Language for syntax-aware processing (e.g., "rust", "python", "javascript")
    /// When specified, patterns are matched only within the specified scope.
    /// Use `language` for a single language or `languages` for multiple.
    #[serde(default)]
    pub language: Option<String>,
    /// Multiple languages for syntax-aware processing.
    /// When specified, the step is applied to files matching any of these languages.
    /// Example: `languages = ["rust", "python", "typescript"]`
    #[serde(default)]
    pub languages: Option<Vec<String>>,
    /// Scope filter for syntax-aware matching:
    /// - "all" or "*": Match anywhere (default)
    /// - "code": Match only in code, excluding strings and comments
    /// - "strings": Match only in string literals
    /// - "comments": Match only in comments
    /// - "functions": Match only in function/method bodies
    #[serde(default)]
    pub scope: Option<String>,
    /// Scopes to exclude from matching.
    /// Example: `exclude_scopes = ["comments", "strings"]`
    #[serde(default)]
    pub exclude_scopes: Option<Vec<String>>,
    // === Extract step enhancements ===
    /// Names for capture groups in extract steps.
    /// When specified, extracted captures are labeled with these names.
    /// Example: `capture_names = ["user", "domain"]` for pattern `(\w+)@(\w+)`
    #[serde(default)]
    pub capture_names: Option<Vec<String>>,
    /// Output format for extract steps: "text", "json", "csv", "jsonl"
    /// Default is "text" (tab-separated matches)
    #[serde(default)]
    pub output_format: Option<ExtractOutputFormat>,
    /// Custom output template using capture group references.
    /// Example: `output_template = "User: $1, Domain: $2"`
    #[serde(default)]
    pub output_template: Option<String>,
    /// Only match first occurrence (default: false, match all with global flag)
    #[serde(default)]
    pub first_only: Option<bool>,
    /// Deduplicate extracted values (only unique values are output)
    #[serde(default)]
    pub deduplicate: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
    #[default]
    Substitute,
    Filter,
    Extract,
    Validate,
    Transform,
    /// Block-scoped processing: apply actions only within matching blocks
    Block,
}

/// Unified action enum for Filter and Block steps.
/// The action is interpreted based on the step type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepAction {
    // === Filter actions ===
    /// Keep lines that match the pattern
    KeepLine,
    /// Drop lines that match the pattern
    DropLine,
    /// Keep only the matched portions of each line
    KeepMatch,
    /// Remove matched portions, keeping the rest of the line
    DropMatch,
    /// Deduplicate lines based on prefix from first capture group.
    /// The prefix length is inferred from the capture group in the pattern.
    /// Example: pattern = "^(.{50}).*$" captures first 50 chars as prefix
    DeduplicateByPrefix,
    // === Block actions ===
    /// Keep only lines within matching blocks
    KeepBlock,
    /// Drop lines within matching blocks
    DropBlock,
    /// Collect and output block contents together
    CollectBlock,
    /// Deduplicate identical blocks (output each unique block only once)
    Deduplicate,
    /// Mark/tag lines within matching blocks (prepend marker)
    #[serde(rename = "mark_block")]
    MarkBlock {
        /// Marker to prepend to lines in the block
        marker: String,
    },
    /// Apply a substitution to lines within the block
    #[serde(rename = "substitute_in_block")]
    SubstituteInBlock {
        /// Pattern to match within block lines
        pattern: String,
        /// Replacement text
        replacement: String,
    },
}

/// Action to take when validation fails
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OnMismatch {
    /// Stop processing and return error (default)
    #[default]
    Error,
    /// Log a warning but continue processing
    Warn,
    /// Silently skip the line
    Skip,
}

/// Convert StepAction to a string representation for error messages
fn step_action_to_str(action: &StepAction) -> &'static str {
    match action {
        StepAction::KeepLine => "keep_line",
        StepAction::DropLine => "drop_line",
        StepAction::KeepMatch => "keep_match",
        StepAction::DropMatch => "drop_match",
        StepAction::DeduplicateByPrefix => "deduplicate_by_prefix",
        StepAction::KeepBlock => "keep_block",
        StepAction::DropBlock => "drop_block",
        StepAction::CollectBlock => "collect_block",
        StepAction::Deduplicate => "deduplicate",
        StepAction::MarkBlock { .. } => "mark_block",
        StepAction::SubstituteInBlock { .. } => "substitute_in_block",
    }
}

/// Legacy FilterAction for backward compatibility (internal use)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterAction {
    KeepLine,
    DropLine,
    KeepMatch,
    DropMatch,
    DeduplicateByPrefix,
}

/// Output format for extract steps
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExtractOutputFormat {
    /// Tab-separated text output (default)
    #[default]
    Text,
    /// JSON object with capture names as keys
    Json,
    /// JSON Lines (one JSON object per match)
    Jsonl,
    /// CSV format with headers from capture_names
    Csv,
}

/// Actions for Transform step type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransformAction {
    /// Convert matched text to uppercase
    Uppercase,
    /// Convert matched text to lowercase
    Lowercase,
    /// Trim whitespace from matched text
    Trim,
    /// Prepend text to matched content
    Prepend,
    /// Append text to matched content
    Append,
    /// Reverse the matched text
    Reverse,
    /// Remove all whitespace from matched text
    RemoveWhitespace,
    /// Capitalize first letter of each word
    TitleCase,
    /// Execute an external command, passing matched text as stdin
    /// The command's stdout becomes the replacement text
    #[serde(rename = "shell")]
    Shell {
        /// Command to execute (passed to shell)
        command: String,
    },
    /// Apply a custom transformation by name (from registered plugins)
    #[serde(rename = "plugin")]
    Plugin {
        /// Name of the registered plugin function
        name: String,
        /// Optional arguments to pass to the plugin
        #[serde(default)]
        args: Vec<String>,
    },
    /// Base64 encode the matched text
    Base64Encode,
    /// Base64 decode the matched text
    Base64Decode,
    /// URL encode the matched text
    UrlEncode,
    /// URL decode the matched text
    UrlDecode,
    /// Replace runs of whitespace with a single space
    NormalizeWhitespace,
    /// Remove duplicate lines (when applied to full line matches)
    Deduplicate,
    /// Sort characters in the matched text
    SortChars,
    /// Count characters and replace with count
    CharCount,
    /// Count words and replace with count
    WordCount,
    /// Format-preserving encryption using FF1 algorithm (requires fpe feature)
    /// Encrypts matched text while preserving format (digits remain digits)
    #[cfg(feature = "fpe")]
    #[serde(rename = "fpe_encrypt")]
    FpeEncrypt {
        /// Encryption key (hex-encoded, 16/24/32 bytes for AES-128/192/256)
        /// Either `key` or `key_file` must be provided
        #[serde(default)]
        key: Option<String>,
        /// Path to file containing the encryption key (alternative to inline key)
        /// File should contain hex-encoded key, whitespace is trimmed
        #[serde(default)]
        key_file: Option<String>,
        /// Optional tweak value (hex-encoded, up to 16 bytes)
        #[serde(default)]
        tweak: String,
        /// Path to file containing the tweak value (alternative to inline tweak)
        #[serde(default)]
        tweak_file: Option<String>,
        /// Character set for encryption (default: "0123456789")
        /// Common values: "0123456789", "0123456789ABCDEF", "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
        #[serde(default = "default_fpe_radix")]
        radix: String,
    },
    /// Format-preserving decryption using FF1 algorithm (requires fpe feature)
    /// Decrypts text that was encrypted with fpe_encrypt using the same key/tweak
    #[cfg(feature = "fpe")]
    #[serde(rename = "fpe_decrypt")]
    FpeDecrypt {
        /// Decryption key (hex-encoded, must match encryption key)
        /// Either `key` or `key_file` must be provided
        #[serde(default)]
        key: Option<String>,
        /// Path to file containing the decryption key (alternative to inline key)
        #[serde(default)]
        key_file: Option<String>,
        /// Optional tweak value (hex-encoded, must match encryption tweak)
        #[serde(default)]
        tweak: String,
        /// Path to file containing the tweak value (alternative to inline tweak)
        #[serde(default)]
        tweak_file: Option<String>,
        /// Character set for decryption (must match encryption radix)
        #[serde(default = "default_fpe_radix")]
        radix: String,
    },
    /// Deterministic masking - consistent one-way transformation
    /// Same input always produces same output (useful for joining masked datasets)
    #[serde(rename = "mask_deterministic")]
    MaskDeterministic {
        /// Seed for deterministic hashing (different seeds produce different outputs)
        /// Either `seed` or `seed_file` must be provided
        #[serde(default)]
        seed: Option<String>,
        /// Path to file containing the seed value (alternative to inline seed)
        #[serde(default)]
        seed_file: Option<String>,
        /// Preserve first N characters (e.g., 4 for credit card prefix)
        #[serde(default)]
        preserve_prefix: usize,
        /// Preserve last N characters (e.g., 4 for last 4 of SSN)
        #[serde(default)]
        preserve_suffix: usize,
        /// Mask character (default: '*')
        #[serde(default = "default_mask_char")]
        mask_char: char,
    },
}

#[cfg(feature = "fpe")]
fn default_fpe_radix() -> String {
    "0123456789".to_string()
}

fn default_mask_char() -> char {
    '*'
}

/// Internal representation of block actions (converted from StepAction)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockAction {
    /// Keep only lines within matching blocks
    KeepBlock,
    /// Drop lines within matching blocks
    DropBlock,
    /// Collect and output block contents together
    CollectBlock,
    /// Deduplicate identical blocks
    Deduplicate,
    /// Mark/tag lines within matching blocks
    MarkBlock { marker: String },
    /// Apply a substitution to lines within the block
    SubstituteInBlock {
        pattern: String,
        replacement: String,
    },
}

impl BlockAction {
    /// Convert from unified StepAction to BlockAction
    pub fn from_step_action(action: &StepAction) -> Option<Self> {
        match action {
            StepAction::KeepBlock => Some(BlockAction::KeepBlock),
            StepAction::DropBlock => Some(BlockAction::DropBlock),
            StepAction::CollectBlock => Some(BlockAction::CollectBlock),
            StepAction::Deduplicate => Some(BlockAction::Deduplicate),
            StepAction::MarkBlock { marker } => Some(BlockAction::MarkBlock {
                marker: marker.clone(),
            }),
            StepAction::SubstituteInBlock {
                pattern,
                replacement,
            } => Some(BlockAction::SubstituteInBlock {
                pattern: pattern.clone(),
                replacement: replacement.clone(),
            }),
            _ => None, // Filter actions don't convert to block actions
        }
    }
}

/// Block context value - can be a simple number or structured config
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BlockContextValue {
    /// Simple number of context lines
    Lines(usize),
    /// Structured configuration with overlap settings
    Config(BlockContextConfig),
}

impl Default for BlockContextValue {
    fn default() -> Self {
        BlockContextValue::Lines(0)
    }
}

impl BlockContextValue {
    /// Get the number of context lines
    pub fn lines(&self) -> usize {
        match self {
            BlockContextValue::Lines(n) => *n,
            BlockContextValue::Config(c) => c.lines.unwrap_or(0),
        }
    }

    /// Get the overlap in characters (if specified)
    pub fn overlap_chars(&self) -> Option<usize> {
        match self {
            BlockContextValue::Lines(_) => None,
            BlockContextValue::Config(c) => c.overlap_chars,
        }
    }

    /// Get the overlap in lines (if specified)
    pub fn overlap_lines(&self) -> Option<usize> {
        match self {
            BlockContextValue::Lines(_) => None,
            BlockContextValue::Config(c) => c.overlap_lines,
        }
    }
}

/// Configuration for block context and overlap
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BlockContextConfig {
    /// Number of lines of context after the trigger pattern
    #[serde(default)]
    pub lines: Option<usize>,
    /// Number of characters to overlap between blocks (for chunking)
    #[serde(default)]
    pub overlap_chars: Option<usize>,
    /// Number of lines to overlap between blocks
    #[serde(default)]
    pub overlap_lines: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegexFlag {
    Global,
    CaseInsensitive,
    Multiline,
    DotAll,
    Unicode,
    Extended,
}

#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub lines_processed: u64,
    pub matches_found: u64,
    pub transformations_applied: u64,
    pub errors: Vec<PipelineError>,
    pub step_results: Vec<StepResult>,
}

/// Result from processing a single pipeline step.
#[derive(Debug, Clone)]
pub struct StepResult {
    /// Index of this step in the pipeline (0-based)
    pub step_index: usize,
    /// The type of step (Substitute, Filter, etc.)
    pub step_type: StepType,
    /// The regex pattern used by this step
    pub pattern: String,
    /// Number of matches found by this step
    pub matches: u64,
    /// Number of transformations applied by this step
    pub transformations: u64,
    /// Time spent processing this step in milliseconds
    pub processing_time_ms: u64,
    /// Any errors that occurred during this step
    pub errors: Vec<String>,
}

/// An error that occurred during pipeline processing.
#[derive(Debug, Clone)]
pub struct PipelineError {
    /// Index of the step where the error occurred (0-based)
    pub step_index: usize,
    /// Line number where the error occurred (1-based)
    pub line_number: u64,
    /// The type/category of error
    pub error_type: ErrorType,
    /// Human-readable error message
    pub message: String,
    /// Optional context (e.g., the line content that caused the error)
    pub context: Option<String>,
}

/// Categories of errors that can occur during pipeline processing.
#[derive(Debug, Clone)]
pub enum ErrorType {
    /// Error compiling a regex pattern
    RegexCompilation,
    /// Pattern matching failed (e.g., validation step)
    PatternMatch,
    /// Error during text substitution
    Substitution,
    /// I/O error (reading/writing files)
    IoError,
    /// Configuration error (invalid settings)
    ConfigurationError,
}

impl PipelineConfig {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::from_file_with_base_dir(path.as_ref(), path.as_ref().parent())
    }

    /// Load a pipeline config, resolving `extends` relative to the given base directory.
    fn from_file_with_base_dir(path: &Path, base_dir: Option<&Path>) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let mut config: PipelineConfig = toml::from_str(&content)?;

        // Handle extends: merge base configuration
        if let Some(ref extends_path) = config.extends {
            let base_path = if Path::new(extends_path).is_absolute() {
                std::path::PathBuf::from(extends_path)
            } else if let Some(dir) = base_dir {
                dir.join(extends_path)
            } else {
                std::path::PathBuf::from(extends_path)
            };

            // Prevent infinite recursion by limiting depth
            let base_config = Self::from_file_with_base_dir(&base_path, base_path.parent())?;
            config = config.merge_with_base(base_config);
        }

        Ok(config)
    }

    /// Merge this config with a base config (for extends support)
    fn merge_with_base(self, base: PipelineConfig) -> PipelineConfig {
        // Steps from base are prepended to this config's steps
        let mut merged_steps = base.step;
        merged_steps.extend(self.step);

        // Pattern includes are merged
        let mut merged_patterns = base.patterns_include;
        for pattern in self.patterns_include {
            if !merged_patterns.contains(&pattern) {
                merged_patterns.push(pattern);
            }
        }

        PipelineConfig {
            // Keep this config's metadata (name, description, version)
            name: self.name.or(base.name),
            description: self.description.or(base.description),
            version: self.version.or(base.version),
            extends: None, // Clear extends since we've processed it
            patterns_include: merged_patterns,
            // Merge settings (this config's settings override base)
            settings: self.settings.merge_with_base(base.settings),
            step: merged_steps,
            // Use this config's advanced configs if present, else base
            bidirectional: if self.bidirectional != BidirectionalConfig::default() {
                self.bidirectional
            } else {
                base.bidirectional
            },
            checkpoint: if self.checkpoint != CheckpointConfig::default() {
                self.checkpoint
            } else {
                base.checkpoint
            },
            cross_file: if self.cross_file != CrossFileConfig::default() {
                self.cross_file
            } else {
                base.cross_file
            },
            tests: if !self.tests.is_empty() {
                self.tests
            } else {
                base.tests
            },
            finalize: if self.finalize.is_configured() {
                self.finalize
            } else {
                base.finalize
            },
        }
    }

    /// Create a pipeline from a single inline pattern.
    ///
    /// # Arguments
    /// * `pattern` - The regex pattern to match
    /// * `replacement` - Optional replacement text (if provided, creates a Substitute step)
    ///
    /// # Example
    /// ```
    /// use rexpipe::pipeline::PipelineConfig;
    ///
    /// // Create a filter pipeline (no replacement)
    /// let filter = PipelineConfig::from_inline_pattern(r"\d+", None);
    ///
    /// // Create a substitution pipeline
    /// let substitute = PipelineConfig::from_inline_pattern(r"\d+", Some("NUMBER"));
    /// ```
    pub fn from_inline_pattern(pattern: &str, replacement: Option<&str>) -> Self {
        Self::from_inline_pattern_with_settings(pattern, replacement, PipelineSettings::default())
    }

    pub fn from_inline_pattern_with_settings(
        pattern: &str,
        replacement: Option<&str>,
        settings: PipelineSettings,
    ) -> Self {
        let step_type = if replacement.is_some() {
            StepType::Substitute
        } else {
            StepType::Filter
        };

        let step = PipelineStep {
            step_type,
            pattern: pattern.to_string(),
            replacement: replacement.map(|s| s.to_string()),
            action: if replacement.is_none() {
                Some(StepAction::KeepMatch)
            } else {
                None
            },
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
            start_pattern: None,
            end_pattern: None,
            block_context: None,
            on_mismatch: None,
            language: None,
            languages: None,
            scope: None,
            exclude_scopes: None,
            capture_names: None,
            output_format: None,
            output_template: None,
            first_only: None,
            deduplicate: None,
        };

        PipelineConfig {
            name: Some("Inline Pipeline".to_string()),
            description: Some("Generated from command line pattern".to_string()),
            version: Some("1.0.0".to_string()),
            extends: None,
            patterns_include: Vec::new(),
            settings,
            step: vec![step],
            bidirectional: BidirectionalConfig::default(),
            checkpoint: CheckpointConfig::default(),
            cross_file: CrossFileConfig::default(),
            tests: Vec::new(),
            finalize: FinalizeConfig::default(),
        }
    }

    /// Set pipeline settings using builder pattern.
    ///
    /// # Example
    /// ```
    /// use rexpipe::pipeline::{PipelineConfig, PipelineSettings};
    ///
    /// let settings = PipelineSettings {
    ///     pcre_mode: true,
    ///     ..Default::default()
    /// };
    /// let config = PipelineConfig::from_inline_pattern(r"\d+", None)
    ///     .with_settings(settings);
    /// ```
    pub fn with_settings(mut self, settings: PipelineSettings) -> Self {
        self.settings = settings;
        self
    }

    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Create a pipeline configuration from a JSON string.
    ///
    /// # Example
    /// ```
    /// use rexpipe::pipeline::PipelineConfig;
    ///
    /// let json = r#"{
    ///     "name": "Test Pipeline",
    ///     "step": [{
    ///         "type": "substitute",
    ///         "pattern": "\\d+",
    ///         "replacement": "NUM"
    ///     }]
    /// }"#;
    /// let config = PipelineConfig::from_json(json).unwrap();
    /// ```
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.step.is_empty() {
            errors.push("Pipeline must contain at least one step".to_string());
        }

        for (i, step) in self.step.iter().enumerate() {
            // For Block steps, use start_pattern; for others, use pattern
            let effective_pattern = if matches!(step.step_type, StepType::Block) {
                step.start_pattern.as_ref().unwrap_or(&step.pattern)
            } else {
                &step.pattern
            };

            if effective_pattern.is_empty() {
                errors.push(format!("Step {}: Pattern cannot be empty", i + 1));
            }

            match step.step_type {
                StepType::Substitute => {
                    if step.replacement.is_none() {
                        errors.push(format!(
                            "Step {}: Substitute type requires replacement",
                            i + 1
                        ));
                    }
                }
                StepType::Filter => {
                    if step.action.is_none() {
                        errors.push(format!("Step {}: Filter type requires action", i + 1));
                    }
                }
                StepType::Transform => {
                    if step.transform.is_none() {
                        errors.push(format!(
                            "Step {}: Transform type requires transform action",
                            i + 1
                        ));
                    }
                }
                _ => {}
            }

            if !step.enabled.unwrap_or(true) {
                continue;
            }
        }

        // Check for pattern references without loaded libraries
        if self.patterns_include.is_empty() {
            for (i, step) in self.step.iter().enumerate() {
                if crate::library::has_pattern_references(&step.pattern) {
                    errors.push(format!(
                        "Step {}: Pattern uses reference syntax (${{...}}) but no pattern libraries are included. \
                         Add 'patterns_include' to your config or use --library",
                        i + 1
                    ));
                }
            }
        }

        // Check for contradictory filter configurations
        errors.extend(self.check_contradictory_filters());

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Check for contradictory filter configurations between steps.
    ///
    /// Detects when two filter steps have opposite actions (keep_line vs drop_line)
    /// on identical patterns, which would make the second step ineffective.
    fn check_contradictory_filters(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let enabled_steps: Vec<_> = self
            .step
            .iter()
            .enumerate()
            .filter(|(_, s)| s.enabled.unwrap_or(true))
            .collect();

        for (i, &(idx1, step1)) in enabled_steps.iter().enumerate() {
            if !matches!(step1.step_type, StepType::Filter) {
                continue;
            }

            for &(idx2, step2) in enabled_steps.iter().skip(i + 1) {
                if !matches!(step2.step_type, StepType::Filter) {
                    continue;
                }

                // Check if patterns are identical
                if step1.pattern != step2.pattern {
                    continue;
                }

                // Check if actions are contradictory
                if let (Some(action1), Some(action2)) = (&step1.action, &step2.action) {
                    let contradictory = matches!(
                        (action1, action2),
                        (StepAction::KeepLine, StepAction::DropLine)
                            | (StepAction::DropLine, StepAction::KeepLine)
                            | (StepAction::KeepMatch, StepAction::DropMatch)
                            | (StepAction::DropMatch, StepAction::KeepMatch)
                    );

                    if contradictory {
                        let action1_str = step_action_to_str(action1);
                        let action2_str = step_action_to_str(action2);
                        errors.push(format!(
                            "Contradictory filters: Step {} ({} on '{}') conflicts with \
                             Step {} ({} on same pattern). The second filter will have no effect.",
                            idx1 + 1,
                            action1_str,
                            step1.pattern,
                            idx2 + 1,
                            action2_str
                        ));
                    }
                }
            }
        }

        errors
    }

    /// Comprehensive validation that returns structured errors.
    ///
    /// This method returns `ValidationError` types for better error handling
    /// and more informative error messages.
    pub fn validate_comprehensive(&self) -> std::result::Result<(), crate::error::ValidationError> {
        let mut errors = Vec::new();

        if self.step.is_empty() {
            return Err(crate::error::ValidationError::EmptyPipeline);
        }

        for (i, step) in self.step.iter().enumerate() {
            let step_num = i + 1;

            // Check empty pattern
            if step.pattern.is_empty() {
                let step_type_str = match step.step_type {
                    StepType::Substitute => "substitute",
                    StepType::Filter => "filter",
                    StepType::Extract => "extract",
                    StepType::Validate => "validate",
                    StepType::Transform => "transform",
                    StepType::Block => "block",
                };
                errors.push(crate::error::ValidationError::missing_field(
                    step_num,
                    "pattern",
                    step_type_str,
                ));
            }

            // Check type-specific requirements
            match step.step_type {
                StepType::Substitute => {
                    if step.replacement.is_none() {
                        errors.push(crate::error::ValidationError::missing_field(
                            step_num,
                            "replacement",
                            "substitute",
                        ));
                    }
                }
                StepType::Filter => {
                    if step.action.is_none() {
                        errors.push(crate::error::ValidationError::missing_field(
                            step_num, "action", "filter",
                        ));
                    }
                }
                StepType::Transform => {
                    if step.transform.is_none() {
                        errors.push(crate::error::ValidationError::step_error(
                            step_num,
                            "Transform type requires a 'transform' field",
                            "Add a transform action like 'uppercase', 'lowercase', 'trim', etc.",
                        ));
                    }
                }
                _ => {}
            }

            // Check for pattern references without libraries
            if self.patterns_include.is_empty()
                && crate::library::has_pattern_references(&step.pattern)
            {
                errors.push(crate::error::ValidationError::step_error(
                    step_num,
                    "Pattern uses reference syntax (${...}) but no libraries are loaded",
                    "Add 'patterns_include' to your config or use --library flag",
                ));
            }
        }

        // Check for contradictory filters
        if let Some(conflict) = self.find_contradictory_filters() {
            errors.push(conflict);
        }

        // Return first error or success
        if let Some(first_error) = errors.into_iter().next() {
            Err(first_error)
        } else {
            Ok(())
        }
    }

    /// Find contradictory filter configurations and return a structured error.
    fn find_contradictory_filters(&self) -> Option<crate::error::ValidationError> {
        let enabled_steps: Vec<_> = self
            .step
            .iter()
            .enumerate()
            .filter(|(_, s)| s.enabled.unwrap_or(true))
            .collect();

        for (i, &(idx1, step1)) in enabled_steps.iter().enumerate() {
            if !matches!(step1.step_type, StepType::Filter) {
                continue;
            }

            for &(idx2, step2) in enabled_steps.iter().skip(i + 1) {
                if !matches!(step2.step_type, StepType::Filter) {
                    continue;
                }

                if step1.pattern != step2.pattern {
                    continue;
                }

                if let (Some(action1), Some(action2)) = (&step1.action, &step2.action) {
                    let contradictory = matches!(
                        (action1, action2),
                        (StepAction::KeepLine, StepAction::DropLine)
                            | (StepAction::DropLine, StepAction::KeepLine)
                            | (StepAction::KeepMatch, StepAction::DropMatch)
                            | (StepAction::DropMatch, StepAction::KeepMatch)
                    );

                    if contradictory {
                        return Some(crate::error::ValidationError::ContradictoryFilters {
                            step1: idx1 + 1,
                            step2: idx2 + 1,
                            pattern: step1.pattern.clone(),
                            action1: format!("{:?}", action1),
                            action2: format!("{:?}", action2),
                        });
                    }
                }
            }
        }

        None
    }

    pub fn enabled_steps(&self) -> impl Iterator<Item = &PipelineStep> {
        self.step.iter().filter(|step| step.enabled.unwrap_or(true))
    }

    /// Check if any step uses shell transforms.
    ///
    /// Shell transforms execute external commands and may be restricted
    /// for security reasons when processing untrusted input.
    pub fn has_shell_transforms(&self) -> bool {
        self.step
            .iter()
            .any(|step| matches!(&step.transform, Some(TransformAction::Shell { .. })))
    }

    /// Get a list of shell commands used in this pipeline.
    ///
    /// Returns the commands that would be executed by shell transforms.
    pub fn get_shell_commands(&self) -> Vec<&str> {
        self.step
            .iter()
            .filter_map(|step| {
                if let Some(TransformAction::Shell { command }) = &step.transform {
                    Some(command.as_str())
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn summary(&self) -> String {
        let total_steps = self.step.len();
        let enabled_steps = self.enabled_steps().count();

        format!(
            "Pipeline '{}' (v{}): {} steps ({} enabled)\n{}",
            self.name.as_deref().unwrap_or("Unnamed"),
            self.version.as_deref().unwrap_or("1.0.0"),
            total_steps,
            enabled_steps,
            self.description.as_deref().unwrap_or("")
        )
    }

    /// Resolve pattern references like ${pattern_name} in all steps
    ///
    /// This method modifies the config in place, replacing pattern references
    /// with their actual values from the provided library.
    pub fn resolve_pattern_references(
        &mut self,
        library: &crate::library::ResolvedLibrary,
    ) -> Result<(), Vec<String>> {
        let mut all_errors = Vec::new();

        for (i, step) in self.step.iter_mut().enumerate() {
            // Check if pattern contains references
            if !crate::library::has_pattern_references(&step.pattern) {
                continue;
            }

            match crate::library::resolve_pattern_references(&step.pattern, library) {
                Ok(resolved) => {
                    step.pattern = resolved;
                }
                Err(errors) => {
                    for error in errors {
                        all_errors.push(format!("Step {}: {}", i + 1, error));
                    }
                }
            }
        }

        if all_errors.is_empty() {
            Ok(())
        } else {
            Err(all_errors)
        }
    }

    /// Check if this config uses pattern libraries
    pub fn uses_pattern_libraries(&self) -> bool {
        !self.patterns_include.is_empty()
    }
}

impl Default for PipelineResult {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineResult {
    pub fn new() -> Self {
        Self {
            lines_processed: 0,
            matches_found: 0,
            transformations_applied: 0,
            errors: Vec::new(),
            step_results: Vec::new(),
        }
    }

    pub fn add_step_result(&mut self, result: StepResult) {
        self.matches_found += result.matches;
        self.transformations_applied += result.transformations;
        self.step_results.push(result);
    }

    /// Add an error to the pipeline result.
    ///
    /// Used during processing to record errors that occur (e.g., validation failures).
    pub fn add_error(&mut self, error: PipelineError) {
        self.errors.push(error);
    }

    pub fn success_rate(&self) -> f64 {
        if self.lines_processed == 0 {
            return 0.0;
        }

        let error_lines = self.errors.len() as u64;
        (self.lines_processed - error_lines) as f64 / self.lines_processed as f64
    }

    pub fn performance_summary(&self) -> String {
        let total_time: u64 = self.step_results.iter().map(|r| r.processing_time_ms).sum();

        format!(
            "Performance Summary:\n\
             Lines processed: {}\n\
             Total matches: {}\n\
             Transformations: {}\n\
             Processing time: {}ms\n\
             Success rate: {:.2}%\n\
             Errors: {}",
            self.lines_processed,
            self.matches_found,
            self.transformations_applied,
            total_time,
            self.success_rate() * 100.0,
            self.errors.len()
        )
    }
}

impl StepResult {
    pub fn new(step_index: usize, step_type: StepType, pattern: String) -> Self {
        Self {
            step_index,
            step_type,
            pattern,
            matches: 0,
            transformations: 0,
            processing_time_ms: 0,
            errors: Vec::new(),
        }
    }

    pub fn add_match(&mut self) {
        self.matches += 1;
    }

    pub fn add_transformation(&mut self) {
        self.transformations += 1;
    }

    /// Add an error message to this step's result.
    ///
    /// Used to record step-specific errors during pipeline execution.
    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
    }

    pub fn set_processing_time(&mut self, time_ms: u64) {
        self.processing_time_ms = time_ms;
    }
}

impl PipelineError {
    pub fn new(
        step_index: usize,
        line_number: u64,
        error_type: ErrorType,
        message: String,
    ) -> Self {
        Self {
            step_index,
            line_number,
            error_type,
            message,
            context: None,
        }
    }

    pub fn with_context(mut self, context: String) -> Self {
        self.context = Some(context);
        self
    }
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Step {} (Line {}): {:?} - {}{}",
            self.step_index + 1,
            self.line_number,
            self.error_type,
            self.message,
            self.context
                .as_ref()
                .map_or(String::new(), |c| format!("\nContext: {}", c))
        )
    }
}

impl std::error::Error for PipelineError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inline_pipeline_creation() {
        let config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUMBER"));
        assert_eq!(config.step.len(), 1);
        assert!(matches!(config.step[0].step_type, StepType::Substitute));
        assert_eq!(config.step[0].pattern, r"\d+");
        assert_eq!(config.step[0].replacement, Some("NUMBER".to_string()));
    }

    #[test]
    fn test_pipeline_validation() {
        let mut config = PipelineConfig {
            name: Some("Test".to_string()),
            step: vec![],
            ..Default::default()
        };

        // Empty pipeline should be rejected
        let err = config
            .validate()
            .expect_err("Empty pipeline should be invalid");
        assert!(
            err.iter().any(|e| e.contains("at least one step")),
            "Error should mention missing steps: {:?}",
            err
        );

        config.step.push(PipelineStep {
            step_type: StepType::Substitute,
            pattern: "test".to_string(),
            replacement: None,
            action: None,
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        });

        // Substitute step without replacement should be rejected
        let err = config
            .validate()
            .expect_err("Substitute without replacement should be invalid");
        assert!(
            err.iter().any(|e| e.contains("replacement")),
            "Error should mention missing replacement: {:?}",
            err
        );

        config.step[0].replacement = Some("replacement".to_string());
        config
            .validate()
            .expect("Valid config should pass validation");
    }

    #[test]
    fn test_pipeline_result_tracking() {
        let mut result = PipelineResult::new();
        assert_eq!(result.lines_processed, 0);
        assert_eq!(result.matches_found, 0);

        let step_result = StepResult {
            step_index: 0,
            step_type: StepType::Substitute,
            pattern: "test".to_string(),
            matches: 5,
            transformations: 3,
            processing_time_ms: 100,
            errors: Vec::new(),
        };

        result.add_step_result(step_result);
        assert_eq!(result.matches_found, 5);
        assert_eq!(result.transformations_applied, 3);
    }

    #[test]
    fn test_contradictory_filter_detection() {
        let config = PipelineConfig {
            name: Some("Test".to_string()),
            step: vec![
                PipelineStep {
                    step_type: StepType::Filter,
                    pattern: "ERROR".to_string(),
                    action: Some(StepAction::KeepLine),
                    enabled: Some(true),
                    ..Default::default()
                },
                PipelineStep {
                    step_type: StepType::Filter,
                    pattern: "ERROR".to_string(),
                    action: Some(StepAction::DropLine),
                    enabled: Some(true),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("Contradictory")));
    }

    #[test]
    fn test_non_contradictory_filters_same_pattern() {
        // keep_line followed by drop_match is NOT contradictory
        let config = PipelineConfig {
            name: Some("Test".to_string()),
            step: vec![
                PipelineStep {
                    step_type: StepType::Filter,
                    pattern: "ERROR".to_string(),
                    action: Some(StepAction::KeepLine),
                    enabled: Some(true),
                    ..Default::default()
                },
                PipelineStep {
                    step_type: StepType::Filter,
                    pattern: "ERROR".to_string(),
                    action: Some(StepAction::KeepLine),
                    enabled: Some(true),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        // Same action twice is redundant but not contradictory
        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_transform_step_requires_transform_action() {
        let config = PipelineConfig {
            name: Some("Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings::default(),
            step: vec![PipelineStep {
                step_type: StepType::Transform,
                pattern: "test".to_string(),
                replacement: None,
                action: None,
                transform: None, // Missing!
                flags: None,
                description: None,
                enabled: Some(true),
                ..Default::default()
            }],
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("Transform")));
    }

    #[test]
    fn test_transform_step_with_action_validates() {
        let config = PipelineConfig {
            name: Some("Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings::default(),
            step: vec![PipelineStep {
                step_type: StepType::Transform,
                pattern: "test".to_string(),
                replacement: None,
                action: None,
                transform: Some(TransformAction::Uppercase),
                flags: None,
                description: None,
                enabled: Some(true),
                ..Default::default()
            }],
            ..Default::default()
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_pattern_reference_without_library_warning() {
        let config = PipelineConfig {
            name: Some("Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(), // No libraries!
            settings: PipelineSettings::default(),
            step: vec![PipelineStep {
                step_type: StepType::Substitute,
                pattern: "${email}".to_string(), // Uses reference
                replacement: Some("REDACTED".to_string()),
                action: None,
                transform: None,
                flags: None,
                description: None,
                enabled: Some(true),
                ..Default::default()
            }],
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("${") && e.contains("library"))
        );
    }

    #[test]
    fn test_disabled_step_not_checked_for_contradictions() {
        let config = PipelineConfig {
            name: Some("Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings::default(),
            step: vec![
                PipelineStep {
                    step_type: StepType::Filter,
                    pattern: "ERROR".to_string(),
                    replacement: None,
                    action: Some(StepAction::KeepLine),
                    transform: None,
                    flags: None,
                    description: None,
                    enabled: Some(true),
                    ..Default::default()
                },
                PipelineStep {
                    step_type: StepType::Filter,
                    pattern: "ERROR".to_string(),
                    replacement: None,
                    action: Some(StepAction::DropLine),
                    transform: None,
                    flags: None,
                    description: None,
                    enabled: Some(false), // Disabled!
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        // Should not detect contradiction since second step is disabled
        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_comprehensive_validation_empty_pipeline() {
        let config = PipelineConfig {
            name: Some("Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings::default(),
            step: vec![],
            ..Default::default()
        };

        let result = config.validate_comprehensive();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::ValidationError::EmptyPipeline
        ));
    }

    #[test]
    fn test_comprehensive_validation_contradictory_filters() {
        let config = PipelineConfig {
            name: Some("Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings::default(),
            step: vec![
                PipelineStep {
                    step_type: StepType::Filter,
                    pattern: "ERROR".to_string(),
                    replacement: None,
                    action: Some(StepAction::KeepLine),
                    transform: None,
                    flags: None,
                    description: None,
                    enabled: Some(true),
                    ..Default::default()
                },
                PipelineStep {
                    step_type: StepType::Filter,
                    pattern: "ERROR".to_string(),
                    replacement: None,
                    action: Some(StepAction::DropLine),
                    transform: None,
                    flags: None,
                    description: None,
                    enabled: Some(true),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let result = config.validate_comprehensive();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::ValidationError::ContradictoryFilters { .. }
        ));
    }

    #[test]
    fn test_config_merge_with_base() {
        // Create a base config with settings
        let base = PipelineConfig {
            name: Some("Base".to_string()),
            description: Some("Base description".to_string()),
            version: Some("1.0.0".to_string()),
            settings: PipelineSettings {
                pcre_mode: true,
                strict_mode: true,
                ..Default::default()
            },
            step: vec![PipelineStep {
                pattern: "base_pattern".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        // Create a child config that overrides some values
        // Note: Child can only override if the value differs from default
        let child = PipelineConfig {
            name: Some("Child".to_string()),
            description: None, // Should inherit from base
            version: None,     // Should inherit from base
            settings: PipelineSettings {
                block_mode: true, // Explicitly set (differs from default false)
                timeout_ms: 5000, // Explicitly set (differs from default 0)
                ..Default::default()
            },
            step: vec![PipelineStep {
                pattern: "child_pattern".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let merged = child.merge_with_base(base);

        // Child name should override
        assert_eq!(merged.name, Some("Child".to_string()));
        // Should inherit base description
        assert_eq!(merged.description, Some("Base description".to_string()));
        // Should inherit base version
        assert_eq!(merged.version, Some("1.0.0".to_string()));
        // Base pcre_mode should be inherited (child didn't set it)
        assert!(merged.settings.pcre_mode);
        // Base strict_mode should be inherited (child used default)
        assert!(merged.settings.strict_mode);
        // Child block_mode should be set (explicitly differs from default)
        assert!(merged.settings.block_mode);
        // Child timeout should be set
        assert_eq!(merged.settings.timeout_ms, 5000);
        // Steps are merged: base steps first, then child steps
        assert_eq!(merged.step.len(), 2);
        assert_eq!(merged.step[0].pattern, "base_pattern");
        assert_eq!(merged.step[1].pattern, "child_pattern");
    }
}
