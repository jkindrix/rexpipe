//! First-class pipeline testing support.
//!
//! This module provides built-in testing capabilities for rexpipe pipelines:
//!
//! - **Inline test cases**: Define tests directly in pipeline configuration
//! - **Test execution**: Run tests with detailed reporting
//! - **Coverage tracking**: Measure which patterns are exercised
//! - **Regression testing**: Ensure pipelines behave consistently
//!
//! ## Example
//!
//! ```toml
//! name = "sanitizer"
//!
//! [[step]]
//! type = "substitute"
//! pattern = '\d{3}-\d{2}-\d{4}'
//! replacement = "XXX-XX-XXXX"
//!
//! [[test]]
//! name = "ssn_redaction"
//! input = "SSN: 123-45-6789"
//! expected = "SSN: XXX-XX-XXXX"
//!
//! [[test]]
//! name = "no_false_positive"
//! input = "Phone: 555-1234"
//! expected = "Phone: 555-1234"
//! ```

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Errors that can occur during testing.
#[derive(Error, Debug)]
pub enum TestError {
    #[error("Test '{name}' failed: expected '{expected}', got '{actual}'")]
    AssertionFailed {
        name: String,
        expected: String,
        actual: String,
    },

    #[error("Test '{name}' unexpectedly matched")]
    UnexpectedMatch { name: String },

    #[error("Test '{name}' failed to match when expected")]
    ExpectedMatch { name: String },

    #[error("Pipeline execution failed: {0}")]
    PipelineError(String),

    #[error("Test timeout after {0:?}")]
    Timeout(Duration),

    #[error("Invalid test configuration: {0}")]
    InvalidConfig(String),
}

pub type Result<T> = std::result::Result<T, TestError>;

/// A single test case for a pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    /// Test name/identifier
    pub name: String,

    /// Input text to process
    pub input: String,

    /// Expected output (for positive tests)
    #[serde(default)]
    pub expected: Option<String>,

    /// Whether this is a negative test (should NOT match)
    #[serde(default)]
    pub should_not_match: bool,

    /// Expected number of matches
    #[serde(default)]
    pub expected_matches: Option<u64>,

    /// Expected number of transformations
    #[serde(default)]
    pub expected_transformations: Option<u64>,

    /// Test description
    #[serde(default)]
    pub description: Option<String>,

    /// Tags for filtering tests
    #[serde(default)]
    pub tags: Vec<String>,

    /// Whether to skip this test
    #[serde(default)]
    pub skip: bool,

    /// Reason for skipping
    #[serde(default)]
    pub skip_reason: Option<String>,
}

impl TestCase {
    /// Create a new positive test case.
    pub fn new(
        name: impl Into<String>,
        input: impl Into<String>,
        expected: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            input: input.into(),
            expected: Some(expected.into()),
            should_not_match: false,
            expected_matches: None,
            expected_transformations: None,
            description: None,
            tags: Vec::new(),
            skip: false,
            skip_reason: None,
        }
    }

    /// Create a negative test case (should not match).
    pub fn negative(name: impl Into<String>, input: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            input: input.into(),
            expected: None,
            should_not_match: true,
            expected_matches: None,
            expected_transformations: None,
            description: None,
            tags: Vec::new(),
            skip: false,
            skip_reason: None,
        }
    }

    /// Set expected match count.
    pub fn with_expected_matches(mut self, count: u64) -> Self {
        self.expected_matches = Some(count);
        self
    }

    /// Add a tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Skip this test.
    pub fn skip(mut self, reason: impl Into<String>) -> Self {
        self.skip = true;
        self.skip_reason = Some(reason.into());
        self
    }
}

/// Result of running a single test.
#[derive(Debug, Clone)]
pub struct TestResult {
    /// Test name
    pub name: String,
    /// Whether the test passed
    pub passed: bool,
    /// Actual output (if produced)
    pub actual_output: Option<String>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Execution time
    pub duration: Duration,
    /// Number of matches found
    pub matches: u64,
    /// Number of transformations applied
    pub transformations: u64,
    /// Whether the test was skipped
    pub skipped: bool,
    /// Skip reason
    pub skip_reason: Option<String>,
}

