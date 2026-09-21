//! Use-case orchestration for the model probe (ADR-0041).
//!
//! Decides whether the action is allowed, resolves the saved service's address
//! and key in the backend, runs one outbound request, and maps machine-readable
//! adapter outcomes onto stable error codes.
//!
//! What it logs is deliberately thin: the service ID, the dialect, and nothing
//! else. The address, the key, the prompt and the reply never reach a log.

use crate::adapters::model_probe::{self, ProbeError, ProbeTarget};
use crate::compat::ccswitch::model_probe::{
    credentials_for, credentials_for_effective, http_client_for,
};
use crate::compat::ccswitch::provider::ProviderStore;
use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::{
    AppError, ErrorCode, ModelCatalog, ModelProbeOutcome, ModelProbeRequest, ProviderWireProtocol,
    ToolId, MAX_PROBE_PROMPT_CHARS,
};

/// The same ceiling the edit form applies to a typed model name.
const MAX_PROBE_MODEL_CHARS: usize = 256;

fn unsupported(tool: ToolId) -> AppError {
    AppError::new(
        ErrorCode::ProviderNotFound,
        "error.provider.unsupportedTool",
    )
    .with_technical(format!("{} cannot manage services", tool.as_str()))
}

fn gate(app_handle: &tauri::AppHandle, tool: ToolId) -> Result<ProviderStore, AppError> {
    if !capabilities_for(tool).can_manage_provider {
        return Err(unsupported(tool));
    }
    ProviderStore::open(app_handle)
}

fn invalid_request(message_key: &'static str, technical: &'static str) -> AppError {
    AppError::new(ErrorCode::ProviderUnreachable, message_key)
        .with_technical(technical)
        .with_remediation("error.remediation.checkServiceSettings")
}

