//! Structure-aware text processing using tree-sitter.
//!
//! This module provides syntax-aware pattern matching and transformation,
//! allowing patterns to be scoped to specific language constructs (e.g.,
//! only in function bodies, excluding strings and comments).
//!
//! # Example
//!
//! ```toml
//! [[step]]
//! type = "substitute"
//! pattern = "old_function"
//! replacement = "new_function"
//! scope = "code"  # Only in code, not in strings or comments
//! language = "rust"
//! ```

use std::collections::HashSet;
use std::str::FromStr;

/// Supported languages for syntax-aware processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Json,
    Yaml,
}

impl FromStr for Language {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rust" | "rs" => Ok(Language::Rust),
            "python" | "py" => Ok(Language::Python),
            "javascript" | "js" => Ok(Language::JavaScript),
            "typescript" | "ts" => Ok(Language::TypeScript),
            "go" | "golang" => Ok(Language::Go),
            "json" => Ok(Language::Json),
            "yaml" | "yml" => Ok(Language::Yaml),
            _ => Err(format!(
                "Unknown language: '{}'. Supported: rust, python, javascript, typescript, go, json, yaml",
                s
            )),
        }
    }
}

impl Language {
    /// Get the tree-sitter language for this language.
    #[cfg(feature = "tree-sitter")]
    pub fn tree_sitter_language(&self) -> tree_sitter::Language {
        match self {
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
            Language::Python => tree_sitter_python::LANGUAGE.into(),
            Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Language::Go => tree_sitter_go::LANGUAGE.into(),
            Language::Json => tree_sitter_json::LANGUAGE.into(),
            Language::Yaml => tree_sitter_yaml::LANGUAGE.into(),
        }
    }

