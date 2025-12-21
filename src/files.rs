//! Multi-file processing module for rexpipe
//!
//! Provides directory recursion, in-place editing, parallel processing,
//! progress indicators, dry-run preview, and VCS-aware file discovery.
//!
//! ## Parallel Processing
//!
//! When `parallel` is enabled, files are processed using Rayon's parallel iterators.
//! To avoid overhead on small file sets, parallel processing is only used when the
//! file count exceeds [`PARALLEL_THRESHOLD`].
//!
//! ## Permission Error Handling
//!
//! During directory recursion, permission errors are handled gracefully:
//!
//! - **Inaccessible files/directories are skipped** rather than stopping processing
//! - **Errors are logged** at debug level for troubleshooting
//! - **Errors are collected** in [`MultiFileResult::walk_errors`] for reporting
//! - **Processing continues** with all accessible files
//!
//! This behavior matches tools like ripgrep and find, which continue processing
//! when encountering permission denied errors. Use `discover_files_with_errors()`
//! to get detailed error information.
//!
//! ## Symlink Handling
//!
//! By default, symlink directories are **not followed** during recursion for security:
//!
//! - Prevents path traversal attacks via malicious symlinks
//! - Avoids infinite loops from circular symlink chains
//! - Matches the default behavior of tools like ripgrep
//!
//! However, **symlink files ARE discovered and processed** - they are valid file entries
//! that can be read. This means:
//!
//! - `link.txt -> real.txt` - Both files are discovered during directory walk
//! - `link_dir/ -> real_dir/` - The symlinked directory is NOT followed
//!
//! Broken symlinks (pointing to non-existent targets) are skipped gracefully.
//!
//! # Example
//!
//! ```no_run
//! use rexpipe::pipeline::PipelineConfig;
//! use rexpipe::files::{FileProcessingOptions, MultiFileProcessor};
//! use std::path::PathBuf;
//!
//! // Create a pipeline to replace TODO with DONE
//! let config = PipelineConfig::from_inline_pattern(r"TODO", Some("DONE"));
//!
//! // Set up options for processing
//! let options = FileProcessingOptions::new()
//!     .include_pattern("*.txt".to_string())
//!     .parallel(true);
//!
//! // Create processor and discover files
//! let processor = MultiFileProcessor::new(config, options);
//! let files = processor.discover_files(&[PathBuf::from(".")]).unwrap();
//!
//! // Process the files
//! let result = processor.process_files(&files).unwrap();
//! println!("Processed {} files, {} matches", result.files_processed, result.total_matches);
//! ```

use crate::pipeline::PipelineConfig;
use crate::processor::StreamProcessor;
use anyhow::Result;
use diffy::{PatchFormatter, create_patch};
use ignore::WalkBuilder;
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, info, trace};
use rayon::prelude::*;
use std::fs::{self, File};
use std::io::{BufReader, IsTerminal};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Minimum number of files required before parallel processing is used.
///
/// For small file counts, the overhead of parallel processing (thread pool,
/// synchronization) can exceed the benefit. This threshold ensures parallel
/// processing is only used when it's likely to provide a performance gain.
pub const PARALLEL_THRESHOLD: usize = 4;

/// Number of bytes to read when checking if a file is binary.
/// Checking the first 8KB is usually sufficient to detect binary files.
const BINARY_DETECTION_BYTES: usize = 8 * 1024;

/// Binary file handling mode.
///
/// Controls how rexpipe handles files that appear to be binary (contain NUL bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BinaryMode {
    /// Automatically skip binary files (default behavior).
    /// Files are detected as binary if they contain a NUL byte in the first 8KB.
    #[default]
    Auto,
    /// Process all files as text, even if they contain binary data.
    /// Use this when you know files are text but contain embedded NUL bytes.
    Text,
    /// Always skip binary files without processing.
    Skip,
}

impl BinaryMode {
    /// Parse a binary mode from a string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "auto" => Some(BinaryMode::Auto),
            "text" => Some(BinaryMode::Text),
            "skip" => Some(BinaryMode::Skip),
            _ => None,
        }
    }
}

/// Check if a file appears to be binary by looking for NUL bytes.
///
/// This uses the same heuristic as ripgrep: a file is considered binary
/// if it contains a NUL byte in the first few kilobytes.
pub fn is_binary_file(path: &Path) -> std::io::Result<bool> {
    use std::io::Read;

    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut buffer = [0u8; BINARY_DETECTION_BYTES];

    let bytes_read = reader.read(&mut buffer)?;

    // Check for NUL bytes in the buffer
    Ok(buffer[..bytes_read].contains(&0))
}

/// Format a directory walk error into a human-readable message.
///
/// This function translates `ignore::Error` into actionable messages
/// that help users understand what went wrong and how to fix it.
fn format_walk_error(err: &ignore::Error) -> String {
    use std::io::ErrorKind;

    // The ignore crate wraps various error types
    match err {
        ignore::Error::Io(io_err) => {
            let kind = io_err.kind();
            match kind {
                ErrorKind::PermissionDenied => {
                    format!(
                        "Permission denied: {} (run with elevated privileges or exclude this path)",
                        io_err
                    )
                }
                ErrorKind::NotFound => {
                    format!("Path not found: {}", io_err)
                }
                _ => {
                    format!("I/O error: {} ({})", io_err, kind)
                }
            }
        }
        ignore::Error::WithPath { path, err: inner } => {
            // Recursively format the inner error with path context
            let inner_msg = format_walk_error(inner);
            format!("{}: {}", path.display(), inner_msg)
        }
        ignore::Error::WithDepth { depth, err: inner } => {
            let inner_msg = format_walk_error(inner);
            format!("At depth {}: {}", depth, inner_msg)
        }
        ignore::Error::WithLineNumber { line, err: inner } => {
            let inner_msg = format_walk_error(inner);
            format!("Line {}: {}", line, inner_msg)
        }
        ignore::Error::Loop { ancestor, child } => {
            format!(
                "Filesystem loop detected: {} points back to {}",
                child.display(),
                ancestor.display()
            )
        }
        ignore::Error::Glob { glob, err } => {
            format!("Invalid glob pattern '{}': {}", glob.as_deref().unwrap_or("<unknown>"), err)
        }
        ignore::Error::UnrecognizedFileType(file_type) => {
            format!("Unrecognized file type: {}", file_type)
        }
        ignore::Error::InvalidDefinition => {
            "Invalid file type definition".to_string()
        }
        ignore::Error::Partial(errors) => {
            let msgs: Vec<String> = errors.iter().map(format_walk_error).collect();
            format!("Multiple errors: {}", msgs.join("; "))
        }
    }
}

// ============================================================================
// Graceful Shutdown Support
// ============================================================================

/// Signal for graceful shutdown coordination.
///
/// This type allows coordinating shutdown between the signal handler and
/// the file processing loops. When a shutdown is requested (via Ctrl+C or
/// SIGTERM), in-progress files complete normally before the processor exits.
///
/// # Example
///
/// ```
/// use rexpipe::files::ShutdownSignal;
///
/// let signal = ShutdownSignal::new();
///
/// // Check if shutdown was requested
/// if signal.is_shutdown_requested() {
///     println!("Shutting down...");
/// }
/// ```
#[derive(Clone)]
pub struct ShutdownSignal {
    flag: Arc<AtomicBool>,
}

impl Default for ShutdownSignal {
    fn default() -> Self {
        Self::new()
    }
}

