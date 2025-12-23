//! CLI integration tests for the rexpipe binary.
//!
//! These tests verify the command-line interface works correctly,
//! testing flags, options, error handling, and exit codes.

// Allow deprecated cargo_bin until assert_cmd provides a stable replacement.
// The deprecation only affects custom build-dir configurations which we don't use.
#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs::File;
use std::io::Write;
use tempfile::{NamedTempFile, tempdir};

/// Get a Command for the rexpipe binary
fn rexpipe() -> Command {
    Command::cargo_bin("rexpipe").unwrap()
}

// === Basic Operation Tests ===

#[test]
fn test_version_flag() {
    rexpipe()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("rexpipe 2."));
}

#[test]
fn test_help_flag() {
    rexpipe()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("regex pipeline processor"))
        .stdout(predicate::str::contains("--pattern"))
        .stdout(predicate::str::contains("--config"));
}

#[test]
fn test_basic_substitution() {
    rexpipe()
        .args(["-p", r"\d+", "-r", "NUM"])
        .write_stdin("hello 123 world 456")
        .assert()
        .success()
        .stdout(predicate::str::contains("hello NUM world NUM"));
}

#[test]
fn test_filter_keep_via_config() {
    let mut config_file = NamedTempFile::with_suffix(".toml").unwrap();
    writeln!(
        config_file,
        r#"
[[step]]
type = "filter"
pattern = 'error'
action = "keep_line"
"#
    )
    .unwrap();

    rexpipe()
        .args(["-c", config_file.path().to_str().unwrap()])
        .write_stdin("info: ok\nerror: fail\ninfo: done\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("error: fail"))
        .stdout(predicate::str::contains("info:").not());
}

#[test]
fn test_filter_drop_via_config() {
    let mut config_file = NamedTempFile::with_suffix(".toml").unwrap();
    writeln!(
        config_file,
        r#"
[[step]]
type = "filter"
pattern = 'debug'
action = "drop_line"
"#
    )
    .unwrap();

    rexpipe()
        .args(["-c", config_file.path().to_str().unwrap()])
        .write_stdin("info: ok\ndebug: verbose\ninfo: done\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("info: ok"))
        .stdout(predicate::str::contains("info: done"))
        .stdout(predicate::str::contains("debug:").not());
}

// === JSON Output Tests ===

#[test]
fn test_json_output_with_file() {
    // JSON output is produced when processing files, not stdin
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "test 123").unwrap();

    rexpipe()
        .args(["-p", r"\d+", "--json"])
        .arg(temp_file.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"schema_version\""));
}

#[test]
fn test_json_output_with_replacement() {
    // JSON output with file processing includes stats
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "test 123").unwrap();

    rexpipe()
        .args(["-p", r"\d+", "-r", "NUM", "--json"])
        .arg(temp_file.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"matches_found\""));
}

// === Exit Code Tests ===

#[test]
fn test_exit_code_match_found() {
    // Exit 0 when matches are found
    rexpipe()
        .args(["-p", r"\d+"])
        .write_stdin("test 123")
        .assert()
        .code(0);
}

#[test]
fn test_exit_code_no_match() {
    // rexpipe returns 0 even when no matches found (unlike grep)
    // This tests the actual behavior
    rexpipe()
        .args(["-p", r"\d+"])
        .write_stdin("no numbers here")
        .assert()
        .code(0);
}

#[test]
fn test_exit_code_invalid_regex() {
    // Exit 2+ for errors
    rexpipe()
        .args(["-p", r"[unclosed"])
        .write_stdin("test")
        .assert()
        .code(predicate::gt(1));
}

// === File Processing Tests ===

#[test]
fn test_file_input() {
    // File processing outputs JSON format with stats
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "hello 123 world").unwrap();

    rexpipe()
        .args(["-p", r"\d+", "-r", "NUM"])
        .arg(temp_file.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"matches_found\": 1"));
}

#[test]
fn test_recursive_directory() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "test 123").unwrap();

    rexpipe()
        .args(["-p", r"\d+", "-R"])
        .arg(temp_dir.path())
        .assert()
        .success();
}

