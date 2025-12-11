use crate::pipeline::{PipelineConfig, PipelineSettings, StepType, FilterAction, RegexFlag, PipelineResult, StepResult, PipelineError, ErrorType};
use regex::{Regex, RegexBuilder};
use std::io::{BufRead, Write};
use std::collections::HashMap;
use std::time::Instant;

#[cfg(feature = "pcre")]
use fancy_regex::Regex as FancyRegex;

pub struct StreamProcessor {
    config: PipelineConfig,
    compiled_steps: Vec<CompiledStep>,
    stats: ProcessorStats,
}

/// Abstraction over different regex engines
#[derive(Clone)]
pub enum CompiledPattern {
    /// Standard Rust regex (fast, but no lookahead/lookbehind)
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

    pub fn find_iter<'a>(&'a self, text: &'a str) -> Vec<(usize, usize, String)> {
        match self {
            CompiledPattern::Standard(re) => {
                re.find_iter(text)
                    .map(|m| (m.start(), m.end(), m.as_str().to_string()))
                    .collect()
            }
            CompiledPattern::Fixed(s) => {
                text.match_indices(s)
                    .map(|(start, matched)| (start, start + matched.len(), matched.to_string()))
                    .collect()
            }
            #[cfg(feature = "pcre")]
            CompiledPattern::Pcre(re) => {
                re.find_iter(text)
                    .filter_map(|m| m.ok())
                    .map(|m| (m.start(), m.end(), m.as_str().to_string()))
                    .collect()
            }
        }
    }

    pub fn captures_iter<'a>(&'a self, text: &'a str) -> Vec<CaptureGroup> {
        match self {
            CompiledPattern::Standard(re) => {
                re.captures_iter(text)
                    .map(|caps| {
                        let groups: Vec<Option<String>> = (0..caps.len())
                            .map(|i| caps.get(i).map(|m| m.as_str().to_string()))
                            .collect();
                        let full_match = caps.get(0).map(|m| (m.start(), m.end(), m.as_str().to_string()));
                        CaptureGroup { groups, full_match }
                    })
                    .collect()
            }
            CompiledPattern::Fixed(s) => {
                text.match_indices(s)
                    .map(|(start, matched)| CaptureGroup {
                        groups: vec![Some(matched.to_string())],
                        full_match: Some((start, start + matched.len(), matched.to_string())),
                    })
                    .collect()
            }
            #[cfg(feature = "pcre")]
            CompiledPattern::Pcre(re) => {
                re.captures_iter(text)
                    .filter_map(|caps| caps.ok())
                    .map(|caps| {
                        let groups: Vec<Option<String>> = (0..caps.len())
                            .map(|i| caps.get(i).map(|m| m.as_str().to_string()))
                            .collect();
                        let full_match = caps.get(0).map(|m| (m.start(), m.end(), m.as_str().to_string()));
                        CaptureGroup { groups, full_match }
                    })
                    .collect()
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct CaptureGroup {
    pub groups: Vec<Option<String>>,
    pub full_match: Option<(usize, usize, String)>,
}

struct CompiledStep {
    step_index: usize,
    pattern: CompiledPattern,
    replacement: Option<String>,
    action: Option<FilterAction>,
    step_type: StepType,
    is_global: bool,
}

#[derive(Debug, Default)]
pub struct ProcessorStats {
    pub lines_read: u64,
    pub bytes_processed: u64,
    pub processing_start: Option<Instant>,
    pub step_timings: HashMap<usize, u64>,
}

#[derive(Debug, Clone)]
pub struct MatchInfo {
    pub line_number: u64,
    pub byte_start: usize,
    pub byte_end: usize,
    pub full_match: String,
    pub captures: Vec<Option<String>>,
    pub replacement_preview: Option<String>,
}

impl StreamProcessor {
    pub fn new(config: PipelineConfig) -> Result<Self, Box<dyn std::error::Error>> {
        if let Err(validation_errors) = config.validate() {
            return Err(format!("Pipeline validation failed: {}", validation_errors.join("; ")).into());
        }

        let compiled_steps = Self::compile_steps(&config)?;

        Ok(Self {
            config,
            compiled_steps,
            stats: ProcessorStats::default(),
        })
    }

    fn compile_steps(config: &PipelineConfig) -> Result<Vec<CompiledStep>, Box<dyn std::error::Error>> {
        let mut compiled_steps = Vec::new();
        let settings = &config.settings;

        for (index, step) in config.enabled_steps().enumerate() {
            let is_global = step.flags.as_ref()
                .map(|f| f.iter().any(|flag| matches!(flag, RegexFlag::Global)))
                .unwrap_or(false);

            let pattern = Self::build_pattern(&step.pattern, &step.flags, settings)?;
            let replacement = step.replacement.clone();

            compiled_steps.push(CompiledStep {
                step_index: index,
                pattern,
                replacement,
                action: step.action.clone(),
                step_type: step.step_type.clone(),
                is_global,
            });
        }

        Ok(compiled_steps)
    }

    fn build_pattern(
        pattern: &str,
        flags: &Option<Vec<RegexFlag>>,
        settings: &PipelineSettings,
    ) -> Result<CompiledPattern, Box<dyn std::error::Error>> {
        // Fixed string mode - no regex interpretation
        if settings.fixed_strings {
            return Ok(CompiledPattern::Fixed(pattern.to_string()));
        }

        // PCRE mode - use fancy-regex for advanced features
        #[cfg(feature = "pcre")]
        if settings.pcre_mode {
            let re = FancyRegex::new(pattern)?;
            return Ok(CompiledPattern::Pcre(re));
        }

        #[cfg(not(feature = "pcre"))]
        if settings.pcre_mode {
            return Err("PCRE mode requested but the 'pcre' feature is not enabled. Rebuild with --features pcre".into());
        }

        // Standard regex mode
        let regex = Self::build_regex(pattern, flags)?;
        Ok(CompiledPattern::Standard(regex))
    }

    fn build_regex(pattern: &str, flags: &Option<Vec<RegexFlag>>) -> Result<Regex, regex::Error> {
        let mut builder = RegexBuilder::new(pattern);

        if let Some(flags) = flags {
            for flag in flags {
                match flag {
                    RegexFlag::Global => {}, // Global is handled in processing, not compilation
                    RegexFlag::CaseInsensitive => { builder.case_insensitive(true); },
                    RegexFlag::Multiline => { builder.multi_line(true); },
                    RegexFlag::DotAll => { builder.dot_matches_new_line(true); },
                    RegexFlag::Unicode => { builder.unicode(true); },
                    RegexFlag::Extended => { builder.ignore_whitespace(true); },
                }
            }
        }

        builder.build()
    }

    pub fn process_stream<R: BufRead, W: Write>(
        &mut self,
        mut reader: R,
        mut writer: W,
    ) -> Result<PipelineResult, Box<dyn std::error::Error>> {
        self.stats.processing_start = Some(Instant::now());
        let mut result = PipelineResult::new();
        let mut line_buffer = String::new();
        let mut line_number = 0u64;

        while reader.read_line(&mut line_buffer)? > 0 {
            line_number += 1;
            self.stats.lines_read += 1;
            self.stats.bytes_processed += line_buffer.len() as u64;

            let processed_line = self.process_line(&line_buffer, line_number, &mut result)?;
            
            if let Some(output) = processed_line {
                writer.write_all(output.as_bytes())?;
                if !output.ends_with('\n') {
                    writer.write_all(b"\n")?;
                }
            }

            line_buffer.clear();
        }

        result.lines_processed = line_number;
        Ok(result)
    }

    fn process_line(
        &mut self,
        line: &str,
        line_number: u64,
        result: &mut PipelineResult,
    ) -> Result<Option<String>, Box<dyn std::error::Error>> {
        let mut current_line = line.trim_end_matches('\n').to_string();
        let mut should_output = true;

        for compiled_step in &self.compiled_steps {
            let step_start = Instant::now();
            let mut step_result = StepResult::new(
                compiled_step.step_index,
                compiled_step.step_type.clone(),
                format!("{:?}", compiled_step.pattern),
            );

            match compiled_step.step_type {
                StepType::Substitute => {
                    if let Some(ref replacement) = compiled_step.replacement {
                        let original = current_line.clone();
                        current_line = self.apply_substitution(
                            &compiled_step.pattern,
                            &current_line,
                            replacement,
                            compiled_step.is_global,
                            &mut step_result,
                        )?;

                        if current_line != original {
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
                    // Extract matched content only
                    let captures = compiled_step.pattern.captures_iter(&current_line);
                    if let Some(cap) = captures.first() {
                        if let Some((_, _, matched)) = &cap.full_match {
                            current_line = matched.clone();
                            step_result.add_match();
                            step_result.add_transformation();
                        }
                    }
                }
                StepType::Validate => {
                    let is_valid = compiled_step.pattern.is_match(&current_line);
                    if !is_valid {
                        result.add_error(PipelineError::new(
                            compiled_step.step_index,
                            line_number,
                            ErrorType::PatternMatch,
                            "Line failed validation".to_string(),
                        ).with_context(current_line.clone()));
                        should_output = false;
                        break;
                    }
                }
                StepType::Transform => {
                    // Custom transformation logic would go here
                    // For now, just check if pattern matches
                    if compiled_step.pattern.is_match(&current_line) {
                        step_result.add_match();
                    }
                }
            }

            let elapsed = step_start.elapsed().as_millis() as u64;
            step_result.set_processing_time(elapsed);
            self.stats.step_timings.insert(compiled_step.step_index, elapsed);
            result.add_step_result(step_result);
        }

        if should_output {
            Ok(Some(current_line))
        } else {
            Ok(None)
        }
    }

    fn apply_substitution(
        &self,
        pattern: &CompiledPattern,
        input: &str,
        replacement: &str,
        is_global: bool,
        step_result: &mut StepResult,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let result = if is_global {
            pattern.replace_all(input, replacement)
        } else {
            pattern.replace(input, replacement)
        };
        step_result.add_match();
        Ok(result)
    }

    pub fn inspect_line(
        &self,
        line: &str,
        step_index: Option<usize>,
    ) -> Result<Vec<MatchInfo>, Box<dyn std::error::Error>> {
        let mut matches = Vec::new();
        let steps_to_inspect = if let Some(index) = step_index {
            vec![&self.compiled_steps[index]]
        } else {
            self.compiled_steps.iter().collect()
        };

        for step in steps_to_inspect {
            for cap in step.pattern.captures_iter(line) {
                if let Some((start, end, matched)) = cap.full_match {
                    let replacement_preview = if let Some(ref replacement) = step.replacement {
                        Some(step.pattern.replace(line, replacement))
                    } else {
                        None
                    };

                    matches.push(MatchInfo {
                        line_number: 1, // Will be set by caller
                        byte_start: start,
                        byte_end: end,
                        full_match: matched,
                        captures: cap.groups,
                        replacement_preview,
                    });
                }
            }
        }

        Ok(matches)
    }

    pub fn get_stats(&self) -> &ProcessorStats {
        &self.stats
    }

    pub fn get_config(&self) -> &PipelineConfig {
        &self.config
    }

    pub fn performance_report(&self) -> String {
        let total_time = self.stats.processing_start
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
                self.stats.step_timings.values().sum::<u64>() as f64 / self.stats.step_timings.len() as f64
            } else {
                0.0
            }
        )
    }
}

impl ProcessorStats {
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
        assert!(result.transformations_applied > 0);
    }

    #[test]
    fn test_filter_processing() {
        let config = PipelineConfig {
            name: Some("Test Filter".to_string()),
            description: None,
            version: None,
            settings: PipelineSettings::default(),
            step: vec![PipelineStep {
                step_type: StepType::Filter,
                pattern: "keep".to_string(),
                replacement: None,
                action: Some(FilterAction::KeepLine),
                flags: None,
                description: None,
                enabled: Some(true),
            }],
        };

        let mut processor = StreamProcessor::new(config).unwrap();

        let input = "keep this line\ndrop this line\nkeep this too";
        let reader = Cursor::new(input);
        let mut output = Vec::new();
        
        let result = processor.process_stream(reader, &mut output).unwrap();
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
}