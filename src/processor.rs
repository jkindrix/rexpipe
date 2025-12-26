//! Core streaming text processor for rexpipe pipelines.
//!
//! This module provides `StreamProcessor`, the heart of rexpipe's text transformation
//! engine. It processes text line-by-line with constant memory usage, making it suitable
//! for files of any size.
//!
//! # Overview
//!
//! The processor executes pipeline steps in sequence against each input line:
//! - **Substitute**: Replace pattern matches with replacement text
//! - **Filter**: Keep or drop lines based on pattern matches
//! - **Extract**: Extract and output matched portions
//! - **Validate**: Check that patterns match (or don't match)
//! - **Transform**: Apply transformations to matched text
//! - **Block**: Multi-line state machine processing
//!
//! # Examples
//!
//! ## Basic Substitution
//!
//! ```rust
//! use rexpipe::pipeline::PipelineConfig;
//! use rexpipe::processor::StreamProcessor;
//! use std::io::Cursor;
//!
//! // Create a simple substitution pipeline
//! let config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUM"));
//! let mut processor = StreamProcessor::new(config).unwrap();
//!
//! // Process input
//! let input = Cursor::new("Order 12345 shipped\nItem 67890 received\n");
//! let mut output = Vec::new();
//! processor.process_stream(input, &mut output).unwrap();
//!
//! let result = String::from_utf8(output).unwrap();
//! assert!(result.contains("Order NUM shipped"));
//! assert!(result.contains("Item NUM received"));
//! ```
//!
//! ## Processing Statistics
//!
//! ```rust
//! use rexpipe::pipeline::PipelineConfig;
//! use rexpipe::processor::StreamProcessor;
//! use std::io::Cursor;
//!
//! let config = PipelineConfig::from_inline_pattern(r"error", Some("ERROR"));
//! let mut processor = StreamProcessor::new(config).unwrap();
//!
//! let input = Cursor::new("error: failed\ninfo: ok\nerror: timeout\n");
//! let mut output = Vec::new();
//! let result = processor.process_stream(input, &mut output).unwrap();
//!
//! // Access processing statistics
//! let stats = processor.get_stats();
//! assert_eq!(stats.lines_read(), 3);
//! ```
//!
//! # Architecture
//!
//! ```text
//! Input Stream
//!      │
//!      ▼
//! ┌─────────────────────────────────┐
//! │     StreamProcessor             │
//! │  ┌───────────────────────────┐  │
//! │  │ Compiled Pipeline Steps   │  │
//! │  │  - Pattern matching       │  │
//! │  │  - Transformations        │  │
//! │  │  - Context tracking       │  │
//! │  └───────────────────────────┘  │
//! │  ┌───────────────────────────┐  │
//! │  │ Statistics & Metrics      │  │
//! │  └───────────────────────────┘  │
//! └─────────────────────────────────┘
//!      │
//!      ▼
//! Output Stream
//! ```

use crate::bidirectional::{BidirectionalManager, Direction, generate_reverse_pipeline};
use crate::error::{PatternError, ValidationError};
use crate::pipeline::{
    BlockAction, ErrorType, MaxLineAction, OnMismatch, PipelineConfig, PipelineError,
    PipelineResult, PipelineSettings, RegexFlag, StepAction, StepResult, StepType, TransformAction,
};
use anyhow::{Context, Result};
use log::{debug, trace};
use regex::{Regex, RegexBuilder};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, Write};
use std::sync::LazyLock;
use std::time::Instant;

/// Pre-compiled regex for detecting repetition patterns like `{10000}` in ReDoS analysis
#[cfg(feature = "pcre")]
static REPETITION_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{(\d+)\}").expect("invalid repetition regex"));

/// Pre-compiled regex for stripping ANSI escape sequences.
/// Matches CSI sequences (e.g., colors), OSC sequences, and simple escape sequences.
static ANSI_ESCAPE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        \x1b\[[0-9;]*[A-Za-z]     |  # CSI sequences (colors, cursor, etc.)
        \x1b\][^\x07]*\x07        |  # OSC sequences (title, etc.)
        \x1b\][^\x1b]*\x1b\\      |  # OSC with ST terminator
        \x1b[PX^_][^\x1b]*\x1b\\  |  # DCS, SOS, PM, APC sequences
        \x1b.                        # Simple two-byte escape sequences
        ",
    )
    .expect("invalid ANSI escape regex")
});

/// Strip ANSI escape sequences from a string.
fn strip_ansi_codes(s: &str) -> std::borrow::Cow<'_, str> {
    ANSI_ESCAPE_REGEX.replace_all(s, "")
}

#[cfg(feature = "pcre")]
use fancy_regex::Regex as FancyRegex;

/// Represents the line ending style detected in input.
///
/// Used internally to preserve the original line ending style when
/// `preserve_line_endings` is enabled in pipeline settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum LineEnding {
    /// Unix-style line ending (LF, `\n`)
    #[default]
    Lf,
    /// Windows-style line ending (CRLF, `\r\n`)
    Crlf,
    /// No line ending (last line of file without trailing newline)
    None,
}

impl LineEnding {
    /// Get the byte sequence for this line ending.
    fn as_bytes(&self) -> &'static [u8] {
        match self {
            LineEnding::Lf => b"\n",
            LineEnding::Crlf => b"\r\n",
            LineEnding::None => b"",
        }
    }
}

/// Detect the line ending style from a line buffer
fn detect_line_ending(line: &str) -> LineEnding {
    if line.ends_with("\r\n") {
        LineEnding::Crlf
    } else if line.ends_with('\n') {
        LineEnding::Lf
    } else {
        LineEnding::None
    }
}

/// Result of handling a line that exceeds the maximum length limit.
///
/// This enum represents the three possible outcomes when a line
/// is too long based on the configured `max_line_action`.
#[derive(Debug)]
enum LongLineResult {
    /// Skip the line - output it unchanged, don't process
    Skip,
    /// Line was truncated - continue processing with truncated content
    Truncated,
    /// Return an error - line is too long and error action is configured
    Error(String),
}

/// Handle a line that exceeds the maximum length limit.
///
/// # Arguments
///
/// * `line` - The line buffer (may be mutated if truncating)
/// * `line_number` - Current line number for error messages
/// * `max_length` - Maximum allowed line length in bytes
/// * `action` - What to do when line exceeds limit
///
/// # Returns
///
/// `LongLineResult` indicating how the line was handled
fn handle_long_line(
    line: &mut String,
    line_number: u64,
    max_length: usize,
    action: MaxLineAction,
) -> LongLineResult {
    match action {
        MaxLineAction::Error => LongLineResult::Error(format!(
            "Line {} exceeds maximum length ({} > {} bytes). \
             Use --max-line-action=skip to skip long lines, or \
             --max-line-action=truncate to truncate them.",
            line_number,
            line.len(),
            max_length
        )),
        MaxLineAction::Skip => {
            debug!(
                "Skipping line {} ({} bytes exceeds limit of {})",
                line_number,
                line.len(),
                max_length
            );
            LongLineResult::Skip
        }
        MaxLineAction::Truncate => {
            debug!(
                "Truncating line {} from {} to {} bytes",
                line_number,
                line.len(),
                max_length
            );
            // Truncate at a UTF-8 character boundary.
            //
            // Why char_indices: Rust strings are UTF-8, and multi-byte characters
            // (emoji, CJK, etc.) must not be split mid-character. char_indices()
            // gives us (byte_offset, char) pairs. We find the last character that
            // ends before max_length, then truncate after it.
            //
            // Why i + c.len_utf8(): 'i' is the start byte of the character, and
            // len_utf8() gives its byte length (1-4). Adding them gives the byte
            // offset where this character ends, which is our safe truncation point.
            let truncate_at = line
                .char_indices()
                .take_while(|(i, _)| *i < max_length)
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(max_length);
            line.truncate(truncate_at);
            // Ensure we have a newline after truncation
            if !line.ends_with('\n') {
                line.push('\n');
            }
            LongLineResult::Truncated
        }
    }
}

/// Represents a line with its metadata for context tracking
#[derive(Debug, Clone)]
struct ContextLine {
    line_number: u64,
    content: String,
    line_ending: LineEnding,
}

/// Pattern complexity analysis result.
///
/// Used internally to assess regex patterns for potential performance issues.
/// The score helps identify patterns that may be slow to compile or match,
/// providing early feedback to users before they hit performance problems.
struct PatternComplexity {
    /// Complexity score from 0 (simple) to 100 (very complex)
    ///
    /// Guidelines:
    /// - 0-30: Simple patterns, no concerns
    /// - 31-50: Moderate complexity, logged for debugging
    /// - 51-80: High complexity, warning logged
    /// - 81-100: Very high complexity, warning printed to stderr
    score: u8,
    /// Human-readable explanation of why the pattern is complex
    explanation: String,
    /// Optional suggestion for optimizing the pattern
    optimization_hint: Option<String>,
}

/// Context for substitution operations with variable expansion.
///
/// This struct groups related parameters for `apply_substitution_with_vars`
/// to improve code clarity and avoid functions with too many arguments.
/// See: <https://github.com/rust-unofficial/patterns/discussions/239>
struct SubstitutionContext<'a> {
    /// The compiled pattern to match against
    pattern: &'a CompiledPattern,
    /// Input text to process
    input: &'a str,
    /// Replacement template (may contain ${seq}, ${count}, capture groups)
    replacement: &'a str,
    /// Whether to replace all matches (global) or just the first
    is_global: bool,
    /// Index of the current pipeline step (for sequence tracking)
    step_index: usize,
    /// Whether to record bidirectional mappings
    record_mappings: bool,
}

/// Core streaming text processor for rexpipe pipelines.
///
/// `StreamProcessor` executes a configured pipeline against text input,
/// processing line-by-line with constant memory usage regardless of input size.
/// It supports substitution, filtering, extraction, validation, and transformation
/// operations through a unified streaming interface.
///
/// # Features
///
/// - **Streaming Processing**: Processes input line-by-line with O(1) memory usage
/// - **Context Lines**: Supports before/after context lines (like grep -B/-A)
/// - **Multiple Regex Engines**: Standard Rust regex, PCRE via fancy-regex, or fixed strings
/// - **Line Ending Preservation**: Optionally preserves CRLF vs LF line endings
/// - **Timeout Protection**: Per-line timeout to prevent ReDoS hangs
///
/// # Example
///
/// ```
/// use rexpipe::pipeline::PipelineConfig;
/// use rexpipe::processor::StreamProcessor;
/// use std::io::Cursor;
///
/// let config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUM"));
/// let mut processor = StreamProcessor::new(config).unwrap();
///
/// let input = Cursor::new("Order 123 shipped\nOrder 456 pending\n");
/// let mut output = Vec::new();
/// let result = processor.process_stream(input, &mut output).unwrap();
///
/// assert_eq!(result.matches_found, 2);
/// ```
pub struct StreamProcessor {
    config: PipelineConfig,
    compiled_steps: Vec<CompiledStep>,
    stats: ProcessorStats,
    /// Buffer for before-context lines.
    ///
    /// Why VecDeque: We need O(1) push_back and pop_front operations to maintain
    /// a sliding window of N most recent lines. VecDeque provides this via a ring
    /// buffer, while Vec would require O(N) shifts for pop_front.
    context_before_buffer: VecDeque<ContextLine>,
    /// Counter for remaining after-context lines to output
    after_context_remaining: usize,
    /// Track which lines have been output to avoid duplicates.
    ///
    /// Why track line numbers: When context ranges overlap (e.g., two matches close
    /// together), we must avoid printing the same line twice. Tracking the last
    /// output line number lets us skip already-printed lines efficiently.
    last_output_line: u64,
    /// Track active blocks for each Block step (step_index -> is_active)
    block_states: Vec<bool>,
    /// Track seen prefixes for deduplicate_by_prefix filter action (step_index -> seen prefixes)
    dedup_prefix_seen: HashMap<usize, std::collections::HashSet<String>>,
    /// Buffer for block content for block-level deduplication (step_index -> current block content)
    dedup_block_buffer: HashMap<usize, Vec<String>>,
    /// Set of seen block hashes for block deduplication (step_index -> seen block hashes)
    dedup_block_seen: HashMap<usize, std::collections::HashSet<u64>>,
    /// Buffer for block context overlap (step_index -> trailing content from previous block)
    block_overlap_buffer: HashMap<usize, String>,
    /// Buffer for content-filtered blocks (step_index -> buffered lines)
    block_content_buffer: HashMap<usize, Vec<String>>,
    /// Track if content pattern matched in current block (step_index -> matched)
    block_content_matched: HashMap<usize, bool>,
    /// Track seen extracted values for deduplicate in extract steps (step_index -> seen values)
    dedup_extract_seen: HashMap<usize, std::collections::HashSet<String>>,
    /// Track whether CSV header has been written for each step (step_index -> header_written)
    csv_header_written: HashMap<usize, bool>,
    /// State for finalize section (aggregation counters)
    finalize_state: Option<FinalizeState>,
    /// Per-step sequence counters for ${seq} variable expansion (step_index -> current seq)
    seq_counters: HashMap<usize, usize>,
    /// Global match count for ${count} variable expansion
    global_match_count: usize,
    /// Bidirectional transform manager for recording/replaying mappings
    bidirectional_manager: Option<BidirectionalManager>,
    /// When true, output dropped lines to stderr (for debugging filters)
    show_dropped: bool,
}

/// State for finalize section - tracks counters and collected values during processing.
#[derive(Debug, Clone)]
pub struct FinalizeState {
    /// Compiled counter patterns with their state
    pub counters: Vec<CompiledCounter>,
    /// Total lines processed
    pub lines_processed: u64,
    /// Total matches across all steps
    pub total_matches: u64,
    /// Total transformations applied
    pub total_transformations: u64,
}

/// A compiled counter with its pattern and current state.
#[derive(Debug, Clone)]
pub struct CompiledCounter {
    /// Counter name (for template reference)
    pub name: String,
    /// Compiled pattern for matching
    pub pattern: CompiledPattern,
    /// Current counter value
    pub count: u64,
    /// Whether to deduplicate (count unique values only)
    pub deduplicate: bool,
    /// Set of seen values (used when deduplicate is true)
    pub seen_values: std::collections::HashSet<String>,
    /// Whether to collect matched values
    pub collect_values: bool,
    /// Maximum values to collect
    pub max_collected_values: usize,
    /// Collected values (when collect_values is true)
    pub collected: Vec<String>,
}

impl FinalizeState {
    /// Create a new FinalizeState from FinalizeConfig
    pub fn new(
        config: &crate::pipeline::FinalizeConfig,
        settings: &crate::pipeline::PipelineSettings,
    ) -> Result<Self> {
        let mut counters = Vec::new();

        for counter_config in &config.counters {
            let pattern = StreamProcessor::build_pattern(
                &counter_config.pattern,
                &counter_config.flags,
                settings,
            )?;

            counters.push(CompiledCounter {
                name: counter_config.name.clone(),
                pattern,
                count: 0,
                deduplicate: counter_config.deduplicate,
                seen_values: std::collections::HashSet::new(),
                collect_values: counter_config.collect_values,
                max_collected_values: counter_config.max_collected_values,
                collected: Vec::new(),
            });
        }

        Ok(Self {
            counters,
            lines_processed: 0,
            total_matches: 0,
            total_transformations: 0,
        })
    }

    /// Update counters for a line
    pub fn process_line(&mut self, line: &str) {
        self.lines_processed += 1;

        for counter in &mut self.counters {
            // Check if pattern matches
            if let Some(matched) = counter.pattern.find(line) {
                let value = matched.as_str().to_string();

                if counter.deduplicate {
                    // Only count if we haven't seen this value
                    if !counter.seen_values.contains(&value) {
                        counter.seen_values.insert(value.clone());
                        counter.count += 1;

                        if counter.collect_values
                            && counter.collected.len() < counter.max_collected_values
                        {
                            counter.collected.push(value);
                        }
                    }
                } else {
                    // Count all matches
                    counter.count += 1;

                    if counter.collect_values
                        && counter.collected.len() < counter.max_collected_values
                    {
                        counter.collected.push(value);
                    }
                }
            }
        }
    }

    /// Get counter value by name
    pub fn get_counter(&self, name: &str) -> u64 {
        self.counters
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.count)
            .unwrap_or(0)
    }

    /// Render the finalize template with counter values
    pub fn render_template(&self, template: &str) -> String {
        let mut result = template.to_string();

        // Replace ${count:NAME} placeholders
        for counter in &self.counters {
            let placeholder = format!("${{count:{}}}", counter.name);
            result = result.replace(&placeholder, &counter.count.to_string());
        }

        // Replace built-in variables
        result = result.replace("${lines}", &self.lines_processed.to_string());
        result = result.replace("${matches}", &self.total_matches.to_string());
        result = result.replace(
            "${transformations}",
            &self.total_transformations.to_string(),
        );

        result
    }

    /// Convert to JSON for JSON output mode
    pub fn to_json(&self) -> serde_json::Value {
        let mut counters_obj = serde_json::Map::new();

        for counter in &self.counters {
            let mut counter_data = serde_json::Map::new();
            counter_data.insert("count".to_string(), serde_json::json!(counter.count));

            if counter.collect_values && !counter.collected.is_empty() {
                counter_data.insert("values".to_string(), serde_json::json!(counter.collected));
            }

            if counter.deduplicate {
                counter_data.insert("unique".to_string(), serde_json::json!(true));
            }

            counters_obj.insert(
                counter.name.clone(),
                serde_json::Value::Object(counter_data),
            );
        }

        serde_json::json!({
            "lines_processed": self.lines_processed,
            "total_matches": self.total_matches,
            "total_transformations": self.total_transformations,
            "counters": counters_obj
        })
    }
}

/// Abstraction over different regex engines.
///
/// `CompiledPattern` provides a unified interface for pattern matching across
/// different matching strategies. The internal representation is opaque - use the
/// provided methods (`is_match`, `replace_all`, etc.) rather than matching on variants.
///
/// # Matching Strategies
///
/// - **Standard**: Uses the Rust `regex` crate with linear-time guarantees (ReDoS-safe)
/// - **Fixed**: Literal string matching (fastest, no regex interpretation)
/// - **PCRE**: Uses `fancy-regex` for advanced features like lookahead/lookbehind
///
/// The engine is selected based on pipeline settings (`fixed_strings`, `pcre_mode`).
///
/// # Thread Safety
///
/// All variants are `Send + Sync`, enabling safe use in parallel processing.
///
/// # Stability
///
/// This enum is marked `#[non_exhaustive]` to allow adding new matching strategies
/// in future versions without breaking existing code. Always use the provided methods
/// rather than pattern matching on variants.
#[derive(Clone)]
#[non_exhaustive]
pub enum CompiledPattern {
    /// Standard Rust regex (fast, ReDoS-safe with linear-time guarantees)
    Standard(Regex),
    /// Fixed string matching (fastest, no regex interpretation)
    Fixed(String),
    /// PCRE-compatible regex via fancy-regex (supports lookahead/lookbehind)
    #[cfg(feature = "pcre")]
    Pcre(FancyRegex),
}