impl TestResult {
    /// Create a passed result.
    pub fn passed(
        name: impl Into<String>,
        duration: Duration,
        matches: u64,
        transformations: u64,
    ) -> Self {
        Self {
            name: name.into(),
            passed: true,
            actual_output: None,
            error: None,
            duration,
            matches,
            transformations,
            skipped: false,
            skip_reason: None,
        }
    }

    /// Create a failed result.
    pub fn failed(name: impl Into<String>, error: impl Into<String>, duration: Duration) -> Self {
        Self {
            name: name.into(),
            passed: false,
            actual_output: None,
            error: Some(error.into()),
            duration,
            matches: 0,
            transformations: 0,
            skipped: false,
            skip_reason: None,
        }
    }

    /// Create a skipped result.
    pub fn skipped(name: impl Into<String>, reason: Option<String>) -> Self {
        Self {
            name: name.into(),
            passed: true,
            actual_output: None,
            error: None,
            duration: Duration::ZERO,
            matches: 0,
            transformations: 0,
            skipped: true,
            skip_reason: reason,
        }
    }
}

/// Summary of test execution.
#[derive(Debug, Clone, Default)]
pub struct TestSummary {
    /// Total number of tests
    pub total: usize,
    /// Number of passed tests
    pub passed: usize,
    /// Number of failed tests
    pub failed: usize,
    /// Number of skipped tests
    pub skipped: usize,
    /// Total execution time
    pub duration: Duration,
    /// Individual test results
    pub results: Vec<TestResult>,
    /// Coverage information
    pub coverage: TestCoverage,
}

impl TestSummary {
    /// Check if all tests passed.
    pub fn all_passed(&self) -> bool {
        self.failed == 0
    }

    /// Get the pass rate as a percentage.
    pub fn pass_rate(&self) -> f64 {
        if self.total == 0 {
            100.0
        } else {
            (self.passed as f64 / self.total as f64) * 100.0
        }
    }
}

/// Test coverage information.
#[derive(Debug, Clone, Default)]
pub struct TestCoverage {
    /// Steps that were exercised
    pub exercised_steps: Vec<usize>,
    /// Patterns that matched at least once
    pub matched_patterns: Vec<String>,
    /// Steps that were never exercised
    pub unexercised_steps: Vec<usize>,
    /// Coverage percentage
    pub percentage: f64,
}

/// Configuration for test execution.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestConfig {
    /// Run tests in parallel
    #[serde(default)]
    pub parallel: bool,

    /// Stop on first failure
    #[serde(default)]
    pub fail_fast: bool,

    /// Timeout per test in milliseconds
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,

    /// Filter tests by tag
    #[serde(default)]
    pub filter_tags: Vec<String>,

    /// Filter tests by name pattern
    #[serde(default)]
    pub filter_name: Option<String>,

    /// Verbose output
    #[serde(default)]
    pub verbose: bool,
}

fn default_timeout() -> u64 {
    5000
}

impl TestConfig {
    /// Create a new test configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable fail-fast mode.
    pub fn fail_fast(mut self, enabled: bool) -> Self {
        self.fail_fast = enabled;
        self
    }

    /// Set timeout.
    pub fn with_timeout(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }

    /// Filter by tag.
    pub fn with_tag_filter(mut self, tag: impl Into<String>) -> Self {
        self.filter_tags.push(tag.into());
        self
    }

    /// Filter by name pattern.
    pub fn with_name_filter(mut self, pattern: impl Into<String>) -> Self {
        self.filter_name = Some(pattern.into());
        self
    }
}

/// Test runner for pipeline testing.
pub struct TestRunner {
    config: TestConfig,
    tests: Vec<TestCase>,
}

