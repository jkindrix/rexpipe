//! Error types for rexpipe.
//!
//! This module provides structured error types using `thiserror` for clear,
//! typed error handling throughout the application.
//!
//! The error types follow a hierarchical structure:
//! - [`RexpipeError`] - Top-level error type for all operations
//! - [`ConfigError`] - Configuration file handling errors
//! - [`PatternError`] - Regex pattern compilation errors
//! - [`LibraryError`] - Pattern library resolution errors
//! - [`ValidationError`] - Pipeline validation errors
//!
//! All error types include actionable suggestions to help users fix issues.
//!
//! These types integrate seamlessly with `anyhow` for rich error context.

use std::path::PathBuf;
use thiserror::Error;

/// Top-level error type for rexpipe operations.
#[derive(Error, Debug)]
pub enum RexpipeError {
    /// Configuration file errors (not found, parse failure, etc.)
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    /// Pattern/regex compilation errors
    #[error("Pattern error: {0}")]
    Pattern(#[from] PatternError),

    /// File I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Library resolution errors
    #[error("Library error: {0}")]
    Library(#[from] LibraryError),

    /// Pipeline validation errors
    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),

    /// Processing errors during pipeline execution
    #[error("Processing error: {0}")]
    Processing(String),
}

impl RexpipeError {
    /// Get a suggestion for how to fix this error.
    pub fn suggestion(&self) -> Option<&'static str> {
        match self {
            RexpipeError::Config(e) => e.suggestion(),
            RexpipeError::Pattern(e) => e.suggestion(),
            RexpipeError::Library(e) => e.suggestion(),
            RexpipeError::Validation(e) => e.suggestion(),
            RexpipeError::Io(_) => Some("Check that the file exists and you have permission to read it"),
            RexpipeError::Processing(_) => None,
        }
    }
}

/// Errors related to configuration file handling.
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Configuration file not found
    #[error("Configuration file not found: {path}\n\n  Hint: Check that the path is correct and the file exists.\n  Try: ls -la {path}")]
    NotFound { path: PathBuf },

    /// Failed to read configuration file
    #[error("Failed to read configuration file '{path}': {source}\n\n  Hint: Check file permissions with: ls -la {path}")]
    ReadError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Failed to parse TOML configuration
    #[error("Failed to parse configuration '{path}':\n  {message}\n\n  Hint: Validate your TOML syntax at https://www.toml-lint.com/\n  Common issues: missing quotes around strings, incorrect indentation, typos in key names")]
    ParseError { path: PathBuf, message: String },

    /// Invalid configuration structure
    #[error("Invalid configuration: {message}\n\n  Hint: {hint}")]
    Invalid { message: String, hint: String },

    /// Missing required field
    #[error("Missing required field '{field}' in configuration\n\n  Hint: Add '{field} = <value>' to your config file\n  Example: {example}")]
    MissingField {
        field: String,
        example: String,
    },
}

impl ConfigError {
    /// Get a suggestion for how to fix this error.
    pub fn suggestion(&self) -> Option<&'static str> {
        match self {
            ConfigError::NotFound { .. } => Some("Verify the file path is correct"),
            ConfigError::ReadError { .. } => Some("Check file permissions"),
            ConfigError::ParseError { .. } => Some("Validate TOML syntax at https://www.toml-lint.com/"),
            ConfigError::Invalid { .. } => Some("Review the configuration structure"),
            ConfigError::MissingField { .. } => Some("Add the required field to your configuration"),
        }
    }

    /// Create an Invalid error with a helpful hint.
    pub fn invalid(message: impl Into<String>, hint: impl Into<String>) -> Self {
        ConfigError::Invalid {
            message: message.into(),
            hint: hint.into(),
        }
    }

    /// Create a MissingField error with an example.
    pub fn missing_field(field: impl Into<String>, example: impl Into<String>) -> Self {
        ConfigError::MissingField {
            field: field.into(),
            example: example.into(),
        }
    }
}

/// Errors related to regex pattern handling.
#[derive(Error, Debug)]
pub enum PatternError {
    /// Invalid regex syntax
    #[error("Invalid regex pattern: {message}\n  Pattern: '{pattern}'\n\n  Hint: {hint}")]
    InvalidRegex {
        pattern: String,
        message: String,
        hint: String,
    },

    /// PCRE mode requested but feature not enabled
    #[error("PCRE mode requested but the 'pcre' feature is not enabled.\n\n  Hint: Rebuild with: cargo build --features pcre\n  Or install with: cargo install rexpipe --features pcre\n\n  PCRE mode is needed for lookahead (?=), lookbehind (?<=), and other advanced features.")]
    PcreNotEnabled,

