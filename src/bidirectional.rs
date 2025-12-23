//! Bidirectional (reversible) pipeline transformations.
//!
//! This module enables pipelines that can be run in both directions:
//! forward (transform) and reverse (untransform). This is useful for:
//!
//! - **Development environments**: Transform production configs for local use
//! - **Data masking**: Mask data for testing, unmask for debugging
//! - **Configuration management**: Transform configs between environments
//!
//! ## Design
//!
//! Bidirectional pipelines maintain a mapping of transformations that can be
//! inverted. Some transformations are inherently reversible (like substitution),
//! while others require storing mappings (like masking).
//!
//! ## Example
//!
//! ```toml
//! [bidirectional]
//! enabled = true
//! mapping_file = ".rexpipe-mappings.json"
//!
//! [[step]]
//! type = "substitute"
//! pattern = "prod-db.company.com"
//! replacement = "localhost:5432"
//! # Automatically reversible: localhost:5432 → prod-db.company.com
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Unescape regex metacharacters to get the literal string.
///
/// This reverses the effect of `regex::escape()` by removing the backslashes
/// before regex metacharacters.
fn unescape_regex(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            // Check if next char is a regex metacharacter that was escaped
            if let Some(&next) = chars.peek() {
                if matches!(
                    next,
                    '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^'
                        | '$'
                ) {
                    // Skip the backslash, use the metacharacter literally.
                    // We already captured `next` from peek(), now advance the iterator
                    // past it. This avoids unwrap() since we know the value exists.
                    chars.next();
                    result.push(next);
                    continue;
                }
            }
        }
        result.push(c);
    }

    result
}

/// Errors that can occur during bidirectional operations.
#[derive(Error, Debug)]
pub enum BidirectionalError {
    #[error("Transformation is not reversible: {0}")]
    NotReversible(String),

    #[error("Mapping not found for value: {0}")]
    MappingNotFound(String),

    #[error("Failed to load mappings: {0}")]
    MappingLoadError(String),

    #[error("Failed to save mappings: {0}")]
    MappingSaveError(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, BidirectionalError>;

/// Direction of pipeline execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Apply transformations normally (default)
    #[default]
    Forward,
    /// Reverse all transformations
    Reverse,
}

impl std::str::FromStr for Direction {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "forward" | "fwd" | "f" => Ok(Direction::Forward),
            "reverse" | "rev" | "r" | "backward" | "back" | "b" => Ok(Direction::Reverse),
            _ => Err(format!(
                "Invalid direction '{}'. Valid options: forward, reverse",
                s
            )),
        }
    }
}

/// Configuration for bidirectional pipeline support.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct BidirectionalConfig {
    /// Enable bidirectional mode
    #[serde(default)]
    pub enabled: bool,

    /// Path to store mappings for non-trivially reversible transforms
    #[serde(default)]
    pub mapping_file: Option<PathBuf>,

    /// Direction to execute (forward or reverse)
    #[serde(default)]
    pub direction: Direction,

    /// Whether to auto-save mappings after each transformation
    #[serde(default = "default_auto_save")]
    pub auto_save: bool,

    /// Whether to fail if a reverse mapping is not found
    #[serde(default)]
    pub strict_reverse: bool,
}

fn default_auto_save() -> bool {
    true
}

impl BidirectionalConfig {
    /// Create a new bidirectional configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable bidirectional mode.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set the mapping file path.
    pub fn with_mapping_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.mapping_file = Some(path.into());
        self
    }

    /// Set the execution direction.
    pub fn with_direction(mut self, direction: Direction) -> Self {
        self.direction = direction;
        self
    }
}

/// Reversibility classification for transformation types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reversibility {
    /// Transformation is inherently reversible (e.g., substitute A→B can be B→A)
    Inherent,
    /// Transformation is reversible with stored mappings (e.g., masking)
    WithMapping,
    /// Transformation is not reversible (e.g., hash, one-way functions)
    NotReversible,
}

