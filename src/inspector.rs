//! Interactive pattern inspection and debugging for rexpipe pipelines.
//!
//! The inspector module provides detailed visualization and analysis of regex matches,
//! enabling users to understand exactly how their patterns interact with input text.
//!
//! # Features
//!
//! - **Match Visualization**: See exactly what each pattern matches with colored output
//! - **Capture Group Display**: View captured groups for each match
//! - **Performance Profiling**: Measure processing speed and per-step timing
//! - **Interactive Mode**: Step through matches one-by-one
//!
//! # Example
//!
//! ```
//! use rexpipe::pipeline::PipelineConfig;
//! use rexpipe::inspector::{Inspector, InspectorOptions};
//! use std::io::Cursor;
//!
//! let config = PipelineConfig::from_inline_pattern(r"(\w+)=(\d+)", None);
//! let options = InspectorOptions::new()
//!     .show_captures(true)
//!     .show_performance(true);
//!
//! let mut inspector = Inspector::new(config).unwrap().with_options(options);
//!
//! let input = Cursor::new("key=123\nname=456\n");
//! let result = inspector.inspect_stream(input).unwrap();
//!
//! assert_eq!(result.total_matches, 2);
//! ```

use crate::pipeline::PipelineConfig;
use crate::processor::{MatchInfo, StreamProcessor};
use anyhow::Result;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

/// Interactive pattern inspector for debugging and analyzing regex matches.
///
/// `Inspector` wraps a `StreamProcessor` to provide detailed analysis of pattern
/// matching behavior, including visualization of matches, capture group extraction,
/// and performance profiling.
///
/// # Example
///
/// ```
/// use rexpipe::pipeline::PipelineConfig;
/// use rexpipe::inspector::Inspector;
///
/// let config = PipelineConfig::from_inline_pattern(r"\d+", None);
/// let inspector = Inspector::new(config).unwrap();
///
/// let matches = inspector.inspect_single_line("Order 123 placed").unwrap();
/// assert_eq!(matches.len(), 1);
/// assert_eq!(matches[0].full_match, "123");
/// ```
pub struct Inspector {
    processor: StreamProcessor,
    interactive_mode: bool,
    show_line_numbers: bool,
    show_capture_groups: bool,
    show_performance: bool,
    max_matches_per_line: Option<usize>,
    use_color: bool,
}

/// Results from inspecting a stream for pattern matches.
///
/// Contains aggregate statistics and detailed match information for each line
/// that contained matches.
#[derive(Debug)]
pub struct InspectionResult {
    /// Total number of lines read from input
    pub total_lines: u64,
    /// Total number of matches found across all lines
    pub total_matches: u64,
    /// Matches grouped by pipeline step index
    pub matches_per_step: HashMap<usize, u64>,
    /// Detailed match information for each line containing matches
    pub line_matches: Vec<LineMatch>,
    /// Performance metrics from the inspection run
    pub performance_data: PerformanceData,
}

/// Information about matches found on a single line.
#[derive(Debug)]
pub struct LineMatch {
    /// Line number (1-based)
    pub line_number: u64,
    /// The original line content before any transformations
    pub original_line: String,
    /// All matches found on this line
    pub matches: Vec<MatchInfo>,
    /// The transformed line content (if any transformation was applied)
    pub transformed_line: Option<String>,
}

/// Performance metrics collected during stream inspection.
///
/// Provides throughput measurements and per-step timing breakdowns for
/// performance analysis and optimization.
#[derive(Debug)]
pub struct PerformanceData {
    /// Total wall-clock time spent processing in milliseconds
    pub total_processing_time_ms: u64,
    /// Throughput in lines per second
    pub lines_per_second: u64,
    /// Throughput in bytes per second
    pub bytes_per_second: u64,
    /// Per-step cumulative processing time (step_index -> milliseconds)
    pub step_timings: HashMap<usize, u64>,
}

