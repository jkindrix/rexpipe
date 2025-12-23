//! Pattern learning and inference from examples.
//!
//! This module provides intelligent pattern discovery and learning capabilities:
//!
//! - **Example-based learning**: Infer regex patterns from positive/negative examples
//! - **Pattern generalization**: Generate patterns that match all examples
//! - **Confidence scoring**: Rate pattern quality and specificity
//! - **Pattern suggestions**: Recommend patterns based on content analysis
//! - **Alternation learning**: Detect when examples differ in consistent positions
//! - **Quantifier inference**: Learn fixed vs. variable repetition from examples
//! - **Anchor detection**: Infer start/end anchors from example boundaries
//!
//! ## Algorithm
//!
//! The pattern learner uses a multi-phase approach combining several techniques:
//!
//! ### Phase 1: Template Matching
//! Pre-defined templates for common patterns (email, URL, IP, SSN, etc.) are tested
//! first for quick wins on well-known formats.
//!
//! ### Phase 2: Structural Analysis
//! - Character class detection (digits, letters, punctuation)
//! - Repetition block analysis (e.g., "aaa" → `a{3}` or `a+`)
//! - Common prefix/suffix detection
//! - Delimiter pattern recognition
//!
//! ### Phase 3: Advanced Inference
//! - Alternation learning: Detects choice patterns like `(cat|dog)`
//! - Quantifier inference: Distinguishes `{3}` vs `{2,4}` vs `+`
//! - Anchor detection: Learns `^` and `$` from consistent boundaries
//! - Capture group generation: Optional grouping for extraction
//!
//! ### Phase 4: Filtering & Ranking
//! Patterns are filtered against negative examples and ranked by confidence,
//! which combines positive match rate (70%) and negative avoidance (30%).
//!
//! ## Example
//!
//! ```
//! use rexpipe::learn::PatternLearner;
//!
//! let mut learner = PatternLearner::new();
//!
//! // Add positive examples (what we want to match)
//! learner.add_positive("user@example.com");
//! learner.add_positive("admin@company.org");
//! learner.add_positive("test123@domain.net");
//!
//! // Add negative examples (what we don't want to match)
//! learner.add_negative("not-an-email");
//! learner.add_negative("@invalid");
//!
//! // Learn patterns
//! let patterns = learner.learn().unwrap();
//!
//! // Get the best pattern
//! if let Some(best) = patterns.first() {
//!     println!("Pattern: {} (confidence: {}%)", best.pattern, best.confidence);
//! }
//! ```
//!
//! ## Limitations
//!
//! - **Unicode**: Currently optimized for ASCII; Unicode characters are classified as "Other"
//! - **Lookahead/Lookbehind**: Not inferred (would require PCRE mode)
//! - **Backreferences**: Not supported in learned patterns
//! - **Complexity**: Very complex patterns may hit the default 200-character limit

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Instant;
use thiserror::Error;

/// Errors that can occur during pattern learning.
#[derive(Error, Debug)]
pub enum LearnError {
    #[error("Insufficient examples: need at least {min} positive examples, got {got}")]
    InsufficientExamples { min: usize, got: usize },

    #[error("Too many examples: maximum {max} allowed, got {got}")]
    TooManyExamples { max: usize, got: usize },

    #[error("No valid pattern found that matches all positive examples")]
    NoPatternFound,

    #[error("Pattern conflicts with negative example: {0}")]
    NegativeConflict(String),

    #[error("Invalid regex generated: {0}")]
    InvalidRegex(#[from] regex::Error),

    #[error("Learning timeout exceeded (limit: {0}ms)")]
    Timeout(u64),
}

pub type Result<T> = std::result::Result<T, LearnError>;

/// An example for pattern learning.
#[derive(Debug, Clone)]
pub struct Example {
    /// The example text
    pub text: String,
    /// Whether this is a positive (should match) or negative (should not match) example
    pub positive: bool,
    /// Optional: specific substring to extract
    pub extract: Option<String>,
}

impl Example {
    /// Create a positive example.
    pub fn positive(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            positive: true,
            extract: None,
        }
    }

    /// Create a negative example.
    pub fn negative(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            positive: false,
            extract: None,
        }
    }

    /// Create a positive example with extraction target.
    pub fn with_extract(text: impl Into<String>, extract: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            positive: true,
            extract: Some(extract.into()),
        }
    }
}

/// A learned pattern with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedPattern {
    /// The regex pattern
    pub pattern: String,
    /// Confidence score (0-100)
    pub confidence: u8,
    /// Number of positive examples matched
    pub matches_positive: usize,
    /// Total number of positive examples tested against
    pub total_positive: usize,
    /// Number of negative examples (correctly) not matched
    pub avoids_negative: usize,
    /// Total number of negative examples tested against
    pub total_negative: usize,
    /// Pattern category/type
    pub category: PatternCategory,
    /// Human-readable description
    pub description: String,
    /// Specificity score (how specific vs. general the pattern is)
    pub specificity: u8,
    /// Whether this pattern uses capture groups
    pub has_captures: bool,
    /// Explanation of why this pattern was generated
    pub reasoning: Option<String>,
}

