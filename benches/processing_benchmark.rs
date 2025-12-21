use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rexpipe::pipeline::PipelineConfig;
use rexpipe::processor::StreamProcessor;
use std::io::Cursor;

fn benchmark_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("regex_processing");

    // Test data of various sizes
    let small_data = generate_test_data(100);
    let medium_data = generate_test_data(1000);
    let large_data = generate_test_data(10000);

    // Simple substitution benchmark
    let substitution_config = PipelineConfig::from_inline_pattern(r"\d+", Some("NUMBER"));

    group.bench_with_input(
        BenchmarkId::new("substitution", "small"),
        &small_data,
        |b, data| {
            b.iter(|| {
                let mut processor = StreamProcessor::new(substitution_config.clone()).unwrap();
                let reader = Cursor::new(data);
                let mut output = Vec::new();
                processor.process_stream(reader, &mut output).unwrap();
            })
        },
    );

    group.bench_with_input(
        BenchmarkId::new("substitution", "medium"),
        &medium_data,
        |b, data| {
            b.iter(|| {
                let mut processor = StreamProcessor::new(substitution_config.clone()).unwrap();
                let reader = Cursor::new(data);
                let mut output = Vec::new();
                processor.process_stream(reader, &mut output).unwrap();
            })
        },
    );

    group.bench_with_input(
        BenchmarkId::new("substitution", "large"),
        &large_data,
        |b, data| {
            b.iter(|| {
                let mut processor = StreamProcessor::new(substitution_config.clone()).unwrap();
                let reader = Cursor::new(data);
                let mut output = Vec::new();
                processor.process_stream(reader, &mut output).unwrap();
            })
        },
    );

    // Complex pipeline benchmark (simulating multi-tool equivalent)
    let complex_config = create_complex_pipeline();

    group.bench_with_input(
        BenchmarkId::new("complex_pipeline", "medium"),
        &medium_data,
        |b, data| {
            b.iter(|| {
                let mut processor = StreamProcessor::new(complex_config.clone()).unwrap();
                let reader = Cursor::new(data);
                let mut output = Vec::new();
                processor.process_stream(reader, &mut output).unwrap();
            })
        },
    );

    group.finish();
}

fn benchmark_inspection(c: &mut Criterion) {
    let mut group = c.benchmark_group("pattern_inspection");

    let test_data = generate_test_data(1000);
    let config = PipelineConfig::from_inline_pattern(r"(\w+)=(\d+)", None);

    group.bench_function("inspect_stream", |b| {
        b.iter(|| {
            let mut inspector = rexpipe::inspector::Inspector::new(config.clone()).unwrap();
            let reader = Cursor::new(&test_data);
            inspector.inspect_stream(reader).unwrap();
        })
    });

    group.finish();
}

fn benchmark_filter_operations(c: &mut Criterion) {
    use rexpipe::pipeline::*;

    let mut group = c.benchmark_group("filter_operations");
    let test_data = generate_test_data(1000);

    // KeepLine filter
    let keep_config = PipelineConfig {
        name: Some("Keep Filter".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Filter,
            pattern: r"\[INFO\]".to_string(),
            replacement: None,
            action: Some(FilterAction::KeepLine),
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
        }],
    };

    group.bench_function("keep_line_filter", |b| {
        b.iter(|| {
            let mut processor = StreamProcessor::new(keep_config.clone()).unwrap();
            let reader = Cursor::new(&test_data);
            let mut output = Vec::new();
            processor.process_stream(reader, &mut output).unwrap();
        })
    });

    // DropLine filter
    let drop_config = PipelineConfig {
        name: Some("Drop Filter".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Filter,
            pattern: r"\[DEBUG\]".to_string(),
            replacement: None,
            action: Some(FilterAction::DropLine),
            transform: None,
            flags: None,
            description: None,
            enabled: Some(true),
        }],
    };

    group.bench_function("drop_line_filter", |b| {
        b.iter(|| {
            let mut processor = StreamProcessor::new(drop_config.clone()).unwrap();
            let reader = Cursor::new(&test_data);
            let mut output = Vec::new();
            processor.process_stream(reader, &mut output).unwrap();
        })
    });

    group.finish();
}

