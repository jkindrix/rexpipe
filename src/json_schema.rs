//! Standardized JSON output schemas for rexpipe
//!
//! This module provides consistent JSON output structures across all modes,
//! including metadata (version, mode) and standardized field naming.

use serde::Serialize;
use std::path::PathBuf;

/// Standard version string for JSON output
pub const SCHEMA_VERSION: &str = "1.0";

/// Common metadata included in all JSON responses
#[derive(Debug, Clone, Serialize)]
pub struct Metadata {
    /// Schema version for forward compatibility
    pub schema_version: &'static str,
    /// The mode that generated this output
    pub mode: String,
    /// Tool version
    pub tool_version: &'static str,
}

impl Metadata {
    pub fn new(mode: &str) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            mode: mode.to_string(),
            tool_version: env!("CARGO_PKG_VERSION"),
        }
    }
}

/// Standard JSON response envelope
#[derive(Debug, Clone, Serialize)]
pub struct JsonResponse<T: Serialize> {
    /// Metadata about the response
    pub metadata: Metadata,
    /// The actual response data
    pub data: T,
}

impl<T: Serialize> JsonResponse<T> {
    pub fn new(mode: &str, data: T) -> Self {
        Self {
            metadata: Metadata::new(mode),
            data,
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

// ============================================================================
// Specific response types for different modes
// ============================================================================

/// Processing result for single-file/stdin processing.
///
/// Used as the data payload in JSON output for basic processing operations.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessingResult {
    /// Number of lines processed
    pub lines_processed: u64,
    /// Number of matches found across all lines
    pub matches_found: u64,
    /// Number of transformations applied
    pub transformations_applied: u64,
    /// Success rate as a value between 0.0 and 1.0
    pub success_rate: f64,
}

/// Count result for --count mode
#[derive(Debug, Clone, Serialize)]
pub struct CountResult {
    pub lines_processed: u64,
    pub matches_found: u64,
    pub transformations_applied: u64,
}

/// File result for multi-file operations
#[derive(Debug, Clone, Serialize)]
pub struct FileResultJson {
    pub path: String,
    pub matches_found: u64,
    pub lines_processed: u64,
    pub modified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Multi-file processing result
#[derive(Debug, Clone, Serialize)]
pub struct MultiFileResultJson {
    pub summary: MultiFileSummary,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileResultJson>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

/// Summary statistics for multi-file processing
#[derive(Debug, Clone, Serialize)]
pub struct MultiFileSummary {
    pub files_processed: u64,
    pub files_matched: u64,
    pub files_modified: u64,
    pub total_matches: u64,
    pub total_lines: u64,
}

/// File list response (for -l/-L modes)
#[derive(Debug, Clone, Serialize)]
pub struct FileListResult {
    pub count: usize,
    pub files: Vec<String>,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceResult {
    pub lines_processed: u64,
    pub matches_found: u64,
    pub transformations_applied: u64,
    pub success_rate: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_processed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines_per_second: Option<f64>,
}

/// Structured error response for machine-parseable error output.
///
/// This provides machine-readable errors with categorized exit codes,
/// making it easy for scripts and programs to handle errors programmatically.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorResult {
    /// Human-readable error message
    pub message: String,
    /// Error category (config, pattern, io, validation, general)
    pub category: String,
    /// Unix-style exit code for this error type
    pub exit_code: i32,
    /// Optional additional context about the error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    /// Optional suggestion for how to fix the error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

// ============================================================================
// Conversion helpers
// ============================================================================

impl From<&crate::files::MultiFileResult> for MultiFileResultJson {
    fn from(result: &crate::files::MultiFileResult) -> Self {
        Self {
            summary: MultiFileSummary {
                files_processed: result.files_processed,
                files_matched: result.files_matched,
                files_modified: result.files_modified,
                total_matches: result.total_matches,
                total_lines: result.total_lines,
            },
            files: result
                .file_results
                .iter()
                .map(|f| FileResultJson {
                    path: f.path.display().to_string(),
                    matches_found: f.matches_found,
                    lines_processed: f.lines_processed,
                    modified: f.modified,
                    error: f.error.clone(),
                })
                .collect(),
            errors: result.errors.clone(),
        }
    }
}

impl From<&crate::pipeline::PipelineResult> for ProcessingResult {
    fn from(result: &crate::pipeline::PipelineResult) -> Self {
        Self {
            lines_processed: result.lines_processed,
            matches_found: result.matches_found,
            transformations_applied: result.transformations_applied,
            success_rate: result.success_rate(),
        }
    }
}

impl From<&crate::pipeline::PipelineResult> for CountResult {
    fn from(result: &crate::pipeline::PipelineResult) -> Self {
        Self {
            lines_processed: result.lines_processed,
            matches_found: result.matches_found,
            transformations_applied: result.transformations_applied,
        }
    }
}

/// Convert a list of paths to a FileListResult
pub fn paths_to_file_list(paths: &[PathBuf]) -> FileListResult {
    FileListResult {
        count: paths.len(),
        files: paths.iter().map(|p| p.display().to_string()).collect(),
    }
}

// ============================================================================
// Output helpers
// ============================================================================

/// Output a standardized JSON response for processing results.
///
/// Converts a PipelineResult into a JSON string with metadata envelope.
pub fn output_processing_json(
    result: &crate::pipeline::PipelineResult,
) -> Result<String, serde_json::Error> {
    let data = ProcessingResult::from(result);
    let response = JsonResponse::new("processing", data);
    response.to_json()
}

/// Output a standardized JSON response for count mode
pub fn output_count_json(
    result: &crate::pipeline::PipelineResult,
) -> Result<String, serde_json::Error> {
    let data = CountResult::from(result);
    let response = JsonResponse::new("count", data);
    response.to_json()
}

/// Output a standardized JSON response for multi-file results
pub fn output_multi_file_json(
    result: &crate::files::MultiFileResult,
) -> Result<String, serde_json::Error> {
    let data = MultiFileResultJson::from(result);
    let response = JsonResponse::new("multi_file", data);
    response.to_json()
}

/// Output a standardized JSON response for file lists
pub fn output_file_list_json(paths: &[PathBuf], mode: &str) -> Result<String, serde_json::Error> {
    let data = paths_to_file_list(paths);
    let response = JsonResponse::new(mode, data);
    response.to_json()
}

/// Output a single file result as compact JSONL (JSON Lines format).
///
/// This is used for streaming output where each file's result is printed
/// as it's processed, rather than buffering all results. Each line is a
/// valid JSON object representing one file's result.
///
/// # Example Output
/// ```jsonl
/// {"path":"file1.txt","matches_found":5,"lines_processed":100,"modified":true}
/// {"path":"file2.txt","matches_found":0,"lines_processed":50,"modified":false}
/// ```
pub fn output_file_result_jsonl(
    result: &crate::files::FileResult,
) -> Result<String, serde_json::Error> {
    let data = FileResultJson {
        path: result.path.display().to_string(),
        matches_found: result.matches_found,
        lines_processed: result.lines_processed,
        modified: result.modified,
        error: result.error.clone(),
    };
    // Compact JSON for JSONL (no pretty printing, no newlines)
    serde_json::to_string(&data)
}

/// Output a streaming summary as JSONL footer.
///
/// This is printed after all file results to provide aggregate statistics.
pub fn output_streaming_summary_jsonl(
    result: &crate::files::MultiFileResult,
) -> Result<String, serde_json::Error> {
    let data = MultiFileSummary {
        files_processed: result.files_processed,
        files_matched: result.files_matched,
        files_modified: result.files_modified,
        total_matches: result.total_matches,
        total_lines: result.total_lines,
    };
    serde_json::to_string(&data)
}

/// Output a standardized JSON response for performance metrics
pub fn output_performance_json(
    result: &crate::pipeline::PipelineResult,
) -> Result<String, serde_json::Error> {
    let data = PerformanceResult {
        lines_processed: result.lines_processed,
        matches_found: result.matches_found,
        transformations_applied: result.transformations_applied,
        success_rate: result.success_rate(),
        bytes_processed: None,
        duration_ms: None,
        lines_per_second: None,
    };
    let response = JsonResponse::new("performance", data);
    response.to_json()
}

/// Output a standardized JSON error response.
///
/// Converts an error and exit code into a structured JSON response that
/// scripts and programs can parse and handle programmatically.
///
/// # Example Output
/// ```json
/// {
///   "metadata": {
///     "schema_version": "1.0",
///     "mode": "error",
///     "tool_version": "1.1.0"
///   },
///   "data": {
///     "message": "Invalid regex pattern",
///     "category": "pattern",
///     "exit_code": 4,
///     "suggestion": "Check regex syntax"
///   }
/// }
/// ```
pub fn output_error_json(
    message: &str,
    exit_code: i32,
    details: Option<&str>,
) -> Result<String, serde_json::Error> {
    let category = match exit_code {
        2 => "usage",
        3 => "config",
        4 => "pattern",
        5 => "io",
        6 => "validation",
        _ => "general",
    };

    let suggestion = match exit_code {
        2 => Some("Run 'rexpipe --help' for usage information"),
        3 => Some("Check configuration file syntax and paths"),
        4 => Some("Verify regex pattern syntax; use --fixed for literal strings"),
        5 => Some("Check file paths and permissions"),
        6 => Some("Review pipeline configuration for validation errors"),
        _ => None,
    };

    let data = ErrorResult {
        message: message.to_string(),
        category: category.to_string(),
        exit_code,
        details: details.map(|s| s.to_string()),
        suggestion: suggestion.map(|s| s.to_string()),
    };
    let response = JsonResponse::new("error", data);
    response.to_json()
}
