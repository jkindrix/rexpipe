//! Natural language interface for rexpipe.
//!
//! This module provides a no-code/natural language mode that allows users
//! to describe text transformations in plain English and have them converted
//! to pipeline configurations.
//!
//! # Examples
//!
//! ```
//! use rexpipe::natural::NaturalLanguageParser;
//! use rexpipe::pipeline::PipelineConfig;
//!
//! let parser = NaturalLanguageParser::new();
//!
//! // Parse a natural language description
//! let config = parser.parse("replace all numbers with NUM").unwrap();
//! assert_eq!(config.step.len(), 1);
//!
//! // Parse multiple operations
//! let config = parser.parse("remove blank lines and replace emails with [EMAIL]").unwrap();
//! assert_eq!(config.step.len(), 2);
//! ```

use crate::pipeline::{
    FilterAction, PipelineConfig, PipelineStep, StepType, TransformAction,
};
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during natural language parsing.
#[derive(Debug, Error)]
pub enum NaturalLanguageError {
    #[error("Could not understand: {0}")]
    NotUnderstood(String),

    #[error("Ambiguous request: {0}")]
    Ambiguous(String),

    #[error("Missing required information: {0}")]
    MissingInfo(String),
}

type Result<T> = std::result::Result<T, NaturalLanguageError>;

/// A parsed intent from natural language.
#[derive(Debug, Clone)]
pub struct ParsedIntent {
    /// The type of operation
    pub operation: IntentOperation,
    /// The target pattern or subject
    pub target: Option<String>,
    /// The replacement or destination
    pub replacement: Option<String>,
    /// Additional modifiers
    pub modifiers: Vec<IntentModifier>,
}

/// Types of operations that can be parsed.
#[derive(Debug, Clone, PartialEq)]
pub enum IntentOperation {
    Replace,
    Remove,
    Keep,
    Drop,
    Extract,
    Transform,
    Validate,
    Mask,
    Redact,
}

/// Modifiers that can be applied to operations.
#[derive(Debug, Clone, PartialEq)]
pub enum IntentModifier {
    CaseInsensitive,
    Global,
    WholeWord,
    FirstOnly,
    LastOnly,
    InvertMatch,
}

/// Built-in pattern definitions for common entities.
#[derive(Debug, Clone)]
pub struct BuiltinPatterns {
    patterns: HashMap<String, &'static str>,
}

impl Default for BuiltinPatterns {
    fn default() -> Self {
        Self::new()
    }
}