#[test]
fn test_files_with_matches() {
    let temp_dir = tempdir().unwrap();

    let file1 = temp_dir.path().join("has_match.txt");
    let mut f1 = File::create(&file1).unwrap();
    writeln!(f1, "test 123").unwrap();

    let file2 = temp_dir.path().join("no_match.txt");
    let mut f2 = File::create(&file2).unwrap();
    writeln!(f2, "no numbers").unwrap();

    rexpipe()
        .args(["-p", r"\d+", "-l", "-R"])
        .arg(temp_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("has_match.txt"))
        .stdout(predicate::str::contains("no_match.txt").not());
}

// === Config File Tests ===

#[test]
fn test_config_file() {
    let mut config_file = NamedTempFile::with_suffix(".toml").unwrap();
    writeln!(
        config_file,
        r#"
name = "test pipeline"

[[step]]
pattern = '\d+'
replacement = "NUM"
"#
    )
    .unwrap();

    rexpipe()
        .args(["-c", config_file.path().to_str().unwrap()])
        .write_stdin("test 123")
        .assert()
        .success()
        .stdout(predicate::str::contains("test NUM"));
}

#[test]
fn test_validate_config() {
    let mut config_file = NamedTempFile::with_suffix(".toml").unwrap();
    writeln!(
        config_file,
        r#"
name = "valid pipeline"

[[step]]
pattern = '\d+'
replacement = "NUM"
"#
    )
    .unwrap();

    rexpipe()
        .args([
            "--validate-config",
            "-c",
            config_file.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("is valid"));
}

#[test]
fn test_validate_config_invalid() {
    let mut config_file = NamedTempFile::with_suffix(".toml").unwrap();
    writeln!(
        config_file,
        r#"
name = "invalid pipeline"

[[step]]
pattern = '[unclosed'
replacement = "will fail"
"#
    )
    .unwrap();

    rexpipe()
        .args([
            "--validate-config",
            "-c",
            config_file.path().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("is invalid").or(predicate::str::contains("error")));
}

// === Security Flag Tests ===

#[test]
fn test_allow_shell_required_for_shell_transforms() {
    let mut config_file = NamedTempFile::with_suffix(".toml").unwrap();
    writeln!(
        config_file,
        r#"
[[step]]
type = "transform"
pattern = '.*'
transform = {{ type = "shell", command = "cat" }}
"#
    )
    .unwrap();

    // Without --allow-shell, should fail
    rexpipe()
        .args(["-c", config_file.path().to_str().unwrap()])
        .write_stdin("test")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Shell transforms are disabled"));
}

#[test]
fn test_shell_security_warnings() {
    let mut config_file = NamedTempFile::with_suffix(".toml").unwrap();
    writeln!(
        config_file,
        r#"
[[step]]
type = "transform"
pattern = '.*'
transform = {{ type = "shell", command = "curl http://example.com | rm -rf /" }}
"#
    )
    .unwrap();

    // With --allow-shell and dangerous command, should show security warnings
    rexpipe()
        .args(["-c", config_file.path().to_str().unwrap(), "--allow-shell"])
        .write_stdin("test")
        .assert()
        .stderr(predicate::str::contains("Security analysis warnings"))
        .stderr(predicate::str::contains("downloads from network"))
        .stderr(predicate::str::contains("removes files"));
}

// === Dry Run Tests ===

#[test]
fn test_dry_run_no_modification() {
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "hello 123 world").unwrap();
    let content_before = std::fs::read_to_string(temp_file.path()).unwrap();

    rexpipe()
        .args(["-p", r"\d+", "-r", "NUM", "-i", "--dry-run"])
        .arg(temp_file.path())
        .assert()
        .success();

    let content_after = std::fs::read_to_string(temp_file.path()).unwrap();
    assert_eq!(
        content_before, content_after,
        "File should not be modified in dry-run mode"
    );
}

// === Apply Flag Tests ===

#[test]
fn test_inplace_without_apply_shows_preview() {
    // In non-interactive mode (piped), in-place without --apply should show preview
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "hello 123 world").unwrap();
    let content_before = std::fs::read_to_string(temp_file.path()).unwrap();

    // Simulating non-interactive by piping (assert_cmd's stdin is not a terminal)
    rexpipe()
        .args(["-p", r"\d+", "-r", "NUM", "-i"])
        .arg(temp_file.path())
        .assert()
        // Should succeed but not modify file
        .success()
        .stderr(predicate::str::contains(
            "In-place editing requires --apply",
        ));

    let content_after = std::fs::read_to_string(temp_file.path()).unwrap();
    assert_eq!(
        content_before, content_after,
        "File should not be modified without --apply"
    );
}