impl ShutdownSignal {
    /// Create a new shutdown signal.
    pub fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Request a shutdown. Called by signal handlers.
    pub fn request_shutdown(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// Check if shutdown has been requested.
    pub fn is_shutdown_requested(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// Install signal handlers for Ctrl+C (SIGINT) and SIGTERM.
    ///
    /// Returns Ok(()) if handlers were installed successfully, or an error
    /// if signal handling is not supported on this platform.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use rexpipe::files::ShutdownSignal;
    ///
    /// let signal = ShutdownSignal::new();
    /// signal.install_handlers().expect("Failed to install signal handlers");
    ///
    /// // Later, check if shutdown was requested
    /// if signal.is_shutdown_requested() {
    ///     println!("Received shutdown signal");
    /// }
    /// ```
    pub fn install_handlers(&self) -> Result<(), ctrlc::Error> {
        let signal = self.clone();
        ctrlc::set_handler(move || {
            // Only print message on first signal
            if !signal.is_shutdown_requested() {
                eprintln!("\nShutdown requested. Completing in-progress files...");
            }
            signal.request_shutdown();
        })
    }
}

/// Result indicating the processor was interrupted by a shutdown signal.
#[derive(Debug, Clone)]
pub struct ShutdownInterrupted {
    /// Number of files that were processed before shutdown
    pub files_completed: u64,
    /// Number of files that were not processed due to shutdown
    pub files_remaining: u64,
}

/// Result of processing a single file
#[derive(Debug, Clone)]
pub struct FileResult {
    pub path: PathBuf,
    pub matches_found: u64,
    pub lines_processed: u64,
    pub modified: bool,
    pub error: Option<String>,
}

/// Aggregated results from processing multiple files
#[derive(Debug, Default)]
pub struct MultiFileResult {
    pub files_processed: u64,
    pub files_matched: u64,
    pub files_modified: u64,
    /// Number of files skipped (e.g., binary files)
    pub files_skipped: u64,
    pub total_matches: u64,
    pub total_lines: u64,
    pub file_results: Vec<FileResult>,
    /// Errors that occurred during file processing
    pub errors: Vec<String>,
    /// Errors that occurred during directory walking (permission denied, etc.)
    ///
    /// These are non-fatal errors that didn't stop processing but indicate
    /// that some files/directories were skipped.
    pub walk_errors: Vec<String>,
    /// Set if processing was interrupted by a shutdown signal
    pub interrupted: Option<ShutdownInterrupted>,
}

/// Callback type for streaming file results as they're processed
pub type StreamingCallback = Box<dyn Fn(&FileResult) + Send + Sync>;

/// Options for multi-file processing
#[derive(Clone)]
pub struct FileProcessingOptions {
    /// Edit files in-place
    pub in_place: bool,
    /// Backup suffix for in-place edits (e.g., ".bak")
    pub backup_suffix: Option<String>,
    /// Respect .gitignore and other VCS ignore files
    pub respect_gitignore: bool,
    /// Include hidden files
    pub include_hidden: bool,
    /// Maximum depth for directory recursion (None = unlimited)
    pub max_depth: Option<usize>,
    /// Glob patterns to include
    pub include_patterns: Vec<String>,
    /// Glob patterns to exclude
    pub exclude_patterns: Vec<String>,
    /// Process files in parallel
    pub parallel: bool,
    /// Only count matches, don't output content
    pub count_only: bool,
    /// Only list files with matches
    pub files_with_matches: bool,
    /// Only list files without matches
    pub files_without_matches: bool,
    /// Quiet mode - no output, only exit code
    pub quiet: bool,
    /// Show progress indicator for multi-file processing
    pub show_progress: bool,
    /// Enable streaming output mode (emit results as files are processed)
    pub streaming_output: bool,
    /// Optional shutdown signal for graceful termination
    pub shutdown_signal: Option<ShutdownSignal>,
    /// Binary file handling mode
    pub binary_mode: BinaryMode,
}

impl std::fmt::Debug for FileProcessingOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileProcessingOptions")
            .field("in_place", &self.in_place)
            .field("backup_suffix", &self.backup_suffix)
            .field("respect_gitignore", &self.respect_gitignore)
            .field("include_hidden", &self.include_hidden)
            .field("max_depth", &self.max_depth)
            .field("include_patterns", &self.include_patterns)
            .field("exclude_patterns", &self.exclude_patterns)
            .field("parallel", &self.parallel)
            .field("count_only", &self.count_only)
            .field("files_with_matches", &self.files_with_matches)
            .field("files_without_matches", &self.files_without_matches)
            .field("quiet", &self.quiet)
            .field("show_progress", &self.show_progress)
            .field("streaming_output", &self.streaming_output)
            .field("shutdown_signal", &self.shutdown_signal.is_some())
            .field("binary_mode", &self.binary_mode)
            .finish()
    }
}

impl Default for FileProcessingOptions {
    fn default() -> Self {
        Self {
            in_place: false,
            backup_suffix: None,
            respect_gitignore: true,
            include_hidden: false,
            max_depth: None,
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            parallel: false,
            count_only: false,
            files_with_matches: false,
            files_without_matches: false,
            quiet: false,
            show_progress: false,
            streaming_output: false,
            shutdown_signal: None,
            binary_mode: BinaryMode::Auto,
        }
    }
}

impl FileProcessingOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn in_place(mut self, in_place: bool) -> Self {
        self.in_place = in_place;
        self
    }

    pub fn backup_suffix(mut self, suffix: Option<String>) -> Self {
        self.backup_suffix = suffix;
        self
    }

    pub fn respect_gitignore(mut self, respect: bool) -> Self {
        self.respect_gitignore = respect;
        self
    }

    pub fn include_hidden(mut self, include: bool) -> Self {
        self.include_hidden = include;
        self
    }

    pub fn max_depth(mut self, depth: Option<usize>) -> Self {
        self.max_depth = depth;
        self
    }

    pub fn include_pattern(mut self, pattern: String) -> Self {
        self.include_patterns.push(pattern);
        self
    }

    pub fn exclude_pattern(mut self, pattern: String) -> Self {
        self.exclude_patterns.push(pattern);
        self
    }

    pub fn parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }

    pub fn count_only(mut self, count: bool) -> Self {
        self.count_only = count;
        self
    }

    pub fn files_with_matches(mut self, files_only: bool) -> Self {
        self.files_with_matches = files_only;
        self
    }

    pub fn files_without_matches(mut self, files_only: bool) -> Self {
        self.files_without_matches = files_only;
        self
    }

    pub fn quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }

    pub fn show_progress(mut self, show: bool) -> Self {
        self.show_progress = show;
        self
    }

    /// Enable streaming output mode (results emitted as files are processed)
    pub fn streaming_output(mut self, streaming: bool) -> Self {
        self.streaming_output = streaming;
        self
    }

    /// Set a shutdown signal for graceful termination.
    ///
    /// When set, the processor will check this signal between files and stop
    /// processing new files if shutdown is requested. In-progress files will
    /// complete normally to avoid leaving files in a partial state.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use rexpipe::files::{FileProcessingOptions, ShutdownSignal};
    ///
    /// let signal = ShutdownSignal::new();
    /// signal.install_handlers().expect("Failed to install signal handlers");
    ///
    /// let options = FileProcessingOptions::new()
    ///     .parallel(true)
    ///     .shutdown_signal(signal);
    /// ```
    pub fn shutdown_signal(mut self, signal: ShutdownSignal) -> Self {
        self.shutdown_signal = Some(signal);
        self
    }

    /// Set binary file handling mode.
    ///
    /// - `BinaryMode::Auto` (default): Skip files detected as binary
    /// - `BinaryMode::Text`: Process all files as text
    /// - `BinaryMode::Skip`: Always skip binary files
    pub fn binary_mode(mut self, mode: BinaryMode) -> Self {
        self.binary_mode = mode;
        self
    }

    /// Check if a file should be skipped based on binary detection settings.
    ///
    /// Returns `true` if the file should be skipped, `false` if it should be processed.
    pub fn should_skip_file(&self, path: &Path) -> bool {
        match self.binary_mode {
            BinaryMode::Text => false, // Never skip in text mode
            BinaryMode::Skip | BinaryMode::Auto => {
                // Check if file is binary
                match is_binary_file(path) {
                    Ok(true) => {
                        trace!("Skipping binary file: {}", path.display());
                        true
                    }
                    Ok(false) => false,
                    Err(e) => {
                        // If we can't read the file, let the actual processing handle the error
                        trace!("Could not check if file is binary: {}: {}", path.display(), e);
                        false
                    }
                }
            }
        }
    }
}

