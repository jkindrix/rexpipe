//! Plugin system for custom transformations.
//!
//! This module provides an extensible plugin system for rexpipe, allowing users
//! to register custom transformation functions that can be used in pipelines.
//!
//! # Plugin Types
//!
//! - **Builtin plugins**: Pre-registered common transformations
//! - **Custom plugins**: User-defined functions registered at runtime
//! - **Shell plugins**: External commands executed via the shell
//!
//! # Built-in Plugins Reference
//!
//! ## String Manipulation
//!
//! | Plugin | Args | Description |
//! |--------|------|-------------|
//! | `reverse` | - | Reverse the string characters |
//! | `repeat` | `count` (default: 2) | Repeat the string N times |
//! | `slice` | `start`, `end` | Extract substring by character position |
//! | `pad_left` | `width`, `char` (default: space) | Left-pad to width |
//! | `pad_right` | `width`, `char` (default: space) | Right-pad to width |
//! | `truncate` | `max_len`, `suffix` (default: "...") | Truncate with ellipsis |
//! | `squeeze` | - | Collapse multiple whitespace to single space |
//! | `strip_prefix` | `prefix` | Remove prefix if present |
//! | `strip_suffix` | `suffix` | Remove suffix if present |
//!
//! ## Case Transformations
//!
//! | Plugin | Description |
//! |--------|-------------|
//! | `snake_case` | Convert to snake_case |
//! | `camel_case` | Convert to camelCase |
//! | `pascal_case` | Convert to PascalCase |
//! | `kebab_case` | Convert to kebab-case |
//!
//! ## Text Analysis
//!
//! | Plugin | Description |
//! |--------|-------------|
//! | `length` | Return string length in bytes |
//! | `word_count` | Count words (whitespace-separated) |
//! | `line_count` | Count lines (newline-separated) |
//! | `char_freq` | Character frequency analysis |
//!
//! ## Numeric Operations
//!
//! | Plugin | Args | Description |
//! |--------|------|-------------|
//! | `increment` | `amount` (default: 1) | Add to numeric string |
//! | `decrement` | `amount` (default: 1) | Subtract from numeric string |
//! | `format_number` | - | Add thousand separators |
//!
//! ## Encoding
//!
//! | Plugin | Description |
//! |--------|-------------|
//! | `hex_encode` | Encode string as hexadecimal |
//! | `hex_decode` | Decode hexadecimal to string |
//!
//! ## Utility
//!
//! | Plugin | Description |
//! |--------|-------------|
//! | `timestamp` | Return current Unix timestamp |
//!
//! # Custom Plugin Example
//!
//! ```rust
//! use rexpipe::plugin::{PluginRegistry, TransformFn};
//!
//! let mut registry = PluginRegistry::new();
//!
//! // Register a custom plugin
//! registry.register("double", |input, _args| {
//!     format!("{}{}", input, input)
//! });
//!
//! // Use the plugin
//! let result = registry.execute("double", "hello", &[]).unwrap();
//! assert_eq!(result, "hellohello");
//! ```
//!
//! # Shell Plugin Example
//!
//! Shell plugins allow external commands to transform text. Input is passed via
//! stdin and output is captured from stdout.
//!
//! ```rust,no_run
//! use rexpipe::plugin::PluginRegistry;
//!
//! // Execute a shell command (Unix-like systems)
//! let result = PluginRegistry::execute_shell("tr 'a-z' 'A-Z'", "hello");
//! // result is Ok("HELLO")
//! ```
//!
//! ## Security Note
//!
//! Shell commands receive input via stdin, not through command-line interpolation,
//! preventing command injection. However, the command string itself should still
//! be treated as trusted input.

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::{Arc, LazyLock};

/// Global plugin registry instance with built-in plugins pre-registered.
///
/// This static registry is initialized once and reused for all plugin
/// executions, avoiding the overhead of re-registering built-in plugins
/// for every transform operation.
static GLOBAL_REGISTRY: LazyLock<PluginRegistry> = LazyLock::new(PluginRegistry::new);

/// Type alias for plugin transformation functions
///
/// A transform function takes the matched text and optional arguments,
/// returning the transformed text.
pub type TransformFn = Arc<dyn Fn(&str, &[String]) -> String + Send + Sync>;

