//! Official provider seed data
//!
//! At startup `Database::init_default_official_providers` writes these entries into the
//! `providers` table so every user has a one-click way back to the official provider.
//!
//! Fields mirror the frontend presets, see:
//! - `src/config/claudeProviderPresets.ts` ("Claude Official")
//! - `src/config/codexProviderPresets.ts` ("OpenAI Official")
//! - `src/config/geminiProviderPresets.ts` ("Google Official")
//! - `src/components/providers/forms/GrokBuildProviderForm.tsx` ("Grok Official")

use crate::app_config::AppType;

pub(crate) const CLAUDE_DESKTOP_OFFICIAL_PROVIDER_ID: &str = "claude-desktop-official";
pub(crate) const CODEX_OFFICIAL_PROVIDER_ID: &str = "codex-official";
pub(crate) const GROKBUILD_OFFICIAL_PROVIDER_ID: &str = "grokbuild-official";

/// A single official provider seed definition.
pub(crate) struct OfficialProviderSeed {
    pub id: &'static str,
    pub app_type: AppType,
    pub name: &'static str,
    pub website_url: &'static str,
    pub icon: &'static str,
    pub icon_color: &'static str,
    /// JSON string for settings_config; the shape differs per app.
    pub settings_config_json: &'static str,
}

/// Official presets for Claude / Claude Desktop / Codex / Gemini.
///
/// The id is fixed to make idempotency checks easy; the name keeps its English original
/// (matching the frontend presets) and is not localized.
pub(crate) const OFFICIAL_SEEDS: &[OfficialProviderSeed] = &[
    OfficialProviderSeed {
        id: "claude-official",
        app_type: AppType::Claude,
        name: "Claude Official",
        website_url: "https://www.anthropic.com/claude-code",
        icon: "anthropic",
        icon_color: "#D4915D",
        // An empty env leaves the user on the Claude CLI default auth flow
        settings_config_json: r#"{"env":{}}"#,
    },
    OfficialProviderSeed {
        id: CLAUDE_DESKTOP_OFFICIAL_PROVIDER_ID,
        app_type: AppType::ClaudeDesktop,
        name: "Claude Desktop Official",
        website_url: "https://claude.ai/download",
        icon: "anthropic",
        icon_color: "#D4915D",
        // The empty env is just a placeholder; switching to this provider restores Claude Desktop 1P mode
        settings_config_json: r#"{"env":{}}"#,
    },
    OfficialProviderSeed {
        id: CODEX_OFFICIAL_PROVIDER_ID,
        app_type: AppType::Codex,
        name: "OpenAI Official",
        website_url: "https://chatgpt.com/codex",
        icon: "openai",
        icon_color: "#00A67E",
        // Empty auth + empty config leaves the user on ChatGPT Plus/Pro OAuth
        settings_config_json: r#"{"auth":{},"config":""}"#,
    },
    OfficialProviderSeed {
        id: "gemini-official",
        app_type: AppType::Gemini,
        name: "Google Official",
        website_url: "https://ai.google.dev/",
        icon: "gemini",
        icon_color: "#4285F4",
        // Empty env + empty config leaves the user on Google OAuth
        settings_config_json: r#"{"env":{},"config":{}}"#,
    },
    OfficialProviderSeed {
        id: GROKBUILD_OFFICIAL_PROVIDER_ID,
        app_type: AppType::GrokBuild,
        name: "Grok Official",
        website_url: "https://x.ai/grok",
        icon: "grok",
        icon_color: "currentColor",
        // Empty config = no custom model table written, so Grok CLI falls back to its built-in xAI OAuth login
        settings_config_json: r#"{"config":""}"#,
    },
];

/// Report whether the given provider id belongs to a built-in official seed.
///
/// Single source of truth: scan `OFFICIAL_SEEDS` directly instead of duplicating the id list.
pub(crate) fn is_official_seed_id(id: &str) -> bool {
    OFFICIAL_SEEDS.iter().any(|seed| seed.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_seeds_include_claude_desktop() {
        let seed = OFFICIAL_SEEDS
            .iter()
            .find(|seed| seed.id == CLAUDE_DESKTOP_OFFICIAL_PROVIDER_ID)
            .expect("claude desktop official seed");

        assert_eq!(seed.app_type, AppType::ClaudeDesktop);
        assert!(is_official_seed_id(CLAUDE_DESKTOP_OFFICIAL_PROVIDER_ID));
    }

    #[test]
    fn official_seeds_include_grokbuild() {
        let seed = OFFICIAL_SEEDS
            .iter()
            .find(|seed| seed.id == GROKBUILD_OFFICIAL_PROVIDER_ID)
            .expect("grok build official seed");

        assert_eq!(seed.app_type, AppType::GrokBuild);
        assert!(is_official_seed_id(GROKBUILD_OFFICIAL_PROVIDER_ID));
        // Empty config = official login state: no custom model table is injected on switch
        assert_eq!(seed.settings_config_json, r#"{"config":""}"#);
    }
}