/// Create a progress bar for file processing
/// Returns None if progress should not be shown (quiet mode, non-TTY, etc.)
fn create_progress_bar(file_count: u64, show_progress: bool, quiet: bool) -> Option<ProgressBar> {
    // Don't show progress if quiet mode or progress disabled
    if quiet || !show_progress {
        return None;
    }

    // Only show progress if stderr is a terminal
    if !std::io::stderr().is_terminal() {
        return None;
    }

    let pb = ProgressBar::new(file_count);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} files ({per_sec}, ETA: {eta})")
            .expect("Invalid progress bar template")
            .progress_chars("█▓▒░  ")
    );
    pb.set_message("Processing files...");
    Some(pb)
}

/// Multi-file processor for batch operations
pub struct MultiFileProcessor {
    config: PipelineConfig,
    options: FileProcessingOptions,
}

impl MultiFileProcessor {
    pub fn new(config: PipelineConfig, options: FileProcessingOptions) -> Self {
        Self { config, options }
    }

    /// Discover files matching the criteria starting from the given paths
    pub fn discover_files(&self, paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
        debug!("Discovering files from {} starting paths", paths.len());
        let mut files = Vec::new();

        for path in paths {
            if path.is_file() {
                trace!("Adding file: {}", path.display());
                files.push(path.clone());
            } else if path.is_dir() {
                debug!("Walking directory: {}", path.display());
                let discovered = self.walk_directory(path)?;
                debug!("Found {} files in {}", discovered.len(), path.display());
                files.extend(discovered);
            }
        }

        info!("File discovery complete: {} files found", files.len());
        Ok(files)
    }

    fn walk_directory(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        self.walk_directory_with_errors(dir).map(|(files, _)| files)
    }

    /// Walk directory and return both discovered files and any errors encountered.
    ///
    /// Unlike `walk_directory`, this method returns partial results even when some
    /// files or directories cannot be accessed due to permission errors.
    ///
    /// # Returns
    ///
    /// A tuple of (discovered_files, errors) where:
    /// - `discovered_files`: All accessible files matching the criteria
    /// - `errors`: Human-readable error messages for inaccessible entries
    ///
    /// # Permission Error Behavior
    ///
    /// Permission errors during directory walking are handled gracefully:
    /// - Inaccessible files/directories are skipped
    /// - Errors are logged and collected for reporting
    /// - Processing continues with accessible files
    /// - The operation only fails if the root directory itself is inaccessible
    pub fn walk_directory_with_errors(&self, dir: &Path) -> Result<(Vec<PathBuf>, Vec<String>)> {
        let mut builder = WalkBuilder::new(dir);

        // Security: Do not follow symlinks (default, but explicit for clarity)
        // This prevents path traversal attacks via malicious symlinks
        builder.follow_links(false);

        // Configure gitignore handling
        builder.git_ignore(self.options.respect_gitignore);
        builder.git_global(self.options.respect_gitignore);
        builder.git_exclude(self.options.respect_gitignore);

        // Configure hidden files
        builder.hidden(!self.options.include_hidden);

        // Configure max depth
        if let Some(depth) = self.options.max_depth {
            builder.max_depth(Some(depth));
        }

        let mut files = Vec::new();
        let mut errors = Vec::new();

        for entry_result in builder.build() {
            let entry = match entry_result {
                Ok(e) => e,
                Err(e) => {
                    // Handle permission and other errors gracefully
                    let error_msg = format_walk_error(&e);
                    debug!("Directory walk error: {}", error_msg);
                    errors.push(error_msg);
                    continue;
                }
            };

            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            // Check include patterns
            if !self.options.include_patterns.is_empty() {
                let matches_include = self.options.include_patterns.iter().any(|pattern| {
                    glob::Pattern::new(pattern)
                        .map(|p| p.matches_path(path))
                        .unwrap_or(false)
                });
                if !matches_include {
                    continue;
                }
            }

            // Check exclude patterns
            let matches_exclude = self.options.exclude_patterns.iter().any(|pattern| {
                glob::Pattern::new(pattern)
                    .map(|p| p.matches_path(path))
                    .unwrap_or(false)
            });
            if matches_exclude {
                continue;
            }

            files.push(path.to_path_buf());
        }

        Ok((files, errors))
    }

    /// Discover files with detailed error reporting.
    ///
    /// Like `discover_files`, but returns both discovered files and any permission
    /// or access errors encountered during directory traversal.
    ///
    /// # Returns
    ///
    /// A tuple of (discovered_files, walk_errors) where:
    /// - `discovered_files`: All accessible files matching the criteria
    /// - `walk_errors`: Errors encountered while walking directories
    pub fn discover_files_with_errors(
        &self,
        paths: &[PathBuf],
    ) -> Result<(Vec<PathBuf>, Vec<String>)> {
        debug!("Discovering files from {} starting paths", paths.len());
        let mut files = Vec::new();
        let mut all_errors = Vec::new();

        for path in paths {
            if path.is_file() {
                trace!("Adding file: {}", path.display());
                files.push(path.clone());
            } else if path.is_dir() {
                debug!("Walking directory: {}", path.display());
                let (discovered, errors) = self.walk_directory_with_errors(path)?;
                debug!(
                    "Found {} files in {} ({} errors)",
                    discovered.len(),
                    path.display(),
                    errors.len()
                );
                files.extend(discovered);
                all_errors.extend(errors);
            } else {
                // Path doesn't exist or is a special file
                let msg = format!(
                    "{}: No such file or directory (or insufficient permissions)",
                    path.display()
                );
                debug!("{}", msg);
                all_errors.push(msg);
            }
        }

        info!(
            "File discovery complete: {} files found, {} errors",
            files.len(),
            all_errors.len()
        );
        Ok((files, all_errors))
    }

    /// Process multiple files
    ///
    /// When `options.parallel` is true, files are processed in parallel using Rayon.
    /// However, to avoid overhead on small file sets, parallel processing is only
    /// used when the file count exceeds [`PARALLEL_THRESHOLD`].
    pub fn process_files(&self, files: &[PathBuf]) -> Result<MultiFileResult> {
        // Only use parallel processing if enabled AND file count exceeds threshold
        let use_parallel = self.options.parallel && files.len() >= PARALLEL_THRESHOLD;
        debug!(
            "Processing {} files (parallel: {}, threshold: {})",
            files.len(),
            use_parallel,
            PARALLEL_THRESHOLD
        );
        if use_parallel {
            self.process_files_parallel(files)
        } else {
            self.process_files_sequential(files)
        }
    }