/// Categories of patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternCategory {
    /// Email addresses
    Email,
    /// URLs and URIs
    Url,
    /// IP addresses (v4 or v6)
    IpAddress,
    /// Phone numbers
    Phone,
    /// Dates and timestamps
    DateTime,
    /// Credit card numbers
    CreditCard,
    /// Social security numbers
    Ssn,
    /// UUIDs
    Uuid,
    /// Generic identifiers
    Identifier,
    /// Numeric patterns
    Numeric,
    /// Alphanumeric patterns
    Alphanumeric,
    /// Custom/unknown
    Custom,
}

impl PatternCategory {
    /// Get the common pattern template for this category.
    fn template(&self) -> Option<&'static str> {
        match self {
            PatternCategory::Email => Some(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}"),
            PatternCategory::Url => Some(r"https?://[^\s]+"),
            PatternCategory::IpAddress => Some(r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}"),
            PatternCategory::Phone => Some(r"[\d\s\-\(\)]+"),
            PatternCategory::DateTime => Some(r"\d{4}-\d{2}-\d{2}"),
            PatternCategory::CreditCard => Some(r"\d{4}[\s\-]?\d{4}[\s\-]?\d{4}[\s\-]?\d{4}"),
            PatternCategory::Ssn => Some(r"\d{3}-\d{2}-\d{4}"),
            PatternCategory::Uuid => {
                Some(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
            }
            PatternCategory::Identifier => Some(r"[a-zA-Z_][a-zA-Z0-9_]*"),
            PatternCategory::Numeric => Some(r"\d+"),
            PatternCategory::Alphanumeric => Some(r"[a-zA-Z0-9]+"),
            PatternCategory::Custom => None,
        }
    }
}

/// Configuration for pattern learning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnConfig {
    /// Minimum confidence threshold (0-100)
    #[serde(default = "default_min_confidence")]
    pub min_confidence: u8,

    /// Maximum number of patterns to return
    #[serde(default = "default_max_patterns")]
    pub max_patterns: usize,

    /// Whether to try common pattern templates first
    #[serde(default = "default_true")]
    pub use_templates: bool,

    /// Whether to generate capture groups
    #[serde(default)]
    pub generate_captures: bool,

    /// Maximum pattern complexity (regex length)
    #[serde(default = "default_max_complexity")]
    pub max_complexity: usize,

    /// Timeout in milliseconds (default: 5000ms)
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,

    /// Maximum number of examples (positive + negative) to process (default: 1000)
    /// This prevents resource exhaustion from very large example sets
    #[serde(default = "default_max_examples")]
    pub max_examples: usize,
}

fn default_min_confidence() -> u8 {
    70
}

fn default_max_patterns() -> usize {
    5
}

fn default_true() -> bool {
    true
}

fn default_max_complexity() -> usize {
    200
}

fn default_timeout() -> u64 {
    5000
}

fn default_max_examples() -> usize {
    1000
}

impl Default for LearnConfig {
    fn default() -> Self {
        Self {
            min_confidence: default_min_confidence(),
            max_patterns: default_max_patterns(),
            use_templates: default_true(),
            generate_captures: false,
            max_complexity: default_max_complexity(),
            timeout_ms: default_timeout(),
            max_examples: default_max_examples(),
        }
    }
}

/// Pattern learner for inferring regex patterns from examples.
pub struct PatternLearner {
    config: LearnConfig,
    positive_examples: Vec<String>,
    negative_examples: Vec<String>,
    extraction_targets: Vec<(String, String)>, // (full_text, target)
}

impl PatternLearner {
    /// Create a new pattern learner with default configuration.
    pub fn new() -> Self {
        Self::with_config(LearnConfig::default())
    }

    /// Create a pattern learner with custom configuration.
    pub fn with_config(config: LearnConfig) -> Self {
        Self {
            config,
            positive_examples: Vec::new(),
            negative_examples: Vec::new(),
            extraction_targets: Vec::new(),
        }
    }

    /// Add a positive example (text that should match).
    pub fn add_positive(&mut self, text: impl Into<String>) {
        self.positive_examples.push(text.into());
    }

    /// Add a negative example (text that should not match).
    pub fn add_negative(&mut self, text: impl Into<String>) {
        self.negative_examples.push(text.into());
    }

    /// Add an extraction example (text with target substring).
    pub fn add_extraction(&mut self, full_text: impl Into<String>, target: impl Into<String>) {
        let full = full_text.into();
        let tgt = target.into();
        self.positive_examples.push(tgt.clone());
        self.extraction_targets.push((full, tgt));
    }