/// Adapter outcome -> stable product error.
///
/// `technical` is a fixed English phrase, never upstream text: an upstream
/// explanation belongs in the reply the user reads, not in a diagnostic that
/// may be logged.
fn probe_error(error: ProbeError) -> AppError {
    let (code, message_key, technical) = match error {
        ProbeError::Timeout => (
            ErrorCode::NetworkError,
            "error.provider.modelProbeTimeout",
            "model probe timed out",
        ),
        ProbeError::Network => (
            ErrorCode::NetworkError,
            "error.provider.modelProbeUnreachable",
            "model probe could not reach the service",
        ),
        ProbeError::ResponseTooLarge => (
            ErrorCode::ProviderUnreachable,
            "error.provider.modelProbeUnreadable",
            "model probe response exceeded the size limit",
        ),
        ProbeError::ResponseUnreadable => (
            ErrorCode::ProviderUnreachable,
            "error.provider.modelProbeUnreadable",
            "model probe response was not valid JSON",
        ),
        ProbeError::ImageTooLarge => (
            ErrorCode::ProviderUnreachable,
            "error.provider.modelProbeImageTooLarge",
            "generated image exceeded the size limit",
        ),
    };
    AppError::new(code, message_key)
        .with_technical(technical)
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn validate(request: &ModelProbeRequest) -> Result<(), AppError> {
    if request.model.trim().is_empty() || request.model.chars().count() > MAX_PROBE_MODEL_CHARS {
        return Err(invalid_request(
            "error.provider.modelProbeModelInvalid",
            "model name is empty or too long",
        ));
    }
    if request.prompt.trim().is_empty() || request.prompt.chars().count() > MAX_PROBE_PROMPT_CHARS {
        return Err(invalid_request(
            "error.provider.modelProbePromptInvalid",
            "prompt is empty or too long",
        ));
    }
    Ok(())
}

fn target_for(
    app_handle: &tauri::AppHandle,
    tool: ToolId,
    id: &str,
) -> Result<ProbeTarget, AppError> {
    let store = gate(app_handle, tool)?;
    let credentials = credentials_for(&store, tool, id)?;
    Ok(ProbeTarget {
        base_url: credentials.base_url,
        api_key: credentials.api_key,
        protocol: ProviderWireProtocol::for_tool(tool),
    })
}

/// The same target, for the connection in force that has no saved service
/// behind it — one a shell profile exports, or the tool's own configuration
/// file declares. The renderer names only the tool.
fn effective_target(tool: ToolId) -> Result<ProbeTarget, AppError> {
    if !capabilities_for(tool).can_manage_provider {
        return Err(unsupported(tool));
    }
    let credentials = credentials_for_effective(tool)?;
    Ok(ProbeTarget {
        base_url: credentials.base_url,
        api_key: credentials.api_key,
        protocol: ProviderWireProtocol::for_tool(tool),
    })
}

/// The subject written to the log line. Never an address and never a key: a
/// saved service's own id, or the fixed word for the connection in force.
const EFFECTIVE_SUBJECT: &str = "effective connection";

async fn run_list(
    tool: ToolId,
    target: ProbeTarget,
    subject: &str,
) -> Result<ModelCatalog, AppError> {
    let client = http_client_for(&target.base_url);
    let catalog = model_probe::list_models(&client, &target)
        .await
        .map_err(probe_error)?;
    log::info!(
        "model catalogue for {}/{subject} over {:?}: {} models{}",
        tool.as_str(),
        target.protocol,
        catalog.models.len(),
        if catalog.rejection.is_some() {
            " (refused)"
        } else {
            ""
        }
    );
    Ok(catalog)
}

async fn run_probe(
    tool: ToolId,
    target: ProbeTarget,
    subject: &str,
    request: ModelProbeRequest,
) -> Result<ModelProbeOutcome, AppError> {
    validate(&request)?;
    let client = http_client_for(&target.base_url);
    let outcome = model_probe::probe_model(&client, &target, &request)
        .await
        .map_err(probe_error)?;
    log::info!(
        "model probe for {}/{subject} over {:?}: {:?}",
        tool.as_str(),
        target.protocol,
        outcome
    );
    Ok(outcome)
}

pub struct ModelProbeService;

impl ModelProbeService {
    /// Lists the models the endpoint actually serves.
    pub async fn list_models(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        id: &str,
    ) -> Result<ModelCatalog, AppError> {
        run_list(tool, target_for(app_handle, tool, id)?, id).await
    }

    /// The same, for the connection in force that is not a saved service.
    pub async fn list_models_for_effective(tool: ToolId) -> Result<ModelCatalog, AppError> {
        run_list(tool, effective_target(tool)?, EFFECTIVE_SUBJECT).await
    }

    /// Sends one real request to the service and reports what came back.
    pub async fn probe(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
        id: &str,
        request: ModelProbeRequest,
    ) -> Result<ModelProbeOutcome, AppError> {
        run_probe(tool, target_for(app_handle, tool, id)?, id, request).await
    }

    /// The same, for the connection in force that is not a saved service.
    pub async fn probe_effective(
        tool: ToolId,
        request: ModelProbeRequest,
    ) -> Result<ModelProbeOutcome, AppError> {
        run_probe(tool, effective_target(tool)?, EFFECTIVE_SUBJECT, request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ProbeModelKind;

    fn request(model: &str, prompt: &str) -> ModelProbeRequest {
        serde_json::from_value(serde_json::json!({
            "model": model,
            "kind": "text",
            "prompt": prompt,
        }))
        .expect("build probe request")
    }

    #[test]
    fn an_ordinary_request_passes_validation() {
        assert!(validate(&request("gpt-5.2", "hi")).is_ok());
    }

    #[test]
    fn an_empty_model_or_prompt_is_refused_before_any_request() {
        assert_eq!(
            validate(&request("   ", "hi")).unwrap_err().message_key,
            "error.provider.modelProbeModelInvalid"
        );
        assert_eq!(
            validate(&request("gpt-5.2", "  ")).unwrap_err().message_key,
            "error.provider.modelProbePromptInvalid"
        );
    }

    #[test]
    fn oversized_inputs_are_refused_before_any_request() {
        let long_model = "m".repeat(MAX_PROBE_MODEL_CHARS + 1);
        assert!(validate(&request(&long_model, "hi")).is_err());

        let long_prompt = "字".repeat(MAX_PROBE_PROMPT_CHARS + 1);
        assert!(validate(&request("gpt-5.2", &long_prompt)).is_err());

        // Counting characters, not bytes: a 200-character prompt of multi-byte
        // text is exactly at the limit, not three times over it.
        let boundary = "字".repeat(MAX_PROBE_PROMPT_CHARS);
        assert!(validate(&request("gpt-5.2", &boundary)).is_ok());
    }

    #[test]
    fn every_adapter_outcome_maps_to_a_registered_key() {
        for error in [
            ProbeError::Timeout,
            ProbeError::Network,
            ProbeError::ResponseTooLarge,
            ProbeError::ResponseUnreadable,
            ProbeError::ImageTooLarge,
        ] {
            let mapped = probe_error(error);
            assert!(crate::domain::message_keys::USER_FACING_MESSAGE_KEYS
                .contains(&mapped.message_key.as_str()));
            // Upstream words never travel in a diagnostic field.
            assert!(mapped.technical_message.is_some());
        }
    }

    #[test]
    fn a_probe_request_debug_rendering_stays_redacted() {
        let request = ModelProbeRequest {
            model: "gpt-5.2".to_string(),
            kind: ProbeModelKind::Text,
            prompt: "a private question".to_string(),
        };
        assert!(!format!("{request:?}").contains("a private question"));
    }
}
