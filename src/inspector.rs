use crate::pipeline::PipelineConfig;
use crate::processor::{MatchInfo, StreamProcessor};
use anyhow::Result;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

pub struct Inspector {
    processor: StreamProcessor,
    interactive_mode: bool,
    show_line_numbers: bool,
    show_capture_groups: bool,
    show_performance: bool,
    max_matches_per_line: Option<usize>,
    use_color: bool,
}

#[derive(Debug)]
pub struct InspectionResult {
    pub total_lines: u64,
    pub total_matches: u64,
    pub matches_per_step: HashMap<usize, u64>,
    pub line_matches: Vec<LineMatch>,
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

#[derive(Debug)]
pub struct PerformanceData {
    pub total_processing_time_ms: u64,
    pub lines_per_second: u64,
    pub bytes_per_second: u64,
    pub step_timings: HashMap<usize, u64>,
}

impl Inspector {
    pub fn new(config: PipelineConfig) -> Result<Self> {
        let processor = StreamProcessor::new(config)?;

        Ok(Self {
            processor,
            interactive_mode: false,
            show_line_numbers: true,
            show_capture_groups: true,
            show_performance: false,
            max_matches_per_line: Some(10),
            use_color: true,
        })
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

    pub fn with_options(mut self, options: InspectorOptions) -> Self {
        self.interactive_mode = options.interactive;
        self.show_line_numbers = options.show_line_numbers;
        self.show_capture_groups = options.show_captures;
        self.show_performance = options.show_performance;
        self.max_matches_per_line = options.max_matches_per_line;
        self
    }

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

#[derive(Debug, Default)]
pub struct InspectorOptions {
    pub interactive: bool,
    pub show_line_numbers: bool,
    pub show_captures: bool,
    pub show_performance: bool,
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

        let input = "Line 1: 123\nLine 2: no numbers\nLine 3: 456 and 789";
        let reader = Cursor::new(input);

        let result = inspector.inspect_stream(reader).unwrap();

        assert_eq!(result.total_lines, 3);
        assert!(result.line_matches.len() >= 2); // Lines with matches
        assert!(result.total_matches >= 2); // At least 2 numbers found
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
}