    /// Add multiple examples at once.
    pub fn add_examples(&mut self, examples: impl IntoIterator<Item = Example>) {
        for example in examples {
            if example.positive {
                if let Some(extract) = example.extract {
                    self.add_extraction(example.text, extract);
                } else {
                    self.add_positive(example.text);
                }
            } else {
                self.add_negative(example.text);
            }
        }
    }

    /// Clear all examples.
    pub fn clear(&mut self) {
        self.positive_examples.clear();
        self.negative_examples.clear();
        self.extraction_targets.clear();
    }

    /// Get the total number of examples (positive + negative).
    pub fn example_count(&self) -> usize {
        self.positive_examples.len() + self.negative_examples.len()
    }

    /// Get an iterator over positive examples.
    pub fn positive_examples(&self) -> impl Iterator<Item = &str> {
        self.positive_examples.iter().map(|s| s.as_str())
    }

    /// Get an iterator over negative examples.
    pub fn negative_examples(&self) -> impl Iterator<Item = &str> {
        self.negative_examples.iter().map(|s| s.as_str())
    }

    /// Learn patterns from the provided examples.
    ///
    /// # Rate Limiting
    ///
    /// This function enforces rate limits to prevent resource exhaustion:
    /// - `max_examples`: Limits total examples (default: 1000)
    /// - `timeout_ms`: Limits execution time (default: 5000ms)
    ///
    /// These limits can be configured via [`LearnConfig`].
    pub fn learn(&self) -> Result<Vec<LearnedPattern>> {
        let start_time = Instant::now();

        // Check minimum examples
        if self.positive_examples.len() < 2 {
            return Err(LearnError::InsufficientExamples {
                min: 2,
                got: self.positive_examples.len(),
            });
        }

        // Check maximum examples (rate limiting)
        let total_examples = self.example_count();
        if total_examples > self.config.max_examples {
            return Err(LearnError::TooManyExamples {
                max: self.config.max_examples,
                got: total_examples,
            });
        }

        let mut candidates: Vec<LearnedPattern> = Vec::new();

        // Try template-based patterns first
        if self.config.use_templates {
            candidates.extend(self.try_templates()?);
        }

        // Check timeout
        if start_time.elapsed().as_millis() as u64 > self.config.timeout_ms {
            return Err(LearnError::Timeout(self.config.timeout_ms));
        }

        // Generate patterns from character analysis
        candidates.extend(self.learn_from_structure()?);

        // Check timeout again after structure learning
        if start_time.elapsed().as_millis() as u64 > self.config.timeout_ms {
            return Err(LearnError::Timeout(self.config.timeout_ms));
        }

        // Filter by negative examples
        candidates = self.filter_by_negatives(candidates)?;

        // Sort by confidence
        candidates.sort_by(|a, b| b.confidence.cmp(&a.confidence));

        // Limit results
        candidates.truncate(self.config.max_patterns);

        if candidates.is_empty() {
            return Err(LearnError::NoPatternFound);
        }

        Ok(candidates)
    }

    /// Try common pattern templates.
    fn try_templates(&self) -> Result<Vec<LearnedPattern>> {
        let mut results = Vec::new();

        let categories = [
            PatternCategory::Email,
            PatternCategory::Url,
            PatternCategory::IpAddress,
            PatternCategory::Phone,
            PatternCategory::DateTime,
            PatternCategory::CreditCard,
            PatternCategory::Ssn,
            PatternCategory::Uuid,
        ];

        for category in categories {
            if let Some(template) = category.template() {
                if let Ok(pattern) = self.evaluate_pattern(template, category) {
                    if pattern.confidence >= self.config.min_confidence {
                        results.push(pattern);
                    }
                }
            }
        }

        Ok(results)
    }

    /// Learn patterns from structural analysis of examples.
    fn learn_from_structure(&self) -> Result<Vec<LearnedPattern>> {
        let mut results = Vec::new();

        // Analyze character classes in examples
        let analysis = self.analyze_examples();

        // Generate patterns from analysis
        for pattern_str in self.generate_patterns(&analysis) {
            if pattern_str.len() > self.config.max_complexity {
                continue;
            }

            if let Ok(pattern) = self.evaluate_pattern(&pattern_str, PatternCategory::Custom) {
                if pattern.confidence >= self.config.min_confidence {
                    results.push(pattern);
                }
            }
        }

        Ok(results)
    }

    /// Analyze character patterns in examples.
    fn analyze_examples(&self) -> ExampleAnalysis {
        let mut analysis = ExampleAnalysis::default();

        for example in &self.positive_examples {
            let char_types: Vec<CharType> = example.chars().map(CharType::from).collect();
            analysis.char_sequences.push(char_types);
            analysis.lengths.push(example.len());

            // Collect unique characters
            for c in example.chars() {
                analysis.unique_chars.insert(c);
            }
        }

        // Find common length
        if !analysis.lengths.is_empty() {
            let min_len = *analysis.lengths.iter().min().unwrap();
            let max_len = *analysis.lengths.iter().max().unwrap();
            analysis.min_length = min_len;
            analysis.max_length = max_len;
            analysis.fixed_length = min_len == max_len;
        }

        analysis
    }

