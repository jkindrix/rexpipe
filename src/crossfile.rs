//! Cross-file semantic relationship processing.
//!
//! This module enables pipelines that understand relationships between files,
//! allowing for coordinated transformations across file boundaries.
//!
//! ## Features
//!
//! - **Trigger-based processing**: When pattern matches in file A, check file B
//! - **Atomic cross-file operations**: Apply changes consistently across related files
//! - **Dependency tracking**: Ensure related files are processed together
//! - **Consistency validation**: Verify patterns exist across file sets
//!
//! ## Example
//!
//! ```toml
//! [[cross_file_rule]]
//! name = "api-version-sync"
//! trigger_pattern = "api/v1/"
//! trigger_files = "**/*.ts"
//! related_files = "**/*.test.ts"
//! ensure_pattern = "api/v1/"
//! action = "warn"  # or "fail", "fix"
//! ```

#[cfg(feature = "cli")]
use globset::{Glob, GlobMatcher};
use serde::{Deserialize, Serialize};
#[cfg(feature = "cli")]
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur during cross-file operations.
#[derive(Error, Debug)]
pub enum CrossFileError {
    #[error("Pattern mismatch: {trigger_file} has '{pattern}' but {related_file} does not")]
    PatternMismatch {
        trigger_file: PathBuf,
        related_file: PathBuf,
        pattern: String,
    },

    #[error("Missing related file: {trigger_file} expects {related_pattern}")]
    MissingRelatedFile {
        trigger_file: PathBuf,
        related_pattern: String,
    },

    #[error("Circular dependency detected: {cycle}")]
    CircularDependency { cycle: String },

    #[error("Invalid glob pattern: {0}")]
    InvalidPattern(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),
}

pub type Result<T> = std::result::Result<T, CrossFileError>;

/// Compile a glob pattern into a matcher.
/// This uses globset which properly handles `**` for recursive directory matching.
#[cfg(feature = "cli")]
fn compile_glob(pattern: &str) -> Result<GlobMatcher> {
    Glob::new(pattern)
        .map(|g| g.compile_matcher())
        .map_err(|e| CrossFileError::InvalidPattern(e.to_string()))
}

/// Action to take when a cross-file rule is violated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ViolationAction {
    /// Log a warning but continue processing
    #[default]
    Warn,
    /// Fail the entire pipeline
    Fail,
    /// Automatically fix by applying the same transformation
    Fix,
    /// Skip the file but continue with others
    Skip,
}

impl std::str::FromStr for ViolationAction {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "warn" | "warning" => Ok(ViolationAction::Warn),
            "fail" | "error" => Ok(ViolationAction::Fail),
            "fix" | "auto" | "auto-fix" => Ok(ViolationAction::Fix),
            "skip" | "ignore" => Ok(ViolationAction::Skip),
            _ => Err(format!(
                "Invalid action '{}'. Valid: warn, fail, fix, skip",
                s
            )),
        }
    }
}

