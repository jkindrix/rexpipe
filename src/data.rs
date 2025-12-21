//! Unified data types for multi-format processing.
//!
//! This module provides a format-agnostic data representation that allows
//! rexpipe to work with JSON, CSV, YAML, XML, TOML, and plain text using
//! a unified interface.
//!
//! ## Supported Formats
//!
//! - **JSON**: Full support including nested objects and arrays
//! - **CSV**: Tabular data with headers and typed values
//! - **YAML**: Configuration and structured documents
//! - **XML**: Element-based documents with attributes
//! - **TOML**: Configuration files with sections
//! - **Text**: Plain text lines (traditional rexpipe mode)
//!
//! ## Example
//!
//! ```
//! use rexpipe::data::{DataValue, DataFormat};
//!
//! // Parse JSON
//! let json = r#"{"name": "Alice", "age": 30}"#;
//! let value = DataValue::parse(json, DataFormat::Json).unwrap();
//!
//! // Query nested data
//! let name = value.query(".name").unwrap();
//! assert_eq!(name.as_str(), Some("Alice"));
//!
//! // Convert to different format
//! let yaml = value.to_format(DataFormat::Yaml).unwrap();
//! ```

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::io::{BufRead, Write};
use std::path::Path;
use thiserror::Error;

/// Errors that can occur during data operations.
#[derive(Error, Debug)]
pub enum DataError {
    #[error("Failed to parse {format}: {message}")]
    ParseError { format: String, message: String },

    #[error("Failed to serialize to {format}: {message}")]
    SerializeError { format: String, message: String },

    #[error("Invalid query: {0}")]
    QueryError(String),

    #[error("Type mismatch: expected {expected}, got {actual}")]
    TypeError { expected: String, actual: String },

    #[error("Key not found: {0}")]
    KeyNotFound(String),

    #[error("Index out of bounds: {index} (length: {length})")]
    IndexOutOfBounds { index: usize, length: usize },

    #[error("Unsupported conversion from {from} to {to}")]
    UnsupportedConversion { from: String, to: String },

    #[error("CSV error: {0}")]
    CsvError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Schema mismatch: {0}")]
    SchemaMismatch(String),
}

pub type Result<T> = std::result::Result<T, DataError>;

/// Supported data formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataFormat {
    /// Plain text (line-based)
    Text,
    /// JSON (JavaScript Object Notation)
    Json,
    /// JSON Lines (newline-delimited JSON)
    JsonLines,
    /// CSV (Comma-Separated Values)
    Csv,
    /// TSV (Tab-Separated Values)
    Tsv,
    /// YAML (YAML Ain't Markup Language)
    Yaml,
    /// XML (eXtensible Markup Language)
    Xml,
    /// TOML (Tom's Obvious Minimal Language)
    Toml,
}

impl DataFormat {
    /// Detect format from file extension.
    pub fn from_extension(path: impl AsRef<Path>) -> Option<Self> {
        let ext = path.as_ref().extension()?.to_str()?.to_lowercase();
        match ext.as_str() {
            "json" => Some(Self::Json),
            "jsonl" | "ndjson" => Some(Self::JsonLines),
            "csv" => Some(Self::Csv),
            "tsv" => Some(Self::Tsv),
            "yaml" | "yml" => Some(Self::Yaml),
            "xml" => Some(Self::Xml),
            "toml" => Some(Self::Toml),
            "txt" | "log" => Some(Self::Text),
            _ => None,
        }
    }

    /// Detect format from content (heuristic).
    pub fn detect(content: &str) -> Self {
        let trimmed = content.trim();

        // Check for JSON
        if (trimmed.starts_with('{') && trimmed.ends_with('}'))
            || (trimmed.starts_with('[') && trimmed.ends_with(']'))
        {
            return Self::Json;
        }

        // Check for JSON Lines
        if trimmed.lines().all(|line| {
            let l = line.trim();
            l.is_empty() || l.starts_with('{') || l.starts_with('[')
        }) && trimmed.lines().filter(|l| !l.trim().is_empty()).count() > 1
        {
            return Self::JsonLines;
        }

        // Check for XML
        if trimmed.starts_with("<?xml") || trimmed.starts_with('<') {
            return Self::Xml;
        }

        // Check for YAML (common indicators)
        if trimmed.starts_with("---")
            || trimmed.contains(": ")
            || trimmed.lines().any(|l| l.starts_with("- "))
        {
            // Could be YAML or TOML, check for TOML indicators
            if trimmed.contains("[") && trimmed.lines().any(|l| l.trim().starts_with('[')) {
                // Check if it looks like TOML sections
                let has_toml_section = trimmed
                    .lines()
                    .any(|l| l.trim().starts_with('[') && l.trim().ends_with(']'));
                if has_toml_section {
                    return Self::Toml;
                }
            }
            return Self::Yaml;
        }

        // Check for CSV/TSV
        let first_lines: Vec<&str> = trimmed.lines().take(5).collect();
        if first_lines.len() > 1 {
            let comma_counts: Vec<usize> = first_lines.iter().map(|l| l.matches(',').count()).collect();
            let tab_counts: Vec<usize> = first_lines.iter().map(|l| l.matches('\t').count()).collect();

            // Consistent comma counts suggest CSV
            if comma_counts.iter().all(|&c| c == comma_counts[0]) && comma_counts[0] > 0 {
                return Self::Csv;
            }

            // Consistent tab counts suggest TSV
            if tab_counts.iter().all(|&c| c == tab_counts[0]) && tab_counts[0] > 0 {
                return Self::Tsv;
            }
        }

        // Default to plain text
        Self::Text
    }

