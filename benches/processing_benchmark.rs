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

criterion_group!(benches, benchmark_processing, benchmark_inspection);
criterion_main!(benches);