    /// Get node types that represent string literals for this language.
    pub fn string_node_types(&self) -> &'static [&'static str] {
        match self {
            Language::Rust => &["string_literal", "raw_string_literal", "char_literal"],
            Language::Python => &["string", "string_content", "interpolation"],
            Language::JavaScript | Language::TypeScript => {
                &["string", "template_string", "template_literal_type"]
            }
            Language::Go => &["raw_string_literal", "interpreted_string_literal"],
            Language::Json => &["string", "string_content"],
            Language::Yaml => &[
                "string_scalar",
                "double_quote_scalar",
                "single_quote_scalar",
            ],
        }
    }

    /// Get node types that represent comments for this language.
    pub fn comment_node_types(&self) -> &'static [&'static str] {
        match self {
            Language::Rust => &["line_comment", "block_comment"],
            Language::Python => &["comment"],
            Language::JavaScript | Language::TypeScript => &["comment"],
            Language::Go => &["comment"],
            Language::Json => &[], // JSON has no comments
            Language::Yaml => &["comment"],
        }
    }

    /// Get node types that represent function/method definitions.
    pub fn function_node_types(&self) -> &'static [&'static str] {
        match self {
            Language::Rust => &["function_item", "impl_item"],
            Language::Python => &["function_definition", "async_function_definition"],
            Language::JavaScript | Language::TypeScript => &[
                "function_declaration",
                "arrow_function",
                "method_definition",
            ],
            Language::Go => &["function_declaration", "method_declaration"],
            Language::Json => &[],
            Language::Yaml => &[],
        }
    }

    /// Get node types that represent function/method calls.
    pub fn function_call_node_types(&self) -> &'static [&'static str] {
        match self {
            Language::Rust => &["call_expression", "method_call_expression"],
            Language::Python => &["call", "attribute"],
            Language::JavaScript | Language::TypeScript => {
                &["call_expression", "member_expression"]
            }
            Language::Go => &["call_expression"],
            Language::Json => &[],
            Language::Yaml => &[],
        }
    }

    /// Get node types that represent import/use statements.
    pub fn import_node_types(&self) -> &'static [&'static str] {
        match self {
            Language::Rust => &["use_declaration", "extern_crate_declaration"],
            Language::Python => &["import_statement", "import_from_statement"],
            Language::JavaScript | Language::TypeScript => {
                &["import_statement", "import_specifier", "export_statement"]
            }
            Language::Go => &["import_declaration", "import_spec"],
            Language::Json => &[],
            Language::Yaml => &[],
        }
    }

    /// Get node types that represent type annotations.
    pub fn type_node_types(&self) -> &'static [&'static str] {
        match self {
            Language::Rust => &[
                "type_identifier",
                "generic_type",
                "scoped_type_identifier",
                "type_item",
                "type_annotation",
            ],
            Language::Python => &["type", "subscript", "attribute"],
            Language::JavaScript => &[],
            Language::TypeScript => &[
                "type_annotation",
                "type_identifier",
                "generic_type",
                "type_alias_declaration",
            ],
            Language::Go => &["type_identifier", "type_spec", "type_declaration"],
            Language::Json => &[],
            Language::Yaml => &[],
        }
    }

    /// Get node types that represent identifiers.
    pub fn identifier_node_types(&self) -> &'static [&'static str] {
        match self {
            Language::Rust => &["identifier", "field_identifier", "type_identifier"],
            Language::Python => &["identifier"],
            Language::JavaScript | Language::TypeScript => &["identifier", "property_identifier"],
            Language::Go => &["identifier", "field_identifier", "type_identifier"],
            Language::Json => &["string"],
            Language::Yaml => &["flow_node", "block_scalar"],
        }
    }

    /// Get node types that represent macro invocations.
    pub fn macro_node_types(&self) -> &'static [&'static str] {
        match self {
            Language::Rust => &["macro_invocation", "macro_definition"],
            Language::Python => &["decorator"], // Python decorators are similar to macros
            Language::JavaScript | Language::TypeScript => &["decorator"],
            Language::Go => &[], // Go doesn't have macros
            Language::Json => &[],
            Language::Yaml => &[],
        }
    }

    /// Get node types that represent control flow statements.
    pub fn control_flow_node_types(&self) -> &'static [&'static str] {
        match self {
            Language::Rust => &[
                "if_expression",
                "match_expression",
                "for_expression",
                "while_expression",
                "loop_expression",
            ],
            Language::Python => &[
                "if_statement",
                "for_statement",
                "while_statement",
                "match_statement",
                "try_statement",
            ],
            Language::JavaScript | Language::TypeScript => &[
                "if_statement",
                "for_statement",
                "for_in_statement",
                "while_statement",
                "do_statement",
                "switch_statement",
                "try_statement",
            ],
            Language::Go => &[
                "if_statement",
                "for_statement",
                "switch_statement",
                "select_statement",
            ],
            Language::Json => &[],
            Language::Yaml => &[],
        }
    }

    /// Get test framework function names for this language.
    /// These are call expressions that indicate test code (describe, it, test, etc.)
    pub fn test_framework_functions(&self) -> &'static [&'static str] {
        match self {
            // Rust tests are identified by #[test] attribute, not function calls
            Language::Rust => &[],
            // Python: pytest.mark, unittest assertions
            Language::Python => &["pytest", "unittest"],
            // JavaScript/TypeScript: Jest, Mocha, Vitest, etc.
            Language::JavaScript | Language::TypeScript => &[
                "describe",
                "it",
                "test",
                "beforeEach",
                "afterEach",
                "beforeAll",
                "afterAll",
                "expect",
                "jest",
                "vi",
            ],
            // Go tests are identified by Test prefix, not function calls
            Language::Go => &[],
            Language::Json => &[],
            Language::Yaml => &[],
        }
    }

    /// Get test function name patterns for this language.
    /// Returns (prefix, suffix) patterns that indicate test functions.
    pub fn test_function_patterns(&self) -> (&'static [&'static str], &'static [&'static str]) {
        match self {
            // Rust: functions in #[cfg(test)] modules or with #[test] attribute
            Language::Rust => (&["test_"], &["_test"]),
            // Python: test_ prefix (pytest), Test class prefix (unittest)
            Language::Python => (&["test_", "Test"], &["_test"]),
            // JavaScript/TypeScript: .test.js, .spec.js files, but functions vary
            Language::JavaScript | Language::TypeScript => (&["test", "spec"], &["Test", "Spec"]),
            // Go: Test prefix required by testing package
            Language::Go => (&["Test", "Benchmark", "Example"], &[]),
            Language::Json => (&[], &[]),
            Language::Yaml => (&[], &[]),
        }
    }

    /// Get node types that contain test module definitions.
    pub fn test_module_node_types(&self) -> &'static [&'static str] {
        match self {
            // Rust: mod tests { } blocks
            Language::Rust => &["mod_item"],
            // Python: test files are typically whole modules
            Language::Python => &["module"],
            // JavaScript/TypeScript: describe blocks
            Language::JavaScript | Language::TypeScript => &["call_expression"],
            // Go: _test.go files
            Language::Go => &["source_file"],
            Language::Json => &[],
            Language::Yaml => &[],
        }
    }
}

/// Scope filter for syntax-aware processing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeFilter {
    /// Match anywhere (no filtering)
    All,
    /// Match only in code (exclude strings and comments)
    Code,
    /// Match only in strings
    Strings,
    /// Match only in comments
    Comments,
    /// Match only in function/method definitions
    Functions,
    /// Match only in function/method calls
    FunctionCalls,
    /// Match only in import/use statements
    Imports,
    /// Match only in type annotations
    Types,
    /// Match only in identifiers
    Identifiers,
    /// Match only in macro invocations (language-specific)
    Macros,
    /// Match only in control flow (if, for, while, match, etc.)
    ControlFlow,
    /// Match only in test code (test functions, describe/it blocks, #[test] attributes)
    Tests,
    /// Custom node types to include
    Include(HashSet<String>),
    /// Custom node types to exclude
    Exclude(HashSet<String>),
}

