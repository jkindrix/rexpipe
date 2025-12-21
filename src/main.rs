use anyhow::{Error as AnyhowError, Result, anyhow};
use clap::{Arg, ArgAction, Command, ValueHint, value_parser};
use clap_complete::{Generator, Shell, generate};
use log::{debug, info};
use std::fs::File;
use std::io::{self, BufReader, IsTerminal};
use std::path::{Path, PathBuf};

// Import from the library crate
use rexpipe::compass::CompassAgent;
use rexpipe::error::{ConfigError, LibraryError, PatternError, RexpipeError, ValidationError};
use rexpipe::files::{FileProcessingOptions, MultiFileProcessor, MultiFileResult};
use rexpipe::inspector::{Inspector, InspectorOptions};
use rexpipe::json_schema;
use rexpipe::library;
use rexpipe::library::LibraryResolver;
use rexpipe::pipeline::{PipelineConfig, PipelineSettings};
use rexpipe::processor::StreamProcessor;

/// Exit codes for different error conditions
mod exit_codes {
    /// Success - operation completed normally (implicit, not explicitly used).
    /// Defined for completeness but Rust's main() returns 0 implicitly on success.
    #[allow(dead_code)]
    pub const SUCCESS: i32 = 0;
    /// No matches found (used with -q/--quiet mode)
    pub const NO_MATCHES: i32 = 1;
    /// General error (unspecified)
    pub const GENERAL_ERROR: i32 = 1;
    /// Invalid command line usage or missing arguments
    pub const USAGE_ERROR: i32 = 2;
    /// Configuration file error (not found, invalid TOML, etc.)
    pub const CONFIG_ERROR: i32 = 3;
    /// Invalid regex pattern
    pub const PATTERN_ERROR: i32 = 4;
    /// File I/O error (file not found, permission denied, etc.)
    pub const IO_ERROR: i32 = 5;
    /// Validation error (pipeline configuration validation failed)
    pub const VALIDATION_ERROR: i32 = 6;
}

/// Determine if colored output should be used.
/// Respects the NO_COLOR environment variable (https://no-color.org/)
/// and the --no-color CLI flag.
fn should_use_color(matches: &clap::ArgMatches) -> bool {
    // --no-color flag explicitly disables color
    if matches.get_flag("no-color") {
        return false;
    }

    // NO_COLOR env var (any non-empty value disables color)
    if std::env::var("NO_COLOR").map(|v| !v.is_empty()).unwrap_or(false) {
        return false;
    }

    // Otherwise, use color if stdout is a terminal
    std::io::stdout().is_terminal()
}

/// Categorize error type using structured error types for exit code selection.
/// Falls back to string matching for errors that weren't wrapped in our types.
fn categorize_error(error: &AnyhowError) -> i32 {
    // Try to downcast to our structured error types first
    if let Some(rexpipe_err) = error.downcast_ref::<RexpipeError>() {
        return match rexpipe_err {
            RexpipeError::Config(_) => exit_codes::CONFIG_ERROR,
            RexpipeError::Pattern(_) => exit_codes::PATTERN_ERROR,
            RexpipeError::Io(_) => exit_codes::IO_ERROR,
            RexpipeError::Library(_) => exit_codes::CONFIG_ERROR, // Library errors are config-related
            RexpipeError::Validation(_) => exit_codes::VALIDATION_ERROR,
            RexpipeError::Processing(_) => exit_codes::GENERAL_ERROR,
        };
    }

    // Check for specific nested error types
    if error.downcast_ref::<ConfigError>().is_some() {
        return exit_codes::CONFIG_ERROR;
    }
    if error.downcast_ref::<PatternError>().is_some() {
        return exit_codes::PATTERN_ERROR;
    }
    if error.downcast_ref::<LibraryError>().is_some() {
        return exit_codes::CONFIG_ERROR;
    }
    if error.downcast_ref::<ValidationError>().is_some() {
        return exit_codes::VALIDATION_ERROR;
    }
    if error.downcast_ref::<std::io::Error>().is_some() {
        return exit_codes::IO_ERROR;
    }

    // Fallback to string matching for errors from external crates
    let error_msg = error.to_string().to_lowercase();

    if error_msg.contains("missing required")
        || error_msg.contains("must specify")
        || error_msg.contains("invalid argument")
    {
        exit_codes::USAGE_ERROR
    } else if error_msg.contains("no such file")
        || error_msg.contains("not found")
        || error_msg.contains("permission denied")
    {
        exit_codes::IO_ERROR
    } else if error_msg.contains("invalid regex") || error_msg.contains("regex parse error") {
        exit_codes::PATTERN_ERROR
    } else if error_msg.contains("toml") || error_msg.contains("parse error") {
        exit_codes::CONFIG_ERROR
    } else if error_msg.contains("validation") {
        exit_codes::VALIDATION_ERROR
    } else {
        exit_codes::GENERAL_ERROR
    }
}