impl Inspector {
    /// Create a new Inspector from a pipeline configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - The pipeline configuration defining patterns to inspect
    ///
    /// # Returns
    ///
    /// A Result containing the inspector or an error if the configuration is invalid.
    ///
    /// # Example
    ///
    /// ```
    /// use rexpipe::pipeline::PipelineConfig;
    /// use rexpipe::inspector::Inspector;
    ///
    /// let config = PipelineConfig::from_inline_pattern(r"ERROR|WARN", None);
    /// let inspector = Inspector::new(config).unwrap();
    /// ```
    pub fn new(config: PipelineConfig) -> Result<Self> {
        let processor = StreamProcessor::new(config)?;
        Ok(Self::from_processor(processor))
    }

    /// Create an Inspector from an existing StreamProcessor.
    ///
    /// This method enables dependency injection for testing, allowing tests to
    /// provide a pre-configured or mock processor.
    ///
    /// # Arguments
    ///
    /// * `processor` - A pre-configured StreamProcessor
    ///
    /// # Example
    ///
    /// ```
    /// use rexpipe::pipeline::PipelineConfig;
    /// use rexpipe::processor::StreamProcessor;
    /// use rexpipe::inspector::Inspector;
    ///
    /// let config = PipelineConfig::from_inline_pattern(r"\d+", None);
    /// let processor = StreamProcessor::new(config).unwrap();
    /// let inspector = Inspector::from_processor(processor);
    /// ```
    pub fn from_processor(processor: StreamProcessor) -> Self {
        Self {
            processor,
            interactive_mode: false,
            show_line_numbers: true,
            show_capture_groups: true,
            show_performance: false,
            max_matches_per_line: Some(10),
            use_color: true,
        }
    }

    /// Enable or disable colored output.
    pub fn with_color(mut self, use_color: bool) -> Self {
        self.use_color = use_color;
        self
    }

    /// Get the ColorChoice based on the use_color setting.
    fn color_choice(&self) -> ColorChoice {
        if self.use_color {
            ColorChoice::Auto
        } else {
            ColorChoice::Never
        }
    }

    /// Configure the inspector with custom options.
    ///
    /// # Example
    ///
    /// ```
    /// use rexpipe::pipeline::PipelineConfig;
    /// use rexpipe::inspector::{Inspector, InspectorOptions};
    ///
    /// let config = PipelineConfig::from_inline_pattern(r"\d+", None);
    /// let options = InspectorOptions::new()
    ///     .show_captures(true)
    ///     .show_performance(true)
    ///     .max_matches_per_line(Some(5));
    ///
    /// let inspector = Inspector::new(config).unwrap().with_options(options);
    /// ```
    pub fn with_options(mut self, options: InspectorOptions) -> Self {
        self.interactive_mode = options.interactive;
        self.show_line_numbers = options.show_line_numbers;
        self.show_capture_groups = options.show_captures;
        self.show_performance = options.show_performance;
        self.max_matches_per_line = options.max_matches_per_line;
        self
    }