impl FromStr for ScopeFilter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "all" | "*" => Ok(ScopeFilter::All),
            "code" => Ok(ScopeFilter::Code),
            "strings" | "string" => Ok(ScopeFilter::Strings),
            "comments" | "comment" => Ok(ScopeFilter::Comments),
            "functions" | "function" | "fn" => Ok(ScopeFilter::Functions),
            "function_calls" | "calls" => Ok(ScopeFilter::FunctionCalls),
            "imports" | "import" | "use" => Ok(ScopeFilter::Imports),
            "types" | "type" => Ok(ScopeFilter::Types),
            "identifiers" | "identifier" | "ident" => Ok(ScopeFilter::Identifiers),
            "macros" | "macro" => Ok(ScopeFilter::Macros),
            "control_flow" | "control" | "flow" => Ok(ScopeFilter::ControlFlow),
            "tests" | "test" | "specs" | "spec" => Ok(ScopeFilter::Tests),
            _ => Err(format!(
                "Unknown scope: '{}'. Supported: all, code, strings, comments, functions, function_calls, imports, types, identifiers, macros, control_flow, tests",
                s
            )),
        }
    }
}

/// Byte range in the source code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

impl ByteRange {
    pub fn new(start: usize, end: usize) -> Self {
        ByteRange { start, end }
    }

    pub fn contains(&self, byte: usize) -> bool {
        byte >= self.start && byte < self.end
    }