/// A rule defining cross-file relationships.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CrossFileRule {
    /// Rule name for identification (optional, auto-generated if not provided)
    #[serde(default)]
    pub name: String,

    /// Pattern that triggers this rule when matched
    pub trigger_pattern: String,

    /// Glob pattern for files where trigger pattern is searched
    #[serde(default)]
    pub trigger_files: String,

    /// Glob pattern for related files to check
    pub related_files: String,

    /// Pattern that must exist in related files (defaults to trigger_pattern)
    #[serde(default)]
    pub ensure_pattern: Option<String>,

    /// Action when violation is detected
    #[serde(default)]
    pub action: ViolationAction,

    /// Whether to process files atomically (all or none)
    #[serde(default)]
    pub atomic: bool,

    /// Description of this rule
    #[serde(default)]
    pub description: Option<String>,

    /// Mapping function to derive related file path from trigger file
    /// Supports: {dir}, {name}, {ext}, {stem}
    #[serde(default)]
    pub related_path_template: Option<String>,

    /// Whether this rule is enabled (defaults to true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl CrossFileRule {
    /// Create a new cross-file rule.
    pub fn new(
        name: impl Into<String>,
        trigger_pattern: impl Into<String>,
        related_files: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            trigger_pattern: trigger_pattern.into(),
            trigger_files: "**/*".to_string(),
            related_files: related_files.into(),
            ensure_pattern: None,
            action: ViolationAction::default(),
            atomic: false,
            description: None,
            related_path_template: None,
            enabled: true,
        }
    }

    /// Enable or disable this rule.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set the action for violations.
    pub fn with_action(mut self, action: ViolationAction) -> Self {
        self.action = action;
        self
    }

    /// Set atomic processing.
    pub fn atomic(mut self, atomic: bool) -> Self {
        self.atomic = atomic;
        self
    }

    /// Get the pattern to ensure in related files.
    pub fn ensure_pattern(&self) -> &str {
        self.ensure_pattern
            .as_deref()
            .unwrap_or(&self.trigger_pattern)
    }

    /// Derive related file path from trigger file using template.
    pub fn derive_related_path(&self, trigger_path: &Path) -> Option<PathBuf> {
        let template = self.related_path_template.as_ref()?;

        let dir = trigger_path.parent()?.to_string_lossy();
        let name = trigger_path.file_name()?.to_string_lossy();
        let stem = trigger_path.file_stem()?.to_string_lossy();
        let ext = trigger_path
            .extension()
            .map(|e| e.to_string_lossy())
            .unwrap_or_default();

        let result = template
            .replace("{dir}", &dir)
            .replace("{name}", &name)
            .replace("{stem}", &stem)
            .replace("{ext}", &ext);

        Some(PathBuf::from(result))
    }
}

/// Result of a cross-file check.
#[derive(Debug, Clone)]
pub struct CrossFileCheckResult {
    /// Rule that was checked
    pub rule_name: String,
    /// File that triggered the check
    pub trigger_file: PathBuf,
    /// Pattern that was found
    pub trigger_pattern: String,
    /// Related files that were checked
    pub related_files: Vec<PathBuf>,
    /// Violations found
    pub violations: Vec<CrossFileViolation>,
    /// Whether the check passed
    pub passed: bool,
}

/// A single cross-file violation.
#[derive(Debug, Clone)]
pub struct CrossFileViolation {
    /// File where violation occurred
    pub file: PathBuf,
    /// Expected pattern
    pub expected_pattern: String,
    /// Line numbers where pattern was expected but not found
    pub missing_at: Vec<u64>,
    /// Description of the violation
    pub description: String,
}

/// Manager for cross-file relationship processing.
///
/// Only available with the `cli` feature because it reads file contents
/// from disk and uses globset for pattern matching.
#[cfg(feature = "cli")]
pub struct CrossFileManager {
    rules: Vec<CrossFileRule>,
    file_contents: HashMap<PathBuf, String>,
    trigger_matches: HashMap<PathBuf, Vec<TriggerMatch>>,
}

/// A match of a trigger pattern in a file.
#[derive(Debug, Clone)]
pub struct TriggerMatch {
    /// Rule that matched
    pub rule_index: usize,
    /// Line number of match
    pub line_number: u64,
    /// The matched text
    pub matched_text: String,
}

