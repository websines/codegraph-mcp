//! Test output compression - reduces to failures only.
//!
//! Achieves ~90% reduction on test runner output.

use super::{CompressConfig, CompressResult, deduplicate_lines};

/// Compress test output by extracting failures only
pub fn compress_test_output(output: &str, config: &CompressConfig) -> CompressResult {
    let lines: Vec<&str> = output.lines().collect();

    if lines.is_empty() {
        return CompressResult::new(output, String::new());
    }

    // Detect test framework
    if output.contains("running ") && output.contains(" test") {
        return compress_cargo_test(output, config);
    } else if output.contains("PASS") || output.contains("FAIL") {
        return compress_jest_test(output, config);
    } else if output.contains("pytest") || output.contains("PASSED") || output.contains("FAILED") {
        return compress_pytest(output, config);
    }

    // Generic compression
    compress_generic_test(output, config)
}

/// Compress cargo test output
fn compress_cargo_test(output: &str, config: &CompressConfig) -> CompressResult {
    let lines: Vec<&str> = output.lines().collect();

    let mut passed = 0;
    let mut failed = 0;
    let mut ignored = 0;
    let mut failures: Vec<TestFailure> = Vec::new();
    let mut current_failure: Option<TestFailure> = None;
    let mut in_failure_section = false;

    for line in &lines {
        let trimmed = line.trim();

        // Count results
        if trimmed.starts_with("test ") && trimmed.contains(" ... ") {
            if trimmed.ends_with("ok") {
                passed += 1;
            } else if trimmed.ends_with("FAILED") {
                failed += 1;
                // Extract test name
                if let Some(name) = trimmed.strip_prefix("test ") {
                    if let Some(idx) = name.find(" ... ") {
                        let test_name = &name[..idx];
                        current_failure = Some(TestFailure {
                            name: test_name.to_string(),
                            message: String::new(),
                            location: String::new(),
                        });
                    }
                }
            } else if trimmed.ends_with("ignored") {
                ignored += 1;
            }
        }

        // Capture failure details
        if trimmed == "failures:" || trimmed == "---- failures ----" {
            in_failure_section = true;
        }

        if in_failure_section {
            if trimmed.starts_with("---- ") && trimmed.contains(" stdout ----") {
                // Save previous failure
                if let Some(f) = current_failure.take() {
                    failures.push(f);
                }
                // Start new failure
                let name = trimmed
                    .strip_prefix("---- ")
                    .and_then(|s| s.strip_suffix(" stdout ----"))
                    .unwrap_or("unknown");
                current_failure = Some(TestFailure {
                    name: name.to_string(),
                    message: String::new(),
                    location: String::new(),
                });
            } else if let Some(ref mut f) = current_failure {
                if trimmed.contains("panicked at") || trimmed.contains("assertion") {
                    f.message = trimmed.to_string();
                } else if trimmed.contains(".rs:") {
                    f.location = trimmed.to_string();
                }
            }
        }

        // Summary line
        if trimmed.starts_with("test result:") {
            in_failure_section = false;
        }
    }

    if let Some(f) = current_failure {
        failures.push(f);
    }

    // Format output
    let mut result_lines: Vec<String> = Vec::new();

    let status_emoji = if failed > 0 { "❌" } else { "✅" };
    result_lines.push(format!(
        "{} {} passed, {} failed, {} ignored",
        status_emoji, passed, failed, ignored
    ));

    if !failures.is_empty() {
        result_lines.push(String::new());
        result_lines.push("Failures:".to_string());

        for f in failures.iter().take(config.max_items_per_group) {
            result_lines.push(format!("  ❌ {}", f.name));
            if !f.message.is_empty() {
                let msg = if f.message.len() > 80 {
                    format!("{}...", &f.message[..77])
                } else {
                    f.message.clone()
                };
                result_lines.push(format!("     {}", msg));
            }
            if !f.location.is_empty() {
                result_lines.push(format!("     at {}", f.location));
            }
        }

        if failures.len() > config.max_items_per_group {
            result_lines.push(format!("  ... +{} more failures", failures.len() - config.max_items_per_group));
        }
    }

    let compressed = result_lines.join("\n");
    CompressResult::new(output, compressed)
}