    /// Get the MIME type for this format.
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Text => "text/plain",
            Self::Json => "application/json",
            Self::JsonLines => "application/x-ndjson",
            Self::Csv => "text/csv",
            Self::Tsv => "text/tab-separated-values",
            Self::Yaml => "application/x-yaml",
            Self::Xml => "application/xml",
            Self::Toml => "application/toml",
        }
    }

    /// Get the typical file extension for this format.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Text => "txt",
            Self::Json => "json",
            Self::JsonLines => "jsonl",
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            Self::Yaml => "yaml",
            Self::Xml => "xml",
            Self::Toml => "toml",
        }
    }

    /// Check if this format is structured (vs plain text).
    pub fn is_structured(&self) -> bool {
        !matches!(self, Self::Text)
    }

    /// Check if this format is tabular.
    pub fn is_tabular(&self) -> bool {
        matches!(self, Self::Csv | Self::Tsv)
    }
}

impl fmt::Display for DataFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::Json => write!(f, "json"),
            Self::JsonLines => write!(f, "jsonl"),
            Self::Csv => write!(f, "csv"),
            Self::Tsv => write!(f, "tsv"),
            Self::Yaml => write!(f, "yaml"),
            Self::Xml => write!(f, "xml"),
            Self::Toml => write!(f, "toml"),
        }
    }
}

impl std::str::FromStr for DataFormat {
    type Err = DataError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "text" | "txt" | "plain" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            "jsonl" | "jsonlines" | "ndjson" => Ok(Self::JsonLines),
            "csv" => Ok(Self::Csv),
            "tsv" => Ok(Self::Tsv),
            "yaml" | "yml" => Ok(Self::Yaml),
            "xml" => Ok(Self::Xml),
            "toml" => Ok(Self::Toml),
            _ => Err(DataError::ParseError {
                format: "DataFormat".to_string(),
                message: format!("Unknown format: {}", s),
            }),
        }
    }
}

/// A universal data value that can represent any structured data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(untagged)]
pub enum DataValue {
    /// Null value
    #[default]
    Null,
    /// Boolean value
    Bool(bool),
    /// Integer value
    Integer(i64),
    /// Floating-point value
    Float(f64),
    /// String value
    String(String),
    /// Array/list of values
    Array(Vec<DataValue>),
    /// Object/map of key-value pairs
    Object(BTreeMap<String, DataValue>),
}

impl DataValue {
    /// Parse data from a string in the specified format.
    pub fn parse(content: &str, format: DataFormat) -> Result<Self> {
        match format {
            DataFormat::Text => Ok(Self::String(content.to_string())),
            DataFormat::Json => Self::from_json(content),
            DataFormat::JsonLines => Self::from_json_lines(content),
            DataFormat::Csv => Self::from_csv(content, b','),
            DataFormat::Tsv => Self::from_csv(content, b'\t'),
            DataFormat::Yaml => Self::from_yaml(content),
            DataFormat::Xml => Self::from_xml(content),
            DataFormat::Toml => Self::from_toml(content),
        }
    }

    /// Parse JSON string.
    pub fn from_json(content: &str) -> Result<Self> {
        serde_json::from_str(content).map_err(|e| DataError::ParseError {
            format: "JSON".to_string(),
            message: e.to_string(),
        })
    }

    /// Parse JSON Lines (newline-delimited JSON).
    pub fn from_json_lines(content: &str) -> Result<Self> {
        let values: std::result::Result<Vec<DataValue>, _> = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(Self::from_json)
            .collect();
        Ok(Self::Array(values?))
    }

    /// Parse CSV content.
    pub fn from_csv(content: &str, delimiter: u8) -> Result<Self> {
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(delimiter)
            .has_headers(true)
            .from_reader(content.as_bytes());

        let headers: Vec<String> = reader
            .headers()
            .map_err(|e| DataError::CsvError(e.to_string()))?
            .iter()
            .map(|h| h.to_string())
            .collect();

        let mut records = Vec::new();
        for result in reader.records() {
            let record = result.map_err(|e| DataError::CsvError(e.to_string()))?;
            let mut obj = BTreeMap::new();
            for (i, field) in record.iter().enumerate() {
                let key = headers.get(i).cloned().unwrap_or_else(|| format!("field_{}", i));
                obj.insert(key, Self::infer_type(field));
            }
            records.push(Self::Object(obj));
        }

        Ok(Self::Array(records))
    }