impl TestRunner {
    /// Create a new test runner.
    pub fn new(config: TestConfig) -> Self {
        Self {
            config,
            tests: Vec::new(),
        }
    }

    /// Add a test case.
    pub fn add_test(&mut self, test: TestCase) {
        self.tests.push(test);
    }

    /// Add multiple test cases.
    pub fn add_tests(&mut self, tests: impl IntoIterator<Item = TestCase>) {
        self.tests.extend(tests);
    }

    /// Get the number of tests.
    pub fn test_count(&self) -> usize {
        self.tests.len()
    }

    /// Run a single test with a processor function.
    pub fn run_single<F>(&self, test: &TestCase, processor: F) -> TestResult
    where
        F: Fn(&str) -> std::result::Result<(String, u64, u64), String>,
    {
        // Handle skipped tests
        if test.skip {
            return TestResult::skipped(&test.name, test.skip_reason.clone());
        }

        // Check filters
        if !self.matches_filters(test) {
            return TestResult::skipped(&test.name, Some("Filtered out".to_string()));
        }

        let start = Instant::now();

        // Run the processor
        let result = processor(&test.input);
        let duration = start.elapsed();

        match result {
            Ok((output, matches, transformations)) => {
                // Check expectations
                if test.should_not_match {
                    // Negative test: should have no matches
                    if matches > 0 {
                        return TestResult::failed(
                            &test.name,
                            format!("Expected no matches, got {}", matches),
                            duration,
                        );
                    }
                } else if let Some(ref expected) = test.expected {
                    // Positive test: check output
                    let output_trimmed = output.trim_end_matches('\n');
                    let expected_trimmed = expected.trim_end_matches('\n');

                    if output_trimmed != expected_trimmed {
                        let mut result = TestResult::failed(
                            &test.name,
                            format!("Expected '{}', got '{}'", expected_trimmed, output_trimmed),
                            duration,
                        );
                        result.actual_output = Some(output);
                        return result;
                    }
                }

                // Check match count if specified
                if let Some(expected_matches) = test.expected_matches {
                    if matches != expected_matches {
                        return TestResult::failed(
                            &test.name,
                            format!("Expected {} matches, got {}", expected_matches, matches),
                            duration,
                        );
                    }
                }

                // Check transformation count if specified
                if let Some(expected_trans) = test.expected_transformations {
                    if transformations != expected_trans {
                        return TestResult::failed(
                            &test.name,
                            format!(
                                "Expected {} transformations, got {}",
                                expected_trans, transformations
                            ),
                            duration,
                        );
                    }
                }

                TestResult::passed(&test.name, duration, matches, transformations)
            }
            Err(err) => TestResult::failed(&test.name, err, duration),
        }
    }

    /// Run all tests with a processor function.
    pub fn run_all<F>(&self, processor: F) -> TestSummary
    where
        F: Fn(&str) -> std::result::Result<(String, u64, u64), String> + Clone,
    {
        let start = Instant::now();
        let mut summary = TestSummary::default();

        for test in &self.tests {
            let result = self.run_single(test, processor.clone());

            if result.skipped {
                summary.skipped += 1;
            } else if result.passed {
                summary.passed += 1;
            } else {
                summary.failed += 1;

                if self.config.fail_fast {
                    summary.results.push(result);
                    break;
                }
            }

            summary.results.push(result);
        }

        summary.total = self.tests.len();
        summary.duration = start.elapsed();

        summary
    }

    /// Check if a test matches the configured filters.
    fn matches_filters(&self, test: &TestCase) -> bool {
        // Check tag filter
        if !self.config.filter_tags.is_empty() {
            let has_matching_tag = test
                .tags
                .iter()
                .any(|t| self.config.filter_tags.contains(t));
            if !has_matching_tag {
                return false;
            }
        }

        // Check name filter
        if let Some(ref pattern) = self.config.filter_name {
            if !test.name.contains(pattern) {
                return false;
            }
        }

        true
    }
}