impl std::fmt::Debug for CompiledPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompiledPattern::Standard(re) => write!(f, "Standard({})", re.as_str()),
            CompiledPattern::Fixed(s) => write!(f, "Fixed({})", s),
            #[cfg(feature = "pcre")]
            CompiledPattern::Pcre(re) => write!(f, "Pcre({})", re.as_str()),
        }
    }
}

/// A simple match result containing the matched text.
#[derive(Debug, Clone)]
pub struct PatternMatch {
    matched: String,
}

impl PatternMatch {
    /// Get the matched text as a string slice
    pub fn as_str(&self) -> &str {
        &self.matched
    }
}

impl CompiledPattern {
    pub fn is_match(&self, text: &str) -> bool {
        match self {
            CompiledPattern::Standard(re) => re.is_match(text),
            CompiledPattern::Fixed(s) => text.contains(s),
            #[cfg(feature = "pcre")]
            CompiledPattern::Pcre(re) => re.is_match(text).unwrap_or(false),
        }
    }

    /// Find the first match in the text.
    ///
    /// Returns Some(PatternMatch) if a match is found, None otherwise.
    /// For capture groups, the first capture group is returned if present,
    /// otherwise the full match is returned.
    pub fn find(&self, text: &str) -> Option<PatternMatch> {
        match self {
            CompiledPattern::Standard(re) => {
                // Check for capture groups - if present, use first group
                if let Some(caps) = re.captures(text) {
                    // Return first capture group if exists, otherwise full match
                    let matched = caps
                        .get(1)
                        .or_else(|| caps.get(0))
                        .map(|m| m.as_str().to_string())?;
                    Some(PatternMatch { matched })
                } else {
                    None
                }
            }
            CompiledPattern::Fixed(s) => {
                if text.contains(s) {
                    Some(PatternMatch { matched: s.clone() })
                } else {
                    None
                }
            }
            #[cfg(feature = "pcre")]
            CompiledPattern::Pcre(re) => {
                // Check for capture groups - if present, use first group
                if let Ok(Some(caps)) = re.captures(text) {
                    // Return first capture group if exists, otherwise full match
                    let matched = caps
                        .get(1)
                        .or_else(|| caps.get(0))
                        .map(|m| m.as_str().to_string())?;
                    Some(PatternMatch { matched })
                } else {
                    None
                }
            }
        }
    }

    pub fn replace_all(&self, text: &str, replacement: &str) -> String {
        match self {
            CompiledPattern::Standard(re) => re.replace_all(text, replacement).to_string(),
            CompiledPattern::Fixed(s) => text.replace(s, replacement),
            #[cfg(feature = "pcre")]
            CompiledPattern::Pcre(re) => re.replace_all(text, replacement).to_string(),
        }
    }

    pub fn replace(&self, text: &str, replacement: &str) -> String {
        match self {
            CompiledPattern::Standard(re) => re.replace(text, replacement).to_string(),
            CompiledPattern::Fixed(s) => text.replacen(s, replacement, 1),
            #[cfg(feature = "pcre")]
            CompiledPattern::Pcre(re) => re.replace(text, replacement).to_string(),
        }
    }

    /// Replace all matches and return both the result and match count in a single pass.
    ///
    /// Why single-pass: Running the regex twice (once to count, once to replace) doubles
    /// CPU time for large inputs. By using a closure in replace_all, we increment a counter
    /// during the replacement pass itself. For the standard regex engine, we use `Cell<usize>`
    /// because Rust closures in replace_all are FnMut, but Cell provides interior mutability
    /// without requiring &mut self.
    pub fn replace_all_counting(&self, text: &str, replacement: &str) -> (String, usize) {
        match self {
            CompiledPattern::Standard(re) => {
                use std::cell::Cell;
                let count = Cell::new(0usize);
                // Use closure to count while expanding capture groups properly
                let result = re
                    .replace_all(text, |caps: &regex::Captures| {
                        count.set(count.get() + 1);
                        let mut dst = String::new();
                        caps.expand(replacement, &mut dst);
                        dst
                    })
                    .to_string();
                (result, count.get())
            }
            CompiledPattern::Fixed(s) => {
                let count = text.matches(s).count();
                let result = text.replace(s, replacement);
                (result, count)
            }
            #[cfg(feature = "pcre")]
            CompiledPattern::Pcre(re) => {
                // fancy_regex doesn't support closure-based replace, so count first
                let count = re.find_iter(text).filter_map(|m| m.ok()).count();
                let result = re.replace_all(text, replacement).to_string();
                (result, count)
            }
        }
    }

    /// Replace first match and return both the result and whether a match occurred.
    /// This avoids running the regex twice.
    pub fn replace_counting(&self, text: &str, replacement: &str) -> (String, bool) {
        match self {
            CompiledPattern::Standard(re) => {
                use std::cell::Cell;
                let had_match = Cell::new(false);
                let result = re
                    .replace(text, |caps: &regex::Captures| {
                        had_match.set(true);
                        let mut dst = String::new();
                        caps.expand(replacement, &mut dst);
                        dst
                    })
                    .to_string();
                (result, had_match.get())
            }
            CompiledPattern::Fixed(s) => {
                let had_match = text.contains(s);
                let result = text.replacen(s, replacement, 1);
                (result, had_match)
            }
            #[cfg(feature = "pcre")]
            CompiledPattern::Pcre(re) => {
                let had_match = re.is_match(text).unwrap_or(false);
                let result = re.replace(text, replacement).to_string();
                (result, had_match)
            }
        }
    }

    pub fn find_iter<'a>(&'a self, text: &'a str) -> Vec<(usize, usize, String)> {
        match self {
            CompiledPattern::Standard(re) => re
                .find_iter(text)
                .map(|m| (m.start(), m.end(), m.as_str().to_string()))
                .collect(),
            CompiledPattern::Fixed(s) => text
                .match_indices(s)
                .map(|(start, matched)| (start, start + matched.len(), matched.to_string()))
                .collect(),
            #[cfg(feature = "pcre")]
            CompiledPattern::Pcre(re) => re
                .find_iter(text)
                .filter_map(|m| m.ok())
                .map(|m| (m.start(), m.end(), m.as_str().to_string()))
                .collect(),
        }
    }

    pub fn captures_iter<'a>(&'a self, text: &'a str) -> Vec<CaptureGroup> {
        match self {
            CompiledPattern::Standard(re) => re
                .captures_iter(text)
                .map(|caps| {
                    let groups: Vec<Option<String>> = (0..caps.len())
                        .map(|i| caps.get(i).map(|m| m.as_str().to_string()))
                        .collect();
                    let full_match = caps
                        .get(0)
                        .map(|m| (m.start(), m.end(), m.as_str().to_string()));
                    CaptureGroup { groups, full_match }
                })
                .collect(),
            CompiledPattern::Fixed(s) => text
                .match_indices(s)
                .map(|(start, matched)| CaptureGroup {
                    groups: vec![Some(matched.to_string())],
                    full_match: Some((start, start + matched.len(), matched.to_string())),
                })
                .collect(),
            #[cfg(feature = "pcre")]
            CompiledPattern::Pcre(re) => re
                .captures_iter(text)
                .filter_map(|caps| caps.ok())
                .map(|caps| {
                    let groups: Vec<Option<String>> = (0..caps.len())
                        .map(|i| caps.get(i).map(|m| m.as_str().to_string()))
                        .collect();
                    let full_match = caps
                        .get(0)
                        .map(|m| (m.start(), m.end(), m.as_str().to_string()));
                    CaptureGroup { groups, full_match }
                })
                .collect(),
        }
    }

    /// Expand capture groups in a replacement string for a specific match.
    ///
    /// This method finds captures for the match at the given position and expands
    /// backreferences like `$1`, `$2`, etc. in the replacement string.
    pub fn expand_captures(
        &self,
        text: &str,
        start: usize,
        _end: usize,
        replacement: &str,
    ) -> String {
        match self {
            CompiledPattern::Standard(re) => {
                // Find captures for this specific match
                if let Some(caps) = re.captures(&text[start..]) {
                    let mut result = String::new();
                    caps.expand(replacement, &mut result);
                    result
                } else {
                    replacement.to_string()
                }
            }
            CompiledPattern::Fixed(_) => {
                // Fixed strings don't have capture groups
                replacement.to_string()
            }
            #[cfg(feature = "pcre")]
            CompiledPattern::Pcre(re) => {
                // fancy_regex has different API for captures
                if let Ok(Some(caps)) = re.captures(&text[start..]) {
                    // Manual expansion for fancy_regex
                    let mut result = replacement.to_string();
                    for i in (0..=9).rev() {
                        if let Some(m) = caps.get(i) {
                            result = result.replace(&format!("${}", i), m.as_str());
                            result = result.replace(&format!("${{{}}}", i), m.as_str());
                        }
                    }
                    // Handle $0 / full match
                    if let Some(m) = caps.get(0) {
                        result = result.replace("$0", m.as_str());
                        result = result.replace("${0}", m.as_str());
                    }
                    result
                } else {
                    replacement.to_string()
                }
            }
        }
    }

    /// Returns the original pattern string.
    ///
    /// This method provides access to the pattern used to create this compiled pattern
    /// without exposing the internal regex engine representation.
    ///
    /// # Examples
    ///
    /// ```
    /// use rexpipe::processor::StreamProcessor;
    /// use rexpipe::pipeline::PipelineConfig;
    ///
    /// let config = PipelineConfig::from_inline_pattern(r"\d+", None);
    /// let processor = StreamProcessor::new(config).unwrap();
    /// // Pattern string is available for debugging/display
    /// ```
    pub fn pattern_str(&self) -> &str {
        match self {
            CompiledPattern::Standard(re) => re.as_str(),
            CompiledPattern::Fixed(s) => s,
            #[cfg(feature = "pcre")]
            CompiledPattern::Pcre(re) => re.as_str(),
        }
    }
}

/// Represents captured groups from a regex match.
///
/// Contains both the full match and any named or numbered capture groups
/// extracted during pattern matching.
#[derive(Debug, Clone)]
pub struct CaptureGroup {
    /// Captured group values (index 0 is full match, 1+ are capture groups).
    /// `None` indicates an optional group that didn't participate in the match.
    pub groups: Vec<Option<String>>,
    /// The full match as (start_offset, end_offset, matched_text).
    /// `None` if there was no match.
    pub full_match: Option<(usize, usize, String)>,
}

struct CompiledStep {
    step_index: usize,
    pattern: CompiledPattern,
    /// Negative pattern - if this matches, the line is excluded even if `pattern` matches
    not_pattern: Option<CompiledPattern>,
    replacement: Option<String>,
    /// Unified action for Filter and Block steps
    action: Option<StepAction>,
    transform_action: Option<TransformAction>,
    step_type: StepType,
    is_global: bool,
    // Block step fields
    /// End pattern for block steps (previously `until`)
    end_pattern: Option<CompiledPattern>,
    /// Content pattern for filtering blocks - blocks only kept/dropped if content matches
    content_pattern: Option<CompiledPattern>,
    block_context: Option<crate::pipeline::BlockContextValue>,
    // Validation step fields
    /// Action to take when validation fails
    on_mismatch: OnMismatch,
    // Extract step enhancements
    capture_names: Option<Vec<String>>,
    output_format: Option<crate::pipeline::ExtractOutputFormat>,
    output_template: Option<String>,
    first_only: bool,
    deduplicate: bool,
    /// Human-readable name for this step (used in trace/debug output)
    name: Option<String>,
    // Syntax-aware processing fields (require tree-sitter feature)
    /// Languages this step applies to (if any of these match, the step is applied)
    #[cfg(feature = "tree-sitter")]
    languages: Option<Vec<crate::syntax::Language>>,
    #[cfg(feature = "tree-sitter")]
    scope_filter: Option<crate::syntax::ScopeFilter>,
}

/// Runtime statistics collected during stream processing.
///
/// Provides performance metrics including line counts, byte throughput,
/// and per-step timing breakdowns for optimization and debugging.
///
/// # Field Access
///
/// While fields are currently public for backward compatibility, prefer using
/// the accessor methods which provide a stable API.
///
/// # Construction
///
/// This struct cannot be constructed outside of rexpipe due to `#[non_exhaustive]`.
/// Use [`StreamProcessor::get_stats`] to obtain processing statistics.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct ProcessorStats {
    /// Total number of lines read from input
    pub lines_read: u64,
    /// Total bytes processed from input stream
    pub bytes_processed: u64,
    /// Timestamp when processing started (for throughput calculation)
    pub processing_start: Option<Instant>,
    /// Per-step cumulative processing time in milliseconds (step_index -> ms)
    pub step_timings: HashMap<usize, u64>,
}

impl ProcessorStats {
    /// Get the total number of lines read from input.
    #[inline]
    pub fn lines_read(&self) -> u64 {
        self.lines_read
    }

    /// Get the total bytes processed from input stream.
    #[inline]
    pub fn bytes_processed(&self) -> u64 {
        self.bytes_processed
    }

    /// Get the timestamp when processing started.
    #[inline]
    pub fn processing_start(&self) -> Option<Instant> {
        self.processing_start
    }

    /// Get per-step cumulative processing time in milliseconds.
    #[inline]
    pub fn step_timings(&self) -> &HashMap<usize, u64> {
        &self.step_timings
    }

    /// Calculate elapsed processing time since start.
    ///
    /// Returns `None` if processing hasn't started yet.
    pub fn elapsed(&self) -> Option<std::time::Duration> {
        self.processing_start.map(|start| start.elapsed())
    }
}

/// Detailed information about a single regex match.
///
/// This struct provides comprehensive information about where a pattern matched,
/// what was captured, and what the replacement would look like.
///
/// # Field Access
///
/// While fields are currently public for backward compatibility, prefer using
/// the accessor methods (`line_number()`, `full_match()`, etc.) which provide
/// a stable API. Direct field access may be deprecated in future versions.
///
/// # Construction
///
/// This struct cannot be constructed outside of rexpipe due to `#[non_exhaustive]`.
/// Use [`StreamProcessor::inspect_line`] to obtain match information.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MatchInfo {
    /// Line number where this match occurred (1-based, for display)
    pub line_number: u64,
    /// Byte offset where the match starts
    pub byte_start: usize,
    /// Byte offset where the match ends
    pub byte_end: usize,
    /// The full matched text
    pub full_match: String,
    /// Captured groups (index 0 is full match, 1+ are capture groups)
    pub captures: Vec<Option<String>>,
    /// Preview of what the replacement would look like
    pub replacement_preview: Option<String>,
    /// Index of the pipeline step that produced this match
    pub step_index: usize,
}

impl MatchInfo {
    /// Get the line number where this match occurred (1-based).
    #[inline]
    pub fn line_number(&self) -> u64 {
        self.line_number
    }

    /// Get the byte offset where the match starts.
    #[inline]
    pub fn byte_start(&self) -> usize {
        self.byte_start
    }

    /// Get the byte offset where the match ends.
    #[inline]
    pub fn byte_end(&self) -> usize {
        self.byte_end
    }

    /// Get the full matched text.
    #[inline]
    pub fn full_match(&self) -> &str {
        &self.full_match
    }

    /// Get the captured groups (index 0 is full match, 1+ are capture groups).
    #[inline]
    pub fn captures(&self) -> &[Option<String>] {
        &self.captures
    }

    /// Get the replacement preview, if available.
    #[inline]
    pub fn replacement_preview(&self) -> Option<&str> {
        self.replacement_preview.as_deref()
    }

    /// Get the index of the pipeline step that produced this match.
    #[inline]
    pub fn step_index(&self) -> usize {
        self.step_index
    }

    /// Get the byte range of this match as a Range.
    ///
    /// # Example
    ///
    /// ```
    /// # use rexpipe::processor::MatchInfo;
    /// // MatchInfo is obtained via StreamProcessor::inspect_line()
    /// // Example shows usage once you have a MatchInfo instance
    /// ```
    #[inline]
    pub fn byte_range(&self) -> std::ops::Range<usize> {
        self.byte_start..self.byte_end
    }

    /// Get the length of the match in bytes.
    #[inline]
    pub fn match_len(&self) -> usize {
        self.byte_end - self.byte_start
    }

    /// Check if this match has any captured groups (beyond the full match).
    #[inline]
    pub fn has_captures(&self) -> bool {
        self.captures.len() > 1
    }

    /// Get a specific capture group by index (1-based for named groups).
    ///
    /// Returns `None` if the index is out of bounds or the group didn't participate
    /// in the match.
    pub fn get_capture(&self, index: usize) -> Option<&str> {
        self.captures.get(index).and_then(|c| c.as_deref())
    }
}

