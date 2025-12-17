//! Plugin system for custom transformations
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
//! # Example
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

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::Arc;

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
    /// # Security Note
    ///
    /// This executes arbitrary shell commands. Only use with trusted input.
    pub fn execute_shell(command: &str, input: &str) -> Result<String, String> {
        use std::io::Write;

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

        // Write input to stdin
        if let Some(ref mut stdin) = child.stdin {
            stdin
                .write_all(input.as_bytes())
                .map_err(|e| format!("Failed to write to stdin: {}", e))?;
        }

        let output = child
            .wait_with_output()
            .map_err(|e| format!("Failed to wait for command: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Command failed: {}", stderr.trim()))
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
    fn test_builtin_snake_case() {
        let registry = PluginRegistry::new();
        assert_eq!(
            registry.execute("snake_case", "helloWorld", &[]).unwrap(),
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
    fn test_builtin_length() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.execute("length", "hello", &[]).unwrap(), "5");
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
    fn test_custom_plugin() {
        let mut registry = PluginRegistry::new();
        registry.register("double", |s, _| format!("{}{}", s, s));
        assert_eq!(registry.execute("double", "hi", &[]).unwrap(), "hihi");
    }

    #[test]
    fn test_plugin_not_found() {
        let registry = PluginRegistry::new();
        let result = registry.execute("nonexistent", "test", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_list_plugins() {
        let registry = PluginRegistry::new();
        let plugins = registry.list_plugins();
        assert!(plugins.contains(&"reverse"));
        assert!(plugins.contains(&"snake_case"));
        assert!(plugins.contains(&"length"));
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
}