/// A single mapping entry for bidirectional transforms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingEntry {
    /// Original value (before transformation)
    pub original: String,
    /// Transformed value (after transformation)
    pub transformed: String,
    /// Step index that created this mapping
    pub step_index: usize,
    /// Pattern that matched
    pub pattern: String,
    /// Timestamp when mapping was created
    pub created_at: u64,
}

/// Storage for bidirectional mappings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MappingStore {
    /// Version of the mapping format
    pub version: String,
    /// Pipeline identifier
    pub pipeline_id: Option<String>,
    /// Forward mappings: original → transformed
    pub forward: HashMap<String, MappingEntry>,
    /// Reverse mappings: transformed → original
    pub reverse: HashMap<String, MappingEntry>,
    /// Step-specific mappings
    pub by_step: HashMap<usize, Vec<MappingEntry>>,
}

impl MappingStore {
    /// Create a new empty mapping store.
    pub fn new() -> Self {
        Self {
            version: "1.0.0".to_string(),
            pipeline_id: None,
            forward: HashMap::new(),
            reverse: HashMap::new(),
            by_step: HashMap::new(),
        }
    }

    /// Load mappings from a file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref()).map_err(|e| {
            BidirectionalError::MappingLoadError(format!("{}: {}", path.as_ref().display(), e))
        })?;

        serde_json::from_str(&content)
            .map_err(|e| BidirectionalError::MappingLoadError(format!("Invalid JSON: {}", e)))
    }

    /// Save mappings to a file.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;

        // Create parent directories if needed
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = File::create(path.as_ref())?;
        file.write_all(json.as_bytes())?;
        Ok(())
    }

    /// Add a mapping entry.
    pub fn add_mapping(&mut self, entry: MappingEntry) {
        let step_index = entry.step_index;

        // Add to forward map
        self.forward.insert(entry.original.clone(), entry.clone());

        // Add to reverse map
        self.reverse
            .insert(entry.transformed.clone(), entry.clone());

        // Add to step-specific map
        self.by_step.entry(step_index).or_default().push(entry);
    }

    /// Look up the transformed value for an original value.
    pub fn get_forward(&self, original: &str) -> Option<&str> {
        self.forward.get(original).map(|e| e.transformed.as_str())
    }

    /// Look up the original value for a transformed value.
    pub fn get_reverse(&self, transformed: &str) -> Option<&str> {
        self.reverse.get(transformed).map(|e| e.original.as_str())
    }

    /// Get all mappings for a specific step.
    pub fn get_step_mappings(&self, step_index: usize) -> Option<&Vec<MappingEntry>> {
        self.by_step.get(&step_index)
    }

    /// Clear all mappings.
    pub fn clear(&mut self) {
        self.forward.clear();
        self.reverse.clear();
        self.by_step.clear();
    }

    /// Merge another mapping store into this one.
    pub fn merge(&mut self, other: MappingStore) {
        for (_, entry) in other.forward {
            self.add_mapping(entry);
        }
    }

    /// Get the number of mappings.
    pub fn len(&self) -> usize {
        self.forward.len()
    }

    /// Check if the store is empty.
    pub fn is_empty(&self) -> bool {
        self.forward.is_empty()
    }

    /// Get statistics about the mapping store.
    pub fn stats(&self) -> MappingStats {
        let unique_steps: std::collections::HashSet<usize> =
            self.by_step.keys().copied().collect();

        MappingStats {
            total_mappings: self.forward.len(),
            steps_with_mappings: unique_steps.len(),
            unique_originals: self.forward.len(),
            unique_transformed: self.reverse.len(),
        }
    }
}

/// Statistics about bidirectional mappings.
#[derive(Debug, Clone)]
pub struct MappingStats {
    /// Total number of recorded mappings
    pub total_mappings: usize,
    /// Number of pipeline steps that recorded mappings
    pub steps_with_mappings: usize,
    /// Number of unique original values
    pub unique_originals: usize,
    /// Number of unique transformed values
    pub unique_transformed: usize,
}

impl std::fmt::Display for MappingStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Mappings: {} total ({} original → {} transformed, {} steps)",
            self.total_mappings,
            self.unique_originals,
            self.unique_transformed,
            self.steps_with_mappings
        )
    }
}