#[cfg(feature = "cli")]
impl CrossFileManager {
    /// Create a new cross-file manager.
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            file_contents: HashMap::new(),
            trigger_matches: HashMap::new(),
        }
    }

    /// Add a rule.
    pub fn add_rule(&mut self, rule: CrossFileRule) {
        self.rules.push(rule);
    }

    /// Add multiple rules.
    pub fn add_rules(&mut self, rules: impl IntoIterator<Item = CrossFileRule>) {
        self.rules.extend(rules);
    }

    /// Load file content for analysis.
    pub fn load_file(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref().to_path_buf();
        let content = std::fs::read_to_string(&path)?;
        self.file_contents.insert(path, content);
        Ok(())
    }

    /// Scan files for trigger patterns.
    pub fn scan_triggers(&mut self) -> Result<()> {
        self.trigger_matches.clear();

        for (file_path, content) in &self.file_contents {
            for (rule_index, rule) in self.rules.iter().enumerate() {
                // Skip disabled rules
                if !rule.enabled {
                    continue;
                }

                // Check if file matches trigger pattern
                let trigger_glob = compile_glob(&rule.trigger_files)?;

                if !trigger_glob.is_match(file_path) {
                    continue;
                }

                // Search for trigger pattern
                let regex = regex::Regex::new(&rule.trigger_pattern)?;

                for (line_num, line) in content.lines().enumerate() {
                    if let Some(m) = regex.find(line) {
                        let trigger_match = TriggerMatch {
                            rule_index,
                            line_number: (line_num + 1) as u64,
                            matched_text: m.as_str().to_string(),
                        };

                        self.trigger_matches
                            .entry(file_path.clone())
                            .or_default()
                            .push(trigger_match);
                    }
                }
            }
        }

        Ok(())
    }

    /// Check all cross-file rules.
    pub fn check_all(&self) -> Result<Vec<CrossFileCheckResult>> {
        let mut results = Vec::new();

        for (trigger_file, matches) in &self.trigger_matches {
            for trigger_match in matches {
                let rule = &self.rules[trigger_match.rule_index];
                let result = self.check_rule(trigger_file, trigger_match, rule)?;
                results.push(result);
            }
        }

        Ok(results)
    }

    /// Check a single rule for a trigger match.
    fn check_rule(
        &self,
        trigger_file: &Path,
        trigger_match: &TriggerMatch,
        rule: &CrossFileRule,
    ) -> Result<CrossFileCheckResult> {
        let related_glob = compile_glob(&rule.related_files)?;

        let ensure_pattern = rule.ensure_pattern();
        let ensure_regex = regex::Regex::new(ensure_pattern)?;

        let mut related_files = Vec::new();
        let mut violations = Vec::new();

        // Find related files
        for (file_path, content) in &self.file_contents {
            if file_path == trigger_file {
                continue;
            }

            // Check if file matches related pattern
            let matches_glob = related_glob.is_match(file_path);
            let matches_template = rule
                .derive_related_path(trigger_file)
                .map(|p| &p == file_path)
                .unwrap_or(false);

            if !matches_glob && !matches_template {
                continue;
            }

            related_files.push(file_path.clone());

            // Check if ensure pattern exists
            let pattern_found = ensure_regex.is_match(content);

            if !pattern_found {
                violations.push(CrossFileViolation {
                    file: file_path.clone(),
                    expected_pattern: ensure_pattern.to_string(),
                    missing_at: vec![],
                    description: format!(
                        "Pattern '{}' found in {} but not in {}",
                        trigger_match.matched_text,
                        trigger_file.display(),
                        file_path.display()
                    ),
                });
            }
        }

        Ok(CrossFileCheckResult {
            rule_name: rule.name.clone(),
            trigger_file: trigger_file.to_path_buf(),
            trigger_pattern: trigger_match.matched_text.clone(),
            related_files,
            passed: violations.is_empty(),
            violations,
        })
    }

    /// Get files that need to be processed together (for atomic operations).
    pub fn get_file_groups(&self) -> Vec<FileGroup> {
        let mut groups = Vec::new();
        let mut processed: HashSet<PathBuf> = HashSet::new();

        for (trigger_file, matches) in &self.trigger_matches {
            if processed.contains(trigger_file) {
                continue;
            }

            let mut group = FileGroup {
                primary: trigger_file.clone(),
                related: Vec::new(),
                rules: Vec::new(),
            };

            for trigger_match in matches {
                let rule = &self.rules[trigger_match.rule_index];

                if rule.atomic {
                    group.rules.push(rule.name.clone());

                    // Find related files for this rule
                    let related_glob = match compile_glob(&rule.related_files) {
                        Ok(p) => p,
                        Err(_) => continue,
                    };

                    for file_path in self.file_contents.keys() {
                        if file_path != trigger_file
                            && related_glob.is_match(file_path)
                            && !group.related.contains(file_path)
                        {
                            group.related.push(file_path.clone());
                        }
                    }
                }
            }

            if !group.related.is_empty() {
                processed.insert(trigger_file.clone());
                for related in &group.related {
                    processed.insert(related.clone());
                }
                groups.push(group);
            }
        }

        groups
    }

    /// Apply auto-fixes for cross-file violations.
    ///
    /// For each violation, this appends the trigger pattern's matched text
    /// to the related file. Returns the number of files modified.
    ///
    /// # Arguments
    /// * `results` - The check results containing violations to fix
    /// * `dry_run` - If true, report what would be done without modifying files
    ///
    /// # Returns
    /// A tuple of (files_modified, fixes_applied)
    pub fn apply_fixes(
        &self,
        results: &[CrossFileCheckResult],
        dry_run: bool,
    ) -> Result<(usize, usize)> {
        let mut files_modified = 0;
        let mut fixes_applied = 0;

        for result in results {
            if result.passed {
                continue;
            }

            for violation in &result.violations {
                // The fix is to append the matched text from trigger to related file
                let fix_text = format!(
                    "\n// Auto-fix: Added to match pattern from {}\n{}\n",
                    result.trigger_file.display(),
                    result.trigger_pattern
                );

                if dry_run {
                    eprintln!(
                        "Would append to {}: {}",
                        violation.file.display(),
                        result.trigger_pattern
                    );
                } else {
                    use std::fs::OpenOptions;
                    use std::io::Write;

                    let mut file = OpenOptions::new().append(true).open(&violation.file)?;

                    writeln!(file, "{}", fix_text)?;
                    files_modified += 1;
                }
                fixes_applied += 1;
            }
        }

        Ok((files_modified, fixes_applied))
    }
}