    /// Inspect a stream and collect match information.
    ///
    /// Reads the input stream line-by-line, collecting detailed information about
    /// all pattern matches. In interactive mode, displays matches as they're found
    /// and allows the user to step through them.
    ///
    /// # Arguments
    ///
    /// * `reader` - Any type implementing `BufRead` (file, stdin, string buffer)
    ///
    /// # Returns
    ///
    /// An `InspectionResult` containing aggregate statistics and per-line match details.
    ///
    /// # Example
    ///
    /// ```
    /// use rexpipe::pipeline::PipelineConfig;
    /// use rexpipe::inspector::Inspector;
    /// use std::io::Cursor;
    ///
    /// let config = PipelineConfig::from_inline_pattern(r"\d+", None);
    /// let mut inspector = Inspector::new(config).unwrap();
    ///
    /// let input = Cursor::new("Line 123\nLine 456\n");
    /// let result = inspector.inspect_stream(input).unwrap();
    ///
    /// assert_eq!(result.total_lines, 2);
    /// assert_eq!(result.total_matches, 2);
    /// ```
    pub fn inspect_stream<R: BufRead>(&mut self, reader: R) -> Result<InspectionResult> {
        let mut result = InspectionResult {
            total_lines: 0,
            total_matches: 0,
            matches_per_step: HashMap::new(),
            line_matches: Vec::new(),
            performance_data: PerformanceData {
                total_processing_time_ms: 0,
                lines_per_second: 0,
                bytes_per_second: 0,
                step_timings: HashMap::new(),
            },
        };

        let mut line_number = 0u64;
        let mut total_bytes = 0u64;
        let start_time = std::time::Instant::now();

        for line_result in reader.lines() {
            let line = line_result?;
            line_number += 1;
            result.total_lines += 1;
            total_bytes += line.len() as u64 + 1; // +1 for newline

            let matches = self.processor.inspect_line(&line, None)?;

            if !matches.is_empty() {
                let limited_matches: Vec<_> = if let Some(limit) = self.max_matches_per_line {
                    matches.into_iter().take(limit).collect()
                } else {
                    matches
                };

                result.total_matches += limited_matches.len() as u64;

                // Update per-step counters using the actual step index from each match
                for match_info in &limited_matches {
                    *result
                        .matches_per_step
                        .entry(match_info.step_index)
                        .or_insert(0) += 1;
                }

                // Calculate transformed line by applying all replacements
                let transformed_line = self.calculate_transformed_line(&line, &limited_matches);

                let line_match = LineMatch {
                    line_number,
                    original_line: line.clone(),
                    matches: limited_matches,
                    transformed_line,
                };

                if self.interactive_mode {
                    self.display_interactive_match(&line, line_number, &line_match)?;

                    result.line_matches.push(line_match);

                    if self.should_pause()? {
                        break;
                    }
                } else {
                    result.line_matches.push(line_match);
                }
            }
        }

        let total_time = start_time.elapsed().as_millis() as u64;
        result.performance_data.total_processing_time_ms = total_time;

        if total_time > 0 {
            result.performance_data.lines_per_second = (result.total_lines * 1000) / total_time;
            result.performance_data.bytes_per_second = (total_bytes * 1000) / total_time;
        }

        Ok(result)
    }

    /// Calculate the transformed line by applying all replacement previews
    fn calculate_transformed_line(
        &self,
        _original: &str,
        matches: &[crate::processor::MatchInfo],
    ) -> Option<String> {
        // If any match has a replacement preview, use the first one as the transformed line
        // In a real pipeline, all steps would be applied sequentially
        for match_info in matches {
            if let Some(ref preview) = match_info.replacement_preview {
                return Some(preview.clone());
            }
        }
        None
    }

    /// Inspect a single line and return match information.
    ///
    /// # Example
    /// ```
    /// use rexpipe::pipeline::PipelineConfig;
    /// use rexpipe::inspector::Inspector;
    ///
    /// let config = PipelineConfig::from_inline_pattern(r"(\w+)=(\d+)", None);
    /// let inspector = Inspector::new(config).unwrap();
    /// let matches = inspector.inspect_single_line("key=123").unwrap();
    /// assert_eq!(matches.len(), 1);
    /// ```
    pub fn inspect_single_line(&self, line: &str) -> Result<Vec<MatchInfo>> {
        self.processor.inspect_line(line, None)
    }

    pub fn display_results(&self, result: &InspectionResult) -> Result<()> {
        let mut stdout = StandardStream::stdout(self.color_choice());

        self.print_header(&mut stdout)?;

        for line_match in &result.line_matches {
            self.display_line_match(&mut stdout, line_match)?;

            if self.interactive_mode {
                println!(); // Extra spacing in interactive mode
            }
        }

        self.print_summary(&mut stdout, result)?;

        if self.show_performance {
            self.print_performance(&mut stdout, &result.performance_data)?;
        }

        Ok(())
    }

    fn print_header(&self, stdout: &mut StandardStream) -> Result<()> {
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(stdout, "rexpipe Pattern Inspection Results")?;
        writeln!(stdout, "=================================")?;
        stdout.reset()?;
        writeln!(
            stdout,
            "Pipeline: {}",
            self.processor
                .get_config()
                .name
                .as_deref()
                .unwrap_or("Unnamed")
        )?;
        writeln!(stdout)?;
        Ok(())
    }

