//! Whether the local route can carry a tool's current endpoint (ADR-0054).
//!
//! The answer comes from the same adapter the forwarder uses: a request can
//! only be forwarded when the endpoint yields an address and a credential (or
//! is an account the forwarder signs in for itself). A tool that signs in
//! with its own account keeps that login in the tool, out of AI Manager's
//! reach, so routing it would send every request nowhere.

use crate::app_config::AppType;
use crate::domain::RoutingUnavailable;
use crate::provider::Provider;

pub(super) fn unavailable_reason(
    app_type: &AppType,
    provider: Option<&Provider>,
) -> Option<RoutingUnavailable> {
    let Some(provider) = provider else {
        return Some(RoutingUnavailable::NoService);
    };
    if crate::services::provider::official_provider_supports_proxy_takeover(app_type, provider) {
        return None;
    }
    let Some(adapter) = crate::proxy::providers::get_adapter(app_type) else {
        return Some(RoutingUnavailable::Incomplete);
    };
    let has_address = adapter
        .extract_base_url(provider)
        .is_ok_and(|url| !url.trim().is_empty());
    let has_credential = adapter.extract_auth(provider).is_some();
    if has_address && has_credential {
        return None;
    }
    let own_login = provider.category.as_deref() == Some("official") || !has_address;
    Some(if own_login {
        RoutingUnavailable::OwnLogin
    } else {
        RoutingUnavailable::Incomplete
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::unavailable_reason;
    use crate::app_config::AppType;
    use crate::domain::RoutingUnavailable;
    use crate::provider::Provider;

    fn provider(settings: serde_json::Value, category: Option<&str>) -> Provider {
        let mut provider = Provider::with_id("p".into(), "P".into(), settings, None);
        provider.category = category.map(str::to_string);
        provider
    }

    #[test]
    fn an_api_key_endpoint_can_be_routed() {
        let relay = provider(
            json!({ "env": {
                "ANTHROPIC_BASE_URL": "https://relay.example.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-relay"
            }}),
            Some("third_party"),
        );
        assert_eq!(unavailable_reason(&AppType::Claude, Some(&relay)), None);
    }

    #[test]
    fn an_own_account_login_cannot_be_routed() {
        let official = provider(json!({ "env": {} }), Some("official"));
        assert_eq!(
            unavailable_reason(&AppType::Claude, Some(&official)),
            Some(RoutingUnavailable::OwnLogin)
        );
        let google = provider(json!({ "env": {} }), Some("official"));
        assert_eq!(
            unavailable_reason(&AppType::Gemini, Some(&google)),
            Some(RoutingUnavailable::OwnLogin)
        );
    }

    #[test]
    fn an_endpoint_without_a_key_or_without_a_choice_cannot_be_routed() {
        let keyless = provider(
            json!({ "env": { "ANTHROPIC_BASE_URL": "https://relay.example.com" } }),
            Some("third_party"),
        );
        assert_eq!(
            unavailable_reason(&AppType::Claude, Some(&keyless)),
            Some(RoutingUnavailable::Incomplete)
        );
        assert_eq!(
            unavailable_reason(&AppType::Codex, None),
            Some(RoutingUnavailable::NoService)
        );
    }
}
