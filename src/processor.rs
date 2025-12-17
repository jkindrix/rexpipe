use crate::error::{PatternError, ValidationError};
use crate::pipeline::{
    ErrorType, FilterAction, PipelineConfig, PipelineError, PipelineResult, PipelineSettings,
    RegexFlag, StepResult, StepType, TransformAction,
};
use anyhow::{Context, Result};
use regex::{Regex, RegexBuilder};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, Write};
use std::time::Instant;

#[cfg(feature = "pcre")]
use fancy_regex::Regex as FancyRegex;

/// Represents a line with its metadata for context tracking
#[derive(Debug, Clone)]
struct ContextLine {
    line_number: u64,
    content: String,
    #[allow(dead_code)]
    is_match: bool,
}

pub struct StreamProcessor {
    config: PipelineConfig,
    compiled_steps: Vec<CompiledStep>,
    stats: ProcessorStats,
    /// Buffer for before-context lines
    context_before_buffer: VecDeque<ContextLine>,
    /// Counter for remaining after-context lines to output
    after_context_remaining: usize,
    /// Track which lines have been output to avoid duplicates
    last_output_line: u64,
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
    transform_action: Option<TransformAction>,
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
    #[allow(dead_code)]
    pub line_number: u64,
    pub byte_start: usize,
    pub byte_end: usize,
    pub full_match: String,
    pub captures: Vec<Option<String>>,
    pub replacement_preview: Option<String>,
    /// Index of the pipeline step that produced this match
    pub step_index: usize,
}