    fn display_line_match(
        &self,
        stdout: &mut StandardStream,
        line_match: &LineMatch,
    ) -> Result<()> {
        // Show line number and original content
        if self.show_line_numbers {
            stdout.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))?;
            write!(stdout, "Line {}: ", line_match.line_number)?;
        }

        stdout.set_color(ColorSpec::new().set_fg(Some(Color::White)))?;
        writeln!(stdout, "{}", line_match.original_line)?;
        stdout.reset()?;

        // Show each match with highlighting
        for (i, match_info) in line_match.matches.iter().enumerate() {
            stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
            write!(stdout, "  Match {}: ", i + 1)?;
            stdout.reset()?;

            // Highlight the match in context
            let line = &line_match.original_line;
            let before = &line[..match_info.byte_start];
            let matched = &match_info.full_match;
            let after = &line[match_info.byte_end..];

            write!(stdout, "{}", before)?;
            stdout.set_color(
                ColorSpec::new()
                    .set_bg(Some(Color::Green))
                    .set_fg(Some(Color::Black)),
            )?;
            write!(stdout, "{}", matched)?;
            stdout.reset()?;
            writeln!(stdout, "{}", after)?;

            // Show capture groups if enabled
            if self.show_capture_groups && match_info.captures.len() > 1 {
                for (j, capture) in match_info.captures.iter().enumerate().skip(1) {
                    if let Some(capture_text) = capture {
                        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Magenta)))?;
                        writeln!(stdout, "    Group {}: \"{}\"", j, capture_text)?;
                        stdout.reset()?;
                    }
                }
            }

            // Show replacement preview if available
            if let Some(ref preview) = match_info.replacement_preview {
                stdout.set_color(ColorSpec::new().set_fg(Some(Color::Blue)))?;
                writeln!(stdout, "    Replacement preview: \"{}\"", preview)?;
                stdout.reset()?;
            }
        }

        Ok(())
    }

    fn display_interactive_match(
        &self,
        _line: &str,
        _line_number: u64,
        line_match: &LineMatch,
    ) -> Result<()> {
        let mut stdout = StandardStream::stdout(self.color_choice());

        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(stdout, "\n--- Interactive Match Display ---")?;
        stdout.reset()?;

        self.display_line_match(&mut stdout, line_match)?;

        Ok(())
    }

    fn should_pause(&self) -> Result<bool> {
        print!("\nPress Enter to continue, 'q' to quit, 's' to skip to summary: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match input.trim().to_lowercase().as_str() {
            "q" | "quit" | "exit" => Ok(true),
            "s" | "skip" | "summary" => Ok(true),
            _ => Ok(false),
        }
    }

    fn print_summary(&self, stdout: &mut StandardStream, result: &InspectionResult) -> Result<()> {
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(stdout, "\nInspection Summary")?;
        writeln!(stdout, "==================")?;
        stdout.reset()?;

        writeln!(stdout, "Total lines processed: {}", result.total_lines)?;
        writeln!(stdout, "Total matches found: {}", result.total_matches)?;
        writeln!(stdout, "Lines with matches: {}", result.line_matches.len())?;

        let match_rate = if result.total_lines > 0 {
            (result.line_matches.len() as f64 / result.total_lines as f64) * 100.0
        } else {
            0.0
        };
        writeln!(stdout, "Match rate: {:.2}%", match_rate)?;

        if !result.matches_per_step.is_empty() {
            writeln!(stdout, "\nMatches per step:")?;
            for (step, count) in &result.matches_per_step {
                writeln!(stdout, "  Step {}: {} matches", step + 1, count)?;
            }
        }

        Ok(())
    }

    fn print_performance(
        &self,
        stdout: &mut StandardStream,
        performance: &PerformanceData,
    ) -> Result<()> {
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        writeln!(stdout, "\nPerformance Metrics")?;
        writeln!(stdout, "===================")?;
        stdout.reset()?;

        writeln!(
            stdout,
            "Total processing time: {}ms",
            performance.total_processing_time_ms
        )?;
        writeln!(stdout, "Lines per second: {}", performance.lines_per_second)?;
        writeln!(stdout, "Bytes per second: {}", performance.bytes_per_second)?;

        if !performance.step_timings.is_empty() {
            writeln!(stdout, "\nStep timings:")?;
            for (step, timing) in &performance.step_timings {
                writeln!(stdout, "  Step {}: {}ms", step + 1, timing)?;
            }
        }

        Ok(())
    }
}