    pub fn overlaps(&self, other: &ByteRange) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// Syntax analyzer for a specific language.
#[cfg(feature = "tree-sitter")]
pub struct SyntaxAnalyzer {
    parser: tree_sitter::Parser,
    language: Language,
}

#[cfg(feature = "tree-sitter")]
impl SyntaxAnalyzer {
    /// Create a new syntax analyzer for the given language.
    pub fn new(language: Language) -> Result<Self, String> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&language.tree_sitter_language())
            .map_err(|e| format!("Failed to set language: {}", e))?;
        Ok(SyntaxAnalyzer { parser, language })
    }

    /// Parse source code and return the syntax tree.
    pub fn parse(&mut self, source: &str) -> Option<tree_sitter::Tree> {
        self.parser.parse(source, None)
    }

    /// Find all ranges that match the given scope filter.
    pub fn find_scope_ranges(&mut self, source: &str, filter: &ScopeFilter) -> Vec<ByteRange> {
        let tree = match self.parse(source) {
            Some(t) => t,
            None => return vec![ByteRange::new(0, source.len())], // Fall back to full range
        };

        match filter {
            ScopeFilter::All => vec![ByteRange::new(0, source.len())],
            ScopeFilter::Code => self.find_code_ranges(&tree, source),
            ScopeFilter::Strings => {
                self.find_node_type_ranges(&tree, self.language.string_node_types())
            }
            ScopeFilter::Comments => {
                self.find_node_type_ranges(&tree, self.language.comment_node_types())
            }
            ScopeFilter::Functions => {
                self.find_node_type_ranges(&tree, self.language.function_node_types())
            }
            ScopeFilter::FunctionCalls => {
                self.find_node_type_ranges(&tree, self.language.function_call_node_types())
            }
            ScopeFilter::Imports => {
                self.find_node_type_ranges(&tree, self.language.import_node_types())
            }
            ScopeFilter::Types => {
                self.find_node_type_ranges(&tree, self.language.type_node_types())
            }
            ScopeFilter::Identifiers => {
                self.find_node_type_ranges(&tree, self.language.identifier_node_types())
            }
            ScopeFilter::Macros => {
                self.find_node_type_ranges(&tree, self.language.macro_node_types())
            }
            ScopeFilter::ControlFlow => {
                self.find_node_type_ranges(&tree, self.language.control_flow_node_types())
            }
            ScopeFilter::Tests => self.find_test_ranges(&tree, source),
            ScopeFilter::Include(types) => {
                let types_vec: Vec<&str> = types.iter().map(|s| s.as_str()).collect();
                self.find_node_type_ranges(&tree, &types_vec)
            }
            ScopeFilter::Exclude(scope_names) => {
                // Convert high-level scope names to actual node types
                let mut exclude_types: Vec<&'static str> = Vec::new();
                for name in scope_names {
                    match name.to_lowercase().as_str() {
                        "strings" | "string" => {
                            exclude_types.extend(self.language.string_node_types());
                        }
                        "comments" | "comment" => {
                            exclude_types.extend(self.language.comment_node_types());
                        }
                        "functions" | "function" | "fn" => {
                            exclude_types.extend(self.language.function_node_types());
                        }
                        "function_calls" | "calls" => {
                            exclude_types.extend(self.language.function_call_node_types());
                        }
                        "imports" | "import" | "use" => {
                            exclude_types.extend(self.language.import_node_types());
                        }
                        "types" | "type" => {
                            exclude_types.extend(self.language.type_node_types());
                        }
                        "identifiers" | "identifier" | "ident" => {
                            exclude_types.extend(self.language.identifier_node_types());
                        }
                        "macros" | "macro" => {
                            exclude_types.extend(self.language.macro_node_types());
                        }
                        "control_flow" | "control" | "flow" => {
                            exclude_types.extend(self.language.control_flow_node_types());
                        }
                        _ => {
                            // Treat as raw node type name for advanced users
                            log::debug!("Unknown scope name '{}', treating as raw node type", name);
                        }
                    }
                }
                let exclude_ranges = self.find_node_type_ranges(&tree, &exclude_types);
                self.invert_ranges(&exclude_ranges, source.len())
            }
        }
    }

    /// Find ranges that are "code" (not strings or comments).
    fn find_code_ranges(&self, tree: &tree_sitter::Tree, source: &str) -> Vec<ByteRange> {
        let mut exclude_types = Vec::new();
        exclude_types.extend(self.language.string_node_types());
        exclude_types.extend(self.language.comment_node_types());

        let exclude_ranges = self.find_node_type_ranges(tree, &exclude_types);
        self.invert_ranges(&exclude_ranges, source.len())
    }

    /// Find ranges that contain test code.
    /// This includes:
    /// - Rust: Functions with #[test] attribute, modules named "tests", #[cfg(test)] blocks
    /// - Python: Functions starting with test_, classes starting with Test
    /// - JavaScript/TypeScript: describe(), it(), test() call expressions
    /// - Go: Functions starting with Test, Benchmark, or Example
    fn find_test_ranges(&self, tree: &tree_sitter::Tree, source: &str) -> Vec<ByteRange> {
        let mut ranges = Vec::new();
        self.collect_test_nodes(tree.root_node(), source, &mut ranges);

        // Sort and merge overlapping ranges
        ranges.sort_by_key(|r| r.start);
        self.merge_ranges(ranges)
    }

    /// Recursively collect nodes that represent test code.
    fn collect_test_nodes(
        &self,
        node: tree_sitter::Node,
        source: &str,
        ranges: &mut Vec<ByteRange>,
    ) {
        let kind = node.kind();

        match self.language {
            Language::Rust => {
                // Check for #[test] or #[cfg(test)] attributes on functions
                if kind == "function_item" && self.rust_has_test_attribute(&node, source) {
                    ranges.push(ByteRange::new(node.start_byte(), node.end_byte()));
                    return;
                }
                // Check for mod tests { } blocks
                if kind == "mod_item" {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = &source[name_node.start_byte()..name_node.end_byte()];
                        if name == "tests" || name == "test" {
                            ranges.push(ByteRange::new(node.start_byte(), node.end_byte()));
                            return;
                        }
                    }
                }
            }
            Language::Python => {
                // Check for test_ prefix on functions or Test prefix on classes
                if kind == "function_definition" || kind == "async_function_definition" {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = &source[name_node.start_byte()..name_node.end_byte()];
                        if name.starts_with("test_") || name.starts_with("test") {
                            ranges.push(ByteRange::new(node.start_byte(), node.end_byte()));
                            return;
                        }
                    }
                }
                if kind == "class_definition" {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = &source[name_node.start_byte()..name_node.end_byte()];
                        if name.starts_with("Test") {
                            ranges.push(ByteRange::new(node.start_byte(), node.end_byte()));
                            return;
                        }
                    }
                }
            }
            Language::JavaScript | Language::TypeScript => {
                // Check for describe(), it(), test() call expressions
                if kind == "call_expression" && self.js_is_test_call(&node, source) {
                    ranges.push(ByteRange::new(node.start_byte(), node.end_byte()));
                    return;
                }
            }
            Language::Go => {
                // Check for Test, Benchmark, Example prefixes on functions
                if kind == "function_declaration" {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = &source[name_node.start_byte()..name_node.end_byte()];
                        if name.starts_with("Test")
                            || name.starts_with("Benchmark")
                            || name.starts_with("Example")
                        {
                            ranges.push(ByteRange::new(node.start_byte(), node.end_byte()));
                            return;
                        }
                    }
                }
            }
            Language::Json | Language::Yaml => {
                // JSON/YAML don't have test concepts
            }
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_test_nodes(child, source, ranges);
        }
    }

    /// Check if a Rust function has #[test] or #[cfg(test)] attribute.
    fn rust_has_test_attribute(&self, func_node: &tree_sitter::Node, source: &str) -> bool {
        // Look for attribute_item siblings before the function
        if let Some(parent) = func_node.parent() {
            let mut cursor = parent.walk();
            let mut prev_was_test_attr = false;

            for child in parent.children(&mut cursor) {
                if child.id() == func_node.id() {
                    return prev_was_test_attr;
                }
                if child.kind() == "attribute_item" {
                    let attr_text = &source[child.start_byte()..child.end_byte()];
                    prev_was_test_attr = attr_text.contains("#[test]")
                        || attr_text.contains("#[cfg(test)]")
                        || attr_text.contains("#[tokio::test]")
                        || attr_text.contains("#[async_std::test]");
                } else if child.kind() != "line_comment" && child.kind() != "block_comment" {
                    prev_was_test_attr = false;
                }
            }
        }
        false
    }

    /// Check if a JavaScript/TypeScript call expression is a test function.
    fn js_is_test_call(&self, call_node: &tree_sitter::Node, source: &str) -> bool {
        // Get the function being called (first child is typically the callee)
        if let Some(callee) = call_node.child(0) {
            let callee_text = &source[callee.start_byte()..callee.end_byte()];
            let test_functions = self.language.test_framework_functions();
            for test_fn in test_functions {
                if callee_text == *test_fn || callee_text.ends_with(&format!(".{}", test_fn)) {
                    return true;
                }
            }
        }
        false
    }

    /// Find all ranges of nodes matching the given types.
    fn find_node_type_ranges(&self, tree: &tree_sitter::Tree, types: &[&str]) -> Vec<ByteRange> {
        let mut ranges = Vec::new();
        let type_set: HashSet<&str> = types.iter().copied().collect();

        self.collect_matching_nodes(tree.root_node(), &type_set, &mut ranges);

        // Sort and merge overlapping ranges
        ranges.sort_by_key(|r| r.start);
        self.merge_ranges(ranges)
    }

    /// Recursively collect nodes matching the given types.
    #[allow(clippy::only_used_in_recursion)]
    fn collect_matching_nodes(
        &self,
        node: tree_sitter::Node,
        types: &HashSet<&str>,
        ranges: &mut Vec<ByteRange>,
    ) {
        if types.contains(node.kind()) {
            ranges.push(ByteRange::new(node.start_byte(), node.end_byte()));
            // Don't recurse into matched nodes (they're already included)
            return;
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_matching_nodes(child, types, ranges);
        }
    }

    /// Merge overlapping and adjacent ranges.
    fn merge_ranges(&self, ranges: Vec<ByteRange>) -> Vec<ByteRange> {
        if ranges.is_empty() {
            return ranges;
        }

        let mut merged = Vec::new();
        let mut current = ranges[0];

        for range in ranges.into_iter().skip(1) {
            if range.start <= current.end {
                // Overlapping or adjacent - merge
                current.end = current.end.max(range.end);
            } else {
                merged.push(current);
                current = range;
            }
        }
        merged.push(current);

        merged
    }

    /// Invert ranges to get the complement.
    fn invert_ranges(&self, ranges: &[ByteRange], total_len: usize) -> Vec<ByteRange> {
        if ranges.is_empty() {
            return vec![ByteRange::new(0, total_len)];
        }

        let mut inverted = Vec::new();
        let mut pos = 0;

        for range in ranges {
            if pos < range.start {
                inverted.push(ByteRange::new(pos, range.start));
            }
            pos = range.end;
        }

        if pos < total_len {
            inverted.push(ByteRange::new(pos, total_len));
        }

        inverted
    }

    /// Check if a byte position is within the allowed ranges.
    pub fn is_in_scope(&mut self, source: &str, byte_pos: usize, filter: &ScopeFilter) -> bool {
        let ranges = self.find_scope_ranges(source, filter);
        ranges.iter().any(|r| r.contains(byte_pos))
    }

    /// Apply a pattern match only within scoped ranges.
    /// Returns the byte ranges where the pattern matched AND is in scope.
    pub fn scoped_match(
        &mut self,
        source: &str,
        pattern: &regex::Regex,
        filter: &ScopeFilter,
    ) -> Vec<ByteRange> {
        let scope_ranges = self.find_scope_ranges(source, filter);
        let mut results = Vec::new();

        for m in pattern.find_iter(source) {
            let match_range = ByteRange::new(m.start(), m.end());
            // Check if the match falls entirely within a scoped range
            if scope_ranges
                .iter()
                .any(|r| r.start <= match_range.start && r.end >= match_range.end)
            {
                results.push(match_range);
            }
        }

        results
    }

    /// Apply a substitution only within scoped ranges.
    pub fn scoped_replace(
        &mut self,
        source: &str,
        pattern: &regex::Regex,
        replacement: &str,
        filter: &ScopeFilter,
    ) -> String {
        let scope_ranges = self.find_scope_ranges(source, filter);

        // Collect all matches that are in scope
        let mut matches: Vec<(usize, usize)> = Vec::new();
        for m in pattern.find_iter(source) {
            let match_range = ByteRange::new(m.start(), m.end());
            if scope_ranges
                .iter()
                .any(|r| r.start <= match_range.start && r.end >= match_range.end)
            {
                matches.push((m.start(), m.end()));
            }
        }

        // Apply replacements in reverse order to preserve byte positions
        let mut result = source.to_string();
        for (start, end) in matches.into_iter().rev() {
            let matched_text = &source[start..end];
            let replaced = pattern.replace(matched_text, replacement);
            result.replace_range(start..end, &replaced);
        }

        result
    }

    /// Extract all matches that fall within scoped ranges.
    ///
    /// Returns a vector of matched strings that are both:
    /// 1. Matched by the pattern
    /// 2. Entirely within a scoped range
    ///
    /// # Arguments
    /// * `source` - The source code to search
    /// * `pattern` - The regex pattern to match
    /// * `filter` - The scope filter to apply
    ///
    /// # Example
    ///
    /// ```ignore
    /// let matches = analyzer.scoped_extract(source, &pattern, &ScopeFilter::Code);
    /// // Returns only matches that are in code, not in strings or comments
    /// ```
    pub fn scoped_extract(
        &mut self,
        source: &str,
        pattern: &regex::Regex,
        filter: &ScopeFilter,
    ) -> Vec<String> {
        let scope_ranges = self.find_scope_ranges(source, filter);
        let mut results = Vec::new();

        for m in pattern.find_iter(source) {
            let match_range = ByteRange::new(m.start(), m.end());
            // Check if the match falls entirely within a scoped range
            if scope_ranges
                .iter()
                .any(|r| r.start <= match_range.start && r.end >= match_range.end)
            {
                results.push(m.as_str().to_string());
            }
        }

        results
    }

    /// Apply a transformation function to matches within scoped ranges.
    ///
    /// Similar to `scoped_replace`, but uses a closure to transform matches
    /// instead of a fixed replacement string.
    ///
    /// # Arguments
    /// * `source` - The source code to transform
    /// * `pattern` - The regex pattern to match
    /// * `transformer` - Function that takes a matched string and returns the replacement
    /// * `filter` - The scope filter to apply
    ///
    /// # Returns
    ///
    /// The source with all in-scope matches transformed.
    pub fn scoped_transform<F>(
        &mut self,
        source: &str,
        pattern: &regex::Regex,
        transformer: F,
        filter: &ScopeFilter,
    ) -> String
    where
        F: Fn(&str) -> String,
    {
        let scope_ranges = self.find_scope_ranges(source, filter);

        // Collect all matches that are in scope
        let mut matches: Vec<(usize, usize, String)> = Vec::new();
        for m in pattern.find_iter(source) {
            let match_range = ByteRange::new(m.start(), m.end());
            if scope_ranges
                .iter()
                .any(|r| r.start <= match_range.start && r.end >= match_range.end)
            {
                matches.push((m.start(), m.end(), m.as_str().to_string()));
            }
        }

        // Apply transformations in reverse order to preserve byte positions
        let mut result = source.to_string();
        for (start, end, matched) in matches.into_iter().rev() {
            let transformed = transformer(&matched);
            result.replace_range(start..end, &transformed);
        }

        result
    }

    /// Check if all matches of a pattern fall within scoped ranges.
    ///
    /// This is useful for validation - checking that certain patterns
    /// only appear in expected scopes (e.g., ensuring `unsafe` only appears in code).
    ///
    /// # Arguments
    /// * `source` - The source code to validate
    /// * `pattern` - The regex pattern to check
    /// * `filter` - The scope filter - matches should be within this scope
    ///
    /// # Returns
    ///
    /// `true` if all matches are within scope, `false` if any match is outside scope.
    /// Returns `true` if there are no matches (vacuous truth).
    pub fn validate_in_scope(
        &mut self,
        source: &str,
        pattern: &regex::Regex,
        filter: &ScopeFilter,
    ) -> bool {
        let scope_ranges = self.find_scope_ranges(source, filter);

        for m in pattern.find_iter(source) {
            let match_range = ByteRange::new(m.start(), m.end());
            // Check if the match falls entirely within any scoped range
            let in_scope = scope_ranges
                .iter()
                .any(|r| r.start <= match_range.start && r.end >= match_range.end);
            if !in_scope {
                return false;
            }
        }

        true
    }

    /// Get matches with their scope status for detailed validation reporting.
    ///
    /// Returns each match along with whether it's in scope, useful for
    /// generating detailed validation error messages.
    ///
    /// # Arguments
    /// * `source` - The source code to validate
    /// * `pattern` - The regex pattern to check
    /// * `filter` - The scope filter
    ///
    /// # Returns
    ///
    /// Vector of (matched_text, byte_range, is_in_scope) tuples.
    pub fn validate_matches_detailed(
        &mut self,
        source: &str,
        pattern: &regex::Regex,
        filter: &ScopeFilter,
    ) -> Vec<(String, ByteRange, bool)> {
        let scope_ranges = self.find_scope_ranges(source, filter);
        let mut results = Vec::new();

        for m in pattern.find_iter(source) {
            let match_range = ByteRange::new(m.start(), m.end());
            let in_scope = scope_ranges
                .iter()
                .any(|r| r.start <= match_range.start && r.end >= match_range.end);
            results.push((m.as_str().to_string(), match_range, in_scope));
        }

        results
    }
}