#[cfg(feature = "cli")]
impl Default for CrossFileManager {
    fn default() -> Self {
        Self::new()
    }
}

/// A group of files that should be processed together.
#[derive(Debug, Clone)]
pub struct FileGroup {
    /// Primary file that triggered the group
    pub primary: PathBuf,
    /// Related files in the group
    pub related: Vec<PathBuf>,
    /// Rules that apply to this group
    pub rules: Vec<String>,
}

/// Configuration for cross-file processing in pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CrossFileConfig {
    /// Enable cross-file processing
    #[serde(default)]
    pub enabled: bool,

    /// Cross-file rules
    #[serde(default)]
    pub rules: Vec<CrossFileRule>,

    /// Default action for violations
    #[serde(default)]
    pub default_action: ViolationAction,

    /// Whether to process file groups atomically
    #[serde(default)]
    pub atomic_by_default: bool,
}

impl CrossFileConfig {
    /// Create a new configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable cross-file processing.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Add a rule.
    pub fn with_rule(mut self, rule: CrossFileRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// Load cross-file rules from a TOML file.
    ///
    /// The file format is:
    /// ```toml
    /// [[rule]]
    /// name = "test-coverage"
    /// trigger_pattern = "pub fn (\\w+)"
    /// trigger_files = "src/*.rs"
    /// related_files = "tests/*_test.rs"
    /// action = "warn"
    /// ```
    #[cfg(feature = "cli")]
    pub fn load_rules_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref()).map_err(CrossFileError::Io)?;

        #[derive(Deserialize)]
        struct RulesFile {
            #[serde(default)]
            rule: Vec<CrossFileRule>,
        }

        let rules_file: RulesFile = toml::from_str(&content)
            .map_err(|e| CrossFileError::InvalidPattern(format!("TOML parse error: {}", e)))?;

        Ok(Self {
            enabled: true,
            rules: rules_file.rule,
            default_action: ViolationAction::default(),
            atomic_by_default: false,
        })
    }
}

