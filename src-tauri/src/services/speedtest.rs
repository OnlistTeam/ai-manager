use futures::future::join_all;
use reqwest::{Client, Url};
use serde::Serialize;
use std::error::Error as _;
use std::fmt;
use std::time::{Duration, Instant};

use crate::error::AppError;

const DEFAULT_TIMEOUT_SECS: u64 = 8;
const MAX_TIMEOUT_SECS: u64 = 30;
const MIN_TIMEOUT_SECS: u64 = 2;
const MAX_URL_BYTES: usize = 2_048;

/// Stable probe failures. Never store or return `reqwest`'s raw error text:
/// it can contain a URL and is not suitable as product copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EndpointTestFailure {
    InvalidUrl,
    Timeout,
    Dns,
    Tls,
    Connection,
    Request,
}

/// Endpoint latency measurement result
#[derive(Clone, Serialize)]
pub struct EndpointLatency {
    pub url: String,
    pub latency: Option<u128>,
    pub status: Option<u16>,
    pub error: Option<EndpointTestFailure>,
}

impl fmt::Debug for EndpointLatency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EndpointLatency")
            .field("url", &"<redacted>")
            .field("latency", &self.latency)
            .field("status", &self.status)
            .field("error", &self.error)
            .finish()
    }
}

/// Network speed-test business logic
pub struct SpeedtestService;

impl SpeedtestService {
    /// Measure the response latency of a set of endpoints.
    pub async fn test_endpoints(
        urls: Vec<String>,
        timeout_secs: Option<u64>,
    ) -> Result<Vec<EndpointLatency>, AppError> {
        let timeout = Self::sanitize_timeout(timeout_secs);
        let (client, request_timeout) = Self::build_client(timeout)?;
        Ok(Self::test_endpoints_with_client(urls, client, request_timeout).await)
    }

    async fn test_endpoints_with_client(
        urls: Vec<String>,
        client: Client,
        request_timeout: Duration,
    ) -> Vec<EndpointLatency> {
        if urls.is_empty() {
            return vec![];
        }

        let mut results: Vec<Option<EndpointLatency>> = vec![None; urls.len()];
        let mut valid_targets = Vec::new();

        for (idx, raw_url) in urls.into_iter().enumerate() {
            let trimmed = raw_url.trim().to_string();

            match Self::validated_url(&trimmed) {
                Ok(parsed_url) => valid_targets.push((idx, trimmed, parsed_url)),
                Err(()) => {
                    results[idx] = Some(EndpointLatency {
                        url: trimmed,
                        latency: None,
                        status: None,
                        error: Some(EndpointTestFailure::InvalidUrl),
                    });
                }
            }
        }

        if valid_targets.is_empty() {
            return results.into_iter().flatten().collect::<Vec<_>>();
        }

        let tasks = valid_targets.into_iter().map(|(idx, trimmed, parsed_url)| {
            let client = client.clone();
            async move {
                // Warm-up request first; the result is ignored, it only reuses the connection / avoids the first-packet penalty.
                let _ = client
                    .get(parsed_url.clone())
                    .timeout(request_timeout)
                    .send()
                    .await;

                // The second request is timed and returned as the result.
                let start = Instant::now();
                let latency = match client.get(parsed_url).timeout(request_timeout).send().await {
                    Ok(resp) => EndpointLatency {
                        url: trimmed,
                        latency: Some(start.elapsed().as_millis()),
                        status: Some(resp.status().as_u16()),
                        error: None,
                    },
                    Err(err) => {
                        let status = err.status().map(|s| s.as_u16());

                        EndpointLatency {
                            url: trimmed,
                            latency: None,
                            status,
                            error: Some(Self::classify_request_error(&err)),
                        }
                    }
                };

                (idx, latency)
            }
        });

        for (idx, latency) in join_all(tasks).await {
            results[idx] = Some(latency);
        }

        results.into_iter().flatten().collect::<Vec<_>>()
    }

    fn validated_url(raw: &str) -> Result<Url, ()> {
        if raw.is_empty() || raw.len() > MAX_URL_BYTES {
            return Err(());
        }
        let parsed = Url::parse(raw).map_err(|_| ())?;
        let safe = matches!(parsed.scheme(), "http" | "https")
            && parsed.host_str().is_some()
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.query().is_none()
            && parsed.fragment().is_none();
        safe.then_some(parsed).ok_or(())
    }

    fn classify_request_error(error: &reqwest::Error) -> EndpointTestFailure {
        if error.is_timeout() {
            return EndpointTestFailure::Timeout;
        }

        let mut source = error.source();
        while let Some(cause) = source {
            let message = cause.to_string().to_ascii_lowercase();
            if ["tls", "ssl", "certificate", "cert verify"]
                .iter()
                .any(|marker| message.contains(marker))
            {
                return EndpointTestFailure::Tls;
            }
            if [
                "dns error",
                "failed to lookup address",
                "name or service not known",
                "nodename nor servname",
                "name resolution",
                "no such host",
            ]
            .iter()
            .any(|marker| message.contains(marker))
            {
                return EndpointTestFailure::Dns;
            }
            source = cause.source();
        }

        if error.is_connect() {
            EndpointTestFailure::Connection
        } else {
            EndpointTestFailure::Request
        }
    }

    fn build_client(timeout_secs: u64) -> Result<(Client, Duration), AppError> {
        // Use the global HTTP client (proxy configuration already applied)
        // Return the timeout Duration for per-request use
        let timeout = Duration::from_secs(timeout_secs);
        Ok((crate::proxy::http_client::get(), timeout))
    }

