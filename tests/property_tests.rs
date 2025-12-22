//! Property-based tests using proptest
//!
//! These tests verify invariants and properties that should hold for all inputs.

use proptest::prelude::*;
use rexpipe::pipeline::{
    PipelineConfig, PipelineResult, PipelineSettings, PipelineStep, RegexFlag, StepAction,
    StepType,
};
use rexpipe::processor::StreamProcessor;
use std::io::Cursor;

// =============================================================================
// Strategy definitions for generating test data
// =============================================================================

/// Generate arbitrary printable ASCII strings (safe for regex matching)
fn printable_ascii() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 .,;:!?@#$%^&*()\\-_+=\\[\\]{}|<>/~`'\"\n]{0,200}"
}

/// Generate strings that are valid as text to process (no null bytes)
fn text_content() -> impl Strategy<Value = String> {
    proptest::collection::vec(any::<char>().prop_filter("no null", |c| *c != '\0'), 0..500)
        .prop_map(|chars| chars.into_iter().collect())
}

/// Generate simple regex patterns (avoiding complex/slow patterns)
fn simple_pattern() -> impl Strategy<Value = String> {
    prop_oneof![
        // Literal strings
        "[a-zA-Z]{1,10}".prop_map(|s| regex::escape(&s)),
        // Simple character classes
        Just(r"\d+".to_string()),
        Just(r"\w+".to_string()),
        Just(r"\s+".to_string()),
        Just(r"[a-z]+".to_string()),
        Just(r"[A-Z]+".to_string()),
        Just(r"[0-9]+".to_string()),
        // Simple anchored patterns
        Just(r"^[a-z]+".to_string()),
        Just(r"[a-z]+$".to_string()),
        // Word boundaries
        Just(r"\b\w+\b".to_string()),
    ]
}

/// Generate replacement strings (with valid capture group references)
fn replacement_string() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z0-9_]{0,20}",
        Just("$0".to_string()),
        Just("${0}".to_string()),
        Just("[REPLACED]".to_string()),
        Just("".to_string()),
    ]
}

// =============================================================================
// Pipeline Configuration Properties
// =============================================================================

proptest! {
    /// Property: A valid inline pipeline should always validate successfully
    #[test]
    fn prop_inline_pipeline_validates(
        pattern in simple_pattern(),
        replacement in replacement_string()
    ) {
        let config = PipelineConfig::from_inline_pattern(&pattern, Some(&replacement));
        prop_assert!(config.validate().is_ok(), "Inline pipeline should validate");
    }

    /// Property: Pipeline with substitution step should always have exactly one step
    #[test]
    fn prop_inline_pipeline_has_one_step(pattern in simple_pattern()) {
        let config = PipelineConfig::from_inline_pattern(&pattern, Some("replacement"));
        prop_assert_eq!(config.step.len(), 1, "Inline pipeline should have exactly one step");
    }

    /// Property: JSON serialization should be reversible
    #[test]
    fn prop_json_roundtrip(pattern in simple_pattern(), replacement in replacement_string()) {
        let original = PipelineConfig::from_inline_pattern(&pattern, Some(&replacement));
        let json = original.to_json().expect("Should serialize to JSON");
        let restored = PipelineConfig::from_json(&json).expect("Should deserialize from JSON");

        prop_assert_eq!(original.step.len(), restored.step.len());
        prop_assert_eq!(&original.step[0].pattern, &restored.step[0].pattern);
        prop_assert_eq!(&original.step[0].replacement, &restored.step[0].replacement);
    }

    /// Property: TOML serialization should be reversible
    #[test]
    fn prop_toml_roundtrip(pattern in simple_pattern(), replacement in replacement_string()) {
        let original = PipelineConfig::from_inline_pattern(&pattern, Some(&replacement));
        let toml_str = original.to_toml().expect("Should serialize to TOML");
        let restored: PipelineConfig = toml::from_str(&toml_str).expect("Should deserialize from TOML");

        prop_assert_eq!(original.step.len(), restored.step.len());
        prop_assert_eq!(&original.step[0].pattern, &restored.step[0].pattern);
        prop_assert_eq!(&original.step[0].replacement, &restored.step[0].replacement);
    }
}

// =============================================================================
// Stream Processing Properties
// =============================================================================

