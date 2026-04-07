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

#[cfg(feature = "cli")]
use crate::error::LibraryError;
#[cfg(feature = "cli")]
use anyhow::Context;
use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
#[cfg(feature = "cli")]
use std::fs;
use std::path::PathBuf;
#[cfg(feature = "cli")]
use std::path::Path;
use std::sync::LazyLock;

/// Built-in pattern library for common regex patterns.
///
/// These patterns can be used in configs with `${builtin:pattern_name}` syntax.
/// For example: `pattern = "${builtin:email}"` expands to the email regex.
static BUILTIN_PATTERNS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    // Email and identity
    m.insert("email", r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}");
    m.insert("ipv4", r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b");
    m.insert("ipv6", r"\b([0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}\b");
    m.insert("phone_us", r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b");
    m.insert("ssn", r"\b\d{3}-\d{2}-\d{4}\b");

    // Dates and times
    m.insert("date_iso", r"\b\d{4}-\d{2}-\d{2}\b");
    m.insert("date_us", r"\b\d{1,2}/\d{1,2}/\d{4}\b");
    m.insert("time_24h", r"\b\d{1,2}:\d{2}(:\d{2})?\b");
    m.insert("datetime_iso", r"\b\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}");

    // Identifiers
    m.insert(
        "uuid",
        r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b",
    );
    m.insert("hex_id", r"\b[0-9a-fA-F]{8,}\b");
    m.insert("url", r#"https?://[^\s<>"']+"#);
    m.insert("credit_card", r"\b\d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}\b");
    m.insert("api_key", r"\b[A-Za-z0-9_-]{20,}\b");
    m.insert("base64", r"\b[A-Za-z0-9+/]{20,}={0,2}\b");

    // Common log patterns
    m.insert(
        "log_level",
        r"\b(DEBUG|INFO|WARN|WARNING|ERROR|FATAL|TRACE)\b",
    );
    m.insert(
        "timestamp_syslog",
        r"[A-Z][a-z]{2}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2}",
    );
    m.insert("json_object", r"\{[^{}]*\}");

    // Semantic versioning
    m.insert(
        "semver",
        r"\b\d+\.\d+\.\d+(-[a-zA-Z0-9.]+)?(\+[a-zA-Z0-9.]+)?\b",
    );
    m
});

/// Get a builtin pattern by name.
///
/// Returns `None` if the pattern name is not found.
pub fn get_builtin_pattern(name: &str) -> Option<&'static str> {
    BUILTIN_PATTERNS.get(name).copied()
}

/// List all available builtin pattern names.
pub fn list_builtin_patterns() -> Vec<&'static str> {
    let mut names: Vec<_> = BUILTIN_PATTERNS.keys().copied().collect();
    names.sort();
    names
}

/// Check if a path string is a URL (http:// or https://)
pub fn is_url(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://")
}

/// Fetch content from a URL.
///
/// Requires the `remote` feature to be enabled.
///
/// # Example
///
/// ```rust,no_run
/// use rexpipe::library::fetch_url;
///
/// let content = fetch_url("https://example.com/patterns.toml").unwrap();
/// ```
#[cfg(feature = "remote")]
pub fn fetch_url(url: &str) -> Result<String> {
    log::debug!("Fetching remote library: {}", url);

    let config = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build();
    let agent: ureq::Agent = config.into();

    let response = agent
        .get(url)
        .call()
        .map_err(|e| anyhow::anyhow!("Failed to fetch '{}': {}", url, e))?;

    if response.status() != 200 {
        return Err(anyhow::anyhow!(
            "Failed to fetch '{}': HTTP {}",
            url,
            response.status()
        ));
    }

    response
        .into_body()
        .read_to_string()
        .map_err(|e| anyhow::anyhow!("Failed to read response from '{}': {}", url, e))
}

/// Stub for when remote feature is disabled.
#[cfg(not(feature = "remote"))]
pub fn fetch_url(url: &str) -> Result<String> {
    Err(anyhow::anyhow!(
        "Remote library support requires the 'remote' feature. \
         Cannot fetch '{}'.\n\
         Install with: cargo install rexpipe --features remote",
        url
    ))
}