fn benchmark_fixed_vs_regex(c: &mut Criterion) {
    let mut group = c.benchmark_group("fixed_vs_regex");
    let test_data = generate_test_data(1000);

    // Regex pattern
    let regex_config = PipelineConfig::from_inline_pattern(r"\[ERROR\]", Some("[REPLACED]"));

    // Fixed string pattern (same pattern, but fixed mode)
    let fixed_settings = rexpipe::pipeline::PipelineSettings {
        fixed_strings: true,
        ..Default::default()
    };
    let fixed_config = PipelineConfig::from_inline_pattern_with_settings(
        "[ERROR]",
        Some("[REPLACED]"),
        fixed_settings,
    );

    group.bench_function("regex_match", |b| {
        b.iter(|| {
            let mut processor = StreamProcessor::new(regex_config.clone()).unwrap();
            let reader = Cursor::new(&test_data);
            let mut output = Vec::new();
            processor.process_stream(reader, &mut output).unwrap();
        })
    });

    group.bench_function("fixed_string_match", |b| {
        b.iter(|| {
            let mut processor = StreamProcessor::new(fixed_config.clone()).unwrap();
            let reader = Cursor::new(&test_data);
            let mut output = Vec::new();
            processor.process_stream(reader, &mut output).unwrap();
        })
    });

    group.finish();
}

fn benchmark_transform_operations(c: &mut Criterion) {
    use rexpipe::pipeline::*;

    let mut group = c.benchmark_group("transform_operations");
    let test_data = generate_test_data(1000);

    // Uppercase transform
    let uppercase_config = PipelineConfig {
        name: Some("Uppercase Transform".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Transform,
            pattern: r"[a-z]+".to_string(),
            replacement: None,
            action: None,
            transform: Some(TransformAction::Uppercase),
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
        }],
    };

    group.bench_function("uppercase_transform", |b| {
        b.iter(|| {
            let mut processor = StreamProcessor::new(uppercase_config.clone()).unwrap();
            let reader = Cursor::new(&test_data);
            let mut output = Vec::new();
            processor.process_stream(reader, &mut output).unwrap();
        })
    });

    // Trim transform
    let trim_config = PipelineConfig {
        name: Some("Trim Transform".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![PipelineStep {
            step_type: StepType::Transform,
            pattern: r"^\s+|\s+$".to_string(),
            replacement: None,
            action: None,
            transform: Some(TransformAction::Trim),
            flags: Some(vec![RegexFlag::Global]),
            description: None,
            enabled: Some(true),
        }],
    };

    group.bench_function("trim_transform", |b| {
        b.iter(|| {
            let mut processor = StreamProcessor::new(trim_config.clone()).unwrap();
            let reader = Cursor::new(&test_data);
            let mut output = Vec::new();
            processor.process_stream(reader, &mut output).unwrap();
        })
    });

    group.finish();
}

fn benchmark_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialization");

    let config = create_complex_pipeline();

    group.bench_function("to_json", |b| {
        b.iter(|| {
            config.to_json().unwrap();
        })
    });

    group.bench_function("to_toml", |b| {
        b.iter(|| {
            config.to_toml().unwrap();
        })
    });

    let json = config.to_json().unwrap();
    group.bench_function("from_json", |b| {
        b.iter(|| {
            PipelineConfig::from_json(&json).unwrap();
        })
    });

    let toml_str = config.to_toml().unwrap();
    group.bench_function("from_toml", |b| {
        b.iter(|| {
            let _config: PipelineConfig = toml::from_str(&toml_str).unwrap();
        })
    });

    group.finish();
}

