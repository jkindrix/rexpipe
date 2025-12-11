use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    #[serde(default)]
    pub settings: PipelineSettings,
    pub step: Vec<PipelineStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PipelineSettings {
    /// Use PCRE2-compatible regex engine (requires pcre feature)
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStep {
    #[serde(rename = "type")]
    pub step_type: StepType,
    pub pattern: String,
    pub replacement: Option<String>,
    pub action: Option<FilterAction>,
    pub flags: Option<Vec<RegexFlag>>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
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

#[derive(Debug, Clone)]
pub struct StepResult {
    pub step_index: usize,
    pub step_type: StepType,
    pub pattern: String,
    pub matches: u64,
    pub transformations: u64,
    pub processing_time_ms: u64,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PipelineError {
    pub step_index: usize,
    pub line_number: u64,
    pub error_type: ErrorType,
    pub message: String,
    pub context: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ErrorType {
    RegexCompilation,
    PatternMatch,
    Substitution,
    IoError,
    ConfigurationError,
}

impl PipelineConfig {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: PipelineConfig = toml::from_str(&content)?;
        Ok(config)
    }

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
            action: if replacement.is_none() { Some(FilterAction::KeepMatch) } else { None },
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
        };

        PipelineConfig {
            name: Some("Inline Pipeline".to_string()),
            description: Some("Generated from command line pattern".to_string()),
            version: Some("1.0.0".to_string()),
            settings,
            step: vec![step],
        }
    }

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
                        errors.push(format!("Step {}: Substitute type requires replacement", i + 1));
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
        let total_time: u64 = self.step_results.iter()
            .map(|r| r.processing_time_ms)
            .sum();

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
            self.context.as_ref().map_or(String::new(), |c| format!("\nContext: {}", c))
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
            settings: PipelineSettings::default(),
            step: vec![],
        };

        assert!(config.validate().is_err());

        config.step.push(PipelineStep {
            step_type: StepType::Substitute,
            pattern: "test".to_string(),
            replacement: None,
            action: None,
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