/// Pre-compiled regex for pattern references like `${pattern.name}` or `${builtin:email}`
static PATTERN_REF_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // Allows: letters, digits, underscores, dots, and colons (for builtin:name syntax)
    Regex::new(r"\$\{([a-zA-Z_][a-zA-Z0-9_.:]*)\}").expect("invalid pattern ref regex")
});

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

    /// Check if a pattern exists in the library.
    ///
    /// # Example
    ///
    /// ```
    /// use rexpipe::library::ResolvedLibrary;
    ///
    /// let mut library = ResolvedLibrary::new();
    /// library.patterns.insert("email".to_string(), r"[\w.]+@[\w.]+".to_string());
    ///
    /// assert!(library.contains("email"));
    /// assert!(!library.contains("unknown"));
    /// ```
    pub fn contains(&self, name: &str) -> bool {
        self.patterns.contains_key(name)
    }

    /// Get an iterator over all pattern names in the library.
    ///
    /// # Example
    ///
    /// ```
    /// use rexpipe::library::ResolvedLibrary;
    ///
    /// let mut library = ResolvedLibrary::new();
    /// library.patterns.insert("email".to_string(), r"[\w.]+@[\w.]+".to_string());
    /// library.patterns.insert("phone".to_string(), r"\d{3}-\d{4}".to_string());
    ///
    /// let names: Vec<_> = library.pattern_names().collect();
    /// assert_eq!(names.len(), 2);
    /// ```
    pub fn pattern_names(&self) -> impl Iterator<Item = &String> {
        self.patterns.keys()
    }

    /// Merge another library into this one (other takes lower precedence)
    /// Emits warnings to stderr when patterns conflict
    pub fn merge(&mut self, other: ResolvedLibrary) {
        use std::collections::hash_map::Entry;

        let self_source = self
            .source_files
            .first()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        for (name, pattern) in other.patterns {
            match self.patterns.entry(name.clone()) {
                Entry::Occupied(_) => {
                    eprintln!(
                        "Warning: Pattern '{}' defined in multiple libraries, using definition from '{}'",
                        name, self_source
                    );
                }
                Entry::Vacant(entry) => {
                    entry.insert(pattern);
                }
            }
        }
        self.source_files.extend(other.source_files);
    }
}

/// Maximum depth for library includes to prevent excessive recursion
#[cfg(feature = "cli")]
const MAX_INCLUDE_DEPTH: usize = 32;

/// Library resolver handles loading pattern libraries with circular reference detection.
///
/// Only available with the `cli` feature because it reads pattern library files
/// from the filesystem and uses `dirs::home_dir()` to locate the global library
/// directory. WASM consumers should use inline `[aliases]` in the pipeline config
/// instead.
#[cfg(feature = "cli")]
pub struct LibraryResolver {
    /// Paths to search for libraries
    search_paths: Vec<PathBuf>,
    /// Cache of loaded libraries by canonical path
    loaded: HashMap<PathBuf, PatternLibrary>,
    /// Cache of loaded remote libraries by URL
    remote_cache: HashMap<String, PatternLibrary>,
    /// Stack of libraries currently being resolved (for cycle detection and depth limiting)
    resolution_stack: Vec<PathBuf>,
    /// Stack of remote URLs currently being resolved (for cycle detection)
    remote_resolution_stack: Vec<String>,
}

#[cfg(feature = "cli")]
impl LibraryResolver {
    /// Create a new resolver with search paths
    ///
    /// Search order:
    /// 1. Relative to the config file (if base_path is Some)
    /// 2. Current working directory
    /// 3. Global ~/.rexpipe/patterns/ directory
    pub fn new(base_path: Option<&Path>) -> Self {
        let mut search_paths = Vec::new();

        // Add base path (relative to config file)
        if let Some(base) = base_path {
            search_paths.push(base.to_path_buf());
        }

        // Add current working directory for portable configs
        if let Ok(cwd) = std::env::current_dir() {
            search_paths.push(cwd);
        }

        // Add global patterns directory
        if let Some(home) = dirs::home_dir() {
            search_paths.push(home.join(".rexpipe").join("patterns"));
        }

        Self {
            search_paths,
            loaded: HashMap::new(),
            remote_cache: HashMap::new(),
            resolution_stack: Vec::new(),
            remote_resolution_stack: Vec::new(),
        }
    }