fn create_complex_pipeline() -> PipelineConfig {
    use rexpipe::pipeline::*;

    PipelineConfig {
        name: Some("Complex Benchmark Pipeline".to_string()),
        description: Some("Multi-step processing simulation".to_string()),
        version: Some("1.0.0".to_string()),
        patterns_include: Vec::new(),
        settings: PipelineSettings::default(),
        step: vec![
            PipelineStep {
                step_type: StepType::Substitute,
                pattern: r"\[ERROR\]".to_string(),
                replacement: Some("[ERR]".to_string()),
                action: None,
                transform: None,
                flags: Some(vec![RegexFlag::Global]),
                description: Some("Normalize error levels".to_string()),
                enabled: Some(true),
            },
            PipelineStep {
                step_type: StepType::Filter,
                pattern: "DEBUG".to_string(),
                replacement: None,
                action: Some(FilterAction::DropLine),
                transform: None,
                flags: None,
                description: Some("Remove debug messages".to_string()),
                enabled: Some(true),
            },
            PipelineStep {
                step_type: StepType::Substitute,
                pattern: r"user_id=(\d+)".to_string(),
                replacement: Some("uid=${1}".to_string()),
                action: None,
                transform: None,
                flags: Some(vec![RegexFlag::Global]),
                description: Some("Standardize user ID format".to_string()),
                enabled: Some(true),
            },
            PipelineStep {
                step_type: StepType::Substitute,
                pattern: r"192\.168\.".to_string(),
                replacement: Some("10.0.".to_string()),
                action: None,
                transform: None,
                flags: Some(vec![RegexFlag::Global]),
                description: Some("Anonymize IP addresses".to_string()),
                enabled: Some(true),
            },
        ],
    }
}

fn generate_test_data(lines: usize) -> String {
    let log_patterns = [
        "2025-01-08 10:15:23 [INFO] Server startup complete",
        "2025-01-08 10:15:24 [DEBUG] Loading configuration from /etc/config",
        "2025-01-08 10:15:25 [ERROR] Database connection failed for user_id=1234",
        "2025-01-08 10:15:26 [INFO] Retrying connection from 192.168.1.10",
        "2025-01-08 10:15:27 [WARN] Authentication failed for user john@company.com",
        "2025-01-08 10:15:28 [DEBUG] Trace: method_call(param1, param2)",
        "2025-01-08 10:15:29 [ERROR] Permission denied for user_id=5678",
        "2025-01-08 10:15:30 [INFO] Connection established from 192.168.1.15",
    ];

    let mut data = String::new();
    for i in 0..lines {
        let pattern_index = i % log_patterns.len();
        data.push_str(log_patterns[pattern_index]);
        data.push('\n');
    }

    data
}

// =====================================================
// Multi-File Processing Benchmarks
// =====================================================

fn benchmark_multifile_processing(c: &mut Criterion) {
    use rexpipe::files::{FileProcessingOptions, MultiFileProcessor};
    use std::path::PathBuf;
    use tempfile::TempDir;

    let mut group = c.benchmark_group("multifile_processing");
    group.sample_size(20); // Fewer samples for file I/O tests

    // Create temporary test files
    let temp_dir = TempDir::new().unwrap();
    let file_count = 50;
    let lines_per_file = 100;

    let mut paths: Vec<PathBuf> = Vec::new();
    for i in 0..file_count {
        let file_path = temp_dir.path().join(format!("file{:03}.log", i));
        std::fs::write(&file_path, generate_test_data(lines_per_file)).unwrap();
        paths.push(file_path);
    }

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[NUM]"));
    let options_seq = FileProcessingOptions::default();
    let options_par = FileProcessingOptions {
        parallel: true,
        ..Default::default()
    };

    // Sequential processing
    group.bench_function(format!("sequential_{}_files", file_count), |b| {
        let processor = MultiFileProcessor::new(config.clone(), options_seq.clone());
        b.iter(|| {
            processor.process_files(&paths).unwrap();
        })
    });

    // Parallel processing
    group.bench_function(format!("parallel_{}_files", file_count), |b| {
        let processor = MultiFileProcessor::new(config.clone(), options_par.clone());
        b.iter(|| {
            processor.process_files(&paths).unwrap();
        })
    });

    group.finish();
}