#[cfg(test)]
#[cfg(feature = "tree-sitter")]
mod tests {
    use super::*;

    #[test]
    fn test_language_from_str() {
        assert_eq!("rust".parse::<Language>().ok(), Some(Language::Rust));
        assert_eq!("Python".parse::<Language>().ok(), Some(Language::Python));
        assert_eq!("js".parse::<Language>().ok(), Some(Language::JavaScript));
        assert!("unknown".parse::<Language>().is_err());
    }

    #[test]
    fn test_scope_filter_from_str() {
        assert_eq!("all".parse::<ScopeFilter>().ok(), Some(ScopeFilter::All));
        assert_eq!("code".parse::<ScopeFilter>().ok(), Some(ScopeFilter::Code));
        assert_eq!(
            "strings".parse::<ScopeFilter>().ok(),
            Some(ScopeFilter::Strings)
        );
        assert_eq!(
            "comments".parse::<ScopeFilter>().ok(),
            Some(ScopeFilter::Comments)
        );
        assert_eq!(
            "tests".parse::<ScopeFilter>().ok(),
            Some(ScopeFilter::Tests)
        );
        assert_eq!("test".parse::<ScopeFilter>().ok(), Some(ScopeFilter::Tests));
        assert_eq!(
            "specs".parse::<ScopeFilter>().ok(),
            Some(ScopeFilter::Tests)
        );
    }