/// Format test results as a human-readable report.
pub fn format_test_report(summary: &TestSummary) -> String {
    let mut report = String::new();

    report.push_str("╔══════════════════════════════════════════════════════════════════╗\n");
    report.push_str("║                    PIPELINE TEST RESULTS                         ║\n");
    report.push_str("╚══════════════════════════════════════════════════════════════════╝\n\n");

    // Overall summary
    let status = if summary.all_passed() {
        "PASSED"
    } else {
        "FAILED"
    };
    let status_indicator = if summary.all_passed() { "✓" } else { "✗" };

    report.push_str(&format!(
        "{} {} - {}/{} tests passed ({:.1}%)\n",
        status_indicator,
        status,
        summary.passed,
        summary.total,
        summary.pass_rate()
    ));

    if summary.skipped > 0 {
        report.push_str(&format!("  {} tests skipped\n", summary.skipped));
    }

    report.push_str(&format!("  Total time: {:?}\n\n", summary.duration));

    // Individual results
    report.push_str("─── Test Results ─────────────────────────────────────────────────\n\n");

    for result in &summary.results {
        let indicator = if result.skipped {
            "⊘"
        } else if result.passed {
            "✓"
        } else {
            "✗"
        };

        let status_text = if result.skipped {
            "SKIP"
        } else if result.passed {
            "PASS"
        } else {
            "FAIL"
        };

        report.push_str(&format!(
            "  {} {} {} ({:?})\n",
            indicator, status_text, result.name, result.duration
        ));

        if let Some(ref error) = result.error {
            report.push_str(&format!("      Error: {}\n", error));
        }

        if let Some(ref reason) = result.skip_reason {
            report.push_str(&format!("      Reason: {}\n", reason));
        }
    }

    report.push_str("\n══════════════════════════════════════════════════════════════════\n");

    report
}

/// Format test results as TAP (Test Anything Protocol) output.
pub fn format_tap_output(summary: &TestSummary) -> String {
    let mut output = String::new();

    output.push_str("TAP version 14\n");
    output.push_str(&format!("1..{}\n", summary.total));

    for (i, result) in summary.results.iter().enumerate() {
        let num = i + 1;

        if result.skipped {
            output.push_str(&format!(
                "ok {} - {} # SKIP {}\n",
                num,
                result.name,
                result.skip_reason.as_deref().unwrap_or("skipped")
            ));
        } else if result.passed {
            output.push_str(&format!("ok {} - {}\n", num, result.name));
        } else {
            output.push_str(&format!("not ok {} - {}\n", num, result.name));
            if let Some(ref error) = result.error {
                output.push_str(&format!("  ---\n  message: '{}'\n  ...\n", error));
            }
        }
    }

    output
}