    /// Parse YAML content.
    pub fn from_yaml(content: &str) -> Result<Self> {
        serde_yaml::from_str(content).map_err(|e| DataError::ParseError {
            format: "YAML".to_string(),
            message: e.to_string(),
        })
    }

    /// Parse XML content.
    pub fn from_xml(content: &str) -> Result<Self> {
        // Simple XML to DataValue conversion
        // Uses quick-xml for parsing
        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(content);
        reader.config_mut().trim_text(true);

        fn parse_element(reader: &mut Reader<&[u8]>, start_name: &str) -> Result<DataValue> {
            let mut children: BTreeMap<String, Vec<DataValue>> = BTreeMap::new();
            let mut text_content = String::new();

            loop {
                match reader.read_event() {
                    Ok(Event::Start(e)) => {
                        let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                        let child = parse_element(reader, &name)?;
                        children.entry(name).or_default().push(child);
                    }
                    Ok(Event::Empty(e)) => {
                        let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                        children.entry(name).or_default().push(DataValue::Null);
                    }
                    Ok(Event::Text(e)) => {
                        text_content.push_str(&e.unescape().unwrap_or_default());
                    }
                    Ok(Event::End(e)) => {
                        let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                        if name == start_name {
                            break;
                        }
                    }
                    Ok(Event::Eof) => break,
                    Err(e) => {
                        return Err(DataError::ParseError {
                            format: "XML".to_string(),
                            message: e.to_string(),
                        })
                    }
                    _ => {}
                }
            }

            if children.is_empty() {
                if text_content.is_empty() {
                    Ok(DataValue::Null)
                } else {
                    Ok(DataValue::infer_type(&text_content))
                }
            } else {
                let mut obj = BTreeMap::new();
                for (key, values) in children {
                    if values.len() == 1 {
                        obj.insert(key, values.into_iter().next().unwrap());
                    } else {
                        obj.insert(key, DataValue::Array(values));
                    }
                }
                if !text_content.is_empty() {
                    obj.insert("#text".to_string(), DataValue::String(text_content));
                }
                Ok(DataValue::Object(obj))
            }
        }

        let mut root_name = String::new();
        let mut root_value = DataValue::Null;

        loop {
            match reader.read_event() {
                Ok(Event::Start(e)) => {
                    root_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    root_value = parse_element(&mut reader, &root_name)?;
                    break;
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(e) => {
                    return Err(DataError::ParseError {
                        format: "XML".to_string(),
                        message: e.to_string(),
                    })
                }
            }
        }

        if root_name.is_empty() {
            Ok(root_value)
        } else {
            let mut obj = BTreeMap::new();
            obj.insert(root_name, root_value);
            Ok(DataValue::Object(obj))
        }
    }

    /// Parse TOML content.
    pub fn from_toml(content: &str) -> Result<Self> {
        toml::from_str(content).map_err(|e| DataError::ParseError {
            format: "TOML".to_string(),
            message: e.to_string(),
        })
    }

    /// Infer the type of a string value.
    fn infer_type(s: &str) -> Self {
        let trimmed = s.trim();

        // Check for null
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("null") || trimmed.eq_ignore_ascii_case("none") {
            return Self::Null;
        }

        // Check for boolean
        if trimmed.eq_ignore_ascii_case("true") {
            return Self::Bool(true);
        }
        if trimmed.eq_ignore_ascii_case("false") {
            return Self::Bool(false);
        }

        // Check for integer
        if let Ok(i) = trimmed.parse::<i64>() {
            return Self::Integer(i);
        }

        // Check for float
        if let Ok(f) = trimmed.parse::<f64>() {
            return Self::Float(f);
        }

        // Default to string
        Self::String(s.to_string())
    }

    /// Convert to the specified format.
    pub fn to_format(&self, format: DataFormat) -> Result<String> {
        self.to_format_with_options(format, true)
    }

    /// Convert to the specified format with optional pretty printing.
    pub fn to_format_with_options(&self, format: DataFormat, pretty: bool) -> Result<String> {
        match format {
            DataFormat::Text => self.to_text(),
            DataFormat::Json => {
                if pretty {
                    self.to_json()
                } else {
                    self.to_json_compact()
                }
            }
            DataFormat::JsonLines => self.to_json_lines(),
            DataFormat::Csv => self.to_csv(b','),
            DataFormat::Tsv => self.to_csv(b'\t'),
            DataFormat::Yaml => self.to_yaml(),
            DataFormat::Xml => {
                if pretty {
                    self.to_xml()
                } else {
                    self.to_xml_compact()
                }
            }
            DataFormat::Toml => self.to_toml(),
        }
    }

    /// Convert to plain text.
    pub fn to_text(&self) -> Result<String> {
        match self {
            Self::Null => Ok("".to_string()),
            Self::Bool(b) => Ok(b.to_string()),
            Self::Integer(i) => Ok(i.to_string()),
            Self::Float(f) => Ok(f.to_string()),
            Self::String(s) => Ok(s.clone()),
            Self::Array(arr) => {
                let lines: Vec<String> = arr.iter().map(|v| v.to_text()).collect::<Result<_>>()?;
                Ok(lines.join("\n"))
            }
            Self::Object(obj) => {
                let lines: Vec<String> = obj
                    .iter()
                    .map(|(k, v)| Ok(format!("{}: {}", k, v.to_text()?)))
                    .collect::<Result<_>>()?;
                Ok(lines.join("\n"))
            }
        }
    }

    /// Convert to JSON.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| DataError::SerializeError {
            format: "JSON".to_string(),
            message: e.to_string(),
        })
    }

    /// Convert to JSON (compact, no pretty printing).
    pub fn to_json_compact(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| DataError::SerializeError {
            format: "JSON".to_string(),
            message: e.to_string(),
        })
    }

    /// Convert to JSON Lines.
    pub fn to_json_lines(&self) -> Result<String> {
        match self {
            Self::Array(arr) => {
                let lines: Vec<String> = arr
                    .iter()
                    .map(|v| v.to_json_compact())
                    .collect::<Result<_>>()?;
                Ok(lines.join("\n"))
            }
            _ => self.to_json_compact(),
        }
    }

    /// Convert to CSV/TSV.
    pub fn to_csv(&self, delimiter: u8) -> Result<String> {
        match self {
            Self::Array(records) => {
                if records.is_empty() {
                    return Ok(String::new());
                }

                // Collect all headers from all records
                let mut headers: Vec<String> = Vec::new();
                for record in records {
                    if let Self::Object(obj) = record {
                        for key in obj.keys() {
                            if !headers.contains(key) {
                                headers.push(key.clone());
                            }
                        }
                    }
                }

                let mut writer = csv::WriterBuilder::new()
                    .delimiter(delimiter)
                    .from_writer(Vec::new());

                // Write headers
                writer
                    .write_record(&headers)
                    .map_err(|e| DataError::CsvError(e.to_string()))?;

                // Write records
                for record in records {
                    if let Self::Object(obj) = record {
                        let row: Vec<String> = headers
                            .iter()
                            .map(|h| {
                                obj.get(h)
                                    .map(|v| v.to_text().unwrap_or_default())
                                    .unwrap_or_default()
                            })
                            .collect();
                        writer
                            .write_record(&row)
                            .map_err(|e| DataError::CsvError(e.to_string()))?;
                    }
                }

                let data = writer
                    .into_inner()
                    .map_err(|e| DataError::CsvError(e.to_string()))?;
                String::from_utf8(data).map_err(|e| DataError::CsvError(e.to_string()))
            }
            _ => Err(DataError::UnsupportedConversion {
                from: "non-array".to_string(),
                to: "CSV".to_string(),
            }),
        }
    }

    /// Convert to YAML.
    pub fn to_yaml(&self) -> Result<String> {
        serde_yaml::to_string(self).map_err(|e| DataError::SerializeError {
            format: "YAML".to_string(),
            message: e.to_string(),
        })
    }

    /// Convert to XML.
    pub fn to_xml(&self) -> Result<String> {
        fn value_to_xml(value: &DataValue, name: &str, indent: usize) -> String {
            let spaces = "  ".repeat(indent);
            match value {
                DataValue::Null => format!("{}<{}/>\n", spaces, name),
                DataValue::Bool(b) => format!("{}<{}>{}</{}>\n", spaces, name, b, name),
                DataValue::Integer(i) => format!("{}<{}>{}</{}>\n", spaces, name, i, name),
                DataValue::Float(f) => format!("{}<{}>{}</{}>\n", spaces, name, f, name),
                DataValue::String(s) => {
                    let escaped = s
                        .replace('&', "&amp;")
                        .replace('<', "&lt;")
                        .replace('>', "&gt;");
                    format!("{}<{}>{}</{}>\n", spaces, name, escaped, name)
                }
                DataValue::Array(arr) => {
                    let mut output = String::new();
                    for item in arr {
                        output.push_str(&value_to_xml(item, "item", indent));
                    }
                    if output.is_empty() {
                        format!("{}<{}/>\n", spaces, name)
                    } else {
                        format!("{}<{}>\n{}{}</{}>\n", spaces, name, output, spaces, name)
                    }
                }
                DataValue::Object(obj) => {
                    let mut output = String::new();
                    for (key, val) in obj {
                        output.push_str(&value_to_xml(val, key, indent + 1));
                    }
                    if output.is_empty() {
                        format!("{}<{}/>\n", spaces, name)
                    } else {
                        format!("{}<{}>\n{}{}</{}>\n", spaces, name, output, spaces, name)
                    }
                }
            }
        }

        let mut output = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        output.push_str(&value_to_xml(self, "root", 0));
        Ok(output)
    }

    /// Convert to XML (compact, no newlines/indentation).
    pub fn to_xml_compact(&self) -> Result<String> {
        fn value_to_xml_compact(value: &DataValue, name: &str) -> String {
            match value {
                DataValue::Null => format!("<{}/>", name),
                DataValue::Bool(b) => format!("<{}>{}</{}>", name, b, name),
                DataValue::Integer(i) => format!("<{}>{}</{}>", name, i, name),
                DataValue::Float(f) => format!("<{}>{}</{}>", name, f, name),
                DataValue::String(s) => {
                    let escaped = s
                        .replace('&', "&amp;")
                        .replace('<', "&lt;")
                        .replace('>', "&gt;");
                    format!("<{}>{}</{}>", name, escaped, name)
                }
                DataValue::Array(arr) => {
                    let mut output = String::new();
                    for item in arr {
                        output.push_str(&value_to_xml_compact(item, "item"));
                    }
                    if output.is_empty() {
                        format!("<{}/>", name)
                    } else {
                        format!("<{}>{}</{}>", name, output, name)
                    }
                }
                DataValue::Object(obj) => {
                    let mut output = String::new();
                    for (key, val) in obj {
                        output.push_str(&value_to_xml_compact(val, key));
                    }
                    if output.is_empty() {
                        format!("<{}/>", name)
                    } else {
                        format!("<{}>{}</{}>", name, output, name)
                    }
                }
            }
        }

        let mut output = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
        output.push_str(&value_to_xml_compact(self, "root"));
        Ok(output)
    }

    /// Convert to TOML.
    pub fn to_toml(&self) -> Result<String> {
        // TOML requires a table at the root
        let value = match self {
            Self::Object(_) => self.clone(),
            _ => {
                let mut obj = BTreeMap::new();
                obj.insert("value".to_string(), self.clone());
                Self::Object(obj)
            }
        };

        toml::to_string_pretty(&value).map_err(|e| DataError::SerializeError {
            format: "TOML".to_string(),
            message: e.to_string(),
        })
    }

    /// Query a value using a path expression.
    ///
    /// Path syntax:
    /// - `.key` - Access object key
    /// - `[0]` - Access array index
    /// - `.key.subkey` - Nested access
    /// - `.[*]` or `.[]` - All array elements
    pub fn query(&self, path: &str) -> Result<DataValue> {
        if path.is_empty() || path == "." {
            return Ok(self.clone());
        }

        let path = path.strip_prefix('.').unwrap_or(path);
        self.query_parts(&parse_path(path)?)
    }

    fn query_parts(&self, parts: &[PathPart]) -> Result<DataValue> {
        if parts.is_empty() {
            return Ok(self.clone());
        }

        let (first, rest) = parts.split_first().unwrap();

        match (self, first) {
            (Self::Object(obj), PathPart::Key(key)) => {
                let value = obj
                    .get(key)
                    .ok_or_else(|| DataError::KeyNotFound(key.clone()))?;
                value.query_parts(rest)
            }
            (Self::Array(arr), PathPart::Index(idx)) => {
                let value = arr.get(*idx).ok_or(DataError::IndexOutOfBounds {
                    index: *idx,
                    length: arr.len(),
                })?;
                value.query_parts(rest)
            }
            (Self::Array(arr), PathPart::Wildcard) => {
                let results: Vec<DataValue> = arr
                    .iter()
                    .map(|v| v.query_parts(rest))
                    .collect::<Result<_>>()?;
                Ok(Self::Array(results))
            }
            _ => Err(DataError::TypeError {
                expected: format!("{:?}", first),
                actual: self.type_name().to_string(),
            }),
        }
    }

    /// Get the type name of this value.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Integer(_) => "integer",
            Self::Float(_) => "float",
            Self::String(_) => "string",
            Self::Array(_) => "array",
            Self::Object(_) => "object",
        }
    }

    /// Check if the value is null.
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Get as boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Get as integer.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            Self::Float(f) => Some(*f as i64),
            _ => None,
        }
    }

    /// Get as float.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            Self::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Get as string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// Get as array.
    pub fn as_array(&self) -> Option<&Vec<DataValue>> {
        match self {
            Self::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Get as mutable array.
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<DataValue>> {
        match self {
            Self::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Get as object.
    pub fn as_object(&self) -> Option<&BTreeMap<String, DataValue>> {
        match self {
            Self::Object(obj) => Some(obj),
            _ => None,
        }
    }

    /// Get as mutable object.
    pub fn as_object_mut(&mut self) -> Option<&mut BTreeMap<String, DataValue>> {
        match self {
            Self::Object(obj) => Some(obj),
            _ => None,
        }
    }

    /// Get a nested value by key.
    pub fn get(&self, key: &str) -> Option<&DataValue> {
        match self {
            Self::Object(obj) => obj.get(key),
            _ => None,
        }
    }

    /// Get a nested value by index.
    pub fn get_index(&self, index: usize) -> Option<&DataValue> {
        match self {
            Self::Array(arr) => arr.get(index),
            _ => None,
        }
    }

    /// Set a value at a path.
    pub fn set(&mut self, path: &str, value: DataValue) -> Result<()> {
        if path.is_empty() || path == "." {
            *self = value;
            return Ok(());
        }

        let path = path.strip_prefix('.').unwrap_or(path);
        let parts = parse_path(path)?;
        self.set_parts(&parts, value)
    }

    fn set_parts(&mut self, parts: &[PathPart], value: DataValue) -> Result<()> {
        if parts.is_empty() {
            *self = value;
            return Ok(());
        }

        let (first, rest) = parts.split_first().unwrap();

        if rest.is_empty() {
            // Last part - set the value
            match first {
                PathPart::Key(key) => {
                    if let Self::Object(obj) = self {
                        obj.insert(key.clone(), value);
                        Ok(())
                    } else {
                        Err(DataError::TypeError {
                            expected: "object".to_string(),
                            actual: self.type_name().to_string(),
                        })
                    }
                }
                PathPart::Index(idx) => {
                    if let Self::Array(arr) = self {
                        if *idx < arr.len() {
                            arr[*idx] = value;
                            Ok(())
                        } else {
                            Err(DataError::IndexOutOfBounds {
                                index: *idx,
                                length: arr.len(),
                            })
                        }
                    } else {
                        Err(DataError::TypeError {
                            expected: "array".to_string(),
                            actual: self.type_name().to_string(),
                        })
                    }
                }
                PathPart::Wildcard => Err(DataError::QueryError(
                    "Wildcard not supported in set".to_string(),
                )),
            }
        } else {
            // Navigate deeper
            match first {
                PathPart::Key(key) => {
                    if let Self::Object(obj) = self {
                        let entry = obj.entry(key.clone()).or_insert(DataValue::Object(BTreeMap::new()));
                        entry.set_parts(rest, value)
                    } else {
                        Err(DataError::TypeError {
                            expected: "object".to_string(),
                            actual: self.type_name().to_string(),
                        })
                    }
                }
                PathPart::Index(idx) => {
                    if let Self::Array(arr) = self {
                        let len = arr.len();
                        let element = arr.get_mut(*idx).ok_or(DataError::IndexOutOfBounds {
                            index: *idx,
                            length: len,
                        })?;
                        element.set_parts(rest, value)
                    } else {
                        Err(DataError::TypeError {
                            expected: "array".to_string(),
                            actual: self.type_name().to_string(),
                        })
                    }
                }
                PathPart::Wildcard => Err(DataError::QueryError(
                    "Wildcard not supported in set".to_string(),
                )),
            }
        }
    }

    /// Get the length of an array or object, or string.
    pub fn len(&self) -> usize {
        match self {
            Self::Array(arr) => arr.len(),
            Self::Object(obj) => obj.len(),
            Self::String(s) => s.len(),
            _ => 0,
        }
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Iterate over array elements or object values.
    pub fn iter(&self) -> DataValueIter<'_> {
        match self {
            Self::Array(arr) => DataValueIter::Array(arr.iter()),
            Self::Object(obj) => DataValueIter::Object(obj.values()),
            _ => DataValueIter::Single(std::iter::once(self)),
        }
    }

    /// Get keys if this is an object.
    pub fn keys(&self) -> Option<impl Iterator<Item = &String>> {
        match self {
            Self::Object(obj) => Some(obj.keys()),
            _ => None,
        }
    }
}

/// Iterator over DataValue contents.
pub enum DataValueIter<'a> {
    Array(std::slice::Iter<'a, DataValue>),
    Object(std::collections::btree_map::Values<'a, String, DataValue>),
    Single(std::iter::Once<&'a DataValue>),
}

impl<'a> Iterator for DataValueIter<'a> {
    type Item = &'a DataValue;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Array(iter) => iter.next(),
            Self::Object(iter) => iter.next(),
            Self::Single(iter) => iter.next(),
        }
    }
}

/// Path part for query navigation.
#[derive(Debug, Clone)]
enum PathPart {
    Key(String),
    Index(usize),
    Wildcard,
}

/// Parse a path string into parts.
fn parse_path(path: &str) -> Result<Vec<PathPart>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = path.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '.' => {
                if !current.is_empty() {
                    parts.push(PathPart::Key(current.clone()));
                    current.clear();
                }
            }
            '[' => {
                if !current.is_empty() {
                    parts.push(PathPart::Key(current.clone()));
                    current.clear();
                }
                let mut index_str = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ']' {
                        chars.next();
                        break;
                    }
                    index_str.push(chars.next().unwrap());
                }
                if index_str.is_empty() || index_str == "*" {
                    parts.push(PathPart::Wildcard);
                } else {
                    let idx = index_str.parse::<usize>().map_err(|_| {
                        DataError::QueryError(format!("Invalid index: {}", index_str))
                    })?;
                    parts.push(PathPart::Index(idx));
                }
            }
            _ => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        parts.push(PathPart::Key(current));
    }

    Ok(parts)
}