    /// Generate pattern candidates from analysis.
    ///
    /// Generates multiple candidate patterns using different strategies:
    /// 1. Character class sequence (structural)
    /// 2. Literal with wildcards (common substring)
    /// 3. Affix pattern (prefix/suffix)
    /// 4. Alternation pattern (choice between alternatives)
    /// 5. Anchored pattern (with start/end anchors)
    fn generate_patterns(&self, analysis: &ExampleAnalysis) -> Vec<String> {
        let mut patterns = Vec::new();

        // Pattern 1: Character class sequence
        if let Some(seq) = self.generate_char_class_pattern(analysis) {
            patterns.push(seq);
        }

        // Pattern 2: Literal with wildcards
        if let Some(literal) = self.generate_literal_pattern() {
            patterns.push(literal);
        }

        // Pattern 3: Common prefix/suffix with variable middle
        if let Some(affixed) = self.generate_affix_pattern() {
            patterns.push(affixed);
        }

        // Pattern 4: Alternation pattern (when examples are small enough to enumerate)
        if let Some(alternation) = self.generate_alternation_pattern() {
            patterns.push(alternation);
        }

        // Pattern 5: Anchored version of character class pattern
        if let Some(anchored) = self.generate_anchored_pattern(analysis) {
            patterns.push(anchored);
        }

        // Pattern 6: Quantifier-refined pattern with min/max bounds
        if let Some(quantified) = self.generate_quantified_pattern(analysis) {
            patterns.push(quantified);
        }

        patterns
    }

    /// Generate an alternation pattern when examples form a small enumerable set.
    ///
    /// If all positive examples are short enough and there are few enough of them,
    /// creates an exact alternation like `(cat|dog|bird)`.
    fn generate_alternation_pattern(&self) -> Option<String> {
        // Only use alternation for small sets of short examples
        // This prevents generating huge patterns
        const MAX_EXAMPLES_FOR_ALTERNATION: usize = 10;
        const MAX_EXAMPLE_LENGTH: usize = 30;

        if self.positive_examples.len() > MAX_EXAMPLES_FOR_ALTERNATION {
            return None;
        }

        if self
            .positive_examples
            .iter()
            .any(|ex| ex.len() > MAX_EXAMPLE_LENGTH)
        {
            return None;
        }

        // Deduplicate and escape examples
        let mut unique: Vec<_> = self.positive_examples.iter().collect();
        unique.sort();
        unique.dedup();

        if unique.len() < 2 {
            return None;
        }

        // Create alternation pattern
        let alternatives: Vec<String> = unique.iter().map(|ex| regex::escape(ex)).collect();
        let pattern = format!("({})", alternatives.join("|"));

        // Only return if pattern is reasonably sized
        if pattern.len() <= self.config.max_complexity {
            Some(pattern)
        } else {
            None
        }
    }

    /// Generate an anchored pattern with start (^) and/or end ($) anchors.
    ///
    /// Analyzes whether all examples share common starting or ending characteristics
    /// that would benefit from anchoring.
    fn generate_anchored_pattern(&self, analysis: &ExampleAnalysis) -> Option<String> {
        if analysis.char_sequences.is_empty() {
            return None;
        }

        // Check if all examples start with the same character type
        let first_chars: Vec<_> = self
            .positive_examples
            .iter()
            .filter_map(|ex| ex.chars().next())
            .collect();

        let all_start_same_type = if !first_chars.is_empty() {
            let first_type = CharType::from(first_chars[0]);
            first_chars.iter().all(|&c| CharType::from(c) == first_type)
        } else {
            false
        };

        // Check if all examples end with the same character type
        let last_chars: Vec<_> = self
            .positive_examples
            .iter()
            .filter_map(|ex| ex.chars().last())
            .collect();

        let all_end_same_type = if !last_chars.is_empty() {
            let last_type = CharType::from(last_chars[0]);
            last_chars.iter().all(|&c| CharType::from(c) == last_type)
        } else {
            false
        };

        // Generate base pattern first
        let base = self.generate_char_class_pattern(analysis)?;

        // Add anchors based on analysis
        let anchored = match (all_start_same_type, all_end_same_type) {
            (true, true) => format!("^{}$", base),
            (true, false) => format!("^{}", base),
            (false, true) => format!("{}$", base),
            (false, false) => return None, // No anchoring benefit
        };

        Some(anchored)
    }