fn benchmark_large_files(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_file_processing");
    group.sample_size(20);

    // Generate very large test data
    let xlarge_data = generate_test_data(100_000); // ~10 MB of data

    let simple_config = PipelineConfig::from_inline_pattern(r"\d+", Some("X"));
    let complex_config = create_complex_pipeline();

    group.bench_function("simple_pattern_xlarge", |b| {
        b.iter(|| {
            let mut processor = StreamProcessor::new(simple_config.clone()).unwrap();
            let reader = Cursor::new(&xlarge_data);
            let mut output = Vec::new();
            processor.process_stream(reader, &mut output).unwrap();
        })
    });

    group.bench_function("complex_pipeline_xlarge", |b| {
        b.iter(|| {
            let mut processor = StreamProcessor::new(complex_config.clone()).unwrap();
            let reader = Cursor::new(&xlarge_data);
            let mut output = Vec::new();
            processor.process_stream(reader, &mut output).unwrap();
        })
    });

    group.finish();
}

fn benchmark_long_lines(c: &mut Criterion) {
    let mut group = c.benchmark_group("long_line_processing");

    // Generate data with very long lines (common in JSON/XML processing)
    fn generate_long_line_data(line_length: usize, line_count: usize) -> String {
        let base = "word123 ";
        let repeat_count = line_length / base.len();
        let long_line = base.repeat(repeat_count);
        (0..line_count).map(|_| long_line.clone()).collect::<Vec<_>>().join("\n")
    }

    let short_lines = generate_long_line_data(100, 1000);   // 100 chars x 1000 lines
    let long_lines = generate_long_line_data(10_000, 100);  // 10KB lines x 100

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[X]"));

    group.bench_function("short_lines_1000x100", |b| {
        b.iter(|| {
            let mut processor = StreamProcessor::new(config.clone()).unwrap();
            let reader = Cursor::new(&short_lines);
            let mut output = Vec::new();
            processor.process_stream(reader, &mut output).unwrap();
        })
    });

    group.bench_function("long_lines_100x10KB", |b| {
        b.iter(|| {
            let mut processor = StreamProcessor::new(config.clone()).unwrap();
            let reader = Cursor::new(&long_lines);
            let mut output = Vec::new();
            processor.process_stream(reader, &mut output).unwrap();
        })
    });

    group.finish();
}

fn benchmark_many_matches(c: &mut Criterion) {
    let mut group = c.benchmark_group("many_matches");

    // Data with many matches per line (worst case for replacement)
    fn generate_dense_match_data(matches_per_line: usize, lines: usize) -> String {
        let line = (0..matches_per_line)
            .map(|i| format!("num{}", i))
            .collect::<Vec<_>>()
            .join(" ");
        (0..lines).map(|_| line.clone()).collect::<Vec<_>>().join("\n")
    }

    let sparse_data = generate_dense_match_data(2, 500);    // 2 matches per line
    let dense_data = generate_dense_match_data(50, 100);    // 50 matches per line

    let config = PipelineConfig::from_inline_pattern(r"num\d+", Some("[REPLACED]"));

    group.bench_function("sparse_matches_2_per_line", |b| {
        b.iter(|| {
            let mut processor = StreamProcessor::new(config.clone()).unwrap();
            let reader = Cursor::new(&sparse_data);
            let mut output = Vec::new();
            processor.process_stream(reader, &mut output).unwrap();
        })
    });

    group.bench_function("dense_matches_50_per_line", |b| {
        b.iter(|| {
            let mut processor = StreamProcessor::new(config.clone()).unwrap();
            let reader = Cursor::new(&dense_data);
            let mut output = Vec::new();
            processor.process_stream(reader, &mut output).unwrap();
        })
    });

    group.finish();
}

