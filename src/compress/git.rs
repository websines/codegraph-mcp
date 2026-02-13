//! Git output compression - status, diff, log.
//!
//! Achieves ~75-80% reduction on git operations.

use std::collections::HashMap;

use super::{CompressConfig, CompressResult, deduplicate_lines, truncate_with_summary};

/// Compress git status output by grouping by status type
pub fn compress_git_status(output: &str, config: &CompressConfig) -> CompressResult {
    let lines: Vec<&str> = output.lines().collect();

    if lines.is_empty() {
        return CompressResult::new(output, String::new());
    }

    let mut staged: Vec<&str> = Vec::new();
    let mut modified: Vec<&str> = Vec::new();
    let mut untracked: Vec<&str> = Vec::new();
    let mut deleted: Vec<&str> = Vec::new();
    let mut other: Vec<&str> = Vec::new();

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Parse git status short format (e.g., "M  file.rs", "?? newfile")
        if trimmed.len() < 3 {
            other.push(trimmed);
            continue;
        }

        let status = &trimmed[..2];
        let file = trimmed[3..].trim();

        match status {
            "M " | " M" | "MM" => modified.push(file),
            "A " | " A" => staged.push(file),
            "D " | " D" => deleted.push(file),
            "??" => untracked.push(file),
            "R " | " R" => staged.push(file), // renamed
            _ => other.push(trimmed),
        }
    }

    let mut result_lines: Vec<String> = Vec::new();

    // Helper to add section
    let add_section = |lines: &mut Vec<String>, items: &[&str], label: &str, emoji: &str, max: usize| {
        if items.is_empty() {
            return;
        }
        lines.push(format!("{} {} ({})", emoji, label, items.len()));
        for item in items.iter().take(max) {
            lines.push(format!("  {}", item));
        }
        if items.len() > max {
            lines.push(format!("  ... +{} more", items.len() - max));
        }
    };

    add_section(&mut result_lines, &staged, "Staged", "✅", config.max_items_per_group);
    add_section(&mut result_lines, &modified, "Modified", "📝", config.max_items_per_group);
    add_section(&mut result_lines, &deleted, "Deleted", "🗑️", config.max_items_per_group);
    add_section(&mut result_lines, &untracked, "Untracked", "❓", config.max_items_per_group);

    if !other.is_empty() {
        result_lines.push(format!("Other: {}", other.len()));
    }

    if result_lines.is_empty() {
        return CompressResult::new(output, "Clean working tree".to_string());
    }

    let compressed = result_lines.join("\n");
    CompressResult::new(output, compressed)
}

/// Compress git diff output by summarizing changes per file
pub fn compress_git_diff(output: &str, config: &CompressConfig) -> CompressResult {
    let lines: Vec<&str> = output.lines().collect();

    if lines.is_empty() {
        return CompressResult::new(output, String::new());
    }

    let mut files: Vec<FileDiff> = Vec::new();
    let mut current_file: Option<FileDiff> = None;

    for line in &lines {
        if line.starts_with("diff --git") {
            // Save previous file
            if let Some(f) = current_file.take() {
                files.push(f);
            }
            // Extract filename
            let parts: Vec<&str> = line.split(' ').collect();
            let filename = parts.last().map(|s| s.trim_start_matches("b/")).unwrap_or("unknown");
            current_file = Some(FileDiff {
                name: filename.to_string(),
                additions: 0,
                deletions: 0,
                hunks: 0,
            });
        } else if let Some(ref mut f) = current_file {
            if line.starts_with("@@") {
                f.hunks += 1;
            } else if line.starts_with('+') && !line.starts_with("+++") {
                f.additions += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                f.deletions += 1;
            }
        }
    }

    // Don't forget the last file
    if let Some(f) = current_file {
        files.push(f);
    }

    if files.is_empty() {
        return CompressResult::new(output, "No changes".to_string());
    }

    let total_add: usize = files.iter().map(|f| f.additions).sum();
    let total_del: usize = files.iter().map(|f| f.deletions).sum();

    let mut result_lines: Vec<String> = Vec::new();
    result_lines.push(format!("📊 {} files changed, +{} -{}", files.len(), total_add, total_del));
    result_lines.push(String::new());

    for f in files.iter().take(config.max_items_per_group) {
        result_lines.push(format!("  {} (+{} -{}, {} hunks)", f.name, f.additions, f.deletions, f.hunks));
    }

    if files.len() > config.max_items_per_group {
        result_lines.push(format!("  ... +{} more files", files.len() - config.max_items_per_group));
    }

    let compressed = result_lines.join("\n");
    CompressResult::new(output, compressed)
}

struct FileDiff {
    name: String,
    additions: usize,
    deletions: usize,
    hunks: usize,
}