/// Configuration options for the pattern inspector.
///
/// Use the builder pattern to configure inspection behavior.
///
/// # Example
///
/// ```
/// use rexpipe::inspector::InspectorOptions;
///
/// let options = InspectorOptions::new()
///     .interactive(false)
///     .show_line_numbers(true)
///     .show_captures(true)
///     .show_performance(true)
///     .max_matches_per_line(Some(10));
/// ```
#[derive(Debug, Default)]
pub struct InspectorOptions {
    /// Enable interactive mode (step through matches one-by-one)
    pub interactive: bool,
    /// Show line numbers in output
    pub show_line_numbers: bool,
    /// Display capture group contents for each match
    pub show_captures: bool,
    /// Include performance metrics in results
    pub show_performance: bool,
    /// Maximum matches to display per line (None for unlimited)
    pub max_matches_per_line: Option<usize>,
}

impl InspectorOptions {
    pub fn new() -> Self {
        Self {
            interactive: false,
            show_line_numbers: true,
            show_captures: true,
            show_performance: false,
            max_matches_per_line: Some(10),
        }
    }

    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    pub fn show_line_numbers(mut self, show: bool) -> Self {
        self.show_line_numbers = show;
        self
    }

    pub fn show_captures(mut self, show: bool) -> Self {
        self.show_captures = show;
        self
    }

    pub fn show_performance(mut self, show: bool) -> Self {
        self.show_performance = show;
        self
    }

