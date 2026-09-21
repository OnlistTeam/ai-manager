//! Keep native Codex provider IDs stable when constructing a product connection.
//!
//! They are history namespaces, not display names or our database row IDs. Only the
//! new template is adapted; imported records, auth.json and history are never rewritten.

use crate::domain::{AppError, ToolId};
use crate::provider::Provider;
use toml_edit::DocumentMut;

use super::{create::create_error, upstream_detail, ConnectionAttempt, ProviderStore};

const LEGACY_PRODUCT_ID: &str = "ai_manager_openai";

pub(super) fn prepare(
    store: &ProviderStore,
    tool: ToolId,
    attempt: &ConnectionAttempt,
    provider: &mut Provider,
) -> Result<(), AppError> {
    if tool != ToolId::Codex {
        return Ok(());
    }
    let source = if attempt.existing {
        // A retry belongs to the original record, not whichever provider happens to
        // be live now. In particular, do not silently migrate an old branded record.
        store
            .find_raw(tool, &attempt.id)?
            .settings_config
            .get("config")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(create_error)?
            .to_string()
    } else {
        crate::codex_config::read_and_validate_codex_config_text()
            .map_err(|error| create_error().with_technical(upstream_detail(&error)))?
    };
    let config = provider
        .settings_config
        .get("config")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(create_error)?;
    provider.settings_config["config"] = align_template(config, &source, attempt.existing)?.into();
    Ok(())
}

fn parse(text: &str) -> Result<DocumentMut, AppError> {
    // Never include a TOML parser excerpt: configurations can contain credentials.
    text.parse().map_err(|_| create_error())
}

fn active_id(doc: &DocumentMut) -> Option<&str> {
    doc.get("model_provider").and_then(|item| item.as_str())
}

fn align_template(template: &str, source: &str, retry: bool) -> Result<String, AppError> {
    let source = parse(source)?;
    let mut target = parse(template)?;
    let Some(id) = active_id(&source).filter(|id| {
        crate::codex_config::is_custom_codex_model_provider_id(id)
            && (retry || *id != LEGACY_PRODUCT_ID)
    }) else {
        return Ok(template.to_string());
    };
    if source
        .get("model_providers")
        .and_then(|item| item.as_table_like())
        .and_then(|table| table.get(id))
        .and_then(|item| item.as_table_like())
        .is_none()
    {
        return Err(create_error());
    }
    let Some(template_id) = active_id(&target)
        .filter(|id| crate::codex_config::is_custom_codex_model_provider_id(id))
        .map(str::to_string)
    else {
        // Built-in/auth-managed templates must retain their own authentication mode.
        return Ok(template.to_string());
    };
    if template_id == id {
        return Ok(template.to_string());
    }
    let providers = target
        .get_mut("model_providers")
        .and_then(|item| item.as_table_like_mut())
        .ok_or_else(create_error)?;
    if providers.contains_key(id) {
        // Never overwrite a second table or merge credentials from two providers.
        return Err(create_error());
    }
    let table = providers.remove(&template_id).ok_or_else(create_error)?;
    providers.insert(id, table);
    target["model_provider"] = toml_edit::value(id);
    Ok(target.to_string())
}

#[cfg(test)]
#[path = "codex_identity_tests.rs"]
mod tests;