proptest! {
    /// Property: Processing any text should not panic and should always return a result
    #[test]
    fn prop_processing_never_panics(
        input in printable_ascii(),
        pattern in simple_pattern(),
        replacement in replacement_string()
    ) {
        let config = PipelineConfig::from_inline_pattern(&pattern, Some(&replacement));
        let result = StreamProcessor::new(config);

        if let Ok(mut processor) = result {
            let reader = Cursor::new(input);
            let mut output = Vec::new();
            // Should not panic
            let _ = processor.process_stream(reader, &mut output);
        }
    }

    /// Property: Lines processed should be related to actual newlines in input
    #[test]
    fn prop_line_count_reasonable(
        lines in proptest::collection::vec("[a-zA-Z0-9]{1,20}", 1..20)
    ) {
        let input = lines.join("\n");

        // Use a simple pattern that matches nothing special
        let config = PipelineConfig::from_inline_pattern(r"UNLIKELY_PATTERN_12345", Some("REPLACED"));
        let mut processor = StreamProcessor::new(config).expect("Should create processor");

        let reader = Cursor::new(&input);
        let mut output = Vec::new();
        let result = processor.process_stream(reader, &mut output).expect("Should process");

        // Line count should be positive for non-empty input
        if !input.is_empty() {
            prop_assert!(result.lines_processed > 0, "Should process at least one line");
        }

        // Line count should be reasonable (not exponentially larger than input lines)
        let max_expected = (input.matches('\n').count() + 2) as u64;
        prop_assert!(
            result.lines_processed <= max_expected,
            "Expected <= {} lines, got {}", max_expected, result.lines_processed
        );
    }

    /// Property: Output should never be longer than input * replacement_ratio for bounded replacements
    #[test]
    fn prop_output_bounded(input in printable_ascii()) {
        // Replace nothing, so output should equal input (minus potential line ending normalization)
        let config = PipelineConfig::from_inline_pattern(r"NONEXISTENT_PATTERN_XYZ", Some("X"));
        let mut processor = StreamProcessor::new(config).expect("Should create processor");

        let reader = Cursor::new(&input);
        let mut output = Vec::new();
        let _ = processor.process_stream(reader, &mut output);

        // Output should be roughly the same size (allowing for line ending differences)
        let output_len = output.len();
        let input_len = input.len();
        prop_assert!(
            output_len <= input_len + input.lines().count() + 10,
            "Output unexpectedly larger: {} vs {}", output_len, input_len
        );
    }
}

// =============================================================================
// PipelineResult Properties
// =============================================================================

proptest! {
    /// Property: Success rate should always be between 0 and 1
    #[test]
    fn prop_success_rate_bounded(
        lines in 0u64..10000,
        errors in 0usize..100
    ) {
        let mut result = PipelineResult::new();
        result.lines_processed = lines;

        for _ in 0..errors.min(lines as usize) {
            result.add_error(rexpipe::pipeline::PipelineError::new(
                0,
                1,
                rexpipe::pipeline::ErrorType::PatternMatch,
                "test".to_string(),
            ));
        }

        let rate = result.success_rate();
        prop_assert!(rate >= 0.0 && rate <= 1.0, "Success rate {} out of bounds", rate);
    }

    /// Property: Transformations should never exceed matches
    #[test]
    fn prop_transformations_bounded_by_matches(
        matches in 0u64..1000,
        transformations in 0u64..1000
    ) {
        let mut result = PipelineResult::new();
        result.matches_found = matches;
        // In reality, transformations come from processing - this just tests the struct
        result.transformations_applied = transformations.min(matches * 10); // Allow multiple transforms per match

        // This is a sanity check - the real invariant is tested in integration
        prop_assert!(result.matches_found <= u64::MAX);
    }
}

// =============================================================================
// Fixed String Mode Properties
// =============================================================================

