//! Integration tests for the pattern library feature

use rexpipe::library::{LibraryResolver, PatternLibrary};
use rexpipe::pipeline::PipelineConfig;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a temporary directory with test files
fn create_test_library(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, content).unwrap();
    path
}

#[test]
fn test_library_loading_from_file() {
    let dir = TempDir::new().unwrap();
    let lib_path = create_test_library(
        &dir,
        "test.toml",
        r#"
name = "Test Library"
version = "1.0.0"

[patterns]
simple = 'hello'

[patterns.nested]
pattern = 'world'
"#,
    );

    let library: PatternLibrary = toml::from_str(&fs::read_to_string(&lib_path).unwrap()).unwrap();

    assert_eq!(library.name, Some("Test Library".to_string()));
    assert_eq!(library.version, Some("1.0.0".to_string()));
}

#[test]
fn test_library_resolver_relative_path() {
    let dir = TempDir::new().unwrap();
    create_test_library(
        &dir,
        "patterns/test.toml",
        r#"
name = "Test"
[patterns]
test = 'pattern'
"#,
    );

    let mut resolver = LibraryResolver::new(Some(dir.path()));
    let result = resolver.load_libraries(&["patterns/test.toml".to_string()]);

    assert!(result.is_ok());
    let library = result.unwrap();
    assert_eq!(library.get("test"), Some(&"pattern".to_string()));
}

#[test]
fn test_pattern_flattening() {
    let dir = TempDir::new().unwrap();
    create_test_library(
        &dir,
        "nested.toml",
        r#"
name = "Nested Test"

[patterns.category]
item1 = 'pattern1'
item2 = 'pattern2'

[patterns.deep.nested]
item = 'deep_pattern'
"#,
    );

    let mut resolver = LibraryResolver::new(Some(dir.path()));
    let library = resolver
        .load_libraries(&["nested.toml".to_string()])
        .unwrap();

    assert_eq!(library.get("category.item1"), Some(&"pattern1".to_string()));
    assert_eq!(library.get("category.item2"), Some(&"pattern2".to_string()));
    assert_eq!(
        library.get("deep.nested.item"),
        Some(&"deep_pattern".to_string())
    );
}

#[test]
fn test_nested_library_includes() {
    let dir = TempDir::new().unwrap();

    // Create base library
    create_test_library(
        &dir,
        "base.toml",
        r#"
name = "Base"
[patterns]
base_pattern = 'from_base'
"#,
    );

    // Create extended library that includes base
    create_test_library(
        &dir,
        "extended.toml",
        r#"
name = "Extended"
patterns_include = ["base.toml"]

[patterns]
extended_pattern = 'from_extended'
"#,
    );

    let mut resolver = LibraryResolver::new(Some(dir.path()));
    let library = resolver
        .load_libraries(&["extended.toml".to_string()])
        .unwrap();

    // Both patterns should be available
    assert_eq!(library.get("base_pattern"), Some(&"from_base".to_string()));
    assert_eq!(
        library.get("extended_pattern"),
        Some(&"from_extended".to_string())
    );
}

#[test]
fn test_circular_reference_detection() {
    let dir = TempDir::new().unwrap();

    // Create library A that includes B
    create_test_library(
        &dir,
        "a.toml",
        r#"
name = "A"
patterns_include = ["b.toml"]
[patterns]
a = 'pattern_a'
"#,
    );

    // Create library B that includes A (circular!)
    create_test_library(
        &dir,
        "b.toml",
        r#"
name = "B"
patterns_include = ["a.toml"]
[patterns]
b = 'pattern_b'
"#,
    );

    let mut resolver = LibraryResolver::new(Some(dir.path()));
    let result = resolver.load_libraries(&["a.toml".to_string()]);

    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
    assert!(
        error.contains("Circular") || error.contains("circular"),
        "Expected circular reference error, got: {}",
        error
    );
}

#[test]
fn test_library_not_found_error() {
    let dir = TempDir::new().unwrap();
    let mut resolver = LibraryResolver::new(Some(dir.path()));
    let result = resolver.load_libraries(&["nonexistent.toml".to_string()]);

    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
    assert!(
        error.contains("not found"),
        "Expected 'not found' error, got: {}",
        error
    );
}

#[test]
fn test_pipeline_pattern_resolution() {
    let dir = TempDir::new().unwrap();

    create_test_library(
        &dir,
        "patterns.toml",
        r#"
name = "Pipeline Patterns"
[patterns.regex]
digits = '\d+'
word = '\w+'
"#,
    );

    let pipeline_content = r#"
name = "Test Pipeline"
patterns_include = ["patterns.toml"]

[[step]]
type = "substitute"
pattern = '${regex.digits}'
replacement = 'NUMBER'
"#;

    let pipeline_path = create_test_library(&dir, "pipeline.toml", pipeline_content);

    let mut config: PipelineConfig =
        toml::from_str(&fs::read_to_string(&pipeline_path).unwrap()).unwrap();

    // Load and resolve patterns
    let mut resolver = LibraryResolver::new(Some(dir.path()));
    let library = resolver.load_libraries(&config.patterns_include).unwrap();

    config.resolve_pattern_references(&library).unwrap();

    // Pattern should be resolved
    assert_eq!(config.step[0].pattern, r"\d+");
}