/// Trait for types that can be reversed.
pub trait Reversible {
    /// Get the reversibility classification.
    fn reversibility(&self) -> Reversibility;

    /// Create the reverse transformation.
    fn reverse(&self) -> Result<Box<dyn Reversible>>;
}

/// A reversible substitution transformation.
#[derive(Debug, Clone)]
pub struct ReversibleSubstitution {
    /// Pattern to match
    pub pattern: String,
    /// Replacement text
    pub replacement: String,
    /// Whether this is a fixed string (not regex)
    pub fixed_string: bool,
    /// Direction of this substitution
    pub direction: Direction,
}

impl ReversibleSubstitution {
    /// Create a new reversible substitution.
    pub fn new(pattern: impl Into<String>, replacement: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            replacement: replacement.into(),
            fixed_string: true,
            direction: Direction::Forward,
        }
    }

    /// Set whether this is a fixed string pattern.
    pub fn fixed_string(mut self, fixed: bool) -> Self {
        self.fixed_string = fixed;
        self
    }

    /// Get the reversed substitution.
    pub fn reversed(&self) -> Self {
        Self {
            pattern: self.replacement.clone(),
            replacement: self.pattern.clone(),
            fixed_string: self.fixed_string,
            direction: match self.direction {
                Direction::Forward => Direction::Reverse,
                Direction::Reverse => Direction::Forward,
            },
        }
    }
}

/// Manager for bidirectional pipeline execution.
pub struct BidirectionalManager {
    config: BidirectionalConfig,
    mappings: MappingStore,
    modified: bool,
}

impl BidirectionalManager {
    /// Create a new bidirectional manager.
    pub fn new(config: BidirectionalConfig) -> Result<Self> {
        let mappings = if let Some(ref path) = config.mapping_file {
            if path.exists() {
                MappingStore::load(path)?
            } else {
                MappingStore::new()
            }
        } else {
            MappingStore::new()
        };

        Ok(Self {
            config,
            mappings,
            modified: false,
        })
    }

    /// Check if bidirectional mode is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the current direction.
    pub fn direction(&self) -> Direction {
        self.config.direction
    }

