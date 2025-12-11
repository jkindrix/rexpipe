use rexpipe::compass::CompassAgent;
use rexpipe::pipeline::{PipelineConfig, PipelineSettings, PipelineStep, StepType, FilterAction, RegexFlag};
use rexpipe::processor::StreamProcessor;
use rexpipe::inspector::{Inspector, InspectorOptions};
use std::io::Cursor;
use tempfile::NamedTempFile;
use std::io::Write;

#[test]
fn test_compass_agent_full_workflow() {
    let mut agent = CompassAgent::new();
    
    // Execute complete COMPASS workflow
    assert!(agent.clarify_intent("Build regex pipeline processor").is_ok());
    assert!(agent.advance_phase().is_ok());
    
    assert!(agent.orient_research("Existing tools fragmented").is_ok());
    assert!(agent.advance_phase().is_ok());
    
    assert!(agent.map_solution().is_ok());
    assert!(agent.advance_phase().is_ok());
    
    assert!(agent.validate_strategy().is_ok());
    assert!(agent.advance_phase().is_ok());
    
    assert!(agent.architect_implementation().is_ok());
    assert!(agent.advance_phase().is_ok());
    
    assert!(agent.synthesize_final().is_ok());
    
    let report = agent.generate_report();
    assert!(report.contains("COMPASS Agent Execution Report"));
    assert!(report.contains("Confidence: "));
}

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
        settings: PipelineSettings::default(),
        step: vec![
            PipelineStep {
                step_type: StepType::Substitute,
                pattern: r"\[ERROR\]".to_string(),
                replacement: Some("[ERR]".to_string()),
                action: None,
                flags: Some(vec![RegexFlag::Global]),
                description: None,
                enabled: Some(true),
            },
            PipelineStep {
                step_type: StepType::Filter,
                pattern: "DEBUG".to_string(),
                replacement: None,
                action: Some(FilterAction::DropLine),
                flags: None,
                description: None,
                enabled: Some(true),
            },
            PipelineStep {
                step_type: StepType::Substitute,
                pattern: r"user_id=(\d+)".to_string(),
                replacement: Some("uid=${1}".to_string()),
                action: None,
                flags: Some(vec![RegexFlag::Global]),
                description: None,
                enabled: Some(true),
            },
        ],
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
    assert!(result.total_matches >= 3); // Should find 123, 456, 789
    assert_eq!(result.line_matches.len(), 1);
    
    let line_match = &result.line_matches[0];
    assert_eq!(line_match.line_number, 1);
    assert!(line_match.matches.len() >= 3);
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
        settings: PipelineSettings::default(),
        step: vec![
            PipelineStep {
                step_type: StepType::Filter,
                pattern: "DEBUG".to_string(),
                replacement: None,
                action: Some(FilterAction::DropLine),
                flags: None,
                description: None,
                enabled: Some(true),
            },
        ],
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
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
        settings: PipelineSettings::default(),
        step: vec![
            PipelineStep {
                step_type: StepType::Validate,
                pattern: r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}".to_string(),
                replacement: None,
                action: None,
                flags: None,
                description: Some("Validate timestamp format".to_string()),
                enabled: Some(true),
            },
        ],
    };

    let mut processor = StreamProcessor::new(config).unwrap();
    let reader = Cursor::new(input_data);
    let mut output = Vec::new();

    let result = processor.process_stream(reader, &mut output).unwrap();
    
    // Should have validation errors
    assert!(result.errors.len() > 0);
    
    let output_str = String::from_utf8(output).unwrap();
    let lines: Vec<&str> = output_str.trim().split('\n').filter(|s| !s.is_empty()).collect();
    
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
        settings: PipelineSettings::default(),
        step: vec![
            PipelineStep {
                step_type: StepType::Extract,
                pattern: r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}".to_string(),
                replacement: None,
                action: None,
                flags: None,
                description: Some("Extract email addresses".to_string()),
                enabled: Some(true),
            },
        ],
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
        settings: PipelineSettings::default(),
        step: vec![
            PipelineStep {
                step_type: StepType::Substitute,
                pattern: r"[invalid regex(".to_string(), // Invalid regex
                replacement: Some("replacement".to_string()),
                action: None,
                flags: None,
                description: None,
                enabled: Some(true),
            },
        ],
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
        settings: PipelineSettings::default(),
        step: vec![
            PipelineStep {
                step_type: StepType::Substitute,
                pattern: r"\d+".to_string(),
                replacement: Some("NUMBER".to_string()),
                action: None,
                flags: Some(vec![RegexFlag::Global]),
                description: None,
                enabled: Some(false), // Disabled step
            },
        ],
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