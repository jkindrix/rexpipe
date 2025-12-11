mod compass;
mod files;
mod pipeline;
mod processor;
mod inspector;

use clap::{Arg, ArgAction, Command};
use std::io::{self, BufReader};
use std::fs::File;
use std::path::PathBuf;

use compass::CompassAgent;
use files::{FileProcessingOptions, MultiFileProcessor, MultiFileResult};
use pipeline::{PipelineConfig, PipelineSettings};
use processor::StreamProcessor;
use inspector::{Inspector, InspectorOptions};

fn main() {
    let matches = Command::new("rexpipe")
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
        )
        .arg(
            Arg::new("pattern")
                .short('p')
                .long("pattern")
                .value_name("REGEX")
                .help("Inline regex pattern")
        )
        .arg(
            Arg::new("replacement")
                .short('r')
                .long("replacement")
                .value_name("TEXT")
                .help("Replacement text for substitution")
        )
        // === Regex Engine Options ===
        .arg(
            Arg::new("fixed")
                .short('F')
                .long("fixed")
                .help("Treat pattern as fixed string (no regex interpretation)")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("pcre")
                .short('P')
                .long("pcre")
                .help("Use PCRE2-compatible regex (supports lookahead/lookbehind)")
                .action(ArgAction::SetTrue)
        )
        // === File Operations ===
        .arg(
            Arg::new("in-place")
                .short('I')
                .long("in-place")
                .help("Edit files in-place")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("backup")
                .short('b')
                .long("backup")
                .value_name("SUFFIX")
                .help("Create backup with given suffix when editing in-place (e.g., .bak)")
        )
        .arg(
            Arg::new("recursive")
                .short('R')
                .long("recursive")
                .help("Recursively process directories")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("glob")
                .short('g')
                .long("glob")
                .value_name("PATTERN")
                .help("Only process files matching glob pattern (e.g., '*.txt')")
                .action(ArgAction::Append)
        )
        .arg(
            Arg::new("exclude")
                .short('e')
                .long("exclude")
                .value_name("PATTERN")
                .help("Exclude files matching glob pattern")
                .action(ArgAction::Append)
        )
        .arg(
            Arg::new("no-ignore")
                .long("no-ignore")
                .help("Don't respect .gitignore files")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("hidden")
                .long("hidden")
                .help("Include hidden files")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("max-depth")
                .long("max-depth")
                .value_name("NUM")
                .help("Maximum directory recursion depth")
        )
        // === Processing Modes ===
        .arg(
            Arg::new("parallel")
                .short('j')
                .long("parallel")
                .help("Process files in parallel")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("inspect")
                .long("inspect")
                .help("Enable inspection mode")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("interactive")
                .long("interactive")
                .help("Enable interactive inspection")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .help("Validate configuration without processing")
                .action(ArgAction::SetTrue)
        )
        // === Output Modes ===
        .arg(
            Arg::new("count")
                .long("count")
                .help("Only show count of matches per file")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("files-with-matches")
                .short('l')
                .long("files-with-matches")
                .help("Only list files containing matches")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("files-without-matches")
                .short('L')
                .long("files-without-matches")
                .help("Only list files not containing matches")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .help("Quiet mode - only set exit code")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output results as JSON")
                .action(ArgAction::SetTrue)
        )
        // === Context Lines (for inspection) ===
        .arg(
            Arg::new("context-before")
                .short('B')
                .long("before-context")
                .value_name("NUM")
                .help("Show NUM lines before each match")
        )
        .arg(
            Arg::new("context-after")
                .short('A')
                .long("after-context")
                .value_name("NUM")
                .help("Show NUM lines after each match")
        )
        .arg(
            Arg::new("context")
                .short('C')
                .long("context")
                .value_name("NUM")
                .help("Show NUM lines before and after each match")
        )
        // === Misc ===
        .arg(
            Arg::new("performance")
                .long("performance")
                .help("Show performance metrics")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("compass")
                .long("compass")
                .help("Run COMPASS strategic analysis")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("validate")
                .long("validate")
                .help("Validate configuration only")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("export")
                .long("export")
                .value_name("FORMAT")
                .help("Export configuration (toml or json)")
        )
        // === I/O ===
        .arg(
            Arg::new("input")
                .short('i')
                .long("input")
                .value_name("FILE")
                .help("Input file (default: stdin)")
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("FILE")
                .help("Output file (default: stdout)")
        )
        // === Positional Args ===
        .arg(
            Arg::new("paths")
                .help("Files or directories to process")
                .action(ArgAction::Append)
                .num_args(0..)
        )
        .get_matches();

    if let Err(e) = run_application(&matches) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run_application(matches: &clap::ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    // Handle COMPASS mode first
    if matches.get_flag("compass") {
        return run_compass_analysis();
    }

    // Build pipeline settings from CLI flags
    let settings = build_pipeline_settings(matches);

    // Load or create pipeline configuration
    let config = load_pipeline_config(matches, settings)?;

    // Handle export mode
    if let Some(format) = matches.get_one::<String>("export") {
        return export_configuration(&config, format);
    }

    // Validate configuration if requested
    if matches.get_flag("validate") || matches.get_flag("dry-run") {
        return validate_configuration(&config);
    }

    // Check if we're in multi-file mode
    let paths: Vec<PathBuf> = matches
        .get_many::<String>("paths")
        .map(|v| v.map(PathBuf::from).collect())
        .unwrap_or_default();

    let is_multi_file = matches.get_flag("recursive")
        || matches.get_flag("in-place")
        || !paths.is_empty();

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

    PipelineSettings {
        pcre_mode: matches.get_flag("pcre"),
        fixed_strings: matches.get_flag("fixed"),
        context_before,
        context_after,
    }
}

