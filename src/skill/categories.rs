use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstructionCategory {
    Architecture,
    Testing,
    Style,
    Navigation,
    Workflow,
    Tooling,
    Gotchas,
}

impl InstructionCategory {
    pub fn to_str(&self) -> &'static str {
        match self {
            InstructionCategory::Architecture => "architecture",
            InstructionCategory::Testing => "testing",
            InstructionCategory::Style => "style",
            InstructionCategory::Navigation => "navigation",
            InstructionCategory::Workflow => "workflow",
            InstructionCategory::Tooling => "tooling",
            InstructionCategory::Gotchas => "gotchas",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "architecture" => InstructionCategory::Architecture,
            "testing" => InstructionCategory::Testing,
            "style" => InstructionCategory::Style,
            "navigation" => InstructionCategory::Navigation,
            "workflow" => InstructionCategory::Workflow,
            "tooling" => InstructionCategory::Tooling,
            "gotchas" => InstructionCategory::Gotchas,
            _ => InstructionCategory::Gotchas, // Default to gotchas
        }
    }

    /// Order for display (gotchas first, most important)
    pub fn display_order(&self) -> u8 {
        match self {
            InstructionCategory::Gotchas => 0,
            InstructionCategory::Architecture => 1,
            InstructionCategory::Testing => 2,
            InstructionCategory::Style => 3,
            InstructionCategory::Navigation => 4,
            InstructionCategory::Workflow => 5,
            InstructionCategory::Tooling => 6,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInstruction {
    pub id: String,
    pub instruction: String,
    pub category: InstructionCategory,
    pub source: InstructionSource,
    pub confidence: Option<f32>,
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum InstructionSource {
    Pattern { id: String },
    Failure { id: String },
    Convention { pattern_ids: Vec<String> },
    Manual { reason: Option<String> },
}
