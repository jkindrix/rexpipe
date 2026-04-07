//! Checkpoint and incremental processing for rexpipe.
//!
//! This module provides functionality for:
//!
//! - **Checkpointing**: Save and resume processing state
//! - **Incremental Processing**: Only process new/changed content
//! - **Git Integration**: Process only lines changed since a commit
//!
//! ## Features
//!
//! - Resume interrupted processing from saved checkpoint
//! - Process only new lines in growing log files
//! - Git-diff aware processing for changed files
//! - File position tracking for streaming sources
//!
//! ## Example
//!
//! ```ignore
//! use rexpipe::checkpoint::{Checkpoint, CheckpointConfig};
//! use std::path::PathBuf;
//!
//! let config = CheckpointConfig::new()
//!     .with_checkpoint_file("/var/lib/rexpipe/checkpoint.json")
//!     .with_auto_save(true);
//!
//! let mut checkpoint = Checkpoint::new(config);
//!
//! // Process from last position
//! let offset = checkpoint.get_file_position("access.log");
//! // ... process file from offset ...
//! checkpoint.update_file_position("access.log", 1024);
//! checkpoint.save().unwrap();
//! ```

use serde::{Deserialize, Serialize};
#[cfg(feature = "cli")]
use std::collections::HashMap;
#[cfg(feature = "cli")]
use std::fs::{self, File};
#[cfg(feature = "cli")]
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
#[cfg(feature = "cli")]
use std::path::Path;
use std::path::PathBuf;
#[cfg(feature = "cli")]
use std::process::Command;
use thiserror::Error;
#[cfg(feature = "cli")]
use web_time::{SystemTime, UNIX_EPOCH};

/// Errors that can occur during checkpoint operations.
#[derive(Error, Debug)]
pub enum CheckpointError {
    #[error("Failed to load checkpoint: {0}")]
    LoadError(String),

    #[error("Failed to save checkpoint: {0}")]
    SaveError(String),

    #[error("Git operation failed: {0}")]
    GitError(String),

    #[error("File tracking error: {0}")]
    TrackingError(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, CheckpointError>;

/// Configuration for checkpoint functionality.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CheckpointConfig {
    /// Enable checkpointing
    #[serde(default)]
    pub enabled: bool,

    /// Path to checkpoint file
    #[serde(default)]
    pub checkpoint_file: Option<PathBuf>,

    /// Auto-save checkpoint after each file
    #[serde(default = "default_auto_save")]
    pub auto_save: bool,

    /// Save interval in seconds (0 = save after each file)
    #[serde(default)]
    pub save_interval_secs: u64,

    /// Use git diff for incremental processing
    #[serde(default)]
    pub git_diff_mode: bool,

    /// Git reference for diff comparison (e.g., "HEAD~1", "main")
    #[serde(default)]
    pub git_ref: Option<String>,

    /// Track file modification times
    #[serde(default = "default_true")]
    pub track_mtime: bool,

    /// Track file content hashes
    #[serde(default)]
    pub track_content_hash: bool,
}

fn default_auto_save() -> bool {
    true
}

fn default_true() -> bool {
    true
}

impl CheckpointConfig {
    /// Create a new checkpoint configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable checkpointing.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set the checkpoint file path.
    pub fn with_checkpoint_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.checkpoint_file = Some(path.into());
        self.enabled = true;
        self
    }

    /// Enable auto-save.
    pub fn with_auto_save(mut self, auto_save: bool) -> Self {
        self.auto_save = auto_save;
        self
    }

    /// Enable git diff mode.
    pub fn with_git_diff(mut self, git_ref: impl Into<String>) -> Self {
        self.git_diff_mode = true;
        self.git_ref = Some(git_ref.into());
        self
    }
}

/// Tracked state for a single file.
///
/// Only available with the `cli` feature — checkpoint state is tied to
/// filesystem-based incremental processing.
#[cfg(feature = "cli")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileState {
    /// Absolute path to the file
    pub path: PathBuf,
    /// Byte offset of last processed position
    pub byte_offset: u64,
    /// Line number of last processed line
    pub line_number: u64,
    /// File modification time (Unix timestamp)
    pub mtime: Option<u64>,
    /// Content hash of processed portion
    pub content_hash: Option<String>,
    /// File size at last checkpoint
    pub size: u64,
    /// Inode number (for detecting file rotation)
    #[serde(default)]
    pub inode: Option<u64>,
    /// Last processed timestamp
    pub last_processed: u64,
}

