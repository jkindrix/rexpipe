//! Pattern Library Module
//!
//! Provides support for reusable regex pattern libraries that can be shared
//! across multiple pipeline configurations.
//!
//! # Example Library Format
//!
//! ```toml
//! name = "My Patterns"
//! version = "1.0.0"
//!
//! [patterns]
//! simple_pattern = '^\d+'
//!
//! [patterns.category]
//! nested_pattern = '^\w+'
//! ```
//!
//! # Usage in Pipeline Config
//!
//! ```toml
//! patterns_include = ["my-patterns.toml"]
//!
//! [[step]]
//! pattern = '${simple_pattern}'
//! # or
//! pattern = '${category.nested_pattern}'
//! ```

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Pattern library configuration as loaded from TOML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternLibrary {
    /// Library name
    pub name: Option<String>,
    /// Library description
    pub description: Option<String>,
    /// Library version
    pub version: Option<String>,
    /// Other libraries to include (supports nesting)
    #[serde(default)]
    pub patterns_include: Vec<String>,
    /// Pattern definitions (can be nested)
    #[serde(default)]
    pub patterns: HashMap<String, PatternValue>,
}

/// Pattern value - either a direct pattern string or a nested map of patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PatternValue {
    /// A direct regex pattern string
    Pattern(String),
    /// A nested map of patterns (for categorization)
    Nested(HashMap<String, PatternValue>),
}

/// Resolved pattern library with all patterns flattened to dot notation
#[derive(Debug, Clone, Default)]
pub struct ResolvedLibrary {
    /// Flattened patterns: "category.name" -> "pattern"
    pub patterns: HashMap<String, String>,
    /// Source files that contributed patterns (for error messages)
    pub source_files: Vec<PathBuf>,
}

impl ResolvedLibrary {
    /// Create a new empty resolved library
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a pattern by name
    pub fn get(&self, name: &str) -> Option<&String> {
        self.patterns.get(name)
    }

    /// Check if a pattern exists
    #[allow(dead_code)]
    pub fn contains(&self, name: &str) -> bool {
        self.patterns.contains_key(name)
    }

    /// Get all pattern names
    #[allow(dead_code)]
    pub fn pattern_names(&self) -> impl Iterator<Item = &String> {
        self.patterns.keys()
    }

    /// Merge another library into this one (other takes lower precedence)
    /// Emits warnings to stderr when patterns conflict
    pub fn merge(&mut self, other: ResolvedLibrary) {
        let self_source = self
            .source_files
            .first()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        for (name, pattern) in other.patterns {
            if self.patterns.contains_key(&name) {
                eprintln!(
                    "Warning: Pattern '{}' defined in multiple libraries, using definition from '{}'",
                    name, self_source
                );
            } else {
                self.patterns.insert(name, pattern);
            }
        }
        self.source_files.extend(other.source_files);
    }
}

/// Library resolver handles loading pattern libraries with circular reference detection
pub struct LibraryResolver {
    /// Paths to search for libraries
    search_paths: Vec<PathBuf>,
    /// Cache of loaded libraries by canonical path
    loaded: HashMap<PathBuf, PatternLibrary>,
    /// Stack of libraries currently being resolved (for cycle detection)
    resolution_stack: Vec<PathBuf>,
}

impl LibraryResolver {
    /// Create a new resolver with search paths
    ///
    /// Search order:
    /// 1. Relative to the config file (if base_path is Some)
    /// 2. Global ~/.rexpipe/patterns/ directory
    pub fn new(base_path: Option<&Path>) -> Self {
        let mut search_paths = Vec::new();

        // Add base path (relative to config file)
        if let Some(base) = base_path {
            search_paths.push(base.to_path_buf());
        }

        // Add global patterns directory
        if let Some(home) = dirs::home_dir() {
            search_paths.push(home.join(".rexpipe").join("patterns"));
        }

        Self {
            search_paths,
            loaded: HashMap::new(),
            resolution_stack: Vec::new(),
        }
    }

    /// Load and resolve multiple libraries into a single ResolvedLibrary
    pub fn load_libraries(
        &mut self,
        includes: &[String],
    ) -> Result<ResolvedLibrary, Box<dyn std::error::Error>> {
        let mut resolved = ResolvedLibrary::new();

        for include in includes {
            let path = self.find_library(include)?;
            let lib = self.load_library_recursive(&path)?;
            let flattened = self.flatten_library(&lib, &path)?;
            resolved.merge(flattened);
        }

        Ok(resolved)
    }