    #[test]
    fn test_rust_code_scope() {
        let mut analyzer = SyntaxAnalyzer::new(Language::Rust).unwrap();
        let source = r#"
fn main() {
    let x = "hello"; // comment
    println!("{}", x);
}
"#;
        let pattern = regex::Regex::new(r"hello").unwrap();

        // "hello" is in a string, so code scope should not match it
        let code_matches = analyzer.scoped_match(source, &pattern, &ScopeFilter::Code);
        assert!(
            code_matches.is_empty(),
            "Should not match 'hello' in code scope"
        );

        // But string scope should match it
        let string_matches = analyzer.scoped_match(source, &pattern, &ScopeFilter::Strings);
        assert_eq!(
            string_matches.len(),
            1,
            "Should match 'hello' in string scope"
        );
    }

    #[test]
    fn test_scoped_replace() {
        let mut analyzer = SyntaxAnalyzer::new(Language::Rust).unwrap();
        let source = r#"
fn old_function() {
    // old_function comment
    let s = "old_function";
}
"#;
        let pattern = regex::Regex::new(r"old_function").unwrap();

        // Replace only in code (not in strings or comments)
        let result = analyzer.scoped_replace(source, &pattern, "new_function", &ScopeFilter::Code);

        // Function name should be replaced
        assert!(
            result.contains("fn new_function()"),
            "Function name should be replaced"
        );
        // Comment should be unchanged
        assert!(
            result.contains("// old_function"),
            "Comment should be unchanged"
        );
        // String should be unchanged
        assert!(
            result.contains("\"old_function\""),
            "String should be unchanged"
        );
    }