impl StreamProcessor {
    /// Create a new StreamProcessor from a pipeline configuration.
    ///
    /// # Arguments
    /// * `config` - The pipeline configuration to use for processing
    ///
    /// # Returns
    /// A Result containing the processor or an error if validation fails
    ///
    /// # Example
    /// ```
    /// use rexpipe::pipeline::PipelineConfig;
    /// use rexpipe::processor::StreamProcessor;
    /// use std::io::Cursor;
    ///
    /// // Create a simple substitution pipeline
    /// let config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUM"));
    /// let mut processor = StreamProcessor::new(config).unwrap();
    ///
    /// // Process some text
    /// let input = Cursor::new("There are 123 apples and 456 oranges");
    /// let mut output = Vec::new();
    /// let result = processor.process_stream(input, &mut output).unwrap();
    ///
    /// assert_eq!(result.matches_found, 2);
    /// let output_str = String::from_utf8(output).unwrap();
    /// assert!(output_str.contains("NUM"));
    /// ```
    pub fn new(config: PipelineConfig) -> Result<Self> {
        debug!("Creating StreamProcessor with {} steps", config.step.len());

        // If bidirectional reverse mode is enabled, generate reversed pipeline
        let config = if config.bidirectional.enabled
            && config.bidirectional.direction == Direction::Reverse
        {
            debug!("Bidirectional reverse mode enabled, generating reversed pipeline");
            match generate_reverse_pipeline(&config) {
                Ok(reversed) => {
                    debug!(
                        "Successfully generated reversed pipeline with {} steps",
                        reversed.step.len()
                    );
                    reversed
                }
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Failed to generate reversed pipeline: {}. \
                        Only substitute and reversible transform steps can be reversed.",
                        e
                    ));
                }
            }
        } else {
            config
        };

        if let Err(validation_errors) = config.validate() {
            debug!("Pipeline validation failed: {:?}", validation_errors);
            let error = ValidationError::Multiple {
                count: validation_errors.len(),
                errors: validation_errors.join("\n  - "),
            };
            return Err(error).context("Pipeline validation failed");
        }

        let compiled_steps = Self::compile_steps(&config)?;
        debug!("Compiled {} enabled steps", compiled_steps.len());

        // Initialize block states for Block step types
        let block_states = vec![false; compiled_steps.len()];

        // Initialize finalize state if configured
        let finalize_state = if config.finalize.is_configured() {
            debug!(
                "Initializing finalize state with {} counters",
                config.finalize.counters.len()
            );
            Some(FinalizeState::new(&config.finalize, &config.settings)?)
        } else {
            None
        };

        // Initialize bidirectional manager if enabled
        let bidirectional_manager = if config.bidirectional.enabled {
            debug!("Initializing bidirectional manager");
            match BidirectionalManager::new(config.bidirectional.clone()) {
                Ok(manager) => Some(manager),
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Failed to initialize bidirectional manager: {}",
                        e
                    ));
                }
            }
        } else {
            None
        };

        Ok(Self {
            config,
            compiled_steps,
            stats: ProcessorStats::default(),
            context_before_buffer: VecDeque::new(),
            after_context_remaining: 0,
            last_output_line: 0,
            block_states,
            dedup_prefix_seen: HashMap::new(),
            dedup_block_buffer: HashMap::new(),
            dedup_block_seen: HashMap::new(),
            block_overlap_buffer: HashMap::new(),
            block_content_buffer: HashMap::new(),
            block_content_matched: HashMap::new(),
            dedup_extract_seen: HashMap::new(),
            csv_header_written: HashMap::new(),
            finalize_state,
            seq_counters: HashMap::new(),
            global_match_count: 0,
            bidirectional_manager,
            show_dropped: false,
        })
    }

    /// Enable output of dropped lines to stderr for debugging
    pub fn set_show_dropped(&mut self, show: bool) {
        self.show_dropped = show;
    }

    /// Check if context lines feature is enabled
    fn has_context(&self) -> bool {
        self.config.settings.context_before > 0 || self.config.settings.context_after > 0
    }

    fn compile_steps(config: &PipelineConfig) -> Result<Vec<CompiledStep>> {
        let mut compiled_steps = Vec::new();
        let settings = &config.settings;

        for (index, step) in config.enabled_steps().enumerate() {
            // Default to global replacement for consistency with CLI behavior (Issue #11)
            // When flags is None (not specified), default to global=true
            // When flags is Some([...]), respect whether Global is in the list
            let is_global = step
                .flags
                .as_ref()
                .map(|f| f.iter().any(|flag| matches!(flag, RegexFlag::Global)))
                .unwrap_or(true);

            // For Block steps, use start_pattern; for others, use pattern
            let pattern_str = if matches!(step.step_type, StepType::Block) {
                step.start_pattern.as_ref().unwrap_or(&step.pattern)
            } else {
                &step.pattern
            };
            let pattern = Self::build_pattern(pattern_str, &step.flags, settings)?;
            let replacement = step.replacement.clone();

            // Compile the not_pattern if specified
            let not_pattern = if let Some(ref not_pattern_str) = step.not_pattern {
                Some(Self::build_pattern(not_pattern_str, &step.flags, settings)?)
            } else {
                None
            };

            // Compile the end pattern for Block steps
            let end_pattern = if let Some(ref end_str) = step.end_pattern {
                Some(Self::build_pattern(end_str, &step.flags, settings)?)
            } else {
                None
            };

            // Compile content pattern for Block steps that filter by content
            // If start_pattern is specified, the main pattern field is used for content filtering
            let content_pattern = if matches!(step.step_type, StepType::Block)
                && step.start_pattern.is_some()
                && !step.pattern.is_empty()
            {
                Some(Self::build_pattern(&step.pattern, &step.flags, settings)?)
            } else {
                None
            };

            // Parse language(s) and scope for syntax-aware processing
            #[cfg(feature = "tree-sitter")]
            let (languages, scope_filter) = {
                use crate::syntax::{Language, ScopeFilter};
                use std::collections::HashSet;

                // Collect languages from either `language` (single) or `languages` (multiple)
                let langs: Option<Vec<Language>> = {
                    let mut collected = Vec::new();

                    // Add single language if specified
                    if let Some(ref lang_str) = step.language {
                        if let Ok(lang) = lang_str.parse::<Language>() {
                            collected.push(lang);
                        } else {
                            log::warn!(
                                "Step {}: unknown language '{}'; syntax-aware processing disabled for this language",
                                index + 1,
                                lang_str
                            );
                        }
                    }

                    // Add multiple languages if specified
                    if let Some(ref lang_strs) = step.languages {
                        for lang_str in lang_strs {
                            if let Ok(lang) = lang_str.parse::<Language>() {
                                if !collected.contains(&lang) {
                                    collected.push(lang);
                                }
                            } else {
                                log::warn!(
                                    "Step {}: unknown language '{}' in languages list",
                                    index + 1,
                                    lang_str
                                );
                            }
                        }
                    }

                    if collected.is_empty() {
                        None
                    } else {
                        Some(collected)
                    }
                };

                // Parse scope filter, considering exclude_scopes
                let scope = if let Some(ref exclude_scopes) = step.exclude_scopes {
                    // Convert exclude_scopes to a ScopeFilter::Exclude
                    let excluded: HashSet<String> = exclude_scopes.iter().cloned().collect();
                    Some(ScopeFilter::Exclude(excluded))
                } else {
                    step.scope
                        .as_ref()
                        .and_then(|s| s.parse::<ScopeFilter>().ok())
                };

                // Warn if scope is specified without any language
                if scope.is_some() && langs.is_none() {
                    log::warn!(
                        "Step {}: scope specified without language; syntax-aware processing disabled",
                        index + 1
                    );
                }
                (langs, scope)
            };

            // Resolve transform action (handles file-based keys/seeds)
            let transform_action = Self::resolve_transform_action(&step.transform)?;

            compiled_steps.push(CompiledStep {
                step_index: index,
                pattern,
                not_pattern,
                replacement,
                action: step.action.clone(),
                transform_action,
                step_type: step.step_type.clone(),
                is_global,
                end_pattern,
                content_pattern,
                block_context: step.block_context.clone(),
                on_mismatch: step.on_mismatch.clone().unwrap_or_default(),
                capture_names: step.capture_names.clone(),
                output_format: step.output_format.clone(),
                output_template: step.output_template.clone(),
                first_only: step.first_only.unwrap_or(false),
                deduplicate: step.deduplicate.unwrap_or(false),
                name: step.name.clone(),
                #[cfg(feature = "tree-sitter")]
                languages,
                #[cfg(feature = "tree-sitter")]
                scope_filter,
            });
        }

        Ok(compiled_steps)
    }

    /// Resolve transform action, loading keys/seeds from files if specified
    fn resolve_transform_action(
        action: &Option<TransformAction>,
    ) -> Result<Option<TransformAction>> {
        let action = match action {
            Some(a) => a,
            None => return Ok(None),
        };

        let resolved = match action {
            #[cfg(feature = "fpe")]
            TransformAction::FpeEncrypt {
                key,
                key_file,
                tweak,
                tweak_file,
                radix,
            } => {
                let resolved_key =
                    Self::resolve_secret(key.as_deref(), key_file.as_deref(), "key")?;
                let resolved_tweak = Self::resolve_secret(
                    if tweak.is_empty() {
                        None
                    } else {
                        Some(tweak.as_str())
                    },
                    tweak_file.as_deref(),
                    "tweak",
                )?;
                TransformAction::FpeEncrypt {
                    key: Some(resolved_key),
                    key_file: None,
                    tweak: resolved_tweak,
                    tweak_file: None,
                    radix: radix.clone(),
                }
            }
            #[cfg(feature = "fpe")]
            TransformAction::FpeDecrypt {
                key,
                key_file,
                tweak,
                tweak_file,
                radix,
            } => {
                let resolved_key =
                    Self::resolve_secret(key.as_deref(), key_file.as_deref(), "key")?;
                let resolved_tweak = Self::resolve_secret(
                    if tweak.is_empty() {
                        None
                    } else {
                        Some(tweak.as_str())
                    },
                    tweak_file.as_deref(),
                    "tweak",
                )?;
                TransformAction::FpeDecrypt {
                    key: Some(resolved_key),
                    key_file: None,
                    tweak: resolved_tweak,
                    tweak_file: None,
                    radix: radix.clone(),
                }
            }
            TransformAction::MaskDeterministic {
                seed,
                seed_file,
                preserve_prefix,
                preserve_suffix,
                mask_char,
            } => {
                let resolved_seed =
                    Self::resolve_secret(seed.as_deref(), seed_file.as_deref(), "seed")?;
                TransformAction::MaskDeterministic {
                    seed: Some(resolved_seed),
                    seed_file: None,
                    preserve_prefix: *preserve_prefix,
                    preserve_suffix: *preserve_suffix,
                    mask_char: *mask_char,
                }
            }
            // All other transform actions pass through unchanged
            other => other.clone(),
        };

        Ok(Some(resolved))
    }

    /// Resolve a secret value from either an inline value or a file path
    fn resolve_secret(
        inline_value: Option<&str>,
        file_path: Option<&str>,
        secret_name: &str,
    ) -> Result<String> {
        match (inline_value, file_path) {
            (Some(value), None) => Ok(value.to_string()),
            (None, Some(path)) => {
                let content = std::fs::read_to_string(path).with_context(|| {
                    format!("Failed to read {} from file: {}", secret_name, path)
                })?;
                Ok(content.trim().to_string())
            }
            (Some(_), Some(_)) => {
                anyhow::bail!(
                    "Both {} and {}_file specified; use only one",
                    secret_name,
                    secret_name
                )
            }
            (None, None) => {
                anyhow::bail!(
                    "Either {} or {}_file must be specified",
                    secret_name,
                    secret_name
                )
            }
        }
    }

    fn build_pattern(
        pattern: &str,
        flags: &Option<Vec<RegexFlag>>,
        settings: &PipelineSettings,
    ) -> Result<CompiledPattern> {
        // Fixed string mode - no regex interpretation
        if settings.fixed_strings {
            return Ok(CompiledPattern::Fixed(pattern.to_string()));
        }

        // Auto-detect if pattern could use fixed-string mode for better performance
        // This is purely informational - we still compile as regex for correctness
        if let Some(suggestion) = Self::check_could_use_fixed_string(pattern) {
            debug!("{}", suggestion);
        }

        // Check for zero-width match patterns that could cause unexpected behavior
        if let Some(warning) = Self::check_zero_width_pattern(pattern) {
            eprintln!("{}", warning);
        }

        // Report pattern complexity score for debugging/optimization
        let complexity = Self::calculate_pattern_complexity(pattern);
        if complexity.score > 50 {
            debug!(
                "Pattern complexity: {} ({})",
                complexity.score, complexity.explanation
            );
        }
        if complexity.score > 80 {
            eprintln!(
                "Warning: Complex pattern detected (score: {})\n  Pattern: {}\n  Issue: {}\n  Tip: {}",
                complexity.score,
                if pattern.len() > 60 {
                    format!("{}...", &pattern[..60])
                } else {
                    pattern.to_string()
                },
                complexity.explanation,
                complexity
                    .optimization_hint
                    .unwrap_or_else(|| "Consider simplifying the pattern".to_string())
            );
        }

        // Check for per-step PCRE flag in addition to global pcre_mode
        let use_pcre = settings.pcre_mode
            || flags
                .as_ref()
                .map(|f| f.iter().any(|flag| matches!(flag, RegexFlag::Pcre)))
                .unwrap_or(false);

        // PCRE mode - use fancy-regex for advanced features (global or per-step)
        #[cfg(feature = "pcre")]
        if use_pcre {
            // Check for ReDoS risks in PCRE mode (which uses backtracking)
            if let Some(warning) = Self::check_redos_risk(pattern, true) {
                if settings.strict_mode {
                    // In strict mode, reject potentially dangerous patterns
                    return Err(PatternError::potential_redos(
                        pattern,
                        warning.replace("ReDoS Warning:\n", ""),
                    ))
                    .context("Use --no-strict to allow potentially dangerous patterns");
                }
                eprintln!("{}", warning);
            }

            match FancyRegex::new(pattern) {
                Ok(re) => return Ok(CompiledPattern::Pcre(re)),
                Err(e) => {
                    return Err(PatternError::invalid_regex(pattern, e.to_string()))
                        .context("PCRE pattern compilation failed");
                }
            }
        }

        #[cfg(not(feature = "pcre"))]
        if use_pcre {
            return Err(PatternError::PcreNotEnabled).context(
                "Suggestion: Rebuild with `cargo build --features pcre` or remove the -P flag, or remove 'pcre' from step flags",
            );
        }

        // Standard regex mode
        match Self::build_regex(pattern, flags, settings.regex_size_limit) {
            Ok(regex) => Ok(CompiledPattern::Standard(regex)),
            Err(e) => Err(PatternError::invalid_regex(pattern, e.to_string()))
                .context("Regex pattern compilation failed"),
        }
    }

    /// Check if a pattern could use fixed-string mode for better performance.
    ///
    /// Fixed-string mode is faster because it uses literal string matching
    /// instead of regex compilation. This function detects patterns that
    /// don't use any regex metacharacters and could benefit from --fixed-strings.
    fn check_could_use_fixed_string(pattern: &str) -> Option<String> {
        // Regex metacharacters that indicate the pattern needs regex interpretation
        const REGEX_METACHARACTERS: &[char] = &[
            '.', '*', '+', '?', '^', '$', '[', ']', '(', ')', '{', '}', '|', '\\',
        ];

        // Check if pattern contains any regex metacharacters
        if !pattern.chars().any(|c| REGEX_METACHARACTERS.contains(&c)) {
            return Some(format!(
                "Performance hint: Pattern '{}' contains no regex metacharacters.\n  \
                 Consider using --fixed-strings (-F) for faster literal matching.",
                if pattern.len() > 40 {
                    format!("{}...", &pattern[..40])
                } else {
                    pattern.to_string()
                }
            ));
        }

        None
    }

    /// Calculate a complexity score for a regex pattern.
    ///
    /// This helps identify patterns that may be slow to compile or match.
    /// The score considers:
    /// - Pattern length
    /// - Nesting depth of groups
    /// - Use of quantifiers
    /// - Unicode character classes
    /// - Alternation complexity
    fn calculate_pattern_complexity(pattern: &str) -> PatternComplexity {
        let mut score: u32 = 0;
        let mut issues = Vec::new();
        let mut hint = None;

        // Length-based complexity (longer patterns = more complex)
        let length_score = (pattern.len() / 20).min(20) as u32;
        if length_score > 10 {
            issues.push("long pattern");
        }
        score += length_score;

        // Count nesting depth
        let mut max_depth: u32 = 0;
        let mut current_depth: u32 = 0;
        for c in pattern.chars() {
            match c {
                '(' | '[' => {
                    current_depth += 1;
                    max_depth = max_depth.max(current_depth);
                }
                ')' | ']' => {
                    current_depth = current_depth.saturating_sub(1);
                }
                _ => {}
            }
        }
        if max_depth > 3 {
            score += (max_depth - 3) * 10;
            issues.push("deeply nested groups");
            hint = Some("Flatten nested groups where possible".to_string());
        }

        // Count quantifiers
        let quantifier_count = pattern.matches('+').count()
            + pattern.matches('*').count()
            + pattern.matches('?').count()
            + pattern.matches('{').count();
        if quantifier_count > 5 {
            score += ((quantifier_count - 5) * 5) as u32;
            issues.push("many quantifiers");
        }

        // Check for Unicode character classes (can be slow)
        if pattern.contains("\\p{") || pattern.contains("\\P{") {
            score += 15;
            issues.push("Unicode character classes");
            hint = Some(
                "Unicode classes can be slow; consider ASCII alternatives if applicable"
                    .to_string(),
            );
        }

        // Check for complex alternations
        let alternation_count = pattern.matches('|').count();
        if alternation_count > 10 {
            score += ((alternation_count - 10) * 3) as u32;
            issues.push("many alternations");
            hint =
                Some("Consider using character classes instead of long alternations".to_string());
        }

        // Check for greedy quantifiers in succession (.*.*) - often indicates inefficient pattern
        if pattern.contains(".*.*") || pattern.contains(".+.+") {
            score += 20;
            issues.push("consecutive greedy quantifiers");
            hint = Some("Use non-greedy quantifiers (.*?) or be more specific".to_string());
        }

        // Cap score at 100
        let final_score = score.min(100) as u8;

        let explanation = if issues.is_empty() {
            "simple pattern".to_string()
        } else {
            issues.join(", ")
        };

        PatternComplexity {
            score: final_score,
            explanation,
            optimization_hint: hint,
        }
    }

    /// Maximum pattern length before warning (patterns longer than this may be slow)
    #[cfg(feature = "pcre")]
    const PATTERN_LENGTH_WARNING: usize = 1000;

    fn build_regex(
        pattern: &str,
        flags: &Option<Vec<RegexFlag>>,
        regex_size_limit: usize,
    ) -> Result<Regex, regex::Error> {
        let mut builder = RegexBuilder::new(pattern);

        // Apply ReDoS protection via size limits
        // The Rust regex crate already guarantees O(m * n) linear time matching,
        // but we add size limits to prevent compilation DoS attacks
        builder.size_limit(regex_size_limit);

        // Also limit DFA size to prevent memory exhaustion
        builder.dfa_size_limit(regex_size_limit);

        if let Some(flags) = flags {
            for flag in flags {
                match flag {
                    RegexFlag::Global => {} // Global is handled in processing, not compilation
                    RegexFlag::CaseInsensitive => {
                        builder.case_insensitive(true);
                    }
                    RegexFlag::Multiline => {
                        builder.multi_line(true);
                    }
                    RegexFlag::DotAll => {
                        builder.dot_matches_new_line(true);
                    }
                    RegexFlag::Unicode => {
                        builder.unicode(true);
                    }
                    RegexFlag::Extended => {
                        builder.ignore_whitespace(true);
                    }
                    RegexFlag::Pcre => {
                        // Pcre flag is handled at the build_pattern level to select
                        // the regex engine. If we reach here, it means we're building
                        // a standard regex (Pcre was not selected), so ignore this flag.
                    }
                }
            }
        }

        builder.build()
    }

    /// Check for zero-width match patterns that could produce unexpected results.
    ///
    /// Zero-width assertions (like `^`, `$`, `\b`, `(?=...)`, `(?!...)`) match positions
    /// rather than characters. While Rust's regex crate handles these safely (no infinite
    /// loops), users should be warned when using them in replacement contexts as they
    /// may produce unexpected output.
    ///
    /// Returns a warning message if the pattern is primarily zero-width.
    fn check_zero_width_pattern(pattern: &str) -> Option<String> {
        // Patterns that are purely positional assertions
        let pure_zero_width = [
            "^", "$", r"\b", r"\B", r"\A", r"\z", r"\Z", "^$", r"^\b", r"\b$",
        ];

        // Check for pure zero-width patterns
        if pure_zero_width.contains(&pattern) {
            return Some(format!(
                "Warning: Pattern '{}' is a zero-width assertion.\n  \
                 It matches positions, not characters. In replacement mode, this may insert\n  \
                 text at every position in the input.\n  \
                 Consider adding actual characters to match, e.g., '^(.*)' or '\\bword\\b'",
                pattern
            ));
        }

        // Check for patterns that can match empty strings
        let can_match_empty = [
            ".*", ".?", "\\s*", "\\S*", "\\d*", "\\D*", "\\w*", "\\W*", "[^a]*", "()*", "()?",
            "(?:)*", "(?:)?",
        ];

        for empty_pattern in &can_match_empty {
            if pattern == *empty_pattern {
                return Some(format!(
                    "Warning: Pattern '{}' can match empty strings.\n  \
                     This may produce unexpected results in replacement mode.\n  \
                     Consider using '+' instead of '*' or '?' to require at least one character.",
                    pattern
                ));
            }
        }

        // Check for lookahead/lookbehind only patterns
        if (pattern.starts_with("(?=")
            || pattern.starts_with("(?!")
            || pattern.starts_with("(?<=")
            || pattern.starts_with("(?<!"))
            && pattern.ends_with(")")
            && pattern.matches("(?").count() == 1
        {
            return Some(format!(
                "Warning: Pattern '{}' is a pure lookahead/lookbehind assertion.\n  \
                 It matches positions, not characters. In replacement mode, this will\n  \
                 insert text at matched positions without consuming any input.\n  \
                 Consider wrapping with a capture group to match actual text.",
                pattern
            ));
        }

        None
    }

    /// Check pattern for potential ReDoS vulnerabilities (primarily for PCRE mode)
    /// Returns a warning message if the pattern looks potentially dangerous
    #[cfg(feature = "pcre")]
    fn check_redos_risk(pattern: &str, is_pcre: bool) -> Option<String> {
        let mut warnings = Vec::new();

        // Check pattern length
        if pattern.len() > Self::PATTERN_LENGTH_WARNING {
            warnings.push(format!(
                "Pattern is {} characters long. Very long patterns may impact performance.",
                pattern.len()
            ));
        }

        // Check for nested quantifiers (common ReDoS pattern in backtracking engines)
        // Only relevant for PCRE mode since standard mode uses linear-time matching
        if is_pcre {
            // Patterns like (a+)+ or (a*)* or (a+)*
            if pattern.contains(")+)")
                || pattern.contains(")*)")
                || pattern.contains("+)+")
                || pattern.contains("*)*")
                || pattern.contains("+)*")
                || pattern.contains("*)+")
            {
                warnings.push(
                    "Pattern contains nested quantifiers which can cause exponential matching time in PCRE mode. \
                     Consider simplifying the pattern or using standard mode (-P flag removed).".to_string()
                );
            }

            // Patterns with overlapping alternations
            let quantifier_count = pattern.matches('+').count()
                + pattern.matches('*').count()
                + pattern.matches('?').count();
            let alternation_count = pattern.matches('|').count();

            if quantifier_count > 5 && alternation_count > 3 {
                warnings.push(
                    "Pattern has many quantifiers and alternations which may cause slow matching in PCRE mode.".to_string()
                );
            }
        }

        // Check for suspicious repetition patterns like a{10000}
        for cap in REPETITION_REGEX.captures_iter(pattern) {
            if let Some(num_str) = cap.get(1) {
                if let Ok(num) = num_str.as_str().parse::<u32>() {
                    if num > 10000 {
                        warnings.push(format!(
                            "Pattern contains very large repetition count ({{{}}}). This may cause memory issues.",
                            num
                        ));
                    }
                }
            }
        }

        if warnings.is_empty() {
            None
        } else {
            Some(format!("ReDoS Warning:\n  - {}", warnings.join("\n  - ")))
        }
    }

    /// Process an input stream through the pipeline, writing results to output.
    ///
    /// This is the primary entry point for stream processing. It reads the input
    /// line-by-line, applies all pipeline steps, and writes transformed output
    /// to the writer. Memory usage remains constant regardless of input size.
    ///
    /// # Arguments
    ///
    /// * `reader` - Any type implementing `BufRead` (file, stdin, string buffer)
    /// * `writer` - Any type implementing `Write` (file, stdout, vector)
    ///
    /// # Returns
    ///
    /// A `PipelineResult` containing:
    /// - Lines processed count
    /// - Matches found count
    /// - Transformations applied count
    /// - Per-step statistics
    /// - Any errors encountered
    ///
    /// # Example
    ///
    /// ```
    /// use rexpipe::pipeline::PipelineConfig;
    /// use rexpipe::processor::StreamProcessor;
    /// use std::io::Cursor;
    ///
    /// let config = PipelineConfig::from_inline_pattern(r"ERROR", None);
    /// let mut processor = StreamProcessor::new(config).unwrap();
    ///
    /// let input = Cursor::new("INFO: startup\nERROR: failed\nINFO: done\n");
    /// let mut output = Vec::new();
    /// let result = processor.process_stream(input, &mut output).unwrap();
    ///
    /// // Filter step keeps only lines matching "ERROR"
    /// let output_str = String::from_utf8(output).unwrap();
    /// assert!(output_str.contains("ERROR"));
    /// assert!(!output_str.contains("INFO"));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - I/O error occurs reading input or writing output
    /// - Line exceeds `max_line_length` with `max_line_action = Error`
    /// - Validation step fails (when configured to error on mismatch)
    pub fn process_stream<R: BufRead, W: Write>(
        &mut self,
        mut reader: R,
        mut writer: W,
    ) -> Result<PipelineResult> {
        self.stats.processing_start = Some(Instant::now());
        let mut result = PipelineResult::new();
        let mut line_buffer = String::new();
        let mut line_number = 0u64;

        // Reset context state
        self.context_before_buffer.clear();
        self.after_context_remaining = 0;
        self.last_output_line = 0;

        let context_before = self.config.settings.context_before;
        let context_after = self.config.settings.context_after;
        let use_context = self.has_context();
        let preserve_line_endings = self.config.settings.preserve_line_endings;

        let max_line_length = self.config.settings.max_line_length;
        let max_line_action = self.config.settings.max_line_action;
        let sample_limit = self.config.settings.sample_limit;
        let strip_ansi = self.config.settings.strip_ansi;

        // Check if we should suppress normal output (finalize-only mode)
        let suppress_output = self.config.finalize.suppress_output;

        while reader.read_line(&mut line_buffer)? > 0 {
            line_number += 1;

            // Stop processing if sample limit reached
            if sample_limit > 0 && line_number > sample_limit {
                break;
            }

            self.stats.lines_read += 1;
            self.stats.bytes_processed += line_buffer.len() as u64;

            // Update finalize counters with original line content (before processing)
            if let Some(ref mut finalize_state) = self.finalize_state {
                finalize_state.process_line(&line_buffer);
            }

            // Strip ANSI escape sequences if configured
            if strip_ansi {
                let stripped = strip_ansi_codes(&line_buffer);
                // Only reallocate if there were ANSI codes to strip
                if let std::borrow::Cow::Owned(s) = stripped {
                    line_buffer.clear();
                    line_buffer.push_str(&s);
                }
            }

            // Check for lines exceeding the maximum length
            if max_line_length > 0 && line_buffer.len() > max_line_length {
                match handle_long_line(
                    &mut line_buffer,
                    line_number,
                    max_line_length,
                    max_line_action,
                ) {
                    LongLineResult::Error(msg) => {
                        return Err(anyhow::anyhow!(msg));
                    }
                    LongLineResult::Skip => {
                        // Output the original line unchanged
                        writer.write_all(line_buffer.as_bytes())?;
                        line_buffer.clear();
                        continue;
                    }
                    LongLineResult::Truncated => {
                        // Line was truncated in place, continue processing
                    }
                }
            }

            // Detect original line ending style for this line
            let line_ending = if preserve_line_endings {
                detect_line_ending(&line_buffer)
            } else {
                LineEnding::Lf
            };

            let processed_line = self.process_line(&line_buffer, line_number, &mut result)?;
            // Strip both \r\n and \n when extracting content
            let line_content = line_buffer.trim_end_matches(['\r', '\n']).to_string();

            if use_context && !suppress_output {
                // Handle context-aware output (unless suppressed for finalize-only mode)
                if let Some(output) = processed_line {
                    // This line matched - output before-context, then this line
                    // Output before-context lines that haven't been output yet
                    for ctx_line in self.context_before_buffer.iter() {
                        if ctx_line.line_number > self.last_output_line {
                            self.write_context_line(&mut writer, ctx_line)?;
                            self.last_output_line = ctx_line.line_number;
                        }
                    }

                    // Output the matching line
                    if line_number > self.last_output_line {
                        writer.write_all(output.as_bytes())?;
                        self.write_line_ending(&mut writer, &output, line_ending)?;
                        self.last_output_line = line_number;
                    }

                    // Reset after-context counter
                    self.after_context_remaining = context_after;
                } else if self.after_context_remaining > 0 {
                    // No match, but we're in after-context mode
                    if line_number > self.last_output_line {
                        writer.write_all(line_content.as_bytes())?;
                        writer.write_all(line_ending.as_bytes())?;
                        self.last_output_line = line_number;
                    }
                    self.after_context_remaining -= 1;
                }

                // Update before-context buffer
                self.context_before_buffer.push_back(ContextLine {
                    line_number,
                    content: line_content,
                    line_ending,
                });

                // Keep only the needed number of before-context lines
                while self.context_before_buffer.len() > context_before {
                    self.context_before_buffer.pop_front();
                }
            } else if use_context {
                // Context mode but output suppressed - still track context for finalize processing
                self.context_before_buffer.push_back(ContextLine {
                    line_number,
                    content: line_content,
                    line_ending,
                });
                while self.context_before_buffer.len() > context_before {
                    self.context_before_buffer.pop_front();
                }
            } else {
                // No context - simple output (unless suppressed for finalize-only mode)
                if !suppress_output {
                    if let Some(output) = processed_line {
                        writer.write_all(output.as_bytes())?;
                        self.write_line_ending(&mut writer, &output, line_ending)?;
                    }
                }
            }

            line_buffer.clear();
        }

        result.lines_processed = line_number;

        // Update finalize state with final match/transformation counts
        if let Some(ref mut finalize_state) = self.finalize_state {
            finalize_state.total_matches = result.matches_found;
            finalize_state.total_transformations = result.transformations_applied;
        }

        // Output finalize section if configured
        self.write_finalize_output(&mut writer)?;

        // Save bidirectional mappings if enabled
        if let Some(ref mut manager) = self.bidirectional_manager {
            if let Err(e) = manager.save_if_modified() {
                log::warn!("Failed to save bidirectional mappings: {}", e);
            }
        }

        debug!(
            "Processing complete: {} lines, {} matches, {} transformations",
            result.lines_processed, result.matches_found, result.transformations_applied
        );

        Ok(result)
    }

    /// Write finalize output after all lines have been processed
    fn write_finalize_output<W: Write>(&self, writer: &mut W) -> Result<()> {
        if !self.config.finalize.is_configured() {
            return Ok(());
        }

        let finalize_state = match &self.finalize_state {
            Some(state) => state,
            None => return Ok(()),
        };

        // Determine output format
        let output_format = self
            .config
            .finalize
            .output_format
            .as_ref()
            .cloned()
            .unwrap_or_default();

        match output_format {
            crate::pipeline::FinalizeOutputFormat::Json => {
                // Output as JSON
                let json = finalize_state.to_json();
                writeln!(writer, "{}", serde_json::to_string_pretty(&json)?)?;
            }
            crate::pipeline::FinalizeOutputFormat::Text => {
                // Render template if provided
                if let Some(ref template) = self.config.finalize.template {
                    let output = finalize_state.render_template(template);
                    write!(writer, "{}", output)?;
                    // Ensure trailing newline
                    if !output.ends_with('\n') {
                        writeln!(writer)?;
                    }
                }
            }
        }

        // Execute shell command if configured
        if let Some(ref shell_cmd) = self.config.finalize.shell {
            if self.config.settings.allow_shell {
                // Execute shell command with finalize state as JSON input
                let json_input = serde_json::to_string(&finalize_state.to_json())?;
                match crate::plugin::PluginRegistry::execute_shell_with_timeout(
                    shell_cmd,
                    &json_input,
                    self.config.settings.shell_timeout_secs,
                ) {
                    Ok(output) => {
                        write!(writer, "{}", output)?;
                        if !output.ends_with('\n') && !output.is_empty() {
                            writeln!(writer)?;
                        }
                    }
                    Err(e) => {
                        debug!("Finalize shell command failed: {}", e);
                        // Write error to output so user knows something went wrong
                        writeln!(writer, "[finalize shell error: {}]", e)?;
                    }
                }
            } else {
                debug!(
                    "Finalize shell command skipped: allow_shell is false. \
                     Set settings.allow_shell = true to enable."
                );
            }
        }

        Ok(())
    }

    /// Write a context line with its preserved line ending
    fn write_context_line<W: Write>(&self, writer: &mut W, ctx_line: &ContextLine) -> Result<()> {
        writer.write_all(ctx_line.content.as_bytes())?;
        writer.write_all(ctx_line.line_ending.as_bytes())?;
        Ok(())
    }

    /// Write line ending, respecting the output content and original line ending
    fn write_line_ending<W: Write>(
        &self,
        writer: &mut W,
        output: &str,
        line_ending: LineEnding,
    ) -> Result<()> {
        // Don't add line ending if output already has one
        if output.ends_with('\n') || output.ends_with("\r\n") {
            return Ok(());
        }
        // Use the original line ending style
        writer.write_all(line_ending.as_bytes())?;
        Ok(())
    }

    fn process_line(
        &mut self,
        line: &str,
        line_number: u64,
        result: &mut PipelineResult,
    ) -> Result<Option<String>> {
        trace!("Processing line {}: {:?}", line_number, line);
        // Strip both CRLF and LF line endings
        let mut current_line = line.trim_end_matches(['\r', '\n']).to_string();
        let mut should_output = true;
        let line_start = Instant::now();
        let timeout_ms = self.config.settings.timeout_ms;

        for step_idx in 0..self.compiled_steps.len() {
            // Check timeout if configured (0 = no timeout)
            if timeout_ms > 0 && line_start.elapsed().as_millis() as u64 > timeout_ms {
                return Err(anyhow::anyhow!(
                    "Processing timeout ({} ms) exceeded at line {}",
                    timeout_ms,
                    line_number
                ));
            }

            // Extract values we need before mutably borrowing self
            let step_type = self.compiled_steps[step_idx].step_type.clone();
            let step_index = self.compiled_steps[step_idx].step_index;
            let is_global = self.compiled_steps[step_idx].is_global;
            let replacement = self.compiled_steps[step_idx].replacement.clone();
            let pattern_debug = format!("{:?}", self.compiled_steps[step_idx].pattern);
            let step_name = self.compiled_steps[step_idx].name.clone();

            let step_start = Instant::now();
            let mut step_result =
                StepResult::new(step_index, step_type.clone(), pattern_debug, step_name);

            match step_type {
                StepType::Substitute => {
                    if let Some(replacement_str) = replacement {
                        // Clone the pattern for use in apply_substitution
                        let pattern = self.compiled_steps[step_idx].pattern.clone();
                        let (result, was_modified) = self.apply_substitution(
                            &pattern,
                            &current_line,
                            &replacement_str,
                            is_global,
                            step_index,
                            &mut step_result,
                        )?;

                        if was_modified {
                            current_line = result;
                            step_result.add_transformation();
                        }
                    }
                }
                StepType::Filter => {
                    // Re-borrow compiled_step for non-mutable operations
                    let compiled_step = &self.compiled_steps[step_idx];
                    let raw_matches = compiled_step.pattern.is_match(&current_line);

                    // Check not_pattern: if it matches, negate the result
                    // This allows patterns like: pattern = "ERROR", not_pattern = "expected"
                    // to keep ERROR lines but exclude those containing "expected"
                    let matches_after_negation = if raw_matches {
                        // Only check not_pattern if the main pattern matched
                        if let Some(ref not_pattern) = compiled_step.not_pattern {
                            let negation_matches = not_pattern.is_match(&current_line);
                            !negation_matches // If not_pattern matches, treat as no match
                        } else {
                            true // No not_pattern, keep the match
                        }
                    } else {
                        false
                    };

                    // Apply invert_match setting (like grep -v)
                    // When inverted: matching lines are dropped, non-matching lines are kept
                    let matches = if self.config.settings.invert_match {
                        !matches_after_negation
                    } else {
                        matches_after_negation
                    };

                    if raw_matches {
                        step_result.add_match();
                    }

                    // Clone action to avoid borrow conflict with self when mutating dedup state
                    let action_clone = compiled_step.action.clone();

                    if let Some(action) = action_clone {
                        should_output = match action {
                            StepAction::KeepLine => matches,
                            StepAction::DropLine => !matches,
                            StepAction::KeepMatch => matches,
                            StepAction::DropMatch => !matches,
                            StepAction::DeduplicateByPrefix => {
                                // Deduplicate by prefix: extract prefix from first capture group
                                // Pattern like "^(.{50}).*$" captures first 50 chars as prefix
                                let prefix = if let Some(cap) =
                                    compiled_step.pattern.captures_iter(&current_line).first()
                                {
                                    // Use first capture group as prefix
                                    cap.groups
                                        .get(1)
                                        .and_then(|g| g.clone())
                                        .unwrap_or_else(|| current_line.clone())
                                } else {
                                    // If no match, use the whole line as prefix
                                    current_line.clone()
                                };

                                // Get or create the set for this step
                                let seen_set = self.dedup_prefix_seen.entry(step_idx).or_default();

                                // If we've seen this prefix before, drop the line
                                if seen_set.contains(&prefix) {
                                    false
                                } else {
                                    seen_set.insert(prefix);
                                    true
                                }
                            }
                            // Block actions don't apply to Filter steps
                            _ => matches,
                        };

                        if !should_output {
                            // Format step identifier with name if available
                            let step_id = match &compiled_step.name {
                                Some(name) => format!("'{}' (step {})", name, step_idx + 1),
                                None => format!("step {}", step_idx + 1),
                            };
                            // Log step-level attribution for dropped lines
                            trace!(
                                "Line {} DROPPED by {} ({:?}, pattern: {})",
                                line_number,
                                step_id,
                                action,
                                compiled_step.pattern.pattern_str()
                            );
                            // Output dropped line to stderr if debugging is enabled
                            if self.show_dropped {
                                eprintln!(
                                    "[DROPPED line {}] {}: {}",
                                    line_number, step_id, current_line
                                );
                            }
                            // Record the drop in step statistics
                            step_result.add_dropped();
                            let elapsed = step_start.elapsed().as_millis() as u64;
                            step_result.set_processing_time(elapsed);
                            self.stats.step_timings.insert(step_index, elapsed);
                            result.add_step_result(step_result);
                            break;
                        }
                    }
                }
                StepType::Extract => {
                    // Re-borrow compiled_step for non-mutable operations
                    let compiled_step = &self.compiled_steps[step_idx];
                    // Extract all matched content with capture group support
                    let captures: Vec<CaptureGroup> =
                        compiled_step.pattern.captures_iter(&current_line);

                    if captures.is_empty() {
                        // No matches in extract mode - drop the line from output
                        should_output = false;
                        step_result.add_dropped();
                        let elapsed = step_start.elapsed().as_millis() as u64;
                        step_result.set_processing_time(elapsed);
                        self.stats.step_timings.insert(step_index, elapsed);
                        result.add_step_result(step_result);
                        break;
                    } else {
                        // Apply first_only if specified
                        let captures_to_process: Vec<&CaptureGroup> = if compiled_step.first_only {
                            captures.iter().take(1).collect()
                        } else if compiled_step.is_global {
                            captures.iter().collect()
                        } else {
                            captures.iter().take(1).collect()
                        };

                        for _cap in &captures_to_process {
                            step_result.add_match();
                        }

                        // Format the output based on output_format and capture_names
                        use crate::pipeline::ExtractOutputFormat;

                        let output = match &compiled_step.output_format {
                            Some(ExtractOutputFormat::Json) => {
                                // Output as JSON array of objects with capture group names
                                let results: Vec<serde_json::Value> = captures_to_process
                                    .iter()
                                    .map(|cap| {
                                        let mut obj = serde_json::Map::new();
                                        if let Some(ref names) = compiled_step.capture_names {
                                            // Use provided names for capture groups
                                            for (i, name) in names.iter().enumerate() {
                                                let group_idx = i + 1; // Skip group 0 (full match)
                                                if let Some(Some(val)) = cap.groups.get(group_idx) {
                                                    obj.insert(
                                                        name.clone(),
                                                        serde_json::Value::String(val.clone()),
                                                    );
                                                }
                                            }
                                        } else {
                                            // Use numeric indices
                                            for (i, group) in cap.groups.iter().enumerate() {
                                                if let Some(val) = group {
                                                    obj.insert(
                                                        format!("group_{}", i),
                                                        serde_json::Value::String(val.clone()),
                                                    );
                                                }
                                            }
                                        }
                                        serde_json::Value::Object(obj)
                                    })
                                    .collect();
                                serde_json::to_string(&results).unwrap_or_default()
                            }
                            Some(ExtractOutputFormat::Jsonl) => {
                                // Output as JSON Lines (one JSON object per match)
                                captures_to_process
                                    .iter()
                                    .filter_map(|cap| {
                                        let mut obj = serde_json::Map::new();
                                        if let Some(ref names) = compiled_step.capture_names {
                                            for (i, name) in names.iter().enumerate() {
                                                let group_idx = i + 1;
                                                if let Some(Some(val)) = cap.groups.get(group_idx) {
                                                    obj.insert(
                                                        name.clone(),
                                                        serde_json::Value::String(val.clone()),
                                                    );
                                                }
                                            }
                                        } else {
                                            for (i, group) in cap.groups.iter().enumerate() {
                                                if let Some(val) = group {
                                                    obj.insert(
                                                        format!("group_{}", i),
                                                        serde_json::Value::String(val.clone()),
                                                    );
                                                }
                                            }
                                        }
                                        serde_json::to_string(&serde_json::Value::Object(obj)).ok()
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            }
                            Some(ExtractOutputFormat::Csv) => {
                                // Output as CSV rows
                                let mut lines = Vec::new();

                                // Add header row only on first output for this step
                                let header_written =
                                    self.csv_header_written.entry(step_idx).or_insert(false);
                                if !*header_written {
                                    if let Some(ref names) = compiled_step.capture_names {
                                        lines.push(names.join(","));
                                    }
                                    *header_written = true;
                                }

                                for cap in &captures_to_process {
                                    let values: Vec<String> =
                                        if let Some(ref names) = compiled_step.capture_names {
                                            names
                                                .iter()
                                                .enumerate()
                                                .map(|(i, _)| {
                                                    let group_idx = i + 1;
                                                    cap.groups
                                                        .get(group_idx)
                                                        .and_then(|g| g.clone())
                                                        .map(|v| {
                                                            // CSV escape: quote if contains comma, quote, or newline
                                                            if v.contains(',')
                                                                || v.contains('"')
                                                                || v.contains('\n')
                                                            {
                                                                format!(
                                                                    "\"{}\"",
                                                                    v.replace('"', "\"\"")
                                                                )
                                                            } else {
                                                                v
                                                            }
                                                        })
                                                        .unwrap_or_default()
                                                })
                                                .collect()
                                        } else {
                                            cap.groups
                                                .iter()
                                                .skip(1)
                                                .map(|g| {
                                                    g.clone()
                                                        .map(|v| {
                                                            if v.contains(',')
                                                                || v.contains('"')
                                                                || v.contains('\n')
                                                            {
                                                                format!(
                                                                    "\"{}\"",
                                                                    v.replace('"', "\"\"")
                                                                )
                                                            } else {
                                                                v
                                                            }
                                                        })
                                                        .unwrap_or_default()
                                                })
                                                .collect()
                                        };
                                    lines.push(values.join(","));
                                }
                                lines.join("\n")
                            }
                            Some(ExtractOutputFormat::Text) | None => {
                                // Check for output_template
                                if let Some(ref template) = compiled_step.output_template {
                                    // Apply template with capture group substitution
                                    captures_to_process
                                        .iter()
                                        .map(|cap| {
                                            let mut result = template.clone();
                                            // Replace $0, $1, $2, etc. with capture groups
                                            for (i, group) in cap.groups.iter().enumerate() {
                                                if let Some(val) = group {
                                                    result =
                                                        result.replace(&format!("${}", i), val);
                                                    // Also support ${name} format if capture_names provided
                                                    if let Some(ref names) =
                                                        compiled_step.capture_names
                                                    {
                                                        if i > 0 && i <= names.len() {
                                                            result = result.replace(
                                                                &format!("${{{}}}", names[i - 1]),
                                                                val,
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                            result
                                        })
                                        .collect::<Vec<_>>()
                                        .join("\n")
                                } else {
                                    // Default: extract full matches, join with separator
                                    let matches: Vec<String> = captures_to_process
                                        .iter()
                                        .filter_map(|cap| {
                                            cap.full_match.as_ref().map(|(_, _, m)| m.clone())
                                        })
                                        .collect();
                                    let separator =
                                        compiled_step.replacement.as_deref().unwrap_or("\t");
                                    matches.join(separator)
                                }
                            }
                        };

                        // Apply cross-line deduplication if requested
                        let final_output = if compiled_step.deduplicate {
                            // Get or create the seen set for this step
                            let seen_set = self.dedup_extract_seen.entry(step_idx).or_default();

                            // Filter out lines we've already seen across the entire stream
                            let lines: Vec<&str> = output.split('\n').collect();
                            let unique: Vec<&str> = lines
                                .into_iter()
                                .filter(|line| {
                                    if line.is_empty() {
                                        true // Always keep empty lines (formatting)
                                    } else if seen_set.contains(*line) {
                                        false // Skip duplicate
                                    } else {
                                        seen_set.insert(line.to_string());
                                        true // Keep new value
                                    }
                                })
                                .collect();

                            if unique.is_empty() {
                                // All values were duplicates, skip this output entirely
                                should_output = false;
                                String::new()
                            } else {
                                unique.join("\n")
                            }
                        } else {
                            output
                        };

                        current_line = final_output;
                        step_result.add_transformation();
                    }
                }
                StepType::Validate => {
                    // Re-borrow compiled_step for non-mutable operations
                    let compiled_step = &self.compiled_steps[step_idx];
                    let is_valid = compiled_step.pattern.is_match(&current_line);
                    if !is_valid {
                        // Handle based on on_mismatch setting
                        match compiled_step.on_mismatch {
                            OnMismatch::Error => {
                                result.add_error(
                                    PipelineError::new(
                                        compiled_step.step_index,
                                        line_number,
                                        ErrorType::PatternMatch,
                                        "Line failed validation".to_string(),
                                    )
                                    .with_context(current_line.clone()),
                                );
                                should_output = false;
                                break;
                            }
                            OnMismatch::Warn => {
                                log::warn!(
                                    "Step {}: Line {} failed validation: {}",
                                    compiled_step.step_index + 1,
                                    line_number,
                                    current_line.trim()
                                );
                                // Continue processing, still output the line
                            }
                            OnMismatch::Skip => {
                                // Skip the line silently
                                should_output = false;
                                break;
                            }
                        }
                    }
                }
                StepType::Transform => {
                    // Re-borrow compiled_step for non-mutable operations
                    let compiled_step = &self.compiled_steps[step_idx];
                    // Apply transformation to matched text
                    if let Some(ref action) = compiled_step.transform_action {
                        let (result, was_modified) = self.apply_transform(
                            &compiled_step.pattern,
                            &current_line,
                            action,
                            compiled_step.is_global,
                            &compiled_step.replacement,
                            &mut step_result,
                        )?;

                        if was_modified {
                            current_line = result;
                            step_result.add_transformation();
                        }
                    } else {
                        // No transform action specified, just check if pattern matches
                        if compiled_step.pattern.is_match(&current_line) {
                            step_result.add_match();
                        }
                    }
                }
                StepType::Block => {
                    // Re-borrow compiled_step for non-mutable operations
                    let compiled_step = &self.compiled_steps[step_idx];
                    // Cross-line state machine: track blocks between start and end patterns
                    let is_in_block = self.block_states[step_idx];

                    // Check for block boundaries
                    let start_matches = compiled_step.pattern.is_match(&current_line);
                    let end_matches = compiled_step
                        .end_pattern
                        .as_ref()
                        .map(|p| p.is_match(&current_line))
                        .unwrap_or(false);

                    // Check if this step has content filtering
                    let has_content_filter = compiled_step.content_pattern.is_some();

                    // Convert StepAction to BlockAction for block-specific handling
                    let block_action_clone = compiled_step
                        .action
                        .as_ref()
                        .and_then(BlockAction::from_step_action);
                    let block_context_clone = compiled_step.block_context.clone();

                    // State transitions
                    let entering_block = !is_in_block && start_matches;
                    let exiting_block = is_in_block && end_matches;

                    if entering_block {
                        // Enter block on start pattern
                        self.block_states[step_idx] = true;
                        step_result.add_match();

                        // Initialize dedup buffer if this is a Deduplicate block
                        if matches!(block_action_clone, Some(BlockAction::Deduplicate)) {
                            self.dedup_block_buffer.insert(step_idx, Vec::new());
                        }

                        // Initialize content filter buffer if content filtering is enabled
                        if has_content_filter {
                            self.block_content_buffer.insert(step_idx, Vec::new());
                            self.block_content_matched.insert(step_idx, false);
                        }

                        // Handle block context overlap on block entry
                        if let Some(ref _ctx) = block_context_clone {
                            if let Some(overlap) = self.block_overlap_buffer.remove(&step_idx) {
                                // Prepend overlap from previous block
                                current_line = format!("{}{}", overlap, current_line);
                                step_result.add_transformation();
                            }
                        }
                    } else if exiting_block {
                        // Exit block on end pattern
                        self.block_states[step_idx] = false;

                        // Handle content-filtered block completion
                        if has_content_filter {
                            let content_matched = self
                                .block_content_matched
                                .remove(&step_idx)
                                .unwrap_or(false);
                            let mut buffered_lines = self
                                .block_content_buffer
                                .remove(&step_idx)
                                .unwrap_or_default();

                            // Determine if block should be output based on action and content match
                            let output_block = match &block_action_clone {
                                Some(BlockAction::KeepBlock) => content_matched, // Keep only if content matched
                                Some(BlockAction::DropBlock) => !content_matched, // Drop if content matched
                                _ => true, // For other actions, always output
                            };

                            if output_block {
                                // Add the end line to buffered lines
                                buffered_lines.push(current_line.clone());
                                // Join all buffered lines with newlines and set as current_line
                                // This will be output through the normal output path
                                current_line = buffered_lines.join("\n");
                                // Skip the normal block action processing since we've handled it
                                // (continue to next step in pipeline or output)
                            } else {
                                // Don't output this block
                                should_output = false;
                                break;
                            }
                            // Skip normal block action processing for content-filtered blocks
                            continue;
                        }

                        // Handle block context overlap on block exit
                        if let Some(ref ctx) = block_context_clone {
                            use crate::pipeline::BlockContextValue;
                            let overlap_chars = match ctx {
                                BlockContextValue::Lines(_) => None,
                                BlockContextValue::Config(config) => config.overlap_chars,
                            };
                            let overlap_lines = match ctx {
                                BlockContextValue::Lines(_) => None,
                                BlockContextValue::Config(config) => config.overlap_lines,
                            };

                            // Save trailing content for next block
                            if let Some(n) = overlap_chars {
                                // Save last N characters
                                let chars: Vec<char> = current_line.chars().collect();
                                let start = chars.len().saturating_sub(n);
                                let overlap: String = chars[start..].iter().collect();
                                self.block_overlap_buffer.insert(step_idx, overlap);
                            } else if let Some(_n) = overlap_lines {
                                // For line-based overlap, we'd need to track previous lines
                                // This is more complex as it requires buffering multiple lines
                                // For now, save the entire current line as a simple implementation
                                self.block_overlap_buffer
                                    .insert(step_idx, current_line.clone());
                            }
                        }
                    }

                    // For content-filtered blocks, buffer lines and check for content match
                    if has_content_filter && (is_in_block || entering_block) && !exiting_block {
                        // Check if this line matches the content pattern
                        if let Some(ref content_pat) = compiled_step.content_pattern {
                            if content_pat.is_match(&current_line) {
                                self.block_content_matched.insert(step_idx, true);
                            }
                        }
                        // Buffer the line
                        if let Some(buffer) = self.block_content_buffer.get_mut(&step_idx) {
                            buffer.push(current_line.clone());
                        }
                        // Don't output yet - wait until block ends to decide
                        should_output = false;
                        break;
                    }

                    // Apply block action if we're inside the block (including start/end lines)
                    let process_line = is_in_block || start_matches;
                    if process_line {
                        if let Some(ref action) = block_action_clone {
                            match action {
                                BlockAction::KeepBlock => {
                                    // Lines outside blocks are dropped
                                    // (already inside block, so keep this line)
                                }
                                BlockAction::DropBlock => {
                                    // Drop lines inside blocks
                                    should_output = false;
                                    break;
                                }
                                BlockAction::MarkBlock { marker } => {
                                    // Prepend marker to lines in block
                                    current_line = format!("{}{}", marker, current_line);
                                    step_result.add_transformation();
                                }
                                BlockAction::SubstituteInBlock {
                                    pattern,
                                    replacement,
                                } => {
                                    // Apply substitution only within block
                                    if let Ok(sub_pattern) = regex::Regex::new(pattern) {
                                        let new_line = sub_pattern
                                            .replace_all(&current_line, replacement.as_str());
                                        if new_line != current_line {
                                            current_line = new_line.to_string();
                                            step_result.add_transformation();
                                        }
                                    }
                                }
                                BlockAction::CollectBlock => {
                                    // For CollectBlock, we'd need to buffer lines
                                    // and output them together. For now, just mark them.
                                    // Future: implement block collection buffer
                                    step_result.add_match();
                                }
                                BlockAction::Deduplicate => {
                                    // Buffer line for block-level deduplication
                                    if let Some(buffer) = self.dedup_block_buffer.get_mut(&step_idx)
                                    {
                                        buffer.push(current_line.clone());
                                    }
                                    step_result.add_match();

                                    // Suppress output during buffering - will be handled on block exit
                                    should_output = false;
                                }
                            }
                        }
                    } else if matches!(block_action_clone, Some(BlockAction::KeepBlock)) {
                        // KeepBlock: drop lines outside blocks
                        should_output = false;
                        break;
                    }

                    // Handle block exit for Deduplicate action
                    if exiting_block && matches!(block_action_clone, Some(BlockAction::Deduplicate))
                    {
                        // Add the until line to buffer before processing
                        if let Some(buffer) = self.dedup_block_buffer.get_mut(&step_idx) {
                            buffer.push(current_line.clone());
                        }

                        // Hash the block content
                        if let Some(buffer) = self.dedup_block_buffer.remove(&step_idx) {
                            use std::hash::{Hash, Hasher};
                            let mut hasher = std::collections::hash_map::DefaultHasher::new();
                            buffer.hash(&mut hasher);
                            let block_hash = hasher.finish();

                            // Check if we've seen this block before
                            let seen_set = self.dedup_block_seen.entry(step_idx).or_default();

                            if !seen_set.contains(&block_hash) {
                                // New unique block - output all buffered lines
                                seen_set.insert(block_hash);
                                // Return the entire block as the current line (joined with newlines)
                                current_line = buffer.join("\n");
                                should_output = true;
                            } else {
                                // Duplicate block - suppress output
                                should_output = false;
                            }
                        }
                    }
                }
            }

            let elapsed = step_start.elapsed().as_millis() as u64;
            step_result.set_processing_time(elapsed);
            self.stats.step_timings.insert(step_index, elapsed);
            result.add_step_result(step_result);
        }

        if should_output {
            result.add_output_line();
            Ok(Some(current_line))
        } else {
            Ok(None)
        }
    }

    /// Applies substitution and returns (result, was_modified) to avoid cloning for comparison.
    ///
    /// This method handles:
    /// - Standard regex replacements with capture group expansion
    /// - Variable expansion: `${seq}` (per-step sequence) and `${count}` (global match count)
    /// - Bidirectional mapping recording (if enabled)
    fn apply_substitution(
        &mut self,
        pattern: &CompiledPattern,
        input: &str,
        replacement: &str,
        is_global: bool,
        step_index: usize,
        step_result: &mut StepResult,
    ) -> Result<(String, bool)> {
        // Check if we need variable expansion
        let needs_var_expansion =
            replacement.contains("${seq}") || replacement.contains("${count}");

        // Check if bidirectional recording is needed
        let record_mappings = self
            .bidirectional_manager
            .as_ref()
            .is_some_and(|m| m.is_enabled());

        if !needs_var_expansion && !record_mappings {
            // Fast path: no variable expansion or mapping needed
            if is_global {
                let (result, match_count) = pattern.replace_all_counting(input, replacement);
                for _ in 0..match_count {
                    step_result.add_match();
                }
                Ok((result, match_count > 0))
            } else {
                let (result, had_match) = pattern.replace_counting(input, replacement);
                if had_match {
                    step_result.add_match();
                }
                Ok((result, had_match))
            }
        } else {
            // Slow path: handle variable expansion and/or bidirectional mapping
            let ctx = SubstitutionContext {
                pattern,
                input,
                replacement,
                is_global,
                step_index,
                record_mappings,
            };
            self.apply_substitution_with_vars(ctx, step_result)
        }
    }

    /// Applies substitution with variable expansion and optional bidirectional mapping.
    ///
    /// Uses [`SubstitutionContext`] to group related parameters and improve readability.
    fn apply_substitution_with_vars(
        &mut self,
        ctx: SubstitutionContext<'_>,
        step_result: &mut StepResult,
    ) -> Result<(String, bool)> {
        // Collect all matches first (we need to process them with mutable state)
        let matches: Vec<_> = ctx.pattern.find_iter(ctx.input);

        if matches.is_empty() {
            return Ok((ctx.input.to_string(), false));
        }

        // Only process first match if not global
        let matches_to_process = if ctx.is_global {
            matches
        } else {
            vec![
                matches
                    .into_iter()
                    .next()
                    .expect("matches verified non-empty above"),
            ]
        };

        let mut result = String::new();
        let mut last_end = 0;
        let pattern_str = ctx.pattern.pattern_str();

        for (start, end, matched_text) in matches_to_process {
            // Append text before match
            result.push_str(&ctx.input[last_end..start]);

            // Increment sequence counter for this step
            let seq = self.seq_counters.entry(ctx.step_index).or_insert(0);
            *seq += 1;
            let seq_val = *seq;

            // Increment global match count
            self.global_match_count += 1;
            let count_val = self.global_match_count;

            // Expand variables in replacement
            let expanded = ctx
                .replacement
                .replace("${seq}", &seq_val.to_string())
                .replace("${count}", &count_val.to_string());

            // Expand capture groups using the pattern
            let final_replacement = ctx
                .pattern
                .expand_captures(ctx.input, start, end, &expanded);

            // Record bidirectional mapping if enabled
            if ctx.record_mappings {
                if let Some(ref mut manager) = self.bidirectional_manager {
                    manager.record_mapping(
                        &matched_text,
                        &final_replacement,
                        ctx.step_index,
                        pattern_str,
                    );
                }
            }

            result.push_str(&final_replacement);
            last_end = end;
            step_result.add_match();
        }

        // Append remaining text
        result.push_str(&ctx.input[last_end..]);

        Ok((result, true))
    }

    /// Apply a single transform action to matched text.
    ///
    /// This is extracted from apply_transform for clarity. Each action defines
    /// how to transform the matched substring.
    fn transform_match(
        matched: &str,
        action: &TransformAction,
        extra_text: &Option<String>,
        shell_timeout_secs: u64,
    ) -> String {
        match action {
            TransformAction::Uppercase => matched.to_uppercase(),
            TransformAction::Lowercase => matched.to_lowercase(),
            TransformAction::Trim => matched.trim().to_string(),
            TransformAction::Prepend => {
                let prefix = extra_text.as_deref().unwrap_or("");
                format!("{}{}", prefix, matched)
            }
            TransformAction::Append => {
                let suffix = extra_text.as_deref().unwrap_or("");
                format!("{}{}", matched, suffix)
            }
            TransformAction::Reverse => matched.chars().rev().collect(),
            TransformAction::RemoveWhitespace => {
                matched.chars().filter(|c| !c.is_whitespace()).collect()
            }
            TransformAction::TitleCase => matched
                .split_whitespace()
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first) => first
                            .to_uppercase()
                            .chain(chars.flat_map(|c| c.to_lowercase()))
                            .collect(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
            TransformAction::Shell { command } => {
                crate::plugin::PluginRegistry::execute_shell_with_timeout(
                    command,
                    matched,
                    shell_timeout_secs,
                )
                .unwrap_or_else(|e| {
                    eprintln!("Shell transform error: {}", e);
                    matched.to_string()
                })
            }
            TransformAction::Plugin { name, args } => {
                crate::plugin::PluginRegistry::global_execute(name, matched, args).unwrap_or_else(
                    |e| {
                        eprintln!("Plugin error: {}", e);
                        matched.to_string()
                    },
                )
            }
            TransformAction::Base64Encode => {
                use std::io::Write;
                let mut buf = Vec::new();
                let _ = write!(buf, "{}", matched);
                base64_encode(&buf)
            }
            TransformAction::Base64Decode => {
                base64_decode(matched).unwrap_or_else(|| matched.to_string())
            }
            TransformAction::UrlEncode => url_encode(matched),
            TransformAction::UrlDecode => {
                url_decode(matched).unwrap_or_else(|| matched.to_string())
            }
            TransformAction::NormalizeWhitespace => {
                let mut result = String::new();
                let mut last_was_space = false;
                for c in matched.chars() {
                    if c.is_whitespace() {
                        if !last_was_space {
                            result.push(' ');
                            last_was_space = true;
                        }
                    } else {
                        result.push(c);
                        last_was_space = false;
                    }
                }
                result.trim().to_string()
            }
            TransformAction::Deduplicate => {
                let lines: Vec<&str> = matched.lines().collect();
                let mut seen = std::collections::HashSet::new();
                lines
                    .into_iter()
                    .filter(|line| seen.insert(*line))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            TransformAction::SortChars => {
                let mut chars: Vec<char> = matched.chars().collect();
                chars.sort();
                chars.into_iter().collect()
            }
            TransformAction::CharCount => matched.chars().count().to_string(),
            TransformAction::WordCount => matched.split_whitespace().count().to_string(),
            #[cfg(feature = "fpe")]
            TransformAction::FpeEncrypt {
                key,
                key_file: _,
                tweak,
                tweak_file: _,
                radix,
            } => {
                // key is resolved during compile_steps, so it's always Some
                let key = key.as_ref().expect("FPE key should be resolved");
                fpe_encrypt(matched, key, tweak, radix).unwrap_or_else(|e| {
                    eprintln!("FPE encrypt error: {}", e);
                    matched.to_string()
                })
            }
            #[cfg(feature = "fpe")]
            TransformAction::FpeDecrypt {
                key,
                key_file: _,
                tweak,
                tweak_file: _,
                radix,
            } => {
                // key is resolved during compile_steps, so it's always Some
                let key = key.as_ref().expect("FPE key should be resolved");
                fpe_decrypt(matched, key, tweak, radix).unwrap_or_else(|e| {
                    eprintln!("FPE decrypt error: {}", e);
                    matched.to_string()
                })
            }
            TransformAction::MaskDeterministic {
                seed,
                seed_file: _,
                preserve_prefix,
                preserve_suffix,
                mask_char,
            } => {
                // seed is resolved during compile_steps, so it's always Some
                let seed = seed.as_ref().expect("Mask seed should be resolved");
                mask_deterministic(
                    matched,
                    seed,
                    *preserve_prefix,
                    *preserve_suffix,
                    *mask_char,
                )
            }
        }
    }

    fn apply_transform(
        &self,
        pattern: &CompiledPattern,
        input: &str,
        action: &TransformAction,
        is_global: bool,
        extra_text: &Option<String>,
        step_result: &mut StepResult,
    ) -> Result<(String, bool)> {
        let match_count = pattern.find_iter(input).len();

        if match_count == 0 {
            return Ok((input.to_string(), false));
        }

        // Apply transformation to matches
        let result = if is_global {
            // Replace all matches with transformed versions.
            //
            // Why offset tracking with i64: When we replace matched text, the transformed
            // result may be longer or shorter than the original. This shifts all subsequent
            // byte positions. We track the cumulative offset to adjust match positions.
            //
            // Why i64 instead of isize: Transformations can shrink text (negative offset)
            // or grow it (positive offset). i64 ensures we can handle both directions
            // across the full usize range without overflow on 32-bit systems.
            let mut result = input.to_string();
            let mut offset: i64 = 0;

            let shell_timeout = self.config.settings.shell_timeout_secs;
            for (start, end, matched) in pattern.find_iter(input) {
                let transformed =
                    Self::transform_match(&matched, action, extra_text, shell_timeout);
                let adj_start = (start as i64 + offset) as usize;
                let adj_end = (end as i64 + offset) as usize;

                result = format!(
                    "{}{}{}",
                    &result[..adj_start],
                    transformed,
                    &result[adj_end..]
                );

                // Update offset: if transformed is longer, offset grows positive;
                // if shorter, offset becomes negative, shifting future positions left.
                offset += transformed.len() as i64 - matched.len() as i64;
                step_result.add_match();
            }
            result
        } else {
            // Replace only first match
            if let Some((start, end, matched)) = pattern.find_iter(input).first() {
                let shell_timeout = self.config.settings.shell_timeout_secs;
                let transformed = Self::transform_match(matched, action, extra_text, shell_timeout);
                step_result.add_match();
                format!("{}{}{}", &input[..*start], transformed, &input[*end..])
            } else {
                input.to_string()
            }
        };

        // We know there was at least one match (checked at function start)
        Ok((result, true))
    }

    pub fn inspect_line(&self, line: &str, step_index: Option<usize>) -> Result<Vec<MatchInfo>> {
        let mut matches = Vec::new();
        let steps_to_inspect: Vec<(usize, &CompiledStep)> = if let Some(index) = step_index {
            vec![(index, &self.compiled_steps[index])]
        } else {
            self.compiled_steps.iter().enumerate().collect()
        };

        for (idx, step) in steps_to_inspect {
            for cap in step.pattern.captures_iter(line) {
                if let Some((start, end, matched)) = cap.full_match {
                    let replacement_preview = step
                        .replacement
                        .as_ref()
                        .map(|replacement| step.pattern.replace(line, replacement));

                    matches.push(MatchInfo {
                        line_number: 1, // Will be set by caller
                        byte_start: start,
                        byte_end: end,
                        full_match: matched,
                        captures: cap.groups,
                        replacement_preview,
                        step_index: idx,
                    });
                }
            }
        }

        Ok(matches)
    }

    /// Get processing statistics.
    ///
    /// Returns a reference to the internal statistics tracking bytes processed,
    /// lines read, and timing information.
    pub fn get_stats(&self) -> &ProcessorStats {
        &self.stats
    }

    pub fn get_config(&self) -> &PipelineConfig {
        &self.config
    }

    /// Get bidirectional mapping statistics if bidirectional mode is enabled.
    pub fn get_bidirectional_stats(&self) -> Option<crate::bidirectional::MappingStats> {
        self.bidirectional_manager
            .as_ref()
            .map(|m| m.mappings().stats())
    }

    /// Save bidirectional mappings if modified.
    pub fn save_bidirectional_mappings(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref mut manager) = self.bidirectional_manager {
            manager.save_if_modified()?;
        }
        Ok(())
    }

    pub fn performance_report(&self) -> String {
        let total_time = self
            .stats
            .processing_start
            .map(|start| start.elapsed().as_millis())
            .unwrap_or(0);

        let throughput = if total_time > 0 {
            (self.stats.bytes_processed * 1000) / total_time as u64
        } else {
            0
        };

        format!(
            "Processing Performance Report:\n\
             Total time: {}ms\n\
             Lines processed: {}\n\
             Bytes processed: {}\n\
             Throughput: {} bytes/second\n\
             Steps executed: {}\n\
             Average time per step: {:.2}ms",
            total_time,
            self.stats.lines_read,
            self.stats.bytes_processed,
            throughput,
            self.compiled_steps.len(),
            if !self.stats.step_timings.is_empty() {
                self.stats.step_timings.values().sum::<u64>() as f64
                    / self.stats.step_timings.len() as f64
            } else {
                0.0
            }
        )
    }

    /// Check if any steps have syntax-aware processing configured.
    ///
    /// Returns true if any step specifies both a language and scope filter.
    /// This can be used to determine if syntax-aware processing is needed.
    #[cfg(feature = "tree-sitter")]
    pub fn has_syntax_aware_steps(&self) -> bool {
        self.compiled_steps
            .iter()
            .any(|s| s.languages.is_some() && s.scope_filter.is_some())
    }

    /// Process file content with syntax-aware scoping.
    ///
    /// This method applies pipeline steps with syntax-aware filtering when configured.
    /// For steps with `language` and `scope` specified, pattern matching and replacement
    /// is restricted to the specified scope (e.g., only in code, not in strings/comments).
    ///
    /// # Arguments
    ///
    /// * `content` - The full file content to process
    ///
    /// # Returns
    ///
    /// The processed file content with syntax-aware transformations applied.
    ///
    /// # Example
    ///
    /// ```toml
    /// [[step]]
    /// type = "substitute"
    /// pattern = "old_function"
    /// replacement = "new_function"
    /// language = "rust"
    /// scope = "code"  # Only replace in code, not in strings or comments
    /// ```
    ///
    /// For multi-language support, specify languages = ["rust", "python"] and call
    /// this with the appropriate file_language parameter.
    #[cfg(feature = "tree-sitter")]
    pub fn process_file_syntax_aware(
        &mut self,
        content: &str,
        file_language: Option<crate::syntax::Language>,
    ) -> Result<String> {
        use crate::syntax::SyntaxAnalyzer;

        let mut result = content.to_string();

        for step in &self.compiled_steps {
            // Only process steps with syntax configuration
            let (step_languages, scope) = match (&step.languages, &step.scope_filter) {
                (Some(langs), Some(scope)) => (langs, scope),
                _ => continue, // Skip non-syntax-aware steps
            };

            // Determine which language to use for analysis
            let analysis_language = if let Some(file_lang) = file_language {
                // If file language is specified, check if the step applies to it
                if step_languages.contains(&file_lang) {
                    file_lang
                } else {
                    continue; // Step doesn't apply to this file's language
                }
            } else {
                // No file language specified; use the first language from the step
                // This maintains backward compatibility for single-language usage
                step_languages[0]
            };

            // Create analyzer for the determined language
            let mut analyzer = match SyntaxAnalyzer::new(analysis_language) {
                Ok(a) => a,
                Err(e) => {
                    log::warn!("Failed to create syntax analyzer: {}", e);
                    continue;
                }
            };

            // Build standard regex for scoped operations
            let pattern_str = step.pattern.pattern_str();
            let regex = match regex::Regex::new(pattern_str) {
                Ok(r) => r,
                Err(e) => {
                    log::warn!(
                        "Failed to compile pattern for syntax-aware processing: {}",
                        e
                    );
                    continue;
                }
            };

            // Apply step based on type
            match step.step_type {
                StepType::Substitute => {
                    if let Some(ref replacement) = step.replacement {
                        result = analyzer.scoped_replace(&result, &regex, replacement, scope);
                    }
                }
                StepType::Extract => {
                    // Extract matches that are within scope
                    // For file-level processing, we collect extractions but don't modify content
                    let extracts = analyzer.scoped_extract(&result, &regex, scope);
                    if !extracts.is_empty() {
                        log::debug!(
                            "Syntax-aware extract found {} matches in scope {:?}",
                            extracts.len(),
                            scope
                        );
                        // Extractions are collected but content is unchanged
                        // The extracted data would be available in stats/output
                    }
                }
                StepType::Validate => {
                    // Validate that pattern matches only appear within expected scope
                    let validation_result = analyzer.validate_in_scope(&result, &regex, scope);
                    if !validation_result {
                        // Get detailed info for logging
                        let details = analyzer.validate_matches_detailed(&result, &regex, scope);
                        let out_of_scope: Vec<_> = details
                            .iter()
                            .filter(|(_, _, in_scope)| !in_scope)
                            .collect();
                        log::warn!(
                            "Syntax-aware validation failed: {} matches found outside {:?} scope",
                            out_of_scope.len(),
                            scope
                        );
                        for (matched, range, _) in out_of_scope.iter().take(3) {
                            log::debug!(
                                "  Out-of-scope match: '{}' at bytes {}..{}",
                                matched,
                                range.start,
                                range.end
                            );
                        }
                    }
                }
                StepType::Transform => {
                    // Apply transformation only to matches within scope
                    if let Some(ref transform_action) = step.transform_action {
                        let shell_timeout = self.config.settings.shell_timeout_secs;
                        // replacement field serves as extra_text for transforms like prepend/append
                        let extra = step.replacement.clone();
                        result = analyzer.scoped_transform(
                            &result,
                            &regex,
                            |matched| {
                                Self::transform_match(
                                    matched,
                                    transform_action,
                                    &extra,
                                    shell_timeout,
                                )
                            },
                            scope,
                        );
                    }
                }
                StepType::Filter => {
                    // Filter step with syntax-aware scope: remove lines containing
                    // matches that are within scope (or keep only those lines)
                    log::debug!(
                        "Filter step with syntax-aware scope: filtering based on in-scope matches"
                    );

                    // Find all scoped matches
                    let scoped_matches = analyzer.scoped_match(&result, &regex, scope);
                    log::debug!(
                        "Filter found {} in-scope matches",
                        scoped_matches.len()
                    );

                    // Determine which lines contain scoped matches
                    let mut lines_with_matches = std::collections::HashSet::new();
                    let line_starts: Vec<usize> = std::iter::once(0)
                        .chain(result.match_indices('\n').map(|(i, _)| i + 1))
                        .collect();

                    for m in &scoped_matches {
                        // Find which line this match is on
                        for (line_idx, &start) in line_starts.iter().enumerate() {
                            let end = line_starts.get(line_idx + 1).copied().unwrap_or(result.len());
                            if m.start >= start && m.start < end {
                                lines_with_matches.insert(line_idx);
                                break;
                            }
                        }
                    }

                    // Determine action: keep lines with matches or drop them
                    let keep_matches = match step.action {
                        Some(crate::pipeline::StepAction::KeepLine) => true,
                        Some(crate::pipeline::StepAction::DropLine) => false,
                        _ => true, // Default to keep behavior
                    };

                    // Filter lines based on action
                    let filtered_lines: Vec<&str> = result
                        .lines()
                        .enumerate()
                        .filter(|(idx, _)| {
                            let has_match = lines_with_matches.contains(idx);
                            if keep_matches { has_match } else { !has_match }
                        })
                        .map(|(_, line)| line)
                        .collect();

                    result = filtered_lines.join("\n");
                    // Preserve trailing newline if original had one
                    if result.lines().count() > 0 && !result.ends_with('\n') {
                        result.push('\n');
                    }
                }
                StepType::Block => {
                    // Block processing is inherently line-oriented with state machines.
                    // AST-based whole-file processing doesn't align with block semantics.
                    // Use streaming mode (process_stream) for block processing.
                    log::debug!(
                        "Block step type is not applicable to syntax-aware file processing; \
                         use streaming mode for block operations"
                    );
                }
            }
        }

        Ok(result)
    }

    /// Process file content, applying both regular and syntax-aware transformations.
    ///
    /// This is a convenience method that first applies syntax-aware processing
    /// for steps that specify language/scope, then falls back to regular stream
    /// processing for other steps.
    ///
    /// # Arguments
    /// * `content` - The file content to process
    /// * `file_language` - Optional language of the file (for multi-language step support)
    #[cfg(feature = "tree-sitter")]
    pub fn process_file_content(
        &mut self,
        content: &str,
        file_language: Option<crate::syntax::Language>,
    ) -> Result<(String, PipelineResult)> {
        // Apply syntax-aware transformations
        // This only processes steps that have BOTH languages AND scope_filter defined
        let processed = self.process_file_syntax_aware(content, file_language)?;

        // Count actual changes
        let input_lines = content.lines().count() as u64;
        let output_lines = processed.lines().count() as u64;
        let content_changed = processed != content;

        // For filters, matches_found = lines that were filtered (kept or dropped based on match)
        // For substitutions, matches_found = 1 if content changed
        let matches_found = if input_lines != output_lines {
            // Filtering occurred - count the difference or the output lines
            output_lines.max(1)
        } else if content_changed {
            1 // Substitution/transform occurred
        } else {
            0
        };

        let result = PipelineResult {
            lines_processed: input_lines,
            lines_output: output_lines,
            lines_dropped: input_lines.saturating_sub(output_lines),
            matches_found,
            transformations_applied: if content_changed { 1 } else { 0 },
            errors: Vec::new(),
            step_results: Vec::new(),
        };

        Ok((processed, result))
    }

    /// Detect language from file extension
    #[cfg(feature = "tree-sitter")]
    pub fn detect_language_from_extension(extension: &str) -> Option<crate::syntax::Language> {
        extension.parse().ok()
    }

    /// Process a file, automatically using syntax-aware processing if needed.
    ///
    /// This method checks if any steps require syntax-aware processing and routes
    /// to the appropriate processing path:
    /// - If syntax-aware steps exist: reads entire file and uses `process_file_content`
    /// - Otherwise: uses streaming `process_stream`
    ///
    /// # Arguments
    /// * `file_path` - Path to the file to process
    /// * `writer` - Output writer
    ///
    /// # Returns
    /// The pipeline result with processing statistics.
    // When tree-sitter is disabled, `writer` is passed directly to process_stream
    // which takes ownership, so `mut` is only needed for tree-sitter's write_all()
    #[cfg_attr(not(feature = "tree-sitter"), allow(unused_mut))]
    pub fn process_file<W: Write>(
        &mut self,
        file_path: &std::path::Path,
        mut writer: W,
    ) -> Result<PipelineResult> {
        #[cfg(feature = "tree-sitter")]
        if self.has_syntax_aware_steps() {
            // Syntax-aware processing: read entire file, process with AST
            let content = std::fs::read_to_string(file_path)
                .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;

            // Detect language from file extension
            let language = file_path
                .extension()
                .and_then(|ext| ext.to_str())
                .and_then(Self::detect_language_from_extension);

            let (output, result) = self.process_file_content(&content, language)?;
            writer.write_all(output.as_bytes())?;
            return Ok(result);
        }

        // Standard stream processing
        let file = std::fs::File::open(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to open file: {}", e))?;
        let reader = std::io::BufReader::new(file);
        self.process_stream(reader, writer)
    }
}

impl ProcessorStats {
    /// Calculate throughput in bytes per second based on elapsed processing time.
    ///
    /// Returns 0 if processing hasn't started or no time has elapsed.
    pub fn throughput_bytes_per_second(&self) -> u64 {
        if let Some(start) = self.processing_start {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            if elapsed_ms > 0 {
                (self.bytes_processed * 1000) / elapsed_ms
            } else {
                0
            }
        } else {
            0
        }
    }

    /// Calculate lines processed per second based on elapsed processing time.
    ///
    /// Returns 0 if processing hasn't started or no time has elapsed.
    pub fn lines_per_second(&self) -> u64 {
        if let Some(start) = self.processing_start {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            if elapsed_ms > 0 {
                (self.lines_read * 1000) / elapsed_ms
            } else {
                0
            }
        } else {
            0
        }
    }

    /// Get the total elapsed processing time in milliseconds.
    pub fn elapsed_ms(&self) -> u64 {
        self.processing_start
            .map(|start| start.elapsed().as_millis() as u64)
            .unwrap_or(0)
    }

    /// Get the processing time for a specific step in milliseconds.
    pub fn step_time_ms(&self, step_index: usize) -> u64 {
        self.step_timings.get(&step_index).copied().unwrap_or(0)
    }
}

// Helper functions for encoding/decoding transformations

/// Base64 encode bytes using a simple implementation.
///
/// Why not use a crate: Base64 is straightforward (RFC 4648), and adding a dependency
/// for a single transform seems excessive. This implementation handles the standard
/// alphabet and padding correctly.
///
/// Algorithm: Process 3 bytes at a time, producing 4 base64 characters. Each 6-bit
/// group maps to one character from the 64-character alphabet. Padding with '=' is
/// added when input length isn't divisible by 3.
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();

    for chunk in data.chunks(3) {
        let mut n = (chunk[0] as u32) << 16;
        if chunk.len() > 1 {
            n |= (chunk[1] as u32) << 8;
        }
        if chunk.len() > 2 {
            n |= chunk[2] as u32;
        }

        result.push(ALPHABET[(n >> 18) as usize & 0x3F] as char);
        result.push(ALPHABET[(n >> 12) as usize & 0x3F] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[(n >> 6) as usize & 0x3F] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[n as usize & 0x3F] as char);
        } else {
            result.push('=');
        }
    }

    result
}

/// Base64 decode a string.
///
/// Why Option<String>: Decoding can fail if the input contains invalid characters
/// or is malformed. Returning None allows callers to fall back gracefully.
///
/// Why i8 lookup table: Using -1 for invalid characters lets us detect errors
/// in a single array lookup. The table maps ASCII values 0-127 to their 6-bit
/// values (0-63), or -1 if the character isn't part of the base64 alphabet.
fn base64_decode(s: &str) -> Option<String> {
    const DECODE: [i8; 128] = [
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 62, -1, -1,
        -1, 63, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, -1, -1, -1, -1, -1, -1, -1, 0, 1, 2, 3, 4,
        5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, -1, -1, -1,
        -1, -1, -1, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45,
        46, 47, 48, 49, 50, 51, -1, -1, -1, -1, -1,
    ];

    let s = s.trim_end_matches('=');
    let mut bytes = Vec::new();
    let chars: Vec<u8> = s.bytes().collect();

    for chunk in chars.chunks(4) {
        if chunk.iter().any(|&c| c >= 128 || DECODE[c as usize] < 0) {
            return None;
        }

        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, &c)| {
            acc | ((DECODE[c as usize] as u32) << (18 - i * 6))
        });

        bytes.push((n >> 16) as u8);
        if chunk.len() > 2 {
            bytes.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            bytes.push(n as u8);
        }
    }

    String::from_utf8(bytes).ok()
}

/// URL encode a string (percent encoding).
///
/// Why these characters are unreserved: RFC 3986 defines the "unreserved" set as
/// ALPHA / DIGIT / "-" / "." / "_" / "~". These can appear literally in URLs
/// without encoding. All other characters must be percent-encoded.
///
/// Why c as u32: Most ASCII characters fit in a single byte, but we use u32 to
/// handle the full Unicode range correctly (multi-byte sequences become multiple
/// %XX escapes per byte).
fn url_encode(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
                c.to_string()
            } else {
                format!("%{:02X}", c as u32)
            }
        })
        .collect()
}

