//! Analytics tracking for compression savings.
//!
//! Tracks token savings over time, similar to `rtk gain`.

use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Record of a single compression operation
#[derive(Debug, Clone)]
pub struct CompressionRecord {
    pub timestamp: DateTime<Utc>,
    pub command_category: String,
    pub original_tokens: usize,
    pub compressed_tokens: usize,
    pub savings: usize,
    pub reduction_percent: f64,
}

/// Aggregated statistics
#[derive(Debug, Clone, Default)]
pub struct CompressionStats {
    /// Total commands processed
    pub total_commands: usize,
    /// Total tokens saved
    pub total_tokens_saved: usize,
    /// Total original tokens
    pub total_original_tokens: usize,
    /// Average reduction percentage
    pub avg_reduction_percent: f64,
    /// Savings by category
    pub by_category: HashMap<String, CategoryStats>,
}

#[derive(Debug, Clone, Default)]
pub struct CategoryStats {
    pub count: usize,
    pub tokens_saved: usize,
    pub original_tokens: usize,
}

/// Analytics tracker for compression
#[derive(Debug)]
pub struct CompressionAnalytics {
    records: Vec<CompressionRecord>,
    stats: CompressionStats,
}

impl CompressionAnalytics {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            stats: CompressionStats::default(),
        }
    }

    /// Record a compression operation
    pub fn record(&mut self, category: &str, original: usize, compressed: usize) {
        let savings = original.saturating_sub(compressed);
        let reduction = if original > 0 {
            (savings as f64 / original as f64) * 100.0
        } else {
            0.0
        };

        let record = CompressionRecord {
            timestamp: Utc::now(),
            command_category: category.to_string(),
            original_tokens: original,
            compressed_tokens: compressed,
            savings,
            reduction_percent: reduction,
        };

        self.records.push(record);
        self.update_stats(category, original, savings);
    }

    fn update_stats(&mut self, category: &str, original: usize, savings: usize) {
        self.stats.total_commands += 1;
        self.stats.total_tokens_saved += savings;
        self.stats.total_original_tokens += original;

        if self.stats.total_original_tokens > 0 {
            self.stats.avg_reduction_percent =
                (self.stats.total_tokens_saved as f64 / self.stats.total_original_tokens as f64) * 100.0;
        }

        let cat = self.stats.by_category.entry(category.to_string()).or_default();
        cat.count += 1;
        cat.tokens_saved += savings;
        cat.original_tokens += original;
    }

    /// Get current statistics
    pub fn get_stats(&self) -> &CompressionStats {
        &self.stats
    }

    /// Get recent records
    pub fn recent_records(&self, count: usize) -> Vec<&CompressionRecord> {
        self.records.iter().rev().take(count).collect()
    }

    /// Format stats as a report
    pub fn format_report(&self) -> String {
        let stats = &self.stats;
        let mut lines = vec![
            "📊 Compression Analytics".to_string(),
            "═".repeat(40),
            format!("Total commands: {}", stats.total_commands),
            format!("Total tokens saved: {}", format_tokens(stats.total_tokens_saved)),
            format!("Average reduction: {:.1}%", stats.avg_reduction_percent),
            String::new(),
            "By Category:".to_string(),
        ];

        let mut categories: Vec<_> = stats.by_category.iter().collect();
        categories.sort_by(|a, b| b.1.tokens_saved.cmp(&a.1.tokens_saved));

        for (cat, cat_stats) in categories {
            let pct = if cat_stats.original_tokens > 0 {
                (cat_stats.tokens_saved as f64 / cat_stats.original_tokens as f64) * 100.0
            } else {
                0.0
            };
            lines.push(format!(
                "  {}: {} commands, {} saved ({:.0}%)",
                cat,
                cat_stats.count,
                format_tokens(cat_stats.tokens_saved),
                pct
            ));
        }

        lines.join("\n")
    }

    /// Export stats as JSON
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "total_commands": self.stats.total_commands,
            "total_tokens_saved": self.stats.total_tokens_saved,
            "total_original_tokens": self.stats.total_original_tokens,
            "avg_reduction_percent": self.stats.avg_reduction_percent,
            "by_category": self.stats.by_category.iter().map(|(k, v)| {
                (k.clone(), serde_json::json!({
                    "count": v.count,
                    "tokens_saved": v.tokens_saved,
                    "original_tokens": v.original_tokens,
                }))
            }).collect::<HashMap<_, _>>(),
        })
    }

    /// Clear all records (but keep stats)
    pub fn clear_records(&mut self) {
        self.records.clear();
    }

    /// Reset everything
    pub fn reset(&mut self) {
        self.records.clear();
        self.stats = CompressionStats::default();
    }
}

impl Default for CompressionAnalytics {
    fn default() -> Self {
        Self::new()
    }
}

fn format_tokens(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analytics_record() {
        let mut analytics = CompressionAnalytics::new();

        analytics.record("git", 1000, 200);
        analytics.record("git", 500, 100);
        analytics.record("test", 2000, 200);

        let stats = analytics.get_stats();
        assert_eq!(stats.total_commands, 3);
        assert_eq!(stats.total_tokens_saved, 3000); // 800 + 400 + 1800
        assert_eq!(stats.by_category.len(), 2);
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(1500), "1.5K");
        assert_eq!(format_tokens(2_500_000), "2.5M");
    }

    #[test]
    fn test_format_report() {
        let mut analytics = CompressionAnalytics::new();
        analytics.record("git", 1000, 200);

        let report = analytics.format_report();
        assert!(report.contains("Compression Analytics"));
        assert!(report.contains("git"));
    }
}
