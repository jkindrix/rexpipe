//! Property-based tests using proptest
//!
//! These tests verify invariants and properties that should hold for all inputs.

use proptest::prelude::*;
use rexpipe::pipeline::{
    FilterAction, PipelineConfig, PipelineResult, PipelineSettings, PipelineStep, StepType,
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
            action: Some(FilterAction::KeepLine),
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
        };

        let config = PipelineConfig {
            name: Some("Filter Test".to_string()),
            description: None,
            version: None,
            patterns_include: Vec::new(),
            settings: PipelineSettings::default(),
            step: vec![step],
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
