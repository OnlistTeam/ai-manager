//! Which model a tool runs and how hard it thinks, as chosen on Home (ADR-0055).
//!
//! The per-tool facts live in one table: whether the model can be chosen, the
//! effort levels the tool accepts, the models it keeps a saved effort for, and
//! a short list of models its official service offers. The renderer receives
//! them with the current choice, so no screen holds a model name or checks a
//! tool's name.

use serde::{Deserialize, Serialize};

use super::{AppError, EffectiveConnectionSource, ErrorCode, ToolId};

/// Longest model name accepted, matching the endpoint edit form.
pub const MAX_MODEL_NAME_CHARS: usize = 256;

/// What Home can choose for one tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelChoiceSpec {
    /// The levels Home offers for the tool's effort, weakest first. Empty when
    /// the tool has no such setting.
    pub effort_levels: &'static [&'static str],
    /// Levels the tool keeps only through its environment variable, which then
    /// holds them for every session and outranks the tool's own command.
    pub variable_only_levels: &'static [&'static str],
    /// A short starting set for the tool's official service. Anything else can
    /// be typed.
    pub official_models: &'static [&'static str],
    /// Models the tool keeps a saved effort for, one entry each.
    pub effort_models: &'static [EffortModel],
}

/// A model that takes an effort level, under the name the tool saves it by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffortModel {
    /// The canonical name the tool's per-model settings are keyed by.
    pub id: &'static str,
    /// Short names that currently resolve to this model.
    pub aliases: &'static [&'static str],
    /// Whether the tool-wide level in the user's own settings file applies to
    /// this model when it has no saved level of its own.
    pub reads_tool_wide_level: bool,
    /// The level the model runs at when nothing sets one.
    pub default_level: &'static str,
}

/// Claude Code saves `/effort` per model under `modelSettings`, keyed by these
/// names, and matches aliases and `[1m]` variants to the same entry. A
/// top-level `effortLevel` in the user settings file still applies to Opus 5,
/// Fable 5.1, Sonnet 5 and earlier; Opus 5.5 and later ignore it. Reviewed with
/// Claude Code's model configuration page, like the official list below.
const CLAUDE_EFFORT_MODELS: &[EffortModel] = &[
    EffortModel {
        id: "claude-fable-5-1",
        aliases: &["fable"],
        reads_tool_wide_level: true,
        default_level: "high",
    },
    EffortModel {
        id: "claude-opus-5-5",
        aliases: &["opus"],
        reads_tool_wide_level: false,
        default_level: "medium",
    },
    EffortModel {
        id: "claude-opus-5",
        aliases: &[],
        reads_tool_wide_level: true,
        default_level: "high",
    },
    EffortModel {
        id: "claude-sonnet-5",
        aliases: &["sonnet"],
        reads_tool_wide_level: true,
        default_level: "high",
    },
];

/// Claude Code's aliases follow the newest model of each family, so the list
/// does not age with every release. Its settings keys accept `low` to `xhigh`;
/// `max` persists only through `CLAUDE_CODE_EFFORT_LEVEL`.
const CLAUDE_CODE: ModelChoiceSpec = ModelChoiceSpec {
    effort_levels: &["low", "medium", "high", "xhigh", "max"],
    variable_only_levels: &["max"],
    official_models: &["fable", "opus", "opus[1m]", "sonnet", "haiku"],
    effort_models: CLAUDE_EFFORT_MODELS,
};

/// Every model in Codex's own catalogue accepts these four; stronger levels
/// exist only on some models.
const CODEX: ModelChoiceSpec = ModelChoiceSpec {
    effort_levels: &["low", "medium", "high", "xhigh"],
    variable_only_levels: &[],
    official_models: &[
        "gpt-6-sol",
        "gpt-6-astra",
        "gpt-6-luna",
        "gpt-5.6-sol",
        "gpt-5.5",
    ],
    effort_models: &[],
};

/// Gemini CLI resolves these aliases itself; it has no effort setting.
const GEMINI_CLI: ModelChoiceSpec = ModelChoiceSpec {
    effort_levels: &[],
    variable_only_levels: &[],
    official_models: &["auto", "pro", "flash", "flash-lite"],
    effort_models: &[],
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

    /// The table entry a model name refers to: its canonical name, an alias,
    /// or a spelling of either with a `[1m]`, date, gateway or cloud affix.
    pub fn effort_model(&self, name: &str) -> Option<&'static EffortModel> {
        let name = canonical_model_name(name);
        self.effort_models
            .iter()
            .find(|model| model.id == name || model.aliases.contains(&name.as_str()))
    }
}