#[cfg(feature = "cli")]
impl FileState {
    /// Create a new file state starting from the beginning.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            path: path.into(),
            byte_offset: 0,
            line_number: 0,
            mtime: None,
            content_hash: None,
            size: 0,
            inode: None,
            last_processed: now,
        }
    }

    /// Check if the file has been modified since last checkpoint.
    pub fn is_modified(&self, current_mtime: u64, current_size: u64) -> bool {
        if let Some(mtime) = self.mtime {
            current_mtime > mtime || current_size != self.size
        } else {
            true
        }
    }

    /// Check if the file has been rotated (different inode).
    pub fn is_rotated(&self, current_inode: u64) -> bool {
        if let Some(inode) = self.inode {
            current_inode != inode
        } else {
            false
        }
    }
}

/// Complete checkpoint state.
#[cfg(feature = "cli")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointState {
    /// Version of checkpoint format
    pub version: String,
    /// Pipeline identifier
    pub pipeline_id: Option<String>,
    /// When checkpoint was created
    pub created_at: u64,
    /// When checkpoint was last updated
    pub updated_at: u64,
    /// File states by path
    pub files: HashMap<PathBuf, FileState>,
    /// Custom metadata
    pub metadata: HashMap<String, String>,
}

#[cfg(feature = "cli")]
impl Default for CheckpointState {
    fn default() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            version: "1.0.0".to_string(),
            pipeline_id: None,
            created_at: now,
            updated_at: now,
            files: HashMap::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Checkpoint manager for tracking and resuming processing state.
#[cfg(feature = "cli")]
pub struct Checkpoint {
    config: CheckpointConfig,
    state: CheckpointState,
    modified: bool,
    last_save: web_time::Instant,
}

#[cfg(feature = "cli")]
impl Checkpoint {
    /// Create a new checkpoint with default state.
    pub fn new(config: CheckpointConfig) -> Self {
        Self {
            config,
            state: CheckpointState::default(),
            modified: false,
            last_save: web_time::Instant::now(),
        }
    }

    /// Load existing checkpoint or create new one.
    pub fn load_or_create(config: &CheckpointConfig) -> Result<Self> {
        if let Some(ref path) = config.checkpoint_file {
            if path.exists() {
                return Self::load(config.clone(), path);
            }
        }
        Ok(Self::new(config.clone()))
    }

    /// Load checkpoint from file.
    pub fn load(config: CheckpointConfig, path: impl AsRef<Path>) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref()).map_err(|e| {
            CheckpointError::LoadError(format!("{}: {}", path.as_ref().display(), e))
        })?;

        let state: CheckpointState = serde_json::from_str(&content)
            .map_err(|e| CheckpointError::LoadError(format!("Invalid checkpoint JSON: {}", e)))?;

