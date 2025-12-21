use crate::error::{PatternError, ValidationError};
use crate::pipeline::{
    BlockAction, ErrorType, FilterAction, MaxLineAction, PipelineConfig, PipelineError,
    PipelineResult, PipelineSettings, RegexFlag, StepResult, StepType, TransformAction,
};
use anyhow::{Context, Result};
use log::{debug, trace};
use regex::{Regex, RegexBuilder};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, Write};
#[cfg(feature = "pcre")]
use std::sync::LazyLock;
use std::time::Instant;

/// Pre-compiled regex for detecting repetition patterns like `{10000}` in ReDoS analysis
#[cfg(feature = "pcre")]
static REPETITION_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{(\d+)\}").expect("invalid repetition regex"));

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

impl CompiledPattern {
    pub fn is_match(&self, text: &str) -> bool {
        match self {
            CompiledPattern::Standard(re) => re.is_match(text),
            CompiledPattern::Fixed(s) => text.contains(s),
            #[cfg(feature = "pcre")]
            CompiledPattern::Pcre(re) => re.is_match(text).unwrap_or(false),
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
    replacement: Option<String>,
    action: Option<FilterAction>,
    transform_action: Option<TransformAction>,
    step_type: StepType,
    is_global: bool,
    // Block step fields
    until_pattern: Option<CompiledPattern>,
    block_action: Option<BlockAction>,
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
        debug!(
            "Creating StreamProcessor with {} steps",
            config.step.len()
        );

        if let Err(validation_errors) = config.validate() {
            debug!("Pipeline validation failed: {:?}", validation_errors);
            let error = ValidationError::Multiple {
                count: validation_errors.len(),
                errors: validation_errors.join("\n  - "),
            };
            return Err(error).context("Pipeline validation failed");
        }

        let compiled_steps = Self::compile_steps(&config)?;
        debug!(
            "Compiled {} enabled steps",
            compiled_steps.len()
        );

        // Initialize block states for Block step types
        let block_states = vec![false; compiled_steps.len()];

        Ok(Self {
            config,
            compiled_steps,
            stats: ProcessorStats::default(),
            context_before_buffer: VecDeque::new(),
            after_context_remaining: 0,
            last_output_line: 0,
            block_states,
        })
    }

    /// Check if context lines feature is enabled
    fn has_context(&self) -> bool {
        self.config.settings.context_before > 0 || self.config.settings.context_after > 0
    }

    fn compile_steps(config: &PipelineConfig) -> Result<Vec<CompiledStep>> {
        let mut compiled_steps = Vec::new();
        let settings = &config.settings;

        for (index, step) in config.enabled_steps().enumerate() {
            let is_global = step
                .flags
                .as_ref()
                .map(|f| f.iter().any(|flag| matches!(flag, RegexFlag::Global)))
                .unwrap_or(false);

            let pattern = Self::build_pattern(&step.pattern, &step.flags, settings)?;
            let replacement = step.replacement.clone();

            // Compile the until pattern for Block steps
            let until_pattern = if let Some(ref until_str) = step.until {
                Some(Self::build_pattern(until_str, &step.flags, settings)?)
            } else {
                None
            };

            compiled_steps.push(CompiledStep {
                step_index: index,
                pattern,
                replacement,
                action: step.action.clone(),
                transform_action: step.transform.clone(),
                step_type: step.step_type.clone(),
                is_global,
                until_pattern,
                block_action: step.block_action.clone(),
            });
        }

        Ok(compiled_steps)
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

        // Check for zero-width match patterns that could cause unexpected behavior
        if let Some(warning) = Self::check_zero_width_pattern(pattern) {
            eprintln!("{}", warning);
        }

        // PCRE mode - use fancy-regex for advanced features
        #[cfg(feature = "pcre")]
        if settings.pcre_mode {
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
        if settings.pcre_mode {
            return Err(PatternError::PcreNotEnabled).context(
                "Suggestion: Rebuild with `cargo build --features pcre` or remove the -P flag",
            );
        }

        // Standard regex mode
        match Self::build_regex(pattern, flags, settings.regex_size_limit) {
            Ok(regex) => Ok(CompiledPattern::Standard(regex)),
            Err(e) => Err(PatternError::invalid_regex(pattern, e.to_string()))
                .context("Regex pattern compilation failed"),
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
            "^", "$", r"\b", r"\B", r"\A", r"\z", r"\Z",
            "^$", r"^\b", r"\b$",
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
            ".*", ".?", "\\s*", "\\S*", "\\d*", "\\D*", "\\w*", "\\W*",
            "[^a]*", "()*", "()?", "(?:)*", "(?:)?",
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
        if (pattern.starts_with("(?=") || pattern.starts_with("(?!") ||
            pattern.starts_with("(?<=") || pattern.starts_with("(?<!")) &&
            pattern.ends_with(")") &&
            pattern.matches("(?").count() == 1 {
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

        while reader.read_line(&mut line_buffer)? > 0 {
            line_number += 1;
            self.stats.lines_read += 1;
            self.stats.bytes_processed += line_buffer.len() as u64;

            // Check for lines exceeding the maximum length
            if max_line_length > 0 && line_buffer.len() > max_line_length {
                match handle_long_line(&mut line_buffer, line_number, max_line_length, max_line_action) {
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

            if use_context {
                // Handle context-aware output
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
            } else {
                // No context - simple output
                if let Some(output) = processed_line {
                    writer.write_all(output.as_bytes())?;
                    self.write_line_ending(&mut writer, &output, line_ending)?;
                }
            }

            line_buffer.clear();
        }

        result.lines_processed = line_number;

        debug!(
            "Processing complete: {} lines, {} matches, {} transformations",
            result.lines_processed,
            result.matches_found,
            result.transformations_applied
        );

        Ok(result)
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
            let compiled_step = &self.compiled_steps[step_idx];
            let step_start = Instant::now();
            let mut step_result = StepResult::new(
                compiled_step.step_index,
                compiled_step.step_type.clone(),
                format!("{:?}", compiled_step.pattern),
            );

            match compiled_step.step_type.clone() {
                StepType::Substitute => {
                    if let Some(ref replacement) = compiled_step.replacement {
                        let (result, was_modified) = self.apply_substitution(
                            &compiled_step.pattern,
                            &current_line,
                            replacement,
                            compiled_step.is_global,
                            &mut step_result,
                        )?;

                        if was_modified {
                            current_line = result;
                            step_result.add_transformation();
                        }
                    }
                }
                StepType::Filter => {
                    let matches = compiled_step.pattern.is_match(&current_line);
                    if matches {
                        step_result.add_match();
                    }

                    if let Some(ref action) = compiled_step.action {
                        should_output = match action {
                            FilterAction::KeepLine => matches,
                            FilterAction::DropLine => !matches,
                            FilterAction::KeepMatch => matches,
                            FilterAction::DropMatch => !matches,
                        };

                        if !should_output {
                            break;
                        }
                    }
                }
                StepType::Extract => {
                    // Extract all matched content, joined by newlines (or separator if specified)
                    let captures = compiled_step.pattern.captures_iter(&current_line);
                    let matches: Vec<String> = captures
                        .into_iter()
                        .filter_map(|cap| cap.full_match.map(|(_, _, matched)| matched))
                        .collect();

                    if !matches.is_empty() {
                        // For global flag, join all matches; otherwise take first
                        if compiled_step.is_global {
                            for _ in &matches {
                                step_result.add_match();
                            }
                            // Join multiple matches with the replacement string as separator, or newline if not specified
                            let separator = compiled_step.replacement.as_deref().unwrap_or("\t");
                            current_line = matches.join(separator);
                        } else {
                            current_line = matches.into_iter().next().unwrap_or_default();
                            step_result.add_match();
                        }
                        step_result.add_transformation();
                    }
                }
                StepType::Validate => {
                    let is_valid = compiled_step.pattern.is_match(&current_line);
                    if !is_valid {
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
                }
                StepType::Transform => {
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
                    // Cross-line state machine: track blocks between trigger and until patterns
                    let is_in_block = self.block_states[step_idx];

                    // Check for block boundaries
                    let trigger_matches = compiled_step.pattern.is_match(&current_line);
                    let until_matches = compiled_step
                        .until_pattern
                        .as_ref()
                        .map(|p| p.is_match(&current_line))
                        .unwrap_or(false);

                    // State transitions
                    if !is_in_block && trigger_matches {
                        // Enter block on trigger pattern
                        self.block_states[step_idx] = true;
                        step_result.add_match();
                    } else if is_in_block && until_matches {
                        // Exit block on until pattern
                        self.block_states[step_idx] = false;
                    }

                    // Apply block action if we're inside the block (including trigger/until lines)
                    let process_line = is_in_block || trigger_matches;
                    if process_line {
                        if let Some(ref action) = compiled_step.block_action {
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
                                BlockAction::SubstituteInBlock { pattern, replacement } => {
                                    // Apply substitution only within block
                                    if let Ok(sub_pattern) =
                                        regex::Regex::new(pattern)
                                    {
                                        let new_line =
                                            sub_pattern.replace_all(&current_line, replacement.as_str());
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
                            }
                        }
                    } else if matches!(
                        compiled_step.block_action,
                        Some(BlockAction::KeepBlock)
                    ) {
                        // KeepBlock: drop lines outside blocks
                        should_output = false;
                        break;
                    }
                }
            }

            let elapsed = step_start.elapsed().as_millis() as u64;
            step_result.set_processing_time(elapsed);
            self.stats
                .step_timings
                .insert(compiled_step.step_index, elapsed);
            result.add_step_result(step_result);
        }

        if should_output {
            Ok(Some(current_line))
        } else {
            Ok(None)
        }
    }

    /// Applies substitution and returns (result, was_modified) to avoid cloning for comparison.
    fn apply_substitution(
        &self,
        pattern: &CompiledPattern,
        input: &str,
        replacement: &str,
        is_global: bool,
        step_result: &mut StepResult,
    ) -> Result<(String, bool)> {
        // Use single-pass methods that count while replacing
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
                crate::plugin::PluginRegistry::global().execute(name, matched, args).unwrap_or_else(|e| {
                    eprintln!("Plugin error: {}", e);
                    matched.to_string()
                })
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
            TransformAction::FpeEncrypt { key, tweak, radix } => {
                fpe_encrypt(matched, key, tweak, radix).unwrap_or_else(|e| {
                    eprintln!("FPE encrypt error: {}", e);
                    matched.to_string()
                })
            }
            #[cfg(feature = "fpe")]
            TransformAction::FpeDecrypt { key, tweak, radix } => {
                fpe_decrypt(matched, key, tweak, radix).unwrap_or_else(|e| {
                    eprintln!("FPE decrypt error: {}", e);
                    matched.to_string()
                })
            }
            TransformAction::MaskDeterministic {
                seed,
                preserve_prefix,
                preserve_suffix,
                mask_char,
            } => mask_deterministic(matched, seed, *preserve_prefix, *preserve_suffix, *mask_char),
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
                let transformed = Self::transform_match(&matched, action, extra_text, shell_timeout);
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
            return Err(format!(
                "Character '{}' not in radix '{}'",
                c, radix
            ));
        }
    }

    if numerals.len() < 2 {
        return Err("Input must have at least 2 characters for FPE".to_string());
    }

    // Create FF1 cipher
    let ff1 = FF1::<aes::Aes256>::new(&key_bytes, radix_size as u32)
        .map_err(|e| format!("FF1 initialization error: {:?}", e))?;

    // Encrypt
    let bns = BinaryNumeralString::from_bytes_le(&numerals.iter().map(|&n| n as u8).collect::<Vec<_>>());
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
            return Err(format!(
                "Character '{}' not in radix '{}'",
                c, radix
            ));
        }
    }

    if numerals.len() < 2 {
        return Err("Input must have at least 2 characters for FPE".to_string());
    }

    // Create FF1 cipher
    let ff1 = FF1::<aes::Aes256>::new(&key_bytes, radix_size as u32)
        .map_err(|e| format!("FF1 initialization error: {:?}", e))?;

    // Decrypt
    let bns = BinaryNumeralString::from_bytes_le(&numerals.iter().map(|&n| n as u8).collect::<Vec<_>>());
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
        assert_eq!(result.transformations_applied, 1, "One substitution step applied");
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
                action: Some(FilterAction::KeepLine),
                transform: None,
                flags: None,
                description: None,
                enabled: Some(true),
                ..Default::default()
            }],
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
                action: Some(FilterAction::KeepLine),
                transform: None,
                flags: None,
                description: None,
                enabled: Some(true),
                ..Default::default()
            }],
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
                action: Some(FilterAction::KeepLine),
                transform: None,
                flags: None,
                description: None,
                enabled: Some(true),
                ..Default::default()
            }],
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
        assert!(
            lines.len() >= 3,
            "Expected at least 3 CRLF-delimited lines"
        );
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
            result.unwrap_err().to_string().contains("exceeds maximum length"),
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
                pattern: r"^BEGIN$".to_string(),
                replacement: None,
                action: None,
                transform: None,
                flags: None,
                description: None,
                enabled: Some(true),
                until: Some(r"^END$".to_string()),
                block_action: Some(BlockAction::KeepBlock),
                block_context: None,
            }],
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
        assert!(!output_str.contains("outside 1"), "Should not include outside 1");
        assert!(!output_str.contains("outside 2"), "Should not include outside 2");
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
                pattern: r"^BEGIN$".to_string(),
                replacement: None,
                action: None,
                transform: None,
                flags: None,
                description: None,
                enabled: Some(true),
                until: Some(r"^END$".to_string()),
                block_action: Some(BlockAction::DropBlock),
                block_context: None,
            }],
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
                pattern: r"^START$".to_string(),
                replacement: None,
                action: None,
                transform: None,
                flags: None,
                description: None,
                enabled: Some(true),
                until: Some(r"^STOP$".to_string()),
                block_action: Some(BlockAction::MarkBlock {
                    marker: ">>> ".to_string(),
                }),
                block_context: None,
            }],
        };

        let mut processor = StreamProcessor::new(config).unwrap();

        let input = "normal\nSTART\nmarked line\nSTOP\nnormal again\n";
        let reader = Cursor::new(input);
        let mut output = Vec::new();

        processor.process_stream(reader, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        // Lines inside block should be marked
        assert!(output_str.contains(">>> START"), "START should be marked");
        assert!(output_str.contains(">>> marked line"), "marked line should be marked");
        // Lines outside block should not be marked
        assert!(output_str.contains("\nnormal\n") || output_str.starts_with("normal\n"),
                "normal should not be marked");
    }

    #[test]
    fn test_mask_deterministic_helper() {
        // Test the deterministic masking helper function
        let result1 = mask_deterministic("123456789", "seed123", 0, 0, '*');
        let result2 = mask_deterministic("123456789", "seed123", 0, 0, '*');
        // Same input + seed should produce same output
        assert_eq!(result1, result2, "Deterministic masking should be consistent");

        // Different seed should produce different output
        let result3 = mask_deterministic("123456789", "different_seed", 0, 0, '*');
        assert_ne!(result1, result3, "Different seeds should produce different results");

        // Test prefix preservation
        let result4 = mask_deterministic("123456789", "seed123", 4, 0, 'X');
        assert!(result4.starts_with("1234"), "Should preserve first 4 chars: {}", result4);
        assert!(result4.chars().skip(4).all(|c| c == 'X'), "Rest should be masked with X");

        // Test suffix preservation
        let result5 = mask_deterministic("123456789", "seed123", 0, 4, 'X');
        assert!(result5.ends_with("6789"), "Should preserve last 4 chars: {}", result5);

        // Test both prefix and suffix
        let result6 = mask_deterministic("1234-5678-9012", "seed", 4, 4, '*');
        assert!(result6.starts_with("1234"), "Should preserve prefix");
        assert!(result6.ends_with("9012"), "Should preserve suffix: {}", result6);
    }
}