/// URL decode a string (percent decoding).
///
/// Why '+' becomes space: HTML form encoding (application/x-www-form-urlencoded)
/// uses '+' for spaces instead of '%20'. We support both for compatibility with
/// form data.
///
/// Why Option: Returns None if percent sequences are malformed (incomplete or
/// non-hex characters). This allows callers to preserve the original on failure.
fn url_decode(s: &str) -> Option<String> {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            // Read two hex digits after '%'
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                } else {
                    return None; // Invalid hex digits
                }
            } else {
                return None; // Truncated escape sequence
            }
        } else if c == '+' {
            result.push(' '); // Form-urlencoded space
        } else {
            result.push(c);
        }
    }

    Some(result)
}

/// Format-preserving encryption using FF1 algorithm (NIST SP 800-38G).
///
/// Encrypts text while preserving its format - each character is replaced with
/// another character from the same alphabet. For example, encrypting "123456789"
/// with a numeric radix will produce another 9-digit number.
///
/// # Arguments
/// * `input` - The text to encrypt
/// * `key` - Hex-encoded encryption key (16/24/32 bytes for AES-128/192/256)
/// * `tweak` - Hex-encoded tweak value (up to 16 bytes, can be empty)
/// * `radix` - The character set (e.g., "0123456789" for digits)
///
/// # Returns
/// Encrypted text with same length and character set as input
#[cfg(feature = "fpe")]
fn fpe_encrypt(input: &str, key: &str, tweak: &str, radix: &str) -> Result<String, String> {
    use fpe::ff1::{BinaryNumeralString, FF1};

    // Parse key from hex
    let key_bytes = hex_decode(key).map_err(|e| format!("Invalid key hex: {}", e))?;
    if key_bytes.len() != 16 && key_bytes.len() != 24 && key_bytes.len() != 32 {
        return Err(format!(
            "Key must be 16, 24, or 32 bytes (got {} bytes)",
            key_bytes.len()
        ));
    }

    // Parse tweak from hex (can be empty)
    let tweak_bytes = if tweak.is_empty() {
        Vec::new()
    } else {
        hex_decode(tweak).map_err(|e| format!("Invalid tweak hex: {}", e))?
    };

    // Build the radix (character set)
    let radix_chars: Vec<char> = radix.chars().collect();
    let radix_size = radix_chars.len();
    if radix_size < 2 {
        return Err("Radix must have at least 2 characters".to_string());
    }

    // Convert input to numeral string (indices into radix)
    let mut numerals: Vec<u16> = Vec::new();
    for c in input.chars() {
        if let Some(idx) = radix_chars.iter().position(|&r| r == c) {
            numerals.push(idx as u16);
        } else {
            // Character not in radix - pass through unchanged
            // For now, we'll error on this
            return Err(format!("Character '{}' not in radix '{}'", c, radix));
        }
    }

    if numerals.len() < 2 {
        return Err("Input must have at least 2 characters for FPE".to_string());
    }

    // Create FF1 cipher
    let ff1 = FF1::<aes::Aes256>::new(&key_bytes, radix_size as u32)
        .map_err(|e| format!("FF1 initialization error: {:?}", e))?;

    // Encrypt
    let bns =
        BinaryNumeralString::from_bytes_le(&numerals.iter().map(|&n| n as u8).collect::<Vec<_>>());
    let encrypted = ff1
        .encrypt(&tweak_bytes, &bns)
        .map_err(|e| format!("Encryption error: {:?}", e))?;

    // Convert back to string
    let encrypted_bytes = encrypted.to_bytes_le();
    let result: String = encrypted_bytes
        .iter()
        .map(|&idx| radix_chars.get(idx as usize).copied().unwrap_or('?'))
        .collect();

    Ok(result)
}

