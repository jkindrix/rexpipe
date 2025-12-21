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
    fn test_validation_missing_field() {
        let err = ValidationError::missing_field(1, "pattern", "substitute");
        let msg = err.to_string();
        assert!(msg.contains("Step 1"));
        assert!(msg.contains("pattern"));
        assert!(msg.contains("substitute"));
    }

    #[test]
    fn test_error_suggestions() {
        assert!(PatternError::PcreNotEnabled.suggestion().is_some());
        assert!(ValidationError::EmptyPipeline.suggestion().is_some());
        assert!(ConfigError::NotFound { path: PathBuf::from("test") }.suggestion().is_some());
    }
}
