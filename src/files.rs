//! Multi-file processing module for rexpipe
//!
//! Provides directory recursion, in-place editing, parallel processing,
//! and VCS-aware file discovery.

use crate::pipeline::PipelineConfig;
use crate::processor::StreamProcessor;
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
    pub total_matches: u64,
    pub total_lines: u64,
    pub file_results: Vec<FileResult>,
    pub errors: Vec<String>,
}

/// Options for multi-file processing
#[derive(Debug, Clone)]
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
    pub fn discover_files(&self, paths: &[PathBuf]) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
        let mut files = Vec::new();

        for path in paths {
            if path.is_file() {
                files.push(path.clone());
            } else if path.is_dir() {
                let discovered = self.walk_directory(path)?;
                files.extend(discovered);
            }
        }

        Ok(files)
    }

    fn walk_directory(&self, dir: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
        let mut builder = WalkBuilder::new(dir);

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

        for entry in builder.build() {
            let entry = entry?;
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

        Ok(files)
    }

    /// Process multiple files
    pub fn process_files(&self, files: &[PathBuf]) -> Result<MultiFileResult, Box<dyn std::error::Error>> {
        if self.options.parallel {
            self.process_files_parallel(files)
        } else {
            self.process_files_sequential(files)
        }
    }

    fn process_files_sequential(&self, files: &[PathBuf]) -> Result<MultiFileResult, Box<dyn std::error::Error>> {
        let mut result = MultiFileResult::default();

        for file in files {
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
        }

        Ok(result)
    }

    fn process_files_parallel(&self, files: &[PathBuf]) -> Result<MultiFileResult, Box<dyn std::error::Error>> {
        let files_processed = AtomicU64::new(0);
        let files_matched = AtomicU64::new(0);
        let files_modified = AtomicU64::new(0);
        let total_matches = AtomicU64::new(0);
        let total_lines = AtomicU64::new(0);

        let file_results: Vec<FileResult> = files
            .par_iter()
            .map(|file| {
                match self.process_single_file(file) {
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
                }
            })
            .collect();

        let errors: Vec<String> = file_results
            .iter()
            .filter_map(|r| r.error.as_ref().map(|e| format!("{}: {}", r.path.display(), e)))
            .collect();

        Ok(MultiFileResult {
            files_processed: files_processed.load(Ordering::Relaxed),
            files_matched: files_matched.load(Ordering::Relaxed),
            files_modified: files_modified.load(Ordering::Relaxed),
            total_matches: total_matches.load(Ordering::Relaxed),
            total_lines: total_lines.load(Ordering::Relaxed),
            file_results,
            errors,
        })
    }

    fn process_single_file(&self, path: &Path) -> Result<FileResult, Box<dyn std::error::Error>> {
        let mut processor = StreamProcessor::new(self.config.clone())?;

        // Read the file
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        if self.options.in_place {
            // Process to a temporary buffer, then write back
            let mut output = Vec::new();
            let pipeline_result = processor.process_stream(reader, &mut output)?;

            // Create backup if requested
            if let Some(ref suffix) = self.options.backup_suffix {
                let backup_path = format!("{}{}", path.display(), suffix);
                fs::copy(path, &backup_path)?;
            }

            // Write the processed content back
            fs::write(path, output)?;

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
    pub fn count_matches(&self, files: &[PathBuf]) -> Result<MultiFileResult, Box<dyn std::error::Error>> {
        let processor_fn = |path: &Path| -> Result<(u64, u64), Box<dyn std::error::Error>> {
            let mut processor = StreamProcessor::new(self.config.clone())?;
            let file = File::open(path)?;
            let reader = BufReader::new(file);
            let mut output = std::io::sink();
            let result = processor.process_stream(reader, &mut output)?;
            Ok((result.matches_found, result.lines_processed))
        };

        let results: Vec<FileResult> = if self.options.parallel {
            files
                .par_iter()
                .map(|path| {
                    match processor_fn(path) {
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
                    }
                })
                .collect()
        } else {
            files
                .iter()
                .map(|path| {
                    match processor_fn(path) {
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
                    }
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
                result.errors.push(format!("{}: {}", file_result.path.display(), e));
            }
            result.file_results.push(file_result);
        }

        Ok(result)
    }

    /// List files with matches
    pub fn files_with_matches(&self, files: &[PathBuf]) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
        let result = self.count_matches(files)?;
        Ok(result
            .file_results
            .into_iter()
            .filter(|r| r.matches_found > 0 && r.error.is_none())
            .map(|r| r.path)
            .collect())
    }

    /// List files without matches
    pub fn files_without_matches(&self, files: &[PathBuf]) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
        let result = self.count_matches(files)?;
        Ok(result
            .file_results
            .into_iter()
            .filter(|r| r.matches_found == 0 && r.error.is_none())
            .map(|r| r.path)
            .collect())
    }
}

impl MultiFileResult {
    pub fn summary(&self) -> String {
        format!(
            "Files: {} processed, {} matched, {} modified\n\
             Lines: {} total\n\
             Matches: {} total\n\
             Errors: {}",
            self.files_processed,
            self.files_matched,
            self.files_modified,
            self.total_lines,
            self.total_matches,
            self.errors.len()
        )
    }

    pub fn has_matches(&self) -> bool {
        self.total_matches > 0
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

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

        let discovered = processor.discover_files(&[temp_dir.path().to_path_buf()]).unwrap();
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
        let result = processor.process_files(&[file_path.clone()]).unwrap();

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
        let options = FileProcessingOptions::default()
            .include_pattern("*.txt".to_string());

        let processor = MultiFileProcessor::new(config, options);
        let discovered = processor.discover_files(&[temp_dir.path().to_path_buf()]).unwrap();

        assert_eq!(discovered.len(), 2); // Only .txt files
    }
}