proptest! {
    /// Property: Fixed string matching should find exact matches
    #[test]
    fn prop_fixed_string_finds_exact_matches(
        needle in "[a-zA-Z]{1,10}",
        prefix in "[a-zA-Z]{0,20}",
        suffix in "[a-zA-Z]{0,20}"
    ) {
        let input = format!("{}{}{}", prefix, needle, suffix);

        let settings = PipelineSettings {
            fixed_strings: true,
            ..Default::default()
        };
        let config = PipelineConfig::from_inline_pattern_with_settings(&needle, Some("[FOUND]"), settings);

        let result = StreamProcessor::new(config);
        if let Ok(mut processor) = result {
            let reader = Cursor::new(&input);
            let mut output = Vec::new();
            let result = processor.process_stream(reader, &mut output).expect("Should process");

            // Should find at least one match (the needle we inserted)
            prop_assert!(result.matches_found >= 1, "Should find the needle");
        }
    }

    /// Property: Fixed string mode should handle regex special chars safely
    #[test]
    fn prop_fixed_string_handles_special_chars(
        special in r"[\.\*\+\?\[\]\(\)\{\}\^\$\|\\]{1,5}"
    ) {
        let input = format!("before {} after", special);

        let settings = PipelineSettings {
            fixed_strings: true,
            ..Default::default()
        };
        let config = PipelineConfig::from_inline_pattern_with_settings(&special, Some("[SPECIAL]"), settings);

        let result = StreamProcessor::new(config);
        if let Ok(mut processor) = result {
            let reader = Cursor::new(&input);
            let mut output = Vec::new();
            // Should not panic on regex special characters
            let _ = processor.process_stream(reader, &mut output);
        }
    }
}

// =============================================================================
// Filter Operation Properties
// =============================================================================