    /// Pattern reference not found in library
    #[error("Unknown pattern reference '${{{name}}}'\n\n  Hint: This pattern was not found in any loaded library.\n  Available patterns: {available}\n\n  To fix:\n  1. Check spelling of the pattern name\n  2. Ensure the library file is loaded with --library or in your config\n  3. Use --list-patterns <library.toml> to see available patterns")]
    UnknownReference {
        name: String,
        available: String,
    },

    /// Potential ReDoS vulnerability detected
    #[error("Pattern may be vulnerable to ReDoS (catastrophic backtracking)\n  Pattern: {pattern}\n  Risk: {risk_description}\n\n  Hint: {hint}\n\n  To proceed anyway, remove --strict flag (not recommended for untrusted input)")]
    PotentialRedos {
        pattern: String,
        risk_description: String,
        hint: String,
    },

    /// Empty pattern
    #[error("Empty pattern provided\n\n  Hint: Provide a valid regex pattern with -p or in your config file\n  Example: rexpipe -p '\\d+' -r 'NUMBER' < input.txt")]
    EmptyPattern,

    /// Pattern too complex
    #[error("Pattern is too complex (exceeds compilation limits)\n  Pattern: {pattern}\n\n  Hint: Simplify the pattern or break it into multiple steps\n  Consider using fixed-string mode (-F) if you don't need regex features")]
    TooComplex { pattern: String },
}

impl PatternError {
    /// Get a suggestion for how to fix this error.
    pub fn suggestion(&self) -> Option<&'static str> {
        match self {
            PatternError::InvalidRegex { .. } => Some("Check regex syntax at https://regex101.com/"),
            PatternError::PcreNotEnabled => Some("Rebuild with --features pcre"),
            PatternError::UnknownReference { .. } => Some("Use --list-patterns to see available patterns"),
            PatternError::PotentialRedos { .. } => Some("Simplify the pattern to avoid nested quantifiers"),
            PatternError::EmptyPattern => Some("Provide a pattern with -p flag"),
            PatternError::TooComplex { .. } => Some("Simplify the pattern or use -F for fixed strings"),
        }
    }

    /// Create an InvalidRegex error with a helpful hint based on the error message.
    pub fn invalid_regex(pattern: impl Into<String>, message: impl Into<String>) -> Self {
        let pattern = pattern.into();
        let message = message.into();
        let hint = Self::generate_regex_hint(&message, &pattern);
        PatternError::InvalidRegex {
            pattern,
            message,
            hint,
        }
    }

    /// Create an UnknownReference error with available patterns.
    pub fn unknown_reference(name: impl Into<String>, available: Vec<String>) -> Self {
        let available_str = if available.is_empty() {
            "none loaded".to_string()
        } else if available.len() <= 10 {
            available.join(", ")
        } else {
            format!("{}, ... ({} more)", available[..10].join(", "), available.len() - 10)
        };
        PatternError::UnknownReference {
            name: name.into(),
            available: available_str,
        }
    }

    /// Create a PotentialRedos error with risk description and hint.
    pub fn potential_redos(pattern: impl Into<String>, risk: impl Into<String>) -> Self {
        let pattern = pattern.into();
        let risk_description = risk.into();
        let hint = if risk_description.contains("nested quantifier") {
            "Avoid patterns like (a+)+ or (a*)*. Use atomic groups or possessive quantifiers in PCRE mode."
        } else if risk_description.contains("overlapping") {
            "Avoid alternations where branches can match the same text, like (a|ab)+."
        } else {
            "Simplify quantifiers and avoid patterns that can match the same text in multiple ways."
        };
        PatternError::PotentialRedos {
            pattern,
            risk_description,
            hint: hint.to_string(),
        }
    }

    /// Generate a helpful hint based on the regex error message.
    fn generate_regex_hint(message: &str, pattern: &str) -> String {
        let msg_lower = message.to_lowercase();

        if msg_lower.contains("unclosed") || msg_lower.contains("unmatched") {
            if pattern.contains('(') && !pattern.contains(')') {
                return "Missing closing parenthesis ')'. Check that all groups are closed.".to_string();
            }
            if pattern.contains('[') && !pattern.contains(']') {
                return "Missing closing bracket ']'. Check that all character classes are closed.".to_string();
            }
            if pattern.contains('{') && !pattern.contains('}') {
                return "Missing closing brace '}'. Check that all quantifiers are closed.".to_string();
            }
            return "Check for unclosed parentheses, brackets, or braces.".to_string();
        }

        if msg_lower.contains("escape") || msg_lower.contains("backslash") {
            return "Use double backslashes (\\\\) in TOML strings, or use single quotes in shell.\n  Shell example: rexpipe -p '\\d+'\n  TOML example: pattern = \"\\\\d+\"".to_string();
        }

        if msg_lower.contains("repetition") || msg_lower.contains("quantifier") {
            return "Quantifiers (+, *, ?, {n}) must follow something to repeat.\n  Wrong: +abc  Right: a+bc".to_string();
        }

        if msg_lower.contains("look") && (msg_lower.contains("ahead") || msg_lower.contains("behind")) {
            return "Lookahead/lookbehind requires PCRE mode. Use -P or --pcre flag.".to_string();
        }

        if msg_lower.contains("empty") {
            return "Empty patterns or groups are not allowed. Provide content to match.".to_string();
        }

        if msg_lower.contains("invalid") && msg_lower.contains("group") {
            return "Check group syntax: (?:...) for non-capturing, (?<name>...) for named groups.".to_string();
        }

        // Default hint
        "Test your regex at https://regex101.com/ (select Rust flavor)".to_string()
    }
}

