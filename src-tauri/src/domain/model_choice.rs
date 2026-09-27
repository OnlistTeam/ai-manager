//! Which model a tool runs and how hard it thinks, as chosen on Home (ADR-0054).
//!
//! The per-tool facts live in one table: whether the model can be chosen, the
//! effort levels the tool's own setting accepts, and a short list of models its
//! official service offers. The renderer receives them with the current choice,
//! so no screen holds a model name or checks a tool's name.

use serde::{Deserialize, Serialize};

use super::{AppError, ErrorCode, ToolId};

/// Longest model name accepted, matching the endpoint edit form.
pub const MAX_MODEL_NAME_CHARS: usize = 256;

/// What Home can choose for one tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelChoiceSpec {
    /// The levels the tool's own effort setting accepts, weakest first. Empty
    /// when the tool has no such setting.
    pub effort_levels: &'static [&'static str],
    /// A short starting set for the tool's official service. Anything else can
    /// be typed.
    pub official_models: &'static [&'static str],
}

/// Claude Code's aliases follow the newest model of each family, so the list
/// does not age with every release. `max` is accepted by Claude Code for one
/// session only and never in a settings file.
const CLAUDE_CODE: ModelChoiceSpec = ModelChoiceSpec {
    effort_levels: &["low", "medium", "high", "xhigh"],
    official_models: &["fable", "opus", "opus[1m]", "sonnet", "haiku"],
};

/// Every model in Codex's own catalogue accepts these four; stronger levels
/// exist only on some models.
const CODEX: ModelChoiceSpec = ModelChoiceSpec {
    effort_levels: &["low", "medium", "high", "xhigh"],
    official_models: &[
        "gpt-6-sol",
        "gpt-6-astra",
        "gpt-6-luna",
        "gpt-5.6-sol",
        "gpt-5.5",
    ],
};

/// Gemini CLI resolves these aliases itself; it has no effort setting.
const GEMINI_CLI: ModelChoiceSpec = ModelChoiceSpec {
    effort_levels: &[],
    official_models: &["auto", "pro", "flash", "flash-lite"],
};

impl ModelChoiceSpec {
    /// `None` for a tool whose model is not chosen on Home: OpenCode picks it
    /// inside the tool (ADR-0039), and the long-tail tools keep the endpoint
    /// picker only.
    pub fn for_tool(tool: ToolId) -> Option<Self> {
        match tool {
            ToolId::ClaudeCode => Some(CLAUDE_CODE),
            ToolId::Codex => Some(CODEX),
            ToolId::GeminiCli => Some(GEMINI_CLI),
            ToolId::OpenCode
            | ToolId::GrokBuild
            | ToolId::OpenClaw
            | ToolId::Hermes
            | ToolId::Pi
            | ToolId::KimiCode
            | ToolId::DeepSeekDsh => None,
        }
    }

    pub fn has_effort(&self) -> bool {
        !self.effort_levels.is_empty()
    }
}

/// A model the tool keeps its own effort for, which outranks the tool-wide
/// setting this product writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffortOverride {
    pub model: String,
    pub effort: String,
}

/// The model and effort in the tool's live files, with what may be chosen.
///
/// `None` means this product's key is absent and the tool decides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolModelChoice {
    pub tool: ToolId,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub effort_levels: Vec<String>,
    pub official_models: Vec<String>,
    pub effort_overrides: Vec<EffortOverride>,
}

fn invalid(technical: &'static str) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.modelChoice.invalid")
        .with_technical(technical)
        .with_remediation("error.remediation.checkServiceSettings")
}

/// Trims a typed model name and refuses one that would change the shape of
/// the file it is written to.
pub fn normalize_model_name(model: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(model) = model.map(str::trim) else {
        return Ok(None);
    };
    if model.is_empty() {
        return Err(invalid("model name is empty"));
    }
    if model.chars().count() > MAX_MODEL_NAME_CHARS {
        return Err(invalid("model name is too long"));
    }
    if model
        .chars()
        .any(|character| character.is_control() || character == '"' || character == '\\')
    {
        return Err(invalid(
            "model name contains a control character or a quote",
        ));
    }
    Ok(Some(model.to_string()))
}

/// Only a level the tool's own setting accepts may be written.
pub fn validate_effort(spec: &ModelChoiceSpec, effort: Option<&str>) -> Result<(), AppError> {
    match effort {
        Some(level) if !spec.effort_levels.contains(&level) => {
            Err(invalid("effort level is not one the tool accepts"))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_tools_in_the_table_choose_a_model() {
        let choosing: Vec<ToolId> = ToolId::ALL
            .into_iter()
            .filter(|tool| ModelChoiceSpec::for_tool(*tool).is_some())
            .collect();
        assert_eq!(
            choosing,
            vec![ToolId::ClaudeCode, ToolId::Codex, ToolId::GeminiCli]
        );
    }

    #[test]
    fn claude_code_never_offers_a_session_only_level() {
        let spec = ModelChoiceSpec::for_tool(ToolId::ClaudeCode).expect("claude code");
        assert!(!spec.effort_levels.contains(&"max"));
        assert!(validate_effort(&spec, Some("max")).is_err());
        assert!(validate_effort(&spec, Some("xhigh")).is_ok());
        assert!(validate_effort(&spec, None).is_ok());
    }

    #[test]
    fn gemini_has_no_effort_setting() {
        let spec = ModelChoiceSpec::for_tool(ToolId::GeminiCli).expect("gemini");
        assert!(!spec.has_effort());
        assert!(validate_effort(&spec, Some("low")).is_err());
    }

    #[test]
    fn a_typed_model_name_is_trimmed_and_checked() {
        assert_eq!(
            normalize_model_name(Some("  gpt-5.5 ")).unwrap(),
            Some("gpt-5.5".to_string())
        );
        assert_eq!(normalize_model_name(None).unwrap(), None);
        for bad in ["", "   ", "a\nb", "say \"hi\"", "back\\slash"] {
            assert!(normalize_model_name(Some(bad)).is_err(), "{bad:?}");
        }
        let long = "m".repeat(MAX_MODEL_NAME_CHARS + 1);
        assert!(normalize_model_name(Some(&long)).is_err());
    }
}