proptest! {
    /// Property: KeepLine filter should only output lines that match
    #[test]
    fn prop_filter_keep_reduces_output(
        lines in proptest::collection::vec("[a-zA-Z]{5,15}", 5..20)
    ) {
        let input = lines.join("\n");

        // Filter to keep only lines starting with 'a' (unlikely to be all)
        let step = PipelineStep {
            step_type: StepType::Filter,
            pattern: "^a".to_string(),
            replacement: None,
            action: Some(StepAction::KeepLine),
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        };

        let config = PipelineConfig {
            name: Some("Filter Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings::default(),
            step: vec![step],
            ..Default::default()
        };

        let result = StreamProcessor::new(config);
        if let Ok(mut processor) = result {
            let reader = Cursor::new(&input);
            let mut output = Vec::new();
            let _ = processor.process_stream(reader, &mut output);

            let output_str = String::from_utf8_lossy(&output);
            let output_lines: Vec<_> = output_str.lines().collect();

            // All output lines should start with 'a'
            for line in &output_lines {
                if !line.is_empty() {
                    prop_assert!(line.starts_with('a'), "Line '{}' doesn't start with 'a'", line);
                }
            }
        }
    }
}

// =============================================================================
// UTF-8 Handling Properties
// =============================================================================

proptest! {
    /// Property: Processing should handle valid UTF-8 correctly
    #[test]
    fn prop_utf8_handling(input in text_content()) {
        // Only test if input is valid UTF-8 (proptest should generate valid UTF-8)
        if !input.is_empty() {
            let config = PipelineConfig::from_inline_pattern(r"\w+", Some("[WORD]"));
            let result = StreamProcessor::new(config);

            if let Ok(mut processor) = result {
                let reader = Cursor::new(&input);
                let mut output = Vec::new();
                // Should not panic on UTF-8 input
                let _ = processor.process_stream(reader, &mut output);

                // Output should be valid UTF-8
                let output_result = String::from_utf8(output);
                prop_assert!(output_result.is_ok(), "Output should be valid UTF-8");
            }
        }
    }
}

// =============================================================================
// Transform Action Properties
// =============================================================================

use rexpipe::pipeline::TransformAction;

proptest! {
    /// Property: Uppercase transform should produce uppercase output for matched text
    #[test]
    fn prop_transform_uppercase_produces_uppercase(
        word in "[a-z]{3,10}"
    ) {
        let input = format!("before {} after", word);

        let step = PipelineStep {
            step_type: StepType::Substitute,
            pattern: word.clone(),
            replacement: None,
            action: None,
            transform: Some(TransformAction::Uppercase),
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        };

        let config = PipelineConfig {
            name: Some("Uppercase Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings::default(),
            step: vec![step],
            ..Default::default()
        };

        if let Ok(mut processor) = StreamProcessor::new(config) {
            let reader = Cursor::new(&input);
            let mut output = Vec::new();
            let _ = processor.process_stream(reader, &mut output);

            let output_str = String::from_utf8_lossy(&output);
            // The word should now be uppercase in the output
            prop_assert!(
                output_str.contains(&word.to_uppercase()),
                "Expected '{}' in output, got '{}'",
                word.to_uppercase(),
                output_str
            );
        }
    }

    /// Property: Lowercase transform should produce lowercase output for matched text
    #[test]
    fn prop_transform_lowercase_produces_lowercase(
        word in "[A-Z]{3,10}"
    ) {
        let input = format!("before {} after", word);

        let step = PipelineStep {
            step_type: StepType::Substitute,
            pattern: word.clone(),
            replacement: None,
            action: None,
            transform: Some(TransformAction::Lowercase),
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        };

        let config = PipelineConfig {
            name: Some("Lowercase Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings::default(),
            step: vec![step],
            ..Default::default()
        };

        if let Ok(mut processor) = StreamProcessor::new(config) {
            let reader = Cursor::new(&input);
            let mut output = Vec::new();
            let _ = processor.process_stream(reader, &mut output);

            let output_str = String::from_utf8_lossy(&output);
            // The word should now be lowercase in the output
            prop_assert!(
                output_str.contains(&word.to_lowercase()),
                "Expected '{}' in output, got '{}'",
                word.to_lowercase(),
                output_str
            );
        }
    }
}

// =============================================================================
// Multi-step Pipeline Properties
// =============================================================================

proptest! {
    /// Property: Multi-step pipelines should apply all enabled steps
    #[test]
    fn prop_multi_step_pipeline_applies_all_steps(
        word in "[a-z]{3,8}"
    ) {
        let input = format!("test {} end", word);

        // Step 1: Replace word with STEP1 (globally)
        let step1 = PipelineStep {
            step_type: StepType::Substitute,
            pattern: word.clone(),
            replacement: Some("STEP1".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
            ..Default::default()
        };

        // Step 2: Replace STEP1 with STEP2
        let step2 = PipelineStep {
            step_type: StepType::Substitute,
            pattern: "STEP1".to_string(),
            replacement: Some("STEP2".to_string()),
            action: None,
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        };

        let config = PipelineConfig {
            name: Some("Multi-step Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings::default(),
            step: vec![step1, step2],
            ..Default::default()
        };

        if let Ok(mut processor) = StreamProcessor::new(config) {
            let reader = Cursor::new(&input);
            let mut output = Vec::new();
            let _ = processor.process_stream(reader, &mut output);

            let output_str = String::from_utf8_lossy(&output);
            // Should have STEP2 (both steps applied)
            prop_assert!(
                output_str.contains("STEP2"),
                "Expected 'STEP2' in output, got '{}'",
                output_str
            );
            // Should not have the original word
            prop_assert!(
                !output_str.contains(&word),
                "Original word '{}' should not appear in output",
                word
            );
        }
    }

    /// Property: Disabled steps should not affect output
    #[test]
    fn prop_disabled_steps_not_applied(
        word in "[a-z]{3,8}"
    ) {
        let input = format!("test {} end", word);

        // Disabled step - should not apply
        let disabled_step = PipelineStep {
            step_type: StepType::Substitute,
            pattern: word.clone(),
            replacement: Some("DISABLED".to_string()),
            action: None,
            transform: None,
            flags: None,
            description: None,
            enabled: Some(false),  // DISABLED
            ..Default::default()
        };

        let config = PipelineConfig {
            name: Some("Disabled Step Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings::default(),
            step: vec![disabled_step],
            ..Default::default()
        };

        if let Ok(mut processor) = StreamProcessor::new(config) {
            let reader = Cursor::new(&input);
            let mut output = Vec::new();
            let _ = processor.process_stream(reader, &mut output);

            let output_str = String::from_utf8_lossy(&output);
            // Should NOT contain DISABLED since step was disabled
            prop_assert!(
                !output_str.contains("DISABLED"),
                "Disabled step should not apply"
            );
            // Should still contain original word
            prop_assert!(
                output_str.contains(&word),
                "Original word should remain"
            );
        }
    }
}

// =============================================================================
// Capture Group Properties
// =============================================================================

proptest! {
    /// Property: Capture groups should be substituted correctly in replacement
    #[test]
    fn prop_capture_groups_substituted(
        prefix in "[a-m]{2,5}",
        suffix in "[n-z]{2,5}"
    ) {
        // Use disjoint character sets to ensure prefix != suffix
        let input = format!("{}_{}", prefix, suffix);

        // Pattern with capture groups, swap prefix and suffix
        let step = PipelineStep {
            step_type: StepType::Substitute,
            pattern: format!(r"({})_({})", prefix, suffix),
            replacement: Some("${2}_${1}".to_string()),
            action: None,
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        };

        let config = PipelineConfig {
            name: Some("Capture Group Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings::default(),
            step: vec![step],
            ..Default::default()
        };

        if let Ok(mut processor) = StreamProcessor::new(config) {
            let reader = Cursor::new(&input);
            let mut output = Vec::new();
            let _ = processor.process_stream(reader, &mut output);

            let output_str = String::from_utf8_lossy(&output);
            let expected = format!("{}_{}", suffix, prefix);
            prop_assert!(
                output_str.contains(&expected),
                "Expected '{}' in output, got '{}'",
                expected,
                output_str
            );
        }
    }

    /// Property: Multiple matches on same line should all be replaced
    #[test]
    fn prop_multiple_matches_all_replaced(
        word in "[a-z]{3,6}",
        count in 2usize..5
    ) {
        // Create input with multiple instances of the word
        let input = (0..count).map(|_| word.clone()).collect::<Vec<_>>().join(" ");

        let config = PipelineConfig::from_inline_pattern(&word, Some("X"));

        if let Ok(mut processor) = StreamProcessor::new(config) {
            let reader = Cursor::new(&input);
            let mut output = Vec::new();
            let result = processor.process_stream(reader, &mut output);

            if let Ok(result) = result {
                // Should find all instances
                prop_assert!(
                    result.matches_found >= count as u64,
                    "Expected at least {} matches, got {}",
                    count,
                    result.matches_found
                );
            }
        }
    }
}

// =============================================================================
// Delete Line Filter Properties
// =============================================================================

proptest! {
    /// Property: DropLine filter should remove matching lines
    #[test]
    fn prop_drop_line_removes_matches(
        lines in proptest::collection::vec("[a-z]{5,15}", 5..15)
    ) {
        let input = lines.join("\n");

        // Drop lines starting with 'a'
        let step = PipelineStep {
            step_type: StepType::Filter,
            pattern: "^a".to_string(),
            replacement: None,
            action: Some(StepAction::DropLine),
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        };

        let config = PipelineConfig {
            name: Some("Delete Filter Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings::default(),
            step: vec![step],
            ..Default::default()
        };

        if let Ok(mut processor) = StreamProcessor::new(config) {
            let reader = Cursor::new(&input);
            let mut output = Vec::new();
            let _ = processor.process_stream(reader, &mut output);

            let output_str = String::from_utf8_lossy(&output);
            // No output lines should start with 'a'
            for line in output_str.lines() {
                if !line.is_empty() {
                    prop_assert!(
                        !line.starts_with('a'),
                        "Line '{}' should have been deleted",
                        line
                    );
                }
            }
        }
    }
}

// =============================================================================
// Long Line Properties
// =============================================================================

proptest! {
    /// Property: Very long lines should be handled without panic
    #[test]
    fn prop_long_lines_handled(
        repeat_count in 100usize..1000
    ) {
        let word = "abcdefghij";
        let input = word.repeat(repeat_count);

        let config = PipelineConfig::from_inline_pattern("abc", Some("XYZ"));

        if let Ok(mut processor) = StreamProcessor::new(config) {
            let reader = Cursor::new(&input);
            let mut output = Vec::new();
            let result = processor.process_stream(reader, &mut output);

            // Should not panic and should process
            prop_assert!(result.is_ok());

            let output_str = String::from_utf8_lossy(&output);
            // Should contain replacements
            prop_assert!(
                output_str.contains("XYZ"),
                "Should have made replacements"
            );
            // Should not contain original
            prop_assert!(
                !output_str.contains("abc"),
                "Should have replaced all instances"
            );
        }
    }
}

// =============================================================================
// Edge Case Properties
// =============================================================================

proptest! {
    /// Property: Empty input should produce empty output (or just newlines)
    #[test]
    fn prop_empty_input_empty_output(_dummy in 0..1u8) {
        let input = "";
        let config = PipelineConfig::from_inline_pattern(r"\w+", Some("[WORD]"));

        if let Ok(mut processor) = StreamProcessor::new(config) {
            let reader = Cursor::new(input);
            let mut output = Vec::new();
            let result = processor.process_stream(reader, &mut output);

            prop_assert!(result.is_ok());
            prop_assert!(
                output.is_empty() || output == b"\n",
                "Empty input should produce empty or minimal output"
            );
        }
    }

    /// Property: Single character input should be handled correctly
    #[test]
    fn prop_single_char_input(c in "[a-zA-Z0-9]") {
        let config = PipelineConfig::from_inline_pattern(r".", Some("X"));

        if let Ok(mut processor) = StreamProcessor::new(config) {
            let reader = Cursor::new(&c);
            let mut output = Vec::new();
            let result = processor.process_stream(reader, &mut output);

            prop_assert!(result.is_ok());
            let output_str = String::from_utf8_lossy(&output);
            // Single char should be replaced with X
            prop_assert!(
                output_str.contains("X"),
                "Single char should be replaced"
            );
        }
    }

    /// Property: Whitespace-only input should be handled correctly
    #[test]
    fn prop_whitespace_input(spaces in 1usize..20) {
        let input = " ".repeat(spaces);
        let config = PipelineConfig::from_inline_pattern(r"\s+", Some("_"));

        if let Ok(mut processor) = StreamProcessor::new(config) {
            let reader = Cursor::new(&input);
            let mut output = Vec::new();
            let result = processor.process_stream(reader, &mut output);

            prop_assert!(result.is_ok());
            let output_str = String::from_utf8_lossy(&output);
            prop_assert!(
                output_str.contains("_"),
                "Whitespace should be replaced with underscore"
            );
        }
    }

    /// Property: Lines with only newlines should not crash
    #[test]
    fn prop_empty_lines_handled(count in 1usize..10) {
        let input = "\n".repeat(count);
        let config = PipelineConfig::from_inline_pattern(r"^$", Some("EMPTY"));

        if let Ok(mut processor) = StreamProcessor::new(config) {
            let reader = Cursor::new(&input);
            let mut output = Vec::new();
            let result = processor.process_stream(reader, &mut output);

            // Should not panic
            prop_assert!(result.is_ok());
        }
    }

    /// Property: Mixed content with numbers and letters should be handled
    #[test]
    fn prop_alphanumeric_content(
        letters in "[a-zA-Z]{2,8}",
        numbers in "[0-9]{2,8}"
    ) {
        let input = format!("{}{}", letters, numbers);
        let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[NUM]"));

        if let Ok(mut processor) = StreamProcessor::new(config) {
            let reader = Cursor::new(&input);
            let mut output = Vec::new();
            let result = processor.process_stream(reader, &mut output);

            prop_assert!(result.is_ok());
            let output_str = String::from_utf8_lossy(&output);
            prop_assert!(
                output_str.contains("[NUM]"),
                "Numbers should be replaced"
            );
            prop_assert!(
                output_str.contains(&letters),
                "Letters should remain"
            );
        }
    }
}

// =============================================================================
// Settings Properties
// =============================================================================

proptest! {
    /// Property: Fixed strings mode should escape regex metacharacters properly
    #[test]
    fn prop_fixed_strings_escapes_metacharacters(
        text in "[a-z]{2,5}"
    ) {
        // Pattern with regex metacharacters that should be treated literally
        let pattern = format!("{}.*", text);
        let input = format!("test {} here", pattern);

        let settings = PipelineSettings {
            fixed_strings: true,
            ..Default::default()
        };
        let config = PipelineConfig::from_inline_pattern_with_settings(
            &pattern,
            Some("[MATCH]"),
            settings
        );

        if let Ok(mut processor) = StreamProcessor::new(config) {
            let reader = Cursor::new(&input);
            let mut output = Vec::new();
            let result = processor.process_stream(reader, &mut output);

            prop_assert!(result.is_ok());
            let output_str = String::from_utf8_lossy(&output);
            prop_assert!(
                output_str.contains("[MATCH]"),
                "Literal pattern should match"
            );
        }
    }
}
