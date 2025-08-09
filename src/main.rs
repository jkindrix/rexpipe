mod compass;
mod pipeline;
mod processor;
mod inspector;

use clap::{Arg, ArgAction, Command};
use std::io::{self, BufReader};
use std::fs::File;

use compass::CompassAgent;
use pipeline::PipelineConfig;
use processor::StreamProcessor;
use inspector::{Inspector, InspectorOptions};

fn main() {
    let matches = Command::new("rexpipe")
        .version("1.0.0")
        .author("Strategic Collaboration Agent")
        .about("Unified regex pipeline processor with COMPASS framework integration")
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

    // Load or create pipeline configuration
    let config = load_pipeline_config(matches)?;

    // Validate configuration if requested
    if matches.get_flag("validate") || matches.get_flag("dry-run") {
        return validate_configuration(&config);
    }

    // Create input reader
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

fn load_pipeline_config(matches: &clap::ArgMatches) -> Result<PipelineConfig, Box<dyn std::error::Error>> {
    if let Some(config_file) = matches.get_one::<String>("config") {
        PipelineConfig::from_file(config_file)
    } else if let Some(pattern) = matches.get_one::<String>("pattern") {
        let replacement = matches.get_one::<String>("replacement").map(|s| s.as_str());
        Ok(PipelineConfig::from_inline_pattern(pattern, replacement))
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
    let mut processor = StreamProcessor::new(config.clone())?;
    
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
