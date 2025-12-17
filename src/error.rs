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

/// Errors related to configuration file handling.
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Configuration file not found
    #[error("Configuration file not found: {path}")]
    NotFound { path: PathBuf },

    /// Failed to read configuration file
    #[error("Failed to read configuration file '{path}': {source}")]
    ReadError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Failed to parse TOML configuration
    #[error("Failed to parse configuration '{path}': {message}")]
    ParseError { path: PathBuf, message: String },

    /// Invalid configuration structure
    #[error("Invalid configuration: {0}")]
    Invalid(String),
}

/// Errors related to regex pattern handling.
#[derive(Error, Debug)]
pub enum PatternError {
    /// Invalid regex syntax
    #[error("Invalid regex pattern '{pattern}': {message}")]
    InvalidRegex { pattern: String, message: String },

    /// PCRE mode requested but feature not enabled
    #[error(
        "PCRE mode requested but the 'pcre' feature is not enabled. Rebuild with: cargo build --features pcre"
    )]
    PcreNotEnabled,

    /// Pattern reference not found in library
    #[error("Unknown pattern reference '${{{name}}}' - not found in library")]
    UnknownReference { name: String },

    /// Potential ReDoS vulnerability detected
    #[error("Pattern may be vulnerable to ReDoS (catastrophic backtracking): {pattern}")]
    PotentialRedos { pattern: String },
}

/// Errors related to pattern library handling.
#[derive(Error, Debug)]
pub enum LibraryError {
    /// Library file not found
    #[error("Pattern library not found: '{name}' (searched: {searched_paths})")]
    NotFound {
        name: String,
        searched_paths: String,
    },

    /// Failed to read library file
    #[error("Failed to read library '{path}': {source}")]
    ReadError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Failed to parse library TOML
    #[error("Failed to parse library '{path}': {message}")]
    ParseError { path: PathBuf, message: String },

    /// Circular include detected
    #[error("Circular pattern library include detected: {cycle}")]
    CircularInclude { cycle: String },

    /// Invalid pattern in library
    #[error("Invalid pattern in library '{library}': {errors}")]
    InvalidPatterns { library: String, errors: String },
}

/// Errors related to pipeline validation.
#[derive(Error, Debug)]
pub enum ValidationError {
    /// Pipeline has no steps
    #[error("Pipeline must contain at least one step")]
    EmptyPipeline,

    /// Step validation error
    #[error("Step {step}: {message}")]
    StepError { step: usize, message: String },

    /// Multiple validation errors
    #[error("Validation failed with {count} errors:\n{errors}")]
    Multiple { count: usize, errors: String },
}

impl From<toml::de::Error> for ConfigError {
    fn from(err: toml::de::Error) -> Self {
        ConfigError::Invalid(err.to_string())
    }
}

impl From<regex::Error> for PatternError {
    fn from(err: regex::Error) -> Self {
        PatternError::InvalidRegex {
            pattern: String::new(),
            message: err.to_string(),
        }
    }
}

#[cfg(feature = "pcre")]
impl From<fancy_regex::Error> for PatternError {
    fn from(err: fancy_regex::Error) -> Self {
        PatternError::InvalidRegex {
            pattern: String::new(),
            message: err.to_string(),
        }
    }
}

/// Result type alias for rexpipe operations.
pub type Result<T> = std::result::Result<T, RexpipeError>;