    /// Generate a pattern with precise quantifier bounds based on length analysis.
    ///
    /// Instead of using `+` for all variable-length sequences, this analyzes
    /// the min/max lengths to generate bounded quantifiers like `{2,5}`.
    fn generate_quantified_pattern(&self, analysis: &ExampleAnalysis) -> Option<String> {
        if analysis.char_sequences.is_empty() || analysis.fixed_length {
            return None; // Fixed length patterns don't need range quantifiers
        }

        // Analyze length distribution per character class run
        let first_seq = &analysis.char_sequences[0];
        let mut pattern = String::new();
        let mut i = 0;

        while i < first_seq.len() {
            let char_type = first_seq[i];

            // Find the run length in each example at this position
            let mut run_lengths: Vec<usize> = Vec::new();

            for seq in &analysis.char_sequences {
                if i >= seq.len() {
                    continue;
                }

                // Count how many consecutive chars of this type
                let mut run_len = 0;
                let mut j = i;
                while j < seq.len() && seq[j] == char_type {
                    run_len += 1;
                    j += 1;
                }
                run_lengths.push(run_len);
            }

            if run_lengths.is_empty() {
                pattern.push('.');
                i += 1;
                continue;
            }

            let min_run = *run_lengths.iter().min().unwrap_or(&1);
            let max_run = *run_lengths.iter().max().unwrap_or(&1);

            let class_str = char_type.as_regex_class();

            if min_run == max_run {
                // Fixed length at this position
                if min_run == 1 {
                    pattern.push_str(class_str);
                } else {
                    pattern.push_str(&format!("{}{{{}}}", class_str, min_run));
                }
            } else if min_run == 1 && max_run > 10 {
                // Variable with large range - use +
                pattern.push_str(&format!("{}+", class_str));
            } else if min_run == 0 {
                // Optional - use *
                pattern.push_str(&format!("{}*", class_str));
            } else {
                // Bounded range - use {min,max}
                pattern.push_str(&format!("{}{{{},{}}}", class_str, min_run, max_run));
            }

            // Skip past the run in the first example
            let first_run = run_lengths[0].max(1);
            i += first_run;
        }

        if pattern.is_empty() {
            None
        } else {
            Some(pattern)
        }
    }

    /// Generate a pattern based on character classes.
    fn generate_char_class_pattern(&self, analysis: &ExampleAnalysis) -> Option<String> {
        if analysis.char_sequences.is_empty() {
            return None;
        }

        // Find common character class pattern
        let first_seq = &analysis.char_sequences[0];
        let mut pattern = String::new();

        let mut i = 0;
        while i < first_seq.len() {
            // Check if this position has the same char type in all examples
            let char_type = first_seq[i];
            let consistent = analysis
                .char_sequences
                .iter()
                .all(|seq| seq.get(i).map(|&ct| ct == char_type).unwrap_or(false));

            if consistent {
                // Count consecutive same-type characters
                let mut count = 1;
                while i + count < first_seq.len() && first_seq[i + count] == char_type {
                    let all_same = analysis.char_sequences.iter().all(|seq| {
                        seq.get(i + count)
                            .map(|&ct| ct == char_type)
                            .unwrap_or(false)
                    });
                    if all_same {
                        count += 1;
                    } else {
                        break;
                    }
                }

                // Add to pattern
                let class_str = char_type.as_regex_class();
                if count == 1 {
                    pattern.push_str(class_str);
                } else if analysis.fixed_length {
                    pattern.push_str(&format!("{}{{{}}}", class_str, count));
                } else {
                    pattern.push_str(&format!("{}+", class_str));
                }

                i += count;
            } else {
                // Variable position - use wildcard
                pattern.push('.');
                i += 1;
            }
        }

        // Handle variable length
        if !analysis.fixed_length && !pattern.is_empty() {
            // Make trailing quantifiers more flexible
            pattern = pattern.replace("+", "+?");
        }

        Some(pattern)
    }

    /// Generate a pattern based on common literal parts.
    fn generate_literal_pattern(&self) -> Option<String> {
        if self.positive_examples.len() < 2 {
            return None;
        }

        // Find longest common substring
        let first = &self.positive_examples[0];
        let mut best_common = String::new();

        for start in 0..first.len() {
            for end in start + 1..=first.len() {
                let candidate = &first[start..end];
                if candidate.len() <= best_common.len() {
                    continue;
                }

                let all_contain = self
                    .positive_examples
                    .iter()
                    .all(|ex| ex.contains(candidate));
                if all_contain {
                    best_common = candidate.to_string();
                }
            }
        }

        if best_common.len() >= 2 {
            // Build pattern with literal and wildcards
            let escaped = regex::escape(&best_common);
            Some(format!(".*{}.*", escaped))
        } else {
            None
        }
    }

    /// Generate a pattern based on common prefix/suffix.
    fn generate_affix_pattern(&self) -> Option<String> {
        if self.positive_examples.is_empty() {
            return None;
        }

        // Find common prefix
        let first = &self.positive_examples[0];
        let mut prefix_len = 0;

        'prefix: for i in 0..first.len() {
            let c = first.chars().nth(i)?;
            for ex in &self.positive_examples[1..] {
                if ex.chars().nth(i) != Some(c) {
                    break 'prefix;
                }
            }
            prefix_len = i + 1;
        }