/// Registry for custom transformation plugins
///
/// The registry holds both built-in and user-registered transformation functions
/// that can be invoked by name during pipeline execution.
#[derive(Clone)]
pub struct PluginRegistry {
    plugins: HashMap<String, TransformFn>,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRegistry {
    /// Create a new plugin registry with built-in plugins
    pub fn new() -> Self {
        let mut registry = Self {
            plugins: HashMap::new(),
        };
        registry.register_builtins();
        registry
    }

    /// Get a reference to the global plugin registry.
    ///
    /// This is the recommended way to access built-in plugins for transform
    /// operations. The global registry is initialized once and reused,
    /// avoiding the overhead of re-registering plugins.
    ///
    /// # Example
    ///
    /// ```
    /// use rexpipe::plugin::PluginRegistry;
    ///
    /// let registry = PluginRegistry::global();
    /// let result = registry.execute("reverse", "hello", &[]).unwrap();
    /// assert_eq!(result, "olleh");
    /// ```
    pub fn global() -> &'static PluginRegistry {
        &GLOBAL_REGISTRY
    }

    /// Register built-in plugin functions
    fn register_builtins(&mut self) {
        // String manipulation
        self.register("reverse", |s, _| s.chars().rev().collect());
        self.register("repeat", |s, args| {
            let count: usize = args.first().and_then(|a| a.parse().ok()).unwrap_or(2);
            s.repeat(count)
        });
        self.register("slice", |s, args| {
            let start: usize = args.first().and_then(|a| a.parse().ok()).unwrap_or(0);
            let end: usize = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(s.len());
            s.chars()
                .skip(start)
                .take(end.saturating_sub(start))
                .collect()
        });
        self.register("pad_left", |s, args| {
            let width: usize = args.first().and_then(|a| a.parse().ok()).unwrap_or(10);
            let pad_char: char = args.get(1).and_then(|a| a.chars().next()).unwrap_or(' ');
            format!("{:>width$}", s, width = width)
                .chars()
                .map(|c| if c == ' ' { pad_char } else { c })
                .collect()
        });
        self.register("pad_right", |s, args| {
            let width: usize = args.first().and_then(|a| a.parse().ok()).unwrap_or(10);
            let pad_char: char = args.get(1).and_then(|a| a.chars().next()).unwrap_or(' ');
            format!("{:<width$}", s, width = width)
                .chars()
                .map(|c| if c == ' ' { pad_char } else { c })
                .collect()
        });
        self.register("truncate", |s, args| {
            let max_len: usize = args.first().and_then(|a| a.parse().ok()).unwrap_or(10);
            let suffix: &str = args.get(1).map(|s| s.as_str()).unwrap_or("...");
            if s.len() > max_len {
                format!("{}{}", &s[..max_len.saturating_sub(suffix.len())], suffix)
            } else {
                s.to_string()
            }
        });

        // Case transformations
        self.register("snake_case", |s, _| {
            let mut result = String::new();
            for (i, c) in s.chars().enumerate() {
                if c.is_uppercase() && i > 0 {
                    result.push('_');
                }
                result.push(c.to_lowercase().next().unwrap_or(c));
            }
            result
        });
        self.register("camel_case", |s, _| {
            s.split(&[' ', '_', '-'][..])
                .enumerate()
                .map(|(i, word)| {
                    if i == 0 {
                        word.to_lowercase()
                    } else {
                        let mut chars = word.chars();
                        match chars.next() {
                            None => String::new(),
                            Some(c) => {
                                c.to_uppercase().collect::<String>()
                                    + chars.as_str().to_lowercase().as_str()
                            }
                        }
                    }
                })
                .collect()
        });
        self.register("pascal_case", |s, _| {
            s.split(&[' ', '_', '-'][..])
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(c) => {
                            c.to_uppercase().collect::<String>()
                                + chars.as_str().to_lowercase().as_str()
                        }
                    }
                })
                .collect()
        });
        self.register("kebab_case", |s, _| {
            let mut result = String::new();
            for (i, c) in s.chars().enumerate() {
                if c.is_uppercase() && i > 0 {
                    result.push('-');
                }
                result.push(c.to_lowercase().next().unwrap_or(c));
            }
            result.replace([' ', '_'], "-")
        });

        // Text analysis
        self.register("length", |s, _| s.len().to_string());
        self.register("word_count", |s, _| {
            s.split_whitespace().count().to_string()
        });
        self.register("line_count", |s, _| s.lines().count().to_string());
        self.register("char_freq", |s, _| {
            let mut freq: HashMap<char, usize> = HashMap::new();
            for c in s.chars() {
                *freq.entry(c).or_insert(0) += 1;
            }
            let mut pairs: Vec<_> = freq.into_iter().collect();
            pairs.sort_by(|a, b| b.1.cmp(&a.1));
            pairs
                .into_iter()
                .take(5)
                .map(|(c, n)| format!("{}:{}", c, n))
                .collect::<Vec<_>>()
                .join(",")
        });

        // Encoding
        self.register("hex_encode", |s, _| {
            s.bytes().map(|b| format!("{:02x}", b)).collect()
        });
        self.register("hex_decode", |s, _| {
            let bytes: Vec<u8> = (0..s.len())
                .step_by(2)
                .filter_map(|i| u8::from_str_radix(&s[i..i + 2.min(s.len())], 16).ok())
                .collect();
            String::from_utf8_lossy(&bytes).to_string()
        });

        // Whitespace handling
        self.register("squeeze", |s, _| {
            let mut result = String::new();
            let mut last_was_space = false;
            for c in s.chars() {
                if c.is_whitespace() {
                    if !last_was_space {
                        result.push(' ');
                        last_was_space = true;
                    }
                } else {
                    result.push(c);
                    last_was_space = false;
                }
            }
            result
        });
        self.register("strip_prefix", |s, args| {
            args.first()
                .map(|prefix| s.strip_prefix(prefix.as_str()).unwrap_or(s))
                .unwrap_or(s)
                .to_string()
        });
        self.register("strip_suffix", |s, args| {
            args.first()
                .map(|suffix| s.strip_suffix(suffix.as_str()).unwrap_or(s))
                .unwrap_or(s)
                .to_string()
        });

        // Numeric
        self.register("increment", |s, args| {
            let delta: i64 = args.first().and_then(|a| a.parse().ok()).unwrap_or(1);
            s.parse::<i64>()
                .map(|n| (n + delta).to_string())
                .unwrap_or_else(|_| s.to_string())
        });
        self.register("decrement", |s, args| {
            let delta: i64 = args.first().and_then(|a| a.parse().ok()).unwrap_or(1);
            s.parse::<i64>()
                .map(|n| (n - delta).to_string())
                .unwrap_or_else(|_| s.to_string())
        });
        self.register("format_number", |s, _| {
            s.parse::<i64>()
                .map(|n| {
                    let s = n.abs().to_string();
                    let mut result = String::new();
                    for (i, c) in s.chars().rev().enumerate() {
                        if i > 0 && i % 3 == 0 {
                            result.push(',');
                        }
                        result.push(c);
                    }
                    if n < 0 {
                        result.push('-');
                    }
                    result.chars().rev().collect()
                })
                .unwrap_or_else(|_| s.to_string())
        });

        // Date/time (simple formats)
        self.register("timestamp", |_, _| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs().to_string())
                .unwrap_or_else(|_| "0".to_string())
        });
    }

    /// Register a custom plugin function
    ///
    /// # Arguments
    ///
    /// * `name` - The name to register the plugin under
    /// * `func` - The transformation function
    ///
    /// # Example
    ///
    /// ```rust
    /// use rexpipe::plugin::PluginRegistry;
    ///
    /// let mut registry = PluginRegistry::new();
    /// registry.register("exclaim", |s, _| format!("{}!", s));
    /// ```
    pub fn register<F>(&mut self, name: &str, func: F)
    where
        F: Fn(&str, &[String]) -> String + Send + Sync + 'static,
    {
        self.plugins.insert(name.to_string(), Arc::new(func));
    }

    /// Check if a plugin is registered
    pub fn has_plugin(&self, name: &str) -> bool {
        self.plugins.contains_key(name)
    }

    /// List all registered plugin names
    pub fn list_plugins(&self) -> Vec<&str> {
        let mut names: Vec<_> = self.plugins.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }

    /// Execute a registered plugin
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the plugin to execute
    /// * `input` - The input text to transform
    /// * `args` - Additional arguments for the plugin
    ///
    /// # Returns
    ///
    /// The transformed text, or an error if the plugin is not found
    pub fn execute(&self, name: &str, input: &str, args: &[String]) -> Result<String, String> {
        self.plugins
            .get(name)
            .map(|func| func(input, args))
            .ok_or_else(|| {
                format!(
                    "Plugin not found: '{}'. Available: {:?}",
                    name,
                    self.list_plugins()
                )
            })
    }

    /// Execute a shell command as a transformation
    ///
    /// The input text is passed to the command via stdin, and the command's
    /// stdout is returned as the transformed text.
    ///
    /// # Arguments
    ///
    /// * `command` - The shell command to execute
    /// * `input` - The input text to pass to the command
    ///
    /// # Returns
    ///
    /// The command's stdout, or an error message
    ///
    /// # Security Considerations
    ///
    /// This executes shell commands defined in the configuration file.
    /// The matched text is passed via stdin (NOT interpolated into the command),
    /// which prevents shell injection from regex matches.
    ///
    /// **Safe pattern**: The command template comes from the config file (user-controlled)
    /// and the matched text is piped to stdin, not embedded in the command string.
    ///
    /// **Timeout**: Commands have a 30-second timeout to prevent hanging.
    ///
    /// # Example Configuration (TOML)
    /// ```toml
    /// [[step]]
    /// type = "transform"
    /// pattern = "\\d+"
    /// transform_action = { shell = { command = "python -c 'import sys; print(int(sys.stdin.read()) * 2)'" } }
    /// ```
    ///
    /// Uses the default timeout of 30 seconds. For configurable timeout,
    /// use [`Self::execute_shell_with_timeout`].
    pub fn execute_shell(command: &str, input: &str) -> Result<String, String> {
        Self::execute_shell_with_timeout(command, input, 30)
    }

    /// Execute a shell command with input and configurable timeout.
    ///
    /// # Arguments
    ///
    /// * `command` - The shell command to execute
    /// * `input` - Input to pass to the command via stdin
    /// * `timeout_secs` - Maximum execution time in seconds (0 = no timeout)
    ///
    /// # Returns
    ///
    /// The command's stdout output on success, or an error message on failure.
    pub fn execute_shell_with_timeout(
        command: &str,
        input: &str,
        timeout_secs: u64,
    ) -> Result<String, String> {
        use std::io::Write;
        use std::time::Duration;

        #[cfg(target_os = "windows")]
        let shell_cmd = ("cmd", "/C");
        #[cfg(not(target_os = "windows"))]
        let shell_cmd = ("sh", "-c");

        let mut child = Command::new(shell_cmd.0)
            .arg(shell_cmd.1)
            .arg(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn shell command: {}", e))?;

        // Write input to stdin (safe - no shell interpolation)
        if let Some(ref mut stdin) = child.stdin {
            stdin
                .write_all(input.as_bytes())
                .map_err(|e| format!("Failed to write to stdin: {}", e))?;
        }
        // Close stdin so the child process knows input is complete
        drop(child.stdin.take());

        // Wait with timeout to prevent hanging commands (0 = no timeout)
        let timeout = if timeout_secs > 0 {
            Some(Duration::from_secs(timeout_secs))
        } else {
            None
        };
        let start = std::time::Instant::now();

        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    // Process exited, read output
                    let output = child
                        .wait_with_output()
                        .map_err(|e| format!("Failed to read command output: {}", e))?;

                    if status.success() {
                        return Ok(String::from_utf8_lossy(&output.stdout)
                            .trim_end()
                            .to_string());
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        return Err(format!("Command failed: {}", stderr.trim()));
                    }
                }
                Ok(None) => {
                    // Still running, check timeout
                    if let Some(timeout_duration) = timeout {
                        if start.elapsed() >= timeout_duration {
                            let _ = child.kill();
                            return Err(format!(
                                "Shell command timed out after {} seconds",
                                timeout_secs
                            ));
                        }
                    }
                    // Brief sleep before retry
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => return Err(format!("Failed to check command status: {}", e)),
            }
        }
    }
}