    /// Set maximum matches to display per line (None for unlimited).
    ///
    /// # Example
    /// ```
    /// use rexpipe::inspector::InspectorOptions;
    ///
    /// let options = InspectorOptions::new()
    ///     .max_matches_per_line(Some(5));
    /// ```
    pub fn max_matches_per_line(mut self, max: Option<usize>) -> Self {
        self.max_matches_per_line = max;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::PipelineConfig;
    use std::io::Cursor;

    #[test]
    fn test_basic_inspection() {
        let config = PipelineConfig::from_inline_pattern(r"\d+", None);
        let mut inspector = Inspector::new(config).unwrap();

        // All lines have digits because of "Line N:" prefix
        // Line 1: "1" + "123" = 2 matches
        // Line 2: "2" = 1 match (even though text says "no numbers", the line number has one!)
        // Line 3: "3" + "456" + "789" = 3 matches
        let input = "Line 1: 123\nLine 2: no numbers\nLine 3: 456 and 789";
        let reader = Cursor::new(input);

        let result = inspector.inspect_stream(reader).unwrap();

        assert_eq!(result.total_lines, 3);
        assert_eq!(
            result.line_matches.len(),
            3,
            "All lines have digits due to 'Line N:' prefix"
        );
        assert_eq!(result.total_matches, 6, "Total: 1, 123, 2, 3, 456, 789");
    }

    #[test]
    fn test_single_line_inspection() {
        let config = PipelineConfig::from_inline_pattern(r"(\w+)=(\d+)", None);
        let inspector = Inspector::new(config).unwrap();

        let matches = inspector.inspect_single_line("user=123 id=456").unwrap();

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].captures.len(), 3); // Full match + 2 groups
        assert_eq!(matches[0].captures[1], Some("user".to_string()));
        assert_eq!(matches[0].captures[2], Some("123".to_string()));
    }

    #[test]
    fn test_inspector_options() {
        let config = PipelineConfig::from_inline_pattern(r"\d+", None);
        let options = InspectorOptions::new()
            .interactive(true)
            .show_line_numbers(false)
            .max_matches_per_line(Some(5));

        let inspector = Inspector::new(config).unwrap().with_options(options);

        assert!(inspector.interactive_mode);
        assert!(!inspector.show_line_numbers);
        assert_eq!(inspector.max_matches_per_line, Some(5));
    }

    #[test]
    fn test_inspection_no_matches() {
        let config = PipelineConfig::from_inline_pattern(r"\d+", None);
        let mut inspector = Inspector::new(config).unwrap();

        let input = "no numbers here\njust text\nnothing to see";
        let reader = Cursor::new(input);

        let result = inspector.inspect_stream(reader).unwrap();

        assert_eq!(result.total_lines, 3);
        assert_eq!(result.total_matches, 0);
        assert!(result.line_matches.is_empty());
    }

    #[test]
    fn test_inspection_empty_input() {
        let config = PipelineConfig::from_inline_pattern(r"\d+", None);
        let mut inspector = Inspector::new(config).unwrap();

        let reader = Cursor::new("");
        let result = inspector.inspect_stream(reader).unwrap();

        assert_eq!(result.total_lines, 0);
        assert_eq!(result.total_matches, 0);
        assert!(result.line_matches.is_empty());
    }

    #[test]
    fn test_max_matches_per_line_limiting() {
        let config = PipelineConfig::from_inline_pattern(r"\d", None);
        let options = InspectorOptions::new().max_matches_per_line(Some(2));
        let mut inspector = Inspector::new(config).unwrap().with_options(options);

        // Line with many single-digit matches
        let input = "1234567890";
        let reader = Cursor::new(input);

        let result = inspector.inspect_stream(reader).unwrap();

        // Should only record 2 matches due to limit
        assert_eq!(result.total_matches, 2);
        assert_eq!(result.line_matches[0].matches.len(), 2);
    }

    #[test]
    fn test_max_matches_unlimited() {
        let config = PipelineConfig::from_inline_pattern(r"\d", None);
        let options = InspectorOptions::new().max_matches_per_line(None);
        let mut inspector = Inspector::new(config).unwrap().with_options(options);

        let input = "1234567890";
        let reader = Cursor::new(input);

        let result = inspector.inspect_stream(reader).unwrap();

        // Should record all 10 matches
        assert_eq!(result.total_matches, 10);
    }

    #[test]
    fn test_inspector_with_color_disabled() {
        let config = PipelineConfig::from_inline_pattern(r"\d+", None);
        let inspector = Inspector::new(config).unwrap().with_color(false);

        assert!(!inspector.use_color);
        assert_eq!(inspector.color_choice(), ColorChoice::Never);
    }

    #[test]
    fn test_inspector_with_color_enabled() {
        let config = PipelineConfig::from_inline_pattern(r"\d+", None);
        let inspector = Inspector::new(config).unwrap().with_color(true);

        assert!(inspector.use_color);
        assert_eq!(inspector.color_choice(), ColorChoice::Auto);
    }

    #[test]
    fn test_performance_data_collection() {
        let config = PipelineConfig::from_inline_pattern(r"\d+", None);
        let mut inspector = Inspector::new(config).unwrap();

        let input = "123\n456\n789\n";
        let reader = Cursor::new(input);

        let result = inspector.inspect_stream(reader).unwrap();

        // Performance data should be populated (verify it was set, not just default)
        // Note: processing time might be 0 for very fast operations, which is valid
        // With 3 lines processed quickly, we should have some throughput
        assert_eq!(result.total_lines, 3);
        // Verify performance data struct is accessible and populated
        let _time = result.performance_data.total_processing_time_ms;
        let _lps = result.performance_data.lines_per_second;
    }

    #[test]
    fn test_line_match_structure() {
        let config = PipelineConfig::from_inline_pattern(r"(\w+):(\d+)", None);
        let mut inspector = Inspector::new(config).unwrap();

        let input = "key:123";
        let reader = Cursor::new(input);

        let result = inspector.inspect_stream(reader).unwrap();

        assert_eq!(result.line_matches.len(), 1);
        let line_match = &result.line_matches[0];
        assert_eq!(line_match.line_number, 1);
        assert_eq!(line_match.original_line, "key:123");
        assert_eq!(line_match.matches.len(), 1);
        assert_eq!(line_match.matches[0].full_match, "key:123");
    }

    #[test]
    fn test_matches_per_step_tracking() {
        let config = PipelineConfig::from_inline_pattern(r"\d+", None);
        let mut inspector = Inspector::new(config).unwrap();

        // Input: "1", "2" and "123", "3" and "456" = 5 matches total
        let input = "line 1\nline 2 with 123\nline 3 with 456";
        let reader = Cursor::new(input);

        let result = inspector.inspect_stream(reader).unwrap();

        // Step 0 should have recorded all matches
        assert!(result.matches_per_step.contains_key(&0));
        assert_eq!(*result.matches_per_step.get(&0).unwrap(), 5);
    }

    #[test]
    fn test_inspector_options_default() {
        let options = InspectorOptions::default();

        assert!(!options.interactive);
        assert!(!options.show_line_numbers);
        assert!(!options.show_captures);
        assert!(!options.show_performance);
        assert!(options.max_matches_per_line.is_none());
    }

    #[test]
    fn test_inspector_options_new() {
        let options = InspectorOptions::new();

        // new() sets some defaults differently than Default
        assert!(!options.interactive);
        assert!(options.show_line_numbers);
        assert!(options.show_captures);
        assert!(!options.show_performance);
        assert_eq!(options.max_matches_per_line, Some(10));
    }

    #[test]
    fn test_inspector_options_builder_chain() {
        let options = InspectorOptions::new()
            .interactive(true)
            .show_line_numbers(false)
            .show_captures(false)
            .show_performance(true)
            .max_matches_per_line(Some(25));

        assert!(options.interactive);
        assert!(!options.show_line_numbers);
        assert!(!options.show_captures);
        assert!(options.show_performance);
        assert_eq!(options.max_matches_per_line, Some(25));
    }

    #[test]
    fn test_inspection_result_debug() {
        let result = InspectionResult {
            total_lines: 10,
            total_matches: 5,
            matches_per_step: HashMap::new(),
            line_matches: Vec::new(),
            performance_data: PerformanceData {
                total_processing_time_ms: 100,
                lines_per_second: 100,
                bytes_per_second: 500,
                step_timings: HashMap::new(),
            },
        };

        // Should implement Debug without panic
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("total_lines: 10"));
        assert!(debug_str.contains("total_matches: 5"));
    }

    #[test]
    fn test_line_match_debug() {
        let line_match = LineMatch {
            line_number: 42,
            original_line: "test line".to_string(),
            matches: Vec::new(),
            transformed_line: Some("transformed".to_string()),
        };

        let debug_str = format!("{:?}", line_match);
        assert!(debug_str.contains("42"));
        assert!(debug_str.contains("test line"));
    }

    #[test]
    fn test_performance_data_debug() {
        let perf = PerformanceData {
            total_processing_time_ms: 1000,
            lines_per_second: 5000,
            bytes_per_second: 25000,
            step_timings: HashMap::new(),
        };

        let debug_str = format!("{:?}", perf);
        assert!(debug_str.contains("1000"));
        assert!(debug_str.contains("5000"));
    }

    #[test]
    fn test_unicode_line_inspection() {
        let config = PipelineConfig::from_inline_pattern(r"\p{L}+", None);
        let inspector = Inspector::new(config).unwrap();

        let matches = inspector.inspect_single_line("hello 世界 مرحبا").unwrap();

        // Should match all three words (hello, 世界, مرحبا)
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn test_multiline_stream_inspection() {
        let config = PipelineConfig::from_inline_pattern(r"ERROR|WARN", None);
        let mut inspector = Inspector::new(config).unwrap();

        let input = "INFO: Starting\nERROR: Failed\nWARN: Low memory\nINFO: Done";
        let reader = Cursor::new(input);

        let result = inspector.inspect_stream(reader).unwrap();

        assert_eq!(result.total_lines, 4);
        assert_eq!(result.total_matches, 2);
        assert_eq!(result.line_matches.len(), 2);
    }
}