/// A streaming data reader for processing large files.
pub struct DataReader<R: BufRead> {
    reader: R,
    format: DataFormat,
    buffer: Vec<DataValue>,
}

impl<R: BufRead> DataReader<R> {
    /// Create a new data reader.
    pub fn new(reader: R, format: DataFormat) -> Self {
        Self {
            reader,
            format,
            buffer: Vec::new(),
        }
    }

    /// Read the next value.
    pub fn next_value(&mut self) -> Result<Option<DataValue>> {
        match self.format {
            DataFormat::Text => {
                let mut line = String::new();
                match self.reader.read_line(&mut line) {
                    Ok(0) => Ok(None),
                    Ok(_) => Ok(Some(DataValue::String(line.trim_end().to_string()))),
                    Err(e) => Err(DataError::IoError(e)),
                }
            }
            DataFormat::JsonLines => {
                let mut line = String::new();
                loop {
                    line.clear();
                    match self.reader.read_line(&mut line) {
                        Ok(0) => return Ok(None),
                        Ok(_) => {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                return DataValue::from_json(trimmed).map(Some);
                            }
                        }
                        Err(e) => return Err(DataError::IoError(e)),
                    }
                }
            }
            _ => {
                // For other formats, read entire content
                if self.buffer.is_empty() {
                    let mut content = String::new();
                    self.reader.read_to_string(&mut content)?;
                    if content.is_empty() {
                        return Ok(None);
                    }
                    let value = DataValue::parse(&content, self.format)?;
                    match value {
                        DataValue::Array(arr) => {
                            self.buffer = arr.into_iter().rev().collect();
                        }
                        other => {
                            self.buffer.push(other);
                        }
                    }
                }
                Ok(self.buffer.pop())
            }
        }
    }
}