/// Format-preserving decryption using FF1 algorithm.
///
/// Decrypts text that was encrypted with fpe_encrypt using the same key, tweak, and radix.
#[cfg(feature = "fpe")]
fn fpe_decrypt(input: &str, key: &str, tweak: &str, radix: &str) -> Result<String, String> {
    use fpe::ff1::{BinaryNumeralString, FF1};

    // Parse key from hex
    let key_bytes = hex_decode(key).map_err(|e| format!("Invalid key hex: {}", e))?;
    if key_bytes.len() != 16 && key_bytes.len() != 24 && key_bytes.len() != 32 {
        return Err(format!(
            "Key must be 16, 24, or 32 bytes (got {} bytes)",
            key_bytes.len()
        ));
    }

    // Parse tweak from hex (can be empty)
    let tweak_bytes = if tweak.is_empty() {
        Vec::new()
    } else {
        hex_decode(tweak).map_err(|e| format!("Invalid tweak hex: {}", e))?
    };

    // Build the radix (character set)
    let radix_chars: Vec<char> = radix.chars().collect();
    let radix_size = radix_chars.len();
    if radix_size < 2 {
        return Err("Radix must have at least 2 characters".to_string());
    }

    // Convert input to numeral string (indices into radix)
    let mut numerals: Vec<u16> = Vec::new();
    for c in input.chars() {
        if let Some(idx) = radix_chars.iter().position(|&r| r == c) {
            numerals.push(idx as u16);
        } else {
            return Err(format!("Character '{}' not in radix '{}'", c, radix));
        }
    }

    if numerals.len() < 2 {
        return Err("Input must have at least 2 characters for FPE".to_string());
    }

    // Create FF1 cipher
    let ff1 = FF1::<aes::Aes256>::new(&key_bytes, radix_size as u32)
        .map_err(|e| format!("FF1 initialization error: {:?}", e))?;

    // Decrypt
    let bns =
        BinaryNumeralString::from_bytes_le(&numerals.iter().map(|&n| n as u8).collect::<Vec<_>>());
    let decrypted = ff1
        .decrypt(&tweak_bytes, &bns)
        .map_err(|e| format!("Decryption error: {:?}", e))?;

    // Convert back to string
    let decrypted_bytes = decrypted.to_bytes_le();
    let result: String = decrypted_bytes
        .iter()
        .map(|&idx| radix_chars.get(idx as usize).copied().unwrap_or('?'))
        .collect();

    Ok(result)
}