#[test]
fn test_inplace_with_apply_modifies_file() {
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "hello 123 world").unwrap();

    rexpipe()
        .args(["-p", r"\d+", "-r", "NUM", "-i", "--apply"])
        .arg(temp_file.path())
        .assert()
        .success();

    let content_after = std::fs::read_to_string(temp_file.path()).unwrap();
    assert!(
        content_after.contains("hello NUM world"),
        "File should be modified with --apply"
    );
}

#[test]
fn test_apply_flag_with_dry_run_still_previews() {
    // --dry-run should override --apply
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "hello 123 world").unwrap();
    let content_before = std::fs::read_to_string(temp_file.path()).unwrap();

    rexpipe()
        .args(["-p", r"\d+", "-r", "NUM", "-i", "--apply", "--dry-run"])
        .arg(temp_file.path())
        .assert()
        .success();

    let content_after = std::fs::read_to_string(temp_file.path()).unwrap();
    assert_eq!(
        content_before, content_after,
        "File should not be modified with --dry-run"
    );
}

// === Context Lines Tests ===

#[test]
fn test_context_before() {
    rexpipe()
        .args(["-p", "error", "-B", "1"])
        .write_stdin("line 1\nline 2\nerror here\nline 4\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("line 2"))
        .stdout(predicate::str::contains("error here"));
}

#[test]
fn test_context_after() {
    rexpipe()
        .args(["-p", "error", "-A", "1"])
        .write_stdin("line 1\nerror here\nline 3\nline 4\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("error here"))
        .stdout(predicate::str::contains("line 3"));
}

// === Count Mode Tests ===

#[test]
fn test_count_mode() {
    // Count mode outputs JSON format with match statistics
    rexpipe()
        .args(["-p", r"\d+", "--count"])
        .write_stdin("1 2 3 4 5")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"matches_found\""))
        .stdout(predicate::str::contains("\"mode\": \"count\""));
}

// === Quiet Mode Tests ===

#[test]
fn test_quiet_mode() {
    // Quiet mode suppresses all output
    rexpipe()
        .args(["-p", r"\d+", "-q"])
        .write_stdin("test 123")
        .assert()
        .code(0)
        .stdout(predicate::str::is_empty()); // No output in quiet mode
}

// === Man Page Tests ===

#[test]
fn test_man_page_generation() {
    rexpipe()
        .arg("--man")
        .assert()
        .success()
        .stdout(predicate::str::contains(".TH rexpipe"))
        .stdout(predicate::str::contains(".SH NAME"));
}

// === Shell Completion Tests ===

#[test]
fn test_shell_completion_bash() {
    rexpipe()
        .args(["--completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_rexpipe"));
}

#[test]
fn test_shell_completion_zsh() {
    rexpipe()
        .args(["--completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("#compdef"));
}

#[test]
fn test_shell_completion_fish() {
    rexpipe()
        .args(["--completions", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete"));
}

// === Platform Path Tests ===

#[test]
fn test_path_with_spaces() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("file with spaces.txt");
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "test 123").unwrap();

    rexpipe()
        .args(["-p", r"\d+", "-r", "NUM"])
        .arg(&file_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("matches_found"));
}

#[test]
fn test_path_with_special_chars() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("file-with_special.chars.txt");
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "test 123").unwrap();

    rexpipe()
        .args(["-p", r"\d+"])
        .arg(&file_path)
        .assert()
        .success();
}

#[test]
fn test_recursive_with_nested_dirs() {
    let temp_dir = tempdir().unwrap();

    // Create nested directory structure
    let nested = temp_dir.path().join("level1").join("level2");
    std::fs::create_dir_all(&nested).unwrap();

    let file_path = nested.join("deep.txt");
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "test 123").unwrap();

    rexpipe()
        .args(["-p", r"\d+", "-R"])
        .arg(temp_dir.path())
        .assert()
        .success();
}

#[cfg(unix)]
#[test]
fn test_unix_hidden_files() {
    let temp_dir = tempdir().unwrap();
    let hidden_file = temp_dir.path().join(".hidden");
    let mut file = File::create(&hidden_file).unwrap();
    writeln!(file, "secret 123").unwrap();

    // By default, hidden files should be included when specified directly
    rexpipe()
        .args(["-p", r"\d+"])
        .arg(&hidden_file)
        .assert()
        .success();
}