    /// Process multiple files with a streaming callback
    ///
    /// The callback is invoked for each file as it's processed, enabling
    /// real-time output for large file sets. This is useful when processing
    /// many files and you want to see results immediately rather than waiting
    /// for all files to complete.
    ///
    /// # Arguments
    ///
    /// * `files` - List of file paths to process
    /// * `callback` - Function called with each file's result as it completes
    ///
    /// # Example
    ///
    /// ```ignore
    /// processor.process_files_streaming(&files, |result| {
    ///     if result.matches_found > 0 {
    ///         println!("{}: {} matches", result.path.display(), result.matches_found);
    ///     }
    /// })?;
    /// ```
    pub fn process_files_streaming<F>(
        &self,
        files: &[PathBuf],
        callback: F,
    ) -> Result<MultiFileResult>
    where
        F: Fn(&FileResult) + Send + Sync,
    {
        self.process_files_streaming_impl(files, &callback)
    }

    fn process_files_streaming_impl<F>(
        &self,
        files: &[PathBuf],
        callback: &F,
    ) -> Result<MultiFileResult>
    where
        F: Fn(&FileResult) + Send + Sync,
    {
        let mut result = MultiFileResult::default();
        let progress = create_progress_bar(
            files.len() as u64,
            self.options.show_progress,
            self.options.quiet,
        );

        for file in files {
            let file_result = match self.process_single_file(file) {
                Ok(fr) => fr,
                Err(e) => FileResult {
                    path: file.clone(),
                    matches_found: 0,
                    lines_processed: 0,
                    modified: false,
                    error: Some(e.to_string()),
                },
            };

            // Invoke callback immediately for streaming output
            callback(&file_result);

            // Update aggregated result
            result.files_processed += 1;
            result.total_lines += file_result.lines_processed;
            result.total_matches += file_result.matches_found;

            if file_result.matches_found > 0 {
                result.files_matched += 1;
            }
            if file_result.modified {
                result.files_modified += 1;
            }
            if let Some(ref e) = file_result.error {
                result.errors.push(format!("{}: {}", file.display(), e));
            }

            result.file_results.push(file_result);

            if let Some(ref pb) = progress {
                pb.inc(1);
            }
        }

        if let Some(pb) = progress {
            pb.finish_with_message(format!(
                "Processed {} files ({} matches)",
                result.files_processed, result.total_matches
            ));
        }

        Ok(result)
    }

    fn process_files_sequential(&self, files: &[PathBuf]) -> Result<MultiFileResult> {
        let mut result = MultiFileResult::default();
        let progress = create_progress_bar(
            files.len() as u64,
            self.options.show_progress,
            self.options.quiet,
        );

        let total_files = files.len() as u64;
        let mut processed_count = 0u64;

        for file in files {
            // Check for shutdown before starting each file
            if let Some(ref signal) = self.options.shutdown_signal {
                if signal.is_shutdown_requested() {
                    info!(
                        "Shutdown requested after {} files, {} remaining",
                        processed_count,
                        total_files - processed_count
                    );
                    result.interrupted = Some(ShutdownInterrupted {
                        files_completed: processed_count,
                        files_remaining: total_files - processed_count,
                    });
                    break;
                }
            }

            // Skip binary files based on binary_mode setting
            if self.options.should_skip_file(file) {
                debug!("Skipping binary file: {}", file.display());
                result.files_skipped += 1;
                processed_count += 1;
                if let Some(ref pb) = progress {
                    pb.inc(1);
                }
                continue;
            }

            match self.process_single_file(file) {
                Ok(file_result) => {
                    result.files_processed += 1;
                    result.total_lines += file_result.lines_processed;
                    result.total_matches += file_result.matches_found;

                    if file_result.matches_found > 0 {
                        result.files_matched += 1;
                    }
                    if file_result.modified {
                        result.files_modified += 1;
                    }

                    result.file_results.push(file_result);
                }
                Err(e) => {
                    result.errors.push(format!("{}: {}", file.display(), e));
                    result.file_results.push(FileResult {
                        path: file.clone(),
                        matches_found: 0,
                        lines_processed: 0,
                        modified: false,
                        error: Some(e.to_string()),
                    });
                }
            }

            processed_count += 1;

            if let Some(ref pb) = progress {
                pb.inc(1);
            }
        }

        if let Some(pb) = progress {
            let msg = if result.interrupted.is_some() {
                format!(
                    "Interrupted after {} files ({} matches)",
                    result.files_processed, result.total_matches
                )
            } else {
                format!(
                    "Processed {} files ({} matches)",
                    result.files_processed, result.total_matches
                )
            };
            pb.finish_with_message(msg);
        }

        Ok(result)
    }

    fn process_files_parallel(&self, files: &[PathBuf]) -> Result<MultiFileResult> {
        let files_processed = AtomicU64::new(0);
        let files_matched = AtomicU64::new(0);
        let files_modified = AtomicU64::new(0);
        let files_skipped = AtomicU64::new(0);
        let total_matches = AtomicU64::new(0);
        let total_lines = AtomicU64::new(0);
        let shutdown_detected = AtomicBool::new(false);

        let progress = create_progress_bar(
            files.len() as u64,
            self.options.show_progress,
            self.options.quiet,
        );

        let shutdown_signal = &self.options.shutdown_signal;

        let file_results: Vec<Option<FileResult>> = files
            .par_iter()
            .map(|file| {
                // Check for shutdown before starting this file
                // Note: Files already in-progress will continue to completion
                if let Some(signal) = shutdown_signal {
                    if signal.is_shutdown_requested() {
                        shutdown_detected.store(true, Ordering::Relaxed);
                        files_skipped.fetch_add(1, Ordering::Relaxed);
                        if let Some(ref pb) = progress {
                            pb.inc(1);
                        }
                        return None; // Skip this file
                    }
                }

                // Skip binary files based on binary_mode setting
                if self.options.should_skip_file(file) {
                    debug!("Skipping binary file: {}", file.display());
                    files_skipped.fetch_add(1, Ordering::Relaxed);
                    if let Some(ref pb) = progress {
                        pb.inc(1);
                    }
                    return None; // Skip binary file
                }

                let result = match self.process_single_file(file) {
                    Ok(file_result) => {
                        files_processed.fetch_add(1, Ordering::Relaxed);
                        total_lines.fetch_add(file_result.lines_processed, Ordering::Relaxed);
                        total_matches.fetch_add(file_result.matches_found, Ordering::Relaxed);

                        if file_result.matches_found > 0 {
                            files_matched.fetch_add(1, Ordering::Relaxed);
                        }
                        if file_result.modified {
                            files_modified.fetch_add(1, Ordering::Relaxed);
                        }

                        file_result
                    }
                    Err(e) => FileResult {
                        path: file.clone(),
                        matches_found: 0,
                        lines_processed: 0,
                        modified: false,
                        error: Some(e.to_string()),
                    },
                };

                if let Some(ref pb) = progress {
                    pb.inc(1);
                }

                Some(result)
            })
            .collect();

        // Flatten results, filtering out skipped files
        let file_results: Vec<FileResult> = file_results.into_iter().flatten().collect();

        let errors: Vec<String> = file_results
            .iter()
            .filter_map(|r| {
                r.error
                    .as_ref()
                    .map(|e| format!("{}: {}", r.path.display(), e))
            })
            .collect();

        let was_interrupted = shutdown_detected.load(Ordering::Relaxed);
        let skipped_count = files_skipped.load(Ordering::Relaxed);

        let mut result = MultiFileResult {
            files_processed: files_processed.load(Ordering::Relaxed),
            files_matched: files_matched.load(Ordering::Relaxed),
            files_modified: files_modified.load(Ordering::Relaxed),
            files_skipped: skipped_count,
            total_matches: total_matches.load(Ordering::Relaxed),
            total_lines: total_lines.load(Ordering::Relaxed),
            file_results,
            errors,
            walk_errors: Vec::new(), // Walk errors are collected during discovery, not processing
            interrupted: None,
        };

        if was_interrupted {
            result.interrupted = Some(ShutdownInterrupted {
                files_completed: result.files_processed,
                files_remaining: skipped_count,
            });
        }

        if let Some(pb) = progress {
            let msg = if was_interrupted {
                format!(
                    "Interrupted after {} files ({} matches, {} skipped)",
                    result.files_processed, result.total_matches, skipped_count
                )
            } else {
                format!(
                    "Processed {} files ({} matches)",
                    result.files_processed, result.total_matches
                )
            };
            pb.finish_with_message(msg);
        }

        Ok(result)
    }