fn benchmark_pattern_complexity(c: &mut Criterion) {
    let mut group = c.benchmark_group("pattern_complexity");
    let test_data = generate_test_data(1000);

    // Simple literal pattern
    let simple_config = PipelineConfig::from_inline_pattern("ERROR", Some("ERR"));

    // Alternation pattern
    let alternation_config = PipelineConfig::from_inline_pattern(
        r"ERROR|WARN|INFO|DEBUG",
        Some("[LEVEL]")
    );

    // Complex pattern with groups and lookarounds (anchors)
    let complex_config = PipelineConfig::from_inline_pattern(
        r"(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2}):(\d{2})",
        Some("${1}/${2}/${3} ${4}:${5}:${6}")
    );

    group.bench_function("simple_literal", |b| {
        b.iter(|| {
            let mut processor = StreamProcessor::new(simple_config.clone()).unwrap();
            let reader = Cursor::new(&test_data);
            let mut output = Vec::new();
            processor.process_stream(reader, &mut output).unwrap();
        })
    });

    group.bench_function("alternation", |b| {
        b.iter(|| {
            let mut processor = StreamProcessor::new(alternation_config.clone()).unwrap();
            let reader = Cursor::new(&test_data);
            let mut output = Vec::new();
            processor.process_stream(reader, &mut output).unwrap();
        })
    });

    group.bench_function("complex_groups", |b| {
        b.iter(|| {
            let mut processor = StreamProcessor::new(complex_config.clone()).unwrap();
            let reader = Cursor::new(&test_data);
            let mut output = Vec::new();
            processor.process_stream(reader, &mut output).unwrap();
        })
    });

    group.finish();
}

fn benchmark_discovery_and_filtering(c: &mut Criterion) {
    use rexpipe::files::{FileProcessingOptions, MultiFileProcessor};
    use tempfile::TempDir;

    let mut group = c.benchmark_group("file_discovery");
    group.sample_size(20);

    // Create a directory structure with many files
    let temp_dir = TempDir::new().unwrap();

    // Create nested directories with files
    for i in 0..10 {
        let subdir = temp_dir.path().join(format!("subdir{}", i));
        std::fs::create_dir(&subdir).unwrap();
        for j in 0..50 {
            let file_path = subdir.join(format!("file{}.txt", j));
            std::fs::write(&file_path, generate_test_data(10)).unwrap();
            // Also create some log files
            if j % 5 == 0 {
                let log_path = subdir.join(format!("file{}.log", j));
                std::fs::write(&log_path, generate_test_data(10)).unwrap();
            }
        }
    }

    let config = PipelineConfig::from_inline_pattern(r"\d+", Some("[X]"));

    // Discovery without filters
    let options_nofilter = FileProcessingOptions::default();

    // Discovery with exclude filter
    let options_exclude = FileProcessingOptions {
        exclude_patterns: vec!["*.log".to_string()],
        ..Default::default()
    };

    group.bench_function("discover_unfiltered", |b| {
        let processor = MultiFileProcessor::new(config.clone(), options_nofilter.clone());
        b.iter(|| {
            processor.discover_files(&[temp_dir.path().to_path_buf()]).unwrap();
        })
    });

    group.bench_function("discover_with_exclude", |b| {
        let processor = MultiFileProcessor::new(config.clone(), options_exclude.clone());
        b.iter(|| {
            processor.discover_files(&[temp_dir.path().to_path_buf()]).unwrap();
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_processing,
    benchmark_inspection,
    benchmark_filter_operations,
    benchmark_fixed_vs_regex,
    benchmark_transform_operations,
    benchmark_serialization,
    benchmark_multifile_processing,
    benchmark_large_files,
    benchmark_long_lines,
    benchmark_many_matches,
    benchmark_pattern_complexity,
    benchmark_discovery_and_filtering
);
criterion_main!(benches);