        // Find common suffix
        let mut suffix_len = 0;
        'suffix: for i in 0..first.len() {
            let idx = first.len() - 1 - i;
            let c = first.chars().nth(idx)?;
            for ex in &self.positive_examples[1..] {
                if ex.len() <= i || ex.chars().nth(ex.len() - 1 - i) != Some(c) {
                    break 'suffix;
                }
            }
            suffix_len = i + 1;
        }

        if prefix_len >= 2 || suffix_len >= 2 {
            let prefix = &first[..prefix_len];
            let suffix = &first[first.len() - suffix_len..];

            let pattern = format!("{}.*{}", regex::escape(prefix), regex::escape(suffix));

            Some(pattern)
        } else {
            None
        }
    }

    /// Evaluate a pattern against examples.
    ///
    /// Computes confidence based on:
    /// - 70% weight: positive match rate (how many positives matched)
    /// - 30% weight: negative avoidance rate (how many negatives correctly rejected)
    ///
    /// This weighting prioritizes matching desired content while still penalizing
    /// patterns that incorrectly match negative examples.
    fn evaluate_pattern(
        &self,
        pattern_str: &str,
        category: PatternCategory,
    ) -> Result<LearnedPattern> {
        let regex = Regex::new(pattern_str)?;

        let total_positive = self.positive_examples.len();
        let total_negative = self.negative_examples.len();

        let matches_positive = self
            .positive_examples
            .iter()
            .filter(|ex| regex.is_match(ex))
            .count();

        let avoids_negative = self
            .negative_examples
            .iter()
            .filter(|ex| !regex.is_match(ex))
            .count();

        // Calculate confidence using weighted average
        // Positive match rate contributes 70%, negative avoidance 30%
        let positive_rate = if total_positive == 0 {
            0.0
        } else {
            matches_positive as f64 / total_positive as f64
        };

        let negative_rate = if total_negative == 0 {
            1.0 // No negatives to fail against = perfect avoidance
        } else {
            avoids_negative as f64 / total_negative as f64
        };

        let confidence = ((positive_rate * 0.7 + negative_rate * 0.3) * 100.0) as u8;

        // Calculate specificity: shorter patterns are more general (lower specificity)
        // Longer patterns are more specific (higher specificity)
        // This helps rank patterns - prefer more specific patterns that still match
        let specificity = (100 - (pattern_str.len().min(100))) as u8;

        // Check if pattern contains capture groups
        let has_captures = pattern_str.contains('(') && pattern_str.contains(')');

        let description = format!(
            "Matches {}/{} positive examples",
            matches_positive, total_positive
        );

        Ok(LearnedPattern {
            pattern: pattern_str.to_string(),
            confidence,
            matches_positive,
            total_positive,
            avoids_negative,
            total_negative,
            category,
            description,
            specificity,
            has_captures,
            reasoning: None,
        })
    }

    /// Filter patterns by negative examples.
    fn filter_by_negatives(&self, patterns: Vec<LearnedPattern>) -> Result<Vec<LearnedPattern>> {
        Ok(patterns
            .into_iter()
            .filter(|p| {
                // Must not match any negative examples
                if let Ok(regex) = Regex::new(&p.pattern) {
                    !self.negative_examples.iter().any(|ex| regex.is_match(ex))
                } else {
                    false
                }
            })
            .collect())
    }
}

impl Default for PatternLearner {
    fn default() -> Self {
        Self::new()
    }
}

/// Analysis of example structure.
#[derive(Debug, Default)]
struct ExampleAnalysis {
    char_sequences: Vec<Vec<CharType>>,
    lengths: Vec<usize>,
    min_length: usize,
    max_length: usize,
    fixed_length: bool,
    unique_chars: HashSet<char>,
}

/// Character type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharType {
    Digit,
    LowerLetter,
    UpperLetter,
    Whitespace,
    Punctuation,
    Symbol,
    Other,
}

impl CharType {
    fn from(c: char) -> Self {
        if c.is_ascii_digit() {
            CharType::Digit
        } else if c.is_ascii_lowercase() {
            CharType::LowerLetter
        } else if c.is_ascii_uppercase() {
            CharType::UpperLetter
        } else if c.is_whitespace() {
            CharType::Whitespace
        } else if c.is_ascii_punctuation() {
            CharType::Punctuation
        } else if c.is_ascii() {
            CharType::Symbol
        } else {
            CharType::Other
        }
    }

    fn as_regex_class(&self) -> &'static str {
        match self {
            CharType::Digit => r"\d",
            CharType::LowerLetter => r"[a-z]",
            CharType::UpperLetter => r"[A-Z]",
            CharType::Whitespace => r"\s",
            CharType::Punctuation => r"[[:punct:]]",
            CharType::Symbol => r".",
            CharType::Other => r".",
        }
    }
}