    fn process_single_file(&self, path: &Path) -> Result<FileResult> {
        trace!("Processing file: {}", path.display());
        let mut processor = StreamProcessor::new(self.config.clone())?;

        // Read the file
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        if self.options.in_place {
            // Process to a temporary buffer, then write back atomically
            let mut output = Vec::new();
            let pipeline_result = processor.process_stream(reader, &mut output)?;

            // Atomic write: write to temp file, then rename
            // This ensures we never leave the file in a partial state
            let parent = path.parent().unwrap_or(Path::new("."));
            let temp_path = parent.join(format!(
                ".{}.rexpipe.tmp",
                path.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "file".to_string())
            ));

            // Write to temporary file
            fs::write(&temp_path, &output)?;

            // Create backup if requested (rename original to backup)
            if let Some(ref suffix) = self.options.backup_suffix {
                let backup_path = format!("{}{}", path.display(), suffix);
                // Remove existing backup if present
                let _ = fs::remove_file(&backup_path);
                fs::rename(path, &backup_path)?;
            }

            // Atomically replace original with temp file
            fs::rename(&temp_path, path)?;

            if pipeline_result.transformations_applied > 0 {
                debug!(
                    "Modified {}: {} matches, {} transformations",
                    path.display(),
                    pipeline_result.matches_found,
                    pipeline_result.transformations_applied
                );
            }

            Ok(FileResult {
                path: path.to_path_buf(),
                matches_found: pipeline_result.matches_found,
                lines_processed: pipeline_result.lines_processed,
                modified: pipeline_result.transformations_applied > 0,
                error: None,
            })
        } else {
            // Just process and count matches
            let mut output = std::io::sink();
            let pipeline_result = processor.process_stream(reader, &mut output)?;

            Ok(FileResult {
                path: path.to_path_buf(),
                matches_found: pipeline_result.matches_found,
                lines_processed: pipeline_result.lines_processed,
                modified: false,
                error: None,
            })
        }
    }

    /// Count matches in files without modifying them
    pub fn count_matches(&self, files: &[PathBuf]) -> Result<MultiFileResult> {
        let processor_fn = |path: &Path| -> Result<(u64, u64)> {
            let mut processor = StreamProcessor::new(self.config.clone())?;
            let file = File::open(path)?;
            let reader = BufReader::new(file);
            let mut output = std::io::sink();
            let result = processor.process_stream(reader, &mut output)?;
            Ok((result.matches_found, result.lines_processed))
        };

        // Only use parallel processing if enabled AND file count exceeds threshold
        let use_parallel = self.options.parallel && files.len() >= PARALLEL_THRESHOLD;
        let results: Vec<FileResult> = if use_parallel {
            files
                .par_iter()
                .map(|path| match processor_fn(path) {
                    Ok((matches, lines)) => FileResult {
                        path: path.clone(),
                        matches_found: matches,
                        lines_processed: lines,
                        modified: false,
                        error: None,
                    },
                    Err(e) => FileResult {
                        path: path.clone(),
                        matches_found: 0,
                        lines_processed: 0,
                        modified: false,
                        error: Some(e.to_string()),
                    },
                })
                .collect()
        } else {
            files
                .iter()
                .map(|path| match processor_fn(path) {
                    Ok((matches, lines)) => FileResult {
                        path: path.clone(),
                        matches_found: matches,
                        lines_processed: lines,
                        modified: false,
                        error: None,
                    },
                    Err(e) => FileResult {
                        path: path.clone(),
                        matches_found: 0,
                        lines_processed: 0,
                        modified: false,
                        error: Some(e.to_string()),
                    },
                })
                .collect()
        };

        let mut result = MultiFileResult::default();
        for file_result in results {
            result.files_processed += 1;
            result.total_lines += file_result.lines_processed;
            result.total_matches += file_result.matches_found;
            if file_result.matches_found > 0 {
                result.files_matched += 1;
            }
            if let Some(ref e) = file_result.error {
                result
                    .errors
                    .push(format!("{}: {}", file_result.path.display(), e));
            }
            result.file_results.push(file_result);
        }

        Ok(result)
    }

    /// List files with matches
    pub fn files_with_matches(&self, files: &[PathBuf]) -> Result<Vec<PathBuf>> {
        let result = self.count_matches(files)?;
        Ok(result
            .file_results
            .into_iter()
            .filter(|r| r.matches_found > 0 && r.error.is_none())
            .map(|r| r.path)
            .collect())
    }

    /// List files without matches
    pub fn files_without_matches(&self, files: &[PathBuf]) -> Result<Vec<PathBuf>> {
        let result = self.count_matches(files)?;
        Ok(result
            .file_results
            .into_iter()
            .filter(|r| r.matches_found == 0 && r.error.is_none())
            .map(|r| r.path)
            .collect())
    }

    /// Preview changes that would be made during in-place editing (dry-run mode)
    /// Returns a string containing unified diff output for all files that would be modified
    pub fn preview_changes(&self, files: &[PathBuf], use_color: bool) -> Result<String> {
        let mut output = String::new();
        let mut files_with_changes = 0;

        for file in files {
            match self.preview_single_file(file) {
                Ok(Some((original, modified))) => {
                    if original != modified {
                        files_with_changes += 1;
                        let patch = create_patch(&original, &modified);

                        // Add file header
                        output.push_str(&format!("--- {}\n", file.display()));
                        output.push_str(&format!("+++ {}\n", file.display()));

                        // Format the patch
                        if use_color && std::io::stderr().is_terminal() {
                            let formatter = PatchFormatter::new().with_color();
                            output.push_str(&format!("{}", formatter.fmt_patch(&patch)));
                        } else {
                            output.push_str(&format!("{}", patch));
                        }
                        output.push('\n');
                    }
                }
                Ok(None) => {
                    // No changes for this file
                }
                Err(e) => {
                    output.push_str(&format!("Error processing {}: {}\n", file.display(), e));
                }
            }
        }

        if files_with_changes == 0 {
            output.push_str("No changes would be made.\n");
        } else {
            output.push_str(&format!(
                "\n{} file(s) would be modified.\n",
                files_with_changes
            ));
        }

        Ok(output)
    }

    /// Preview changes for a single file
    /// Returns (original_content, modified_content) if the file has matches
    fn preview_single_file(&self, path: &Path) -> Result<Option<(String, String)>> {
        let mut processor = StreamProcessor::new(self.config.clone())?;

        // Read the original file
        let original = fs::read_to_string(path)?;

        // Process through the pipeline
        let reader = std::io::Cursor::new(original.as_bytes());
        let mut output = Vec::new();
        let result = processor.process_stream(reader, &mut output)?;

        // If no transformations, return None
        if result.transformations_applied == 0 {
            return Ok(None);
        }

        let modified = String::from_utf8_lossy(&output).to_string();
        Ok(Some((original, modified)))
    }
}