        Ok(Self {
            config,
            state,
            modified: false,
            last_save: web_time::Instant::now(),
        })
    }

    /// Save checkpoint to file.
    pub fn save(&mut self) -> Result<()> {
        if let Some(ref path) = self.config.checkpoint_file {
            // Create parent directories if needed
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            // Update timestamp
            self.state.updated_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let json = serde_json::to_string_pretty(&self.state)?;
            let mut file = File::create(path)?;
            file.write_all(json.as_bytes())?;

            self.modified = false;
            self.last_save = web_time::Instant::now();
        }
        Ok(())
    }

    /// Save if auto-save is enabled and conditions are met.
    pub fn save_if_needed(&mut self) -> Result<()> {
        if !self.modified || !self.config.auto_save {
            return Ok(());
        }

        if self.config.save_interval_secs > 0 {
            let elapsed = self.last_save.elapsed().as_secs();
            if elapsed < self.config.save_interval_secs {
                return Ok(());
            }
        }

        self.save()
    }

    /// Check if checkpointing is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the file state for a path.
    pub fn get_file_state(&self, path: impl AsRef<Path>) -> Option<&FileState> {
        self.state.files.get(path.as_ref())
    }

    /// Get the byte offset for a file.
    pub fn get_file_offset(&self, path: impl AsRef<Path>) -> u64 {
        self.state
            .files
            .get(path.as_ref())
            .map(|s| s.byte_offset)
            .unwrap_or(0)
    }

    /// Get the line number for a file.
    pub fn get_line_number(&self, path: impl AsRef<Path>) -> u64 {
        self.state
            .files
            .get(path.as_ref())
            .map(|s| s.line_number)
            .unwrap_or(0)
    }

    /// Update file state after processing.
    pub fn update_file_state(
        &mut self,
        path: impl AsRef<Path>,
        byte_offset: u64,
        line_number: u64,
        size: u64,
    ) {
        let path = path.as_ref().to_path_buf();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let state = self
            .state
            .files
            .entry(path.clone())
            .or_insert_with(|| FileState::new(path));

        state.byte_offset = byte_offset;
        state.line_number = line_number;
        state.size = size;
        state.last_processed = now;

        // Get mtime if tracking enabled
        if self.config.track_mtime {
            if let Ok(metadata) = fs::metadata(&state.path) {
                if let Ok(mtime) = metadata.modified() {
                    state.mtime = Some(
                        mtime
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs(),
                    );
                }
            }
        }

        self.modified = true;
    }

    /// Reset file state (e.g., after rotation).
    pub fn reset_file_state(&mut self, path: impl AsRef<Path>) {
        if let Some(state) = self.state.files.get_mut(path.as_ref()) {
            state.byte_offset = 0;
            state.line_number = 0;
            state.content_hash = None;
            self.modified = true;
        }
    }

    /// Remove file from checkpoint.
    pub fn remove_file(&mut self, path: impl AsRef<Path>) {
        if self.state.files.remove(path.as_ref()).is_some() {
            self.modified = true;
        }
    }

    /// Get all tracked files.
    pub fn tracked_files(&self) -> impl Iterator<Item = &PathBuf> {
        self.state.files.keys()
    }

    /// Set pipeline identifier.
    pub fn set_pipeline_id(&mut self, id: impl Into<String>) {
        self.state.pipeline_id = Some(id.into());
        self.modified = true;
    }

    /// Set custom metadata.
    pub fn set_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.state.metadata.insert(key.into(), value.into());
        self.modified = true;
    }

    /// Check if a file needs processing based on checkpoint state.
    pub fn needs_processing(&self, path: impl AsRef<Path>) -> Result<bool> {
        let path = path.as_ref();

        // If we don't have state for this file, it needs processing
        let state = match self.state.files.get(path) {
            Some(s) => s,
            None => return Ok(true),
        };

        // Get current file metadata
        let metadata = fs::metadata(path)?;
        let current_size = metadata.len();

        // If file is smaller than our offset, it was probably rotated
        if current_size < state.byte_offset {
            return Ok(true);
        }

        // If file has grown, there's new content
        if current_size > state.size {
            return Ok(true);
        }

        // Check mtime if tracking enabled
        if self.config.track_mtime {
            if let Ok(mtime) = metadata.modified() {
                let current_mtime = mtime
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                if state.is_modified(current_mtime, current_size) {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}

/// Git-based incremental processing support.
#[cfg(feature = "cli")]
pub struct GitDiff {
    /// Repository root path
    repo_root: PathBuf,
    /// Reference to compare against
    base_ref: String,
}

#[cfg(feature = "cli")]
impl GitDiff {
    /// Create a new GitDiff for the given repository.
    pub fn new(repo_root: impl Into<PathBuf>, base_ref: impl Into<String>) -> Self {
        Self {
            repo_root: repo_root.into(),
            base_ref: base_ref.into(),
        }
    }

    /// Discover the repository root from a path.
    pub fn discover(path: impl AsRef<Path>, base_ref: impl Into<String>) -> Result<Self> {
        let output = Command::new("git")
            .arg("-C")
            .arg(path.as_ref())
            .arg("rev-parse")
            .arg("--show-toplevel")
            .output()
            .map_err(|e| CheckpointError::GitError(format!("Failed to run git: {}", e)))?;

        if !output.status.success() {
            return Err(CheckpointError::GitError(
                "Not a git repository".to_string(),
            ));
        }

        let repo_root = String::from_utf8_lossy(&output.stdout).trim().to_string();

        Ok(Self {
            repo_root: PathBuf::from(repo_root),
            base_ref: base_ref.into(),
        })
    }

    /// Get list of files changed since the base reference.
    pub fn changed_files(&self) -> Result<Vec<PathBuf>> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.repo_root)
            .arg("diff")
            .arg("--name-only")
            .arg(&self.base_ref)
            .output()
            .map_err(|e| CheckpointError::GitError(format!("Failed to run git diff: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CheckpointError::GitError(format!(
                "git diff failed: {}",
                stderr
            )));
        }

        let files = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| self.repo_root.join(line))
            .collect();

        Ok(files)
    }

    /// Get line ranges that changed in a specific file.
    pub fn changed_lines(&self, file: impl AsRef<Path>) -> Result<Vec<LineRange>> {
        let relative_path = file
            .as_ref()
            .strip_prefix(&self.repo_root)
            .unwrap_or(file.as_ref());

        let output = Command::new("git")
            .arg("-C")
            .arg(&self.repo_root)
            .arg("diff")
            .arg("-U0")
            .arg(&self.base_ref)
            .arg("--")
            .arg(relative_path)
            .output()
            .map_err(|e| CheckpointError::GitError(format!("Failed to run git diff: {}", e)))?;

        if !output.status.success() {
            return Ok(Vec::new()); // File might not exist in base ref
        }

        let diff_output = String::from_utf8_lossy(&output.stdout);
        let ranges = parse_diff_hunks(&diff_output);

        Ok(ranges)
    }

    /// Check if a file has changes.
    pub fn has_changes(&self, file: impl AsRef<Path>) -> Result<bool> {
        let relative_path = file
            .as_ref()
            .strip_prefix(&self.repo_root)
            .unwrap_or(file.as_ref());

        let output = Command::new("git")
            .arg("-C")
            .arg(&self.repo_root)
            .arg("diff")
            .arg("--quiet")
            .arg(&self.base_ref)
            .arg("--")
            .arg(relative_path)
            .status()
            .map_err(|e| CheckpointError::GitError(format!("Failed to run git: {}", e)))?;

        // git diff --quiet exits with 1 if there are differences
        Ok(!output.success())
    }
}