    /// Load and resolve multiple libraries into a single ResolvedLibrary
    ///
    /// Supports both local files and remote URLs (with the `remote` feature):
    ///
    /// ```toml
    /// patterns_include = [
    ///     "local-patterns.toml",
    ///     "https://example.com/patterns/common.toml"
    /// ]
    /// ```
    pub fn load_libraries(&mut self, includes: &[String]) -> Result<ResolvedLibrary> {
        let mut resolved = ResolvedLibrary::new();

        for include in includes {
            if is_url(include) {
                // Handle remote library
                let lib = self.load_remote_library(include)?;
                let flattened = self.flatten_remote_library(&lib, include)?;
                resolved.merge(flattened);
            } else {
                // Handle local file
                let path = self.find_library(include)?;
                let lib = self.load_library_recursive(&path)?;
                let flattened = self.flatten_library(&lib, &path)?;
                resolved.merge(flattened);
            }
        }

        Ok(resolved)
    }

    /// Find a library file in the search paths
    fn find_library(&self, name: &str) -> Result<PathBuf> {
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

        Err(LibraryError::NotFound {
            name: name.to_string(),
            searched_paths: searched.join(", "),
        }
        .into())
    }

    /// Load a remote library from a URL
    ///
    /// Requires the `remote` feature.
    fn load_remote_library(&mut self, url: &str) -> Result<PatternLibrary> {
        // Check for circular reference
        if self.remote_resolution_stack.contains(&url.to_string()) {
            let cycle: Vec<String> = self.remote_resolution_stack.clone();
            return Err(LibraryError::CircularInclude {
                cycle: format!("{} -> {}", cycle.join(" -> "), url),
            }
            .into());
        }

        // Check for excessive depth
        let total_depth = self.resolution_stack.len() + self.remote_resolution_stack.len();
        if total_depth >= MAX_INCLUDE_DEPTH {
            return Err(anyhow::anyhow!(
                "Maximum library include depth ({}) exceeded",
                MAX_INCLUDE_DEPTH
            ));
        }

        // Check cache
        if let Some(lib) = self.remote_cache.get(url) {
            return Ok(lib.clone());
        }

        // Mark as in-progress
        self.remote_resolution_stack.push(url.to_string());

        // Fetch and parse the library
        let content = fetch_url(url)?;
        let library: PatternLibrary = toml::from_str(&content)
            .with_context(|| format!("Failed to parse remote library '{}'", url))?;

        // Process nested includes (remote libraries can include other remote libraries)
        for include in &library.patterns_include {
            if is_url(include) {
                // Recursively load remote
                let _nested = self.load_remote_library(include)?;
            } else {
                // Remote library including a local file - not supported
                // The remote library should only reference other remote libraries
                // or have all patterns inline
                log::warn!(
                    "Remote library '{}' references local file '{}' which is not supported. \
                     Remote libraries can only include other remote libraries or inline patterns.",
                    url,
                    include
                );
            }
        }

        // Remove from resolution stack
        self.remote_resolution_stack.pop();

        // Cache the loaded library
        self.remote_cache.insert(url.to_string(), library.clone());

        Ok(library)
    }

    /// Flatten a remote library's patterns to dot notation
    fn flatten_remote_library(
        &self,
        library: &PatternLibrary,
        url: &str,
    ) -> Result<ResolvedLibrary> {
        let mut resolved = ResolvedLibrary::new();
        resolved.source_files.push(PathBuf::from(url));

        // Flatten the patterns
        flatten_patterns_recursive(&library.patterns, "", &mut resolved.patterns);

        // Include patterns from nested remote includes
        for include in &library.patterns_include {
            if is_url(include) {
                if let Some(nested_lib) = self.remote_cache.get(include) {
                    let nested_resolved = self.flatten_remote_library(nested_lib, include)?;
                    for (name, pattern) in nested_resolved.patterns {
                        resolved.patterns.entry(name).or_insert(pattern);
                    }
                    resolved.source_files.extend(nested_resolved.source_files);
                }
            }
        }

        Ok(resolved)
    }

    /// Load a library file recursively, handling nested includes
    fn load_library_recursive(&mut self, path: &Path) -> Result<PatternLibrary> {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("Failed to resolve path '{}'", path.display()))?;

        // Check for circular reference
        if self.resolution_stack.contains(&canonical) {
            let cycle: Vec<String> = self
                .resolution_stack
                .iter()
                .map(|p| p.display().to_string())
                .collect();
            return Err(LibraryError::CircularInclude {
                cycle: format!("{} -> {}", cycle.join(" -> "), canonical.display()),
            }
            .into());
        }