impl MultiFileResult {
    pub fn summary(&self) -> String {
        let mut summary = format!(
            "Files: {} processed, {} matched, {} modified",
            self.files_processed, self.files_matched, self.files_modified
        );

        if self.files_skipped > 0 {
            summary.push_str(&format!(", {} skipped", self.files_skipped));
        }

        summary.push_str(&format!(
            "\nLines: {} total\nMatches: {} total",
            self.total_lines, self.total_matches
        ));

        if !self.errors.is_empty() {
            summary.push_str(&format!("\nProcessing errors: {}", self.errors.len()));
        }

        if !self.walk_errors.is_empty() {
            summary.push_str(&format!(
                "\nAccess errors: {} (some files/directories were inaccessible)",
                self.walk_errors.len()
            ));
        }

        if let Some(ref interrupted) = self.interrupted {
            summary.push_str(&format!(
                "\nInterrupted: {} files completed, {} files remaining",
                interrupted.files_completed, interrupted.files_remaining
            ));
        }

        summary
    }

    pub fn has_matches(&self) -> bool {
        self.total_matches > 0
    }

    /// Check if any errors occurred during processing.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Check if any permission/access errors occurred during directory walking.
    ///
    /// When true, some files or directories were inaccessible and skipped.
    /// Check `walk_errors` field for details.
    pub fn has_walk_errors(&self) -> bool {
        !self.walk_errors.is_empty()
    }

    /// Check if any errors occurred (processing errors or walk errors).
    pub fn has_any_errors(&self) -> bool {
        self.has_errors() || self.has_walk_errors()
    }

    /// Check if processing was interrupted by a shutdown signal.
    ///
    /// When true, some files may not have been processed. Check
    /// `interrupted` field for details on completed vs remaining files.
    pub fn was_interrupted(&self) -> bool {
        self.interrupted.is_some()
    }
}

// ============================================================================
// Async Processing Support (requires `async` feature)
// ============================================================================

#[cfg(feature = "async")]
pub mod async_processing {
    //! Async file processing using tokio
    //!
    //! This module provides async versions of the file processing functions
    //! for non-blocking I/O operations. Enable with `--features async`.
    //!
    //! # Example
    //!
    //! ```ignore
    //! use rexpipe::files::{AsyncMultiFileProcessor, FileProcessingOptions};
    //! use rexpipe::pipeline::PipelineConfig;
    //!
    //! #[tokio::main]
    //! async fn main() {
    //!     let config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUM"));
    //!     let options = FileProcessingOptions::default();
    //!     let processor = AsyncMultiFileProcessor::new(config, options);
    //!
    //!     let files = vec![PathBuf::from("test.txt")];
    //!     let result = processor.process_files_async(&files).await.unwrap();
    //!     println!("Processed {} files", result.files_processed);
    //! }
    //! ```

    use super::*;
    use tokio::fs as async_fs;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as AsyncBufReader};

    /// Async multi-file processor for non-blocking batch operations.
    ///
    /// This processor uses tokio for async I/O operations, allowing efficient
    /// processing of many files without blocking. Ideal for I/O-bound workloads
    /// where files may be on network storage or when processing many small files.
    ///
    /// Unlike the synchronous [`MultiFileProcessor`], this version processes
    /// files one at a time asynchronously, yielding to the runtime between files.
    ///
    /// # Features
    ///
    /// - Async file reading and writing
    /// - In-place editing with atomic writes
    /// - Match counting without modification
    /// - File filtering by match presence
    ///
    /// # Example
    ///
    /// ```ignore
    /// use rexpipe::files::{AsyncMultiFileProcessor, FileProcessingOptions};
    /// use rexpipe::pipeline::PipelineConfig;
    ///
    /// let config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUM"));
    /// let processor = AsyncMultiFileProcessor::new(config, FileProcessingOptions::default());
    ///
    /// // Use within an async context
    /// let result = processor.process_files_async(&files).await?;
    /// ```
    pub struct AsyncMultiFileProcessor {
        config: PipelineConfig,
        options: FileProcessingOptions,
    }

    impl AsyncMultiFileProcessor {
        /// Create a new async processor with the given configuration and options.
        ///
        /// # Arguments
        ///
        /// * `config` - Pipeline configuration defining patterns and transformations
        /// * `options` - File processing options (in-place editing, backups, etc.)
        pub fn new(config: PipelineConfig, options: FileProcessingOptions) -> Self {
            Self { config, options }
        }

        /// Asynchronously process multiple files
        pub async fn process_files_async(
            &self,
            files: &[PathBuf],
        ) -> Result<MultiFileResult, String> {
            let mut result = MultiFileResult::default();
            let mut handles = Vec::new();

            // Clone config and options for each task
            for file in files.iter().cloned() {
                let config = self.config.clone();
                let options = self.options.clone();
                let file_clone = file.clone();

                let handle = tokio::spawn(async move {
                    process_single_file_async(&file_clone, &config, &options).await
                });

                handles.push((file, handle));
            }

            // Await all results
            for (file, handle) in handles {
                match handle.await {
                    Ok(Ok(file_result)) => {
                        result.files_processed += 1;
                        result.total_lines += file_result.lines_processed;
                        result.total_matches += file_result.matches_found;

                        if file_result.matches_found > 0 {
                            result.files_matched += 1;
                        }
                        if file_result.modified {
                            result.files_modified += 1;
                        }

                        result.file_results.push(file_result);
                    }
                    Ok(Err(e)) => {
                        result.errors.push(format!("{}: {}", file.display(), e));
                        result.file_results.push(FileResult {
                            path: file.clone(),
                            matches_found: 0,
                            lines_processed: 0,
                            modified: false,
                            error: Some(e),
                        });
                    }
                    Err(e) => {
                        result
                            .errors
                            .push(format!("{}: task panicked: {}", file.display(), e));
                        result.file_results.push(FileResult {
                            path: file.clone(),
                            matches_found: 0,
                            lines_processed: 0,
                            modified: false,
                            error: Some(format!("task panicked: {}", e)),
                        });
                    }
                }
            }

            Ok(result)
        }

        /// Asynchronously count matches in files
        pub async fn count_matches_async(
            &self,
            files: &[PathBuf],
        ) -> Result<MultiFileResult, String> {
            let mut result = MultiFileResult::default();
            let mut handles = Vec::new();

            for file in files.iter().cloned() {
                let config = self.config.clone();
                let file_clone = file.clone();

                let handle = tokio::spawn(async move {
                    count_matches_single_file_async(&file_clone, &config).await
                });

                handles.push((file, handle));
            }

            for (file, handle) in handles {
                match handle.await {
                    Ok(Ok((matches, lines))) => {
                        result.files_processed += 1;
                        result.total_lines += lines;
                        result.total_matches += matches;

                        if matches > 0 {
                            result.files_matched += 1;
                        }

                        result.file_results.push(FileResult {
                            path: file,
                            matches_found: matches,
                            lines_processed: lines,
                            modified: false,
                            error: None,
                        });
                    }
                    Ok(Err(e)) => {
                        result.errors.push(format!("{}: {}", file.display(), e));
                        result.file_results.push(FileResult {
                            path: file,
                            matches_found: 0,
                            lines_processed: 0,
                            modified: false,
                            error: Some(e),
                        });
                    }
                    Err(e) => {
                        result.errors.push(format!("{}: {}", file.display(), e));
                        result.file_results.push(FileResult {
                            path: file,
                            matches_found: 0,
                            lines_processed: 0,
                            modified: false,
                            error: Some(e.to_string()),
                        });
                    }
                }
            }

            Ok(result)
        }

        /// Asynchronously list files with matches
        pub async fn files_with_matches_async(
            &self,
            files: &[PathBuf],
        ) -> Result<Vec<PathBuf>, String> {
            let result = self.count_matches_async(files).await?;
            Ok(result
                .file_results
                .into_iter()
                .filter(|r| r.matches_found > 0)
                .map(|r| r.path)
                .collect())
        }

        /// Asynchronously list files without matches
        pub async fn files_without_matches_async(
            &self,
            files: &[PathBuf],
        ) -> Result<Vec<PathBuf>, String> {
            let result = self.count_matches_async(files).await?;
            Ok(result
                .file_results
                .into_iter()
                .filter(|r| r.matches_found == 0)
                .map(|r| r.path)
                .collect())
        }
    }

    /// Asynchronously process a single file
    async fn process_single_file_async(
        path: &Path,
        config: &PipelineConfig,
        options: &FileProcessingOptions,
    ) -> Result<FileResult, String> {
        let content = async_fs::read_to_string(path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let mut processor = StreamProcessor::new(config.clone())
            .map_err(|e| format!("Failed to create processor: {}", e))?;

        let reader = std::io::Cursor::new(content.as_bytes());
        let mut output = Vec::new();
        let pipeline_result = processor
            .process_stream(reader, &mut output)
            .map_err(|e| format!("Failed to process stream: {}", e))?;

        if options.in_place {
            // Atomic write: write to temp file, then rename
            let parent = path.parent().unwrap_or(Path::new("."));
            let temp_path = parent.join(format!(
                ".{}.rexpipe.tmp",
                path.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "file".to_string())
            ));

            // Write to temporary file
            async_fs::write(&temp_path, &output)
                .await
                .map_err(|e| format!("Failed to write temp file: {}", e))?;

            // Create backup if requested (rename original to backup)
            if let Some(ref suffix) = options.backup_suffix {
                let backup_path = format!("{}{}", path.display(), suffix);
                // Remove existing backup if present
                let _ = async_fs::remove_file(&backup_path).await;
                async_fs::rename(path, &backup_path)
                    .await
                    .map_err(|e| format!("Failed to create backup: {}", e))?;
            }

            // Atomically replace original with temp file
            async_fs::rename(&temp_path, path)
                .await
                .map_err(|e| format!("Failed to rename temp file: {}", e))?;

            Ok(FileResult {
                path: path.to_path_buf(),
                matches_found: pipeline_result.matches_found,
                lines_processed: pipeline_result.lines_processed,
                modified: pipeline_result.transformations_applied > 0,
                error: None,
            })
        } else {
            Ok(FileResult {
                path: path.to_path_buf(),
                matches_found: pipeline_result.matches_found,
                lines_processed: pipeline_result.lines_processed,
                modified: false,
                error: None,
            })
        }
    }

    /// Asynchronously count matches in a single file
    async fn count_matches_single_file_async(
        path: &Path,
        config: &PipelineConfig,
    ) -> Result<(u64, u64), String> {
        let content = async_fs::read_to_string(path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let mut processor = StreamProcessor::new(config.clone())
            .map_err(|e| format!("Failed to create processor: {}", e))?;

        let reader = std::io::Cursor::new(content.as_bytes());
        let mut output = std::io::sink();
        let result = processor
            .process_stream(reader, &mut output)
            .map_err(|e| format!("Failed to process stream: {}", e))?;

        Ok((result.matches_found, result.lines_processed))
    }

    /// Read a file asynchronously and process line by line
    pub async fn read_lines_async(path: &Path) -> Result<Vec<String>, String> {
        let file = async_fs::File::open(path)
            .await
            .map_err(|e| format!("Failed to open file: {}", e))?;
        let reader = AsyncBufReader::new(file);
        let mut lines = reader.lines();
        let mut result = Vec::new();

        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|e| format!("Failed to read line: {}", e))?
        {
            result.push(line);
        }

        Ok(result)
    }

    /// Write lines to a file asynchronously
    pub async fn write_lines_async(path: &Path, lines: &[String]) -> Result<(), String> {
        let mut file = async_fs::File::create(path)
            .await
            .map_err(|e| format!("Failed to create file: {}", e))?;

        for line in lines {
            file.write_all(line.as_bytes())
                .await
                .map_err(|e| format!("Failed to write line: {}", e))?;
            file.write_all(b"\n")
                .await
                .map_err(|e| format!("Failed to write newline: {}", e))?;
        }

        file.flush()
            .await
            .map_err(|e| format!("Failed to flush file: {}", e))?;
        Ok(())
    }
}

