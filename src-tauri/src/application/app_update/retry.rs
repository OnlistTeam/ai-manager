use std::time::Duration;

use tauri_plugin_updater::Error;

pub(super) const MAX_ATTEMPTS: u8 = 3;

pub(super) fn endpoint_order<T: Clone>(endpoints: &[T], attempt: u8) -> Vec<T> {
    if endpoints.is_empty() {
        return Vec::new();
    }
    let offset = usize::from(attempt.saturating_sub(1)) % endpoints.len();
    endpoints[offset..]
        .iter()
        .chain(endpoints[..offset].iter())
        .cloned()
        .collect()
}

pub(super) fn backoff_after(attempt: u8) -> Duration {
    match attempt {
        1 => Duration::from_millis(500),
        2 => Duration::from_millis(1_500),
        _ => Duration::ZERO,
    }
}

pub(super) fn retryable_updater_error(error: &Error) -> bool {
    match error {
        Error::Reqwest(error) => error
            .status()
            .map(retryable_http_status)
            .unwrap_or_else(|| {
                error.is_timeout()
                    || error.is_connect()
                    || (error.is_request() && !error.is_builder())
            }),
        Error::Network(message) => download_status(message).is_some_and(retryable_http_status),
        Error::EmptyEndpoints
        | Error::Io(_)
        | Error::Semver(_)
        | Error::Serialization(_)
        | Error::ReleaseNotFound
        | Error::UnsupportedArch
        | Error::UnsupportedOs
        | Error::FailedToDetermineExtractPath
        | Error::UrlParse(_)
        | Error::TargetNotFound(_)
        | Error::TargetsNotFound(_)
        | Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::TempDirNotOnSameMountPoint
        | Error::BinaryNotFoundInArchive
        | Error::TempDirNotFound
        | Error::AuthenticationFailed
        | Error::DebInstallFailed
        | Error::PackageInstallFailed
        | Error::InvalidUpdaterFormat
        | Error::Http(_)
        | Error::InvalidHeaderValue(_)
        | Error::InvalidHeaderName(_)
        | Error::FormatDate
        | Error::InsecureTransportProtocol
        | Error::Tauri(_) => false,
        _ => false,
    }
}

fn retryable_http_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

fn download_status(message: &str) -> Option<reqwest::StatusCode> {
    let (_, suffix) = message.rsplit_once("status:")?;
    let code = suffix.split_whitespace().next()?.parse::<u16>().ok()?;
    reqwest::StatusCode::from_u16(code).ok()
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use tauri_plugin_updater::Error;

    #[test]
    fn retries_rotate_the_preferred_endpoint_without_losing_fallbacks() {
        let endpoints = ["primary", "backup"];
        assert_eq!(super::endpoint_order(&endpoints, 1), endpoints);
        assert_eq!(super::endpoint_order(&endpoints, 2), ["backup", "primary"]);
        assert_eq!(super::endpoint_order(&endpoints, 3), endpoints);
        assert!(super::endpoint_order::<&str>(&[], 1).is_empty());
    }

    #[test]
    fn retry_budget_is_bounded_and_backoff_does_not_grow_forever() {
        assert_eq!(super::MAX_ATTEMPTS, 3);
        assert_eq!(super::backoff_after(1).as_millis(), 500);
        assert_eq!(super::backoff_after(2).as_millis(), 1_500);
        assert!(super::backoff_after(3).is_zero());
    }

    #[test]
    fn only_temporary_download_statuses_are_retried() {
        for status in [408, 429, 500, 502, 503, 599] {
            assert!(super::retryable_updater_error(&Error::Network(format!(
                "Download request failed with status: {status}"
            ))));
        }
        for status in [400, 401, 403, 404, 407, 422] {
            assert!(!super::retryable_updater_error(&Error::Network(format!(
                "Download request failed with status: {status}"
            ))));
        }
    }

    #[test]
    fn metadata_and_signature_shape_failures_never_retry() {
        assert!(!super::retryable_updater_error(&Error::ReleaseNotFound));
        let invalid_base64 = base64::engine::general_purpose::STANDARD
            .decode("***")
            .expect_err("invalid signature encoding");
        assert!(!super::retryable_updater_error(&Error::Base64(
            invalid_base64
        )));
    }
}