fn export_configuration(config: &PipelineConfig, format: &str) -> Result<(), Box<dyn std::error::Error>> {
    let output = match format.to_lowercase().as_str() {
        "toml" => config.to_toml()?,
        "json" => config.to_json()?,
        _ => return Err(format!("Unknown export format: {}. Use 'toml' or 'json'", format).into()),
    };
    println!("{}", output);
    Ok(())
}

fn run_multi_file_mode(
    config: &PipelineConfig,
    matches: &clap::ArgMatches,
    paths: Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let quiet = matches.get_flag("quiet");
    let json_output = matches.get_flag("json");

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
        .quiet(quiet);

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

    if files.is_empty() {
        if !quiet {
            eprintln!("No files found matching criteria");
        }
        return Ok(());
    }

    // Process based on mode
    let result = if options.files_with_matches {
        let matching = processor.files_with_matches(&files)?;
        output_file_list(&matching, quiet, json_output)?;
        return Ok(());
    } else if options.files_without_matches {
        let non_matching = processor.files_without_matches(&files)?;
        output_file_list(&non_matching, quiet, json_output)?;
        return Ok(());
    } else if options.count_only {
        let result = processor.count_matches(&files)?;
        output_count_results(&result, quiet, json_output)?;
        return Ok(());
    } else {
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

    // Set exit code based on matches
    if !result.has_matches() {
        std::process::exit(1);
    }

    Ok(())
}

fn output_file_list(files: &[PathBuf], quiet: bool, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    if quiet {
        return Ok(());
    }

    if json {
        let json_files: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();
        println!("{}", serde_json::to_string_pretty(&json_files)?);
    } else {
        for file in files {
            println!("{}", file.display());
        }
    }
    Ok(())
}

fn output_count_results(result: &MultiFileResult, quiet: bool, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    if quiet {
        return Ok(());
    }

    if json {
        #[derive(serde::Serialize)]
        struct CountResult {
            file: String,
            matches: u64,
            lines: u64,
        }

        let counts: Vec<CountResult> = result
            .file_results
            .iter()
            .map(|r| CountResult {
                file: r.path.display().to_string(),
                matches: r.matches_found,
                lines: r.lines_processed,
            })
            .collect();

        println!("{}", serde_json::to_string_pretty(&counts)?);
    } else {
        for file_result in &result.file_results {
            println!("{}:{}", file_result.path.display(), file_result.matches_found);
        }
        println!("---");
        println!("Total: {} matches in {} files", result.total_matches, result.files_matched);
    }
    Ok(())
}

fn output_multi_file_json(result: &MultiFileResult) -> Result<(), Box<dyn std::error::Error>> {
    #[derive(serde::Serialize)]
    struct JsonResult {
        files_processed: u64,
        files_matched: u64,
        files_modified: u64,
        total_matches: u64,
        total_lines: u64,
        errors: Vec<String>,
    }

    let json_result = JsonResult {
        files_processed: result.files_processed,
        files_matched: result.files_matched,
        files_modified: result.files_modified,
        total_matches: result.total_matches,
        total_lines: result.total_lines,
        errors: result.errors.clone(),
    };

    println!("{}", serde_json::to_string_pretty(&json_result)?);
    Ok(())
}

fn output_multi_file_summary(result: &MultiFileResult) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", result.summary());
    if !result.errors.is_empty() {
        eprintln!("\nErrors:");
        for error in &result.errors {
            eprintln!("  {}", error);
        }
    }
    Ok(())
}

