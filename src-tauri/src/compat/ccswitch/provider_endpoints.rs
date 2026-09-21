//! Product-safe facade over the inherited concurrent endpoint speed-test engine.

use std::collections::HashSet;

use crate::domain::{
    AppError, ErrorCode, ProviderEndpointCandidate, ProviderEndpointFailure,
    ProviderEndpointTestResult, MAX_PROVIDER_ENDPOINT_CANDIDATES, MAX_PROVIDER_ENDPOINT_ID_BYTES,
};
use crate::services::{EndpointLatency, EndpointTestFailure, SpeedtestService};

const REVIEWED_PRESET_TIMEOUT_SECS: u64 = 2;

fn invalid_batch(reason: &'static str) -> AppError {
    AppError::new(ErrorCode::ProviderUnreachable, "error.provider.testFailed")
        .with_technical(reason)
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn validate_candidates(candidates: &[ProviderEndpointCandidate]) -> Result<(), AppError> {
    if candidates.len() > MAX_PROVIDER_ENDPOINT_CANDIDATES {
        return Err(invalid_batch("endpoint candidate limit exceeded"));
    }

    let mut ids = HashSet::with_capacity(candidates.len());
    for candidate in candidates {
        let id = candidate.id.as_str();
        if id.trim() != id
            || id.is_empty()
            || id.len() > MAX_PROVIDER_ENDPOINT_ID_BYTES
            || id.chars().any(char::is_control)
        {
            return Err(invalid_batch("endpoint candidate id is invalid"));
        }
        if !ids.insert(id) {
            return Err(invalid_batch("endpoint candidate ids must be unique"));
        }
    }
    Ok(())
}

fn failure_from(value: EndpointTestFailure) -> ProviderEndpointFailure {
    match value {
        EndpointTestFailure::InvalidUrl => ProviderEndpointFailure::InvalidUrl,
        EndpointTestFailure::Timeout => ProviderEndpointFailure::Timeout,
        EndpointTestFailure::Dns => ProviderEndpointFailure::Dns,
        EndpointTestFailure::Tls => ProviderEndpointFailure::Tls,
        EndpointTestFailure::Connection => ProviderEndpointFailure::Connection,
        EndpointTestFailure::Request => ProviderEndpointFailure::Request,
    }
}

fn project_results(
    candidates: &[ProviderEndpointCandidate],
    raw_results: Vec<EndpointLatency>,
) -> Result<Vec<ProviderEndpointTestResult>, AppError> {
    if candidates.len() != raw_results.len() {
        return Err(invalid_batch(
            "endpoint speed-test engine returned an unexpected result count",
        ));
    }

    Ok(candidates
        .iter()
        .zip(raw_results)
        .map(|(candidate, result)| ProviderEndpointTestResult {
            candidate_id: candidate.id.clone(),
            latency_ms: result
                .latency
                .map(|latency| u64::try_from(latency).unwrap_or(u64::MAX)),
            http_status: result.status,
            failure: result.error.map(failure_from),
        })
        .collect())
}

pub async fn test_provider_endpoints(
    candidates: &[ProviderEndpointCandidate],
) -> Result<Vec<ProviderEndpointTestResult>, AppError> {
    validate_candidates(candidates)?;
    let urls = candidates
        .iter()
        .map(|candidate| candidate.url.clone())
        .collect();
    let raw_results = SpeedtestService::test_endpoints(urls, None)
        .await
        .map_err(|_| {
            AppError::new(ErrorCode::NetworkError, "error.provider.testFailed")
                .with_technical("endpoint speed-test engine failed")
                .with_remediation("error.remediation.retryOrViewDetails")
        })?;
    project_results(candidates, raw_results)
}

pub async fn test_reviewed_provider_presets(
    tool: crate::domain::ToolId,
) -> Result<Vec<ProviderEndpointTestResult>, AppError> {
    let candidates = super::provider::reviewed_endpoint_candidates(tool)?;
    let urls = candidates
        .iter()
        .map(|candidate| candidate.url.clone())
        .collect();
    let raw_results = SpeedtestService::test_endpoints(urls, Some(REVIEWED_PRESET_TIMEOUT_SECS))
        .await
        .map_err(|_| {
            AppError::new(ErrorCode::NetworkError, "error.provider.testFailed")
                .with_technical("reviewed preset speed-test engine failed")
                .with_remediation("error.remediation.retryOrViewDetails")
        })?;
    project_results(&candidates, raw_results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, url: &str) -> ProviderEndpointCandidate {
        ProviderEndpointCandidate {
            id: id.to_string(),
            url: url.to_string(),
        }
    }

    #[test]
    fn projection_correlates_by_id_without_returning_urls() {
        let candidates = vec![candidate(
            "official",
            "https://user:secret@example.test/v1?token=hidden",
        )];
        let projected = project_results(
            &candidates,
            vec![EndpointLatency {
                url: candidates[0].url.clone(),
                latency: Some(42),
                status: Some(204),
                error: None,
            }],
        )
        .expect("project result");

        let json = serde_json::to_string(&projected).expect("serialize projected result");
        assert_eq!(
            json,
            r#"[{"candidateId":"official","latencyMs":42,"httpStatus":204,"failure":null}]"#
        );
        assert!(!json.contains("example.test"));
        assert!(!json.contains("secret"));
        assert!(!json.contains("hidden"));
    }

    #[test]
    fn every_inherited_failure_maps_to_a_stable_domain_failure() {
        for (raw, expected) in [
            (
                EndpointTestFailure::InvalidUrl,
                ProviderEndpointFailure::InvalidUrl,
            ),
            (
                EndpointTestFailure::Timeout,
                ProviderEndpointFailure::Timeout,
            ),
            (EndpointTestFailure::Dns, ProviderEndpointFailure::Dns),
            (EndpointTestFailure::Tls, ProviderEndpointFailure::Tls),
            (
                EndpointTestFailure::Connection,
                ProviderEndpointFailure::Connection,
            ),
            (
                EndpointTestFailure::Request,
                ProviderEndpointFailure::Request,
            ),
        ] {
            assert_eq!(failure_from(raw), expected);
        }
    }

    #[test]
    fn candidate_validation_is_bounded_and_never_echoes_a_url() {
        let duplicate = vec![
            candidate("same", "https://first.example.test"),
            candidate("same", "https://second.example.test?token=secret"),
        ];
        let error = validate_candidates(&duplicate).expect_err("reject duplicate IDs");
        assert_eq!(error.code, ErrorCode::ProviderUnreachable);
        let debug = format!("{error:?}");
        assert!(!debug.contains("first.example.test"));
        assert!(!debug.contains("second.example.test"));
        assert!(!debug.contains("secret"));

        let too_many = (0..=MAX_PROVIDER_ENDPOINT_CANDIDATES)
            .map(|index| candidate(&format!("endpoint-{index}"), "https://example.test"))
            .collect::<Vec<_>>();
        assert!(validate_candidates(&too_many).is_err());
    }

    #[test]
    fn reviewed_candidates_match_every_profile_without_exposing_credentials() {
        for tool in crate::domain::ToolId::ALL {
            if !crate::compat::ccswitch::tools::capabilities_for(tool).can_manage_provider {
                continue;
            }
            let profile = crate::compat::ccswitch::provider::connection_profile_for(tool)
                .expect("reviewed profile");
            let candidates = super::super::provider::reviewed_endpoint_candidates(tool)
                .expect("reviewed endpoint candidates");
            assert_eq!(
                candidates
                    .iter()
                    .map(|candidate| candidate.id.as_str())
                    .collect::<Vec<_>>(),
                profile
                    .presets
                    .iter()
                    .map(|preset| preset.id.as_str())
                    .collect::<Vec<_>>(),
                "{tool:?} preset IDs drifted"
            );
            for candidate in candidates {
                let parsed = url::Url::parse(&candidate.url).expect("validated endpoint URL");
                assert_eq!(parsed.scheme(), "https");
                assert!(parsed.username().is_empty());
                assert!(parsed.password().is_none());
                assert!(parsed.query().is_none());
                assert!(parsed.fragment().is_none());
                assert!(!format!("{candidate:?}").contains(parsed.host_str().unwrap()));
            }
        }
    }
}
