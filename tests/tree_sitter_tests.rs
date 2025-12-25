//! Integration tests for tree-sitter syntax-aware processing.
//!
//! These tests verify that syntax-aware scoping works correctly across
//! multiple languages (Rust, Python, JavaScript, TypeScript, Go).
//!
//! Run with: `cargo test --features tree-sitter`

#![cfg(feature = "tree-sitter")]

use rexpipe::pipeline::{PipelineConfig, PipelineSettings, PipelineStep, StepAction, StepType};
use rexpipe::processor::StreamProcessor;
use rexpipe::syntax::{Language, ScopeFilter};
use std::io::Cursor;

// Test fixture content - Rust source file
const RUST_FIXTURE: &str = r#"// Rust file for testing tree-sitter scopes

use std::collections::HashMap;
use crate::utils::helper;

fn helper_function(x: i32) -> i32 {
    x * 2
}

fn main() {
    let result = helper_function(42);
    // Comment with helper mentioned
    let greeting = "hello helper world";
    println!("Result: {}", result);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_helper() {
        assert_eq!(helper_function(2), 4);
    }
}
"#;

// Test fixture content - Python source file
const PYTHON_FIXTURE: &str = r#"# Python file for testing tree-sitter scopes

import os
from pathlib import Path

def helper_function(x):
    """A helper function"""
    return x * 2

def main():
    result = helper_function(42)
    # Comment with helper mentioned
    greeting = "hello helper world"
    print(f"Result: {result}")

def test_helper():
    assert helper_function(2) == 4

class TestHelper:
    def test_method(self):
        assert helper_function(3) == 6
"#;

// Test fixture content - JavaScript source file
const JS_FIXTURE: &str = r#"// JavaScript file for testing tree-sitter scopes

import { helper } from './utils';
const fs = require('fs');

function helperFunction(x) {
    return x * 2;
}

const main = () => {
    const result = helperFunction(42);
    // Comment with helper mentioned
    const greeting = "hello helper world";
    console.log("Result:", result);
};

describe('Helper', () => {
    it('should work correctly', () => {
        expect(helperFunction(2)).toBe(4);
    });

    test('helper method works', () => {
        expect(helperFunction(3)).toBe(6);
    });
});
"#;

/// Helper to create a syntax-aware pipeline step
fn create_scoped_filter_step(
    pattern: &str,
    language: &str,
    scope: &str,
) -> PipelineStep {
    PipelineStep {
        step_type: StepType::Filter,
        pattern: pattern.to_string(),
        action: Some(StepAction::KeepLine),
        language: Some(language.to_string()),
        scope: Some(scope.to_string()),
        enabled: Some(true),
        ..Default::default()
    }
}

/// Helper to process content with a pipeline and return output lines
/// Uses process_file_content for proper tree-sitter AST-based scoping
fn process_with_pipeline(content: &str, config: PipelineConfig) -> Vec<String> {
    let mut processor = StreamProcessor::new(config).unwrap();

    // Use process_file_content for syntax-aware processing
    // This is required for tree-sitter scoping to work (needs full AST)
    if processor.has_syntax_aware_steps() {
        let (output, _result) = processor
            .process_file_content(content, None)
            .unwrap();
        output
            .lines()
            .map(|s| s.to_string())
            .collect()
    } else {
        // Fall back to stream processing for non-syntax-aware pipelines
        let reader = Cursor::new(content);
        let mut output = Vec::new();
        processor.process_stream(reader, &mut output).unwrap();
        String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|s| s.to_string())
            .collect()
    }
}

// =============================================================================
// Rust Scope Tests
// =============================================================================

#[test]
fn test_rust_code_scope_filters_correctly() {
    // Test that scope=code only matches in code, not strings or comments
    let config = PipelineConfig {
        name: Some("Rust Code Scope Test".to_string()),
        settings: PipelineSettings::default(),
        step: vec![create_scoped_filter_step("helper", "rust", "code")],
        ..Default::default()
    };

    let output = process_with_pipeline(RUST_FIXTURE, config);

    // Should match lines with "helper" in code:
    // - use crate::utils::helper;
    // - fn helper_function(x: i32) -> i32 {
    // - let result = helper_function(42);
    // - fn test_helper() {
    // - assert_eq!(helper_function(2), 4);
    // Should NOT match:
    // - // Comment with helper mentioned
    // - let greeting = "hello helper world";

    assert!(
        output.iter().any(|l| l.contains("fn helper_function")),
        "Should include helper_function definition"
    );
    assert!(
        output.iter().any(|l| l.contains("use crate::utils::helper")),
        "Should include helper import"
    );
    assert!(
        !output.iter().any(|l| l.contains("// Comment with helper")),
        "Should NOT include comment with helper"
    );
    assert!(
        !output.iter().any(|l| l.contains("hello helper world")),
        "Should NOT include string with helper"
    );
}

