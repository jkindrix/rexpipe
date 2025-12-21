//! Python bindings for rexpipe using PyO3.
//!
//! This module provides Python bindings for rexpipe's core functionality,
//! allowing Python users to leverage rexpipe's regex pipeline processing.
//!
//! # Usage from Python
//!
//! ```python
//! import rexpipe
//!
//! # Simple substitution
//! result = rexpipe.substitute(r"\d+", "NUM", "There are 42 apples")
//! print(result)  # "There are NUM apples"
//!
//! # Using a pipeline
//! pipeline = rexpipe.Pipeline()
//! pipeline.add_substitute(r"\d+", "NUM")
//! pipeline.add_filter(r"NUM", "keep")
//! result = pipeline.process("Line 1\nLine 2 with 42\nLine 3")
//!
//! # Case-insensitive matching
//! pipeline2 = rexpipe.Pipeline()
//! pipeline2.add_substitute(r"error", "[ERROR]", case_insensitive=True)
//! result = pipeline2.process("Error: something went wrong")
//!
//! # Data format conversion
//! json_data = '{"name": "John", "age": 30}'
//! yaml_data = rexpipe.convert(json_data, "json", "yaml")
//!
//! # Query JSON data
//! data = '{"users": [{"name": "Alice"}, {"name": "Bob"}]}'
//! result = rexpipe.query(data, ".users[0].name")
//!
//! # Load pipeline from TOML configuration
//! pipeline = rexpipe.Pipeline.from_file("pipeline.toml")
//! result = pipeline.process(input_text)
//! ```

#[cfg(feature = "python")]
use pyo3::prelude::*;

#[cfg(feature = "python")]
use crate::data::{DataFormat, DataValue};
#[cfg(feature = "python")]
use crate::pipeline::{PipelineConfig, PipelineStep, StepType, FilterAction, RegexFlag};
#[cfg(feature = "python")]
use crate::processor::StreamProcessor;

/// A regex pipeline for text processing.
#[cfg(feature = "python")]
#[pyclass]
pub struct Pipeline {
    config: PipelineConfig,
}

#[cfg(feature = "python")]
#[pymethods]
impl Pipeline {
    /// Create a new empty pipeline.
    #[new]
    pub fn new() -> Self {
        Self {
            config: PipelineConfig::default(),
        }
    }

    /// Add a substitution step to the pipeline.
    ///
    /// Args:
    ///     pattern: The regex pattern to match.
    ///     replacement: The replacement text.
    ///     case_insensitive: Whether to match case-insensitively (default: False).
    #[pyo3(signature = (pattern, replacement, case_insensitive = false))]
    pub fn add_substitute(&mut self, pattern: &str, replacement: &str, case_insensitive: bool) {
        let flags = if case_insensitive {
            Some(vec![RegexFlag::CaseInsensitive])
        } else {
            None
        };
        self.config.step.push(PipelineStep {
            step_type: StepType::Substitute,
            pattern: pattern.to_string(),
            replacement: Some(replacement.to_string()),
            flags,
            ..Default::default()
        });
    }

    /// Add a filter step to the pipeline.
    ///
    /// Args:
    ///     pattern: The regex pattern to match.
    ///     action: The filter action ("keep" or "drop").
    ///     case_insensitive: Whether to match case-insensitively (default: False).
    #[pyo3(signature = (pattern, action, case_insensitive = false))]
    pub fn add_filter(&mut self, pattern: &str, action: &str, case_insensitive: bool) {
        let filter_action = match action.to_lowercase().as_str() {
            "keep" | "keep_line" => Some(FilterAction::KeepLine),
            "drop" | "drop_line" => Some(FilterAction::DropLine),
            "keep_match" => Some(FilterAction::KeepMatch),
            "drop_match" => Some(FilterAction::DropMatch),
            _ => Some(FilterAction::KeepLine),
        };

        let flags = if case_insensitive {
            Some(vec![RegexFlag::CaseInsensitive])
        } else {
            None
        };

        self.config.step.push(PipelineStep {
            step_type: StepType::Filter,
            pattern: pattern.to_string(),
            action: filter_action,
            flags,
            ..Default::default()
        });
    }

    /// Add an extraction step to the pipeline.
    ///
    /// Args:
    ///     pattern: The regex pattern with capture groups.
    ///     case_insensitive: Whether to match case-insensitively (default: False).
    #[pyo3(signature = (pattern, case_insensitive = false))]
    pub fn add_extract(&mut self, pattern: &str, case_insensitive: bool) {
        let flags = if case_insensitive {
            Some(vec![RegexFlag::CaseInsensitive])
        } else {
            None
        };
        self.config.step.push(PipelineStep {
            step_type: StepType::Extract,
            pattern: pattern.to_string(),
            flags,
            ..Default::default()
        });
    }