impl<R: BufRead> Iterator for DataReader<R> {
    type Item = Result<DataValue>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_value() {
            Ok(Some(value)) => Some(Ok(value)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

/// A streaming data writer.
pub struct DataWriter<W: Write> {
    writer: W,
    format: DataFormat,
    first: bool,
    csv_headers: Option<Vec<String>>,
}

impl<W: Write> DataWriter<W> {
    /// Create a new data writer.
    pub fn new(writer: W, format: DataFormat) -> Self {
        Self {
            writer,
            format,
            first: true,
            csv_headers: None,
        }
    }

    /// Write a value.
    pub fn write_value(&mut self, value: &DataValue) -> Result<()> {
        match self.format {
            DataFormat::Text => {
                writeln!(self.writer, "{}", value.to_text()?)?;
            }
            DataFormat::Json => {
                if self.first {
                    writeln!(self.writer, "[")?;
                    self.first = false;
                } else {
                    writeln!(self.writer, ",")?;
                }
                write!(self.writer, "  {}", value.to_json_compact()?)?;
            }
            DataFormat::JsonLines => {
                writeln!(self.writer, "{}", value.to_json_compact()?)?;
            }
            DataFormat::Csv | DataFormat::Tsv => {
                let delimiter = if self.format == DataFormat::Csv { b',' } else { b'\t' };
                if let DataValue::Object(obj) = value {
                    if self.csv_headers.is_none() {
                        let headers: Vec<String> = obj.keys().cloned().collect();
                        let header_line: Vec<&str> = headers.iter().map(|s| s.as_str()).collect();
                        let mut wtr = csv::WriterBuilder::new()
                            .delimiter(delimiter)
                            .from_writer(Vec::new());
                        wtr.write_record(&header_line)
                            .map_err(|e| DataError::CsvError(e.to_string()))?;
                        let data = wtr.into_inner().map_err(|e| DataError::CsvError(e.to_string()))?;
                        self.writer.write_all(&data)?;
                        self.csv_headers = Some(headers);
                    }

                    if let Some(headers) = &self.csv_headers {
                        let row: Vec<String> = headers
                            .iter()
                            .map(|h| {
                                obj.get(h)
                                    .map(|v| v.to_text().unwrap_or_default())
                                    .unwrap_or_default()
                            })
                            .collect();
                        let mut wtr = csv::WriterBuilder::new()
                            .delimiter(delimiter)
                            .from_writer(Vec::new());
                        wtr.write_record(&row)
                            .map_err(|e| DataError::CsvError(e.to_string()))?;
                        let data = wtr.into_inner().map_err(|e| DataError::CsvError(e.to_string()))?;
                        self.writer.write_all(&data)?;
                    }
                }
            }
            DataFormat::Yaml => {
                if !self.first {
                    writeln!(self.writer, "---")?;
                }
                self.first = false;
                write!(self.writer, "{}", value.to_yaml()?)?;
            }
            DataFormat::Xml => {
                write!(self.writer, "{}", value.to_xml()?)?;
            }
            DataFormat::Toml => {
                if !self.first {
                    writeln!(self.writer)?;
                }
                self.first = false;
                write!(self.writer, "{}", value.to_toml()?)?;
            }
        }
        Ok(())
    }

    /// Finish writing (close arrays, etc.).
    pub fn finish(mut self) -> Result<W> {
        if self.format == DataFormat::Json && !self.first {
            writeln!(self.writer)?;
            writeln!(self.writer, "]")?;
        }
        Ok(self.writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_detection() {
        assert_eq!(DataFormat::detect(r#"{"key": "value"}"#), DataFormat::Json);
        assert_eq!(DataFormat::detect(r#"[1, 2, 3]"#), DataFormat::Json);
        assert_eq!(DataFormat::detect("<?xml version=\"1.0\"?>"), DataFormat::Xml);
        assert_eq!(DataFormat::detect("<root></root>"), DataFormat::Xml);
        assert_eq!(DataFormat::detect("key: value"), DataFormat::Yaml);
        assert_eq!(DataFormat::detect("- item1\n- item2"), DataFormat::Yaml);
        assert_eq!(DataFormat::detect("a,b,c\n1,2,3"), DataFormat::Csv);
        assert_eq!(DataFormat::detect("a\tb\tc\n1\t2\t3"), DataFormat::Tsv);
    }

    #[test]
    fn test_json_parsing() {
        let json = r#"{"name": "Alice", "age": 30, "active": true}"#;
        let value = DataValue::from_json(json).unwrap();

        assert_eq!(value.get("name").and_then(|v| v.as_str()), Some("Alice"));
        assert_eq!(value.get("age").and_then(|v| v.as_i64()), Some(30));
        assert_eq!(value.get("active").and_then(|v| v.as_bool()), Some(true));
    }

    #[test]
    fn test_csv_parsing() {
        let csv = "name,age,city\nAlice,30,NYC\nBob,25,LA";
        let value = DataValue::from_csv(csv, b',').unwrap();

        let arr = value.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].get("name").and_then(|v| v.as_str()), Some("Alice"));
        assert_eq!(arr[1].get("age").and_then(|v| v.as_i64()), Some(25));
    }

    #[test]
    fn test_yaml_parsing() {
        let yaml = "name: Alice\nage: 30";
        let value = DataValue::from_yaml(yaml).unwrap();

        assert_eq!(value.get("name").and_then(|v| v.as_str()), Some("Alice"));
        assert_eq!(value.get("age").and_then(|v| v.as_i64()), Some(30));
    }

    #[test]
    fn test_query() {
        let json = r#"{"user": {"name": "Alice", "tags": ["a", "b", "c"]}}"#;
        let value = DataValue::from_json(json).unwrap();

        assert_eq!(
            value.query(".user.name").unwrap().as_str(),
            Some("Alice")
        );
        assert_eq!(
            value.query(".user.tags[1]").unwrap().as_str(),
            Some("b")
        );
    }

    #[test]
    fn test_format_conversion() {
        let json = r#"[{"name": "Alice", "age": 30}]"#;
        let value = DataValue::from_json(json).unwrap();

        // JSON to CSV
        let csv = value.to_csv(b',').unwrap();
        assert!(csv.contains("name"));
        assert!(csv.contains("Alice"));

        // JSON to YAML
        let yaml = value.to_yaml().unwrap();
        assert!(yaml.contains("Alice"));
    }

    #[test]
    fn test_type_inference() {
        assert!(matches!(DataValue::infer_type("123"), DataValue::Integer(123)));
        assert!(matches!(DataValue::infer_type("12.5"), DataValue::Float(_)));
        assert!(matches!(DataValue::infer_type("true"), DataValue::Bool(true)));
        assert!(matches!(DataValue::infer_type("null"), DataValue::Null));
        assert!(matches!(DataValue::infer_type("hello"), DataValue::String(_)));
    }

    #[test]
    fn test_set_value() {
        let mut value = DataValue::from_json(r#"{"user": {"name": "Alice"}}"#).unwrap();
        value.set(".user.name", DataValue::String("Bob".to_string())).unwrap();
        assert_eq!(value.query(".user.name").unwrap().as_str(), Some("Bob"));
    }
}
