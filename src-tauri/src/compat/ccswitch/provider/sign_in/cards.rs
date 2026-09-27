//! The endpoint each signed-in account is (ADR-0061): an official record
//! bound to the account through upstream's `authBinding`, so switching to it
//! is switching account.

use sha2::{Digest, Sha256};

use crate::domain::{AppError, ErrorCode, ToolId};
use crate::provider::{AuthBinding, AuthBindingSource, Provider as UpstreamProvider, ProviderMeta};

use super::super::{app_type_for, upstream_detail, ProviderStore};
use super::{bound_account, claude_accounts};

pub(super) struct Kind {
    tool: ToolId,
    auth_provider: &'static str,
    prefix: &'static str,
    /// The official seed's settings, so a switch writes what switching to
    /// the seed writes, plus the account.
    settings: &'static str,
    website: &'static str,
    icon: &'static str,
    icon_color: &'static str,
}

pub(super) const CLAUDE: Kind = Kind {
    tool: ToolId::ClaudeCode,
    auth_provider: claude_accounts::AUTH_PROVIDER,
    prefix: "claude-account",
    settings: r#"{"env":{}}"#,
    website: "https://www.anthropic.com/claude-code",
    icon: "anthropic",
    icon_color: "#D4915D",
};

pub(super) const CODEX: Kind = Kind {
    tool: ToolId::Codex,
    auth_provider: "codex_oauth",
    prefix: "codex-account",
    settings: r#"{"auth":{},"config":""}"#,
    website: "https://chatgpt.com/codex",
    icon: "openai",
    icon_color: "#00A67E",
};

/// The same account always gets the same record id.
pub(super) fn card_id(kind: &Kind, account: &str) -> String {
    let digest = Sha256::digest(account.as_bytes());
    let hex: String = digest[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("{}-{hex}", kind.prefix)
}

fn create_failed() -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.provider.createFailed")
        .with_remediation("error.remediation.retryOrViewDetails")
}

/// Returns the account's endpoint, adding it when there is none. The caller
/// holds the provider mutation lock.
pub(super) fn ensure(
    store: &ProviderStore,
    kind: &Kind,
    account: &str,
    name: &str,
) -> Result<String, AppError> {
    let (rows, _) = store.raw_inventory(kind.tool)?;
    if let Some(row) = rows
        .values()
        .find(|row| bound_account(row, kind.auth_provider).as_deref() == Some(account))
    {
        return Ok(row.id.clone());
    }
    let settings = serde_json::from_str(kind.settings).map_err(|_| create_failed())?;
    let mut provider = UpstreamProvider::with_id(
        card_id(kind, account),
        name.to_string(),
        settings,
        Some(kind.website.to_string()),
    );
    provider.category = Some("official".to_string());
    provider.icon = Some(kind.icon.to_string());
    provider.icon_color = Some(kind.icon_color.to_string());
    provider.sort_index = Some(
        rows.values()
            .filter_map(|row| row.sort_index)
            .max()
            .map_or(0, |index| index + 1),
    );
    provider.created_at = Some(chrono::Utc::now().timestamp_millis());
    provider.meta = Some(ProviderMeta {
        auth_binding: Some(AuthBinding {
            source: AuthBindingSource::ManagedAccount,
            auth_provider: Some(kind.auth_provider.to_string()),
            account_id: Some(account.to_string()),
        }),
        ..Default::default()
    });
    store
        .state
        .db
        .save_provider(app_type_for(kind.tool).as_str(), &provider)
        .map_err(|error| create_failed().with_technical(upstream_detail(&error)))?;
    Ok(provider.id)
}