impl StreamProcessor {
    pub fn new(config: PipelineConfig) -> Result<Self> {
        if let Err(validation_errors) = config.validate() {
            let error = ValidationError::Multiple {
                count: validation_errors.len(),
                errors: validation_errors.join("\n  - "),
            };
            return Err(error).context("Pipeline validation failed");
        }

        let compiled_steps = Self::compile_steps(&config)?;

        Ok(Self {
            config,
            compiled_steps,
            stats: ProcessorStats::default(),
            context_before_buffer: VecDeque::new(),
            after_context_remaining: 0,
            last_output_line: 0,
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

            compiled_steps.push(CompiledStep {
                step_index: index,
                pattern,
                replacement,
                action: step.action.clone(),
                transform_action: step.transform.clone(),
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
    ) -> Result<CompiledPattern> {
        // Fixed string mode - no regex interpretation
        if settings.fixed_strings {
            return Ok(CompiledPattern::Fixed(pattern.to_string()));
        }

        // PCRE mode - use fancy-regex for advanced features
        #[cfg(feature = "pcre")]
        if settings.pcre_mode {
            // Check for ReDoS risks in PCRE mode (which uses backtracking)
            if let Some(warning) = Self::check_redos_risk(pattern, true) {
                eprintln!("{}", warning);
            }

            match FancyRegex::new(pattern) {
                Ok(re) => return Ok(CompiledPattern::Pcre(re)),
                Err(e) => {
                    let error = PatternError::InvalidRegex {
                        pattern: pattern.to_string(),
                        message: e.to_string(),
                    };
                    return Err(error).context(Self::format_regex_error(
                        pattern,
                        &e.to_string(),
                        true,
                    ));
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
        match Self::build_regex(pattern, flags) {
            Ok(regex) => Ok(CompiledPattern::Standard(regex)),
            Err(e) => {
                let error = PatternError::InvalidRegex {
                    pattern: pattern.to_string(),
                    message: e.to_string(),
                };
                Err(error).context(Self::format_regex_error(pattern, &e.to_string(), false))
            }
        }
    }

    /// Format regex compilation errors with helpful suggestions
    fn format_regex_error(pattern: &str, error: &str, is_pcre: bool) -> String {
        let mut msg = format!("Invalid regex pattern: '{}'\n", pattern);
        msg.push_str(&format!("Error: {}\n", error));

        // Add context-specific suggestions
        if error.contains("look")
            || error.contains("(?=")
            || error.contains("(?!")
            || error.contains("(?<")
        {
            if !is_pcre {
                msg.push_str("\nSuggestion: This pattern uses lookahead/lookbehind which requires PCRE mode.\n");
                msg.push_str("Try running with the -P flag: rexpipe -P -p 'pattern' ...\n");
            }
        } else if error.contains("unclosed")
            || error.contains("unbalanced")
            || error.contains("unopened")
        {
            msg.push_str(
                "\nSuggestion: Check for missing closing brackets, parentheses, or braces.\n",
            );
            msg.push_str("Common fixes:\n");
            msg.push_str("  - Ensure all ( have matching )\n");
            msg.push_str("  - Ensure all [ have matching ]\n");
            msg.push_str("  - Ensure all { have matching }\n");
        } else if error.contains("escape") || error.contains("backslash") {
            msg.push_str("\nSuggestion: Check escape sequences. In regex:\n");
            msg.push_str("  - \\d matches digits, \\w matches word chars\n");
            msg.push_str("  - To match literal backslash, use \\\\\n");
            msg.push_str("  - Consider using -F for fixed string matching\n");
        } else if error.contains("quantifier") || error.contains("nothing to repeat") {
            msg.push_str("\nSuggestion: Quantifiers (+, *, ?, {n}) must follow something.\n");
            msg.push_str("  - Invalid: +abc or *test\n");
            msg.push_str("  - Valid: a+bc or te+st\n");
        } else if error.contains("invalid") || error.contains("unknown") {
            msg.push_str(
                "\nSuggestion: Check the regex syntax. If you're trying to match literal text,\n",
            );
            msg.push_str("consider using -F for fixed string mode.\n");
        }

        // General tips
        msg.push_str("\nTips:\n");
        msg.push_str("  - Use --inspect to test pattern matching before processing\n");
        msg.push_str("  - Use -F for literal string matching (no regex interpretation)\n");

        msg
    }

    /// Default regex size limit (10MB) to prevent ReDoS via compilation complexity
    const DEFAULT_REGEX_SIZE_LIMIT: usize = 10 * 1024 * 1024;

    /// Maximum pattern length before warning (patterns longer than this may be slow)
    const PATTERN_LENGTH_WARNING: usize = 1000;

    fn build_regex(pattern: &str, flags: &Option<Vec<RegexFlag>>) -> Result<Regex, regex::Error> {
        let mut builder = RegexBuilder::new(pattern);

        // Apply ReDoS protection via size limits
        // The Rust regex crate already guarantees O(m * n) linear time matching,
        // but we add size limits to prevent compilation DoS attacks
        builder.size_limit(Self::DEFAULT_REGEX_SIZE_LIMIT);

        // Also limit DFA size to prevent memory exhaustion
        builder.dfa_size_limit(Self::DEFAULT_REGEX_SIZE_LIMIT);

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

    /// Check pattern for potential ReDoS vulnerabilities (primarily for PCRE mode)
    /// Returns a warning message if the pattern looks potentially dangerous
    #[allow(dead_code)]
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
        let repetition_re = regex::Regex::new(r"\{(\d+)\}").unwrap();
        for cap in repetition_re.captures_iter(pattern) {
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

        while reader.read_line(&mut line_buffer)? > 0 {
            line_number += 1;
            self.stats.lines_read += 1;
            self.stats.bytes_processed += line_buffer.len() as u64;

            let processed_line = self.process_line(&line_buffer, line_number, &mut result)?;
            let line_content = line_buffer.trim_end_matches('\n').to_string();
            let is_match = processed_line.is_some();

            if use_context {
                // Handle context-aware output
                if let Some(output) = processed_line {
                    // This line matched - output before-context, then this line
                    // Output before-context lines that haven't been output yet
                    for ctx_line in self.context_before_buffer.iter() {
                        if ctx_line.line_number > self.last_output_line {
                            self.write_context_line(&mut writer, ctx_line, false)?;
                            self.last_output_line = ctx_line.line_number;
                        }
                    }

                    // Output the matching line
                    if line_number > self.last_output_line {
                        writer.write_all(output.as_bytes())?;
                        if !output.ends_with('\n') {
                            writer.write_all(b"\n")?;
                        }
                        self.last_output_line = line_number;
                    }

                    // Reset after-context counter
                    self.after_context_remaining = context_after;
                } else if self.after_context_remaining > 0 {
                    // No match, but we're in after-context mode
                    if line_number > self.last_output_line {
                        writer.write_all(line_content.as_bytes())?;
                        writer.write_all(b"\n")?;
                        self.last_output_line = line_number;
                    }
                    self.after_context_remaining -= 1;
                }

                // Update before-context buffer
                self.context_before_buffer.push_back(ContextLine {
                    line_number,
                    content: line_content,
                    is_match,
                });

                // Keep only the needed number of before-context lines
                while self.context_before_buffer.len() > context_before {
                    self.context_before_buffer.pop_front();
                }
            } else {
                // No context - simple output
                if let Some(output) = processed_line {
                    writer.write_all(output.as_bytes())?;
                    if !output.ends_with('\n') {
                        writer.write_all(b"\n")?;
                    }
                }
            }

            line_buffer.clear();
        }

        result.lines_processed = line_number;
        Ok(result)
    }

    /// Write a context line with optional separator
    fn write_context_line<W: Write>(
        &self,
        writer: &mut W,
        ctx_line: &ContextLine,
        _is_separator: bool,
    ) -> Result<()> {
        writer.write_all(ctx_line.content.as_bytes())?;
        writer.write_all(b"\n")?;
        Ok(())
    }

    fn process_line(
        &mut self,
        line: &str,
        line_number: u64,
        result: &mut PipelineResult,
    ) -> Result<Option<String>> {
        let mut current_line = line.trim_end_matches('\n').to_string();
        let mut should_output = true;
        let line_start = Instant::now();
        let timeout_ms = self.config.settings.timeout_ms;

        for compiled_step in &self.compiled_steps {
            // Check timeout if configured (0 = no timeout)
            if timeout_ms > 0 && line_start.elapsed().as_millis() as u64 > timeout_ms {
                return Err(anyhow::anyhow!(
                    "Processing timeout ({} ms) exceeded at line {}",
                    timeout_ms,
                    line_number
                ));
            }
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
                        let original = current_line.clone();
                        current_line = self.apply_transform(
                            &compiled_step.pattern,
                            &current_line,
                            action,
                            compiled_step.is_global,
                            &compiled_step.replacement,
                            &mut step_result,
                        )?;

                        if current_line != original {
                            step_result.add_transformation();
                        }
                    } else {
                        // No transform action specified, just check if pattern matches
                        if compiled_step.pattern.is_match(&current_line) {
                            step_result.add_match();
                        }
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

    fn apply_substitution(
        &self,
        pattern: &CompiledPattern,
        input: &str,
        replacement: &str,
        is_global: bool,
        step_result: &mut StepResult,
    ) -> Result<String> {
        // Count actual matches before replacement
        let match_count = pattern.find_iter(input).len();

        let result = if is_global {
            pattern.replace_all(input, replacement)
        } else {
            pattern.replace(input, replacement)
        };

        // Add the actual number of matches (or 1 for non-global if there was a match)
        if is_global {
            for _ in 0..match_count {
                step_result.add_match();
            }
        } else if match_count > 0 {
            step_result.add_match();
        }

        Ok(result)
    }

    fn apply_transform(
        &self,
        pattern: &CompiledPattern,
        input: &str,
        action: &TransformAction,
        is_global: bool,
        extra_text: &Option<String>,
        step_result: &mut StepResult,
    ) -> Result<String> {
        let match_count = pattern.find_iter(input).len();

        if match_count == 0 {
            return Ok(input.to_string());
        }

        // Transform function that will be applied to each match
        let transform_fn = |matched: &str| -> String {
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
                TransformAction::Shell { command } => crate::plugin::PluginRegistry::execute_shell(
                    command, matched,
                )
                .unwrap_or_else(|e| {
                    eprintln!("Shell transform error: {}", e);
                    matched.to_string()
                }),
                TransformAction::Plugin { name, args } => {
                    let registry = crate::plugin::PluginRegistry::new();
                    registry.execute(name, matched, args).unwrap_or_else(|e| {
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
            }
        };

        // Apply transformation to matches
        let result = if is_global {
            // Replace all matches with transformed versions
            let mut result = input.to_string();
            let mut offset: i64 = 0;

            for (start, end, matched) in pattern.find_iter(input) {
                let transformed = transform_fn(&matched);
                let adj_start = (start as i64 + offset) as usize;
                let adj_end = (end as i64 + offset) as usize;

                result = format!(
                    "{}{}{}",
                    &result[..adj_start],
                    transformed,
                    &result[adj_end..]
                );

                offset += transformed.len() as i64 - matched.len() as i64;
                step_result.add_match();
            }
            result
        } else {
            // Replace only first match
            if let Some((start, end, matched)) = pattern.find_iter(input).first() {
                let transformed = transform_fn(matched);
                step_result.add_match();
                format!("{}{}{}", &input[..*start], transformed, &input[*end..])
            } else {
                input.to_string()
            }
        };

        Ok(result)
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

    #[allow(dead_code)]
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
    #[allow(dead_code)]
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

    #[allow(dead_code)]
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

// Helper functions for encoding/decoding transformations

/// Base64 encode bytes using a simple implementation
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

/// Base64 decode a string
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

/// URL encode a string (percent encoding)
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

/// URL decode a string (percent decoding)
fn url_decode(s: &str) -> Option<String> {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                } else {
                    return None;
                }
            } else {
                return None;
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }

    Some(result)
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
                pcre_mode: false,
                fixed_strings: false,
                context_before: 2,
                context_after: 0,
                timeout_ms: 0,
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
                pcre_mode: false,
                fixed_strings: false,
                context_before: 0,
                context_after: 2,
                timeout_ms: 0,
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
}