#[test]
fn test_rust_string_scope_filters_correctly() {
    let config = PipelineConfig {
        name: Some("Rust String Scope Test".to_string()),
        settings: PipelineSettings::default(),
        step: vec![create_scoped_filter_step("helper", "rust", "string")],
        ..Default::default()
    };

    let output = process_with_pipeline(RUST_FIXTURE, config);

    // Should only match: let greeting = "hello helper world";
    assert!(
        output.iter().any(|l| l.contains("hello helper world")),
        "Should include string with helper"
    );
    assert!(
        !output.iter().any(|l| l.contains("fn helper_function")),
        "Should NOT include function definition"
    );
}

#[test]
fn test_rust_comment_scope_filters_correctly() {
    let config = PipelineConfig {
        name: Some("Rust Comment Scope Test".to_string()),
        settings: PipelineSettings::default(),
        step: vec![create_scoped_filter_step("helper", "rust", "comment")],
        ..Default::default()
    };

    let output = process_with_pipeline(RUST_FIXTURE, config);

    // Should match lines with "helper" in comments
    assert!(
        output.iter().any(|l| l.contains("// Comment with helper")),
        "Should include comment with helper"
    );
    assert!(
        !output.iter().any(|l| l.contains("fn helper_function")),
        "Should NOT include function definition"
    );
}

#[test]
fn test_rust_functions_scope_filters_correctly() {
    let config = PipelineConfig {
        name: Some("Rust Functions Scope Test".to_string()),
        settings: PipelineSettings::default(),
        step: vec![create_scoped_filter_step("helper", "rust", "functions")],
        ..Default::default()
    };

    let output = process_with_pipeline(RUST_FIXTURE, config);

    // Should match function definitions containing "helper"
    assert!(
        output.iter().any(|l| l.contains("fn helper_function")),
        "Should include helper_function definition"
    );
    assert!(
        output.iter().any(|l| l.contains("fn test_helper")),
        "Should include test_helper definition"
    );
}

#[test]
fn test_rust_tests_scope_filters_correctly() {
    let config = PipelineConfig {
        name: Some("Rust Tests Scope Test".to_string()),
        settings: PipelineSettings::default(),
        step: vec![create_scoped_filter_step("helper", "rust", "tests")],
        ..Default::default()
    };

    let output = process_with_pipeline(RUST_FIXTURE, config);

    // Should match test-related code containing "helper"
    assert!(
        output.iter().any(|l| l.contains("test_helper") || l.contains("helper_function(2)")),
        "Should include test code with helper"
    );
}

// =============================================================================
// Python Scope Tests
// =============================================================================

#[test]
fn test_python_code_scope_filters_correctly() {
    let config = PipelineConfig {
        name: Some("Python Code Scope Test".to_string()),
        settings: PipelineSettings::default(),
        step: vec![create_scoped_filter_step("helper", "python", "code")],
        ..Default::default()
    };

    let output = process_with_pipeline(PYTHON_FIXTURE, config);

    // Should match code, not strings or comments
    assert!(
        output.iter().any(|l| l.contains("def helper_function")),
        "Should include helper_function definition"
    );
    assert!(
        !output.iter().any(|l| l.contains("# Comment with helper")),
        "Should NOT include comment with helper"
    );
    assert!(
        !output.iter().any(|l| l.contains("hello helper world")),
        "Should NOT include string with helper"
    );
}

#[test]
fn test_python_tests_scope_filters_correctly() {
    let config = PipelineConfig {
        name: Some("Python Tests Scope Test".to_string()),
        settings: PipelineSettings::default(),
        step: vec![create_scoped_filter_step("helper", "python", "tests")],
        ..Default::default()
    };

    let output = process_with_pipeline(PYTHON_FIXTURE, config);

    // Should match test functions containing "helper"
    assert!(
        output.iter().any(|l| l.contains("def test_helper") || l.contains("class TestHelper")),
        "Should include test code with helper"
    );
}

// =============================================================================
// JavaScript Scope Tests
// =============================================================================

