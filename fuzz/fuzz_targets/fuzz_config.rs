//! Fuzz target for TOML configuration parsing
//!
//! This target tests the TOML parsing and deserialization code paths,
//! ensuring that arbitrary TOML-like strings don't cause panics.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rexpipe::pipeline::PipelineConfig;
use std::io::Write;
use tempfile::NamedTempFile;

fuzz_target!(|data: &[u8]| {
    // Try to interpret the input as a UTF-8 string for use as TOML config
    if let Ok(config_str) = std::str::from_utf8(data) {
        // Skip empty configs
        if config_str.is_empty() {
            return;
        }

        // Skip very large configs to avoid timeout
        if config_str.len() > 10000 {
            return;
        }

        // Write to a temp file and try to parse
        if let Ok(mut temp_file) = NamedTempFile::new() {
            if temp_file.write_all(data).is_ok() {
                // Try to parse the config - we don't care if it fails,
                // we just want to ensure it doesn't panic
                let _ = PipelineConfig::from_file(temp_file.path());
            }
        }

        // Also test the TOML parsing directly
        let _ = toml::from_str::<PipelineConfig>(config_str);
    }
});