#[cfg(feature = "async")]
#[allow(unused_imports)]
pub use async_processing::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::PipelineConfig;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_files(dir: &Path) -> Vec<PathBuf> {
        let files = vec![
            ("test1.txt", "Hello 123 World\nTest 456 Line"),
            ("test2.txt", "No numbers here\nJust text"),
            ("test3.rs", "let x = 789;\nlet y = 012;"),
        ];

        let mut paths = Vec::new();
        for (name, content) in files {
            let path = dir.join(name);
            let mut file = File::create(&path).unwrap();
            file.write_all(content.as_bytes()).unwrap();
            paths.push(path);
        }
        paths
    }

    #[test]
    fn test_file_discovery() {
        let temp_dir = TempDir::new().unwrap();
        let _files = create_test_files(temp_dir.path());

        let config = PipelineConfig::from_inline_pattern(r"\d+", None);
        let options = FileProcessingOptions::default();
        let processor = MultiFileProcessor::new(config, options);

        let discovered = processor
            .discover_files(&[temp_dir.path().to_path_buf()])
            .unwrap();
        assert_eq!(discovered.len(), 3);
    }

    #[test]
    fn test_count_matches() {
        let temp_dir = TempDir::new().unwrap();
        let files = create_test_files(temp_dir.path());

        let config = PipelineConfig::from_inline_pattern(r"\d+", None);
        let options = FileProcessingOptions::default();
        let processor = MultiFileProcessor::new(config, options);

        let result = processor.count_matches(&files).unwrap();
        assert_eq!(result.files_processed, 3);
        assert_eq!(result.files_matched, 2); // test1.txt and test3.rs have numbers
    }

    #[test]
    fn test_in_place_modification() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        {
            let mut file = File::create(&file_path).unwrap();
            file.write_all(b"Hello 123 World").unwrap();
        }

        let config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUMBER"));
        let options = FileProcessingOptions::default()
            .in_place(true)
            .backup_suffix(Some(".bak".to_string()));

        let processor = MultiFileProcessor::new(config, options);
        let result = processor
            .process_files(std::slice::from_ref(&file_path))
            .unwrap();

        assert_eq!(result.files_modified, 1);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello NUMBER World\n");

        // Check backup was created
        let backup_path = format!("{}.bak", file_path.display());
        assert!(Path::new(&backup_path).exists());
    }

    #[test]
    fn test_files_with_matches() {
        let temp_dir = TempDir::new().unwrap();
        let files = create_test_files(temp_dir.path());

        let config = PipelineConfig::from_inline_pattern(r"\d+", None);
        let options = FileProcessingOptions::default();
        let processor = MultiFileProcessor::new(config, options);

        let matching = processor.files_with_matches(&files).unwrap();
        assert_eq!(matching.len(), 2);
    }

    #[test]
    fn test_parallel_processing() {
        let temp_dir = TempDir::new().unwrap();
        let files = create_test_files(temp_dir.path());

        let config = PipelineConfig::from_inline_pattern(r"\d+", None);
        let options = FileProcessingOptions::default().parallel(true);
        let processor = MultiFileProcessor::new(config, options);

        let result = processor.count_matches(&files).unwrap();
        assert_eq!(result.files_processed, 3);
    }

    #[test]
    fn test_glob_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let _files = create_test_files(temp_dir.path());

        let config = PipelineConfig::from_inline_pattern(r"\d+", None);
        let options = FileProcessingOptions::default().include_pattern("*.txt".to_string());

        let processor = MultiFileProcessor::new(config, options);
        let discovered = processor
            .discover_files(&[temp_dir.path().to_path_buf()])
            .unwrap();

        assert_eq!(discovered.len(), 2); // Only .txt files
    }

    // ========================================================================
    // Symlink handling tests (Unix-only)
    // ========================================================================

    #[cfg(unix)]
    mod symlink_tests {
        use super::*;
        use std::os::unix::fs::symlink;

        #[test]
        fn test_symlink_files_discovered_but_directories_not_followed() {
            let temp_dir = TempDir::new().unwrap();

            // Create a real file
            let real_file = temp_dir.path().join("real.txt");
            fs::write(&real_file, "content 123").unwrap();

            // Create a symlink to the file
            // Note: symlink files ARE discovered (the symlink itself is a valid file entry)
            // This matches the behavior of ripgrep and other tools
            let link_file = temp_dir.path().join("link.txt");
            symlink(&real_file, &link_file).unwrap();

            let config = PipelineConfig::from_inline_pattern(r"\d+", None);
            let options = FileProcessingOptions::default();
            let processor = MultiFileProcessor::new(config, options);

            let discovered = processor
                .discover_files(&[temp_dir.path().to_path_buf()])
                .unwrap();

            // Both the real file and symlink file are discovered
            // This is correct behavior - symlinks to files ARE valid files
            // Only symlinks to directories are not followed (to prevent infinite loops)
            assert_eq!(discovered.len(), 2);
        }

        #[test]
        fn test_symlink_directory_not_followed() {
            let temp_dir = TempDir::new().unwrap();

            // Create a subdirectory with a file
            let subdir = temp_dir.path().join("subdir");
            fs::create_dir(&subdir).unwrap();
            fs::write(subdir.join("file.txt"), "content 456").unwrap();

            // Create a symlink to the subdirectory
            let link_dir = temp_dir.path().join("link_dir");
            symlink(&subdir, &link_dir).unwrap();

            let config = PipelineConfig::from_inline_pattern(r"\d+", None);
            let options = FileProcessingOptions::default();
            let processor = MultiFileProcessor::new(config, options);

            let discovered = processor
                .discover_files(&[temp_dir.path().to_path_buf()])
                .unwrap();

            // Only one file should be discovered (from the real subdir)
            // The symlinked directory should not be followed
            assert_eq!(discovered.len(), 1);
        }

        #[test]
        fn test_symlink_loop_handled_gracefully() {
            let temp_dir = TempDir::new().unwrap();

            // Create a circular symlink (link pointing to itself)
            let link_path = temp_dir.path().join("loop_link");
            // Create a symlink pointing to the parent (would cause infinite loop if followed)
            symlink(temp_dir.path(), &link_path).unwrap();

            // Create a real file
            fs::write(temp_dir.path().join("real.txt"), "content").unwrap();

            let config = PipelineConfig::from_inline_pattern(r"\d+", None);
            let options = FileProcessingOptions::default();
            let processor = MultiFileProcessor::new(config, options);

            // Should not hang or crash - should handle the loop gracefully
            let discovered = processor
                .discover_files(&[temp_dir.path().to_path_buf()])
                .unwrap();

            // Only the real file should be discovered
            assert_eq!(discovered.len(), 1);
            assert_eq!(discovered[0].file_name().unwrap(), "real.txt");
        }

        #[test]
        fn test_broken_symlink_handled_gracefully() {
            let temp_dir = TempDir::new().unwrap();

            // Create a symlink to a non-existent file
            let broken_link = temp_dir.path().join("broken.txt");
            symlink("/nonexistent/path/file.txt", &broken_link).unwrap();

            // Create a real file
            fs::write(temp_dir.path().join("real.txt"), "content").unwrap();

            let config = PipelineConfig::from_inline_pattern(r"\d+", None);
            let options = FileProcessingOptions::default();
            let processor = MultiFileProcessor::new(config, options);

            // Should not crash on broken symlink
            let discovered = processor
                .discover_files(&[temp_dir.path().to_path_buf()])
                .unwrap();

            // Only the real file should be discovered
            assert_eq!(discovered.len(), 1);
            assert_eq!(discovered[0].file_name().unwrap(), "real.txt");
        }

        #[test]
        fn test_explicit_symlink_file_can_be_processed() {
            let temp_dir = TempDir::new().unwrap();

            // Create a real file
            let real_file = temp_dir.path().join("real.txt");
            fs::write(&real_file, "content 123").unwrap();

            // Create a symlink to the file
            let link_file = temp_dir.path().join("link.txt");
            symlink(&real_file, &link_file).unwrap();

            let config = PipelineConfig::from_inline_pattern(r"\d+", None);
            let options = FileProcessingOptions::default();
            let processor = MultiFileProcessor::new(config, options);

            // When a symlink is explicitly specified, it should be processable
            // (the symlink itself passes is_file() check)
            let result = processor.count_matches(&[link_file.clone()]).unwrap();

            // The symlink file should be processed
            assert_eq!(result.files_processed, 1);
            assert_eq!(result.files_matched, 1);
        }
    }

    // ========================================================================
    // Binary file handling tests
    // ========================================================================

    #[test]
    fn test_binary_file_detection() {
        let temp_dir = TempDir::new().unwrap();

        // Create a text file
        let text_file = temp_dir.path().join("text.txt");
        fs::write(&text_file, "Hello world\nThis is text\n").unwrap();

        // Create a binary file (contains NUL bytes)
        let binary_file = temp_dir.path().join("binary.bin");
        fs::write(&binary_file, b"Hello\x00World\x00Binary").unwrap();

        assert!(!is_binary_file(&text_file).unwrap());
        assert!(is_binary_file(&binary_file).unwrap());
    }

    #[test]
    fn test_binary_mode_auto_skips_binary() {
        let temp_dir = TempDir::new().unwrap();

        // Create a text file
        fs::write(temp_dir.path().join("text.txt"), "content 123").unwrap();

        // Create a binary file
        fs::write(temp_dir.path().join("binary.bin"), b"data\x00with\x00nulls 456").unwrap();

        let config = PipelineConfig::from_inline_pattern(r"\d+", None);
        let options = FileProcessingOptions::default()
            .binary_mode(BinaryMode::Auto);
        let processor = MultiFileProcessor::new(config, options);

        let files = vec![
            temp_dir.path().join("text.txt"),
            temp_dir.path().join("binary.bin"),
        ];

        let result = processor.process_files(&files).unwrap();

        // Text file processed, binary file skipped
        assert_eq!(result.files_processed, 1);
        assert_eq!(result.files_skipped, 1);
    }

    #[test]
    fn test_binary_mode_text_processes_all() {
        let temp_dir = TempDir::new().unwrap();

        // Create a text file
        fs::write(temp_dir.path().join("text.txt"), "content 123").unwrap();

        // Create a binary file
        fs::write(temp_dir.path().join("binary.bin"), b"data\x00with\x00nulls 456").unwrap();

        let config = PipelineConfig::from_inline_pattern(r"\d+", None);
        let options = FileProcessingOptions::default()
            .binary_mode(BinaryMode::Text);
        let processor = MultiFileProcessor::new(config, options);

        let files = vec![
            temp_dir.path().join("text.txt"),
            temp_dir.path().join("binary.bin"),
        ];

        let result = processor.process_files(&files).unwrap();

        // Both files processed in text mode
        assert_eq!(result.files_processed, 2);
        assert_eq!(result.files_skipped, 0);
    }
}