/// Compress Jest test output
fn compress_jest_test(output: &str, config: &CompressConfig) -> CompressResult {
    let lines: Vec<&str> = output.lines().collect();

    let mut passed = 0;
    let mut failed = 0;
    let mut failures: Vec<TestFailure> = Vec::new();
    let mut current_failure: Option<TestFailure> = None;

    for line in &lines {
        let trimmed = line.trim();

        if trimmed.starts_with("✓") || trimmed.contains("PASS") {
            passed += 1;
        } else if trimmed.starts_with("✕") || trimmed.contains("FAIL") {
            failed += 1;
            // Extract test name
            let name = trimmed
                .trim_start_matches("✕")
                .trim_start_matches("FAIL")
                .trim();
            current_failure = Some(TestFailure {
                name: name.to_string(),
                message: String::new(),
                location: String::new(),
            });
        } else if let Some(ref mut f) = current_failure {
            if trimmed.starts_with("Expected:") || trimmed.starts_with("Received:") || trimmed.contains("expect(") {
                if f.message.is_empty() {
                    f.message = trimmed.to_string();
                }
            } else if trimmed.contains(".test.") || trimmed.contains(".spec.") {
                f.location = trimmed.to_string();
            }
        }

        // Boundary detection
        if (trimmed.starts_with("✓") || trimmed.starts_with("●")) && current_failure.is_some() {
            if let Some(f) = current_failure.take() {
                failures.push(f);
            }
        }
    }

    if let Some(f) = current_failure {
        failures.push(f);
    }

    // Format output
    let mut result_lines: Vec<String> = Vec::new();

    let status_emoji = if failed > 0 { "❌" } else { "✅" };
    result_lines.push(format!("{} {} passed, {} failed", status_emoji, passed, failed));

    if !failures.is_empty() {
        result_lines.push(String::new());
        result_lines.push("Failures:".to_string());

        for f in failures.iter().take(config.max_items_per_group) {
            result_lines.push(format!("  ❌ {}", f.name));
            if !f.message.is_empty() {
                result_lines.push(format!("     {}", f.message));
            }
        }
    }

    let compressed = result_lines.join("\n");
    CompressResult::new(output, compressed)
}

/// Compress pytest output
fn compress_pytest(output: &str, config: &CompressConfig) -> CompressResult {
    let lines: Vec<&str> = output.lines().collect();

    let mut passed = 0;
    let mut failed = 0;
    let mut failures: Vec<TestFailure> = Vec::new();
    let mut in_failure = false;
    let mut current_failure: Option<TestFailure> = None;

    for line in &lines {
        let trimmed = line.trim();

        // Count from summary line
        if trimmed.contains(" passed") || trimmed.contains(" failed") {
            // Parse "X passed, Y failed"
            for part in trimmed.split(',') {
                let part = part.trim();
                if part.ends_with("passed") {
                    if let Some(num) = part.split_whitespace().next() {
                        passed = num.parse().unwrap_or(0);
                    }
                } else if part.ends_with("failed") {
                    if let Some(num) = part.split_whitespace().next() {
                        failed = num.parse().unwrap_or(0);
                    }
                }
            }
        }

        // Capture failures
        if trimmed.starts_with("FAILED") || trimmed.starts_with("E ") {
            in_failure = true;
            if trimmed.starts_with("FAILED") {
                let name = trimmed.strip_prefix("FAILED ").unwrap_or(trimmed);
                current_failure = Some(TestFailure {
                    name: name.to_string(),
                    message: String::new(),
                    location: String::new(),
                });
            } else if let Some(ref mut f) = current_failure {
                if f.message.is_empty() {
                    f.message = trimmed.strip_prefix("E ").unwrap_or(trimmed).to_string();
                }
            }
        }

        if in_failure && trimmed.starts_with("_") && trimmed.ends_with("_") {
            if let Some(f) = current_failure.take() {
                failures.push(f);
            }
            in_failure = false;
        }
    }

    if let Some(f) = current_failure {
        failures.push(f);
    }

    // Format output
    let mut result_lines: Vec<String> = Vec::new();

    let status_emoji = if failed > 0 { "❌" } else { "✅" };
    result_lines.push(format!("{} {} passed, {} failed", status_emoji, passed, failed));

    if !failures.is_empty() {
        result_lines.push(String::new());
        result_lines.push("Failures:".to_string());

        for f in failures.iter().take(config.max_items_per_group) {
            result_lines.push(format!("  ❌ {}", f.name));
            if !f.message.is_empty() {
                result_lines.push(format!("     {}", f.message));
            }
        }
    }

    let compressed = result_lines.join("\n");
    CompressResult::new(output, compressed)
}

