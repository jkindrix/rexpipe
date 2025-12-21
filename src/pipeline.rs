use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::audit::AuditConfig;
use crate::bidirectional::BidirectionalConfig;
use crate::checkpoint::CheckpointConfig;
use crate::crossfile::CrossFileConfig;
use crate::testing::TestCase;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    /// Pattern libraries to include (supports ${pattern_name} references in steps)
    #[serde(default)]
    pub patterns_include: Vec<String>,
    #[serde(default)]
    pub settings: PipelineSettings,
    #[serde(default)]
    pub step: Vec<PipelineStep>,

    // === Advanced feature configurations ===

    /// Audit trail configuration for compliance and provenance tracking
    #[serde(default)]
    pub audit: AuditConfig,

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
    /// Allow shell command execution in transforms (set via --no-shell CLI flag)
    /// Defaults to true for backwards compatibility
    #[serde(default = "default_allow_shell")]
    pub allow_shell: bool,
    /// Strict mode - reject patterns with potential ReDoS vulnerabilities
    #[serde(default)]
    pub strict_mode: bool,
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
            preserve_line_endings: false,
            max_line_length: 0,
            max_line_action: MaxLineAction::default(),
            shell_timeout_secs: default_shell_timeout(),
            regex_size_limit: default_regex_size_limit(),
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
    true
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
    #[serde(default)]
    pub pattern: String,
    #[serde(default)]
    pub replacement: Option<String>,
    #[serde(default)]
    pub action: Option<FilterAction>,
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
    /// Pattern that ends the block (for Block step type)
    #[serde(default)]
    pub until: Option<String>,
    /// Action to apply within the block
    #[serde(default)]
    pub block_action: Option<BlockAction>,
    /// Number of context lines after trigger to include in block
    #[serde(default)]
    pub block_context: Option<usize>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterAction {
    KeepLine,
    DropLine,
    KeepMatch,
    DropMatch,
}

/// Actions for Transform step type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

/// Actions for Block step type (cross-line processing)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockAction {
    /// Keep only lines within matching blocks
    KeepBlock,
    /// Drop lines within matching blocks
    DropBlock,
    /// Mark/tag lines within matching blocks (prepend marker)
    MarkBlock {
        /// Marker to prepend to lines in the block
        marker: String,
    },
    /// Apply a substitution to lines within the block
    SubstituteInBlock {
        /// Pattern to match within block lines
        pattern: String,
        /// Replacement text
        replacement: String,
    },
    /// Collect and output block contents together (useful for log extraction)
    CollectBlock,
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
        let content = fs::read_to_string(path)?;
        let config: PipelineConfig = toml::from_str(&content)?;
        Ok(config)
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
                Some(FilterAction::KeepMatch)
            } else {
                None
            },
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
            until: None,
            block_action: None,
            block_context: None,
            language: None,
            languages: None,
            scope: None,
            exclude_scopes: None,
        };

        PipelineConfig {
            name: Some("Inline Pipeline".to_string()),
            description: Some("Generated from command line pattern".to_string()),
            version: Some("1.0.0".to_string()),
            patterns_include: Vec::new(),
            settings,
            step: vec![step],
            audit: AuditConfig::default(),
            bidirectional: BidirectionalConfig::default(),
            checkpoint: CheckpointConfig::default(),
            cross_file: CrossFileConfig::default(),
            tests: Vec::new(),
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
            if step.pattern.is_empty() {
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
                        (FilterAction::KeepLine, FilterAction::DropLine)
                            | (FilterAction::DropLine, FilterAction::KeepLine)
                            | (FilterAction::KeepMatch, FilterAction::DropMatch)
                            | (FilterAction::DropMatch, FilterAction::KeepMatch)
                    );

                    if contradictory {
                        let action1_str = match action1 {
                            FilterAction::KeepLine => "keep_line",
                            FilterAction::DropLine => "drop_line",
                            FilterAction::KeepMatch => "keep_match",
                            FilterAction::DropMatch => "drop_match",
                        };
                        let action2_str = match action2 {
                            FilterAction::KeepLine => "keep_line",
                            FilterAction::DropLine => "drop_line",
                            FilterAction::KeepMatch => "keep_match",
                            FilterAction::DropMatch => "drop_match",
                        };
                        errors.push(format!(
                            "Contradictory filters: Step {} ({} on '{}') conflicts with \
                             Step {} ({} on same pattern). The second filter will have no effect.",
                            idx1 + 1, action1_str, step1.pattern, idx2 + 1, action2_str
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
                            step_num,
                            "action",
                            "filter",
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
                        (FilterAction::KeepLine, FilterAction::DropLine)
                            | (FilterAction::DropLine, FilterAction::KeepLine)
                            | (FilterAction::KeepMatch, FilterAction::DropMatch)
                            | (FilterAction::DropMatch, FilterAction::KeepMatch)
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
        self.step.iter().any(|step| {
            matches!(
                &step.transform,
                Some(TransformAction::Shell { .. })
            )
        })
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
        let err = config.validate().expect_err("Empty pipeline should be invalid");
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
        let err = config.validate().expect_err("Substitute without replacement should be invalid");
        assert!(
            err.iter().any(|e| e.contains("replacement")),
            "Error should mention missing replacement: {:?}",
            err
        );

        config.step[0].replacement = Some("replacement".to_string());
        config.validate().expect("Valid config should pass validation");
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
                    action: Some(FilterAction::KeepLine),
                    enabled: Some(true),
                    ..Default::default()
                },
                PipelineStep {
                    step_type: StepType::Filter,
                    pattern: "ERROR".to_string(),
                    action: Some(FilterAction::DropLine),
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
                    action: Some(FilterAction::KeepLine),
                    enabled: Some(true),
                    ..Default::default()
                },
                PipelineStep {
                    step_type: StepType::Filter,
                    pattern: "ERROR".to_string(),
                    action: Some(FilterAction::KeepLine),
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
        assert!(errors.iter().any(|e| e.contains("${") && e.contains("library")));
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
                    action: Some(FilterAction::KeepLine),
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
                    action: Some(FilterAction::DropLine),
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
                    action: Some(FilterAction::KeepLine),
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
                    action: Some(FilterAction::DropLine),
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
}
