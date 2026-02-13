//! Bash command execution with automatic compression.
//!
//! Wraps command execution and applies appropriate compression based on command type.

use std::process::Command;

use super::{CompressConfig, CompressResult};
use super::{files, git, search, test_output};

/// Execute a bash command and compress its output
pub fn exec_compressed(command: &str, config: &CompressConfig) -> Result<CompressResult, String> {
    // Execute command
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .map_err(|e| format!("Failed to execute command: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Combine stdout and stderr
    let combined = if stderr.is_empty() {
        stdout
    } else if stdout.is_empty() {
        stderr
    } else {
        format!("{}\n--- stderr ---\n{}", stdout, stderr)
    };

    // Detect command type and apply appropriate compression
    let result = compress_for_command(command, &combined, config);

    Ok(result)
}

/// Compress output based on command type
pub fn compress_for_command(command: &str, output: &str, config: &CompressConfig) -> CompressResult {
    let cmd_lower = command.to_lowercase();

    // Git commands
    if cmd_lower.starts_with("git ") {
        if cmd_lower.contains("status") {
            return git::compress_git_status(output, config);
        } else if cmd_lower.contains("diff") {
            return git::compress_git_diff(output, config);
        } else if cmd_lower.contains("log") {
            return git::compress_git_log(output, config);
        } else if cmd_lower.contains("blame") {
            return git::compress_git_blame(output, config);
        }
    }

    // File listing commands
    if cmd_lower.starts_with("ls") || cmd_lower.starts_with("dir") {
        return files::compress_ls(output, config);
    }
    if cmd_lower.starts_with("tree") {
        return files::compress_tree(output, config);
    }
    if cmd_lower.starts_with("find ") {
        return files::compress_file_list(output, config);
    }

    // Search commands
    if cmd_lower.starts_with("grep ")
        || cmd_lower.starts_with("rg ")
        || cmd_lower.starts_with("ag ")
        || cmd_lower.starts_with("ack ")
    {
        return search::compress_grep(output, config);
    }

    // Test commands
    if cmd_lower.contains("test")
        || cmd_lower.contains("cargo t")
        || cmd_lower.contains("pytest")
        || cmd_lower.contains("jest")
        || cmd_lower.contains("npm run test")
        || cmd_lower.contains("yarn test")
        || cmd_lower.contains("go test")
    {
        return test_output::compress_test_output(output, config);
    }

    // Docker commands
    if cmd_lower.starts_with("docker ") {
        return compress_docker(output, config);
    }

    // Package manager commands
    if cmd_lower.starts_with("npm ")
        || cmd_lower.starts_with("yarn ")
        || cmd_lower.starts_with("pnpm ")
        || cmd_lower.starts_with("cargo ")
        || cmd_lower.starts_with("pip ")
    {
        return compress_package_manager(output, config);
    }

    // Default: generic compression
    compress_generic(output, config)
}

/// Compress Docker output
fn compress_docker(output: &str, config: &CompressConfig) -> CompressResult {
    let lines: Vec<&str> = output.lines().collect();

    if lines.is_empty() {
        return CompressResult::new(output, String::new());
    }

    // Filter out progress bars and layer downloads
    let filtered: Vec<&str> = lines
        .iter()
        .filter(|line| {
            let l = line.trim();
            !l.is_empty()
                && !l.contains("Downloading")
                && !l.contains("Extracting")
                && !l.contains("Waiting")
                && !l.contains("Pulling")
                && !l.starts_with("=>")
                && !l.contains("[")  // Progress indicators like [=====>     ]
        })
        .copied()
        .collect();

    let result: Vec<String> = filtered.iter().map(|s| s.to_string()).collect();
    let compressed = super::truncate_with_summary(&result, config.max_lines);
    CompressResult::new(output, compressed)
}

/// Compress package manager output
fn compress_package_manager(output: &str, config: &CompressConfig) -> CompressResult {
    let lines: Vec<&str> = output.lines().collect();

    if lines.is_empty() {
        return CompressResult::new(output, String::new());
    }

    // Filter verbose install/download output
    let filtered: Vec<&str> = lines
        .iter()
        .filter(|line| {
            let l = line.trim();
            !l.is_empty()
                && !l.starts_with("Downloading")
                && !l.starts_with("Downloaded")
                && !l.starts_with("Compiling")
                && !l.starts_with("Installing")
                && !l.starts_with("  ")  // Indented dependency details
                && !l.contains("packages in")
                && !l.contains("up to date")
                && !l.starts_with("added")
        })
        .copied()
        .collect();

    // Keep important lines
    let important: Vec<String> = lines
        .iter()
        .filter(|line| {
            let l = line.trim();
            l.contains("error")
                || l.contains("Error")
                || l.contains("warning")
                || l.contains("Warning")
                || l.contains("WARN")
                || l.starts_with("Finished")
                || l.starts_with("Built")
                || l.contains("Successfully")
        })
        .map(|s| s.to_string())
        .collect();

    let result = if !important.is_empty() {
        important
    } else {
        filtered.iter().map(|s| s.to_string()).collect()
    };

    let compressed = super::truncate_with_summary(&result, config.max_lines);
    CompressResult::new(output, compressed)
}

/// Generic output compression
fn compress_generic(output: &str, config: &CompressConfig) -> CompressResult {
    let lines: Vec<&str> = output.lines().collect();

    if lines.is_empty() {
        return CompressResult::new(output, String::new());
    }

    // Remove empty lines and deduplicate
    let non_empty: Vec<&str> = lines.iter().filter(|l| !l.trim().is_empty()).copied().collect();

    // Deduplicate repeated lines
    let deduped = super::deduplicate_lines(&non_empty, config.dedup_threshold);

    let compressed = super::truncate_with_summary(&deduped, config.max_lines);
    CompressResult::new(output, compressed)
}

/// Get command category for analytics
pub fn categorize_command(command: &str) -> &'static str {
    let cmd_lower = command.to_lowercase();

    if cmd_lower.starts_with("git ") {
        "git"
    } else if cmd_lower.starts_with("ls") || cmd_lower.starts_with("find ") || cmd_lower.starts_with("tree") {
        "files"
    } else if cmd_lower.starts_with("grep ")
        || cmd_lower.starts_with("rg ")
        || cmd_lower.starts_with("ag ")
    {
        "search"
    } else if cmd_lower.contains("test") || cmd_lower.contains("pytest") || cmd_lower.contains("jest") {
        "test"
    } else if cmd_lower.starts_with("docker ") {
        "docker"
    } else if cmd_lower.starts_with("npm ")
        || cmd_lower.starts_with("yarn ")
        || cmd_lower.starts_with("cargo ")
    {
        "package"
    } else {
        "other"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_categorize_command() {
        assert_eq!(categorize_command("git status"), "git");
        assert_eq!(categorize_command("ls -la"), "files");
        assert_eq!(categorize_command("grep -r foo"), "search");
        assert_eq!(categorize_command("cargo test"), "test");
        assert_eq!(categorize_command("docker ps"), "docker");
        assert_eq!(categorize_command("npm install"), "package");
        assert_eq!(categorize_command("echo hello"), "other");
    }

    #[test]
    fn test_compress_for_command() {
        let config = CompressConfig::default();

        // Git status
        let git_output = "M  file.rs\n?? new.rs";
        let result = compress_for_command("git status -s", git_output, &config);
        assert!(result.output.contains("Modified") || result.output.contains("Untracked"));

        // Generic (use "echo hello" to avoid matching "test" commands)
        let generic = "line1\nline2\nline2\nline2";
        let result = compress_for_command("echo hello", generic, &config);
        // Deduplication turns "line2 x3" into "line2 (×3)" which contains "line2"
        assert!(
            result.output.contains("line2") || result.output.contains("line1"),
            "Output should contain original content: {}",
            result.output
        );
    }

    #[test]
    fn test_compress_docker() {
        let input = r#"Pulling from library/node
Downloading [=====>     ] 50%
Extracting...
Downloading [=========> ] 90%
Successfully built abc123
"#;
        let config = CompressConfig::default();
        let result = compress_docker(input, &config);

        assert!(result.output.contains("Successfully"));
        assert!(!result.output.contains("Downloading"));
    }
}