/// Format test results as JUnit XML.
pub fn format_junit_xml(summary: &TestSummary, suite_name: &str) -> String {
    let mut xml = String::new();

    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str(&format!(
        "<testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" skipped=\"{}\" time=\"{:.3}\">\n",
        escape_xml(suite_name),
        summary.total,
        summary.failed,
        summary.skipped,
        summary.duration.as_secs_f64()
    ));

    for result in &summary.results {
        xml.push_str(&format!(
            "  <testcase name=\"{}\" time=\"{:.3}\"",
            escape_xml(&result.name),
            result.duration.as_secs_f64()
        ));

        if result.skipped {
            xml.push_str(">\n");
            xml.push_str(&format!(
                "    <skipped message=\"{}\"/>\n",
                escape_xml(result.skip_reason.as_deref().unwrap_or("skipped"))
            ));
            xml.push_str("  </testcase>\n");
        } else if !result.passed {
            xml.push_str(">\n");
            xml.push_str(&format!(
                "    <failure message=\"{}\"/>\n",
                escape_xml(result.error.as_deref().unwrap_or("test failed"))
            ));
            xml.push_str("  </testcase>\n");
        } else {
            xml.push_str("/>\n");
        }
    }

    xml.push_str("</testsuite>\n");

    xml
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Pipeline test configuration section.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PipelineTestConfig {
    /// Test cases
    #[serde(default, rename = "test")]
    pub tests: Vec<TestCase>,

    /// Test configuration
    #[serde(default)]
    pub test_config: TestConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_case_creation() {
        let test = TestCase::new("basic", "input text", "expected output");
        assert_eq!(test.name, "basic");
        assert!(!test.should_not_match);
        assert_eq!(test.expected, Some("expected output".to_string()));
    }

    #[test]
    fn test_negative_test_case() {
        let test = TestCase::negative("no-match", "some input");
        assert!(test.should_not_match);
        assert!(test.expected.is_none());
    }

    #[test]
    fn test_test_runner_simple() {
        let config = TestConfig::new();
        let mut runner = TestRunner::new(config);

        runner.add_test(TestCase::new("test1", "hello", "hello"));
        runner.add_test(TestCase::new("test2", "world", "world"));

        let summary = runner.run_all(|input| Ok((input.to_string(), 0, 0)));

        assert_eq!(summary.total, 2);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 0);
        assert!(summary.all_passed());
    }

    #[test]
    fn test_test_runner_failure() {
        let config = TestConfig::new();
        let mut runner = TestRunner::new(config);

        runner.add_test(TestCase::new("fail", "input", "different"));

        let summary = runner.run_all(|input| Ok((input.to_string(), 0, 0)));

        assert_eq!(summary.failed, 1);
        assert!(!summary.all_passed());
    }

    #[test]
    fn test_negative_test() {
        let config = TestConfig::new();
        let mut runner = TestRunner::new(config);

        runner.add_test(TestCase::negative("no-match", "input"));

        // Processor returns matches
        let summary = runner.run_all(|_| Ok(("output".to_string(), 1, 1)));

        // Should fail because we expected no matches
        assert_eq!(summary.failed, 1);
    }

    #[test]
    fn test_skipped_test() {
        let config = TestConfig::new();
        let mut runner = TestRunner::new(config);

        runner.add_test(TestCase::new("skip-me", "input", "output").skip("Not implemented yet"));

        let summary = runner.run_all(|_| Ok(("output".to_string(), 0, 0)));

        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.passed, 0);
    }

    #[test]
    fn test_format_tap_output() {
        let mut summary = TestSummary {
            total: 2,
            passed: 1,
            failed: 1,
            ..Default::default()
        };
        summary
            .results
            .push(TestResult::passed("test1", Duration::from_millis(10), 0, 0));
        summary.results.push(TestResult::failed(
            "test2",
            "assertion failed",
            Duration::from_millis(5),
        ));

        let tap = format_tap_output(&summary);
        assert!(tap.contains("TAP version 14"));
        assert!(tap.contains("ok 1 - test1"));
        assert!(tap.contains("not ok 2 - test2"));
    }

    #[test]
    fn test_format_junit_xml() {
        let mut summary = TestSummary {
            total: 1,
            passed: 1,
            ..Default::default()
        };
        summary.results.push(TestResult::passed(
            "test1",
            Duration::from_millis(100),
            0,
            0,
        ));

        let xml = format_junit_xml(&summary, "my-suite");
        assert!(xml.contains("<?xml version"));
        assert!(xml.contains("testsuite name=\"my-suite\""));
        assert!(xml.contains("testcase name=\"test1\""));
    }

    #[test]
    fn test_fail_fast() {
        let config = TestConfig::new().fail_fast(true);
        let mut runner = TestRunner::new(config);

        runner.add_test(TestCase::new("pass", "a", "a"));
        runner.add_test(TestCase::new("fail", "b", "c"));
        runner.add_test(TestCase::new("not-run", "d", "d"));

        let summary = runner.run_all(|input| Ok((input.to_string(), 0, 0)));

        // Should stop after failure
        assert_eq!(summary.results.len(), 2);
    }
}