/// Decode hex string to bytes.
#[cfg(feature = "fpe")]
fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err("Hex string must have even length".to_string());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|_| format!("Invalid hex at position {}", i))
        })
        .collect()
}

/// Deterministic masking using consistent hashing.
///
/// Same input always produces same output (given same seed), which is useful for:
/// - Joining datasets on masked keys
/// - Consistent test data generation
/// - Auditing masked data for duplicates
///
/// Unlike FPE, this is ONE-WAY (cannot be reversed).
fn mask_deterministic(
    input: &str,
    seed: &str,
    preserve_prefix: usize,
    preserve_suffix: usize,
    mask_char: char,
) -> String {
    let len = input.chars().count();

    // Handle edge cases
    if len == 0 || preserve_prefix + preserve_suffix >= len {
        return input.to_string();
    }

    // Simple deterministic hash: combine input and seed
    // Use a FNV-1a like hash for better mixing
    let mut hash: u64 = 0xcbf29ce484222325; // FNV offset basis

    // Mix in seed first
    for c in seed.chars() {
        hash ^= c as u64;
        hash = hash.wrapping_mul(0x100000001b3); // FNV prime
    }

    // Then mix in input
    for c in input.chars() {
        hash ^= c as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }

    // Build result
    let chars: Vec<char> = input.chars().collect();
    let mut result = String::new();

    // Preserve prefix
    for c in chars.iter().take(preserve_prefix) {
        result.push(*c);
    }

    // Mask middle section with deterministic pattern
    let mask_len = len - preserve_prefix - preserve_suffix;
    let mut running_hash = hash;
    for _ in 0..mask_len {
        // Use running hash to deterministically select mask character
        // This ensures same input+seed always produces same output
        // Advance the running hash for each position
        running_hash ^= running_hash >> 12;
        running_hash ^= running_hash << 25;
        running_hash ^= running_hash >> 27;
        running_hash = running_hash.wrapping_mul(0x2545F4914F6CDD1D);

        let masked = if mask_char == '*' {
            // Default: use digits based on hash for format-preserving masking
            char::from_digit((running_hash % 10) as u32, 10).unwrap_or('*')
        } else {
            mask_char
        };
        result.push(masked);
    }

    // Preserve suffix
    for c in chars.iter().skip(len - preserve_suffix) {
        result.push(*c);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::*;
    use std::io::Cursor;

    #[test]
    fn test_basic_substitution() {
        let config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUMBER"));
        let mut processor = StreamProcessor::new(config).unwrap();

        let input = "Test 123 and 456";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        let result = processor.process_stream(reader, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        assert_eq!(output_str.trim(), "Test NUMBER and NUMBER");
        assert_eq!(result.lines_processed, 1);
        // Each substitution (123→NUMBER, 456→NUMBER) counts as one transformation
        assert_eq!(
            result.transformations_applied, 1,
            "One substitution step applied"
        );
        assert_eq!(result.matches_found, 2, "Two digit sequences matched");
    }

    #[test]
    fn test_filter_processing() {
        let config = PipelineConfig {
            name: Some("Test Filter".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings::default(),
            step: vec![PipelineStep {
                step_type: StepType::Filter,
                pattern: "keep".to_string(),
                replacement: None,
                action: Some(StepAction::KeepLine),
                transform: None,
                flags: None,
                description: None,
                enabled: Some(true),
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut processor = StreamProcessor::new(config).unwrap();

        let input = "keep this line\ndrop this line\nkeep this too";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        let _result = processor.process_stream(reader, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = output_str.trim().split('\n').collect();

        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("keep this line"));
        assert!(lines[1].contains("keep this too"));
    }

    #[test]
    fn test_match_inspection() {
        let config = PipelineConfig::from_inline_pattern(r"(\d+)", Some("NUMBER"));
        let processor = StreamProcessor::new(config).unwrap();

        let matches = processor.inspect_line("Test 123 and 456", None).unwrap();

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].full_match, "123");
        assert_eq!(matches[1].full_match, "456");
        assert!(matches[0].replacement_preview.is_some());
    }

    #[test]
    fn test_context_before_lines() {
        // Create a filter config with before-context
        let config = PipelineConfig {
            name: Some("Context Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings {
                context_before: 2,
                ..Default::default()
            },
            step: vec![PipelineStep {
                step_type: StepType::Filter,
                pattern: "MATCH".to_string(),
                replacement: None,
                action: Some(StepAction::KeepLine),
                transform: None,
                flags: None,
                description: None,
                enabled: Some(true),
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut processor = StreamProcessor::new(config).unwrap();

        let input = "line 1\nline 2\nline 3\nMATCH line 4\nline 5";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        processor.process_stream(reader, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = output_str.trim().split('\n').collect();

        // Should have 2 before-context lines + the match line
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("line 2"));
        assert!(lines[1].contains("line 3"));
        assert!(lines[2].contains("MATCH"));
    }

    #[test]
    fn test_context_after_lines() {
        let config = PipelineConfig {
            name: Some("Context After Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings {
                context_after: 2,
                ..Default::default()
            },
            step: vec![PipelineStep {
                step_type: StepType::Filter,
                pattern: "MATCH".to_string(),
                replacement: None,
                action: Some(StepAction::KeepLine),
                transform: None,
                flags: None,
                description: None,
                enabled: Some(true),
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut processor = StreamProcessor::new(config).unwrap();

        let input = "line 1\nMATCH line 2\nline 3\nline 4\nline 5";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        processor.process_stream(reader, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = output_str.trim().split('\n').collect();

        // Should have the match line + 2 after-context lines
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("MATCH"));
        assert!(lines[1].contains("line 3"));
        assert!(lines[2].contains("line 4"));
    }

    #[test]
    fn test_multiple_match_counting() {
        // Test that substitution correctly counts multiple matches
        let config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUM"));
        let mut processor = StreamProcessor::new(config).unwrap();

        let input = "Test 1 2 3 4 5"; // 5 numbers
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        let result = processor.process_stream(reader, &mut output).unwrap();

        // Should count 5 matches, not 1
        assert!(
            result.matches_found >= 5,
            "Expected at least 5 matches, got {}",
            result.matches_found
        );
    }

    #[test]
    fn test_crlf_preservation() {
        // Test that CRLF line endings are preserved when preserve_line_endings is true
        let mut config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUM"));
        config.settings.preserve_line_endings = true;

        let mut processor = StreamProcessor::new(config).unwrap();

        // Input with CRLF line endings
        let input = "Line 1: 123\r\nLine 2: 456\r\nLine 3: 789\r\n";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        processor.process_stream(reader, &mut output).unwrap();
        let output_bytes = output;

        // Output should preserve CRLF endings
        assert!(
            output_bytes.windows(2).any(|w| w == b"\r\n"),
            "Expected CRLF in output, got: {:?}",
            String::from_utf8_lossy(&output_bytes)
        );

        // Should not have bare LF without CR
        let output_str = String::from_utf8(output_bytes).unwrap();
        let lines: Vec<&str> = output_str.split("\r\n").collect();
        assert!(lines.len() >= 3, "Expected at least 3 CRLF-delimited lines");
    }

    #[test]
    fn test_default_lf_output() {
        // Test that default behavior outputs LF line endings
        let config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUM"));
        let mut processor = StreamProcessor::new(config).unwrap();

        // Input with CRLF line endings
        let input = "Line 1: 123\r\nLine 2: 456\r\n";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        processor.process_stream(reader, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        // Default behavior: output should have LF, not CRLF
        // (CRLF from input is stripped, and only LF is added)
        assert!(
            !output_str.contains("\r\n"),
            "Expected LF-only output, but got CRLF: {:?}",
            output_str
        );
    }

    #[test]
    fn test_mixed_line_endings_preserved() {
        // Test that mixed line endings are preserved per-line
        let mut config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUM"));
        config.settings.preserve_line_endings = true;

        let mut processor = StreamProcessor::new(config).unwrap();

        // Mixed input: first line has LF, second has CRLF
        let input = "Unix line 123\nWindows line 456\r\n";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        processor.process_stream(reader, &mut output).unwrap();
        let output_bytes = output;
        let output_str = String::from_utf8_lossy(&output_bytes).to_string();

        // First line should have LF
        assert!(
            output_str.starts_with("Unix line NUM\n"),
            "First line should have LF: {:?}",
            output_str
        );
        // Second line should have CRLF
        assert!(
            output_str.contains("Windows line NUM\r\n"),
            "Second line should have CRLF: {:?}",
            output_str
        );
    }

    #[test]
    fn test_max_line_length_skip() {
        // Test that lines exceeding max length are skipped (output unchanged)
        let mut config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUM"));
        config.settings.max_line_length = 20;
        config.settings.max_line_action = MaxLineAction::Skip;

        let mut processor = StreamProcessor::new(config).unwrap();

        // First line is short (processed), second is long (skipped)
        let input = "Short 123\nThis is a very long line with 456 numbers\n";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        processor.process_stream(reader, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        // Short line should be processed
        assert!(
            output_str.contains("Short NUM"),
            "Short line should be processed: {:?}",
            output_str
        );
        // Long line should be unchanged (skipped)
        assert!(
            output_str.contains("456"),
            "Long line should be unchanged (not replaced): {:?}",
            output_str
        );
    }

    #[test]
    fn test_max_line_length_error() {
        // Test that lines exceeding max length cause an error
        let mut config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUM"));
        config.settings.max_line_length = 20;
        config.settings.max_line_action = MaxLineAction::Error;

        let mut processor = StreamProcessor::new(config).unwrap();

        // Long line should cause error
        let input = "This is a very long line exceeding the limit\n";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        let result = processor.process_stream(reader, &mut output);
        assert!(result.is_err(), "Expected error for long line");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("exceeds maximum length"),
            "Error should mention exceeding max length"
        );
    }

    #[test]
    fn test_max_line_length_truncate() {
        // Test that lines exceeding max length are truncated
        let mut config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUM"));
        config.settings.max_line_length = 15;
        config.settings.max_line_action = MaxLineAction::Truncate;

        let mut processor = StreamProcessor::new(config).unwrap();

        // Long line should be truncated
        let input = "Line 123456789 extra content\n";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        processor.process_stream(reader, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        // Output should be truncated and processed
        assert!(
            output_str.len() < input.len(),
            "Output should be shorter: {:?}",
            output_str
        );
        // Numbers should be replaced in the truncated portion
        assert!(
            output_str.contains("NUM") || !output_str.contains("123"),
            "Numbers should be processed or truncated: {:?}",
            output_str
        );
    }

    #[test]
    fn test_max_line_length_zero_means_unlimited() {
        // Test that max_line_length = 0 means no limit
        let mut config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUM"));
        config.settings.max_line_length = 0; // No limit

        let mut processor = StreamProcessor::new(config).unwrap();

        // Very long line should be processed normally
        let long_line = format!("Long line with {} numbers\n", "1".repeat(10000));
        let reader = Cursor::new(long_line.clone());
        let mut output = Vec::new();

        processor.process_stream(reader, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        // Numbers should be replaced
        assert!(
            output_str.contains("NUM"),
            "Numbers should be replaced: {:?}",
            output_str
        );
    }

    #[test]
    fn test_block_step_keep_block() {
        // Test Block step with KeepBlock action - keep only lines within blocks
        let config = PipelineConfig {
            name: Some("Block Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings::default(),
            step: vec![PipelineStep {
                step_type: StepType::Block,
                pattern: String::new(),
                start_pattern: Some(r"^BEGIN$".to_string()),
                replacement: None,
                action: Some(StepAction::KeepBlock),
                transform: None,
                flags: None,
                description: None,
                enabled: Some(true),
                end_pattern: Some(r"^END$".to_string()),
                block_context: None,
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut processor = StreamProcessor::new(config).unwrap();

        let input = "outside 1\nBEGIN\ninside 1\ninside 2\nEND\noutside 2\n";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        processor.process_stream(reader, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        // Should include lines inside block (including BEGIN/END)
        assert!(output_str.contains("BEGIN"), "Should include BEGIN");
        assert!(output_str.contains("inside 1"), "Should include inside 1");
        assert!(output_str.contains("inside 2"), "Should include inside 2");
        // Lines outside block should be dropped
        assert!(
            !output_str.contains("outside 1"),
            "Should not include outside 1"
        );
        assert!(
            !output_str.contains("outside 2"),
            "Should not include outside 2"
        );
    }

    #[test]
    fn test_block_step_drop_block() {
        // Test Block step with DropBlock action - drop lines within blocks
        let config = PipelineConfig {
            name: Some("Block Drop Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings::default(),
            step: vec![PipelineStep {
                step_type: StepType::Block,
                pattern: String::new(),
                start_pattern: Some(r"^BEGIN$".to_string()),
                replacement: None,
                action: Some(StepAction::DropBlock),
                transform: None,
                flags: None,
                description: None,
                enabled: Some(true),
                end_pattern: Some(r"^END$".to_string()),
                block_context: None,
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut processor = StreamProcessor::new(config).unwrap();

        let input = "keep 1\nBEGIN\ndrop 1\ndrop 2\nEND\nkeep 2\n";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        processor.process_stream(reader, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        // Lines outside block should be kept
        assert!(output_str.contains("keep 1"), "Should include keep 1");
        assert!(output_str.contains("keep 2"), "Should include keep 2");
        // Lines inside block (including BEGIN) should be dropped
        assert!(!output_str.contains("drop 1"), "Should not include drop 1");
        assert!(!output_str.contains("drop 2"), "Should not include drop 2");
    }

    #[test]
    fn test_block_step_mark_block() {
        // Test Block step with MarkBlock action - mark lines within blocks
        let config = PipelineConfig {
            name: Some("Block Mark Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings::default(),
            step: vec![PipelineStep {
                step_type: StepType::Block,
                pattern: String::new(),
                start_pattern: Some(r"^START$".to_string()),
                replacement: None,
                action: Some(StepAction::MarkBlock {
                    marker: ">>> ".to_string(),
                }),
                transform: None,
                flags: None,
                description: None,
                enabled: Some(true),
                end_pattern: Some(r"^STOP$".to_string()),
                block_context: None,
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut processor = StreamProcessor::new(config).unwrap();

        let input = "normal\nSTART\nmarked line\nSTOP\nnormal again\n";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        processor.process_stream(reader, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        // Lines inside block should be marked
        assert!(output_str.contains(">>> START"), "START should be marked");
        assert!(
            output_str.contains(">>> marked line"),
            "marked line should be marked"
        );
        // Lines outside block should not be marked
        assert!(
            output_str.contains("\nnormal\n") || output_str.starts_with("normal\n"),
            "normal should not be marked"
        );
    }

    #[test]
    fn test_mask_deterministic_helper() {
        // Test the deterministic masking helper function
        let result1 = mask_deterministic("123456789", "seed123", 0, 0, '*');
        let result2 = mask_deterministic("123456789", "seed123", 0, 0, '*');
        // Same input + seed should produce same output
        assert_eq!(
            result1, result2,
            "Deterministic masking should be consistent"
        );

        // Different seed should produce different output
        let result3 = mask_deterministic("123456789", "different_seed", 0, 0, '*');
        assert_ne!(
            result1, result3,
            "Different seeds should produce different results"
        );

        // Test prefix preservation
        let result4 = mask_deterministic("123456789", "seed123", 4, 0, 'X');
        assert!(
            result4.starts_with("1234"),
            "Should preserve first 4 chars: {}",
            result4
        );
        assert!(
            result4.chars().skip(4).all(|c| c == 'X'),
            "Rest should be masked with X"
        );

        // Test suffix preservation
        let result5 = mask_deterministic("123456789", "seed123", 0, 4, 'X');
        assert!(
            result5.ends_with("6789"),
            "Should preserve last 4 chars: {}",
            result5
        );

        // Test both prefix and suffix
        let result6 = mask_deterministic("1234-5678-9012", "seed", 4, 4, '*');
        assert!(result6.starts_with("1234"), "Should preserve prefix");
        assert!(
            result6.ends_with("9012"),
            "Should preserve suffix: {}",
            result6
        );
    }

    #[test]
    fn test_finalize_basic_counter() {
        let toml = r#"
name = "Test Finalize"

# Need at least one step for validation
[[step]]
type = "filter"
pattern = "."
action = "keep_line"

[finalize]
template = "Errors: ${count:errors}"
suppress_output = true

[[finalize.counters]]
name = "errors"
pattern = "ERROR"
"#;
        let config: PipelineConfig = toml::from_str(toml).unwrap();
        let mut processor = StreamProcessor::new(config).unwrap();

        let input = "INFO: Started\nERROR: Failed\nINFO: Retrying\nERROR: Failed again\n";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        let _result = processor.process_stream(reader, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        assert!(
            output_str.contains("Errors: 2"),
            "Expected 'Errors: 2', got: {}",
            output_str
        );
    }

    #[test]
    fn test_finalize_deduplicate_counter() {
        let toml = r#"
name = "Test Unique IPs"

[[step]]
type = "filter"
pattern = "."
action = "keep_line"

[finalize]
template = "Unique IPs: ${count:ips}"
suppress_output = true

[[finalize.counters]]
name = "ips"
pattern = "^(\\d+\\.\\d+\\.\\d+\\.\\d+)"
deduplicate = true
"#;
        let config: PipelineConfig = toml::from_str(toml).unwrap();
        let mut processor = StreamProcessor::new(config).unwrap();

        let input = "192.168.1.1 - request 1\n192.168.1.2 - request 2\n192.168.1.1 - request 3\n";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        let _result = processor.process_stream(reader, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        assert!(
            output_str.contains("Unique IPs: 2"),
            "Expected 'Unique IPs: 2' (deduplicated), got: {}",
            output_str
        );
    }

    #[test]
    fn test_finalize_json_output() {
        let toml = r#"
name = "Test JSON Output"

[[step]]
type = "filter"
pattern = "."
action = "keep_line"

[finalize]
output_format = "json"
suppress_output = true

[[finalize.counters]]
name = "errors"
pattern = "ERROR"
"#;
        let config: PipelineConfig = toml::from_str(toml).unwrap();
        let mut processor = StreamProcessor::new(config).unwrap();

        let input = "ERROR: test\nINFO: ok\n";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        let _result = processor.process_stream(reader, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        // Should be valid JSON
        let json: serde_json::Value = serde_json::from_str(&output_str).unwrap();
        assert_eq!(json["counters"]["errors"]["count"], 1);
        assert_eq!(json["lines_processed"], 2);
    }

    #[test]
    fn test_finalize_with_processing() {
        // Test that finalize works alongside normal processing
        let toml = r#"
name = "Test Combined"

[[step]]
type = "substitute"
pattern = "foo"
replacement = "bar"
flags = ["global"]

[finalize]
template = "Substitutions: ${transformations}"
"#;
        let config: PipelineConfig = toml::from_str(toml).unwrap();
        let mut processor = StreamProcessor::new(config).unwrap();

        let input = "foo is foo\n";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        let result = processor.process_stream(reader, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        // Should have both transformed output AND finalize summary
        assert!(
            output_str.contains("bar is bar"),
            "Should transform foo->bar, got: {}",
            output_str
        );
        assert!(
            output_str.contains("Substitutions: 1"),
            "Should show substitution count, got: {}",
            output_str
        );
        assert_eq!(result.transformations_applied, 1);
    }

    #[test]
    fn test_finalize_multiple_counters() {
        let toml = r#"
name = "Test Multiple Counters"

[[step]]
type = "filter"
pattern = "."
action = "keep_line"

[finalize]
template = """
Errors: ${count:errors}
Warnings: ${count:warnings}
Total lines: ${lines}
"""
suppress_output = true

[[finalize.counters]]
name = "errors"
pattern = "ERROR"

[[finalize.counters]]
name = "warnings"
pattern = "WARN"
"#;
        let config: PipelineConfig = toml::from_str(toml).unwrap();
        let mut processor = StreamProcessor::new(config).unwrap();

        let input = "ERROR: fail\nWARN: issue\nERROR: fail2\nINFO: ok\n";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        let _result = processor.process_stream(reader, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        assert!(output_str.contains("Errors: 2"), "Expected 2 errors");
        assert!(output_str.contains("Warnings: 1"), "Expected 1 warning");
        assert!(output_str.contains("Total lines: 4"), "Expected 4 lines");
    }
}

#[cfg(test)]
mod on_mismatch_tests {
    use crate::pipeline::{OnMismatch, PipelineConfig};

    #[test]
    fn test_on_mismatch_parsing() {
        let toml = r#"
name = "Test"

[[step]]
type = "validate"
pattern = "^ok"
on_mismatch = "warn"
"#;
        let config: PipelineConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.step[0].on_mismatch, Some(OnMismatch::Warn));
    }
}

#[cfg(test)]
mod on_mismatch_behavior_tests {
    use crate::pipeline::{OnMismatch, PipelineConfig, PipelineStep, StepType};
    use crate::processor::StreamProcessor;
    use std::io::Cursor;

    #[test]
    fn test_on_mismatch_warn_outputs_line() {
        let config = PipelineConfig {
            name: Some("Test".to_string()),
            step: vec![PipelineStep {
                step_type: StepType::Validate,
                pattern: "^ok".to_string(),
                on_mismatch: Some(OnMismatch::Warn),
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut processor = StreamProcessor::new(config).unwrap();
        let input = Cursor::new("ok line\nbad line\nok again\n");
        let mut output = Vec::new();

        processor.process_stream(input, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        // With on_mismatch = warn, ALL lines should be output
        assert!(output_str.contains("ok line"), "Should contain ok line");
        assert!(
            output_str.contains("bad line"),
            "Should contain bad line (warn mode)"
        );
        assert!(output_str.contains("ok again"), "Should contain ok again");
    }

    #[test]
    fn test_on_mismatch_skip_drops_line() {
        let config = PipelineConfig {
            name: Some("Test".to_string()),
            step: vec![PipelineStep {
                step_type: StepType::Validate,
                pattern: "^ok".to_string(),
                on_mismatch: Some(OnMismatch::Skip),
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut processor = StreamProcessor::new(config).unwrap();
        let input = Cursor::new("ok line\nbad line\nok again\n");
        let mut output = Vec::new();

        processor.process_stream(input, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        // With on_mismatch = skip, only valid lines should be output
        assert!(output_str.contains("ok line"), "Should contain ok line");
        assert!(
            !output_str.contains("bad line"),
            "Should NOT contain bad line (skip mode)"
        );
        assert!(output_str.contains("ok again"), "Should contain ok again");
    }
}