    #[test]
    fn test_rust_tests_scope() {
        let mut analyzer = SyntaxAnalyzer::new(Language::Rust).unwrap();
        let source = r#"
fn regular_function() {
    println!("not a test");
}

#[test]
fn test_something() {
    assert!(true);
}

mod tests {
    fn helper() {}

    #[test]
    fn test_inside_mod() {
        assert!(true);
    }
}
"#;
        let pattern = regex::Regex::new(r"assert").unwrap();

        // Tests scope should only match assert inside test functions
        let test_matches = analyzer.scoped_match(source, &pattern, &ScopeFilter::Tests);
        assert_eq!(
            test_matches.len(),
            2,
            "Should match 'assert' in test functions"
        );
    }

    #[test]
    fn test_python_tests_scope() {
        let mut analyzer = SyntaxAnalyzer::new(Language::Python).unwrap();
        let source = r#"
def regular_function():
    print("not a test")

def test_something():
    assert True

class TestSuite:
    def test_method(self):
        assert True
"#;
        let pattern = regex::Regex::new(r"assert").unwrap();

        // Tests scope should match assert in test_ functions and Test classes
        let test_matches = analyzer.scoped_match(source, &pattern, &ScopeFilter::Tests);
        assert_eq!(
            test_matches.len(),
            2,
            "Should match 'assert' in test functions and Test classes"
        );
    }

    #[test]
    fn test_javascript_tests_scope() {
        let mut analyzer = SyntaxAnalyzer::new(Language::JavaScript).unwrap();
        let source = r#"
function regularFunction() {
    console.log("not a test");
}

describe("my suite", () => {
    it("should work", () => {
        expect(true).toBe(true);
    });
});

test("standalone test", () => {
    expect(1).toBe(1);
});
"#;
        let pattern = regex::Regex::new(r"expect").unwrap();

        // Tests scope should match expect inside describe/it/test blocks
        let test_matches = analyzer.scoped_match(source, &pattern, &ScopeFilter::Tests);
        assert!(
            test_matches.len() >= 2,
            "Should match 'expect' in test blocks"
        );
    }

