use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    /// Pattern libraries to include (supports ${pattern_name} references in steps)
    #[serde(default)]
    pub patterns_include: Vec<String>,
    #[serde(default)]
    pub settings: PipelineSettings,
    pub step: Vec<PipelineStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
    #[default]
    Substitute,
    Filter,
    Extract,
    Validate,
    Transform,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterAction {
    KeepLine,
    DropLine,
    KeepMatch,
    DropMatch,
}

/// Actions for Transform step type
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        };

        PipelineConfig {
            name: Some("Inline Pipeline".to_string()),
            description: Some("Generated from command line pattern".to_string()),
            version: Some("1.0.0".to_string()),
            patterns_include: Vec::new(),
            settings,
            step: vec![step],
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
                _ => {}
            }

            if !step.enabled.unwrap_or(true) {
                continue;
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn enabled_steps(&self) -> impl Iterator<Item = &PipelineStep> {
        self.step.iter().filter(|step| step.enabled.unwrap_or(true))
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
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings::default(),
            step: vec![],
        };

        assert!(config.validate().is_err());

        config.step.push(PipelineStep {
            step_type: StepType::Substitute,
            pattern: "test".to_string(),
            replacement: None,
            action: None,
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
        });

        assert!(config.validate().is_err());

        config.step[0].replacement = Some("replacement".to_string());
        assert!(config.validate().is_ok());
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
}