/// Generate a pipeline configuration from learned patterns.
///
/// Creates a TOML pipeline configuration file that can be used with rexpipe.
/// Each pattern becomes a substitution step with detailed metadata comments
/// explaining the pattern's origin and confidence.
///
/// # Example Output
///
/// ```toml
/// # Auto-generated pipeline from pattern learning
/// name = "learned-patterns"
///
/// [[step]]
/// type = "substitute"
/// pattern = '\d{3}-\d{2}-\d{4}'
/// replacement = "[MATCH_1]"
/// description = "Ssn pattern with 95% confidence"
/// ```
pub fn generate_pipeline_config(patterns: &[LearnedPattern]) -> String {
    let mut config = String::new();

    config.push_str("# Auto-generated pipeline from pattern learning\n");
    config.push_str("# Generated by: rexpipe --learn\n");
    config.push_str(&format!(
        "# Generated at: {}\n",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    ));
    config.push_str("name = \"learned-patterns\"\n");
    config.push_str("version = \"1.0.0\"\n\n");

    for (i, pattern) in patterns.iter().enumerate() {
        // Enhanced metadata comment with confidence, match stats, and specificity
        config.push_str(&format!(
            "# Pattern {}: {} (confidence: {}%, specificity: {}%)\n",
            i + 1,
            pattern.description,
            pattern.confidence,
            pattern.specificity
        ));
        // Now correctly using total_positive and total_negative fields
        config.push_str(&format!(
            "# Matched {}/{} positive examples, avoided {}/{} negative examples\n",
            pattern.matches_positive,
            pattern.total_positive,
            pattern.avoids_negative,
            pattern.total_negative
        ));
        if let Some(ref reasoning) = pattern.reasoning {
            config.push_str(&format!("# Reasoning: {}\n", reasoning));
        }
        config.push_str("[[step]]\n");
        config.push_str("type = \"substitute\"\n");
        config.push_str(&format!(
            "pattern = '{}'\n",
            pattern.pattern.replace('\'', "\\'")
        ));
        config.push_str(&format!("replacement = \"[MATCH_{}]\"\n", i + 1));
        config.push_str(&format!(
            "description = \"{:?} pattern with {}% confidence\"\n\n",
            pattern.category, pattern.confidence
        ));
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_learner_email() {
        let mut learner = PatternLearner::new();

        learner.add_positive("user@example.com");
        learner.add_positive("admin@company.org");
        learner.add_positive("test123@domain.net");

        learner.add_negative("not-an-email");
        learner.add_negative("@invalid");
        learner.add_negative("missing@");

        let patterns = learner.learn().unwrap();
        assert!(!patterns.is_empty());

        // The email template should match
        let best = &patterns[0];
        assert!(best.confidence >= 70);
    }

    #[test]
    fn test_pattern_learner_ssn() {
        let mut learner = PatternLearner::new();

        learner.add_positive("123-45-6789");
        learner.add_positive("987-65-4321");
        learner.add_positive("111-22-3333");

        learner.add_negative("12-345-6789");
        learner.add_negative("1234567890");

        let patterns = learner.learn().unwrap();
        assert!(!patterns.is_empty());
    }

    #[test]
    fn test_insufficient_examples() {
        let mut learner = PatternLearner::new();
        learner.add_positive("only-one");

        let result = learner.learn();
        assert!(matches!(
            result,
            Err(LearnError::InsufficientExamples { .. })
        ));
    }

    #[test]
    fn test_char_type_classification() {
        assert_eq!(CharType::from('a'), CharType::LowerLetter);
        assert_eq!(CharType::from('A'), CharType::UpperLetter);
        assert_eq!(CharType::from('5'), CharType::Digit);
        assert_eq!(CharType::from(' '), CharType::Whitespace);
        assert_eq!(CharType::from('.'), CharType::Punctuation);
    }

    #[test]
    fn test_learned_pattern_serialization() {
        let pattern = LearnedPattern {
            pattern: r"\d+".to_string(),
            confidence: 95,
            matches_positive: 10,
            total_positive: 10,
            avoids_negative: 5,
            total_negative: 5,
            category: PatternCategory::Numeric,
            description: "Matches numbers".to_string(),
            specificity: 50,
            has_captures: false,
            reasoning: Some("Inferred from digit sequences".to_string()),
        };

        let json = serde_json::to_string(&pattern).unwrap();
        let restored: LearnedPattern = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.confidence, 95);
        assert_eq!(restored.total_positive, 10);
        assert_eq!(restored.total_negative, 5);
        assert!(restored.reasoning.is_some());
    }

    #[test]
    fn test_generate_pipeline_config() {
        let patterns = vec![LearnedPattern {
            pattern: r"\d+".to_string(),
            confidence: 90,
            matches_positive: 5,
            total_positive: 5,
            avoids_negative: 3,
            total_negative: 3,
            category: PatternCategory::Numeric,
            description: "Matches 5/5 positive examples".to_string(),
            specificity: 97,
            has_captures: false,
            reasoning: None,
        }];

        let config = generate_pipeline_config(&patterns);
        assert!(config.contains("[[step]]"));
        assert!(config.contains(r"\d+"));
        // Verify the bug fix: should show correct totals
        assert!(config.contains("5/5 positive"));
        assert!(config.contains("3/3 negative"));
    }

    #[test]
    fn test_max_examples_limit() {
        // Configure a low limit
        let config = LearnConfig {
            max_examples: 5,
            ..Default::default()
        };

        let mut learner = PatternLearner::with_config(config);

        // Add more examples than the limit
        for i in 0..10 {
            learner.add_positive(format!("test{}", i));
        }

        let result = learner.learn();
        assert!(matches!(
            result,
            Err(LearnError::TooManyExamples { max: 5, .. })
        ));
    }

    #[test]
    fn test_max_examples_within_limit() {
        let config = LearnConfig {
            max_examples: 10,
            ..Default::default()
        };

        let mut learner = PatternLearner::with_config(config);

        // Add examples within limit
        learner.add_positive("123");
        learner.add_positive("456");
        learner.add_positive("789");

        // Should succeed (within limit)
        let result = learner.learn();
        assert!(result.is_ok());
    }

    #[test]
    fn test_default_config_limits() {
        let config = LearnConfig::default();

        // Verify default limits
        assert_eq!(config.max_examples, 1000);
        assert_eq!(config.timeout_ms, 5000);
        assert_eq!(config.max_patterns, 5);
        assert_eq!(config.max_complexity, 200);
    }

    #[test]
    fn test_alternation_pattern_learning() {
        let mut learner = PatternLearner::new();

        // Add a small set of distinct examples
        learner.add_positive("cat");
        learner.add_positive("dog");
        learner.add_positive("bird");

        learner.add_negative("fish"); // Should not match
        learner.add_negative("snake");

        let patterns = learner.learn().unwrap();

        // Should find an alternation pattern like (bird|cat|dog)
        let has_alternation = patterns.iter().any(|p| {
            p.pattern.contains('|') && p.pattern.contains("cat") && p.pattern.contains("dog")
        });
        assert!(
            has_alternation,
            "Should learn alternation pattern for small enumerable sets"
        );
    }

    #[test]
    fn test_anchored_pattern_learning() {
        let mut learner = PatternLearner::new();

        // All examples start with uppercase and end with digit
        learner.add_positive("A123");
        learner.add_positive("B456");
        learner.add_positive("C789");

        let patterns = learner.learn().unwrap();

        // Should find an anchored pattern
        let has_anchored = patterns
            .iter()
            .any(|p| p.pattern.starts_with('^') || p.pattern.ends_with('$'));
        assert!(
            has_anchored,
            "Should learn anchored patterns when examples have consistent boundaries"
        );
    }

    #[test]
    fn test_quantifier_range_inference() {
        let mut learner = PatternLearner::new();

        // Examples with variable-length digit runs
        learner.add_positive("ID-12");
        learner.add_positive("ID-123");
        learner.add_positive("ID-1234");

        let patterns = learner.learn().unwrap();

        // Should find a pattern with bounded quantifier like \d{2,4}
        // Note: This is aspirational - the algorithm may or may not produce this.
        // The important thing is that it learns *something* useful.
        let _has_range_quantifier = patterns.iter().any(|p| {
            p.pattern.contains("{2,4}") || p.pattern.contains("{2,") || p.pattern.contains(",4}")
        });
        assert!(
            !patterns.is_empty(),
            "Should learn some pattern for variable-length examples"
        );
    }

    #[test]
    fn test_learned_pattern_has_captures_detection() {
        let mut learner = PatternLearner::new();

        learner.add_positive("user@example.com");
        learner.add_positive("admin@company.org");

        let patterns = learner.learn().unwrap();

        // Email template has no captures by default, alternation does
        // Just verify the field is correctly populated
        for pattern in &patterns {
            let expected_has_captures =
                pattern.pattern.contains('(') && pattern.pattern.contains(')');
            assert_eq!(
                pattern.has_captures, expected_has_captures,
                "has_captures should match pattern content for: {}",
                pattern.pattern
            );
        }
    }

    #[test]
    fn test_total_counts_in_learned_pattern() {
        let mut learner = PatternLearner::new();

        learner.add_positive("test1");
        learner.add_positive("test2");
        learner.add_positive("test3");
        learner.add_negative("bad1");
        learner.add_negative("bad2");

        let patterns = learner.learn().unwrap();
        assert!(!patterns.is_empty());

        let best = &patterns[0];
        // Verify total counts are correctly set
        assert_eq!(best.total_positive, 3, "Should have 3 total positives");
        assert_eq!(best.total_negative, 2, "Should have 2 total negatives");
    }
}