/// Format check results as a human-readable report.
#[cfg(feature = "cli")]
pub fn format_check_report(results: &[CrossFileCheckResult]) -> String {
    let mut report = String::new();

    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.len() - passed;

    report.push_str("╔══════════════════════════════════════════════════════════════════╗\n");
    report.push_str("║              CROSS-FILE CONSISTENCY CHECK                        ║\n");
    report.push_str("╚══════════════════════════════════════════════════════════════════╝\n\n");

    // Calculate statistics
    let total_related_files: usize = results.iter().map(|r| r.related_files.len()).sum();
    let total_violations: usize = results.iter().map(|r| r.violations.len()).sum();
    let unique_rules: HashSet<&str> = results.iter().map(|r| r.rule_name.as_str()).collect();

    report.push_str(&format!(
        "Summary: {} checks across {} unique rules\n",
        results.len(),
        unique_rules.len()
    ));
    report.push_str(&format!(
        "Results: {} passed, {} failed ({} violations)\n",
        passed, failed, total_violations
    ));
    report.push_str(&format!(
        "Coverage: {} related files checked\n\n",
        total_related_files
    ));

    // Group results by rule for better organization
    for result in results {
        let status = if result.passed { "✓" } else { "✗" };
        report.push_str(&format!(
            "{} Rule: {} (triggered in {})\n",
            status,
            result.rule_name,
            result.trigger_file.display()
        ));
        report.push_str(&format!(
            "  Related files checked: {}\n",
            result.related_files.len()
        ));

        if !result.passed {
            for violation in &result.violations {
                report.push_str(&format!(
                    "  └─ VIOLATION: {}\n     File: {}\n     Expected: {}\n",
                    violation.description,
                    violation.file.display(),
                    violation.expected_pattern
                ));
            }
        }

        report.push('\n');
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cross_file_rule() {
        let rule = CrossFileRule::new("api-sync", "api/v1/", "**/*.test.ts")
            .with_action(ViolationAction::Fail)
            .atomic(true);

        assert_eq!(rule.name, "api-sync");
        assert_eq!(rule.action, ViolationAction::Fail);
        assert!(rule.atomic);
    }

    #[test]
    fn test_derive_related_path() {
        let rule = CrossFileRule {
            name: "test".to_string(),
            trigger_pattern: ".*".to_string(),
            trigger_files: "**/*.ts".to_string(),
            related_files: "**/*.test.ts".to_string(),
            ensure_pattern: None,
            action: ViolationAction::Warn,
            atomic: false,
            description: None,
            related_path_template: Some("{dir}/{stem}.test.{ext}".to_string()),
            enabled: true,
        };

        let trigger = Path::new("src/components/Button.ts");
        let related = rule.derive_related_path(trigger).unwrap();
        assert_eq!(related, PathBuf::from("src/components/Button.test.ts"));
    }

    #[test]
    fn test_violation_action_parsing() {
        assert_eq!(
            "warn".parse::<ViolationAction>().unwrap(),
            ViolationAction::Warn
        );
        assert_eq!(
            "fail".parse::<ViolationAction>().unwrap(),
            ViolationAction::Fail
        );
        assert_eq!(
            "fix".parse::<ViolationAction>().unwrap(),
            ViolationAction::Fix
        );
        assert_eq!(
            "skip".parse::<ViolationAction>().unwrap(),
            ViolationAction::Skip
        );
    }

    #[test]
    fn test_cross_file_manager() {
        let mut manager = CrossFileManager::new();

        manager.add_rule(CrossFileRule::new("test-sync", r"api/v1/", "**/*.test.ts"));

        manager.file_contents.insert(
            PathBuf::from("src/api.ts"),
            "const endpoint = 'api/v1/users';\n".to_string(),
        );
        manager.file_contents.insert(
            PathBuf::from("src/api.test.ts"),
            "test('api/v1/users');\n".to_string(),
        );

        manager.scan_triggers().unwrap();

        let results = manager.check_all().unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn test_file_group() {
        let group = FileGroup {
            primary: PathBuf::from("src/main.ts"),
            related: vec![
                PathBuf::from("src/main.test.ts"),
                PathBuf::from("src/main.spec.ts"),
            ],
            rules: vec!["test-sync".to_string()],
        };

        assert_eq!(group.related.len(), 2);
    }
}