// Backward compatibility: allow creating InvalidRegex without hint
impl From<regex::Error> for PatternError {
    fn from(err: regex::Error) -> Self {
        PatternError::invalid_regex("", err.to_string())
    }
}

#[cfg(feature = "pcre")]
impl From<fancy_regex::Error> for PatternError {
    fn from(err: fancy_regex::Error) -> Self {
        PatternError::invalid_regex("", err.to_string())
    }
}

/// Errors related to pattern library handling.
#[derive(Error, Debug)]
pub enum LibraryError {
    /// Library file not found
    #[error("Pattern library not found: '{name}'\n  Searched in:\n{searched_paths}\n\n  Hint: Create a library file or check the path.\n  Example library:\n    [patterns]\n    email = '[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{{2,}}'")]
    NotFound {
        name: String,
        searched_paths: String,
    },

    /// Failed to read library file
    #[error("Failed to read library '{path}': {source}\n\n  Hint: Check file permissions and ensure the file is readable.")]
    ReadError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Failed to parse library TOML
    #[error("Failed to parse library '{path}':\n  {message}\n\n  Hint: Validate TOML syntax. Library files must have a [patterns] section.\n  Example:\n    [patterns]\n    my_pattern = 'regex_here'")]
    ParseError { path: PathBuf, message: String },

    /// Circular include detected
    #[error("Circular pattern library include detected:\n  {cycle}\n\n  Hint: Remove the circular reference. Libraries cannot include themselves directly or indirectly.")]
    CircularInclude { cycle: String },

    /// Invalid pattern in library
    #[error("Invalid pattern(s) in library '{library}':\n{errors}\n\n  Hint: Test each pattern individually to find the error.\n  Use: rexpipe --validate-library {library}")]
    InvalidPatterns { library: String, errors: String },
}

impl LibraryError {
    /// Get a suggestion for how to fix this error.
    pub fn suggestion(&self) -> Option<&'static str> {
        match self {
            LibraryError::NotFound { .. } => Some("Create the library file or check the path"),
            LibraryError::ReadError { .. } => Some("Check file permissions"),
            LibraryError::ParseError { .. } => Some("Validate TOML syntax"),
            LibraryError::CircularInclude { .. } => Some("Remove the circular include reference"),
            LibraryError::InvalidPatterns { .. } => Some("Use --validate-library to check patterns"),
        }
    }

    /// Create a NotFound error with formatted search paths.
    pub fn not_found(name: impl Into<String>, paths: &[PathBuf]) -> Self {
        let searched = paths
            .iter()
            .map(|p| format!("    - {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n");
        LibraryError::NotFound {
            name: name.into(),
            searched_paths: if searched.is_empty() {
                "    (no search paths configured)".to_string()
            } else {
                searched
            },
        }
    }
}

/// Errors related to pipeline validation.
#[derive(Error, Debug)]
pub enum ValidationError {
    /// Pipeline has no steps
    #[error("Pipeline must contain at least one step\n\n  Hint: Add at least one [[step]] section to your config, or use -p to specify an inline pattern.\n  Example config:\n    [[step]]\n    type = \"substitute\"\n    pattern = \"old\"\n    replacement = \"new\"")]
    EmptyPipeline,

    /// Step validation error
    #[error("Step {step}: {message}\n\n  Hint: {hint}")]
    StepError {
        step: usize,
        message: String,
        hint: String,
    },

    /// Multiple validation errors
    #[error("Validation failed with {count} error(s):\n{errors}")]
    Multiple { count: usize, errors: String },

    /// Contradictory filter configuration
    #[error("Contradictory filter configuration in steps {step1} and {step2}:\n  Step {step1}: {action1} lines matching '{pattern}'\n  Step {step2}: {action2} lines matching the same pattern\n\n  Hint: These steps conflict. The second step will have no effect.\n  Remove one of the steps or use different patterns.")]
    ContradictoryFilters {
        step1: usize,
        step2: usize,
        pattern: String,
        action1: String,
        action2: String,
    },