    /// Record a mapping for later reversal.
    pub fn record_mapping(
        &mut self,
        original: impl Into<String>,
        transformed: impl Into<String>,
        step_index: usize,
        pattern: impl Into<String>,
    ) {
        if !self.config.enabled {
            return;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let entry = MappingEntry {
            original: original.into(),
            transformed: transformed.into(),
            step_index,
            pattern: pattern.into(),
            created_at: now,
        };

        self.mappings.add_mapping(entry);
        self.modified = true;
    }

    /// Look up a value based on the current direction.
    pub fn lookup(&self, value: &str) -> Option<&str> {
        match self.config.direction {
            Direction::Forward => self.mappings.get_forward(value),
            Direction::Reverse => self.mappings.get_reverse(value),
        }
    }

    /// Transform a value in the current direction.
    pub fn transform(&mut self, value: &str, _step_index: usize) -> Result<Option<String>> {
        if !self.config.enabled {
            return Ok(None);
        }

        match self.config.direction {
            Direction::Forward => {
                // Forward direction - check if we have a stored mapping
                if let Some(transformed) = self.mappings.get_forward(value) {
                    Ok(Some(transformed.to_string()))
                } else {
                    Ok(None)
                }
            }
            Direction::Reverse => {
                // Reverse direction - look up the original value
                if let Some(original) = self.mappings.get_reverse(value) {
                    Ok(Some(original.to_string()))
                } else if self.config.strict_reverse {
                    Err(BidirectionalError::MappingNotFound(value.to_string()))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Save mappings if modified.
    pub fn save_if_modified(&mut self) -> Result<()> {
        if !self.modified || !self.config.auto_save {
            return Ok(());
        }

        if let Some(ref path) = self.config.mapping_file {
            self.mappings.save(path)?;
            self.modified = false;
        }

        Ok(())
    }

    /// Force save mappings.
    pub fn save(&self) -> Result<()> {
        if let Some(ref path) = self.config.mapping_file {
            self.mappings.save(path)?;
        }
        Ok(())
    }

    /// Get the mapping store.
    pub fn mappings(&self) -> &MappingStore {
        &self.mappings
    }

    /// Get mutable access to the mapping store.
    pub fn mappings_mut(&mut self) -> &mut MappingStore {
        self.modified = true;
        &mut self.mappings
    }
}

/// Analyze a pipeline configuration for reversibility.
pub fn analyze_reversibility(
    steps: &[super::pipeline::PipelineStep],
) -> Vec<(usize, Reversibility)> {
    use super::pipeline::{StepType, TransformAction};

    steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let reversibility = match step.step_type {
                StepType::Substitute => {
                    // Substitutions are inherently reversible if both pattern and replacement are fixed strings
                    Reversibility::Inherent
                }
                StepType::Filter => {
                    // Filters are not reversible (dropped lines are lost)
                    Reversibility::NotReversible
                }
                StepType::Extract => {
                    // Extractions are not reversible (context is lost)
                    Reversibility::NotReversible
                }
                StepType::Validate => {
                    // Validation doesn't change data, so it's trivially reversible
                    Reversibility::Inherent
                }
                StepType::Transform => {
                    // Depends on the transform action
                    match &step.transform {
                        Some(TransformAction::Uppercase) | Some(TransformAction::Lowercase) => {
                            // Case changes lose original casing
                            Reversibility::WithMapping
                        }
                        Some(TransformAction::Base64Encode)
                        | Some(TransformAction::Base64Decode)
                        | Some(TransformAction::UrlEncode)
                        | Some(TransformAction::UrlDecode) => Reversibility::Inherent,
                        Some(TransformAction::Reverse) => Reversibility::Inherent,
                        Some(TransformAction::Trim)
                        | Some(TransformAction::RemoveWhitespace)
                        | Some(TransformAction::NormalizeWhitespace) => {
                            // Whitespace changes lose original formatting
                            Reversibility::NotReversible
                        }
                        #[cfg(feature = "fpe")]
                        Some(TransformAction::FpeEncrypt { .. })
                        | Some(TransformAction::FpeDecrypt { .. }) => Reversibility::Inherent,
                        Some(TransformAction::MaskDeterministic { .. }) => {
                            // Masking is one-way
                            Reversibility::NotReversible
                        }
                        _ => Reversibility::WithMapping,
                    }
                }
                StepType::Block => {
                    // Block operations may or may not be reversible depending on action
                    Reversibility::WithMapping
                }
            };

            (i, reversibility)
        })
        .collect()
}

/// Generate the reverse pipeline configuration.
pub fn generate_reverse_pipeline(
    config: &super::pipeline::PipelineConfig,
) -> Result<super::pipeline::PipelineConfig> {
    use super::pipeline::{StepType, TransformAction};

    let mut reversed = config.clone();

    // Reverse the step order
    reversed.step.reverse();

    // Reverse each step
    for step in &mut reversed.step {
        match step.step_type {
            StepType::Substitute => {
                // Swap pattern and replacement
                // The replacement becomes the new pattern, so we need to escape any
                // regex metacharacters in it to match the literal replacement text.
                // The old pattern may have escaped metacharacters, so we unescape
                // them when using it as a replacement string.
                if let Some(ref replacement) = step.replacement {
                    let old_pattern = step.pattern.clone();
                    // Escape regex metacharacters in the replacement to use as literal pattern
                    step.pattern = regex::escape(replacement);
                    // Unescape the old pattern so it becomes a literal replacement string
                    step.replacement = Some(unescape_regex(&old_pattern));
                }
            }
            StepType::Transform => {
                // Reverse the transform action
                step.transform = match &step.transform {
                    Some(TransformAction::Base64Encode) => Some(TransformAction::Base64Decode),
                    Some(TransformAction::Base64Decode) => Some(TransformAction::Base64Encode),
                    Some(TransformAction::UrlEncode) => Some(TransformAction::UrlDecode),
                    Some(TransformAction::UrlDecode) => Some(TransformAction::UrlEncode),
                    Some(TransformAction::Reverse) => Some(TransformAction::Reverse),
                    #[cfg(feature = "fpe")]
                    Some(TransformAction::FpeEncrypt {
                        key,
                        key_file,
                        tweak,
                        tweak_file,
                        radix,
                    }) => Some(TransformAction::FpeDecrypt {
                        key: key.clone(),
                        key_file: key_file.clone(),
                        tweak: tweak.clone(),
                        tweak_file: tweak_file.clone(),
                        radix: radix.clone(),
                    }),
                    #[cfg(feature = "fpe")]
                    Some(TransformAction::FpeDecrypt {
                        key,
                        key_file,
                        tweak,
                        tweak_file,
                        radix,
                    }) => Some(TransformAction::FpeEncrypt {
                        key: key.clone(),
                        key_file: key_file.clone(),
                        tweak: tweak.clone(),
                        tweak_file: tweak_file.clone(),
                        radix: radix.clone(),
                    }),
                    other => {
                        return Err(BidirectionalError::NotReversible(format!(
                            "Transform action {:?} is not reversible",
                            other
                        )));
                    }
                };
            }
            StepType::Filter | StepType::Extract => {
                return Err(BidirectionalError::NotReversible(format!(
                    "{:?} steps are not reversible",
                    step.step_type
                )));
            }
            StepType::Validate => {
                // Validation is a no-op for reversal
            }
            StepType::Block => {
                return Err(BidirectionalError::NotReversible(
                    "Block steps are not reversible".to_string(),
                ));
            }
        }
    }

    // Update name to indicate reversal
    reversed.name = reversed.name.map(|n| format!("{} (reversed)", n));

    Ok(reversed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direction_parsing() {
        assert_eq!("forward".parse::<Direction>().unwrap(), Direction::Forward);
        assert_eq!("fwd".parse::<Direction>().unwrap(), Direction::Forward);
        assert_eq!("reverse".parse::<Direction>().unwrap(), Direction::Reverse);
        assert_eq!("rev".parse::<Direction>().unwrap(), Direction::Reverse);
        assert_eq!("backward".parse::<Direction>().unwrap(), Direction::Reverse);
    }

    #[test]
    fn test_mapping_store() {
        let mut store = MappingStore::new();

        store.add_mapping(MappingEntry {
            original: "original_value".to_string(),
            transformed: "transformed_value".to_string(),
            step_index: 0,
            pattern: r"\w+".to_string(),
            created_at: 0,
        });

        assert_eq!(store.len(), 1);
        assert_eq!(
            store.get_forward("original_value"),
            Some("transformed_value")
        );
        assert_eq!(
            store.get_reverse("transformed_value"),
            Some("original_value")
        );
    }

    #[test]
    fn test_reversible_substitution() {
        let sub = ReversibleSubstitution::new("prod.example.com", "localhost");
        let reversed = sub.reversed();

        assert_eq!(reversed.pattern, "localhost");
        assert_eq!(reversed.replacement, "prod.example.com");
    }

    #[test]
    fn test_bidirectional_manager() {
        let config = BidirectionalConfig::new()
            .enabled(true)
            .with_direction(Direction::Forward);

        let mut manager = BidirectionalManager::new(config).unwrap();

        manager.record_mapping("secret123", "REDACTED", 0, r"\w+");

        assert_eq!(manager.mappings().len(), 1);
        assert_eq!(manager.lookup("secret123"), Some("REDACTED"));
    }

    #[test]
    fn test_bidirectional_reverse_lookup() {
        let config = BidirectionalConfig::new()
            .enabled(true)
            .with_direction(Direction::Reverse);

        let mut manager = BidirectionalManager::new(config).unwrap();
        manager.mappings_mut().add_mapping(MappingEntry {
            original: "secret123".to_string(),
            transformed: "REDACTED".to_string(),
            step_index: 0,
            pattern: r"\w+".to_string(),
            created_at: 0,
        });

        // In reverse mode, lookup goes from transformed → original
        assert_eq!(manager.lookup("REDACTED"), Some("secret123"));
    }
}
