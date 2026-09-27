//! The entry that leaves a tool on its own sign-in (ADR-0057).
//!
//! Upstream seeds one "official" record per tool that has an account of its
//! own: Claude Code's `/login`, Codex's ChatGPT sign-in, Gemini CLI's Google
//! account, Grok Build's xAI login. Switching to it writes the tool's config
//! back to that sign-in. A user who removed it had no way back short of
//! reinstalling, so the add page's subscription card restores it through the
//! same upstream `ensure_official_seed_by_id` the import path already uses.
//!
//! The card does not sign anyone in. That stays inside the tool, and nothing
//! here ever sees the account.

use crate::database::{CODEX_OFFICIAL_PROVIDER_ID, GROKBUILD_OFFICIAL_PROVIDER_ID};
use crate::domain::{AppError, ErrorCode, ProviderCreateResult, ToolId, ToolLoginAccount};

use super::{app_type_for, upstream_detail, ProviderStore};

/// The account and the upstream seed behind it, for the tools that have one.
///
/// Exhaustive on purpose (no `_ =>`): a new `ToolId` forces a conscious call.
fn login_for(tool: ToolId) -> Option<(ToolLoginAccount, &'static str)> {
    match tool {
        ToolId::ClaudeCode => Some((ToolLoginAccount::Claude, "claude-official")),
        ToolId::Codex => Some((ToolLoginAccount::ChatGpt, CODEX_OFFICIAL_PROVIDER_ID)),
        ToolId::GeminiCli => Some((ToolLoginAccount::Google, "gemini-official")),
        ToolId::GrokBuild => Some((ToolLoginAccount::SuperGrok, GROKBUILD_OFFICIAL_PROVIDER_ID)),
        ToolId::OpenCode
        | ToolId::OpenClaw
        | ToolId::Hermes
        | ToolId::Pi
        | ToolId::KimiCode
        | ToolId::DeepSeekDsh => None,
    }
}

/// The account the add page names on this tool's subscription card.
pub(super) fn account_for(tool: ToolId) -> Option<ToolLoginAccount> {
    login_for(tool).map(|(account, _)| account)
}

fn restore_failed() -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.provider.createFailed")
        .with_remediation("error.remediation.retryOrViewDetails")
}

/// Puts the seed back when it is missing and returns the list either way, so
/// a second click (or a stale page) is a no-op rather than a duplicate.
pub(super) fn restore(
    store: &ProviderStore,
    tool: ToolId,
) -> Result<ProviderCreateResult, AppError> {
    let (_, seed) = login_for(tool).ok_or_else(|| {
        restore_failed().with_technical(format!("{} has no sign-in of its own", tool.as_str()))
    })?;
    store
        .state
        .db
        .ensure_official_seed_by_id(seed, app_type_for(tool))
        .map_err(|error| restore_failed().with_technical(upstream_detail(&error)))?;
    Ok(ProviderCreateResult {
        providers: store.list(tool)?,
        created_provider_id: seed.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{account_for, restore};
    use crate::compat::ccswitch::provider::ProviderStore;
    use crate::database::Database;
    use crate::domain::{ProviderKind, ToolId, ToolLoginAccount};
    use crate::store::AppState;

    fn store() -> ProviderStore {
        ProviderStore {
            state: AppState::new(Arc::new(Database::memory().expect("memory database"))),
        }
    }

    #[test]
    fn each_tool_with_its_own_sign_in_names_its_account() {
        assert_eq!(
            account_for(ToolId::ClaudeCode),
            Some(ToolLoginAccount::Claude)
        );
        assert_eq!(account_for(ToolId::Codex), Some(ToolLoginAccount::ChatGpt));
        assert_eq!(
            account_for(ToolId::GeminiCli),
            Some(ToolLoginAccount::Google)
        );
        assert_eq!(
            account_for(ToolId::GrokBuild),
            Some(ToolLoginAccount::SuperGrok)
        );
        for tool in [
            ToolId::OpenCode,
            ToolId::OpenClaw,
            ToolId::Hermes,
            ToolId::Pi,
        ] {
            assert_eq!(account_for(tool), None, "{tool:?}");
        }
    }

    #[test]
    fn restoring_the_sign_in_entry_is_idempotent() {
        let store = store();
        for tool in [
            ToolId::ClaudeCode,
            ToolId::Codex,
            ToolId::GeminiCli,
            ToolId::GrokBuild,
        ] {
            let first = restore(&store, tool).expect("restore");
            let second = restore(&store, tool).expect("restore again");
            assert_eq!(first.created_provider_id, second.created_provider_id);
            assert_eq!(first.providers, second.providers, "{tool:?}");
            let official: Vec<_> = second
                .providers
                .iter()
                .filter(|provider| provider.kind == ProviderKind::Official)
                .collect();
            assert_eq!(official.len(), 1, "{tool:?}");
            assert_eq!(official[0].id, second.created_provider_id);
            assert_eq!(official[0].api_key, None, "{tool:?} saved a key");
        }
    }

    #[test]
    fn a_tool_without_its_own_sign_in_restores_nothing() {
        let store = store();
        assert!(restore(&store, ToolId::OpenCode).is_err());
        assert!(store.list(ToolId::OpenCode).expect("list").is_empty());
    }
}