#[test]
fn test_missing_pattern_reference_error() {
    let dir = TempDir::new().unwrap();

    create_test_library(
        &dir,
        "patterns.toml",
        r#"
name = "Patterns"
[patterns]
exists = 'pattern'
"#,
    );

    let mut config = PipelineConfig {
        name: Some("Test".to_string()),
        description: None,
        version: None,
        patterns_include: vec!["patterns.toml".to_string()],
        settings: Default::default(),
        step: vec![rexpipe::pipeline::PipelineStep {
            step_type: rexpipe::pipeline::StepType::Substitute,
            pattern: "${nonexistent.pattern}".to_string(),
            replacement: Some("x".to_string()),
            action: None,
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut resolver = LibraryResolver::new(Some(dir.path()));
    let library = resolver.load_libraries(&config.patterns_include).unwrap();

    let result = config.resolve_pattern_references(&library);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors[0].contains("nonexistent.pattern"));
}

#[test]
fn test_multiple_pattern_references_in_single_step() {
    let dir = TempDir::new().unwrap();

    create_test_library(
        &dir,
        "patterns.toml",
        r#"
name = "Multi Patterns"
[patterns]
prefix = '^START'
suffix = 'END$'
"#,
    );

    let mut config = PipelineConfig {
        name: Some("Test".to_string()),
        description: None,
        version: None,
        patterns_include: vec!["patterns.toml".to_string()],
        settings: Default::default(),
        step: vec![rexpipe::pipeline::PipelineStep {
            step_type: rexpipe::pipeline::StepType::Filter,
            pattern: "${prefix}.*${suffix}".to_string(),
            replacement: None,
            action: Some(rexpipe::pipeline::StepAction::KeepLine),
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut resolver = LibraryResolver::new(Some(dir.path()));
    let library = resolver.load_libraries(&config.patterns_include).unwrap();

    config.resolve_pattern_references(&library).unwrap();

    assert_eq!(config.step[0].pattern, "^START.*END$");
}

#[test]
fn test_toml_extension_auto_added() {
    let dir = TempDir::new().unwrap();

    create_test_library(
        &dir,
        "mylib.toml",
        r#"
name = "Auto Extension"
[patterns]
test = 'pattern'
"#,
    );

    let mut resolver = LibraryResolver::new(Some(dir.path()));
    // Reference without .toml extension
    let result = resolver.load_libraries(&["mylib".to_string()]);

    assert!(result.is_ok());
    assert_eq!(result.unwrap().get("test"), Some(&"pattern".to_string()));
}

#[test]
fn test_empty_library() {
    let dir = TempDir::new().unwrap();
    create_test_library(
        &dir,
        "empty.toml",
        r#"
name = "Empty Library"
version = "1.0.0"
# No patterns defined
"#,
    );

    let mut resolver = LibraryResolver::new(Some(dir.path()));
    let result = resolver.load_libraries(&["empty.toml".to_string()]);

    assert!(result.is_ok());
    let library = result.unwrap();
    assert!(library.patterns.is_empty(), "Empty library should have no patterns");
}

#[test]
fn test_pattern_conflict_keeps_first_definition() {
    let dir = TempDir::new().unwrap();

    // First library defines a pattern
    create_test_library(
        &dir,
        "first.toml",
        r#"
name = "First"
[patterns]
shared = 'first_value'
"#,
    );

    // Second library defines the same pattern with different value
    create_test_library(
        &dir,
        "second.toml",
        r#"
name = "Second"
[patterns]
shared = 'second_value'
"#,
    );

    let mut resolver = LibraryResolver::new(Some(dir.path()));
    // Load both libraries
    let result = resolver.load_libraries(&[
        "first.toml".to_string(),
        "second.toml".to_string(),
    ]);

    assert!(result.is_ok());
    let library = result.unwrap();
    // When a conflict exists, the library keeps the FIRST definition
    // (and logs a warning about the duplicate)
    assert_eq!(library.get("shared"), Some(&"first_value".to_string()));
}

#[test]
fn test_invalid_toml_syntax_error() {
    let dir = TempDir::new().unwrap();
    create_test_library(
        &dir,
        "invalid.toml",
        r#"
name = "Invalid"
[patterns
broken syntax here
"#,
    );

    let mut resolver = LibraryResolver::new(Some(dir.path()));
    let result = resolver.load_libraries(&["invalid.toml".to_string()]);

    assert!(result.is_err(), "Should fail with invalid TOML syntax");
}

#[test]
fn test_deeply_nested_patterns() {
    let dir = TempDir::new().unwrap();
    create_test_library(
        &dir,
        "deep.toml",
        r#"
name = "Deep Nesting"

[patterns.level1.level2.level3.level4]
pattern = 'deeply_nested'
"#,
    );

    let mut resolver = LibraryResolver::new(Some(dir.path()));
    let library = resolver
        .load_libraries(&["deep.toml".to_string()])
        .unwrap();

    assert_eq!(
        library.get("level1.level2.level3.level4.pattern"),
        Some(&"deeply_nested".to_string())
    );
}

#[test]
fn test_unicode_pattern_names_and_values() {
    let dir = TempDir::new().unwrap();
    create_test_library(
        &dir,
        "unicode.toml",
        r#"
name = "Unicode Test"

[patterns]
# Pattern with Unicode in the value
emoji_pattern = '[\x{1F600}-\x{1F64F}]'
chinese = '[\x{4E00}-\x{9FFF}]+'
"#,
    );

    let mut resolver = LibraryResolver::new(Some(dir.path()));
    let library = resolver
        .load_libraries(&["unicode.toml".to_string()])
        .unwrap();

    assert!(library.get("emoji_pattern").is_some());
    assert!(library.get("chinese").is_some());
}