        // Check for excessive include depth
        if self.resolution_stack.len() >= MAX_INCLUDE_DEPTH {
            return Err(anyhow::anyhow!(
                "Maximum library include depth ({}) exceeded. \
                 Include chain: {}",
                MAX_INCLUDE_DEPTH,
                self.resolution_stack
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" -> ")
            ));
        }

        // Check cache
        if let Some(lib) = self.loaded.get(&canonical) {
            return Ok(lib.clone());
        }

        // Mark as in-progress for cycle detection
        self.resolution_stack.push(canonical.clone());

        // Load and parse the library
        let content = fs::read_to_string(&canonical)
            .with_context(|| format!("Failed to read library '{}'", canonical.display()))?;

        let library: PatternLibrary = toml::from_str(&content)
            .with_context(|| format!("Failed to parse library '{}'", canonical.display()))?;

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
    ) -> Result<ResolvedLibrary> {
        let mut resolved = ResolvedLibrary::new();
        resolved.source_files.push(source_path.to_path_buf());

        // Flatten the patterns
        flatten_patterns_recursive(&library.patterns, "", &mut resolved.patterns);

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

    /// Validate a library file without resolving includes
    pub fn validate_library(path: &Path) -> Result<PatternLibrary> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read library '{}'", path.display()))?;

        let library: PatternLibrary = toml::from_str(&content)
            .with_context(|| format!("Failed to parse library '{}'", path.display()))?;

        // Validate that all pattern strings are valid regex
        let mut errors = Vec::new();
        Self::validate_patterns(&library.patterns, "", &mut errors);

        if !errors.is_empty() {
            return Err(LibraryError::InvalidPatterns {
                library: path.display().to_string(),
                errors: errors.join("\n  "),
            }
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
/// Also supports `${builtin:name}` for built-in patterns (email, ipv4, uuid, etc.).
/// Returns the resolved string and any errors encountered.
///
/// # Built-in patterns
///
/// Use `${builtin:name}` to reference built-in patterns:
/// - `${builtin:email}` - Email addresses
/// - `${builtin:ipv4}` - IPv4 addresses
/// - `${builtin:uuid}` - UUIDs
/// - `${builtin:url}` - URLs
/// - `${builtin:date_iso}` - ISO dates (YYYY-MM-DD)
/// - `${builtin:log_level}` - Common log levels (DEBUG, INFO, WARN, ERROR, etc.)
/// - And more... use `list_builtin_patterns()` to see all available patterns.
pub fn resolve_pattern_references(
    input: &str,
    library: &ResolvedLibrary,
) -> Result<String, Vec<String>> {
    let mut errors = Vec::new();
    let mut unresolved_refs = HashSet::new();

    let result = PATTERN_REF_REGEX
        .replace_all(input, |caps: &regex::Captures| {
            let ref_name = &caps[1];

            // Check for builtin: prefix
            if let Some(builtin_name) = ref_name.strip_prefix("builtin:") {
                match get_builtin_pattern(builtin_name) {
                    Some(pattern) => pattern.to_string(),
                    None => {
                        if !unresolved_refs.contains(ref_name) {
                            errors.push(format!(
                                "Unknown builtin pattern '${{{}}}' - available: {}",
                                ref_name,
                                list_builtin_patterns().join(", ")
                            ));
                            unresolved_refs.insert(ref_name.to_string());
                        }
                        caps[0].to_string()
                    }
                }
            } else {
                // Regular library reference
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
            }
        })
        .into_owned();

    if errors.is_empty() {
        Ok(result)
    } else {
        Err(errors)
    }
}

/// Resolve pattern references using inline aliases.
///
/// Resolves `${alias_name}` references using the provided aliases map.
/// Also handles `${builtin:name}` for built-in patterns.
/// Library references (patterns not in aliases) are left unchanged for later resolution.
///
/// # Example
///
/// ```rust
/// use std::collections::HashMap;
/// use rexpipe::library::resolve_pattern_aliases;
///
/// let mut aliases = HashMap::new();
/// aliases.insert("noise".to_string(), "(^\\[OK\\]|^\\[INFO\\])".to_string());
///
/// let result = resolve_pattern_aliases("${noise}|other", &aliases).unwrap();
/// assert_eq!(result, "(^\\[OK\\]|^\\[INFO\\])|other");
/// ```
pub fn resolve_pattern_aliases(
    input: &str,
    aliases: &std::collections::HashMap<String, String>,
) -> Result<String, Vec<String>> {
    let mut errors = Vec::new();
    let mut unresolved_refs = HashSet::new();

    let result = PATTERN_REF_REGEX
        .replace_all(input, |caps: &regex::Captures| {
            let ref_name = &caps[1];

            // Check for builtin: prefix first
            if let Some(builtin_name) = ref_name.strip_prefix("builtin:") {
                match get_builtin_pattern(builtin_name) {
                    Some(pattern) => pattern.to_string(),
                    None => {
                        if !unresolved_refs.contains(ref_name) {
                            errors.push(format!(
                                "Unknown builtin pattern '${{{}}}' - available: {}",
                                ref_name,
                                list_builtin_patterns().join(", ")
                            ));
                            unresolved_refs.insert(ref_name.to_string());
                        }
                        caps[0].to_string()
                    }
                }
            } else if let Some(pattern) = aliases.get(ref_name) {
                // Found in aliases
                pattern.clone()
            } else {
                // Not found - keep as-is (might be a library reference or error)
                caps[0].to_string()
            }
        })
        .into_owned();

    if errors.is_empty() {
        Ok(result)
    } else {
        Err(errors)
    }
}

/// Check if a pattern reference requires an external library (not a builtin or alias).
///
/// Returns true if the pattern contains `${...}` references that are NOT:
/// - Variable expansions (`${seq}`, `${count}`)
/// - Builtin patterns (`${builtin:...}`)
/// - Defined in the provided aliases
pub fn has_non_alias_pattern_references(
    input: &str,
    aliases: &std::collections::HashMap<String, String>,
) -> bool {
    // Find all ${...} sequences
    let mut i = 0;
    let bytes = input.as_bytes();
    while i < bytes.len().saturating_sub(1) {
        if bytes[i] == b'$' && bytes.get(i + 1) == Some(&b'{') {
            // Find the closing brace
            if let Some(end) = input[i..].find('}') {
                let ref_content = &input[i + 2..i + end];
                // Skip known variable expansions, builtin patterns, and aliases
                if ref_content != "seq"
                    && ref_content != "count"
                    && !ref_content.starts_with("builtin:")
                    && !aliases.contains_key(ref_content)
                {
                    return true;
                }
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    false
}

/// Check if a pattern reference requires an external library (not a builtin).
///
/// Returns true if the pattern contains `${...}` references that are NOT:
/// - Variable expansions (`${seq}`, `${count}`)
/// - Builtin patterns (`${builtin:...}`)
pub fn has_non_builtin_pattern_references(input: &str) -> bool {
    // Find all ${...} sequences
    let mut i = 0;
    let bytes = input.as_bytes();
    while i < bytes.len().saturating_sub(1) {
        if bytes[i] == b'$' && bytes.get(i + 1) == Some(&b'{') {
            // Find the closing brace
            if let Some(end) = input[i..].find('}') {
                let ref_content = &input[i + 2..i + end];
                // Skip known variable expansions and builtin patterns
                if ref_content != "seq"
                    && ref_content != "count"
                    && !ref_content.starts_with("builtin:")
                {
                    return true;
                }
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    false
}

/// Check if a string contains pattern references (library references, not variable expansions)
///
/// Pattern library references use the format `${library.pattern.name}` while
/// variable expansions use `${seq}` or `${count}`. This function returns true
/// only for library references, not variable expansions.
pub fn has_pattern_references(input: &str) -> bool {
    // Find all ${...} sequences
    let mut i = 0;
    let bytes = input.as_bytes();
    while i < bytes.len().saturating_sub(1) {
        if bytes[i] == b'$' && bytes.get(i + 1) == Some(&b'{') {
            // Find the closing brace
            if let Some(end) = input[i..].find('}') {
                let ref_content = &input[i + 2..i + end];
                // Skip known variable expansions
                if ref_content != "seq" && ref_content != "count" {
                    return true;
                }
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    false
}

/// Recursively flatten patterns to dot notation
#[cfg(feature = "cli")]
fn flatten_patterns_recursive(
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
                flatten_patterns_recursive(nested, &full_key, output);
            }
        }
    }
}

/// List all patterns in a library file
#[cfg(feature = "cli")]
pub fn list_patterns(path: &Path) -> Result<Vec<(String, String)>> {
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

    #[cfg(feature = "cli")]
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

        let mut output = HashMap::new();
        super::flatten_patterns_recursive(&patterns, "", &mut output);

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
