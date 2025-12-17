//! Fuzz target for regex pattern compilation
//!
//! This target tests the pattern compilation code paths in the processor module,
//! ensuring that arbitrary pattern strings don't cause panics or undefined behavior.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rexpipe::pipeline::PipelineConfig;
use rexpipe::processor::StreamProcessor;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    // Try to interpret the input as a UTF-8 string for use as a pattern
    if let Ok(pattern) = std::str::from_utf8(data) {
        // Skip empty patterns
        if pattern.is_empty() {
            return;
        }

        // Skip very long patterns to avoid timeout
        if pattern.len() > 1000 {
            return;
        }

        // Test standard regex mode
        let config = PipelineConfig::from_inline_pattern(pattern, Some("REPLACED"));
        if let Ok(mut processor) = StreamProcessor::new(config) {
            let input = Cursor::new("Test 123 input with numbers 456 and text");
            let mut output = Vec::new();
            // Ignore the result - we're testing that it doesn't panic
            let _ = processor.process_stream(input, &mut output);
        }

        // Test with no replacement (match-only mode)
        let config = PipelineConfig::from_inline_pattern(pattern, None);
        if let Ok(mut processor) = StreamProcessor::new(config) {
            let input = Cursor::new("Test input for matching");
            let mut output = Vec::new();
            let _ = processor.process_stream(input, &mut output);
        }
    }
});