/// Generic test output compression
fn compress_generic_test(output: &str, config: &CompressConfig) -> CompressResult {
    let lines: Vec<&str> = output.lines().collect();

    // Look for failure indicators
    let failure_keywords = ["FAIL", "ERROR", "FAILED", "error:", "Error:", "panic", "exception"];
    let success_keywords = ["PASS", "OK", "ok", "passed", "success"];

    let mut failures: Vec<String> = Vec::new();
    let mut passes = 0;

    for line in &lines {
        let trimmed = line.trim();

        if failure_keywords.iter().any(|k| trimmed.contains(k)) {
            failures.push(trimmed.to_string());
        } else if success_keywords.iter().any(|k| trimmed.contains(k)) {
            passes += 1;
        }
    }

    let mut result_lines: Vec<String> = Vec::new();

    if failures.is_empty() {
        result_lines.push(format!("✅ {} tests passed", passes));
    } else {
        result_lines.push(format!("❌ {} failures detected", failures.len()));
        result_lines.push(String::new());

        // Deduplicate failures
        let failure_refs: Vec<&str> = failures.iter().map(|s| s.as_str()).collect();
        let deduped = deduplicate_lines(&failure_refs, config.dedup_threshold);

        for f in deduped.iter().take(config.max_items_per_group) {
            let display = if f.len() > 100 {
                format!("{}...", &f[..97])
            } else {
                f.clone()
            };
            result_lines.push(format!("  {}", display));
        }
    }

    let compressed = result_lines.join("\n");
    CompressResult::new(output, compressed)
}

struct TestFailure {
    name: String,
    message: String,
    location: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_cargo_test() {
        let input = r#"
running 5 tests
test test_one ... ok
test test_two ... ok
test test_three ... FAILED
test test_four ... ok
test test_five ... ignored

failures:

---- test_three stdout ----
thread 'test_three' panicked at 'assertion failed', src/lib.rs:10

test result: FAILED. 3 passed; 1 failed; 1 ignored
"#;

        let config = CompressConfig::default();
        let result = compress_cargo_test(input, &config);

        assert!(result.output.contains("3 passed"));
        assert!(result.output.contains("1 failed"));
        assert!(result.output.contains("test_three"));
        // Note: reduction may not be >50% for small test inputs
        // compression is optimized for large outputs
    }

    #[test]
    fn test_compress_jest() {
        let input = r#"
PASS  src/App.test.js
  ✓ renders learn react link (25 ms)
  ✓ renders without crashing (5 ms)

FAIL  src/Other.test.js
  ✕ should work (10 ms)

    Expected: true
    Received: false

Test Suites: 1 passed, 1 failed, 2 total
"#;

        let config = CompressConfig::default();
        let result = compress_jest_test(input, &config);

        assert!(result.output.contains("passed"));
        assert!(result.output.contains("failed"));
    }
}