/// Build the CLI command structure
/// Separated for use with clap_complete shell completion generation
fn build_cli() -> Command {
    Command::new("rexpipe")
        .version("1.1.0")
        .author("Strategic Collaboration Agent")
        .about("Unified regex pipeline processor with COMPASS framework integration")
        // === Pattern and Config ===
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("TOML configuration file")
                .value_hint(ValueHint::FilePath),
        )
        .arg(
            Arg::new("pattern")
                .short('p')
                .long("pattern")
                .value_name("REGEX")
                .help("Inline regex pattern"),
        )
        .arg(
            Arg::new("replacement")
                .short('r')
                .long("replacement")
                .value_name("TEXT")
                .help("Replacement text for substitution"),
        )
        // === Regex Engine Options ===
        .arg(
            Arg::new("fixed")
                .short('F')
                .long("fixed")
                .help("Treat pattern as fixed string (no regex interpretation)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("pcre")
                .short('P')
                .long("pcre")
                .help("Use PCRE-compatible regex via fancy-regex (supports lookahead/lookbehind)")
                .action(ArgAction::SetTrue),
        )
        // === File Operations ===
        .arg(
            Arg::new("in-place")
                .short('i')
                .long("in-place")
                .help("Edit files in-place (like sed -i)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("backup")
                .short('b')
                .long("backup")
                .value_name("SUFFIX")
                .help("Create backup with given suffix when editing in-place (e.g., .bak)"),
        )
        .arg(
            Arg::new("recursive")
                .short('R')
                .long("recursive")
                .help("Recursively process directories")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("glob")
                .short('g')
                .long("glob")
                .value_name("PATTERN")
                .help("Only process files matching glob pattern (e.g., '*.txt')")
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new("exclude")
                .short('e')
                .long("exclude")
                .value_name("PATTERN")
                .help("Exclude files matching glob pattern")
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new("no-ignore")
                .long("no-ignore")
                .help("Don't respect .gitignore files")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("hidden")
                .long("hidden")
                .help("Include hidden files")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("max-depth")
                .long("max-depth")
                .value_name("NUM")
                .help("Maximum directory recursion depth"),
        )
        // === Processing Modes ===
        .arg(
            Arg::new("parallel")
                .short('j')
                .long("parallel")
                .help("Process files in parallel")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("timeout")
                .long("timeout")
                .value_name("MS")
                .help("Timeout in milliseconds per line (0 = no timeout)")
                .value_parser(value_parser!(u64)),
        )
        .arg(
            Arg::new("async")
                .long("async")
                .help("Use async I/O for file processing (requires async feature)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("progress")
                .long("progress")
                .help("Show progress indicator for multi-file processing")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("inspect")
                .long("inspect")
                .help("Enable inspection mode")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("interactive")
                .long("interactive")
                .help("Enable interactive inspection")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .help("Preview changes without modifying (works with stdin or -i mode)")
                .action(ArgAction::SetTrue),
        )
        // === Output Modes ===
        .arg(
            Arg::new("count")
                .long("count")
                .help("Only show count of matches per file")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("files-with-matches")
                .short('l')
                .long("files-with-matches")
                .help("Only list files containing matches")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("files-without-matches")
                .short('L')
                .long("files-without-matches")
                .help("Only list files not containing matches")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .help("Quiet mode - only set exit code")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output results as JSON")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("jsonl")
                .long("jsonl")
                .help("Output results as streaming JSON Lines (one JSON object per line)")
                .action(ArgAction::SetTrue)
                .conflicts_with("json"),
        )
        .arg(
            Arg::new("no-color")
                .long("no-color")
                .help("Disable colored output (also respects NO_COLOR env var)")
                .action(ArgAction::SetTrue),
        )
        // === Context Lines (for inspection) ===
        .arg(
            Arg::new("context-before")
                .short('B')
                .long("before-context")
                .value_name("NUM")
                .help("Show NUM lines before each match"),
        )
        .arg(
            Arg::new("context-after")
                .short('A')
                .long("after-context")
                .value_name("NUM")
                .help("Show NUM lines after each match"),
        )
        .arg(
            Arg::new("context")
                .short('C')
                .long("context")
                .value_name("NUM")
                .help("Show NUM lines before and after each match"),
        )
        // === Pattern Library ===
        .arg(
            Arg::new("list-patterns")
                .long("list-patterns")
                .value_name("LIBRARY")
                .help("List all patterns in a pattern library file")
                .value_hint(ValueHint::FilePath),
        )
        .arg(
            Arg::new("validate-library")
                .long("validate-library")
                .value_name("LIBRARY")
                .help("Validate a pattern library file")
                .value_hint(ValueHint::FilePath),
        )
        // === Misc ===
        .arg(
            Arg::new("performance")
                .long("performance")
                .help("Show performance metrics")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("compass")
                .long("compass")
                .help("Run COMPASS strategic analysis")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("validate")
                .long("validate")
                .help("Validate configuration only")
                .action(ArgAction::SetTrue),
        )
        // === Security ===
        .arg(
            Arg::new("no-shell")
                .long("no-shell")
                .help("Disable shell command execution in transforms (security)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("strict")
                .long("strict")
                .help("Reject potentially dangerous ReDoS regex patterns")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("export")
                .long("export")
                .value_name("FORMAT")
                .help("Export configuration (toml or json)"),
        )
        .arg(
            Arg::new("completions")
                .long("completions")
                .value_name("SHELL")
                .help("Generate shell completion script")
                .value_parser(value_parser!(Shell)),
        )
        // === I/O ===
        .arg(
            Arg::new("input")
                .short('f')
                .long("input")
                .value_name("FILE")
                .help("Input file (default: stdin)")
                .value_hint(ValueHint::FilePath),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("FILE")
                .help("Output file (default: stdout)")
                .value_hint(ValueHint::FilePath),
        )
        // === Positional Args ===
        .arg(
            Arg::new("paths")
                .help("Files or directories to process")
                .action(ArgAction::Append)
                .num_args(0..)
                .value_hint(ValueHint::AnyPath),
        )
}

/// Generate shell completion script for the given shell
fn print_completions<G: Generator>(generator: G, cmd: &mut Command) {
    generate(
        generator,
        cmd,
        cmd.get_name().to_string(),
        &mut io::stdout(),
    );
}

fn main() {
    // Initialize logger from RUST_LOG environment variable
    // Example: RUST_LOG=rexpipe=debug rexpipe --config my.toml < input.txt
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .format_timestamp(None)
        .init();

    debug!("Starting rexpipe");

    let matches = build_cli().get_matches();

    // Handle completions generation first (before any other processing)
    if let Some(shell) = matches.get_one::<Shell>("completions").copied() {
        let mut cmd = build_cli();
        print_completions(shell, &mut cmd);
        return;
    }

    if let Err(e) = run_application(&matches) {
        eprintln!("Error: {}", e);
        let exit_code = categorize_error(&e);
        std::process::exit(exit_code);
    }
}

fn run_application(matches: &clap::ArgMatches) -> Result<()> {
    // Handle pattern library commands first (don't require pipeline config)
    if let Some(library_path) = matches.get_one::<String>("list-patterns") {
        return list_library_patterns(library_path);
    }

    if let Some(library_path) = matches.get_one::<String>("validate-library") {
        return validate_library_file(library_path);
    }

    // Build pipeline settings from CLI flags
    let settings = build_pipeline_settings(matches);

    // Handle COMPASS mode - can optionally analyze a config
    if matches.get_flag("compass") {
        // Try to load config if provided
        if matches.contains_id("config") && matches.get_one::<String>("config").is_some() {
            let config = load_pipeline_config(matches, settings)?;
            return run_compass_analysis_for_pipeline(&config);
        }
        return run_compass_analysis();
    }

    // Load or create pipeline configuration
    let config = load_pipeline_config(matches, settings)?;

    // Handle export mode
    if let Some(format) = matches.get_one::<String>("export") {
        return export_configuration(&config, format);
    }

    // Validate configuration if requested (unless we have files to preview)
    if matches.get_flag("validate") {
        return validate_configuration(&config);
    }

    // Check if we're in multi-file mode
    let paths: Vec<PathBuf> = matches
        .get_many::<String>("paths")
        .map(|v| v.map(PathBuf::from).collect())
        .unwrap_or_default();

    let is_multi_file =
        matches.get_flag("recursive") || matches.get_flag("in-place") || !paths.is_empty();

    // Handle dry-run: show preview for in-place mode, stdin preview, or just validate
    if matches.get_flag("dry-run") {
        if is_multi_file && matches.get_flag("in-place") {
            return run_dry_run_preview(&config, matches, paths);
        }
        if !is_multi_file {
            // Stdin mode dry-run: show what would change
            let input: Box<dyn io::BufRead> =
                if let Some(input_file) = matches.get_one::<String>("input") {
                    Box::new(BufReader::new(File::open(input_file)?))
                } else {
                    Box::new(io::stdin().lock())
                };
            return run_stdin_dry_run_preview(&config, input, matches);
        }
        return validate_configuration(&config);
    }

    if is_multi_file {
        return run_multi_file_mode(&config, matches, paths);
    }

    // Single file/stdin mode
    let input: Box<dyn io::BufRead> = if let Some(input_file) = matches.get_one::<String>("input") {
        Box::new(BufReader::new(File::open(input_file)?))
    } else {
        Box::new(io::stdin().lock())
    };

    // Handle inspection mode
    if matches.get_flag("inspect") {
        return run_inspection_mode(&config, input, matches);
    }

    // Run standard processing mode
    run_processing_mode(&config, input, matches)
}

fn build_pipeline_settings(matches: &clap::ArgMatches) -> PipelineSettings {
    let context = matches
        .get_one::<String>("context")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let context_before = matches
        .get_one::<String>("context-before")
        .and_then(|s| s.parse().ok())
        .unwrap_or(context);

    let context_after = matches
        .get_one::<String>("context-after")
        .and_then(|s| s.parse().ok())
        .unwrap_or(context);

    let timeout_ms = matches.get_one::<u64>("timeout").copied().unwrap_or(0);

    PipelineSettings {
        pcre_mode: matches.get_flag("pcre"),
        fixed_strings: matches.get_flag("fixed"),
        context_before,
        context_after,
        timeout_ms,
        // --no-shell disables shell transforms
        allow_shell: !matches.get_flag("no-shell"),
        // --strict enables ReDoS pattern rejection
        strict_mode: matches.get_flag("strict"),
    }
}

fn export_configuration(config: &PipelineConfig, format: &str) -> Result<()> {
    let output = match format.to_lowercase().as_str() {
        "toml" => config.to_toml()?,
        "json" => config.to_json()?,
        _ => {
            return Err(anyhow!(
                "Unknown export format: {}. Use 'toml' or 'json'",
                format
            ));
        }
    };
    println!("{}", output);
    Ok(())
}

fn run_multi_file_mode(
    config: &PipelineConfig,
    matches: &clap::ArgMatches,
    paths: Vec<PathBuf>,
) -> Result<()> {
    let quiet = matches.get_flag("quiet");
    let json_output = matches.get_flag("json");
    let jsonl_output = matches.get_flag("jsonl");

    debug!(
        "Entering multi-file mode with {} paths",
        if paths.is_empty() { 1 } else { paths.len() }
    );

    // Set up graceful shutdown handling
    let shutdown_signal = rexpipe::ShutdownSignal::new();
    if let Err(e) = shutdown_signal.install_handlers() {
        if !quiet {
            eprintln!("Warning: Could not install signal handlers: {}", e);
        }
    }

    // Build file processing options
    let mut options = FileProcessingOptions::new()
        .in_place(matches.get_flag("in-place"))
        .backup_suffix(matches.get_one::<String>("backup").cloned())
        .respect_gitignore(!matches.get_flag("no-ignore"))
        .include_hidden(matches.get_flag("hidden"))
        .parallel(matches.get_flag("parallel"))
        .count_only(matches.get_flag("count"))
        .files_with_matches(matches.get_flag("files-with-matches"))
        .files_without_matches(matches.get_flag("files-without-matches"))
        .quiet(quiet)
        .show_progress(matches.get_flag("progress"))
        .shutdown_signal(shutdown_signal);

    // Add max depth
    if let Some(depth) = matches.get_one::<String>("max-depth") {
        options = options.max_depth(Some(depth.parse()?));
    }

    // Add glob patterns
    if let Some(globs) = matches.get_many::<String>("glob") {
        for glob in globs {
            options = options.include_pattern(glob.clone());
        }
    }

    // Add exclude patterns
    if let Some(excludes) = matches.get_many::<String>("exclude") {
        for exclude in excludes {
            options = options.exclude_pattern(exclude.clone());
        }
    }

    let processor = MultiFileProcessor::new(config.clone(), options.clone());

    // Determine paths to process
    let paths_to_process = if paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        paths
    };

    // Discover files
    let files = processor.discover_files(&paths_to_process)?;

    info!("Discovered {} files to process", files.len());

    if files.is_empty() {
        if !quiet {
            eprintln!("Warning: No files found matching criteria");
            if !options.include_patterns.is_empty() {
                eprintln!(
                    "  Glob patterns specified: {}",
                    options.include_patterns.join(", ")
                );
                eprintln!("  Hint: Check that your glob patterns match existing files");
            }
            if !options.exclude_patterns.is_empty() {
                eprintln!(
                    "  Exclude patterns: {}",
                    options.exclude_patterns.join(", ")
                );
            }
        }
        return Ok(());
    }

    // Check if async mode is requested
    let use_async = matches.get_flag("async");

    // Warn if async flag is used without async feature
    #[cfg(not(feature = "async"))]
    if use_async {
        eprintln!("Warning: --async flag requires the 'async' feature.");
        eprintln!("Rebuild with: cargo build --features async");
        eprintln!("Falling back to synchronous processing.");
    }

    // Process based on mode
    let result = if options.files_with_matches {
        let matching = processor.files_with_matches(&files)?;
        output_file_list(&matching, quiet, json_output, "files_with_matches")?;
        return Ok(());
    } else if options.files_without_matches {
        let non_matching = processor.files_without_matches(&files)?;
        output_file_list(&non_matching, quiet, json_output, "files_without_matches")?;
        return Ok(());
    } else if options.count_only {
        #[cfg(feature = "async")]
        if use_async {
            let rt = tokio::runtime::Runtime::new()?;
            let result = rt
                .block_on(
                    rexpipe::files::AsyncMultiFileProcessor::new(config.clone(), options.clone())
                        .count_matches_async(&files),
                )
                .map_err(|e| anyhow!(e))?;
            output_count_results(&result, quiet, json_output)?;
            return Ok(());
        }
        let result = processor.count_matches(&files)?;
        output_count_results(&result, quiet, json_output)?;
        return Ok(());
    } else if jsonl_output {
        // JSONL streaming mode: output each file result as it's processed
        let result = processor.process_files_streaming(&files, |file_result| {
            if let Ok(jsonl) = json_schema::output_file_result_jsonl(file_result) {
                println!("{}", jsonl);
            }
        })?;

        // Output summary as final JSONL line
        if let Ok(summary) = json_schema::output_streaming_summary_jsonl(&result) {
            println!("{}", summary);
        }

        if !result.has_matches() {
            std::process::exit(exit_codes::NO_MATCHES);
        }
        return Ok(());
    } else {
        #[cfg(feature = "async")]
        if use_async {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(
                rexpipe::files::AsyncMultiFileProcessor::new(config.clone(), options.clone())
                    .process_files_async(&files),
            )
            .map_err(|e| anyhow!(e))?
        } else {
            processor.process_files(&files)?
        }
        #[cfg(not(feature = "async"))]
        processor.process_files(&files)?
    };

    // Output results
    if !quiet {
        if json_output {
            output_multi_file_json(&result)?;
        } else {
            output_multi_file_summary(&result)?;
        }
    }

    // Set exit code based on matches (exit 1 = no matches, for grep-like behavior)
    if !result.has_matches() {
        std::process::exit(exit_codes::NO_MATCHES);
    }

    Ok(())
}

fn run_dry_run_preview(
    config: &PipelineConfig,
    matches: &clap::ArgMatches,
    paths: Vec<PathBuf>,
) -> Result<()> {
    // Build file processing options (same as run_multi_file_mode but without in_place)
    let mut options = FileProcessingOptions::new()
        .respect_gitignore(!matches.get_flag("no-ignore"))
        .include_hidden(matches.get_flag("hidden"));

    // Add max depth
    if let Some(depth) = matches.get_one::<String>("max-depth") {
        options = options.max_depth(Some(depth.parse()?));
    }

    // Add glob patterns
    if let Some(globs) = matches.get_many::<String>("glob") {
        for glob in globs {
            options = options.include_pattern(glob.clone());
        }
    }

    // Add exclude patterns
    if let Some(excludes) = matches.get_many::<String>("exclude") {
        for exclude in excludes {
            options = options.exclude_pattern(exclude.clone());
        }
    }

    // Save patterns for warning message before moving options
    let include_patterns = options.include_patterns.clone();
    let exclude_patterns = options.exclude_patterns.clone();

    let processor = MultiFileProcessor::new(config.clone(), options);

    // Determine paths to process
    let paths_to_process = if paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        paths
    };

    // Discover files
    let files = processor.discover_files(&paths_to_process)?;

    if files.is_empty() {
        eprintln!("Warning: No files found matching criteria");
        if !include_patterns.is_empty() {
            eprintln!(
                "  Glob patterns specified: {}",
                include_patterns.join(", ")
            );
            eprintln!("  Hint: Check that your glob patterns match existing files");
        }
        if !exclude_patterns.is_empty() {
            eprintln!("  Exclude patterns: {}", exclude_patterns.join(", "));
        }
        return Ok(());
    }

    // Determine if we should use color (respects --no-color and NO_COLOR env)
    let use_color = should_use_color(matches);

    // Generate preview
    let preview = processor.preview_changes(&files, use_color)?;
    print!("{}", preview);

    Ok(())
}

/// Show a preview of what would change when processing stdin.
/// Displays a diff-like view showing original lines vs transformed lines.
fn run_stdin_dry_run_preview(
    config: &PipelineConfig,
    input: Box<dyn io::BufRead>,
    matches: &clap::ArgMatches,
) -> Result<()> {
    use std::io::BufRead;

    let use_color = should_use_color(matches);
    let mut processor = StreamProcessor::new(config.clone())?;

    // ANSI color codes
    let (red, green, reset) = if use_color {
        ("\x1b[31m", "\x1b[32m", "\x1b[0m")
    } else {
        ("", "", "")
    };

    println!("Dry-run preview (no changes will be made):");
    println!("---");

    let mut total_lines = 0u64;
    let mut changed_lines = 0u64;

    for (line_num, line_result) in input.lines().enumerate() {
        let original = line_result?;
        total_lines += 1;

        // Process the line through the pipeline
        let mut output_buffer = Vec::new();
        {
            let single_line = std::io::Cursor::new(format!("{}\n", original));
            processor.process_stream(single_line, &mut output_buffer)?;
        }

        let transformed = String::from_utf8_lossy(&output_buffer)
            .trim_end_matches('\n')
            .to_string();

        if original != transformed {
            changed_lines += 1;
            println!("{}{}: - {}{}", red, line_num + 1, original, reset);
            println!("{}{}: + {}{}", green, line_num + 1, transformed, reset);
        }
    }

    println!("---");
    println!(
        "Summary: {} of {} lines would change",
        changed_lines, total_lines
    );

    Ok(())
}

fn output_file_list(files: &[PathBuf], quiet: bool, json: bool, mode: &str) -> Result<()> {
    if quiet {
        return Ok(());
    }

    if json {
        println!("{}", json_schema::output_file_list_json(files, mode)?);
    } else {
        for file in files {
            println!("{}", file.display());
        }
    }
    Ok(())
}

fn output_count_results(result: &MultiFileResult, quiet: bool, json: bool) -> Result<()> {
    if quiet {
        return Ok(());
    }

    if json {
        println!("{}", json_schema::output_multi_file_json(result)?);
    } else {
        for file_result in &result.file_results {
            println!(
                "{}:{}",
                file_result.path.display(),
                file_result.matches_found
            );
        }
        println!("---");
        println!(
            "Total: {} matches in {} files",
            result.total_matches, result.files_matched
        );
    }
    Ok(())
}

fn output_multi_file_json(result: &MultiFileResult) -> Result<()> {
    println!("{}", json_schema::output_multi_file_json(result)?);
    Ok(())
}

fn output_multi_file_summary(result: &MultiFileResult) -> Result<()> {
    println!("{}", result.summary());
    if !result.errors.is_empty() {
        eprintln!("\nErrors:");
        for error in &result.errors {
            eprintln!("  {}", error);
        }
    }
    Ok(())
}

fn load_pipeline_config(
    matches: &clap::ArgMatches,
    settings: PipelineSettings,
) -> Result<PipelineConfig> {
    debug!("Loading pipeline configuration");

    if let Some(config_file) = matches.get_one::<String>("config") {
        info!("Loading config from file: {}", config_file);
        let config_path = Path::new(config_file);
        let mut config = PipelineConfig::from_file(config_file)?;

        // Load and resolve pattern libraries if specified
        if config.uses_pattern_libraries() {
            let mut resolver = LibraryResolver::new(config_path.parent());
            let library = resolver.load_libraries(&config.patterns_include)?;

            if let Err(errors) = config.resolve_pattern_references(&library) {
                return Err(anyhow!(
                    "Failed to resolve pattern references:\n  {}",
                    errors.join("\n  ")
                ));
            }
        }

        // Merge CLI settings with config file settings (CLI takes precedence)
        if settings.pcre_mode {
            config.settings.pcre_mode = true;
        }
        if settings.fixed_strings {
            config.settings.fixed_strings = true;
        }
        if settings.context_before > 0 {
            config.settings.context_before = settings.context_before;
        }
        if settings.context_after > 0 {
            config.settings.context_after = settings.context_after;
        }
        if settings.timeout_ms > 0 {
            config.settings.timeout_ms = settings.timeout_ms;
        }
        // --no-shell flag disables shell transforms
        if !settings.allow_shell {
            config.settings.allow_shell = false;
        }

        // --strict flag enables ReDoS pattern rejection
        if settings.strict_mode {
            config.settings.strict_mode = true;
        }

        // Validate shell transform usage if disabled
        if !config.settings.allow_shell && config.has_shell_transforms() {
            let shell_commands = config.get_shell_commands();
            return Err(anyhow!(
                "Shell transforms are disabled (--no-shell flag), but config contains shell transforms:\n  {}",
                shell_commands.join("\n  ")
            ));
        }

        // Warn about shell transforms when not disabled
        if config.has_shell_transforms() {
            let shell_commands = config.get_shell_commands();
            eprintln!(
                "Warning: This pipeline uses shell transforms that will execute external commands:\n  {}\n\
                Use --no-shell to disable shell command execution.",
                shell_commands.join("\n  ")
            );
        }

        Ok(config)
    } else if let Some(pattern) = matches.get_one::<String>("pattern") {
        debug!("Using inline pattern: {}", pattern);
        let replacement = matches.get_one::<String>("replacement").map(|s| s.as_str());
        Ok(PipelineConfig::from_inline_pattern_with_settings(
            pattern,
            replacement,
            settings,
        ))
    } else {
        Err(anyhow!(
            "Missing required input.\n\n\
             You must specify either:\n  \
             - A config file:  rexpipe --config pipeline.toml < input.txt\n  \
             - An inline pattern:  rexpipe --pattern '\\d+' < input.txt\n\n\
             Examples:\n  \
             rexpipe -p 'ERROR' < log.txt              # Match lines with ERROR\n  \
             rexpipe -p '\\d+' -r 'NUM' < data.txt     # Replace numbers with NUM\n  \
             rexpipe -c config.toml --inspect < test   # Preview matches\n\n\
             Run 'rexpipe --help' for full usage information."
        ))
    }
}

fn validate_configuration(config: &PipelineConfig) -> Result<()> {
    match config.validate() {
        Ok(()) => {
            println!("✓ Configuration is valid");
            println!("{}", config.summary());

            // Test compilation
            match StreamProcessor::new(config.clone()) {
                Ok(_) => {
                    println!("✓ All patterns compile successfully");
                    Ok(())
                }
                Err(e) => {
                    println!("✗ Pattern compilation failed:");
                    println!("{}", e);
                    Err(e)
                }
            }
        }
        Err(errors) => {
            println!("✗ Configuration validation failed:\n");
            for (i, error) in errors.iter().enumerate() {
                println!("  {}. {}", i + 1, error);
            }
            println!("\nSuggestions:");
            println!("  - Check that all substitute steps have 'replacement' defined");
            println!(
                "  - Check that all filter steps have 'action' defined (keep_line, drop_line, etc.)"
            );
            println!("  - Verify all patterns are valid regex syntax");
            println!("  - Use 'rexpipe --inspect' to test patterns interactively");
            Err(anyhow!("Configuration is invalid"))
        }
    }
}

fn list_library_patterns(library_path: &str) -> Result<()> {
    let path = Path::new(library_path);
    let patterns = library::list_patterns(path)?;

    if patterns.is_empty() {
        println!("No patterns found in library '{}'", library_path);
        return Ok(());
    }

    println!(
        "Patterns in '{}' ({} total):\n",
        library_path,
        patterns.len()
    );

    // Group by category (prefix before last dot)
    let mut current_category = String::new();
    for (name, pattern) in &patterns {
        let category = if let Some(pos) = name.rfind('.') {
            &name[..pos]
        } else {
            ""
        };

        if category != current_category {
            if !current_category.is_empty() {
                println!();
            }
            if !category.is_empty() {
                println!("[{}]", category);
            }
            current_category = category.to_string();
        }

        // Truncate long patterns for display
        let display_pattern = if pattern.len() > 60 {
            format!("{}...", &pattern[..57])
        } else {
            pattern.clone()
        };

        println!("  {} = '{}'", name, display_pattern);
    }

    Ok(())
}

fn validate_library_file(library_path: &str) -> Result<()> {
    let path = Path::new(library_path);

    match library::LibraryResolver::validate_library(path) {
        Ok(lib) => {
            println!("✓ Library '{}' is valid", library_path);
            if let Some(name) = &lib.name {
                println!("  Name: {}", name);
            }
            if let Some(version) = &lib.version {
                println!("  Version: {}", version);
            }
            if let Some(desc) = &lib.description {
                println!("  Description: {}", desc);
            }

            // Count patterns
            let patterns = library::list_patterns(path)?;
            println!("  Patterns: {}", patterns.len());

            if !lib.patterns_include.is_empty() {
                println!("  Includes: {}", lib.patterns_include.join(", "));
            }

            Ok(())
        }
        Err(e) => {
            println!("✗ Library '{}' is invalid:", library_path);
            println!("  {}", e);
            Err(e)
        }
    }
}

fn run_compass_analysis() -> Result<()> {
    println!("Initializing COMPASS Strategic Collaboration Agent...\n");

    let mut agent = CompassAgent::new();
    run_compass_phases(&mut agent)
}

fn run_compass_analysis_for_pipeline(config: &PipelineConfig) -> Result<()> {
    println!("Initializing COMPASS Strategic Collaboration Agent for Pipeline Analysis...\n");

    let mut agent = CompassAgent::for_pipeline(config);
    println!(
        "Analyzing: {}\n",
        config.name.as_deref().unwrap_or("Unnamed Pipeline")
    );
    run_compass_phases(&mut agent)
}

fn run_compass_phases(agent: &mut CompassAgent) -> Result<()> {
    // Execute COMPASS framework
    println!("Phase 1: Clarifying Core Intent");
    let intent = agent.clarify_intent(&agent.context.problem_statement.clone())?;
    println!("✓ {}\n", intent);
    agent.advance_phase()?;

    println!("Phase 2: Orienting Through Research");
    let research = agent.orient_research("")?;
    println!("✓ {}\n", research);
    agent.advance_phase()?;

    println!("Phase 3: Mapping Solution Space");
    let solution = agent.map_solution()?;
    println!("✓ {}\n", solution);
    agent.advance_phase()?;

    println!("Phase 4: Pausing for Strategic Validation");
    let should_proceed = agent.validate_strategy()?;
    println!(
        "✓ Strategic validation: {}\n",
        if should_proceed { "PROCEED" } else { "PIVOT" }
    );
    agent.advance_phase()?;

    println!("Phase 5: Architecting Implementation");
    let _architecture = agent.architect_implementation()?;
    println!("✓ Architecture defined\n");
    agent.advance_phase()?;

    println!("Phase 6: Synthesizing and Validating");
    let _synthesis = agent.synthesize_final()?;
    println!("✓ Framework execution complete\n");

    // Generate final report
    println!("{}", agent.generate_report());

    Ok(())
}

fn run_inspection_mode(
    config: &PipelineConfig,
    input: Box<dyn io::BufRead>,
    matches: &clap::ArgMatches,
) -> Result<()> {
    let use_color = should_use_color(matches);

    let options = InspectorOptions::new()
        .interactive(matches.get_flag("interactive"))
        .show_performance(matches.get_flag("performance"))
        .show_line_numbers(true)
        .show_captures(true);

    let mut inspector = Inspector::new(config.clone())?
        .with_options(options)
        .with_color(use_color);
    let result = inspector.inspect_stream(input)?;
    inspector.display_results(&result)?;

    Ok(())
}

fn run_processing_mode(
    config: &PipelineConfig,
    input: Box<dyn io::BufRead>,
    matches: &clap::ArgMatches,
) -> Result<()> {
    let quiet = matches.get_flag("quiet");
    let json_output = matches.get_flag("json");
    let count_only = matches.get_flag("count");

    let mut processor = StreamProcessor::new(config.clone())?;

    if quiet {
        // Quiet mode: process but don't output anything
        let mut output = std::io::sink();
        let result = processor.process_stream(input, &mut output)?;
        if result.matches_found == 0 {
            std::process::exit(exit_codes::NO_MATCHES);
        }
        return Ok(());
    }

    if count_only {
        // Count mode: just count matches
        let mut output = std::io::sink();
        let result = processor.process_stream(input, &mut output)?;

        if json_output {
            println!("{}", json_schema::output_count_json(&result)?);
        } else {
            println!("{}", result.matches_found);
        }
        return Ok(());
    }

    let output: Box<dyn io::Write> = if let Some(output_file) = matches.get_one::<String>("output")
    {
        Box::new(File::create(output_file)?)
    } else {
        Box::new(io::stdout())
    };

    let result = processor.process_stream(input, output)?;

    if matches.get_flag("performance") {
        eprintln!("{}", result.performance_summary());
        eprintln!("{}", processor.performance_report());
    }

    if json_output && matches.get_flag("performance") {
        // If both json and performance are requested, output performance as JSON
        eprintln!("{}", json_schema::output_performance_json(&result)?);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_loading_from_pattern() {
        let mut _matches: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        // This would normally be created by clap, but for testing we simulate it

        let config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUMBER"));
        assert_eq!(config.step.len(), 1);
        assert!(config.validate().is_ok());
    }
}