/// A model name reduced to the form a tool keys its per-model settings by:
/// lower case, without a `[1m]` suffix, a gateway (`team/…`) or cloud
/// (`us.anthropic.…`, `…-v1:0`, `…@2026…`) spelling, or a date suffix.
pub fn canonical_model_name(name: &str) -> String {
    let mut name = name.trim().to_ascii_lowercase();
    if let Some(stripped) = name.strip_suffix("[1m]") {
        name = stripped.to_string();
    }
    if let Some((_, rest)) = name.rsplit_once('/') {
        name = rest.to_string();
    }
    if let Some((_, rest)) = name.split_once("anthropic.") {
        name = rest.to_string();
    }
    for separator in ['@', ':'] {
        if let Some((head, _)) = name.split_once(separator) {
            name = head.to_string();
        }
    }
    if let Some(stripped) = name.strip_suffix("-v1") {
        name = stripped.to_string();
    }
    if let Some((head, date)) = name.rsplit_once('-') {
        if date.len() == 8 && date.chars().all(|c| c.is_ascii_digit()) {
            name = head.to_string();
        }
    }
    name
}

/// The level one model runs at, and whether that is only the model's default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelEffort {
    pub model: String,
    pub effort: String,
    pub model_default: bool,
}

/// The effort a new session of the tool runs at, as the tool resolves it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum EffortInForce {
    /// Nothing sets a level; each model runs at its own default.
    ToolDefault,
    /// The tool's settings give this level to the model in use, or to every
    /// model when the model in use is not named.
    Level { level: String },
    /// The model in use is not named and the models run at different levels.
    Mixed { per_model: Vec<ModelEffort> },
    /// The tool's environment variable, set in its own settings file, holds
    /// this level for every session; the tool's own command cannot change it.
    Fixed { level: String },
    /// A terminal variable holds this level; Home cannot change it.
    Terminal {
        level: String,
        source: EffectiveConnectionSource,
    },
}

/// The model and effort in the tool's live files, with what may be chosen.
///
/// `model: None` means this product's key is absent and the tool decides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolModelChoice {
    pub tool: ToolId,
    pub model: Option<String>,
    pub effort: EffortInForce,
    pub effort_levels: Vec<String>,
    pub variable_only_levels: Vec<String>,
    pub official_models: Vec<String>,
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

/// Only a level Home offers for the tool may be written.
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
    fn claude_code_offers_max_only_through_its_variable() {
        let spec = ModelChoiceSpec::for_tool(ToolId::ClaudeCode).expect("claude code");
        assert_eq!(spec.effort_levels.last(), Some(&"max"));
        assert_eq!(spec.variable_only_levels, ["max"]);
        assert!(validate_effort(&spec, Some("max")).is_ok());
        assert!(validate_effort(&spec, Some("ultracode")).is_err());
        assert!(validate_effort(&spec, None).is_ok());
        let codex = ModelChoiceSpec::for_tool(ToolId::Codex).expect("codex");
        assert!(validate_effort(&codex, Some("max")).is_err());
    }

    #[test]
    fn model_names_find_their_entry_in_every_spelling() {
        let spec = ModelChoiceSpec::for_tool(ToolId::ClaudeCode).expect("claude code");
        let id = |name: &str| spec.effort_model(name).map(|model| model.id);
        assert_eq!(id("opus"), Some("claude-opus-5-5"));
        assert_eq!(id("opus[1m]"), Some("claude-opus-5-5"));
        assert_eq!(id(" Claude-Opus-5-5[1m] "), Some("claude-opus-5-5"));
        assert_eq!(id("claude-opus-5"), Some("claude-opus-5"));
        assert_eq!(id("claude-opus-5-20260301"), Some("claude-opus-5"));
        assert_eq!(
            id("us.anthropic.claude-opus-5-5-v1:0"),
            Some("claude-opus-5-5")
        );
        assert_eq!(id("claude-opus-5-5@20260801"), Some("claude-opus-5-5"));
        assert_eq!(id("my-gateway/claude-fable-5-1"), Some("claude-fable-5-1"));
        assert_eq!(id("fable"), Some("claude-fable-5-1"));
        assert_eq!(id("sonnet"), Some("claude-sonnet-5"));
        assert_eq!(id("sonnet[1m]"), Some("claude-sonnet-5"));
        assert_eq!(id("haiku"), None);
        assert_eq!(id("default"), None);
        assert_eq!(id("glm-5"), None);
    }

    #[test]
    fn only_opus_5_5_ignores_the_tool_wide_level() {
        let spec = ModelChoiceSpec::for_tool(ToolId::ClaudeCode).expect("claude code");
        let ignoring: Vec<&str> = spec
            .effort_models
            .iter()
            .filter(|model| !model.reads_tool_wide_level)
            .map(|model| model.id)
            .collect();
        assert_eq!(ignoring, ["claude-opus-5-5"]);
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