/// Compress git log output
pub fn compress_git_log(output: &str, config: &CompressConfig) -> CompressResult {
    let lines: Vec<&str> = output.lines().collect();

    if lines.is_empty() {
        return CompressResult::new(output, String::new());
    }

    let mut commits: Vec<CommitInfo> = Vec::new();
    let mut current: Option<CommitInfo> = None;

    for line in &lines {
        if line.starts_with("commit ") {
            if let Some(c) = current.take() {
                commits.push(c);
            }
            let hash = line.strip_prefix("commit ").unwrap_or("").trim();
            current = Some(CommitInfo {
                hash: hash[..7.min(hash.len())].to_string(),
                author: String::new(),
                date: String::new(),
                message: String::new(),
            });
        } else if let Some(ref mut c) = current {
            let trimmed = line.trim();
            if line.starts_with("Author:") {
                c.author = trimmed.strip_prefix("Author:").unwrap_or("").trim().to_string();
                // Extract just the name, not email
                if let Some(idx) = c.author.find('<') {
                    c.author = c.author[..idx].trim().to_string();
                }
            } else if line.starts_with("Date:") {
                c.date = trimmed.strip_prefix("Date:").unwrap_or("").trim().to_string();
                // Shorten date
                if c.date.len() > 16 {
                    c.date = c.date[..16].to_string();
                }
            } else if !trimmed.is_empty() && c.message.is_empty() {
                c.message = trimmed.to_string();
                // Truncate long messages
                if c.message.len() > 60 {
                    c.message = format!("{}...", &c.message[..57]);
                }
            }
        }
    }

    if let Some(c) = current {
        commits.push(c);
    }

    if commits.is_empty() {
        return CompressResult::new(output, "No commits".to_string());
    }

    let mut result_lines: Vec<String> = Vec::new();
    result_lines.push(format!("📜 {} commits", commits.len()));

    for c in commits.iter().take(config.max_items_per_group) {
        result_lines.push(format!("  {} {} - {}", c.hash, c.author, c.message));
    }

    if commits.len() > config.max_items_per_group {
        result_lines.push(format!("  ... +{} more commits", commits.len() - config.max_items_per_group));
    }

    let compressed = result_lines.join("\n");
    CompressResult::new(output, compressed)
}

struct CommitInfo {
    hash: String,
    author: String,
    date: String,
    message: String,
}

/// Compress git blame output by grouping by author
pub fn compress_git_blame(output: &str, config: &CompressConfig) -> CompressResult {
    let lines: Vec<&str> = output.lines().collect();

    if lines.is_empty() {
        return CompressResult::new(output, String::new());
    }

    let mut by_author: HashMap<String, usize> = HashMap::new();

    for line in &lines {
        // Parse blame format: hash (author date line) content
        if let Some(start) = line.find('(') {
            if let Some(end) = line.find(')') {
                let meta = &line[start + 1..end];
                let parts: Vec<&str> = meta.split_whitespace().collect();
                if !parts.is_empty() {
                    let author = parts[0].to_string();
                    *by_author.entry(author).or_insert(0) += 1;
                }
            }
        }
    }

    if by_author.is_empty() {
        return CompressResult::new(output, output.to_string());
    }

    let mut authors: Vec<_> = by_author.into_iter().collect();
    authors.sort_by(|a, b| b.1.cmp(&a.1));

    let mut result_lines: Vec<String> = Vec::new();
    result_lines.push(format!("👥 {} authors, {} lines", authors.len(), lines.len()));

    for (author, count) in authors.iter().take(config.max_items_per_group) {
        let pct = (*count as f64 / lines.len() as f64) * 100.0;
        result_lines.push(format!("  {} - {} lines ({:.0}%)", author, count, pct));
    }

    let compressed = result_lines.join("\n");
    CompressResult::new(output, compressed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_git_status() {
        let input = r#"M  src/main.rs
M  src/lib.rs
?? newfile.rs
?? another.rs
D  deleted.rs"#;

        let config = CompressConfig::default();
        let result = compress_git_status(input, &config);

        assert!(result.output.contains("Modified"));
        assert!(result.output.contains("Untracked"));
        assert!(result.output.contains("Deleted"));
        // Note: reduction may be 0% for small inputs where grouping adds overhead
    }

    #[test]
    fn test_compress_git_diff() {
        let input = r#"diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,5 +1,6 @@
 fn main() {
+    println!("Hello");
     println!("World");
 }
diff --git a/src/lib.rs b/src/lib.rs
@@ -1,2 +1,3 @@
-old line
+new line
+another line"#;

        let config = CompressConfig::default();
        let result = compress_git_diff(input, &config);

        assert!(result.output.contains("2 files changed"));
        assert!(result.output.contains("src/main.rs"));
        assert!(result.output.contains("src/lib.rs"));
    }

    #[test]
    fn test_compress_git_log() {
        let input = r#"commit abc1234567890
Author: John Doe <john@example.com>
Date:   Mon Jan 1 12:00:00 2024

    First commit message here

commit def4567890123
Author: Jane Smith <jane@example.com>
Date:   Tue Jan 2 13:00:00 2024

    Second commit"#;

        let config = CompressConfig::default();
        let result = compress_git_log(input, &config);

        assert!(result.output.contains("2 commits"));
        assert!(result.output.contains("abc1234"));
        assert!(result.output.contains("John Doe"));
    }
}