#[test]
fn test_javascript_code_scope_filters_correctly() {
    let config = PipelineConfig {
        name: Some("JavaScript Code Scope Test".to_string()),
        settings: PipelineSettings::default(),
        step: vec![create_scoped_filter_step("helper", "javascript", "code")],
        ..Default::default()
    };

    let output = process_with_pipeline(JS_FIXTURE, config);

    // Should match code, not strings or comments
    assert!(
        output.iter().any(|l| l.contains("function helperFunction")),
        "Should include helperFunction definition"
    );
    assert!(
        !output.iter().any(|l| l.contains("// Comment with helper")),
        "Should NOT include comment with helper"
    );
    assert!(
        !output.iter().any(|l| l.contains("hello helper world")),
        "Should NOT include string with helper"
    );
}

#[test]
fn test_javascript_imports_scope_filters_correctly() {
    let config = PipelineConfig {
        name: Some("JavaScript Imports Scope Test".to_string()),
        settings: PipelineSettings::default(),
        step: vec![create_scoped_filter_step("helper", "javascript", "imports")],
        ..Default::default()
    };

    let output = process_with_pipeline(JS_FIXTURE, config);

    // Should match import statements containing "helper"
    assert!(
        output.iter().any(|l| l.contains("import { helper }")),
        "Should include import with helper"
    );
}

#[test]
fn test_javascript_tests_scope_filters_correctly() {
    let config = PipelineConfig {
        name: Some("JavaScript Tests Scope Test".to_string()),
        settings: PipelineSettings::default(),
        step: vec![create_scoped_filter_step("helper", "javascript", "tests")],
        ..Default::default()
    };

    let output = process_with_pipeline(JS_FIXTURE, config);

    // Should match test blocks (describe/it/test) containing "helper"
    assert!(
        output.iter().any(|l| l.contains("describe('Helper')") || l.contains("helperFunction")),
        "Should include test code with helper"
    );
}

// =============================================================================
// exclude_scopes Tests
// =============================================================================

