//! Product-safe facade for the model probe's two upstream dependencies
//! (ADR-0041): the proxy-aware HTTP client, and the saved service's address
//! and key.
//!
//! Nothing assembled here may be logged or projected to the renderer. The
//! credentials exist only long enough to build one request.

use crate::domain::{AppError, ErrorCode, ToolId};

use super::provider::ProviderStore;

/// The address and key of one saved service, resolved in the backend. The
/// renderer supplies neither and never receives either from this path.
pub struct ProviderProbeCredentials {
    pub base_url: String,
    /// Empty when the service has no key. Local gateways commonly need none.
    pub api_key: String,
}

impl std::fmt::Debug for ProviderProbeCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderProbeCredentials")
            .field("base_url", &"<redacted>")
            .field("api_key", &"<redacted>")
            .finish()
    }
}

fn not_probeable() -> AppError {
    AppError::new(ErrorCode::ProviderUnreachable, "error.provider.notTestable")
        .with_technical("saved service has no probeable address")
        .with_remediation("error.remediation.checkServiceSettings")
}

pub fn credentials_for(
    store: &ProviderStore,
    tool: ToolId,
    id: &str,
) -> Result<ProviderProbeCredentials, AppError> {
    let raw = store.find_raw(tool, id)?;
    let (base_url, api_key) =
        super::provider::probe_credentials_for(tool, &raw)?.ok_or_else(not_probeable)?;
    Ok(ProviderProbeCredentials { base_url, api_key })
}

/// The same, for the connection in force that is not a saved service: an
/// address exported by a shell profile, or written straight into the tool's own
/// configuration file.
///
/// It resolves through the same rules the services page displays, so testing
/// tests what the next launch will actually use.
pub fn credentials_for_effective(tool: ToolId) -> Result<ProviderProbeCredentials, AppError> {
    let (base_url, api_key) =
        super::provider_runtime::effective_probe_target(tool)?.ok_or_else(not_probeable)?;
    Ok(ProviderProbeCredentials { base_url, api_key })
}

/// The product's proxy-aware client. `get_for_url` also keeps an ambient OS
/// proxy from intercepting a user's own loopback gateway.
pub fn http_client_for(url: &str) -> reqwest::Client {
    crate::proxy::http_client::get_for_url(url)
}
