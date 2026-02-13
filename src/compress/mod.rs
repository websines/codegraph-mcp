//! RTK-style token compression for LLM output optimization.
//!
//! This module provides compression algorithms that reduce token consumption
//! by 60-90% on common command outputs like file listings, git operations,
//! grep results, and test output.

mod files;
mod git;
mod search;
mod test_output;
mod bash;
mod analytics;

pub use files::{compress_file_list, compress_tree, compress_ls};
pub use git::{compress_git_status, compress_git_diff, compress_git_log, compress_git_blame};
pub use search::{compress_grep, compress_find, compress_symbol_search, SymbolResult};
pub use test_output::compress_test_output;
pub use bash::{exec_compressed, compress_for_command, categorize_command};
pub use analytics::{CompressionAnalytics, CompressionStats, CompressionRecord};

use std::collections::HashMap;

/// Configuration for compression behavior
#[derive(Debug, Clone)]
pub struct CompressConfig {
    /// Maximum lines to show before truncating
    pub max_lines: usize,
    /// Maximum items per group before summarizing
    pub max_items_per_group: usize,
    /// Whether to show occurrence counts for duplicates
    pub show_counts: bool,
    /// Whether to group items by category/directory
    pub group_items: bool,
    /// Minimum occurrences before deduplicating
    pub dedup_threshold: usize,
}

impl Default for CompressConfig {
    fn default() -> Self {
        Self {
            max_lines: 50,
            max_items_per_group: 10,
            show_counts: true,
            group_items: true,
            dedup_threshold: 2,
        }
    }
}

/// Result of a compression operation
#[derive(Debug, Clone)]
pub struct CompressResult {
    /// The compressed output
    pub output: String,
    /// Original size in characters
    pub original_size: usize,
    /// Compressed size in characters
    pub compressed_size: usize,
    /// Estimated token savings (chars / 4 approximation)
    pub estimated_token_savings: usize,
}

impl CompressResult {
    pub fn new(original: &str, compressed: String) -> Self {
        let original_size = original.len();
        let compressed_size = compressed.len();
        let savings = original_size.saturating_sub(compressed_size);

        Self {
            output: compressed,
            original_size,
            compressed_size,
            estimated_token_savings: savings / 4, // rough token estimate
        }
    }

    pub fn reduction_percent(&self) -> f64 {
        if self.original_size == 0 || self.compressed_size >= self.original_size {
            return 0.0;
        }
        ((self.original_size - self.compressed_size) as f64 / self.original_size as f64) * 100.0
    }
}

/// Deduplicate lines and add occurrence counts
pub fn deduplicate_lines(lines: &[&str], threshold: usize) -> Vec<String> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut order: Vec<&str> = Vec::new();

    for line in lines {
        *counts.entry(*line).or_insert(0) += 1;
        if counts[line] == 1 {
            order.push(line);
        }
    }

    order
        .into_iter()
        .map(|line| {
            let count = counts[line];
            if count >= threshold {
                format!("{} (×{})", line, count)
            } else {
                line.to_string()
            }
        })
        .collect()
}

/// Truncate output with a summary of what was hidden
pub fn truncate_with_summary(lines: &[String], max_lines: usize) -> String {
    if lines.len() <= max_lines {
        return lines.join("\n");
    }

    let shown: Vec<_> = lines.iter().take(max_lines).cloned().collect();
    let hidden = lines.len() - max_lines;

    format!(
        "{}\n... ({} more lines hidden)",
        shown.join("\n"),
        hidden
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deduplicate_lines() {
        let lines = vec!["error: foo", "error: foo", "error: foo", "warning: bar"];
        let result = deduplicate_lines(&lines, 2);

        assert_eq!(result.len(), 2);
        assert!(result[0].contains("×3"));
        assert_eq!(result[1], "warning: bar");
    }

    #[test]
    fn test_truncate_with_summary() {
        let lines: Vec<String> = (0..100).map(|i| format!("line {}", i)).collect();
        let result = truncate_with_summary(&lines, 10);

        assert!(result.contains("line 0"));
        assert!(result.contains("line 9"));
        assert!(result.contains("90 more lines hidden"));
        assert!(!result.contains("line 10"));
    }

    #[test]
    fn test_compress_result_reduction() {
        let original = "a".repeat(1000);
        let compressed = "a".repeat(200);
        let result = CompressResult::new(&original, compressed);

        assert!((result.reduction_percent() - 80.0).abs() < 0.1);
        assert_eq!(result.estimated_token_savings, 200); // 800 / 4
    }
}