fn load_pipeline_config(matches: &clap::ArgMatches, settings: PipelineSettings) -> Result<PipelineConfig, Box<dyn std::error::Error>> {
    if let Some(config_file) = matches.get_one::<String>("config") {
        let mut config = PipelineConfig::from_file(config_file)?;
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
        Ok(config)
    } else if let Some(pattern) = matches.get_one::<String>("pattern") {
        let replacement = matches.get_one::<String>("replacement").map(|s| s.as_str());
        Ok(PipelineConfig::from_inline_pattern_with_settings(pattern, replacement, settings))
    } else {
        Err("Must specify either --config FILE or --pattern REGEX".into())
    }
}

fn validate_configuration(config: &PipelineConfig) -> Result<(), Box<dyn std::error::Error>> {
    match config.validate() {
        Ok(()) => {
            println!("✓ Configuration is valid");
            println!("{}", config.summary());
            
            // Test compilation
            StreamProcessor::new(config.clone())?;
            println!("✓ All patterns compile successfully");
            
            Ok(())
        }
        Err(errors) => {
            println!("✗ Configuration validation failed:");
            for error in errors {
                println!("  - {}", error);
            }
            Err("Configuration is invalid".into())
        }
    }
}

fn run_compass_analysis() -> Result<(), Box<dyn std::error::Error>> {
    println!("Initializing COMPASS Strategic Collaboration Agent...\n");
    
    let mut agent = CompassAgent::new();
    
    // Execute COMPASS framework
    println!("Phase 1: Clarifying Core Intent");
    let intent = agent.clarify_intent("Build unified regex pipeline processor")?;
    println!("✓ {}\n", intent);
    agent.advance_phase()?;
    
    println!("Phase 2: Orienting Through Research");
    let research = agent.orient_research("Multi-tool fragmentation causes performance issues")?;
    println!("✓ {}\n", research);
    agent.advance_phase()?;
    
    println!("Phase 3: Mapping Solution Space");
    let solution = agent.map_solution()?;
    println!("✓ {}\n", solution);
    agent.advance_phase()?;
    
    println!("Phase 4: Pausing for Strategic Validation");
    let should_proceed = agent.validate_strategy()?;
    println!("✓ Strategic validation: {}\n", if should_proceed { "PROCEED" } else { "PIVOT" });
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
) -> Result<(), Box<dyn std::error::Error>> {
    let options = InspectorOptions::new()
        .interactive(matches.get_flag("interactive"))
        .show_performance(matches.get_flag("performance"))
        .show_line_numbers(true)
        .show_captures(true);

    let mut inspector = Inspector::new(config.clone())?.with_options(options);
    let result = inspector.inspect_stream(input)?;
    inspector.display_results(&result)?;
    
    Ok(())
}

fn run_processing_mode(
    config: &PipelineConfig,
    input: Box<dyn io::BufRead>,
    matches: &clap::ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    let quiet = matches.get_flag("quiet");
    let json_output = matches.get_flag("json");
    let count_only = matches.get_flag("count");

    let mut processor = StreamProcessor::new(config.clone())?;

    if quiet {
        // Quiet mode: process but don't output anything
        let mut output = std::io::sink();
        let result = processor.process_stream(input, &mut output)?;
        if result.matches_found == 0 {
            std::process::exit(1);
        }
        return Ok(());
    }

    if count_only {
        // Count mode: just count matches
        let mut output = std::io::sink();
        let result = processor.process_stream(input, &mut output)?;

        if json_output {
            #[derive(serde::Serialize)]
            struct CountOutput {
                lines_processed: u64,
                matches_found: u64,
                transformations_applied: u64,
            }
            let count = CountOutput {
                lines_processed: result.lines_processed,
                matches_found: result.matches_found,
                transformations_applied: result.transformations_applied,
            };
            println!("{}", serde_json::to_string_pretty(&count)?);
        } else {
            println!("{}", result.matches_found);
        }
        return Ok(());
    }

    let output: Box<dyn io::Write> = if let Some(output_file) = matches.get_one::<String>("output") {
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
        #[derive(serde::Serialize)]
        struct PerfOutput {
            lines_processed: u64,
            matches_found: u64,
            transformations_applied: u64,
            success_rate: f64,
        }
        let perf = PerfOutput {
            lines_processed: result.lines_processed,
            matches_found: result.matches_found,
            transformations_applied: result.transformations_applied,
            success_rate: result.success_rate(),
        };
        eprintln!("{}", serde_json::to_string_pretty(&perf)?);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_config_loading_from_pattern() {
        let mut _matches: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        // This would normally be created by clap, but for testing we simulate it
        
        let config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUMBER"));
        assert_eq!(config.step.len(), 1);
        assert!(config.validate().is_ok());
    }
}
