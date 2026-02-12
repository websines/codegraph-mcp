use super::categories::{InstructionCategory, ProjectInstruction};
use super::distill::DistillResult;
use std::collections::HashMap;

/// Generate SKILL.md content from distill result
pub fn generate_project_skill_md(result: &DistillResult) -> String {
    let mut output = String::new();

    // Header
    output.push_str("# Project Skill\n\n");
    output.push_str(
        "> Auto-generated from learned patterns and failures. Do not edit manually.\n\n",
    );

    // Group instructions by category
    let mut by_category: HashMap<InstructionCategory, Vec<&ProjectInstruction>> = HashMap::new();
    for instruction in &result.instructions {
        by_category
            .entry(instruction.category.clone())
            .or_default()
            .push(instruction);
    }

    // Sort categories by display order
    let mut categories: Vec<_> = by_category.keys().cloned().collect();
    categories.sort_by_key(|c| c.display_order());

    // Render each category
    for category in categories {
        let instructions = by_category.get(&category).unwrap();

        output.push_str(&format!("## {}\n\n", category_title(&category)));

        for instruction in instructions {
            // Render instruction
            output.push_str(&format!("- {}", instruction.instruction));

            // Add scope if present
            if let Some(scope) = &instruction.scope {
                output.push_str(&format!(" (scope: `{}`)", scope));
            }

            // Add confidence if present
            if let Some(conf) = instruction.confidence {
                output.push_str(&format!(" — {:.0}% confidence", conf * 100.0));
            }

            output.push_str("\n");
        }

        output.push_str("\n");
    }

    // Footer
    output.push_str("---\n\n");
    output.push_str(&format!(
        "*Generated from {} patterns, {} failures, {} conventions*\n",
        count_by_source(&result.instructions, "Pattern"),
        count_by_source(&result.instructions, "Failure"),
        result.conventions.len()
    ));

    output
}

/// Get category title for display
fn category_title(category: &InstructionCategory) -> String {
    match category {
        InstructionCategory::Architecture => "Architecture",
        InstructionCategory::Testing => "Testing",
        InstructionCategory::Style => "Code Style",
        InstructionCategory::Navigation => "Navigation",
        InstructionCategory::Workflow => "Workflow",
        InstructionCategory::Tooling => "Tooling",
        InstructionCategory::Gotchas => "⚠️ Gotchas & Pitfalls",
    }
    .to_string()
}

/// Count instructions by source type
fn count_by_source(instructions: &[ProjectInstruction], source_type: &str) -> usize {
    instructions
        .iter()
        .filter(|i| format!("{:?}", i.source).contains(source_type))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill::categories::{InstructionSource, ProjectInstruction};

    #[test]
    fn test_generate_skill_md() {
        let result = DistillResult {
            instructions: vec![
                ProjectInstruction {
                    id: "1".to_string(),
                    instruction: "Always write tests".to_string(),
                    category: InstructionCategory::Testing,
                    source: InstructionSource::Pattern {
                        id: "p1".to_string(),
                    },
                    confidence: Some(0.9),
                    scope: None,
                },
                ProjectInstruction {
                    id: "2".to_string(),
                    instruction: "❌ Don't use var".to_string(),
                    category: InstructionCategory::Gotchas,
                    source: InstructionSource::Failure {
                        id: "f1".to_string(),
                    },
                    confidence: None,
                    scope: Some("src/**/*.js".to_string()),
                },
            ],
            conventions: vec![],
            navigation_hints: vec![],
        };

        let markdown = generate_project_skill_md(&result);

        assert!(markdown.contains("# Project Skill"));
        assert!(markdown.contains("## ⚠️ Gotchas & Pitfalls"));
        assert!(markdown.contains("## Testing"));
        assert!(markdown.contains("Always write tests"));
        assert!(markdown.contains("Don't use var"));
        assert!(markdown.contains("90% confidence"));
    }
}