impl std::fmt::Debug for PluginRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginRegistry")
            .field("plugins", &self.list_plugins())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_reverse() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.execute("reverse", "hello", &[]).unwrap(), "olleh");
    }

    #[test]
    fn test_builtin_reverse_empty() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.execute("reverse", "", &[]).unwrap(), "");
    }

    #[test]
    fn test_builtin_reverse_unicode() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.execute("reverse", "日本語", &[]).unwrap(), "語本日");
    }

    #[test]
    fn test_builtin_repeat() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry
                .execute("repeat", "ab", &["3".to_string()])
                .unwrap(),
            "ababab"
        );
    }

    #[test]
    fn test_builtin_repeat_default() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.execute("repeat", "x", &[]).unwrap(), "xx");
    }

    #[test]
    fn test_builtin_repeat_zero() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry.execute("repeat", "x", &["0".to_string()]).unwrap(),
            ""
        );
    }

    #[test]
    fn test_builtin_slice() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry
                .execute("slice", "hello", &["1".to_string(), "4".to_string()])
                .unwrap(),
            "ell"
        );
    }

    #[test]
    fn test_builtin_slice_defaults() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.execute("slice", "hello", &[]).unwrap(), "hello");
    }

    #[test]
    fn test_builtin_slice_start_only() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry
                .execute("slice", "hello", &["2".to_string()])
                .unwrap(),
            "llo"
        );
    }

    #[test]
    fn test_builtin_pad_left() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry
                .execute("pad_left", "hi", &["5".to_string()])
                .unwrap(),
            "   hi"
        );
    }

    #[test]
    fn test_builtin_pad_left_custom_char() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry
                .execute("pad_left", "hi", &["5".to_string(), "0".to_string()])
                .unwrap(),
            "000hi"
        );
    }

    #[test]
    fn test_builtin_pad_right() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry
                .execute("pad_right", "hi", &["5".to_string()])
                .unwrap(),
            "hi   "
        );
    }

    #[test]
    fn test_builtin_truncate() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry
                .execute("truncate", "hello world", &["8".to_string()])
                .unwrap(),
            "hello..."
        );
    }

    #[test]
    fn test_builtin_truncate_no_truncation() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry
                .execute("truncate", "hi", &["10".to_string()])
                .unwrap(),
            "hi"
        );
    }

    #[test]
    fn test_builtin_truncate_custom_suffix() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry
                .execute(
                    "truncate",
                    "hello world",
                    &["8".to_string(), ">>".to_string()]
                )
                .unwrap(),
            "hello >>"
        );
    }

    #[test]
    fn test_builtin_snake_case() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry.execute("snake_case", "helloWorld", &[]).unwrap(),
            "hello_world"
        );
    }

    #[test]
    fn test_builtin_snake_case_already_snake() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry.execute("snake_case", "hello_world", &[]).unwrap(),
            "hello_world"
        );
    }

    #[test]
    fn test_builtin_camel_case() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry.execute("camel_case", "hello_world", &[]).unwrap(),
            "helloWorld"
        );
    }

    #[test]
    fn test_builtin_camel_case_from_kebab() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry.execute("camel_case", "hello-world", &[]).unwrap(),
            "helloWorld"
        );
    }

    #[test]
    fn test_builtin_pascal_case() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry.execute("pascal_case", "hello_world", &[]).unwrap(),
            "HelloWorld"
        );
    }

    #[test]
    fn test_builtin_pascal_case_from_space() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry.execute("pascal_case", "hello world", &[]).unwrap(),
            "HelloWorld"
        );
    }

    #[test]
    fn test_builtin_kebab_case() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry.execute("kebab_case", "helloWorld", &[]).unwrap(),
            "hello-world"
        );
    }

    #[test]
    fn test_builtin_kebab_case_from_spaces() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry.execute("kebab_case", "hello world", &[]).unwrap(),
            "hello-world"
        );
    }

    #[test]
    fn test_builtin_length() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.execute("length", "hello", &[]).unwrap(), "5");
    }

    #[test]
    fn test_builtin_length_empty() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.execute("length", "", &[]).unwrap(), "0");
    }

    #[test]
    fn test_builtin_length_unicode() {
        let registry = PluginRegistry::new();
        // Note: length counts bytes, not characters
        assert_eq!(registry.execute("length", "日", &[]).unwrap(), "3");
    }

    #[test]
    fn test_builtin_word_count() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry
                .execute("word_count", "hello world foo", &[])
                .unwrap(),
            "3"
        );
    }

    #[test]
    fn test_builtin_word_count_empty() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.execute("word_count", "", &[]).unwrap(), "0");
    }

    #[test]
    fn test_builtin_word_count_extra_whitespace() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry
                .execute("word_count", "  hello   world  ", &[])
                .unwrap(),
            "2"
        );
    }

    #[test]
    fn test_builtin_line_count() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry
                .execute("line_count", "line1\nline2\nline3", &[])
                .unwrap(),
            "3"
        );
    }

    #[test]
    fn test_builtin_line_count_single() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry.execute("line_count", "single line", &[]).unwrap(),
            "1"
        );
    }

    #[test]
    fn test_builtin_char_freq() {
        let registry = PluginRegistry::new();
        let result = registry.execute("char_freq", "aaabbc", &[]).unwrap();
        // Should have 'a' as most frequent
        assert!(result.starts_with("a:3"));
    }

    #[test]
    fn test_builtin_hex_encode() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry.execute("hex_encode", "AB", &[]).unwrap(),
            "4142"
        );
    }

    #[test]
    fn test_builtin_hex_decode() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry.execute("hex_decode", "4142", &[]).unwrap(),
            "AB"
        );
    }

    #[test]
    fn test_builtin_hex_roundtrip() {
        let registry = PluginRegistry::new();
        let original = "Hello";
        let encoded = registry.execute("hex_encode", original, &[]).unwrap();
        let decoded = registry.execute("hex_decode", &encoded, &[]).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_builtin_squeeze() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry
                .execute("squeeze", "hello   world  foo", &[])
                .unwrap(),
            "hello world foo"
        );
    }

    #[test]
    fn test_builtin_squeeze_tabs_and_newlines() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry
                .execute("squeeze", "hello\t\t\nworld", &[])
                .unwrap(),
            "hello world"
        );
    }

    #[test]
    fn test_builtin_strip_prefix() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry
                .execute("strip_prefix", "hello_world", &["hello_".to_string()])
                .unwrap(),
            "world"
        );
    }

    #[test]
    fn test_builtin_strip_prefix_no_match() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry
                .execute("strip_prefix", "hello_world", &["foo_".to_string()])
                .unwrap(),
            "hello_world"
        );
    }

    #[test]
    fn test_builtin_strip_suffix() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry
                .execute("strip_suffix", "hello_world", &["_world".to_string()])
                .unwrap(),
            "hello"
        );
    }

    #[test]
    fn test_builtin_strip_suffix_no_match() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry
                .execute("strip_suffix", "hello_world", &["_foo".to_string()])
                .unwrap(),
            "hello_world"
        );
    }

    #[test]
    fn test_builtin_increment() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.execute("increment", "42", &[]).unwrap(), "43");
        assert_eq!(
            registry
                .execute("increment", "42", &["10".to_string()])
                .unwrap(),
            "52"
        );
    }

    #[test]
    fn test_builtin_increment_negative() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.execute("increment", "-5", &[]).unwrap(), "-4");
    }

    #[test]
    fn test_builtin_increment_non_number() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry.execute("increment", "not_a_number", &[]).unwrap(),
            "not_a_number"
        );
    }

    #[test]
    fn test_builtin_decrement() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.execute("decrement", "42", &[]).unwrap(), "41");
        assert_eq!(
            registry
                .execute("decrement", "42", &["10".to_string()])
                .unwrap(),
            "32"
        );
    }

    #[test]
    fn test_builtin_format_number() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry.execute("format_number", "1234567", &[]).unwrap(),
            "1,234,567"
        );
    }

    #[test]
    fn test_builtin_format_number_small() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry.execute("format_number", "123", &[]).unwrap(),
            "123"
        );
    }

    #[test]
    fn test_builtin_format_number_negative() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry.execute("format_number", "-1234567", &[]).unwrap(),
            "-1,234,567"
        );
    }

    #[test]
    fn test_builtin_timestamp() {
        let registry = PluginRegistry::new();
        let result = registry.execute("timestamp", "", &[]).unwrap();
        // Should be a valid Unix timestamp (non-empty numeric string)
        let ts: u64 = result.parse().unwrap();
        assert!(ts > 0);
    }

    #[test]
    fn test_custom_plugin() {
        let mut registry = PluginRegistry::new();
        registry.register("double", |s, _| format!("{}{}", s, s));
        assert_eq!(registry.execute("double", "hi", &[]).unwrap(), "hihi");
    }

    #[test]
    fn test_custom_plugin_with_args() {
        let mut registry = PluginRegistry::new();
        registry.register("wrap", |s, args| {
            let prefix = args.first().map(|a| a.as_str()).unwrap_or("[");
            let suffix = args.get(1).map(|a| a.as_str()).unwrap_or("]");
            format!("{}{}{}", prefix, s, suffix)
        });
        assert_eq!(
            registry
                .execute("wrap", "text", &["<".to_string(), ">".to_string()])
                .unwrap(),
            "<text>"
        );
    }

    #[test]
    fn test_custom_plugin_overrides_builtin() {
        let mut registry = PluginRegistry::new();
        registry.register("reverse", |s, _| format!("custom:{}", s));
        assert_eq!(
            registry.execute("reverse", "test", &[]).unwrap(),
            "custom:test"
        );
    }

    #[test]
    fn test_plugin_not_found() {
        let registry = PluginRegistry::new();
        let result = registry.execute("nonexistent", "test", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_plugin_not_found_lists_available() {
        let registry = PluginRegistry::new();
        let result = registry.execute("nonexistent", "test", &[]);
        let err = result.unwrap_err();
        assert!(err.contains("reverse")); // Should list available plugins
    }

    #[test]
    fn test_has_plugin() {
        let registry = PluginRegistry::new();
        assert!(registry.has_plugin("reverse"));
        assert!(registry.has_plugin("snake_case"));
        assert!(!registry.has_plugin("nonexistent"));
    }

    #[test]
    fn test_list_plugins() {
        let registry = PluginRegistry::new();
        let plugins = registry.list_plugins();
        assert!(plugins.contains(&"reverse"));
        assert!(plugins.contains(&"snake_case"));
        assert!(plugins.contains(&"length"));
        assert!(plugins.contains(&"increment"));
        // Should be sorted
        assert!(plugins.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn test_default_trait() {
        let registry = PluginRegistry::default();
        assert!(registry.has_plugin("reverse"));
    }

    #[test]
    fn test_debug_trait() {
        let registry = PluginRegistry::new();
        let debug_str = format!("{:?}", registry);
        assert!(debug_str.contains("PluginRegistry"));
        assert!(debug_str.contains("reverse"));
    }

    #[test]
    fn test_clone_trait() {
        let registry = PluginRegistry::new();
        let cloned = registry.clone();
        assert!(cloned.has_plugin("reverse"));
        assert_eq!(
            registry.list_plugins().len(),
            cloned.list_plugins().len()
        );
    }

    #[test]
    fn test_shell_command() {
        // Only run on Unix-like systems
        #[cfg(not(target_os = "windows"))]
        {
            let result = PluginRegistry::execute_shell("cat", "hello");
            assert_eq!(result.unwrap(), "hello");

            let result = PluginRegistry::execute_shell("tr 'a-z' 'A-Z'", "hello");
            assert_eq!(result.unwrap(), "HELLO");
        }
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_shell_command_with_newlines() {
        let result = PluginRegistry::execute_shell("cat", "line1\nline2");
        assert_eq!(result.unwrap(), "line1\nline2");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_shell_command_failure() {
        let result = PluginRegistry::execute_shell("exit 1", "input");
        assert!(result.is_err());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_shell_command_invalid() {
        let result = PluginRegistry::execute_shell("nonexistent_command_xyz", "input");
        assert!(result.is_err());
    }
}
