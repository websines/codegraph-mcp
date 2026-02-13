//! Search result compression - grep, ripgrep, find.
//!
//! Achieves ~80% reduction on search results.

use std::collections::HashMap;

use super::{CompressConfig, CompressResult, truncate_with_summary};

/// Compress grep/ripgrep output by grouping matches by file
pub fn compress_grep(output: &str, config: &CompressConfig) -> CompressResult {
    let lines: Vec<&str> = output.lines().collect();

    if lines.is_empty() {
        return CompressResult::new(output, String::new());
    }

    // Group by file
    let mut by_file: HashMap<String, Vec<GrepMatch>> = HashMap::new();
    let mut file_order: Vec<String> = Vec::new();

    for line in &lines {
        if let Some((file, line_num, content)) = parse_grep_line(line) {
            if !by_file.contains_key(&file) {
                file_order.push(file.clone());
            }
            by_file.entry(file).or_default().push(GrepMatch {
                line_num,
                content: content.to_string(),
            });
        }
    }

    if by_file.is_empty() {
        return CompressResult::new(output, "No matches".to_string());
    }

    let total_matches: usize = by_file.values().map(|v| v.len()).sum();
    let mut result_lines: Vec<String> = Vec::new();
    result_lines.push(format!("🔍 {} matches in {} files", total_matches, by_file.len()));
    result_lines.push(String::new());

    for file in file_order.iter().take(config.max_items_per_group) {
        let matches = &by_file[file];
        result_lines.push(format!("📄 {} ({} matches)", file, matches.len()));

        for m in matches.iter().take(3) {
            let content = if m.content.len() > 60 {
                format!("{}...", &m.content[..57])
            } else {
                m.content.clone()
            };
            result_lines.push(format!("  L{}: {}", m.line_num, content.trim()));
        }

        if matches.len() > 3 {
            result_lines.push(format!("  ... +{} more matches", matches.len() - 3));
        }
    }

    if file_order.len() > config.max_items_per_group {
        result_lines.push(format!(
            "\n... +{} more files",
            file_order.len() - config.max_items_per_group
        ));
    }

    let compressed = result_lines.join("\n");
    CompressResult::new(output, compressed)
}

struct GrepMatch {
    line_num: usize,
    content: String,
}

fn parse_grep_line(line: &str) -> Option<(String, usize, &str)> {
    // Common formats:
    // file:line:content (grep -n, ripgrep)
    // file:content (grep without -n)

    let parts: Vec<&str> = line.splitn(3, ':').collect();

    match parts.len() {
        3 => {
            let file = parts[0].to_string();
            let line_num = parts[1].parse().unwrap_or(0);
            Some((file, line_num, parts[2]))
        }
        2 => {
            let file = parts[0].to_string();
            Some((file, 0, parts[1]))
        }
        _ => None,
    }
}

/// Compress find command output
pub fn compress_find(output: &str, config: &CompressConfig) -> CompressResult {
    // Find output is just file paths - delegate to file list compression
    super::files::compress_file_list(output, config)
}

/// Compress symbol search results (from codegraph)
pub fn compress_symbol_search(results: &[SymbolResult], config: &CompressConfig) -> CompressResult {
    if results.is_empty() {
        return CompressResult::new("", "No symbols found".to_string());
    }

    // Group by file
    let mut by_file: HashMap<&str, Vec<&SymbolResult>> = HashMap::new();
    let mut file_order: Vec<&str> = Vec::new();

    for r in results {
        if !by_file.contains_key(r.file.as_str()) {
            file_order.push(&r.file);
        }
        by_file.entry(&r.file).or_default().push(r);
    }

    let mut result_lines: Vec<String> = Vec::new();
    result_lines.push(format!("🔎 {} symbols in {} files", results.len(), by_file.len()));
    result_lines.push(String::new());

    for file in file_order.iter().take(config.max_items_per_group) {
        let symbols = &by_file[*file];

        // Shorten file path
        let short_file = shorten_path(file);
        result_lines.push(format!("📄 {}", short_file));

        for s in symbols.iter().take(5) {
            let kind_emoji = match s.kind.as_str() {
                "function" | "method" => "ƒ",
                "class" | "struct" => "◇",
                "interface" | "trait" => "◈",
                "variable" | "field" => "•",
                "module" => "📦",
                _ => "·",
            };
            result_lines.push(format!("  {} {} (L{})", kind_emoji, s.name, s.line));
        }

        if symbols.len() > 5 {
            result_lines.push(format!("  ... +{} more", symbols.len() - 5));
        }
    }

    if file_order.len() > config.max_items_per_group {
        result_lines.push(format!(
            "\n... +{} more files",
            file_order.len() - config.max_items_per_group
        ));
    }

    // Calculate original size estimate
    let original_size: usize = results.iter().map(|r| r.file.len() + r.name.len() + 20).sum();
    let compressed = result_lines.join("\n");

    CompressResult {
        output: compressed.clone(),
        original_size,
        compressed_size: compressed.len(),
        estimated_token_savings: original_size.saturating_sub(compressed.len()) / 4,
    }
}

/// Symbol result for compression
#[derive(Debug, Clone)]
pub struct SymbolResult {
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line: usize,
}

fn shorten_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 3 {
        return path.to_string();
    }

    // Keep first and last two parts
    format!("{}/.../{}",
        parts[0],
        parts[parts.len() - 2..].join("/")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_grep() {
        let input = r#"src/main.rs:10:fn main() {
src/main.rs:15:    let config = Config::new();
src/main.rs:20:    println!("Done");
src/lib.rs:5:pub fn helper() {
src/lib.rs:10:    // helper code
tests/test.rs:1:use super::*;"#;

        let config = CompressConfig::default();
        let result = compress_grep(input, &config);

        assert!(result.output.contains("6 matches in 3 files"));
        assert!(result.output.contains("src/main.rs"));
        assert!(result.output.contains("src/lib.rs"));
    }

    #[test]
    fn test_compress_symbol_search() {
        let results = vec![
            SymbolResult {
                name: "main".to_string(),
                kind: "function".to_string(),
                file: "src/main.rs".to_string(),
                line: 10,
            },
            SymbolResult {
                name: "Config".to_string(),
                kind: "struct".to_string(),
                file: "src/config.rs".to_string(),
                line: 5,
            },
        ];

        let config = CompressConfig::default();
        let result = compress_symbol_search(&results, &config);

        assert!(result.output.contains("2 symbols in 2 files"));
        assert!(result.output.contains("main"));
        assert!(result.output.contains("Config"));
    }

    #[test]
    fn test_shorten_path() {
        assert_eq!(shorten_path("src/main.rs"), "src/main.rs");
        assert_eq!(
            shorten_path("very/long/nested/path/to/file.rs"),
            "very/.../to/file.rs"
        );
    }
}
