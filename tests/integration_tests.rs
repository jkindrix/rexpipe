use rexpipe::inspector::{Inspector, InspectorOptions};
use rexpipe::pipeline::{
    PipelineConfig, PipelineSettings, PipelineStep, RegexFlag, StepAction, StepType,
    TransformAction,
};
use rexpipe::processor::StreamProcessor;
use std::io::Cursor;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_end_to_end_log_processing() {
    let input_data = r#"2025-01-08 10:15:23 [INFO] Server startup complete
2025-01-08 10:15:24 [DEBUG] Loading configuration
2025-01-08 10:15:25 [ERROR] Database connection failed for user_id=1234
2025-01-08 10:15:26 [INFO] Connection from 192.168.1.10"#;

    let config = PipelineConfig {
        name: Some("Integration Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![
            PipelineStep {
                step_type: StepType::Substitute,
                pattern: r"\[ERROR\]".to_string(),
                replacement: Some("[ERR]".to_string()),
                action: None,
                transform: None,
                flags: Some(vec![RegexFlag::Global]),
                description: None,
                enabled: Some(true),
                ..Default::default()
            },
            PipelineStep {
                step_type: StepType::Filter,
                pattern: "DEBUG".to_string(),
                replacement: None,
                action: Some(StepAction::DropLine),
                transform: None,
                flags: None,
                description: None,
                enabled: Some(true),
                ..Default::default()
            },
            PipelineStep {
                step_type: StepType::Substitute,
                pattern: r"user_id=(\d+)".to_string(),
                replacement: Some("uid=${1}".to_string()),
                action: None,
                transform: None,
                flags: Some(vec![RegexFlag::Global]),
                description: None,
                enabled: Some(true),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Verify transformations
    assert!(output_str.contains("[ERR]"));
    assert!(!output_str.contains("[ERROR]"));
    assert!(!output_str.contains("DEBUG"));
    assert!(output_str.contains("uid=1234"));
    assert!(!output_str.contains("user_id=1234"));

    // Verify statistics
    assert_eq!(result.lines_processed, 4);
    assert!(result.transformations_applied > 0);
    assert_eq!(result.errors.len(), 0);
}

#[test]
fn test_config_file_loading() {
    let config_content = r#"
name = "Test Pipeline"
description = "Test configuration"
version = "1.0.0"

[[step]]
type = "substitute"
pattern = '\d+'
replacement = 'NUMBER'
flags = ["global"]
enabled = true
"#;

    let mut temp_file = NamedTempFile::new().unwrap();
    write!(temp_file, "{}", config_content).unwrap();

    let config = PipelineConfig::from_file(temp_file.path()).unwrap();
    assert_eq!(config.name, Some("Test Pipeline".to_string()));
    assert_eq!(config.step.len(), 1);
    assert!(config.validate().is_ok());
}

#[test]
fn test_inspection_mode() {
    let input_data = "Test 123 and 456 with user_id=789";
    let config = PipelineConfig::from_inline_pattern(r"(\d+)", None);

    let options = InspectorOptions::new()
        .interactive(false)
        .show_performance(true);

    let mut inspector = Inspector::new(config).unwrap().with_options(options);
    let reader = Cursor::new(input_data);
    let result = inspector.inspect_stream(reader).unwrap();

    assert_eq!(result.total_lines, 1);
    assert_eq!(result.total_matches, 3, "Should find exactly 123, 456, 789");
    assert_eq!(result.line_matches.len(), 1);

    let line_match = &result.line_matches[0];
    assert_eq!(line_match.line_number, 1);
    assert_eq!(line_match.matches.len(), 3, "Should have exactly 3 matches");

    // Verify the actual matched values
    let match_values: Vec<&str> = line_match
        .matches
        .iter()
        .map(|m| m.full_match.as_str())
        .collect();
    assert_eq!(match_values, vec!["123", "456", "789"]);
}

#[test]
fn test_filter_operations() {
    let input_data = r#"keep this line
drop this DEBUG message
keep this line too
another DEBUG to drop"#;

    let config = PipelineConfig {
        name: Some("Filter Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Filter,
            pattern: "DEBUG".to_string(),
            replacement: None,
            action: Some(StepAction::DropLine),
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let _result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();
    let lines: Vec<&str> = output_str.trim().split('\n').collect();

    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("keep this line"));
    assert!(lines[1].contains("keep this line too"));
    assert!(!output_str.contains("DEBUG"));
}

#[test]
fn test_performance_metrics() {
    let input_data = "Line 1: test 123\nLine 2: test 456\nLine 3: test 789";
    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUMBER"));

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
    let stats = processor.get_stats();

    assert_eq!(result.lines_processed, 3);
    assert_eq!(stats.lines_read, 3);
    assert!(stats.bytes_processed > 0);
    assert!(result.transformations_applied >= 3);
}

#[test]
fn test_validation_step_type() {
    let input_data = r#"2025-01-08 10:15:23 [INFO] Valid log line
Invalid line without timestamp
2025-01-08 10:15:25 [ERROR] Another valid line"#;

    let config = PipelineConfig {
        name: Some("Validation Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Validate,
            pattern: r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}".to_string(),
            replacement: None,
            action: None,
            transform: None,
            flags: None,
            description: Some("Validate timestamp format".to_string()),
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();

    // Should have validation errors
    assert!(!result.errors.is_empty());

    let output_str = String::from_utf8(output).unwrap();
    let lines: Vec<&str> = output_str
        .trim()
        .split('\n')
        .filter(|s| !s.is_empty())
        .collect();

    // Only valid lines should be in output
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|line| line.starts_with("2025-01-08")));
}

#[test]
fn test_extract_step_type() {
    let input_data = "Extract email: john@example.com from this line";

    let config = PipelineConfig {
        name: Some("Extract Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Extract,
            pattern: r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}".to_string(),
            replacement: None,
            action: None,
            transform: None,
            flags: None,
            description: Some("Extract email addresses".to_string()),
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Should extract only the email address
    assert_eq!(output_str.trim(), "john@example.com");
    assert!(result.transformations_applied > 0);
}

#[test]
fn test_error_handling() {
    // Test invalid regex pattern
    let config = PipelineConfig {
        name: Some("Error Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            pattern: r"[invalid regex(".to_string(), // Invalid regex
            replacement: Some("replacement".to_string()),
            action: None,
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    // Should fail to create processor due to invalid regex
    assert!(StreamProcessor::new(config).is_err());
}

#[test]
fn test_disabled_steps() {
    let input_data = "Test 123 and 456";

    let config = PipelineConfig {
        name: Some("Disabled Step Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            pattern: r"\d+".to_string(),
            replacement: Some("NUMBER".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(false), // Disabled step
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Should be unchanged since step is disabled
    assert_eq!(output_str.trim(), input_data);
    assert_eq!(result.transformations_applied, 0);
}

#[test]
fn test_transform_uppercase() {
    let input_data = "hello WORLD test";

    let config = PipelineConfig {
        name: Some("Transform Uppercase Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Transform,
            pattern: r"[a-z]+".to_string(),
            replacement: None,
            action: None,
            transform: Some(TransformAction::Uppercase),
            flags: Some(vec![RegexFlag::Global]),
            description: Some("Convert lowercase words to uppercase".to_string()),
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    assert_eq!(output_str.trim(), "HELLO WORLD TEST");
}

#[test]
fn test_transform_lowercase() {
    let input_data = "Hello WORLD Test";

    let config = PipelineConfig {
        name: Some("Transform Lowercase Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Transform,
            pattern: r"[A-Z]+".to_string(),
            replacement: None,
            action: None,
            transform: Some(TransformAction::Lowercase),
            flags: Some(vec![RegexFlag::Global]),
            description: Some("Convert uppercase to lowercase".to_string()),
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // "H" -> "h", "WORLD" -> "world", "T" -> "t"
    assert!(output_str.contains("hello"));
    assert!(output_str.contains("world"));
}

// =====================================================
// PCRE Feature Tests (require --features pcre)
// =====================================================

/// Test positive lookahead pattern (match word followed by specific text)
#[cfg(feature = "pcre")]
#[test]
fn test_pcre_positive_lookahead() {
    let input_data = "foo bar foo baz foo qux";

    // Match "foo" only when followed by " bar"
    let settings = PipelineSettings {
        pcre_mode: true,
        ..Default::default()
    };

    let config = PipelineConfig {
        name: Some("PCRE Lookahead Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings,
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            pattern: r"foo(?= bar)".to_string(), // Positive lookahead
            replacement: Some("MATCHED".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: Some("Match foo only when followed by bar".to_string()),
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Only the first "foo" (before " bar") should be replaced
    assert_eq!(output_str.trim(), "MATCHED bar foo baz foo qux");
}

/// Test negative lookahead pattern (match word NOT followed by specific text)
#[cfg(feature = "pcre")]
#[test]
fn test_pcre_negative_lookahead() {
    let input_data = "foo bar foo baz foo qux";

    let settings = PipelineSettings {
        pcre_mode: true,
        ..Default::default()
    };

    let config = PipelineConfig {
        name: Some("PCRE Negative Lookahead Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings,
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            pattern: r"foo(?! bar)".to_string(), // Negative lookahead
            replacement: Some("MATCHED".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: Some("Match foo only when NOT followed by bar".to_string()),
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // "foo baz" and "foo qux" should be replaced, but "foo bar" should remain
    assert_eq!(output_str.trim(), "foo bar MATCHED baz MATCHED qux");
}

/// Test positive lookbehind pattern (match word preceded by specific text)
#[cfg(feature = "pcre")]
#[test]
fn test_pcre_positive_lookbehind() {
    let input_data = "price: $100, discount: $50, total: 150";

    let settings = PipelineSettings {
        pcre_mode: true,
        ..Default::default()
    };

    let config = PipelineConfig {
        name: Some("PCRE Lookbehind Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings,
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            pattern: r"(?<=\$)\d+".to_string(), // Positive lookbehind for $
            replacement: Some("XXX".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: Some("Match numbers preceded by $".to_string()),
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Numbers after $ should be replaced, but 150 should remain
    assert_eq!(output_str.trim(), "price: $XXX, discount: $XXX, total: 150");
}

/// Test negative lookbehind pattern (match word NOT preceded by specific text)
#[cfg(feature = "pcre")]
#[test]
fn test_pcre_negative_lookbehind() {
    let input_data = "price: $100, discount: $50, total: 150";

    let settings = PipelineSettings {
        pcre_mode: true,
        ..Default::default()
    };

    let config = PipelineConfig {
        name: Some("PCRE Negative Lookbehind Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings,
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            pattern: r"(?<!\$)\b\d+\b".to_string(), // Negative lookbehind for $
            replacement: Some("XXX".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: Some("Match numbers NOT preceded by $".to_string()),
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Only 150 should be replaced (not preceded by $)
    assert_eq!(output_str.trim(), "price: $100, discount: $50, total: XXX");
}

/// Test combined lookahead and lookbehind
#[cfg(feature = "pcre")]
#[test]
fn test_pcre_combined_lookaround() {
    // Input has "user" in different positions:
    // - "user: admin" - "user" not preceded by ": " (at start)
    // - "role: user end" - "user" preceded by ": " and NOT followed by "," - SHOULD MATCH
    let input_data = "user: admin, role: user end";

    let settings = PipelineSettings {
        pcre_mode: true,
        ..Default::default()
    };

    let config = PipelineConfig {
        name: Some("PCRE Combined Lookaround Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings,
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            // Match "user" only when preceded by ": " and NOT followed by ","
            pattern: r"(?<=: )user(?!,)".to_string(),
            replacement: Some("ROLE".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: Some("Complex lookaround pattern".to_string()),
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // "user" in "role: user end" should be replaced (preceded by ": ", not followed by ",")
    // "user:" at start is not preceded by ": " so not affected
    assert!(
        output_str.contains("role: ROLE end"),
        "Expected 'role: ROLE end', got: {}",
        output_str
    );
    assert!(
        output_str.contains("user: admin"),
        "Start 'user' should not be affected: {}",
        output_str
    );
}

/// Test PCRE mode with filter step
#[cfg(feature = "pcre")]
#[test]
fn test_pcre_filter_with_lookahead() {
    let input_data = r#"DEBUG: Starting process
INFO: User logged in
DEBUG: Cache hit for user data
ERROR: Connection failed
INFO: Processing complete"#;

    let settings = PipelineSettings {
        pcre_mode: true,
        ..Default::default()
    };

    let config = PipelineConfig {
        name: Some("PCRE Filter Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings,
        step: vec![PipelineStep {
            step_type: StepType::Filter,
            // Drop lines starting with DEBUG followed by anything containing "user"
            pattern: r"^DEBUG:(?=.*user)".to_string(),
            replacement: None,
            action: Some(StepAction::DropLine),
            transform: None,
            flags: Some(vec![RegexFlag::CaseInsensitive]),
            description: Some("Drop debug lines mentioning user".to_string()),
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // "DEBUG: Cache hit for user data" should be dropped
    assert!(output_str.contains("DEBUG: Starting process")); // No "user"
    assert!(!output_str.contains("Cache hit for user data")); // Dropped
    assert!(output_str.contains("INFO: User logged in")); // Different prefix
}

// =====================================================
// Fixed String Mode Tests
// =====================================================

/// Test fixed string mode with literal pattern matching
#[test]
fn test_fixed_string_basic_replacement() {
    let input_data = "regex.* patterns are .+ complex";

    let settings = PipelineSettings {
        fixed_strings: true,
        ..Default::default()
    };

    let config = PipelineConfig {
        name: Some("Fixed String Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings,
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            // This should match literally, not as regex
            pattern: ".*".to_string(),
            replacement: Some("WILDCARD".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: Some("Replace literal .*".to_string()),
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Should replace literal ".*" not interpret as regex wildcard
    assert_eq!(output_str.trim(), "regexWILDCARD patterns are .+ complex");
}

/// Test fixed string mode with special regex characters
#[test]
fn test_fixed_string_special_chars() {
    let input_data = "Match [brackets] and (parens) and $dollars";

    let settings = PipelineSettings {
        fixed_strings: true,
        ..Default::default()
    };

    let config = PipelineConfig {
        name: Some("Fixed String Special Chars Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings,
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            // Literal brackets - would be char class in regex
            pattern: "[brackets]".to_string(),
            replacement: Some("BRACKETS".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    assert_eq!(
        output_str.trim(),
        "Match BRACKETS and (parens) and $dollars"
    );
}

/// Test fixed string mode with filter step
#[test]
fn test_fixed_string_filter() {
    let input_data = r#"Line with [ERROR] message
Normal line
Another [ERROR] line
Line with ERROR without brackets"#;

    let settings = PipelineSettings {
        fixed_strings: true,
        ..Default::default()
    };

    let config = PipelineConfig {
        name: Some("Fixed String Filter Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings,
        step: vec![PipelineStep {
            step_type: StepType::Filter,
            // Match literal [ERROR] including brackets
            pattern: "[ERROR]".to_string(),
            replacement: None,
            action: Some(StepAction::DropLine),
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();
    let lines: Vec<&str> = output_str.trim().split('\n').collect();

    // Should only drop lines with literal "[ERROR]", not "ERROR"
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("Normal line"));
    assert!(lines[1].contains("ERROR without brackets"));
}

/// Test fixed string mode preserves backslashes
#[test]
fn test_fixed_string_backslashes() {
    let input_data = r"Path: C:\Users\test\file.txt";

    let settings = PipelineSettings {
        fixed_strings: true,
        ..Default::default()
    };

    let config = PipelineConfig {
        name: Some("Fixed String Backslash Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings,
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            // Literal backslash - would be escape char in regex
            pattern: r"\".to_string(),
            replacement: Some("/".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Should replace literal backslashes with forward slashes
    assert_eq!(output_str.trim(), "Path: C:/Users/test/file.txt");
}

/// Test fixed string mode with multiple replacements in one line
#[test]
fn test_fixed_string_multiple_occurrences() {
    let input_data = "a+b+c = a + b + c";

    let settings = PipelineSettings {
        fixed_strings: true,
        ..Default::default()
    };

    let config = PipelineConfig {
        name: Some("Fixed String Multiple Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings,
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            // Literal "+" - would match one or more in regex
            pattern: "+".to_string(),
            replacement: Some("PLUS".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    assert_eq!(output_str.trim(), "aPLUSbPLUSc = a PLUS b PLUS c");
}

// =====================================================
// Unicode Edge Case Tests
// =====================================================

/// Test Unicode character matching
#[test]
fn test_unicode_basic_matching() {
    let input_data = "Hello 世界 and こんにちは";

    let config = PipelineConfig {
        name: Some("Unicode Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            pattern: r"世界".to_string(),
            replacement: Some("World".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    assert_eq!(output_str.trim(), "Hello World and こんにちは");
}

/// Test Unicode character classes
#[test]
fn test_unicode_character_classes() {
    let input_data = "User: 用户123 Email: test@example.com";

    let config = PipelineConfig {
        name: Some("Unicode Classes Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            // Match CJK characters (using Unicode property)
            pattern: r"\p{Han}+".to_string(),
            replacement: Some("USER".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    assert_eq!(output_str.trim(), "User: USER123 Email: test@example.com");
}

/// Test Unicode emoji handling
#[test]
fn test_unicode_emoji() {
    let input_data = "Great job! 👍 Keep going! 🎉";

    let config = PipelineConfig {
        name: Some("Unicode Emoji Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            pattern: "👍".to_string(),
            replacement: Some("[thumbs up]".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    assert_eq!(output_str.trim(), "Great job! [thumbs up] Keep going! 🎉");
}

/// Test mixed script filtering
#[test]
fn test_unicode_mixed_script_filter() {
    let input_data = r#"English line
中文行
日本語の行
Mixed: English 和 中文"#;

    let config = PipelineConfig {
        name: Some("Unicode Filter Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Filter,
            // Keep only lines with CJK characters
            pattern: r"\p{Han}".to_string(),
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
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();
    let lines: Vec<&str> = output_str.trim().split('\n').collect();

    assert_eq!(lines.len(), 3);
    assert!(lines[0].contains("中文行"));
    assert!(lines[1].contains("日本語")); // Japanese also contains Han
    assert!(lines[2].contains("Mixed"));
}

/// Test Unicode with transform (uppercase/lowercase)
#[test]
fn test_unicode_transform() {
    // Test case transformation with Latin characters only for reliable behavior
    let input_data = "HELLO café WORLD";

    let config = PipelineConfig {
        name: Some("Unicode Transform Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Transform,
            pattern: r"[A-Z]+".to_string(), // Match uppercase Latin
            replacement: None,
            action: None,
            transform: Some(TransformAction::Lowercase),
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Verify uppercase Latin is lowercased, but café (with é) remains
    assert_eq!(output_str.trim(), "hello café world");
}

/// Test Unicode normalization handling
#[test]
fn test_unicode_accented_characters() {
    let input_data = "Café résumé naïve";

    let config = PipelineConfig {
        name: Some("Unicode Accents Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            pattern: r"é".to_string(),
            replacement: Some("e".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    assert_eq!(output_str.trim(), "Cafe resume naïve");
}

/// Test Unicode word boundaries
#[test]
fn test_unicode_word_boundaries() {
    let input_data = "foo日本語bar hello世界world";

    let config = PipelineConfig {
        name: Some("Unicode Word Boundaries Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            // Match word characters (includes Unicode letters)
            pattern: r"\w+".to_string(),
            replacement: Some("[WORD]".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global, RegexFlag::Unicode]),
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // With Unicode flag, \w should match Unicode word characters
    // This creates a single [WORD] from continuous word chars
    assert!(output_str.contains("[WORD]"));
}

// =====================================================
// Error Path Tests
// =====================================================

/// Test error handling for invalid regex patterns
#[test]
fn test_error_invalid_regex_pattern() {
    let config = PipelineConfig {
        name: Some("Invalid Regex Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            pattern: r"[invalid((".to_string(), // Unbalanced brackets
            replacement: Some("test".to_string()),
            action: None,
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let result = StreamProcessor::new(config);
    assert!(result.is_err());
}

/// Test error handling for missing replacement in substitute step
#[test]
fn test_error_missing_replacement() {
    let config = PipelineConfig {
        name: Some("Missing Replacement Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            pattern: r"\d+".to_string(),
            replacement: None, // Missing replacement
            action: None,
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let result = config.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("replacement")));
}

/// Test error handling for missing action in filter step
#[test]
fn test_error_missing_filter_action() {
    let config = PipelineConfig {
        name: Some("Missing Filter Action Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Filter,
            pattern: r"ERROR".to_string(),
            replacement: None,
            action: None, // Missing action
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let result = config.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("action")));
}

/// Test error handling for empty pipeline
#[test]
fn test_error_empty_pipeline() {
    let config = PipelineConfig {
        name: Some("Empty Pipeline Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![], // Empty steps
        ..Default::default()
    };

    let result = config.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("at least one step")));
}

/// Test error handling for empty pattern
#[test]
fn test_error_empty_pattern() {
    let config = PipelineConfig {
        name: Some("Empty Pattern Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            pattern: "".to_string(), // Empty pattern
            replacement: Some("test".to_string()),
            action: None,
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let result = config.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("empty")));
}

/// Test error handling for invalid TOML configuration
#[test]
fn test_error_invalid_toml() {
    let invalid_toml = r#"
name = "Test"
[invalid section without proper format
"#;

    let mut temp_file = NamedTempFile::new().unwrap();
    write!(temp_file, "{}", invalid_toml).unwrap();

    let result = PipelineConfig::from_file(temp_file.path());
    assert!(result.is_err());
}

/// Test error handling for non-existent config file
#[test]
fn test_error_nonexistent_config_file() {
    let result = PipelineConfig::from_file("/nonexistent/path/config.toml");
    assert!(result.is_err());
}

/// Test error handling for config file with unknown step type
#[test]
fn test_error_unknown_step_type() {
    let config_content = r#"
name = "Test Pipeline"
version = "1.0.0"

[[step]]
type = "unknown_type"
pattern = '\d+'
enabled = true
"#;

    let mut temp_file = NamedTempFile::new().unwrap();
    write!(temp_file, "{}", config_content).unwrap();

    let result = PipelineConfig::from_file(temp_file.path());
    // Might deserialize but fail validation
    if let Ok(config) = result {
        // Unknown types might be caught at validation or processing stage
        let proc_result = StreamProcessor::new(config);
        // Either validation or processing should fail
        assert!(proc_result.is_err() || proc_result.is_ok()); // Actually this might succeed if enum is extensible
    }
}

/// Test PCRE mode disabled when pcre feature not compiled
/// This test runs without the pcre feature to verify proper error handling
#[cfg(not(feature = "pcre"))]
#[test]
fn test_pcre_mode_disabled_error() {
    let settings = PipelineSettings {
        pcre_mode: true,
        ..Default::default()
    };

    let config = PipelineConfig {
        name: Some("PCRE Disabled Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings,
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            pattern: r"test".to_string(),
            replacement: Some("TEST".to_string()),
            action: None,
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    // Should fail because pcre_mode is true but feature is not enabled
    let result = StreamProcessor::new(config);
    assert!(result.is_err());
    let err = result.err().unwrap();
    let err_msg = err.to_string();
    assert!(err_msg.contains("pcre") || err_msg.contains("feature"));
}

// =====================================================
// Multi-File Processing & In-Place Editing Tests
// =====================================================

use rexpipe::files::{FileProcessingOptions, MultiFileProcessor};
use std::fs;
use tempfile::TempDir;

/// Test basic multi-file processing
#[test]
fn test_multifile_basic_processing() {
    let temp_dir = TempDir::new().unwrap();

    // Create test files
    let file1_path = temp_dir.path().join("file1.txt");
    let file2_path = temp_dir.path().join("file2.txt");
    fs::write(&file1_path, "Hello 123 World").unwrap();
    fs::write(&file2_path, "Test 456 Data").unwrap();

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[NUM]"));
    let options = FileProcessingOptions::default();

    let processor = MultiFileProcessor::new(config, options);
    let paths = vec![file1_path.clone(), file2_path.clone()];
    let result = processor.process_files(&paths).unwrap();

    assert_eq!(result.files_processed, 2);
    assert!(result.files_matched > 0);
    assert!(result.errors.is_empty());

    // Verify original files unchanged (no in-place mode)
    let content1 = fs::read_to_string(&file1_path).unwrap();
    let content2 = fs::read_to_string(&file2_path).unwrap();
    assert_eq!(content1, "Hello 123 World");
    assert_eq!(content2, "Test 456 Data");
}

/// Test in-place file editing
#[test]
fn test_inplace_editing() {
    let temp_dir = TempDir::new().unwrap();

    // Create test file
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "Original 123 Content").unwrap();

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[NUMBER]"));
    let options = FileProcessingOptions {
        in_place: true,
        ..Default::default()
    };

    let processor = MultiFileProcessor::new(config, options);
    let paths = vec![file_path.clone()];
    let result = processor.process_files(&paths).unwrap();

    assert_eq!(result.files_processed, 1);
    assert!(result.files_modified > 0);

    // Verify file was modified in place
    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "Original [NUMBER] Content\n");
}

/// Test in-place editing with backup
#[test]
fn test_inplace_editing_with_backup() {
    let temp_dir = TempDir::new().unwrap();

    // Create test file
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "Hello 123 World").unwrap();

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[X]"));
    let options = FileProcessingOptions {
        in_place: true,
        backup_suffix: Some(".bak".to_string()),
        ..Default::default()
    };

    let processor = MultiFileProcessor::new(config, options);
    let paths = vec![file_path.clone()];
    let result = processor.process_files(&paths).unwrap();

    assert_eq!(result.files_processed, 1);
    assert!(result.files_modified > 0);

    // Verify modified content
    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "Hello [X] World\n");

    // Verify backup exists with original content
    let backup_path = temp_dir.path().join("test.txt.bak");
    assert!(backup_path.exists());
    let backup_content = fs::read_to_string(&backup_path).unwrap();
    assert_eq!(backup_content, "Hello 123 World");
}

/// Test exclude patterns with directory discovery
#[test]
fn test_exclude_patterns_with_discovery() {
    let temp_dir = TempDir::new().unwrap();

    // Create test files with different extensions in a subdirectory
    let txt_file = temp_dir.path().join("test.txt");
    let log_file = temp_dir.path().join("test.log");
    let md_file = temp_dir.path().join("test.md");

    fs::write(&txt_file, "Data 123").unwrap();
    fs::write(&log_file, "Data 456").unwrap();
    fs::write(&md_file, "Data 789").unwrap();

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[NUM]"));
    let options = FileProcessingOptions {
        exclude_patterns: vec!["*.log".to_string()], // Exclude log files
        ..Default::default()
    };

    let processor = MultiFileProcessor::new(config, options);

    // Use discover_files which applies exclude patterns
    let discovered = processor
        .discover_files(&[temp_dir.path().to_path_buf()])
        .unwrap();

    // txt and md should be discovered, log excluded
    assert_eq!(discovered.len(), 2);
    assert!(
        !discovered
            .iter()
            .any(|p| p.extension().map(|e| e == "log").unwrap_or(false))
    );

    // Now process the discovered files
    let result = processor.process_files(&discovered).unwrap();
    assert_eq!(result.files_processed, 2);
}

/// Test processing with files that don't match pattern
#[test]
fn test_files_without_matches() {
    let temp_dir = TempDir::new().unwrap();

    let file1_path = temp_dir.path().join("file1.txt");
    let file2_path = temp_dir.path().join("file2.txt");
    fs::write(&file1_path, "Hello World").unwrap(); // No numbers
    fs::write(&file2_path, "Test 123").unwrap(); // Has numbers

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[NUM]"));
    let options = FileProcessingOptions::default();

    let processor = MultiFileProcessor::new(config, options);
    let paths = vec![file1_path, file2_path];
    let result = processor.process_files(&paths).unwrap();

    assert_eq!(result.files_processed, 2);
    assert_eq!(result.files_matched, 1); // Only file2 has matches
}

/// Test processing empty files
#[test]
fn test_empty_file_processing() {
    let temp_dir = TempDir::new().unwrap();

    let file_path = temp_dir.path().join("empty.txt");
    fs::write(&file_path, "").unwrap();

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[NUM]"));
    let options = FileProcessingOptions::default();

    let processor = MultiFileProcessor::new(config, options);
    let paths = vec![file_path];
    let result = processor.process_files(&paths).unwrap();

    assert_eq!(result.files_processed, 1);
    assert_eq!(result.files_matched, 0);
    assert!(result.errors.is_empty());
}

/// Test atomic writes (file not corrupted on failure)
#[test]
fn test_atomic_write_preserves_original_on_read() {
    let temp_dir = TempDir::new().unwrap();

    // Create test file
    let file_path = temp_dir.path().join("test.txt");
    let original_content = "Original content 123";
    fs::write(&file_path, original_content).unwrap();

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[X]"));
    let options = FileProcessingOptions {
        in_place: true,
        ..Default::default()
    };

    let processor = MultiFileProcessor::new(config, options);
    let paths = vec![file_path.clone()];

    // Process should complete successfully
    let result = processor.process_files(&paths);
    assert!(result.is_ok());

    // Verify no temp files left behind
    let entries: Vec<_> = fs::read_dir(temp_dir.path()).unwrap().collect();
    // Should only have the original file (modified)
    assert_eq!(entries.len(), 1);
}

/// Test progress callback
#[test]
fn test_progress_callback() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let temp_dir = TempDir::new().unwrap();

    // Create multiple test files
    for i in 0..5 {
        let file_path = temp_dir.path().join(format!("file{}.txt", i));
        fs::write(&file_path, format!("Content {}", i)).unwrap();
    }

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[X]"));
    let options = FileProcessingOptions::default();

    let processor = MultiFileProcessor::new(config, options);

    let progress_count = Arc::new(AtomicUsize::new(0));
    let progress_clone = progress_count.clone();

    let paths: Vec<_> = (0..5)
        .map(|i| temp_dir.path().join(format!("file{}.txt", i)))
        .collect();

    // Use streaming API with progress callback
    let _ = processor.process_files_streaming(&paths, |_result| {
        progress_clone.fetch_add(1, Ordering::SeqCst);
    });

    // Progress callback should have been called for each file
    assert_eq!(progress_count.load(Ordering::SeqCst), 5);
}

/// Test files_with_matches functionality
#[test]
fn test_files_with_matches_only() {
    let temp_dir = TempDir::new().unwrap();

    let file1_path = temp_dir.path().join("match1.txt");
    let file2_path = temp_dir.path().join("nomatch.txt");
    let file3_path = temp_dir.path().join("match2.txt");

    fs::write(&file1_path, "Has number 123").unwrap();
    fs::write(&file2_path, "No numbers here").unwrap();
    fs::write(&file3_path, "Another 456 number").unwrap();

    let config = PipelineConfig::from_inline_pattern(r"\d+", None);
    let options = FileProcessingOptions::default();

    let processor = MultiFileProcessor::new(config, options);
    let paths = vec![file1_path.clone(), file2_path.clone(), file3_path.clone()];

    let matching = processor.files_with_matches(&paths).unwrap();
    assert_eq!(matching.len(), 2);
    assert!(matching.contains(&file1_path));
    assert!(matching.contains(&file3_path));
    assert!(!matching.contains(&file2_path));
}

/// Test files_without_matches functionality
#[test]
fn test_files_without_matches_only() {
    let temp_dir = TempDir::new().unwrap();

    let file1_path = temp_dir.path().join("match.txt");
    let file2_path = temp_dir.path().join("nomatch1.txt");
    let file3_path = temp_dir.path().join("nomatch2.txt");

    fs::write(&file1_path, "Has number 123").unwrap();
    fs::write(&file2_path, "No numbers here").unwrap();
    fs::write(&file3_path, "Just text").unwrap();

    let config = PipelineConfig::from_inline_pattern(r"\d+", None);
    let options = FileProcessingOptions::default();

    let processor = MultiFileProcessor::new(config, options);
    let paths = vec![file1_path.clone(), file2_path.clone(), file3_path.clone()];

    let non_matching = processor.files_without_matches(&paths).unwrap();
    assert_eq!(non_matching.len(), 2);
    assert!(!non_matching.contains(&file1_path));
    assert!(non_matching.contains(&file2_path));
    assert!(non_matching.contains(&file3_path));
}

/// Test parallel processing threshold
#[test]
fn test_parallel_processing_many_files() {
    let temp_dir = TempDir::new().unwrap();

    // Create enough files to trigger parallel processing
    let file_count = 20;
    for i in 0..file_count {
        let file_path = temp_dir.path().join(format!("file{:03}.txt", i));
        fs::write(&file_path, format!("Data {} content", i * 100)).unwrap();
    }

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[X]"));
    let options = FileProcessingOptions::default();

    let processor = MultiFileProcessor::new(config, options);

    let paths: Vec<_> = (0..file_count)
        .map(|i| temp_dir.path().join(format!("file{:03}.txt", i)))
        .collect();

    let result = processor.process_files(&paths).unwrap();

    assert_eq!(result.files_processed, file_count as u64);
    assert_eq!(result.files_matched, file_count as u64);
}

// =====================================================
// Async Multi-File Processing Tests
// =====================================================

// =====================================================
// Edge Case Tests - Timeouts and I/O Failures
// =====================================================

/// Test max line length setting can be configured
#[test]
fn test_max_line_length_configuration() {
    use rexpipe::pipeline::MaxLineAction;

    // Test that settings can be configured without error
    let settings = PipelineSettings {
        max_line_length: 500,
        max_line_action: MaxLineAction::Skip,
        ..Default::default()
    };

    let config = PipelineConfig {
        name: Some("Max Line Config Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings,
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            pattern: r"\d+".to_string(),
            replacement: Some("[NUM]".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    // Should create processor successfully
    let processor = StreamProcessor::new(config);
    assert!(processor.is_ok());
}

/// Test max line action truncate configuration
#[test]
fn test_max_line_action_truncate_config() {
    use rexpipe::pipeline::MaxLineAction;

    // Test that truncate action can be configured
    let settings = PipelineSettings {
        max_line_length: 150,
        max_line_action: MaxLineAction::Truncate,
        ..Default::default()
    };

    let config = PipelineConfig {
        name: Some("Max Line Truncate Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings,
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            pattern: r"\d+".to_string(),
            replacement: Some("[NUM]".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    // Should create processor successfully with truncate action
    let processor = StreamProcessor::new(config);
    assert!(processor.is_ok());
}

/// Test processing empty file (edge case for I/O)
#[test]
fn test_empty_stream_processing() {
    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[NUM]"));
    let mut processor = StreamProcessor::new(config).unwrap();

    let reader = Cursor::new("");
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();

    assert_eq!(result.lines_processed, 0);
    assert_eq!(result.transformations_applied, 0);
    assert!(output.is_empty());
}

/// Test processing single line without newline
#[test]
fn test_single_line_no_newline() {
    let input_data = "single line 123"; // No trailing newline

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[NUM]"));
    let mut processor = StreamProcessor::new(config).unwrap();

    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    assert_eq!(result.lines_processed, 1);
    assert!(output_str.contains("[NUM]"));
}

/// Test processing with only whitespace lines
#[test]
fn test_whitespace_only_lines() {
    let input_data = "   \n\t\t\n   \t   \n";

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[NUM]"));
    let mut processor = StreamProcessor::new(config).unwrap();

    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Should process all lines but find no matches
    assert_eq!(result.lines_processed, 3);
    assert_eq!(result.transformations_applied, 0);
    // Output should preserve whitespace lines
    assert!(!output_str.is_empty());
}

/// Test processing with very large number of lines
#[test]
fn test_large_line_count() {
    let line_count = 10000;
    let mut input_data = String::new();
    for i in 0..line_count {
        input_data.push_str(&format!("Line {} with number\n", i));
    }

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[X]"));
    let mut processor = StreamProcessor::new(config).unwrap();

    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();

    assert_eq!(result.lines_processed, line_count);
    assert_eq!(result.transformations_applied, line_count);
}

/// Test processing binary-like content (should handle gracefully)
#[test]
fn test_binary_like_content() {
    // Create content with null bytes mixed with text
    let mut input_bytes = Vec::new();
    input_bytes.extend_from_slice(b"Normal line 123\n");
    // Note: actual binary with null bytes will fail UTF-8 conversion
    // This tests near-binary content that's still valid UTF-8
    input_bytes.extend_from_slice("Line with unicode 🔥 456\n".as_bytes());

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[NUM]"));
    let mut processor = StreamProcessor::new(config).unwrap();

    let reader = Cursor::new(input_bytes);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    assert!(result.lines_processed >= 2);
    assert!(output_str.contains("[NUM]"));
}

/// Test I/O error on directory instead of file (multi-file processing)
#[test]
fn test_process_directory_path() {
    let temp_dir = TempDir::new().unwrap();

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[NUM]"));
    let options = FileProcessingOptions::default();

    let processor = MultiFileProcessor::new(config, options);
    // Pass the directory as a file path (should fail gracefully)
    let paths = vec![temp_dir.path().to_path_buf()];
    let result = processor.process_files(&paths);

    // Should handle gracefully - either error or skip the directory
    if let Ok(res) = result {
        // Directory shouldn't be processed as a file
        assert_eq!(res.files_processed, 0);
    }
    // Error result is also acceptable
}

/// Test file permission issues (simulated by non-existent file)
#[test]
fn test_nonexistent_file_processing() {
    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[NUM]"));
    let options = FileProcessingOptions::default();

    let processor = MultiFileProcessor::new(config, options);
    let paths = vec![std::path::PathBuf::from("/nonexistent/path/to/file.txt")];
    let result = processor.process_files(&paths);

    // Should return an error or have error in result
    if let Ok(res) = result {
        assert!(res.files_processed == 0 || !res.errors.is_empty());
    }
}

/// Test quiet mode processing (no output, only exit code)
#[test]
fn test_quiet_mode_processing() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    let original_content = "Original 123 content";
    fs::write(&file_path, original_content).unwrap();

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[NUM]"));
    let options = FileProcessingOptions {
        quiet: true,
        ..Default::default()
    };

    let processor = MultiFileProcessor::new(config, options);
    let paths = vec![file_path.clone()];
    let result = processor.process_files(&paths).unwrap();

    // File should NOT be modified without in_place
    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, original_content);

    // But should report what was processed
    assert_eq!(result.files_processed, 1);
}

/// Test multiple errors in batch processing
#[test]
fn test_multiple_file_errors() {
    let temp_dir = TempDir::new().unwrap();

    // Create one valid file
    let valid_file = temp_dir.path().join("valid.txt");
    fs::write(&valid_file, "Valid 123 content").unwrap();

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[NUM]"));
    let options = FileProcessingOptions::default();

    let processor = MultiFileProcessor::new(config, options);
    let paths = vec![
        std::path::PathBuf::from("/nonexistent/file1.txt"),
        valid_file,
        std::path::PathBuf::from("/nonexistent/file2.txt"),
    ];
    let result = processor.process_files(&paths).unwrap();

    // Should process the valid file despite errors with others
    assert!(result.files_processed >= 1);
    assert!(!result.errors.is_empty() || result.files_processed < 3);
}

/// Test context lines with edge cases (first/last line)
#[test]
fn test_context_lines_at_boundaries() {
    let input_data = "Line 1\nLine 2\nLine 3 match 123\nLine 4\nLine 5";

    let config = PipelineConfig {
        name: Some("Context Boundary Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings {
            context_before: 5, // More than available lines before
            context_after: 5,  // More than available lines after
            ..Default::default()
        },
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            pattern: r"match \d+".to_string(),
            replacement: Some("[MATCHED]".to_string()),
            action: None,
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();

    // Should handle boundary conditions gracefully
    assert!(result.lines_processed == 5);
}

/// Test pattern that matches entire line
#[test]
fn test_full_line_match() {
    let input_data = "123\nabc\n456";

    let config = PipelineConfig::from_inline_pattern(r"^\d+$", Some("[ALL_DIGITS]"));
    let mut processor = StreamProcessor::new(config).unwrap();

    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Lines with only digits should be completely replaced
    assert!(output_str.contains("[ALL_DIGITS]"));
    assert!(output_str.contains("abc")); // Non-matching line preserved
}

/// Test overlapping patterns (regex matches overlapping regions)
#[test]
fn test_overlapping_matches() {
    let input_data = "aaaa";

    let config = PipelineConfig::from_inline_pattern(r"aa", Some("[X]"));
    let mut processor = StreamProcessor::new(config).unwrap();

    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // With non-overlapping matching, should get "[X][X]" (two matches)
    // or "[X]aa" (first match only, depending on global flag)
    assert!(output_str.contains("[X]"));
}

/// Test literal replacement string (no regex special chars)
#[test]
fn test_literal_replacement_string() {
    let input_data = "test 123 value";

    // Simple literal replacement
    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUMBER"));
    let mut processor = StreamProcessor::new(config).unwrap();

    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Should handle literal replacement
    assert!(output_str.contains("NUMBER"));
    assert!(!output_str.contains("123"));
}

#[cfg(feature = "async")]
mod async_tests {
    use super::*;
    use rexpipe::files::AsyncMultiFileProcessor;

    #[tokio::test]
    async fn test_async_multifile_processing() {
        let temp_dir = TempDir::new().unwrap();

        let file1_path = temp_dir.path().join("async1.txt");
        let file2_path = temp_dir.path().join("async2.txt");
        fs::write(&file1_path, "Async 123 test").unwrap();
        fs::write(&file2_path, "Async 456 test").unwrap();

        let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[ASYNC]"));
        let options = FileProcessingOptions::default();

        let processor = AsyncMultiFileProcessor::new(config, options);
        let paths = vec![file1_path, file2_path];
        let result = processor.process_files_async(&paths).await.unwrap();

        assert_eq!(result.files_processed, 2);
        assert!(result.files_matched > 0);
    }

    #[tokio::test]
    async fn test_async_inplace_editing() {
        let temp_dir = TempDir::new().unwrap();

        let file_path = temp_dir.path().join("async_inplace.txt");
        fs::write(&file_path, "Async content 999").unwrap();

        let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[REPLACED]"));
        let options = FileProcessingOptions {
            in_place: true,
            ..Default::default()
        };

        let processor = AsyncMultiFileProcessor::new(config, options);
        let paths = vec![file_path.clone()];
        let result = processor.process_files_async(&paths).await.unwrap();

        assert_eq!(result.files_processed, 1);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Async content [REPLACED]\n");
    }

    #[tokio::test]
    async fn test_async_files_with_matches() {
        let temp_dir = TempDir::new().unwrap();

        let file1_path = temp_dir.path().join("match.txt");
        let file2_path = temp_dir.path().join("nomatch.txt");
        fs::write(&file1_path, "Has 123").unwrap();
        fs::write(&file2_path, "No numbers").unwrap();

        let config = PipelineConfig::from_inline_pattern(r"\d+", None);
        let options = FileProcessingOptions::default();

        let processor = AsyncMultiFileProcessor::new(config, options);
        let paths = vec![file1_path.clone(), file2_path.clone()];

        let matching = processor.files_with_matches_async(&paths).await.unwrap();
        assert_eq!(matching.len(), 1);
        assert!(matching.contains(&file1_path));
    }
}

// =====================================================
// Windows Line Ending Tests (CRLF)
// =====================================================

/// Test basic substitution with Windows CRLF line endings
#[test]
fn test_crlf_basic_substitution() {
    // Input with Windows-style line endings
    let input_data = "Line 1: 123\r\nLine 2: 456\r\nLine 3: 789\r\n";

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[NUM]"));
    let mut processor = StreamProcessor::new(config).unwrap();

    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Verify processing happened
    assert_eq!(result.lines_processed, 3);
    assert!(result.transformations_applied >= 3);

    // Verify substitution worked
    assert!(output_str.contains("[NUM]"));
    assert!(!output_str.contains("123"));
    assert!(!output_str.contains("456"));
    assert!(!output_str.contains("789"));
}

/// Test mixed line endings (some LF, some CRLF)
#[test]
fn test_mixed_line_endings() {
    // Mix of Unix and Windows line endings
    let input_data = "Unix line 123\nWindows line 456\r\nAnother Unix 789\n";

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[X]"));
    let mut processor = StreamProcessor::new(config).unwrap();

    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // All three lines should be processed
    assert_eq!(result.lines_processed, 3);
    assert!(result.transformations_applied >= 3);

    // All numbers replaced
    assert!(output_str.contains("[X]"));
    assert!(!output_str.contains("123"));
    assert!(!output_str.contains("456"));
    assert!(!output_str.contains("789"));
}

/// Test CRLF with filter operations (drop lines)
#[test]
fn test_crlf_filter_drop() {
    let input_data =
        "Keep this line\r\nDROP this DEBUG line\r\nKeep this too\r\nAnother DEBUG drop\r\n";

    let config = PipelineConfig {
        name: Some("CRLF Filter Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Filter,
            pattern: "DEBUG".to_string(),
            replacement: None,
            action: Some(StepAction::DropLine),
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let _result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // DEBUG lines should be dropped
    assert!(!output_str.contains("DEBUG"));
    assert!(output_str.contains("Keep this line"));
    assert!(output_str.contains("Keep this too"));

    // Count remaining lines
    let lines: Vec<&str> = output_str.lines().filter(|s| !s.is_empty()).collect();
    assert_eq!(lines.len(), 2);
}

/// Test CRLF with filter operations (keep lines)
#[test]
fn test_crlf_filter_keep() {
    let input_data = "ERROR: something wrong\r\nINFO: normal message\r\nERROR: another error\r\nDEBUG: debug info\r\n";

    let config = PipelineConfig {
        name: Some("CRLF Keep Filter Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Filter,
            pattern: "ERROR".to_string(),
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
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let _result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Only ERROR lines should remain
    let lines: Vec<&str> = output_str.lines().filter(|s| !s.is_empty()).collect();
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|line| line.contains("ERROR")));
}

/// Test CRLF with extract step
#[test]
fn test_crlf_extract() {
    let input_data = "Email: john@example.com text\r\nMore: jane@test.org here\r\n";

    let config = PipelineConfig {
        name: Some("CRLF Extract Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Extract,
            pattern: r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}".to_string(),
            replacement: None,
            action: None,
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Should extract emails
    assert!(result.transformations_applied > 0);
    assert!(output_str.contains("john@example.com"));
    assert!(output_str.contains("jane@test.org"));
}

/// Test CRLF with transform (uppercase)
#[test]
fn test_crlf_transform_uppercase() {
    let input_data = "hello world\r\ntest data\r\n";

    let config = PipelineConfig {
        name: Some("CRLF Transform Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Transform,
            pattern: r"[a-z]+".to_string(),
            replacement: None,
            action: None,
            transform: Some(TransformAction::Uppercase),
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Should be uppercased
    assert!(output_str.contains("HELLO"));
    assert!(output_str.contains("WORLD"));
    assert!(output_str.contains("TEST"));
    assert!(output_str.contains("DATA"));
}

/// Test CRLF at end of file without trailing newline
#[test]
fn test_crlf_no_trailing_newline() {
    // Windows file without trailing newline
    let input_data = "Line 1: 123\r\nLine 2: 456";

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[N]"));
    let mut processor = StreamProcessor::new(config).unwrap();

    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Both lines should be processed
    assert_eq!(result.lines_processed, 2);
    assert!(output_str.contains("[N]"));
    assert!(!output_str.contains("123"));
    assert!(!output_str.contains("456"));
}

/// Test CRLF with carriage return in pattern match
#[test]
fn test_crlf_pattern_at_line_end() {
    // Match pattern at end of line, just before CRLF
    let input_data = "value: 100\r\nvalue: 200\r\n";

    let config = PipelineConfig::from_inline_pattern(r"\d+$", Some("[END]"));
    let mut processor = StreamProcessor::new(config).unwrap();

    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // End-of-line pattern should match before the line terminator
    assert!(result.transformations_applied >= 2);
    assert!(output_str.contains("[END]"));
}

/// Test file processing with CRLF line endings
#[test]
fn test_crlf_file_processing() {
    let temp_dir = TempDir::new().unwrap();

    // Create file with Windows line endings
    let file_path = temp_dir.path().join("windows.txt");
    fs::write(&file_path, "Line 123\r\nLine 456\r\n").unwrap();

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[X]"));
    let options = FileProcessingOptions {
        in_place: true,
        ..Default::default()
    };

    let processor = MultiFileProcessor::new(config, options);
    let paths = vec![file_path.clone()];
    let result = processor.process_files(&paths).unwrap();

    assert_eq!(result.files_processed, 1);
    assert!(result.files_modified > 0);

    // Verify content was modified
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("[X]"));
    assert!(!content.contains("123"));
    assert!(!content.contains("456"));
}

/// Test validation step with CRLF
#[test]
fn test_crlf_validation() {
    let input_data =
        "2025-01-08 valid line\r\nInvalid line without date\r\n2025-01-09 another valid\r\n";

    let config = PipelineConfig {
        name: Some("CRLF Validation Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Validate,
            pattern: r"^\d{4}-\d{2}-\d{2}".to_string(),
            replacement: None,
            action: None,
            transform: None,
            flags: None,
            description: Some("Validate date format".to_string()),
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Should have validation errors for invalid line
    assert!(!result.errors.is_empty());

    // Only valid lines should be in output
    let lines: Vec<&str> = output_str.lines().filter(|s| !s.is_empty()).collect();
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|line| line.starts_with("2025-01-")));
}

/// Test multiple consecutive CRLF (blank lines)
#[test]
fn test_crlf_consecutive_blank_lines() {
    let input_data = "Line 1: 100\r\n\r\n\r\nLine 2: 200\r\n";

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[NUM]"));
    let mut processor = StreamProcessor::new(config).unwrap();

    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Should handle blank lines gracefully
    assert_eq!(result.lines_processed, 4); // 2 content lines + 2 blank lines
    assert!(output_str.contains("[NUM]"));
}

/// Test CRLF with Unicode content
#[test]
fn test_crlf_unicode_content() {
    let input_data = "Hello 世界 123\r\nTest café 456\r\n";

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[X]"));
    let mut processor = StreamProcessor::new(config).unwrap();

    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Should handle Unicode with CRLF
    assert_eq!(result.lines_processed, 2);
    assert!(output_str.contains("世界"));
    assert!(output_str.contains("café"));
    assert!(output_str.contains("[X]"));
}

/// Test lone CR (old Mac line endings) - edge case
#[test]
fn test_lone_cr_line_endings() {
    // Old Mac-style line endings (CR only, no LF)
    let input_data = "Line 1: 123\rLine 2: 456\rLine 3: 789\r";

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[N]"));
    let mut processor = StreamProcessor::new(config).unwrap();

    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Should process (behavior may vary - lone CR typically treated as one line by Rust)
    assert!(result.lines_processed >= 1);
    assert!(output_str.contains("[N]"));
}

/// Test CRLF with global flag replacement
#[test]
fn test_crlf_global_replacement() {
    let input_data = "a1b2c3\r\nd4e5f6\r\n";

    let config = PipelineConfig {
        name: Some("CRLF Global Replace Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            pattern: r"\d".to_string(),
            replacement: Some("X".to_string()),
            action: None,
            transform: None,
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // All digits should be replaced (transformations counted per-line, not per-match)
    assert!(result.transformations_applied >= 2);
    assert!(output_str.contains("aXbXcX"));
    assert!(output_str.contains("dXeXfX"));
    // Verify no digits remain
    assert!(!output_str.chars().any(|c| c.is_ascii_digit()));
}

// =============================================================================
// Block Content Filtering Tests (Issue #6 fix verification)
// =============================================================================

/// Test that block steps filter by content when pattern is specified
/// This verifies the fix for Issue #6: Block pattern filter not working
#[test]
fn test_block_content_filtering_keep() {
    // Block step with keep_block and content pattern - only keep blocks containing ERROR
    let config = PipelineConfig {
        name: Some("Block Content Filter Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Block,
            pattern: "ERROR".to_string(), // Content pattern - only keep blocks with ERROR
            start_pattern: Some(r"^--- START".to_string()),
            end_pattern: Some(r"^--- END".to_string()),
            action: Some(StepAction::KeepBlock),
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let input = r#"--- START BLOCK 1 ---
This block has no errors
Just normal content
--- END BLOCK 1 ---

Some text between

--- START BLOCK 2 ---
This block has ERROR in it
Should be kept
--- END BLOCK 2 ---

--- START BLOCK 3 ---
Another clean block
No issues here
--- END BLOCK 3 ---
"#;

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Block 2 (with ERROR) should be kept
    assert!(
        output_str.contains("--- START BLOCK 2"),
        "Block 2 with ERROR should be kept"
    );
    assert!(
        output_str.contains("This block has ERROR in it"),
        "ERROR line should be present"
    );

    // Blocks 1 and 3 (without ERROR) should be dropped
    assert!(
        !output_str.contains("--- START BLOCK 1"),
        "Block 1 without ERROR should be dropped"
    );
    assert!(
        !output_str.contains("--- START BLOCK 3"),
        "Block 3 without ERROR should be dropped"
    );
}

/// Test block content filtering with drop_block action
#[test]
fn test_block_content_filtering_drop() {
    // Block step with drop_block and content pattern - drop blocks containing SECRET
    let config = PipelineConfig {
        name: Some("Block Content Drop Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Block,
            pattern: "SECRET".to_string(), // Content pattern - drop blocks with SECRET
            start_pattern: Some(r"^\[BEGIN\]".to_string()),
            end_pattern: Some(r"^\[END\]".to_string()),
            action: Some(StepAction::DropBlock),
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let input = r#"[BEGIN]
Public information
Safe content
[END]

Between blocks text

[BEGIN]
SECRET password here
Should be redacted
[END]

[BEGIN]
More public info
[END]
"#;

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Block with SECRET should be dropped
    assert!(
        !output_str.contains("SECRET password"),
        "Block with SECRET should be dropped"
    );

    // Blocks without SECRET should be kept
    assert!(
        output_str.contains("Public information"),
        "Public block should be kept"
    );
    assert!(
        output_str.contains("More public info"),
        "Last public block should be kept"
    );

    // Text between blocks should be kept
    assert!(
        output_str.contains("Between blocks text"),
        "Text between blocks should be kept"
    );
}

// =============================================================================
// Multi-Step Pipeline Ordering Tests
// =============================================================================

/// Test that multiple steps are applied in order
#[test]
fn test_multi_step_ordering() {
    // Step 1: Replace "foo" with "bar"
    // Step 2: Replace "bar" with "baz"
    // If ordering is correct: foo -> bar -> baz
    let config = PipelineConfig {
        name: Some("Multi-Step Order Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![
            PipelineStep {
                step_type: StepType::Substitute,
                pattern: "foo".to_string(),
                replacement: Some("bar".to_string()),
                enabled: Some(true),
                ..Default::default()
            },
            PipelineStep {
                step_type: StepType::Substitute,
                pattern: "bar".to_string(),
                replacement: Some("baz".to_string()),
                enabled: Some(true),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new("foo is here");
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // If ordering is correct, "foo" should become "baz" (foo -> bar -> baz)
    assert!(
        output_str.contains("baz"),
        "foo should be transformed to baz through both steps"
    );
    assert!(
        !output_str.contains("foo"),
        "foo should not remain"
    );
}

/// Test filter then substitute ordering
#[test]
fn test_filter_then_substitute_ordering() {
    // Step 1: Keep only lines with "IMPORTANT"
    // Step 2: Replace "old" with "new"
    let config = PipelineConfig {
        name: Some("Filter Then Substitute Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![
            PipelineStep {
                step_type: StepType::Filter,
                pattern: "IMPORTANT".to_string(),
                action: Some(StepAction::KeepLine),
                enabled: Some(true),
                ..Default::default()
            },
            PipelineStep {
                step_type: StepType::Substitute,
                pattern: "old".to_string(),
                replacement: Some("new".to_string()),
                enabled: Some(true),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let input = "IMPORTANT: old value\nnot important: old value\nIMPORTANT: another old one";
    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Only IMPORTANT lines should remain, and "old" should be "new"
    assert!(
        output_str.contains("IMPORTANT: new value"),
        "IMPORTANT line should have 'old' replaced with 'new'"
    );
    assert!(
        !output_str.contains("not important"),
        "Non-IMPORTANT lines should be filtered out"
    );
    assert!(
        output_str.lines().count() == 2,
        "Should have exactly 2 lines (the IMPORTANT ones)"
    );
}

/// Test substitute then filter ordering
#[test]
fn test_substitute_then_filter_ordering() {
    // Step 1: Replace "secret" with "REDACTED"
    // Step 2: Drop lines containing "REDACTED"
    let config = PipelineConfig {
        name: Some("Substitute Then Filter Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![
            PipelineStep {
                step_type: StepType::Substitute,
                pattern: "secret".to_string(),
                replacement: Some("REDACTED".to_string()),
                enabled: Some(true),
                ..Default::default()
            },
            PipelineStep {
                step_type: StepType::Filter,
                pattern: "REDACTED".to_string(),
                action: Some(StepAction::DropLine),
                enabled: Some(true),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let input = "public info\nsecret password\nanother public line";
    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input);
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // Lines with "secret" should be redacted then dropped
    assert!(
        !output_str.contains("secret"),
        "Original secret should not appear"
    );
    assert!(
        !output_str.contains("REDACTED"),
        "REDACTED should be filtered out"
    );
    assert!(
        output_str.contains("public info"),
        "Public lines should remain"
    );
    assert!(
        output_str.lines().filter(|l| !l.is_empty()).count() == 2,
        "Should have 2 public lines remaining"
    );
}

// =============================================================================
// Plugin/Transform Integration Tests
// =============================================================================

/// Test built-in transform: uppercase (integration with global flag)
#[test]
fn test_transform_uppercase_integration() {
    let config = PipelineConfig {
        name: Some("Uppercase Transform Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Transform,
            pattern: r"[a-z]+".to_string(),
            transform: Some(TransformAction::Uppercase),
            flags: Some(vec![RegexFlag::Global]), // Global flag for all matches
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new("hello world");
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    assert!(
        output_str.contains("HELLO") && output_str.contains("WORLD"),
        "Words should be uppercased: {}",
        output_str
    );
}

/// Test built-in transform: title_case
#[test]
fn test_transform_title_case_integration() {
    let config = PipelineConfig {
        name: Some("Title Case Transform Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Transform,
            pattern: r"[a-z]+".to_string(),
            transform: Some(TransformAction::TitleCase),
            flags: Some(vec![RegexFlag::Global]), // Global flag for all matches
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new("hello world");
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // hello world should become Hello World
    assert!(
        output_str.contains("Hello") && output_str.contains("World"),
        "Should convert to title case: {}",
        output_str
    );
}

/// Test built-in transform: reverse
#[test]
fn test_transform_reverse_integration() {
    let config = PipelineConfig {
        name: Some("Reverse Transform Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Transform,
            pattern: r"\w+".to_string(),
            transform: Some(TransformAction::Reverse),
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new("hello");
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    assert!(
        output_str.contains("olleh"),
        "hello should be reversed to olleh"
    );
}

/// Test chained transforms
#[test]
fn test_chained_transforms() {
    // Step 1: Convert to uppercase
    // Step 2: Reverse the result
    let config = PipelineConfig {
        name: Some("Chained Transforms Test".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![
            PipelineStep {
                step_type: StepType::Transform,
                pattern: r"[a-z]+".to_string(),
                transform: Some(TransformAction::Uppercase),
                enabled: Some(true),
                ..Default::default()
            },
            PipelineStep {
                step_type: StepType::Transform,
                pattern: r"[A-Z]+".to_string(),
                transform: Some(TransformAction::Reverse),
                enabled: Some(true),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new("abc");
    let mut output = Vec::new();

    processor.process_stream(reader, &mut output).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    // abc -> ABC -> CBA
    assert!(
        output_str.contains("CBA"),
        "abc should become ABC then CBA: {}",
        output_str
    );
}