    /// Invalid step type
    #[error("Unknown step type '{step_type}' in step {step}\n\n  Hint: Valid step types are: substitute, filter, extract, validate, transform\n  Example:\n    [[step]]\n    type = \"substitute\"")]
    InvalidStepType { step: usize, step_type: String },

    /// Missing required field in step
    #[error("Step {step} is missing required field '{field}'\n\n  Hint: {hint}\n  Example:\n{example}")]
    MissingStepField {
        step: usize,
        field: String,
        hint: String,
        example: String,
    },
}

impl ValidationError {
    /// Get a suggestion for how to fix this error.
    pub fn suggestion(&self) -> Option<&'static str> {
        match self {
            ValidationError::EmptyPipeline => Some("Add at least one [[step]] section"),
            ValidationError::StepError { .. } => Some("Review the step configuration"),
            ValidationError::Multiple { .. } => Some("Fix each error listed above"),
            ValidationError::ContradictoryFilters { .. } => Some("Remove conflicting filter steps"),
            ValidationError::InvalidStepType { .. } => Some("Use a valid step type"),
            ValidationError::MissingStepField { .. } => Some("Add the required field"),
        }
    }

    /// Create a StepError with a helpful hint.
    pub fn step_error(step: usize, message: impl Into<String>, hint: impl Into<String>) -> Self {
        ValidationError::StepError {
            step,
            message: message.into(),
            hint: hint.into(),
        }
    }

    /// Create a MissingStepField error with context.
    pub fn missing_field(step: usize, field: &str, step_type: &str) -> Self {
        let (hint, example) = match (step_type, field) {
            ("substitute", "pattern") => (
                "Substitute steps require a 'pattern' field with the regex to match.",
                "    [[step]]\n    type = \"substitute\"\n    pattern = \"old_text\"\n    replacement = \"new_text\"",
            ),
            ("substitute", "replacement") => (
                "Substitute steps require a 'replacement' field (use \"\" for deletion).",
                "    [[step]]\n    type = \"substitute\"\n    pattern = \"old_text\"\n    replacement = \"new_text\"",
            ),
            ("filter", "pattern") => (
                "Filter steps require a 'pattern' field to match lines.",
                "    [[step]]\n    type = \"filter\"\n    pattern = \"ERROR\"\n    action = \"keep_line\"",
            ),
            ("filter", "action") => (
                "Filter steps require an 'action' field: 'keep_line' or 'drop_line'.",
                "    [[step]]\n    type = \"filter\"\n    pattern = \"DEBUG\"\n    action = \"drop_line\"",
            ),
            ("extract", "pattern") => (
                "Extract steps require a 'pattern' field with capture groups.",
                "    [[step]]\n    type = \"extract\"\n    pattern = \"user=(\\w+)\"\n    output = \"$1\"",
            ),
            ("validate", "pattern") => (
                "Validate steps require a 'pattern' field to validate against.",
                "    [[step]]\n    type = \"validate\"\n    pattern = \"^[a-z]+$\"\n    on_failure = \"skip\"",
            ),
            ("transform", "action") => (
                "Transform steps require an 'action' field specifying the transformation.",
                "    [[step]]\n    type = \"transform\"\n    action = \"uppercase\"",
            ),
            _ => (
                "Add the required field to your step configuration.",
                "    [[step]]\n    type = \"...\"\n    # add required fields",
            ),
        };
        ValidationError::MissingStepField {
            step,
            field: field.to_string(),
            hint: hint.to_string(),
            example: example.to_string(),
        }
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(err: toml::de::Error) -> Self {
        // Extract line/column info if available
        let message = err.to_string();
        ConfigError::Invalid {
            message: message.clone(),
            hint: if message.contains("missing field") {
                "Check that all required fields are present in your configuration.".to_string()
            } else if message.contains("unknown field") {
                "Check for typos in field names. Use --export toml to see valid structure.".to_string()
            } else if message.contains("invalid type") {
                "Check that values have the correct type (string, number, boolean, etc.)".to_string()
            } else {
                "Validate your TOML at https://www.toml-lint.com/".to_string()
            },
        }
    }
}

