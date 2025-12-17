//! Fuzz target for pipeline processing
//!
//! This target tests the full pipeline processing with structured arbitrary input,
//! ensuring that various combinations of settings and input data don't cause panics.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use rexpipe::pipeline::{FilterAction, PipelineConfig, PipelineSettings, PipelineStep, StepType};
use rexpipe::processor::StreamProcessor;
use std::io::Cursor;

/// Arbitrary pipeline step configuration for fuzzing
#[derive(Arbitrary, Debug, Clone)]
struct FuzzStep {
    /// Step type (0-4 maps to StepType variants)
    step_type: u8,
    /// Pattern string (limited length)
    pattern: String,
    /// Whether to include a replacement
    has_replacement: bool,
    /// Replacement text
    replacement: String,
    /// Filter action (0-3 maps to FilterAction variants)
    filter_action: u8,
    /// Whether step is enabled
    enabled: bool,
}

/// Arbitrary input configuration for fuzzing
#[derive(Arbitrary, Debug)]
struct FuzzInput {
    /// Input text to process
    input_text: String,
    /// Pipeline steps to apply
    steps: Vec<FuzzStep>,
    /// Use fixed string mode
    fixed_strings: bool,
}

fn to_step_type(val: u8) -> StepType {
    match val % 5 {
        0 => StepType::Substitute,
        1 => StepType::Filter,
        2 => StepType::Extract,
        3 => StepType::Validate,
        _ => StepType::Transform,
    }
}

fn to_filter_action(val: u8) -> FilterAction {
    match val % 4 {
        0 => FilterAction::KeepLine,
        1 => FilterAction::DropLine,
        2 => FilterAction::KeepMatch,
        _ => FilterAction::DropMatch,
    }
}

fuzz_target!(|fuzz_input: FuzzInput| {
    // Skip empty inputs
    if fuzz_input.input_text.is_empty() {
        return;
    }

    // Skip very large inputs to avoid timeout
    if fuzz_input.input_text.len() > 10000 {
        return;
    }

    // Skip too many steps
    if fuzz_input.steps.len() > 10 {
        return;
    }

    // Build pipeline steps
    let steps: Vec<PipelineStep> = fuzz_input
        .steps
        .iter()
        .filter(|s| !s.pattern.is_empty() && s.pattern.len() < 500)
        .map(|s| {
            let step_type = to_step_type(s.step_type);
            PipelineStep {
                step_type: step_type.clone(),
                pattern: s.pattern.clone(),
                replacement: if s.has_replacement && step_type == StepType::Substitute {
                    Some(s.replacement.clone())
                } else {
                    None
                },
                action: if step_type == StepType::Filter {
                    Some(to_filter_action(s.filter_action))
                } else {
                    None
                },
                transform: None,
                flags: None,
                description: None,
                enabled: Some(s.enabled),
            }
        })
        .collect();

    // Skip if no valid steps
    if steps.is_empty() {
        return;
    }

    let config = PipelineConfig {
        name: Some("Fuzz Pipeline".to_string()),
        description: None,
        version: None,
        patterns_include: Vec::new(),
        settings: PipelineSettings {
            fixed_strings: fuzz_input.fixed_strings,
            pcre_mode: false, // Disabled for fuzzing to avoid PCRE-specific issues
            context_before: 0,
            context_after: 0,
        },
        step: steps,
    };

    // Try to create processor and run
    if let Ok(mut processor) = StreamProcessor::new(config) {
        let input = Cursor::new(fuzz_input.input_text);
        let mut output = Vec::new();
        // We don't care if processing fails, just that it doesn't panic
        let _ = processor.process_stream(input, &mut output);
    }
});
