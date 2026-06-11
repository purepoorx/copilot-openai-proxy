use crate::error::AppError;

/// Available Copilot models
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum CopilotModel {
    Default,
    Chat,
    Smart,
    Reasoning,
    Research,
    ComputerUse,
    Coco,
}

impl CopilotModel {
    /// Parse an OpenAI model name into a CopilotModel
    pub fn from_openai_name(name: &str) -> Result<Self, AppError> {
        match name {
            "" | "default" => Ok(Self::Default),
            "chat" => Ok(Self::Chat),
            "smart" => Ok(Self::Smart),
            "reasoning" => Ok(Self::Reasoning),
            "research" => Ok(Self::Research),
            "computer_use" | "computer-use" => Ok(Self::ComputerUse),
            "coco" => Ok(Self::Coco),
            s if s.starts_with("think-") || s.starts_with("think_") => Ok(Self::Reasoning),
            other => Err(AppError::UnsupportedModel(format!(
                "unsupported model {other:?}; available: default, chat, smart, reasoning, research, computer_use, coco"
            ))),
        }
    }

    /// Get the internal Copilot mode string
    pub fn to_copilot_mode(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Chat => "chat",
            Self::Smart => "smart",
            Self::Reasoning => "reasoning",
            Self::Research => "research",
            Self::ComputerUse => "computer_use",
            Self::Coco => "coco",
        }
    }
}

/// List of available model names for the /v1/models endpoint
/// Matches original binary: smart, chat, reasoning, coco
pub const AVAILABLE_MODELS: &[&str] = &[
    "smart",
    "chat",
    "reasoning",
    "coco",
];