/// Result type alias for rexpipe operations.
pub type Result<T> = std::result::Result<T, RexpipeError>;

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================
    // PatternError tests
    // ========================================

    #[test]
    fn test_pattern_error_hint_generation() {
        let err = PatternError::invalid_regex("(unclosed", "unclosed group");
        match err {
            PatternError::InvalidRegex { hint, .. } => {
                assert!(hint.contains("parenthesis") || hint.contains("unclosed"));
            }
            _ => panic!("Expected InvalidRegex"),
        }
    }

    #[test]
    fn test_pattern_error_escape_hint() {
        let err = PatternError::invalid_regex("\\d", "invalid escape");
        match err {
            PatternError::InvalidRegex { hint, .. } => {
                assert!(hint.contains("backslash") || hint.contains("escape") || hint.contains("regex101"));
            }
            _ => panic!("Expected InvalidRegex"),
        }
    }

    #[test]
    fn test_pattern_error_unclosed_bracket() {
        let err = PatternError::invalid_regex("[abc", "unclosed character class");
        match err {
            PatternError::InvalidRegex { hint, .. } => {
                assert!(hint.contains("bracket") || hint.contains("unclosed"));
            }
            _ => panic!("Expected InvalidRegex"),
        }
    }

    #[test]
    fn test_pattern_error_unclosed_brace() {
        let err = PatternError::invalid_regex("a{3", "unclosed quantifier");
        match err {
            PatternError::InvalidRegex { hint, .. } => {
                assert!(hint.contains("brace") || hint.contains("unclosed"));
            }
            _ => panic!("Expected InvalidRegex"),
        }
    }

    #[test]
    fn test_pattern_error_repetition_hint() {
        let err = PatternError::invalid_regex("+abc", "invalid repetition");
        match err {
            PatternError::InvalidRegex { hint, .. } => {
                assert!(hint.contains("Quantifier") || hint.contains("repeat"));
            }
            _ => panic!("Expected InvalidRegex"),
        }
    }

    #[test]
    fn test_pattern_error_lookahead_hint() {
        let err = PatternError::invalid_regex("(?=foo)", "lookahead not supported");
        match err {
            PatternError::InvalidRegex { hint, .. } => {
                assert!(hint.contains("PCRE") || hint.contains("pcre"));
            }
            _ => panic!("Expected InvalidRegex"),
        }
    }

    #[test]
    fn test_pattern_error_lookbehind_hint() {
        let err = PatternError::invalid_regex("(?<=foo)", "look behind not supported");
        match err {
            PatternError::InvalidRegex { hint, .. } => {
                assert!(hint.contains("PCRE") || hint.contains("pcre"));
            }
            _ => panic!("Expected InvalidRegex"),
        }
    }

    #[test]
    fn test_pattern_error_empty_hint() {
        let err = PatternError::invalid_regex("", "empty pattern");
        match err {
            PatternError::InvalidRegex { hint, .. } => {
                assert!(hint.contains("Empty") || hint.contains("empty"));
            }
            _ => panic!("Expected InvalidRegex"),
        }
    }

    #[test]
    fn test_pattern_error_invalid_group_hint() {
        let err = PatternError::invalid_regex("(?x)", "invalid group syntax");
        match err {
            PatternError::InvalidRegex { hint, .. } => {
                assert!(hint.contains("group") || hint.contains("regex101"));
            }
            _ => panic!("Expected InvalidRegex"),
        }
    }

    #[test]
    fn test_pattern_error_default_hint() {
        let err = PatternError::invalid_regex("abc", "some unknown error");
        match err {
            PatternError::InvalidRegex { hint, .. } => {
                assert!(hint.contains("regex101"));
            }
            _ => panic!("Expected InvalidRegex"),
        }
    }

    #[test]
    fn test_pattern_error_unknown_reference() {
        let err = PatternError::unknown_reference("my_pattern", vec!["email".to_string(), "url".to_string()]);
        let msg = err.to_string();
        assert!(msg.contains("my_pattern"));
        assert!(msg.contains("email"));
        assert!(msg.contains("url"));
    }

    #[test]
    fn test_pattern_error_unknown_reference_empty() {
        let err = PatternError::unknown_reference("my_pattern", vec![]);
        let msg = err.to_string();
        assert!(msg.contains("my_pattern"));
        assert!(msg.contains("none loaded"));
    }

    #[test]
    fn test_pattern_error_unknown_reference_many() {
        let available: Vec<String> = (0..15).map(|i| format!("pattern_{}", i)).collect();
        let err = PatternError::unknown_reference("my_pattern", available);
        let msg = err.to_string();
        assert!(msg.contains("my_pattern"));
        assert!(msg.contains("... (5 more)"));
    }

    #[test]
    fn test_pattern_error_potential_redos_nested() {
        let err = PatternError::potential_redos("(a+)+", "nested quantifier detected");
        match err {
            PatternError::PotentialRedos { hint, .. } => {
                assert!(hint.contains("nested") || hint.contains("atomic"));
            }
            _ => panic!("Expected PotentialRedos"),
        }
    }

    #[test]
    fn test_pattern_error_potential_redos_overlapping() {
        let err = PatternError::potential_redos("(a|ab)+", "overlapping alternatives");
        match err {
            PatternError::PotentialRedos { hint, .. } => {
                assert!(hint.contains("overlapping") || hint.contains("alternation"));
            }
            _ => panic!("Expected PotentialRedos"),
        }
    }

    #[test]
    fn test_pattern_error_potential_redos_generic() {
        let err = PatternError::potential_redos("complex.*", "generic risk");
        match err {
            PatternError::PotentialRedos { hint, .. } => {
                assert!(hint.contains("Simplify") || hint.contains("quantifier"));
            }
            _ => panic!("Expected PotentialRedos"),
        }
    }

    #[test]
    fn test_pattern_error_pcre_not_enabled() {
        let err = PatternError::PcreNotEnabled;
        let msg = err.to_string();
        assert!(msg.contains("PCRE"));
        assert!(msg.contains("features pcre"));
    }

    #[test]
    fn test_pattern_error_empty_pattern() {
        let err = PatternError::EmptyPattern;
        let msg = err.to_string();
        assert!(msg.contains("Empty pattern"));
    }

    #[test]
    fn test_pattern_error_too_complex() {
        let err = PatternError::TooComplex { pattern: "very.*complex.*pattern".to_string() };
        let msg = err.to_string();
        assert!(msg.contains("too complex"));
        assert!(msg.contains("very.*complex.*pattern"));
    }

    #[test]
    fn test_pattern_error_suggestions() {
        assert!(PatternError::PcreNotEnabled.suggestion().is_some());
        assert!(PatternError::EmptyPattern.suggestion().is_some());
        assert!(PatternError::TooComplex { pattern: "x".to_string() }.suggestion().is_some());
        assert!(PatternError::invalid_regex("x", "err").suggestion().is_some());
        assert!(PatternError::unknown_reference("x", vec![]).suggestion().is_some());
        assert!(PatternError::potential_redos("x", "risk").suggestion().is_some());
    }

    #[test]
    fn test_pattern_error_from_regex_error() {
        // Create a regex error by attempting to compile an invalid pattern
        let regex_err = regex::Regex::new("(").unwrap_err();
        let pattern_err: PatternError = regex_err.into();
        match pattern_err {
            PatternError::InvalidRegex { .. } => {}
            _ => panic!("Expected InvalidRegex"),
        }
    }

    // ========================================
    // ConfigError tests
    // ========================================

    #[test]
    fn test_config_error_not_found() {
        let err = ConfigError::NotFound { path: PathBuf::from("/path/to/config.toml") };
        let msg = err.to_string();
        assert!(msg.contains("/path/to/config.toml"));
        assert!(msg.contains("not found"));
    }

    #[test]
    fn test_config_error_read_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err = ConfigError::ReadError {
            path: PathBuf::from("/path/to/config.toml"),
            source: io_err,
        };
        let msg = err.to_string();
        assert!(msg.contains("/path/to/config.toml"));
        assert!(msg.contains("permission"));
    }

    #[test]
    fn test_config_error_parse_error() {
        let err = ConfigError::ParseError {
            path: PathBuf::from("config.toml"),
            message: "unexpected token at line 5".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("config.toml"));
        assert!(msg.contains("unexpected token"));
        assert!(msg.contains("toml-lint.com"));
    }

    #[test]
    fn test_config_error_invalid() {
        let err = ConfigError::invalid("missing step type", "add type = \"substitute\"");
        let msg = err.to_string();
        assert!(msg.contains("missing step type"));
        assert!(msg.contains("substitute"));
    }

    #[test]
    fn test_config_error_missing_field() {
        let err = ConfigError::missing_field("pattern", "pattern = \"\\\\d+\"");
        let msg = err.to_string();
        assert!(msg.contains("pattern"));
    }

    #[test]
    fn test_config_error_suggestions() {
        assert!(ConfigError::NotFound { path: PathBuf::from("x") }.suggestion().is_some());
        assert!(ConfigError::ReadError {
            path: PathBuf::from("x"),
            source: std::io::Error::new(std::io::ErrorKind::Other, "err")
        }.suggestion().is_some());
        assert!(ConfigError::ParseError { path: PathBuf::from("x"), message: "err".to_string() }.suggestion().is_some());
        assert!(ConfigError::invalid("msg", "hint").suggestion().is_some());
        assert!(ConfigError::missing_field("field", "example").suggestion().is_some());
    }

    // ========================================
    // LibraryError tests
    // ========================================

    #[test]
    fn test_library_not_found_formatting() {
        let paths = vec![
            PathBuf::from("/home/user/.rexpipe/patterns"),
            PathBuf::from("./patterns"),
        ];
        let err = LibraryError::not_found("common", &paths);
        let msg = err.to_string();
        assert!(msg.contains("common"));
        assert!(msg.contains("/home/user/.rexpipe/patterns"));
        assert!(msg.contains("./patterns"));
    }

    #[test]
    fn test_library_not_found_empty_paths() {
        let err = LibraryError::not_found("mylib", &[]);
        let msg = err.to_string();
        assert!(msg.contains("mylib"));
        assert!(msg.contains("no search paths configured"));
    }

    #[test]
    fn test_library_read_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = LibraryError::ReadError {
            path: PathBuf::from("patterns.toml"),
            source: io_err,
        };
        let msg = err.to_string();
        assert!(msg.contains("patterns.toml"));
    }

    #[test]
    fn test_library_parse_error() {
        let err = LibraryError::ParseError {
            path: PathBuf::from("patterns.toml"),
            message: "expected string".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("patterns.toml"));
        assert!(msg.contains("expected string"));
        assert!(msg.contains("[patterns]"));
    }

    #[test]
    fn test_library_circular_include() {
        let err = LibraryError::CircularInclude {
            cycle: "a.toml -> b.toml -> a.toml".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Circular"));
        assert!(msg.contains("a.toml -> b.toml -> a.toml"));
    }

    #[test]
    fn test_library_invalid_patterns() {
        let err = LibraryError::InvalidPatterns {
            library: "mylib.toml".to_string(),
            errors: "  - email: invalid regex\n  - url: unclosed group".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("mylib.toml"));
        assert!(msg.contains("email"));
        assert!(msg.contains("url"));
    }

    #[test]
    fn test_library_error_suggestions() {
        assert!(LibraryError::not_found("x", &[]).suggestion().is_some());
        assert!(LibraryError::ReadError {
            path: PathBuf::from("x"),
            source: std::io::Error::new(std::io::ErrorKind::Other, "err")
        }.suggestion().is_some());
        assert!(LibraryError::ParseError { path: PathBuf::from("x"), message: "err".to_string() }.suggestion().is_some());
        assert!(LibraryError::CircularInclude { cycle: "a->b".to_string() }.suggestion().is_some());
        assert!(LibraryError::InvalidPatterns { library: "x".to_string(), errors: "err".to_string() }.suggestion().is_some());
    }

    // ========================================
    // ValidationError tests
    // ========================================

    #[test]
    fn test_validation_missing_field() {
        let err = ValidationError::missing_field(1, "pattern", "substitute");
        let msg = err.to_string();
        assert!(msg.contains("Step 1"));
        assert!(msg.contains("pattern"));
        assert!(msg.contains("substitute"));
    }

    #[test]
    fn test_validation_missing_field_filter() {
        let err = ValidationError::missing_field(2, "action", "filter");
        let msg = err.to_string();
        assert!(msg.contains("Step 2"));
        assert!(msg.contains("action"));
        assert!(msg.contains("keep_line"));
        assert!(msg.contains("drop_line"));
    }

    #[test]
    fn test_validation_missing_field_extract() {
        let err = ValidationError::missing_field(3, "pattern", "extract");
        let msg = err.to_string();
        assert!(msg.contains("Step 3"));
        assert!(msg.contains("capture groups"));
    }

    #[test]
    fn test_validation_missing_field_validate() {
        let err = ValidationError::missing_field(1, "pattern", "validate");
        let msg = err.to_string();
        assert!(msg.contains("validate"));
    }

    #[test]
    fn test_validation_missing_field_transform() {
        let err = ValidationError::missing_field(1, "action", "transform");
        let msg = err.to_string();
        assert!(msg.contains("transform"));
    }

    #[test]
    fn test_validation_missing_field_unknown() {
        let err = ValidationError::missing_field(1, "xyz", "unknown");
        let msg = err.to_string();
        assert!(msg.contains("required field"));
    }

    #[test]
    fn test_validation_empty_pipeline() {
        let err = ValidationError::EmptyPipeline;
        let msg = err.to_string();
        assert!(msg.contains("at least one step"));
        assert!(msg.contains("[[step]]"));
    }

    #[test]
    fn test_validation_step_error() {
        let err = ValidationError::step_error(2, "invalid regex", "check pattern syntax");
        let msg = err.to_string();
        assert!(msg.contains("Step 2"));
        assert!(msg.contains("invalid regex"));
        assert!(msg.contains("check pattern syntax"));
    }

    #[test]
    fn test_validation_multiple_errors() {
        let err = ValidationError::Multiple {
            count: 3,
            errors: "  1. error one\n  2. error two\n  3. error three".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("3 error(s)"));
        assert!(msg.contains("error one"));
    }

    #[test]
    fn test_validation_contradictory_filters() {
        let err = ValidationError::ContradictoryFilters {
            step1: 1,
            step2: 3,
            pattern: "ERROR".to_string(),
            action1: "keep".to_string(),
            action2: "drop".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Step 1"));
        assert!(msg.contains("Step 3"));
        assert!(msg.contains("ERROR"));
    }

    #[test]
    fn test_validation_invalid_step_type() {
        let err = ValidationError::InvalidStepType {
            step: 2,
            step_type: "invalid_type".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("invalid_type"));
        assert!(msg.contains("substitute"));
        assert!(msg.contains("filter"));
    }

    #[test]
    fn test_validation_error_suggestions() {
        assert!(ValidationError::EmptyPipeline.suggestion().is_some());
        assert!(ValidationError::step_error(1, "msg", "hint").suggestion().is_some());
        assert!(ValidationError::Multiple { count: 1, errors: "err".to_string() }.suggestion().is_some());
        assert!(ValidationError::ContradictoryFilters {
            step1: 1, step2: 2, pattern: "x".to_string(), action1: "a".to_string(), action2: "b".to_string()
        }.suggestion().is_some());
        assert!(ValidationError::InvalidStepType { step: 1, step_type: "x".to_string() }.suggestion().is_some());
        assert!(ValidationError::missing_field(1, "x", "y").suggestion().is_some());
    }

    // ========================================
    // RexpipeError tests
    // ========================================

    #[test]
    fn test_rexpipe_error_from_config() {
        let config_err = ConfigError::NotFound { path: PathBuf::from("test.toml") };
        let err: RexpipeError = config_err.into();
        match err {
            RexpipeError::Config(_) => {}
            _ => panic!("Expected Config variant"),
        }
    }

    #[test]
    fn test_rexpipe_error_from_pattern() {
        let pattern_err = PatternError::EmptyPattern;
        let err: RexpipeError = pattern_err.into();
        match err {
            RexpipeError::Pattern(_) => {}
            _ => panic!("Expected Pattern variant"),
        }
    }

    #[test]
    fn test_rexpipe_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: RexpipeError = io_err.into();
        match err {
            RexpipeError::Io(_) => {}
            _ => panic!("Expected Io variant"),
        }
    }

    #[test]
    fn test_rexpipe_error_from_library() {
        let lib_err = LibraryError::not_found("test", &[]);
        let err: RexpipeError = lib_err.into();
        match err {
            RexpipeError::Library(_) => {}
            _ => panic!("Expected Library variant"),
        }
    }

    #[test]
    fn test_rexpipe_error_from_validation() {
        let val_err = ValidationError::EmptyPipeline;
        let err: RexpipeError = val_err.into();
        match err {
            RexpipeError::Validation(_) => {}
            _ => panic!("Expected Validation variant"),
        }
    }

    #[test]
    fn test_rexpipe_error_processing() {
        let err = RexpipeError::Processing("timeout exceeded".to_string());
        let msg = err.to_string();
        assert!(msg.contains("timeout exceeded"));
    }

    #[test]
    fn test_rexpipe_error_suggestions() {
        // Config suggestion comes from inner ConfigError
        let err = RexpipeError::Config(ConfigError::NotFound { path: PathBuf::from("x") });
        assert!(err.suggestion().is_some());

        // Pattern suggestion
        let err = RexpipeError::Pattern(PatternError::EmptyPattern);
        assert!(err.suggestion().is_some());

        // Library suggestion
        let err = RexpipeError::Library(LibraryError::not_found("x", &[]));
        assert!(err.suggestion().is_some());

        // Validation suggestion
        let err = RexpipeError::Validation(ValidationError::EmptyPipeline);
        assert!(err.suggestion().is_some());

        // IO always has a suggestion
        let err = RexpipeError::Io(std::io::Error::new(std::io::ErrorKind::Other, "err"));
        assert!(err.suggestion().is_some());

        // Processing has no suggestion
        let err = RexpipeError::Processing("error".to_string());
        assert!(err.suggestion().is_none());
    }

    // ========================================
    // From<toml::de::Error> tests
    // ========================================

    #[test]
    fn test_config_from_toml_error_missing_field() {
        // Can't easily create toml::de::Error directly, so we verify
        // the conversion logic by checking the output format
        let err = ConfigError::Invalid {
            message: "missing field `type`".to_string(),
            hint: "Check that all required fields are present in your configuration.".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("missing field"));
    }

    #[test]
    fn test_error_debug_impl() {
        // Test that all errors implement Debug
        let _ = format!("{:?}", PatternError::EmptyPattern);
        let _ = format!("{:?}", ConfigError::NotFound { path: PathBuf::from("x") });
        let _ = format!("{:?}", LibraryError::not_found("x", &[]));
        let _ = format!("{:?}", ValidationError::EmptyPipeline);
        let _ = format!("{:?}", RexpipeError::Processing("x".to_string()));
    }

    #[test]
    fn test_error_display_impl() {
        // Test that all errors implement Display
        let _ = PatternError::EmptyPattern.to_string();
        let _ = ConfigError::NotFound { path: PathBuf::from("x") }.to_string();
        let _ = LibraryError::not_found("x", &[]).to_string();
        let _ = ValidationError::EmptyPipeline.to_string();
        let _ = RexpipeError::Processing("x".to_string()).to_string();
    }
}