    /// Find a library file in the search paths
    fn find_library(&self, name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let name_path = Path::new(name);

        // If it's an absolute path, use it directly
        if name_path.is_absolute() && name_path.exists() {
            return Ok(name_path.to_path_buf());
        }

        // Search in each path
        for search_path in &self.search_paths {
            let candidate = search_path.join(name);
            if candidate.exists() {
                return Ok(candidate);
            }

            // Try with .toml extension if not present
            if candidate.extension().is_none() {
                let with_ext = candidate.with_extension("toml");
                if with_ext.exists() {
                    return Ok(with_ext);
                }
            }
        }

        // Build helpful error message
        let searched: Vec<String> = self
            .search_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect();

        Err(format!(
            "Pattern library not found: '{}' (searched: {})",
            name,
            searched.join(", ")
        )
        .into())
    }

    /// Load a library file recursively, handling nested includes
    fn load_library_recursive(
        &mut self,
        path: &Path,
    ) -> Result<PatternLibrary, Box<dyn std::error::Error>> {
        let canonical = path
            .canonicalize()
            .map_err(|e| format!("Failed to resolve path '{}': {}", path.display(), e))?;

        // Check for circular reference
        if self.resolution_stack.contains(&canonical) {
            let cycle: Vec<String> = self
                .resolution_stack
                .iter()
                .map(|p| p.display().to_string())
                .collect();
            return Err(format!(
                "Circular pattern library include detected: {} -> {}",
                cycle.join(" -> "),
                canonical.display()
            )
            .into());
        }

        // Check cache
        if let Some(lib) = self.loaded.get(&canonical) {
            return Ok(lib.clone());
        }

        // Mark as in-progress for cycle detection
        self.resolution_stack.push(canonical.clone());

        // Load and parse the library
        let content = fs::read_to_string(&canonical)
            .map_err(|e| format!("Failed to read library '{}': {}", canonical.display(), e))?;

        let library: PatternLibrary = toml::from_str(&content)
            .map_err(|e| format!("Failed to parse library '{}': {}", canonical.display(), e))?;

        // Process nested includes
        let parent = canonical.parent().unwrap_or(Path::new("."));
        for include in &library.patterns_include {
            // Resolve relative to current library's location
            let include_path = if Path::new(include).is_absolute() {
                PathBuf::from(include)
            } else {
                parent.join(include)
            };

            // Recursively load (this will detect cycles)
            let _nested = self.load_library_recursive(&include_path)?;
        }

        // Remove from resolution stack
        self.resolution_stack.pop();

        // Cache the loaded library
        self.loaded.insert(canonical, library.clone());

        Ok(library)
    }

    /// Flatten a library's patterns to dot notation
    fn flatten_library(
        &self,
        library: &PatternLibrary,
        source_path: &Path,
    ) -> Result<ResolvedLibrary, Box<dyn std::error::Error>> {
        let mut resolved = ResolvedLibrary::new();
        resolved.source_files.push(source_path.to_path_buf());

        // Flatten the patterns
        self.flatten_patterns(&library.patterns, "", &mut resolved.patterns);

        // Also include patterns from nested includes
        let canonical = source_path.canonicalize()?;
        let parent = canonical.parent().unwrap_or(Path::new("."));

        for include in &library.patterns_include {
            let include_path = if Path::new(include).is_absolute() {
                PathBuf::from(include)
            } else {
                parent.join(include)
            };

            if let Some(nested_lib) = self.loaded.get(&include_path.canonicalize()?) {
                let nested_resolved = self.flatten_library(nested_lib, &include_path)?;
                // Nested patterns have lower precedence (don't overwrite)
                for (name, pattern) in nested_resolved.patterns {
                    resolved.patterns.entry(name).or_insert(pattern);
                }
                resolved.source_files.extend(nested_resolved.source_files);
            }
        }

        Ok(resolved)
    }

    /// Recursively flatten patterns to dot notation
    fn flatten_patterns(
        &self,
        patterns: &HashMap<String, PatternValue>,
        prefix: &str,
        output: &mut HashMap<String, String>,
    ) {
        for (key, value) in patterns {
            let full_key = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", prefix, key)
            };

            match value {
                PatternValue::Pattern(pattern) => {
                    output.insert(full_key, pattern.clone());
                }
                PatternValue::Nested(nested) => {
                    self.flatten_patterns(nested, &full_key, output);
                }
            }
        }
    }

    /// Validate a library file without resolving includes
    pub fn validate_library(path: &Path) -> Result<PatternLibrary, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read library '{}': {}", path.display(), e))?;

        let library: PatternLibrary = toml::from_str(&content)
            .map_err(|e| format!("Failed to parse library '{}': {}", path.display(), e))?;

        // Validate that all pattern strings are valid regex
        let mut errors = Vec::new();
        Self::validate_patterns(&library.patterns, "", &mut errors);

        if !errors.is_empty() {
            return Err(format!(
                "Invalid patterns in library '{}':\n  {}",
                path.display(),
                errors.join("\n  ")
            )
            .into());
        }

        Ok(library)
    }

    /// Recursively validate patterns are valid regex
    fn validate_patterns(
        patterns: &HashMap<String, PatternValue>,
        prefix: &str,
        errors: &mut Vec<String>,
    ) {
        for (key, value) in patterns {
            let full_key = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", prefix, key)
            };

            match value {
                PatternValue::Pattern(pattern) => {
                    if let Err(e) = Regex::new(pattern) {
                        errors.push(format!("'{}': {}", full_key, e));
                    }
                }
                PatternValue::Nested(nested) => {
                    Self::validate_patterns(nested, &full_key, errors);
                }
            }
        }
    }
}