impl BuiltinPatterns {
    /// Create a new set of built-in patterns.
    pub fn new() -> Self {
        let mut patterns = HashMap::new();

        // Numbers and digits
        patterns.insert("number".to_string(), r"\d+");
        patterns.insert("numbers".to_string(), r"\d+");
        patterns.insert("digit".to_string(), r"\d");
        patterns.insert("digits".to_string(), r"\d+");
        patterns.insert("integer".to_string(), r"-?\d+");
        patterns.insert("decimal".to_string(), r"-?\d+\.?\d*");
        patterns.insert("float".to_string(), r"-?\d+\.\d+");
        patterns.insert("percentage".to_string(), r"\d+\.?\d*%");

        // Communication
        patterns.insert("email".to_string(), r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}");
        patterns.insert("emails".to_string(), r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}");
        patterns.insert("phone".to_string(), r"[\+]?[(]?[0-9]{3}[)]?[-\s\.]?[0-9]{3}[-\s\.]?[0-9]{4,6}");
        patterns.insert("phone number".to_string(), r"[\+]?[(]?[0-9]{3}[)]?[-\s\.]?[0-9]{3}[-\s\.]?[0-9]{4,6}");
        patterns.insert("url".to_string(), r"https?://[^\s]+");
        patterns.insert("urls".to_string(), r"https?://[^\s]+");
        patterns.insert("link".to_string(), r"https?://[^\s]+");
        patterns.insert("links".to_string(), r"https?://[^\s]+");

        // Network
        patterns.insert("ip".to_string(), r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b");
        patterns.insert("ip address".to_string(), r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b");
        patterns.insert("ipv4".to_string(), r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b");
        patterns.insert("mac".to_string(), r"([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}");
        patterns.insert("mac address".to_string(), r"([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}");

        // Dates and times
        patterns.insert("date".to_string(), r"\d{4}-\d{2}-\d{2}|\d{1,2}/\d{1,2}/\d{2,4}");
        patterns.insert("dates".to_string(), r"\d{4}-\d{2}-\d{2}|\d{1,2}/\d{1,2}/\d{2,4}");
        patterns.insert("time".to_string(), r"\d{1,2}:\d{2}(:\d{2})?(\s*[AaPp][Mm])?");
        patterns.insert("times".to_string(), r"\d{1,2}:\d{2}(:\d{2})?(\s*[AaPp][Mm])?");
        patterns.insert("timestamp".to_string(), r"\d{4}-\d{2}-\d{2}[T\s]\d{2}:\d{2}:\d{2}");
        patterns.insert("iso date".to_string(), r"\d{4}-\d{2}-\d{2}");
        patterns.insert("iso timestamp".to_string(), r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}");

        // Financial
        patterns.insert("currency".to_string(), r"[$€£¥]\s*\d+([.,]\d{1,2})?");
        patterns.insert("money".to_string(), r"[$€£¥]\s*\d+([.,]\d{1,2})?");
        patterns.insert("price".to_string(), r"[$€£¥]\s*\d+([.,]\d{1,2})?");
        patterns.insert("credit card".to_string(), r"\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b");
        patterns.insert("ssn".to_string(), r"\b\d{3}-\d{2}-\d{4}\b");
        patterns.insert("social security".to_string(), r"\b\d{3}-\d{2}-\d{4}\b");

        // Text patterns
        patterns.insert("word".to_string(), r"\b\w+\b");
        patterns.insert("words".to_string(), r"\b\w+\b");
        patterns.insert("whitespace".to_string(), r"\s+");
        patterns.insert("blank line".to_string(), r"^\s*$");
        patterns.insert("blank lines".to_string(), r"^\s*$");
        patterns.insert("empty line".to_string(), r"^$");
        patterns.insert("empty lines".to_string(), r"^$");
        patterns.insert("leading whitespace".to_string(), r"^\s+");
        patterns.insert("trailing whitespace".to_string(), r"\s+$");
        patterns.insert("extra whitespace".to_string(), r"\s{2,}");
        patterns.insert("multiple spaces".to_string(), r" {2,}");

        // Code patterns
        patterns.insert("comment".to_string(), r"//.*|/\*[\s\S]*?\*/|#.*");
        patterns.insert("comments".to_string(), r"//.*|/\*[\s\S]*?\*/|#.*");
        patterns.insert("string".to_string(), r#""[^"]*"|'[^']*'"#);
        patterns.insert("strings".to_string(), r#""[^"]*"|'[^']*'"#);
        patterns.insert("function".to_string(), r"\b\w+\s*\([^)]*\)");
        patterns.insert("hex".to_string(), r"0x[0-9A-Fa-f]+|#[0-9A-Fa-f]{3,8}");
        patterns.insert("uuid".to_string(), r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}");

        // Log levels
        patterns.insert("error".to_string(), r"\b(ERROR|Error|error)\b");
        patterns.insert("errors".to_string(), r"\b(ERROR|Error|error)\b");
        patterns.insert("warning".to_string(), r"\b(WARN|WARNING|Warn|Warning|warn|warning)\b");
        patterns.insert("warnings".to_string(), r"\b(WARN|WARNING|Warn|Warning|warn|warning)\b");
        patterns.insert("debug".to_string(), r"\b(DEBUG|Debug|debug)\b");
        patterns.insert("info".to_string(), r"\b(INFO|Info|info)\b");

        // HTML/XML
        patterns.insert("html tag".to_string(), r"<[^>]+>");
        patterns.insert("html tags".to_string(), r"<[^>]+>");
        patterns.insert("xml tag".to_string(), r"<[^>]+>");
        patterns.insert("xml tags".to_string(), r"<[^>]+>");

        Self { patterns }
    }

    /// Look up a pattern by name.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.patterns.get(&name.to_lowercase()).copied()
    }

    /// Check if a pattern name exists.
    pub fn contains(&self, name: &str) -> bool {
        self.patterns.contains_key(&name.to_lowercase())
    }

    /// Get all available pattern names.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.patterns.keys().map(|s| s.as_str())
    }
}

/// Natural language parser for creating pipelines.
#[derive(Debug)]
pub struct NaturalLanguageParser {
    patterns: BuiltinPatterns,
}

impl Default for NaturalLanguageParser {
    fn default() -> Self {
        Self::new()
    }
}

impl NaturalLanguageParser {
    /// Create a new natural language parser.
    pub fn new() -> Self {
        Self {
            patterns: BuiltinPatterns::new(),
        }
    }

    /// Parse a natural language description into a pipeline configuration.
    pub fn parse(&self, input: &str) -> Result<PipelineConfig> {
        let original = input.trim();
        let lowered = original.to_lowercase();

        // Split on "and" or "then" for multiple operations
        // We need to track positions so we can extract from original
        let mut parts = Vec::new();
        let mut start = 0;
        let mut prev_end = 0;

        for (i, _) in lowered.match_indices(" and ") {
            if i > prev_end {
                parts.push(original[start..i].trim());
                start = i + 5; // " and ".len()
                prev_end = start;
            }
        }
        if start < original.len() {
            parts.push(original[start..].trim());
        }

        // Further split on " then "
        let mut final_parts = Vec::new();
        for part in parts {
            let lowered_part = part.to_lowercase();
            let mut sub_start = 0;
            let mut sub_prev_end = 0;
            for (i, _) in lowered_part.match_indices(" then ") {
                if i > sub_prev_end {
                    final_parts.push(part[sub_start..i].trim());
                    sub_start = i + 6; // " then ".len()
                    sub_prev_end = sub_start;
                }
            }
            if sub_start < part.len() {
                final_parts.push(part[sub_start..].trim());
            }
        }

        let parts: Vec<&str> = final_parts
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();

        if parts.is_empty() {
            return Err(NaturalLanguageError::NotUnderstood(original.to_string()));
        }

        let mut config = PipelineConfig::default();

        for part in parts {
            if let Some(step) = self.parse_single_operation(part)? {
                config.step.push(step);
            }
        }

        if config.step.is_empty() {
            return Err(NaturalLanguageError::NotUnderstood(original.to_string()));
        }

        Ok(config)
    }

    /// Parse a single operation from natural language.
    fn parse_single_operation(&self, input: &str) -> Result<Option<PipelineStep>> {
        let input = input.trim();

        // Check for replacement patterns
        if let Some(step) = self.try_parse_replacement(input)? {
            return Ok(Some(step));
        }

        // Check for removal patterns
        if let Some(step) = self.try_parse_removal(input)? {
            return Ok(Some(step));
        }

        // Check for filter patterns (keep/drop)
        if let Some(step) = self.try_parse_filter(input)? {
            return Ok(Some(step));
        }

        // Check for extraction patterns
        if let Some(step) = self.try_parse_extraction(input)? {
            return Ok(Some(step));
        }

        // Check for transformation patterns
        if let Some(step) = self.try_parse_transformation(input)? {
            return Ok(Some(step));
        }

        // Check for masking/redacting patterns
        if let Some(step) = self.try_parse_masking(input)? {
            return Ok(Some(step));
        }

        Err(NaturalLanguageError::NotUnderstood(input.to_string()))
    }

    /// Try to parse a replacement operation.
    fn try_parse_replacement(&self, input: &str) -> Result<Option<PipelineStep>> {
        let lowered = input.to_lowercase();

        // Patterns like: "replace X with Y", "change X to Y", "substitute X for Y"
        let replacement_patterns = [
            ("replace all ", " with "),
            ("replace ", " with "),
            ("change all ", " to "),
            ("change ", " to "),
            ("substitute ", " for "),
            ("swap ", " for "),
            ("convert ", " to "),
        ];

        for (prefix, separator) in replacement_patterns {
            if let Some(rest_lowered) = lowered.strip_prefix(prefix) {
                if let Some(sep_pos) = rest_lowered.find(separator) {
                    // Get the original text at these positions
                    let rest = &input[prefix.len()..];
                    let target = rest[..sep_pos].trim();
                    let replacement = rest[sep_pos + separator.len()..].trim();
                    return Ok(Some(self.create_substitute_step(target, replacement)?));
                }
            }
        }

        Ok(None)
    }

    /// Try to parse a removal operation.
    fn try_parse_removal(&self, input: &str) -> Result<Option<PipelineStep>> {
        let lowered = input.to_lowercase();

        // Patterns like: "remove X", "delete X", "strip X"
        let removal_prefixes = [
            "remove all ",
            "remove ",
            "delete all ",
            "delete ",
            "strip all ",
            "strip ",
            "erase all ",
            "erase ",
            "clear all ",
            "clear ",
            "get rid of ",
        ];

        for prefix in removal_prefixes {
            if lowered.starts_with(prefix) {
                let target = input[prefix.len()..].trim();
                return Ok(Some(self.create_substitute_step(target, "")?));
            }
        }

        Ok(None)
    }

    /// Try to parse a filter operation.
    fn try_parse_filter(&self, input: &str) -> Result<Option<PipelineStep>> {
        let lowered = input.to_lowercase();

        // Keep patterns
        let keep_prefixes = [
            "keep only lines with ",
            "keep only lines containing ",
            "keep lines with ",
            "keep lines containing ",
            "only keep lines with ",
            "only keep ",
            "show only lines with ",
            "show only ",
            "filter to ",
            "include only ",
        ];

        for prefix in keep_prefixes {
            if lowered.starts_with(prefix) {
                let target = input[prefix.len()..].trim();
                return Ok(Some(self.create_filter_step(target, FilterAction::KeepLine)?));
            }
        }

        // Drop patterns
        let drop_prefixes = [
            "drop lines with ",
            "drop lines containing ",
            "remove lines with ",
            "remove lines containing ",
            "delete lines with ",
            "delete lines containing ",
            "hide lines with ",
            "filter out ",
            "exclude ",
        ];

        for prefix in drop_prefixes {
            if lowered.starts_with(prefix) {
                let target = input[prefix.len()..].trim();
                return Ok(Some(self.create_filter_step(target, FilterAction::DropLine)?));
            }
        }

        // Drop line shorthand
        if lowered.starts_with("drop ") && lowered.contains(" line") {
            let target = input[5..]
                .replace(" lines", "")
                .replace(" line", "");
            return Ok(Some(self.create_filter_step(&target, FilterAction::DropLine)?));
        }

        Ok(None)
    }

    /// Try to parse an extraction operation.
    fn try_parse_extraction(&self, input: &str) -> Result<Option<PipelineStep>> {
        let lowered = input.to_lowercase();
        let extract_prefixes = [
            "extract all ",
            "extract ",
            "find all ",
            "find ",
            "get all ",
            "get ",
            "pull out ",
            "grab ",
        ];

        for prefix in extract_prefixes {
            if lowered.starts_with(prefix) {
                let target = input[prefix.len()..].trim();
                return Ok(Some(self.create_extract_step(target)?));
            }
        }

        Ok(None)
    }

    /// Try to parse a transformation operation.
    fn try_parse_transformation(&self, input: &str) -> Result<Option<PipelineStep>> {
        let lowered = input.to_lowercase();

        // Uppercase
        if lowered.contains("uppercase") || lowered.contains("upper case") || lowered == "to upper" {
            return Ok(Some(PipelineStep {
                step_type: StepType::Transform,
                pattern: ".*".to_string(),
                transform: Some(TransformAction::Uppercase),
                ..Default::default()
            }));
        }

        // Lowercase
        if lowered.contains("lowercase") || lowered.contains("lower case") || lowered == "to lower" {
            return Ok(Some(PipelineStep {
                step_type: StepType::Transform,
                pattern: ".*".to_string(),
                transform: Some(TransformAction::Lowercase),
                ..Default::default()
            }));
        }

        // Titlecase
        if lowered.contains("title case") || lowered.contains("titlecase") || lowered == "capitalize" {
            return Ok(Some(PipelineStep {
                step_type: StepType::Transform,
                pattern: ".*".to_string(),
                transform: Some(TransformAction::TitleCase),
                ..Default::default()
            }));
        }

        // Trim
        if lowered == "trim" || lowered.contains("trim whitespace") || lowered.contains("trim spaces") {
            return Ok(Some(PipelineStep {
                step_type: StepType::Transform,
                pattern: ".*".to_string(),
                transform: Some(TransformAction::Trim),
                ..Default::default()
            }));
        }

        // Normalize whitespace
        if lowered.contains("normalize whitespace")
            || lowered.contains("normalize spaces")
            || lowered.contains("collapse whitespace")
            || lowered.contains("collapse spaces")
        {
            return Ok(Some(PipelineStep {
                step_type: StepType::Substitute,
                pattern: r"\s+".to_string(),
                replacement: Some(" ".to_string()),
                ..Default::default()
            }));
        }

        Ok(None)
    }

    /// Try to parse a masking/redacting operation.
    fn try_parse_masking(&self, input: &str) -> Result<Option<PipelineStep>> {
        let lowered = input.to_lowercase();
        let mask_prefixes = [
            "mask all ",
            "mask ",
            "redact all ",
            "redact ",
            "hide all ",
            "hide ",
            "anonymize ",
            "censor ",
        ];

        for prefix in mask_prefixes {
            if lowered.starts_with(prefix) {
                let target = input[prefix.len()..].trim();
                let replacement = self.get_mask_replacement(target);
                return Ok(Some(self.create_substitute_step(target, &replacement)?));
            }
        }

        Ok(None)
    }

    /// Get an appropriate mask replacement for a target.
    fn get_mask_replacement(&self, target: &str) -> String {
        match target.to_lowercase().as_str() {
            "email" | "emails" => "[EMAIL]".to_string(),
            "phone" | "phones" | "phone number" | "phone numbers" => "[PHONE]".to_string(),
            "ip" | "ips" | "ip address" | "ip addresses" => "[IP]".to_string(),
            "ssn" | "ssns" | "social security" | "social security number" => "[SSN]".to_string(),
            "credit card" | "credit cards" | "card number" | "card numbers" => "[CARD]".to_string(),
            "url" | "urls" | "link" | "links" => "[URL]".to_string(),
            "date" | "dates" => "[DATE]".to_string(),
            "time" | "times" => "[TIME]".to_string(),
            "number" | "numbers" => "[NUM]".to_string(),
            "name" | "names" => "[NAME]".to_string(),
            "password" | "passwords" => "[REDACTED]".to_string(),
            _ => "[MASKED]".to_string(),
        }
    }

    /// Create a substitution step from a target and replacement.
    fn create_substitute_step(&self, target: &str, replacement: &str) -> Result<PipelineStep> {
        let pattern = self.resolve_target_pattern(target)?;

        Ok(PipelineStep {
            step_type: StepType::Substitute,
            pattern,
            replacement: Some(replacement.to_string()),
            ..Default::default()
        })
    }

    /// Create a filter step from a target and action.
    fn create_filter_step(&self, target: &str, action: FilterAction) -> Result<PipelineStep> {
        let pattern = self.resolve_target_pattern(target)?;

        Ok(PipelineStep {
            step_type: StepType::Filter,
            pattern,
            action: Some(action),
            ..Default::default()
        })
    }

    /// Create an extraction step from a target.
    fn create_extract_step(&self, target: &str) -> Result<PipelineStep> {
        let pattern = self.resolve_target_pattern(target)?;

        Ok(PipelineStep {
            step_type: StepType::Extract,
            pattern,
            ..Default::default()
        })
    }

    /// Resolve a target description to a regex pattern.
    fn resolve_target_pattern(&self, target: &str) -> Result<String> {
        let target = target.trim().to_lowercase();

        // Check if it's a known pattern name
        if let Some(pattern) = self.patterns.get(&target) {
            return Ok(pattern.to_string());
        }

        // Check for "all X" format
        if let Some(rest) = target.strip_prefix("all ") {
            if let Some(pattern) = self.patterns.get(rest.trim()) {
                return Ok(pattern.to_string());
            }
        }

        // Check for quoted literal
        if (target.starts_with('"') && target.ends_with('"'))
            || (target.starts_with('\'') && target.ends_with('\''))
        {
            let literal = &target[1..target.len() - 1];
            return Ok(regex::escape(literal));
        }

        // Check for regex pattern (starts with / or contains special chars)
        if target.starts_with('/') && target.ends_with('/') {
            return Ok(target[1..target.len() - 1].to_string());
        }

        // Check for special syntax indicators
        if target.contains(r"\d")
            || target.contains(r"\w")
            || target.contains(r"\s")
            || target.contains('[')
            || target.contains('(')
            || target.contains('+')
            || target.contains('*')
            || target.contains('?')
        {
            return Ok(target);
        }

        // Try as a literal with word boundaries
        Ok(format!(r"\b{}\b", regex::escape(&target)))
    }

    /// Get a list of available pattern names for help.
    pub fn available_patterns(&self) -> Vec<&str> {
        let mut names: Vec<_> = self.patterns.names().collect();
        names.sort();
        names.dedup();
        names
    }

    /// Suggest corrections for a potentially misspelled pattern.
    pub fn suggest_pattern(&self, input: &str) -> Option<String> {
        let input_lower = input.to_lowercase();

        // Find closest match using simple edit distance
        let mut best_match: Option<(&str, usize)> = None;

        for name in self.patterns.names() {
            let distance = levenshtein(&input_lower, name);
            if distance <= 2 {
                if let Some((_, best_dist)) = best_match {
                    if distance < best_dist {
                        best_match = Some((name, distance));
                    }
                } else {
                    best_match = Some((name, distance));
                }
            }
        }

        best_match.map(|(name, _)| name.to_string())
    }

    /// Parse with suggestions for unrecognized patterns.
    pub fn parse_with_suggestions(&self, input: &str) -> (Result<PipelineConfig>, Vec<String>) {
        let result = self.parse(input);
        let mut suggestions = Vec::new();

        if result.is_err() {
            // Extract potential pattern names from input
            let words: Vec<&str> = input.split_whitespace().collect();
            for word in words {
                if let Some(suggestion) = self.suggest_pattern(word) {
                    suggestions.push(format!("Did you mean '{}'?", suggestion));
                }
            }
        }

        (result, suggestions)
    }
}

/// Simple Levenshtein distance for typo suggestions.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    let m = a.len();
    let n = b.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut matrix = vec![vec![0; n + 1]; m + 1];

    for i in 0..=m {
        matrix[i][0] = i;
    }
    for j in 0..=n {
        matrix[0][j] = j;
    }

    for i in 1..=m {
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[m][n]
}

/// Builder for constructing pipelines interactively.
#[derive(Debug, Default)]
pub struct PipelineBuilder {
    steps: Vec<PipelineStep>,
    parser: NaturalLanguageParser,
}

impl PipelineBuilder {
    /// Create a new pipeline builder.
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            parser: NaturalLanguageParser::new(),
        }
    }

    /// Add a step from natural language.
    pub fn add(&mut self, description: &str) -> Result<&mut Self> {
        let config = self.parser.parse(description)?;
        self.steps.extend(config.step);
        Ok(self)
    }

    /// Add a raw step.
    pub fn add_step(&mut self, step: PipelineStep) -> &mut Self {
        self.steps.push(step);
        self
    }

    /// Build the final pipeline configuration.
    pub fn build(self) -> PipelineConfig {
        PipelineConfig {
            step: self.steps,
            ..Default::default()
        }
    }

    /// Get the current number of steps.
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }

