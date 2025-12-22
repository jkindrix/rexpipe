//! # rexpipe
//!
//! A unified regex pipeline processor that consolidates complex text processing workflows
//! into a single, efficient tool.
//!
//! ## Key Features
//!
//! - **Pipeline Processing**: Chain multiple regex operations (substitute, filter, extract,
//!   validate, transform) in a single pass
//! - **Pattern Libraries**: Reusable regex pattern definitions in TOML format
//! - **Multi-file Processing**: Recursive directory traversal with parallel processing
//! - **VCS Awareness**: Respects `.gitignore` files by default
//! - **Multiple Regex Engines**: Standard Rust regex (fast, ReDoS-safe) and optional PCRE
//!   via `fancy-regex` for advanced patterns
//!
//! ## Quick Start
//!
//! ```
//! use rexpipe::pipeline::PipelineConfig;
//! use rexpipe::processor::StreamProcessor;
//! use std::io::Cursor;
//!
//! // Create a simple substitution pipeline
//! let config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUM"));
//! let mut processor = StreamProcessor::new(config).unwrap();
//!
//! let input = Cursor::new("There are 42 apples and 17 oranges.\n");
//! let mut output = Vec::new();
//! let result = processor.process_stream(input, &mut output).unwrap();
//!
//! assert_eq!(String::from_utf8(output).unwrap(), "There are NUM apples and NUM oranges.\n");
//! ```
//!
//! ## Pipeline Configuration
//!
//! Pipelines can be defined in TOML format for complex multi-step processing:
//!
//! ```
//! use rexpipe::pipeline::PipelineConfig;
//!
//! let toml = r#"
//! name = "Log Processor"
//!
//! [[step]]
//! type = "substitute"
//! pattern = '\[ERROR\]'
//! replacement = "[ERR]"
//!
//! [[step]]
//! type = "filter"
//! pattern = 'DEBUG'
//! action = "drop_line"
//! "#;
//!
//! let config: PipelineConfig = toml::from_str(toml).unwrap();
//! assert_eq!(config.step.len(), 2);
//! ```
//!
//! ## Match Inspection
//!
//! Use the inspector module to analyze pattern matches:
//!
//! ```
//! use rexpipe::pipeline::PipelineConfig;
//! use rexpipe::inspector::Inspector;
//!
//! let config = PipelineConfig::from_inline_pattern(r"(\w+)@(\w+\.com)", None);
//! let inspector = Inspector::new(config).unwrap();
//!
//! let matches = inspector.inspect_single_line("Contact: user@example.com").unwrap();
//! assert_eq!(matches.len(), 1);
//! assert_eq!(matches[0].captures[1], Some("user".to_string()));
//! ```
//!
//! ## Modules
//!
//! - [`pipeline`]: Configuration structures for pipeline definitions
//! - [`processor`]: Core streaming text processing engine
//! - [`files`]: Multi-file processing with directory recursion
//! - [`library`]: Pattern library loading and resolution
//! - [`inspector`]: Interactive debugging and pattern inspection
//! - [`plugin`]: Extensible plugin system for custom transformations
//! - [`stream`]: URI-based streaming source and sink abstractions
//! - [`bidirectional`]: Reversible pipeline transformations
//! - [`checkpoint`]: Incremental processing with checkpoint/resume
//! - [`crossfile`]: Cross-file semantic relationship processing
//! - [`learn`]: Pattern learning and inference from examples
//! - [`testing`]: First-class pipeline testing support
//! - [`data`]: Structured data processing (JSON, CSV, YAML, XML, TOML)
//! - `syntax` (requires `tree-sitter` feature): Syntax-aware pattern matching

// Core modules
pub mod error;
pub mod files;
pub mod inspector;
pub mod json_schema;
pub mod library;
pub mod pipeline;
pub mod plugin;
pub mod processor;
pub mod stream;

// Advanced feature modules
pub mod bidirectional;
pub mod checkpoint;
pub mod crossfile;
pub mod data;
pub mod learn;
pub mod testing;

#[cfg(feature = "tree-sitter")]
pub mod syntax;

// Re-export error types for convenience
pub use error::{ConfigError, LibraryError, PatternError, RexpipeError, ValidationError};

// Re-export shutdown signal types for graceful termination
pub use files::{BinaryMode, ShutdownInterrupted, ShutdownSignal, is_binary_file};

// Re-export stream types for convenience
pub use stream::{StreamUri, StreamSource, StreamSink, create_source, create_sink};

// Re-export bidirectional types for reversible pipelines
pub use bidirectional::{BidirectionalConfig, Direction, MappingStore};

// Re-export checkpoint types for incremental processing
pub use checkpoint::{Checkpoint, CheckpointConfig, GitDiff};

// Re-export cross-file types for semantic relationships
pub use crossfile::{CrossFileConfig, CrossFileRule, CrossFileManager};

// Re-export learning types for pattern inference
pub use learn::{PatternLearner, LearnedPattern, LearnConfig};

// Re-export testing types for pipeline validation
pub use testing::{TestRunner, TestCase, TestConfig, TestSummary};

// Re-export data types for structured data processing
pub use data::{DataFormat, DataValue, DataError, DataReader, DataWriter};