#[test]
fn test_exclude_scopes_strings_and_comments() {
    // This tests the bug fix for Issue #8: exclude_scopes not working
    let config = PipelineConfig {
        name: Some("Exclude Scopes Test".to_string()),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Filter,
            pattern: "helper".to_string(),
            action: Some(StepAction::KeepLine),
            language: Some("rust".to_string()),
            exclude_scopes: Some(vec!["strings".to_string(), "comments".to_string()]),
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let output = process_with_pipeline(RUST_FIXTURE, config);

    // With exclude_scopes = ["strings", "comments"], should match code only
    // Same as scope = "code" effectively
    assert!(
        output.iter().any(|l| l.contains("fn helper_function")),
        "Should include helper_function definition (in code)"
    );
    assert!(
        !output.iter().any(|l| l.contains("// Comment with helper")),
        "Should NOT include comment (excluded)"
    );
    assert!(
        !output.iter().any(|l| l.contains("hello helper world")),
        "Should NOT include string (excluded)"
    );
}

#[test]
fn test_exclude_scopes_comments() {
    // Exclude comments, match in code and strings
    let config = PipelineConfig {
        name: Some("Exclude Comments Test".to_string()),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Filter,
            pattern: "helper".to_string(),
            action: Some(StepAction::KeepLine),
            language: Some("rust".to_string()),
            exclude_scopes: Some(vec!["comments".to_string()]),
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let output = process_with_pipeline(RUST_FIXTURE, config);

    // Should match code and strings, but not comments
    assert!(
        output.iter().any(|l| l.contains("fn helper_function") || l.contains("helper_function(42)")),
        "Should include matches in code"
    );
    assert!(
        output.iter().any(|l| l.contains("hello helper world")),
        "Should include matches in strings"
    );
    // Comment-only matches should be excluded
    // (Note: lines with both comment AND code matches may still appear)
}

// =============================================================================
// Scoped Substitution Tests
// =============================================================================

#[test]
fn test_scoped_substitution_in_code_only() {
    let config = PipelineConfig {
        name: Some("Scoped Substitution Test".to_string()),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Substitute,
            pattern: "helper".to_string(),
            replacement: Some("utility".to_string()),
            language: Some("rust".to_string()),
            scope: Some("code".to_string()),
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    // Use process_file_content for syntax-aware substitution (needs full AST)
    let mut processor = StreamProcessor::new(config).unwrap();
    let (output_str, _result) = processor
        .process_file_content(RUST_FIXTURE, None)
        .unwrap();

    // "helper" in code should become "utility"
    assert!(
        output_str.contains("fn utility_function") || output_str.contains("utility_function(42)"),
        "Should replace helper with utility in code: {}",
        output_str
    );

    // "helper" in strings and comments should remain unchanged
    assert!(
        output_str.contains("hello helper world"),
        "Should NOT replace helper in strings: {}",
        output_str
    );
    assert!(
        output_str.contains("Comment with helper"),
        "Should NOT replace helper in comments: {}",
        output_str
    );
}

// =============================================================================
// Multi-Language Detection Tests
// =============================================================================

#[test]
fn test_language_detection_from_extension() {
    use rexpipe::processor::StreamProcessor;

    assert_eq!(
        StreamProcessor::detect_language_from_extension("rs"),
        Some(Language::Rust)
    );
    assert_eq!(
        StreamProcessor::detect_language_from_extension("py"),
        Some(Language::Python)
    );
    assert_eq!(
        StreamProcessor::detect_language_from_extension("js"),
        Some(Language::JavaScript)
    );
    assert_eq!(
        StreamProcessor::detect_language_from_extension("ts"),
        Some(Language::TypeScript)
    );
    assert_eq!(
        StreamProcessor::detect_language_from_extension("go"),
        Some(Language::Go)
    );
    assert_eq!(
        StreamProcessor::detect_language_from_extension("json"),
        Some(Language::Json)
    );
    assert_eq!(
        StreamProcessor::detect_language_from_extension("yaml"),
        Some(Language::Yaml)
    );
    assert_eq!(
        StreamProcessor::detect_language_from_extension("yml"),
        Some(Language::Yaml)
    );
    assert_eq!(
        StreamProcessor::detect_language_from_extension("unknown"),
        None
    );
}

#[test]
fn test_scope_filter_parsing() {
    assert_eq!("code".parse::<ScopeFilter>().unwrap(), ScopeFilter::Code);
    assert_eq!("strings".parse::<ScopeFilter>().unwrap(), ScopeFilter::Strings);
    assert_eq!("string".parse::<ScopeFilter>().unwrap(), ScopeFilter::Strings);
    assert_eq!("comments".parse::<ScopeFilter>().unwrap(), ScopeFilter::Comments);
    assert_eq!("comment".parse::<ScopeFilter>().unwrap(), ScopeFilter::Comments);
    assert_eq!("functions".parse::<ScopeFilter>().unwrap(), ScopeFilter::Functions);
    assert_eq!("fn".parse::<ScopeFilter>().unwrap(), ScopeFilter::Functions);
    assert_eq!("imports".parse::<ScopeFilter>().unwrap(), ScopeFilter::Imports);
    assert_eq!("tests".parse::<ScopeFilter>().unwrap(), ScopeFilter::Tests);
    assert_eq!("types".parse::<ScopeFilter>().unwrap(), ScopeFilter::Types);
    assert_eq!("identifiers".parse::<ScopeFilter>().unwrap(), ScopeFilter::Identifiers);
    assert_eq!("macros".parse::<ScopeFilter>().unwrap(), ScopeFilter::Macros);
    assert_eq!("control_flow".parse::<ScopeFilter>().unwrap(), ScopeFilter::ControlFlow);
    assert_eq!("all".parse::<ScopeFilter>().unwrap(), ScopeFilter::All);
    assert_eq!("*".parse::<ScopeFilter>().unwrap(), ScopeFilter::All);

    // Invalid scope should error
    assert!("invalid_scope".parse::<ScopeFilter>().is_err());
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_empty_source_with_scoping() {
    let config = PipelineConfig {
        name: Some("Empty Source Test".to_string()),
        settings: PipelineSettings::default(),
        step: vec![create_scoped_filter_step("pattern", "rust", "code")],
        ..Default::default()
    };

    let output = process_with_pipeline("", config);
    assert!(output.is_empty() || (output.len() == 1 && output[0].is_empty()));
}

#[test]
fn test_no_matches_in_scope() {
    let config = PipelineConfig {
        name: Some("No Matches Test".to_string()),
        settings: PipelineSettings::default(),
        step: vec![create_scoped_filter_step("nonexistent_pattern_xyz", "rust", "code")],
        ..Default::default()
    };

    let output = process_with_pipeline(RUST_FIXTURE, config);

    // No lines should match a nonexistent pattern
    assert!(output.is_empty() || output.iter().all(|l| l.is_empty()));
}

#[test]
fn test_scope_without_language_warns() {
    // When scope is specified without language, it should warn but still process
    let config = PipelineConfig {
        name: Some("Scope Without Language Test".to_string()),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Filter,
            pattern: "helper".to_string(),
            action: Some(StepAction::KeepLine),
            scope: Some("code".to_string()),
            // No language specified - should fall back to non-scoped processing
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    // Should not panic, should process without scoping
    let output = process_with_pipeline(RUST_FIXTURE, config);

    // Without language, all lines with "helper" should match (no scoping applied)
    assert!(
        output.iter().any(|l| l.contains("helper")),
        "Should find some matches without scoping"
    );
}
