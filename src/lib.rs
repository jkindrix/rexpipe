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
//! ```no_run
//! use rexpipe::pipeline::{PipelineConfig, PipelineSettings};
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
//! ## Modules
//!
//! - [`pipeline`]: Configuration structures for pipeline definitions
//! - [`processor`]: Core streaming text processing engine
//! - [`files`]: Multi-file processing with directory recursion
//! - [`library`]: Pattern library loading and resolution
//! - [`inspector`]: Interactive debugging and pattern inspection
//! - [`compass`]: COMPASS strategic framework for analysis
//! - [`plugin`]: Extensible plugin system for custom transformations

pub mod compass;
pub mod error;
pub mod files;
pub mod inspector;
pub mod json_schema;
pub mod library;
pub mod pipeline;
pub mod plugin;
pub mod processor;

// Re-export error types for convenience
pub use error::{ConfigError, LibraryError, PatternError, RexpipeError, ValidationError};