/// A range of line numbers.
#[cfg(feature = "cli")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    /// Starting line number (1-based)
    pub start: u64,
    /// Ending line number (inclusive)
    pub end: u64,
}

#[cfg(feature = "cli")]
impl LineRange {
    /// Create a new line range.
    pub fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }

    /// Check if a line number is in this range.
    pub fn contains(&self, line: u64) -> bool {
        line >= self.start && line <= self.end
    }

    /// Get the number of lines in this range.
    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start) + 1
    }

    /// Check if the range is empty.
    pub fn is_empty(&self) -> bool {
        self.start > self.end
    }
}

/// Parse git diff output to extract line ranges.
#[cfg(feature = "cli")]
fn parse_diff_hunks(diff_output: &str) -> Vec<LineRange> {
    let mut ranges = Vec::new();

    for line in diff_output.lines() {
        if line.starts_with("@@") {
            // Parse hunk header: @@ -start,count +start,count @@
            if let Some(range) = parse_hunk_header(line) {
                ranges.push(range);
            }
        }
    }

    ranges
}

/// Parse a single hunk header.
#[cfg(feature = "cli")]
fn parse_hunk_header(header: &str) -> Option<LineRange> {
    // Format: @@ -old_start,old_count +new_start,new_count @@
    let parts: Vec<&str> = header.split_whitespace().collect();

    for part in parts {
        if let Some(stripped) = part.strip_prefix('+') {
            let nums: Vec<&str> = stripped.split(',').collect();
            if let Some(start_str) = nums.first() {
                if let Ok(start) = start_str.parse::<u64>() {
                    let count = nums.get(1).and_then(|s| s.parse::<u64>().ok()).unwrap_or(1);
                    return Some(LineRange::new(start, start + count.saturating_sub(1)));
                }
            }
        }
    }

    None
}

/// Incremental file reader that resumes from checkpoint.
#[cfg(feature = "cli")]
pub struct IncrementalReader {
    file: BufReader<File>,
    path: PathBuf,
    current_offset: u64,
    current_line: u64,
    line_ranges: Option<Vec<LineRange>>,
}

#[cfg(feature = "cli")]
impl IncrementalReader {
    /// Open a file for incremental reading from a checkpoint.
    pub fn open(path: impl AsRef<Path>, checkpoint: &Checkpoint) -> Result<Self> {
        let path = path.as_ref();
        let offset = checkpoint.get_file_offset(path);
        let line = checkpoint.get_line_number(path);

        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(offset))?;

