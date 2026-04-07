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
//! - **Multiple Regex Engines**: Standard Rust regex (fast, ReDoS-safe) with automatic
//!   PCRE fallback via `fancy-regex` for advanced patterns (lookahead/lookbehind)
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
//! ## Feature Flags
//!
//! rexpipe supports two top-level feature sets that partition the library into
//! a WASM-safe core and a full-featured CLI build:
//!
//! - **`core`** (WASM-compatible): Pipeline processing, pattern compilation,
//!   regex engines, step execution. Suitable for `wasm32-unknown-unknown`.
//! - **`cli`** (default): Everything in `core` plus filesystem traversal,
//!   terminal inspection, progress bars, shell plugins, pattern libraries
//!   loaded from disk, bidirectional mapping persistence, checkpointing,
//!   cross-file rules, and the `rexpipe` binary.
//!
//! Consumers targeting WASM should depend on rexpipe with
//! `default-features = false, features = ["core"]`.
//!
//! ## Modules
//!
//! **Always available (in `core`):**
//! - [`pipeline`]: Configuration structures for pipeline definitions
//! - [`processor`]: Core streaming text processing engine
//! - [`library`]: Pattern library data types and resolution (file loading is `cli`-gated)
//! - [`plugin`]: Built-in transforms (shell execution is `cli`-gated)
//! - [`bidirectional`]: Reversible pipeline data types (file I/O is `cli`-gated)
//! - [`checkpoint`]: Checkpoint config data type (runtime is `cli`-gated)
//! - [`crossfile`]: Cross-file rule data types (runtime is `cli`-gated)
//! - [`testing`]: First-class pipeline testing support
//! - [`error`]: Error types
//! - [`json_schema`]: JSON schema generation for configs
//!
//! **Requires `cli` feature:**
//! - `files`: Multi-file processing with directory recursion
//! - `inspector`: Interactive debugging and pattern inspection
//! - `learn`: Pattern learning and inference from examples
//!
//! **Requires `tree-sitter` feature:**
//! - `syntax`: Syntax-aware pattern matching

// === Always-available modules (WASM-safe core) ===
pub mod bidirectional;
pub mod checkpoint;
pub mod crossfile;
pub mod error;
pub mod json_schema;
pub mod library;
pub mod pipeline;
pub mod plugin;
pub mod processor;
pub mod testing;

// === CLI-only modules (filesystem / terminal / parallelism) ===
#[cfg(feature = "cli")]
pub mod files;
#[cfg(feature = "cli")]
pub mod inspector;
#[cfg(feature = "cli")]
pub mod learn;

#[cfg(feature = "tree-sitter")]
pub mod syntax;

// === Always-available re-exports ===
pub use error::{ConfigError, LibraryError, PatternError, RexpipeError, ValidationError};

// Bidirectional: data types + (possibly stubbed) manager are always available
pub use bidirectional::{BidirectionalConfig, Direction, MappingStore};

// Testing: always in core (uses web-time, no filesystem)
pub use testing::{TestCase, TestConfig, TestRunner, TestSummary};

// Finalize: pure data types and processor-internal state
pub use pipeline::{CounterConfig, FinalizeConfig, FinalizeOutputFormat};
pub use processor::{CompiledCounter, FinalizeState};

// Checkpoint: only the Config data type is in core; runtime types are cli-gated
pub use checkpoint::CheckpointConfig;

// Cross-file: config and rule data types are in core; the manager is cli-gated
pub use crossfile::{CrossFileConfig, CrossFileRule};

// === CLI-only re-exports ===
#[cfg(feature = "cli")]
pub use files::{BinaryMode, ShutdownInterrupted, ShutdownSignal, is_binary_file};

#[cfg(feature = "cli")]
pub use checkpoint::{Checkpoint, GitDiff};

#[cfg(feature = "cli")]
pub use crossfile::CrossFileManager;

#[cfg(feature = "cli")]
pub use learn::{LearnConfig, LearnedPattern, PatternLearner};