    fn sanitize_timeout(timeout_secs: Option<u64>) -> u64 {
        let secs = timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
        secs.clamp(MIN_TIMEOUT_SECS, MAX_TIMEOUT_SECS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{ErrorKind, Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn local_endpoint() -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local endpoint");
        listener
            .set_nonblocking(true)
            .expect("make listener nonblocking");
        let address = listener.local_addr().expect("local address");
        let server = thread::spawn(move || {
            // The deadline only keeps a failed test from stranding this thread.
            // The happy path answers both requests in milliseconds and never
            // reaches it, so it is not a timing assumption the assertions rest on.
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut requests = Vec::new();
            while requests.len() < 2 && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // Winsock hands back an accepted socket that inherits the
                        // listener's non-blocking mode, so without this the reads below
                        // return WouldBlock at once and the response races the request.
                        stream
                            .set_nonblocking(false)
                            .expect("blocking accepted stream");
                        stream
                            .set_read_timeout(Some(Duration::from_secs(5)))
                            .expect("set read timeout");
                        // One `read` returns one TCP segment, which is not
                        // necessarily the whole request head on a loaded
                        // machine. Read until the blank line that ends it.
                        let mut request = Vec::new();
                        let mut chunk = [0_u8; 1_024];
                        loop {
                            match stream.read(&mut chunk) {
                                Ok(0) => break,
                                Ok(size) => {
                                    request.extend_from_slice(&chunk[..size]);
                                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                                        break;
                                    }
                                }
                                // Answer what arrived instead of panicking. A
                                // dead server thread never accepts the measured
                                // request, which then fails on a timeout far
                                // from the actual cause.
                                Err(_) => break,
                            }
                        }
                        requests.push(String::from_utf8_lossy(&request).to_string());
                        let _ = stream.write_all(
                            b"HTTP/1.1 204 No Content\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
                        );
                        let _ = stream.flush();
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("accept request: {error}"),
                }
            }
            requests
        });
        (format!("http://{address}/health"), server)
    }

    #[test]
    fn sanitize_timeout_clamps_values() {
        assert_eq!(
            SpeedtestService::sanitize_timeout(Some(1)),
            MIN_TIMEOUT_SECS
        );
        assert_eq!(
            SpeedtestService::sanitize_timeout(Some(999)),
            MAX_TIMEOUT_SECS
        );
        assert_eq!(
            SpeedtestService::sanitize_timeout(Some(10)),
            10.clamp(MIN_TIMEOUT_SECS, MAX_TIMEOUT_SECS)
        );
        assert_eq!(
            SpeedtestService::sanitize_timeout(None),
            DEFAULT_TIMEOUT_SECS
        );
    }

    #[test]
    fn test_endpoints_handles_empty_list() {
        let result =
            tauri::async_runtime::block_on(SpeedtestService::test_endpoints(Vec::new(), Some(5)))
                .expect("empty list should succeed");
        assert!(result.is_empty());
    }

    #[test]
    fn test_endpoints_rejects_unsafe_urls_without_display_strings() {
        let result = tauri::async_runtime::block_on(SpeedtestService::test_endpoints(
            vec![
                "not a url".into(),
                "".into(),
                "ftp://example.test/file".into(),
                "https://user:password@example.test/v1".into(),
                "https://example.test/v1?token=secret".into(),
                "https://example.test/v1#fragment".into(),
            ],
            None,
        ))
        .expect("invalid inputs should still succeed");

        assert_eq!(result.len(), 6);
        assert!(result.iter().all(|item| {
            item.latency.is_none()
                && item.status.is_none()
                && item.error == Some(EndpointTestFailure::InvalidUrl)
        }));
        assert_eq!(
            serde_json::to_string(&result[0].error).unwrap(),
            r#""invalidUrl""#
        );
    }

    #[test]
    fn batch_probe_preserves_order_and_sends_no_auth_headers() {
        let (first_url, first_server) = local_endpoint();
        let (second_url, second_server) = local_endpoint();
        let client = Client::builder().no_proxy().build().expect("test client");

        let result = tauri::async_runtime::block_on(SpeedtestService::test_endpoints_with_client(
            vec![first_url.clone(), second_url.clone()],
            client,
            Duration::from_secs(2),
        ));

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].url, first_url);
        assert_eq!(result[1].url, second_url);
        assert!(result
            .iter()
            .all(|item| item.status == Some(204) && item.error.is_none()));

        let first_requests = first_server.join().expect("first server");
        let second_requests = second_server.join().expect("second server");
        assert_eq!(first_requests.len(), 2, "warm-up plus measured request");
        assert_eq!(second_requests.len(), 2, "warm-up plus measured request");
        let requests = first_requests.into_iter().chain(second_requests);
        for request in requests {
            let lower = request.to_ascii_lowercase();
            assert!(!lower.contains("authorization:"));
            assert!(!lower.contains("x-api-key:"));
            assert!(!lower.contains("api-key:"));
        }
    }

    #[test]
    fn debug_output_never_contains_the_endpoint_url() {
        let result = EndpointLatency {
            url: "https://user:secret@example.test/v1?token=hidden".to_string(),
            latency: None,
            status: None,
            error: Some(EndpointTestFailure::InvalidUrl),
        };
        let debug = format!("{result:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("example.test"));
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("hidden"));
    }
}