    #[test]
    fn test_go_tests_scope() {
        let mut analyzer = SyntaxAnalyzer::new(Language::Go).unwrap();
        let source = r#"
package main

func regularFunction() {
    fmt.Println("not a test")
}

func TestSomething(t *testing.T) {
    if true {
        t.Error("fail")
    }
}

func BenchmarkSpeed(b *testing.B) {
    for i := 0; i < b.N; i++ {
        // benchmark
    }
}
"#;
        let pattern = regex::Regex::new(r"testing").unwrap();

        // Tests scope should match testing.T in Test functions
        let test_matches = analyzer.scoped_match(source, &pattern, &ScopeFilter::Tests);
        assert_eq!(
            test_matches.len(),
            2,
            "Should match 'testing' in Test and Benchmark functions"
        );
    }

    #[test]
    fn test_scoped_extract() {
        let mut analyzer = SyntaxAnalyzer::new(Language::Rust).unwrap();
        let source = r#"
fn main() {
    let foo = "foo_string";  // foo_comment
    let bar = 42;
}
"#;
        let pattern = regex::Regex::new(r"foo").unwrap();

        // Extract only from code scope (not strings or comments)
        let code_extracts = analyzer.scoped_extract(source, &pattern, &ScopeFilter::Code);
        assert_eq!(
            code_extracts.len(),
            1,
            "Should extract 'foo' only from code"
        );
        assert_eq!(code_extracts[0], "foo");

        // Extract only from strings scope
        let string_extracts = analyzer.scoped_extract(source, &pattern, &ScopeFilter::Strings);
        assert_eq!(
            string_extracts.len(),
            1,
            "Should extract 'foo' only from strings"
        );
        assert_eq!(string_extracts[0], "foo");

        // Extract only from comments scope
        let comment_extracts = analyzer.scoped_extract(source, &pattern, &ScopeFilter::Comments);
        assert_eq!(
            comment_extracts.len(),
            1,
            "Should extract 'foo' only from comments"
        );
    }

    #[test]
    fn test_scoped_transform() {
        let mut analyzer = SyntaxAnalyzer::new(Language::Rust).unwrap();
        let source = r#"
fn main() {
    let hello = "hello";  // hello comment
}
"#;
        let pattern = regex::Regex::new(r"hello").unwrap();

        // Transform only in code scope - uppercase
        let result =
            analyzer.scoped_transform(source, &pattern, |s| s.to_uppercase(), &ScopeFilter::Code);

        // Variable name should be uppercased
        assert!(
            result.contains("HELLO"),
            "Variable name should be uppercased"
        );
        // String content should remain unchanged
        assert!(
            result.contains("\"hello\""),
            "String should remain unchanged"
        );
        // Comment should remain unchanged
        assert!(
            result.contains("// hello"),
            "Comment should remain unchanged"
        );
    }

    #[test]
    fn test_validate_in_scope() {
        let mut analyzer = SyntaxAnalyzer::new(Language::Rust).unwrap();

        // Source where "debug" appears only in a string
        let string_only_source = r#"
fn main() {
    let msg = "debug mode";
}
"#;
        let pattern = regex::Regex::new(r"debug").unwrap();

        // All "debug" matches should be in strings scope
        assert!(
            analyzer.validate_in_scope(string_only_source, &pattern, &ScopeFilter::Strings),
            "All 'debug' should be in strings scope"
        );

        // "debug" is NOT in code scope (it's in a string)
        assert!(
            !analyzer.validate_in_scope(string_only_source, &pattern, &ScopeFilter::Code),
            "'debug' in string should fail code scope validation"
        );

        // Source where pattern appears only in code
        let code_only_source = r#"
fn main() {
    let debug_mode = true;
}
"#;

        // All matches should be in code scope
        assert!(
            analyzer.validate_in_scope(code_only_source, &pattern, &ScopeFilter::Code),
            "'debug' in variable name should pass code scope validation"
        );
    }

    #[test]
    fn test_validate_matches_detailed() {
        let mut analyzer = SyntaxAnalyzer::new(Language::Rust).unwrap();
        let source = r#"
fn main() {
    let msg = "TODO: fix this";  // TODO: review
}
"#;
        let pattern = regex::Regex::new(r"TODO").unwrap();

        let details = analyzer.validate_matches_detailed(source, &pattern, &ScopeFilter::Code);

        // Should find 2 TODOs (one in string, one in comment)
        assert_eq!(details.len(), 2, "Should find 2 TODO matches");

        // Neither should be in code scope (both in string and comment)
        let in_code_count = details.iter().filter(|(_, _, in_scope)| *in_scope).count();
        assert_eq!(in_code_count, 0, "No TODO should be in code scope");
    }
}