    /// Clear all steps.
    pub fn clear(&mut self) -> &mut Self {
        self.steps.clear();
        self
    }
}

/// Command parser for interactive mode.
#[derive(Debug)]
pub struct InteractiveCommand {
    /// The command type
    pub command: CommandType,
    /// Command arguments
    pub args: Vec<String>,
}

/// Types of interactive commands.
#[derive(Debug, Clone, PartialEq)]
pub enum CommandType {
    /// Add a step from natural language
    Add,
    /// List current steps
    List,
    /// Remove a step by index
    Remove,
    /// Clear all steps
    Clear,
    /// Test the pipeline
    Test,
    /// Export to TOML
    Export,
    /// Show help
    Help,
    /// Run the pipeline
    Run,
    /// Undo last action
    Undo,
    /// Show available patterns
    Patterns,
    /// Quit interactive mode
    Quit,
}

impl InteractiveCommand {
    /// Parse an interactive command.
    pub fn parse(input: &str) -> Option<Self> {
        let input = input.trim();

        if input.is_empty() {
            return None;
        }

        // Check for command prefix
        if let Some(rest) = input.strip_prefix(':') {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.is_empty() {
                return None;
            }

            let command = match parts[0].to_lowercase().as_str() {
                "add" | "a" => CommandType::Add,
                "list" | "l" | "ls" => CommandType::List,
                "remove" | "rm" | "delete" | "del" => CommandType::Remove,
                "clear" | "clr" => CommandType::Clear,
                "test" | "t" => CommandType::Test,
                "export" | "save" => CommandType::Export,
                "help" | "h" | "?" => CommandType::Help,
                "run" | "r" | "go" => CommandType::Run,
                "undo" | "u" => CommandType::Undo,
                "patterns" | "p" => CommandType::Patterns,
                "quit" | "q" | "exit" => CommandType::Quit,
                _ => return None,
            };

            let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

            Some(InteractiveCommand { command, args })
        } else {
            // Treat as add command
            Some(InteractiveCommand {
                command: CommandType::Add,
                args: vec![input.to_string()],
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_replacement() {
        let parser = NaturalLanguageParser::new();

        let config = parser.parse("replace numbers with NUM").unwrap();
        assert_eq!(config.step.len(), 1);
        assert_eq!(config.step[0].step_type, StepType::Substitute);
        assert_eq!(config.step[0].replacement, Some("NUM".to_string()));
    }

    #[test]
    fn test_parse_removal() {
        let parser = NaturalLanguageParser::new();

        let config = parser.parse("remove blank lines").unwrap();
        assert_eq!(config.step.len(), 1);
        assert_eq!(config.step[0].step_type, StepType::Substitute);
        assert_eq!(config.step[0].replacement, Some("".to_string()));
    }

    #[test]
    fn test_parse_filter_keep() {
        let parser = NaturalLanguageParser::new();

        let config = parser.parse("keep only lines with errors").unwrap();
        assert_eq!(config.step.len(), 1);
        assert_eq!(config.step[0].step_type, StepType::Filter);
        assert_eq!(config.step[0].action, Some(FilterAction::KeepLine));
    }

    #[test]
    fn test_parse_filter_drop() {
        let parser = NaturalLanguageParser::new();

        let config = parser.parse("drop lines containing debug").unwrap();
        assert_eq!(config.step.len(), 1);
        assert_eq!(config.step[0].step_type, StepType::Filter);
        assert_eq!(config.step[0].action, Some(FilterAction::DropLine));
    }

    #[test]
    fn test_parse_extraction() {
        let parser = NaturalLanguageParser::new();

        let config = parser.parse("extract all emails").unwrap();
        assert_eq!(config.step.len(), 1);
        assert_eq!(config.step[0].step_type, StepType::Extract);
    }

    #[test]
    fn test_parse_transformation() {
        let parser = NaturalLanguageParser::new();

        let config = parser.parse("convert to uppercase").unwrap();
        assert_eq!(config.step.len(), 1);
        assert_eq!(config.step[0].step_type, StepType::Transform);
        assert_eq!(config.step[0].transform, Some(TransformAction::Uppercase));
    }

    #[test]
    fn test_parse_masking() {
        let parser = NaturalLanguageParser::new();

        let config = parser.parse("mask all emails").unwrap();
        assert_eq!(config.step.len(), 1);
        assert_eq!(config.step[0].step_type, StepType::Substitute);
        assert_eq!(config.step[0].replacement, Some("[EMAIL]".to_string()));
    }

    #[test]
    fn test_parse_multiple_operations() {
        let parser = NaturalLanguageParser::new();

        let config = parser.parse("remove blank lines and replace numbers with NUM").unwrap();
        assert_eq!(config.step.len(), 2);
    }

    #[test]
    fn test_builtin_patterns() {
        let patterns = BuiltinPatterns::new();

        assert!(patterns.contains("email"));
        assert!(patterns.contains("phone"));
        assert!(patterns.contains("ip address"));
        assert!(patterns.contains("blank line"));

        assert!(patterns.get("email").is_some());
    }

    #[test]
    fn test_pipeline_builder() {
        let mut builder = PipelineBuilder::new();

        builder.add("remove blank lines").unwrap();
        builder.add("replace emails with [EMAIL]").unwrap();

        assert_eq!(builder.step_count(), 2);

        let config = builder.build();
        assert_eq!(config.step.len(), 2);
    }

    #[test]
    fn test_interactive_command_parse() {
        let cmd = InteractiveCommand::parse(":add remove numbers").unwrap();
        assert_eq!(cmd.command, CommandType::Add);

        let cmd = InteractiveCommand::parse(":list").unwrap();
        assert_eq!(cmd.command, CommandType::List);

        let cmd = InteractiveCommand::parse(":quit").unwrap();
        assert_eq!(cmd.command, CommandType::Quit);

        let cmd = InteractiveCommand::parse("remove blank lines").unwrap();
        assert_eq!(cmd.command, CommandType::Add);
    }

    #[test]
    fn test_suggest_pattern() {
        let parser = NaturalLanguageParser::new();

        let suggestion = parser.suggest_pattern("emal");
        assert_eq!(suggestion, Some("email".to_string()));

        let suggestion = parser.suggest_pattern("phne");
        assert_eq!(suggestion, Some("phone".to_string()));
    }

    #[test]
    fn test_quoted_literal() {
        let parser = NaturalLanguageParser::new();

        let config = parser.parse(r#"replace "hello" with "world""#).unwrap();
        assert_eq!(config.step.len(), 1);
        assert_eq!(config.step[0].pattern, "hello"); // escaped literal
    }
}