    /// Process text through the pipeline.
    ///
    /// Args:
    ///     input: The input text to process.
    ///
    /// Returns:
    ///     The processed text.
    pub fn process(&self, input: &str) -> PyResult<String> {
        let mut processor = StreamProcessor::new(self.config.clone())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

        let mut output = Vec::new();
        let cursor = std::io::Cursor::new(input.as_bytes());

        processor
            .process_stream(cursor, &mut output)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        String::from_utf8(output)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyUnicodeError, _>(e.to_string()))
    }

    /// Load pipeline configuration from a TOML file.
    #[staticmethod]
    pub fn from_file(path: &str) -> PyResult<Self> {
        let config = PipelineConfig::from_file(path)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(e.to_string()))?;
        Ok(Self { config })
    }

    /// Load pipeline configuration from a TOML string.
    #[staticmethod]
    pub fn from_toml(toml_str: &str) -> PyResult<Self> {
        let config: PipelineConfig = toml::from_str(toml_str)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self { config })
    }

    /// Get the number of steps in the pipeline.
    pub fn step_count(&self) -> usize {
        self.config.step.len()
    }
}

#[cfg(feature = "python")]
impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Perform a simple substitution.
///
/// Args:
///     pattern: The regex pattern to match.
///     replacement: The replacement text.
///     input: The input text to process.
///
/// Returns:
///     The text with substitutions applied.
#[cfg(feature = "python")]
#[pyfunction]
pub fn substitute(pattern: &str, replacement: &str, input: &str) -> PyResult<String> {
    let mut pipeline = Pipeline::new();
    pipeline.add_substitute(pattern, replacement, false);
    pipeline.process(input)
}

/// Filter lines matching a pattern.
///
/// Args:
///     pattern: The regex pattern to match.
///     input: The input text to filter.
///     keep: If True, keep matching lines; if False, drop them (default: True).
///
/// Returns:
///     The filtered text.
#[cfg(feature = "python")]
#[pyfunction]
#[pyo3(signature = (pattern, input, keep = true))]
pub fn filter_lines(pattern: &str, input: &str, keep: bool) -> PyResult<String> {
    let mut pipeline = Pipeline::new();
    let action = if keep { "keep" } else { "drop" };
    pipeline.add_filter(pattern, action, false);
    pipeline.process(input)
}

/// Convert data between formats.
///
/// Args:
///     input: The input data as a string.
///     input_format: The input format ("json", "yaml", "csv", "toml", "xml").
///     output_format: The output format.
///     pretty: Whether to pretty-print the output (default: True).
///
/// Returns:
///     The converted data as a string.
#[cfg(feature = "python")]
#[pyfunction]
#[pyo3(signature = (input, input_format, output_format, pretty = true))]
pub fn convert(input: &str, input_format: &str, output_format: &str, pretty: bool) -> PyResult<String> {
    let in_fmt = parse_format(input_format)?;
    let out_fmt = parse_format(output_format)?;

    let data = DataValue::parse(input, in_fmt)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Parse error: {}", e)))?;

    data.to_format_with_options(out_fmt, pretty)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Conversion error: {}", e)))
}

/// Query structured data with a path expression.
///
/// Args:
///     input: The input data as a string.
///     path: The query path (e.g., ".users[0].name").
///     input_format: The input format (default: auto-detect).
///
/// Returns:
///     The query result as a JSON string.
#[cfg(feature = "python")]
#[pyfunction]
#[pyo3(signature = (input, path, input_format = None))]
pub fn query(input: &str, path: &str, input_format: Option<&str>) -> PyResult<String> {
    let in_fmt = if let Some(fmt) = input_format {
        parse_format(fmt)?
    } else {
        DataFormat::detect(input)
    };

    let data = DataValue::parse(input, in_fmt)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Parse error: {}", e)))?;

    let result = data.query(path)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyKeyError, _>(format!("Query error: {}", e)))?;

    result.to_json()
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Serialization error: {}", e)))
}

/// Detect the format of input data.
///
/// Args:
///     input: The input data as a string.
///
/// Returns:
///     The detected format name ("json", "yaml", "csv", "xml", "toml", "text").
#[cfg(feature = "python")]
#[pyfunction]
pub fn detect_format(input: &str) -> String {
    format!("{:?}", DataFormat::detect(input)).to_lowercase()
}

#[cfg(feature = "python")]
fn parse_format(s: &str) -> PyResult<DataFormat> {
    match s.to_lowercase().as_str() {
        "text" => Ok(DataFormat::Text),
        "json" => Ok(DataFormat::Json),
        "jsonl" | "jsonlines" | "ndjson" => Ok(DataFormat::JsonLines),
        "csv" => Ok(DataFormat::Csv),
        "tsv" => Ok(DataFormat::Tsv),
        "yaml" | "yml" => Ok(DataFormat::Yaml),
        "xml" => Ok(DataFormat::Xml),
        "toml" => Ok(DataFormat::Toml),
        _ => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            format!("Unknown format: {}. Use: json, yaml, csv, xml, toml, text", s)
        )),
    }
}

/// Python module initialization.
#[cfg(feature = "python")]
#[pymodule]
pub fn rexpipe(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Pipeline>()?;
    m.add_function(wrap_pyfunction!(substitute, m)?)?;
    m.add_function(wrap_pyfunction!(filter_lines, m)?)?;
    m.add_function(wrap_pyfunction!(convert, m)?)?;
    m.add_function(wrap_pyfunction!(query, m)?)?;
    m.add_function(wrap_pyfunction!(detect_format, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_python_module_compiles() {
        // This test just verifies the module compiles correctly
        // Actual Python integration tests would be in a separate test suite
    }
}