/// Resolve pattern references in a string
///
/// Replaces `${pattern_name}` with the actual pattern from the library.
/// Returns the resolved string and any errors encountered.
pub fn resolve_pattern_references(
    input: &str,
    library: &ResolvedLibrary,
) -> Result<String, Vec<String>> {
    let pattern_ref_regex = Regex::new(r"\$\{([a-zA-Z_][a-zA-Z0-9_.]*)\}").unwrap();
    let mut errors = Vec::new();
    let mut unresolved_refs = HashSet::new();

    let result = pattern_ref_regex
        .replace_all(input, |caps: &regex::Captures| {
            let ref_name = &caps[1];
            match library.get(ref_name) {
                Some(pattern) => pattern.clone(),
                None => {
                    if !unresolved_refs.contains(ref_name) {
                        errors.push(format!(
                            "Unknown pattern reference '${{{}}}' - not found in library",
                            ref_name
                        ));
                        unresolved_refs.insert(ref_name.to_string());
                    }
                    caps[0].to_string() // Keep original for error display
                }
            }
        })
        .into_owned();

    if errors.is_empty() {
        Ok(result)
    } else {
        Err(errors)
    }
}

/// Check if a string contains pattern references
pub fn has_pattern_references(input: &str) -> bool {
    input.contains("${")
}

/// List all patterns in a library file
pub fn list_patterns(path: &Path) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let library = LibraryResolver::validate_library(path)?;
    let mut patterns = Vec::new();

    fn collect_patterns(
        map: &HashMap<String, PatternValue>,
        prefix: &str,
        output: &mut Vec<(String, String)>,
    ) {
        for (key, value) in map {
            let full_key = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", prefix, key)
            };

            match value {
                PatternValue::Pattern(pattern) => {
                    output.push((full_key, pattern.clone()));
                }
                PatternValue::Nested(nested) => {
                    collect_patterns(nested, &full_key, output);
                }
            }
        }
    }

    collect_patterns(&library.patterns, "", &mut patterns);
    patterns.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(patterns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_value_deserialize_string() {
        let toml_str = r#"
            [patterns]
            simple = '^\d+'
        "#;
        let lib: PatternLibrary = toml::from_str(toml_str).unwrap();
        assert!(matches!(
            lib.patterns.get("simple"),
            Some(PatternValue::Pattern(_))
        ));
    }

    #[test]
    fn test_pattern_value_deserialize_nested() {
        let toml_str = r#"
            [patterns.category]
            nested = '^\w+'
        "#;
        let lib: PatternLibrary = toml::from_str(toml_str).unwrap();
        assert!(matches!(
            lib.patterns.get("category"),
            Some(PatternValue::Nested(_))
        ));
    }

    #[test]
    fn test_flatten_patterns() {
        let mut patterns = HashMap::new();
        patterns.insert(
            "simple".to_string(),
            PatternValue::Pattern("^simple$".to_string()),
        );

        let mut nested = HashMap::new();
        nested.insert(
            "inner".to_string(),
            PatternValue::Pattern("^inner$".to_string()),
        );
        patterns.insert("category".to_string(), PatternValue::Nested(nested));

        let resolver = LibraryResolver::new(None);
        let mut output = HashMap::new();
        resolver.flatten_patterns(&patterns, "", &mut output);

        assert_eq!(output.get("simple"), Some(&"^simple$".to_string()));
        assert_eq!(output.get("category.inner"), Some(&"^inner$".to_string()));
    }

    #[test]
    fn test_resolve_pattern_references() {
        let mut library = ResolvedLibrary::new();
        library
            .patterns
            .insert("digits".to_string(), r"^\d+$".to_string());
        library
            .patterns
            .insert("words".to_string(), r"^\w+$".to_string());

        let input = "Pattern: ${digits} or ${words}";
        let result = resolve_pattern_references(input, &library).unwrap();
        assert_eq!(result, r"Pattern: ^\d+$ or ^\w+$");
    }

    #[test]
    fn test_resolve_pattern_references_missing() {
        let library = ResolvedLibrary::new();
        let input = "Pattern: ${missing}";
        let result = resolve_pattern_references(input, &library);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors[0].contains("missing"));
    }

    #[test]
    fn test_has_pattern_references() {
        assert!(has_pattern_references("${foo}"));
        assert!(has_pattern_references("prefix ${foo} suffix"));
        assert!(!has_pattern_references("no references"));
        assert!(!has_pattern_references("$ {not a ref}"));
    }
}