        Ok(Self {
            file: BufReader::new(file),
            path: path.to_path_buf(),
            current_offset: offset,
            current_line: line,
            line_ranges: None,
        })
    }

    /// Open with specific line ranges to process (for git diff mode).
    pub fn open_with_ranges(path: impl AsRef<Path>, ranges: Vec<LineRange>) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path)?;

        Ok(Self {
            file: BufReader::new(file),
            path: path.to_path_buf(),
            current_offset: 0,
            current_line: 0,
            line_ranges: Some(ranges),
        })
    }

    /// Read next line, respecting line ranges if set.
    pub fn read_line(&mut self, buf: &mut String) -> Result<Option<u64>> {
        loop {
            buf.clear();
            let bytes_read = self.file.read_line(buf)?;

            if bytes_read == 0 {
                return Ok(None);
            }

            self.current_line += 1;
            self.current_offset += bytes_read as u64;

            // If we have line ranges, check if this line is in range
            if let Some(ref ranges) = self.line_ranges {
                let in_range = ranges.iter().any(|r| r.contains(self.current_line));
                if !in_range {
                    continue; // Skip this line
                }
            }

            return Ok(Some(self.current_line));
        }
    }

    /// Get current byte offset.
    pub fn offset(&self) -> u64 {
        self.current_offset
    }

    /// Get current line number.
    pub fn line_number(&self) -> u64 {
        self.current_line
    }

    /// Get file path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(all(test, feature = "cli"))]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_checkpoint_config_builder() {
        let config = CheckpointConfig::new()
            .enabled(true)
            .with_checkpoint_file("/tmp/checkpoint.json")
            .with_auto_save(true);

        assert!(config.enabled);
        assert_eq!(
            config.checkpoint_file,
            Some(PathBuf::from("/tmp/checkpoint.json"))
        );
        assert!(config.auto_save);
    }

    #[test]
    fn test_file_state() {
        let state = FileState::new("/path/to/file.txt");
        assert_eq!(state.byte_offset, 0);
        assert_eq!(state.line_number, 0);
    }

    #[test]
    fn test_checkpoint_file_tracking() {
        let config = CheckpointConfig::new().enabled(true);
        let mut checkpoint = Checkpoint::new(config);

        checkpoint.update_file_state("/path/to/file.txt", 1024, 50, 2048);

        assert_eq!(checkpoint.get_file_offset("/path/to/file.txt"), 1024);
        assert_eq!(checkpoint.get_line_number("/path/to/file.txt"), 50);
    }

    #[test]
    fn test_line_range() {
        let range = LineRange::new(10, 20);
        assert!(range.contains(10));
        assert!(range.contains(15));
        assert!(range.contains(20));
        assert!(!range.contains(9));
        assert!(!range.contains(21));
        assert_eq!(range.len(), 11);
    }

    #[test]
    fn test_parse_hunk_header() {
        let header = "@@ -10,5 +15,10 @@";
        let range = parse_hunk_header(header).unwrap();
        assert_eq!(range.start, 15);
        assert_eq!(range.end, 24);
    }

    #[test]
    fn test_checkpoint_save_load() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let path = temp_file.path().to_path_buf();

        // Create and save checkpoint
        let config = CheckpointConfig::new()
            .enabled(true)
            .with_checkpoint_file(&path);

        let mut checkpoint = Checkpoint::new(config.clone());
        checkpoint.update_file_state("/test/file.txt", 500, 25, 1000);
        checkpoint.set_metadata("test_key", "test_value");
        checkpoint.save()?;

        // Load checkpoint
        let loaded = Checkpoint::load(config, &path)?;
        assert_eq!(loaded.get_file_offset("/test/file.txt"), 500);
        assert_eq!(loaded.get_line_number("/test/file.txt"), 25);

        Ok(())
    }

    #[test]
    fn test_incremental_reader() -> Result<()> {
        // Create test file
        let mut temp_file = NamedTempFile::new()?;
        writeln!(temp_file, "line 1")?;
        writeln!(temp_file, "line 2")?;
        writeln!(temp_file, "line 3")?;
        temp_file.flush()?;

        let config = CheckpointConfig::new().enabled(true);
        let checkpoint = Checkpoint::new(config);

        let mut reader = IncrementalReader::open(temp_file.path(), &checkpoint)?;
        let mut buf = String::new();

        let line_num = reader.read_line(&mut buf)?;
        assert_eq!(line_num, Some(1));
        assert_eq!(buf.trim(), "line 1");

        let line_num = reader.read_line(&mut buf)?;
        assert_eq!(line_num, Some(2));
        assert_eq!(buf.trim(), "line 2");

        Ok(())
    }
}
