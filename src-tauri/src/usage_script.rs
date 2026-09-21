use rquickjs::{Context, Function, Runtime};
use serde_json::Value;
use std::collections::HashMap;
use url::{Host, Url};

use crate::error::AppError;

/// Execute a usage-query script
pub async fn execute_usage_script(
    script_code: &str,
    api_key: &str,
    base_url: &str,
    timeout_secs: u64,
    access_token: Option<&str>,
    user_id: Option<&str>,
    template_type: Option<&str>,
) -> Result<Value, AppError> {
    // Detect custom template mode
    // Prefer the template_type passed from the frontend
    let is_custom_template = template_type.map(|t| t == "custom").unwrap_or(false);

    // 1. Substitute template variables so no sensitive information leaks
    let script_with_vars =
        build_script_with_vars(script_code, api_key, base_url, access_token, user_id);

    // 2. Validate base_url safety (only when a base_url was provided)
    // In custom template mode the user may skip template variables and write the full URL in the script
    if should_validate_base_url(base_url, is_custom_template) {
        validate_base_url(base_url)?;
    }

    // 3. Extract the request config in its own scope (so Runtime/Context are dropped before the await)
    // Maximum execution time (seconds) allowed for a usage script. Scripts come from
    // untrusted sources (deeplink, synced imports), so their CPU / memory / stack usage
    // must be capped to stop a malicious or buggy script from hanging the whole backend.
    const USAGE_SCRIPT_TIMEOUT_SECS: u64 = 5;
    // 16 MiB is plenty for a script that only builds the request config / extractor.
    const USAGE_SCRIPT_MEMORY_LIMIT_BYTES: usize = 16 * 1024 * 1024;

    /// Create a constrained QuickJS runtime: limited memory and stack, plus an execution-time interrupt handler.
    fn create_script_runtime() -> Result<Runtime, AppError> {
        let runtime = Runtime::new().map_err(|e| {
            AppError::localized(
                "usage_script.runtime_create_failed",
                format!("Failed to create JS runtime: {e}"),
            )
        })?;

        // Memory and stack limits must be set before eval.
        runtime.set_memory_limit(USAGE_SCRIPT_MEMORY_LIMIT_BYTES);
        // The 256 KiB default of set_max_stack_size is enough; request it explicitly for consistency.
        runtime.set_max_stack_size(256 * 1024);

        // Time-slice interrupt handler: every interpreter loop checks for a timeout and throws an uncatchable exception when it expires.
        let deadline = std::time::Instant::now()
            .checked_add(std::time::Duration::from_secs(USAGE_SCRIPT_TIMEOUT_SECS))
            .ok_or_else(|| {
                AppError::localized(
                    "usage_script.invalid_timeout",
                    "Unable to compute script execution deadline",
                )
            })?;
        runtime.set_interrupt_handler(Some(Box::new(move || std::time::Instant::now() > deadline)));

        Ok(runtime)
    }

    let request_config = {
        let runtime = create_script_runtime()?;
        let context = Context::full(&runtime).map_err(|e| {
            AppError::localized(
                "usage_script.context_create_failed",
                format!("Failed to create JS context: {e}"),
            )
        })?;

        context.with(|ctx| {
            // Run the user code to get the config object
            let config: rquickjs::Object = ctx.eval(script_with_vars.clone()).map_err(|e| {
                AppError::localized(
                    "usage_script.config_parse_failed",
                    format!("Failed to parse config: {e}"),
                )
            })?;

            // Extract the request config
            let request: rquickjs::Object = config.get("request").map_err(|e| {
                AppError::localized(
                    "usage_script.request_missing",
                    format!("Missing request config: {e}"),
                )
            })?;

            // Convert request into a JSON string
            let request_json: String = ctx
                .json_stringify(request)
                .map_err(|e| {
                    AppError::localized(
                        "usage_script.request_serialize_failed",
                        format!("Failed to serialize request: {e}"),
                    )
                })?
                .ok_or_else(|| {
                    AppError::localized(
                        "usage_script.serialize_none",
                        "Serialization returned None",
                    )
                })?
                .get()
                .map_err(|e| {
                    AppError::localized(
                        "usage_script.get_string_failed",
                        format!("Failed to get string: {e}"),
                    )
                })?;

            Ok::<_, AppError>(request_json)
        })?
    }; // Runtime and Context are dropped here

    // 4. Parse the request config
    let request: RequestConfig = serde_json::from_str(&request_config).map_err(|e| {
        AppError::localized(
            "usage_script.request_format_invalid",
            format!("Invalid request config format: {e}"),
        )
    })?;

    // 5. Validate the request URL (HTTPS enforced + same-origin check)
    validate_request_url(&request.url, base_url, is_custom_template)?;

    // 6. Send the HTTP request
    let response_data = send_http_request(&request, timeout_secs).await?;

    // 7. Run the extractor in its own scope (so Runtime/Context are dropped before the function returns)
    let result: Value = {
        let runtime = create_script_runtime()?;
        let context = Context::full(&runtime).map_err(|e| {
            AppError::localized(
                "usage_script.context_create_failed",
                format!("Failed to create JS context: {e}"),
            )
        })?;

        context.with(|ctx| {
            // Eval again to get the config object
            let config: rquickjs::Object = ctx.eval(script_with_vars.clone()).map_err(|e| {
                AppError::localized(
                    "usage_script.config_reparse_failed",
                    format!("Failed to re-parse config: {e}"),
                )
            })?;

            // Extract the extractor function
            let extractor: Function = config.get("extractor").map_err(|e| {
                AppError::localized(
                    "usage_script.extractor_missing",
                    format!("Missing extractor function: {e}"),
                )
            })?;

            // Convert the response data into a JS value
            let response_js: rquickjs::Value =
                ctx.json_parse(response_data.as_str()).map_err(|e| {
                    AppError::localized(
                        "usage_script.response_parse_failed",
                        format!("Failed to parse response JSON: {e}"),
                    )
                })?;

            // Call extractor(response)
            let result_js: rquickjs::Value = extractor.call((response_js,)).map_err(|e| {
                AppError::localized(
                    "usage_script.extractor_exec_failed",
                    format!("Failed to run extractor: {e}"),
                )
            })?;

            // Convert to a JSON string
            let result_json: String = ctx
                .json_stringify(result_js)
                .map_err(|e| {
                    AppError::localized(
                        "usage_script.result_serialize_failed",
                        format!("Failed to serialize result: {e}"),
                    )
                })?
                .ok_or_else(|| {
                    AppError::localized(
                        "usage_script.serialize_none",
                        "Serialization returned None",
                    )
                })?
                .get()
                .map_err(|e| {
                    AppError::localized(
                        "usage_script.get_string_failed",
                        format!("Failed to get string: {e}"),
                    )
                })?;

            // Parse into serde_json::Value
            serde_json::from_str(&result_json).map_err(|e| {
                AppError::localized(
                    "usage_script.json_parse_failed",
                    format!("Failed to parse JSON: {e}"),
                )
            })
        })?
    }; // Runtime and Context are dropped here

    // 8. Validate the return value format
    validate_result(&result)?;

    Ok(result)
}

/// Request config structure
#[derive(Debug, serde::Deserialize)]
struct RequestConfig {
    url: String,
    method: String,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    body: Option<String>,
}

/// Send the HTTP request
async fn send_http_request(config: &RequestConfig, timeout_secs: u64) -> Result<String, AppError> {
    // Use the global HTTP client (proxy config already applied)
    let client = crate::proxy::http_client::get();
    // Clamp the timeout so a broken config cannot block for a long time (min 2s, max 30s)
    let request_timeout = std::time::Duration::from_secs(timeout_secs.clamp(2, 30));

    // Validate the HTTP method strictly; an invalid value must not fall back to GET
    let method: reqwest::Method = config.method.parse().map_err(|_| {
        AppError::localized(
            "usage_script.invalid_http_method",
            format!("Unsupported HTTP method: {}", config.method),
        )
    })?;

    let mut req = client
        .request(method.clone(), &config.url)
        .timeout(request_timeout);

    // Add the request headers
    for (k, v) in &config.headers {
        req = req.header(k, v);
    }

    // Add the request body
    if let Some(body) = &config.body {
        req = req.body(body.clone());
    }

    // Send the request
    let resp = req.send().await.map_err(|e| {
        AppError::localized(
            "usage_script.request_failed",
            format!("Request failed: {e}"),
        )
    })?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| {
        AppError::localized(
            "usage_script.read_response_failed",
            format!("Failed to read response: {e}"),
        )
    })?;

    if !status.is_success() {
        let preview = if text.len() > 200 {
            let mut safe_cut = 200usize;
            while !text.is_char_boundary(safe_cut) {
                safe_cut = safe_cut.saturating_sub(1);
            }
            format!("{}...", &text[..safe_cut])
        } else {
            text.clone()
        };
        return Err(AppError::localized(
            "usage_script.http_error",
            format!("HTTP {status} : {preview}"),
        ));
    }

    Ok(text)
}

/// Validate the script return value (single object or array)
fn validate_result(result: &Value) -> Result<(), AppError> {
    // For an array, validate every element
    if let Some(arr) = result.as_array() {
        if arr.is_empty() {
            return Err(AppError::localized(
                "usage_script.empty_array",
                "Script returned empty array",
            ));
        }
        for (idx, item) in arr.iter().enumerate() {
            validate_single_usage(item).map_err(|e| {
                AppError::localized(
                    "usage_script.array_validation_failed",
                    format!("Validation failed at index [{idx}]: {e}"),
                )
            })?;
        }
        return Ok(());
    }

    // For a single object, validate it directly (backward compatible)
    validate_single_usage(result)
}

/// Validate a single usage-data object
fn validate_single_usage(result: &Value) -> Result<(), AppError> {
    let obj = result.as_object().ok_or_else(|| {
        AppError::localized(
            "usage_script.must_return_object",
            "Script must return an object or an array of objects",
        )
    })?;

    // Every field is optional; only type checks are performed
    if obj.contains_key("isValid")
        && !result["isValid"].is_null()
        && !result["isValid"].is_boolean()
    {
        return Err(AppError::localized(
            "usage_script.isvalid_type_error",
            "isValid must be a boolean or null",
        ));
    }
    if obj.contains_key("invalidMessage")
        && !result["invalidMessage"].is_null()
        && !result["invalidMessage"].is_string()
    {
        return Err(AppError::localized(
            "usage_script.invalidmessage_type_error",
            "invalidMessage must be a string or null",
        ));
    }
    if obj.contains_key("remaining")
        && !result["remaining"].is_null()
        && !result["remaining"].is_number()
    {
        return Err(AppError::localized(
            "usage_script.remaining_type_error",
            "remaining must be a number or null",
        ));
    }
    if obj.contains_key("unit") && !result["unit"].is_null() && !result["unit"].is_string() {
        return Err(AppError::localized(
            "usage_script.unit_type_error",
            "unit must be a string or null",
        ));
    }
    if obj.contains_key("total") && !result["total"].is_null() && !result["total"].is_number() {
        return Err(AppError::localized(
            "usage_script.total_type_error",
            "total must be a number or null",
        ));
    }
    if obj.contains_key("used") && !result["used"].is_null() && !result["used"].is_number() {
        return Err(AppError::localized(
            "usage_script.used_type_error",
            "used must be a number or null",
        ));
    }
    if obj.contains_key("planName")
        && !result["planName"].is_null()
        && !result["planName"].is_string()
    {
        return Err(AppError::localized(
            "usage_script.planname_type_error",
            "planName must be a string or null",
        ));
    }
    if obj.contains_key("extra") && !result["extra"].is_null() && !result["extra"].is_string() {
        return Err(AppError::localized(
            "usage_script.extra_type_error",
            "extra must be a string or null",
        ));
    }

    Ok(())
}

/// Build the script with variables substituted, staying compatible with older scripts
fn build_script_with_vars(
    script_code: &str,
    api_key: &str,
    base_url: &str,
    access_token: Option<&str>,
    user_id: Option<&str>,
) -> String {
    let mut replaced = script_code
        .replace("{{apiKey}}", api_key)
        .replace("{{baseUrl}}", base_url);

    if let Some(token) = access_token {
        replaced = replaced.replace("{{accessToken}}", token);
    }
    if let Some(uid) = user_id {
        replaced = replaced.replace("{{userId}}", uid);
    }

    replaced
}

/// Validate the basic safety of base_url
fn validate_base_url(base_url: &str) -> Result<(), AppError> {
    if base_url.is_empty() {
        return Err(AppError::localized(
            "usage_script.base_url_empty",
            "base_url must not be empty",
        ));
    }

    // Parse the URL
    let parsed_url = Url::parse(base_url).map_err(|e| {
        AppError::localized(
            "usage_script.base_url_invalid",
            format!("Invalid base_url: {e}"),
        )
    })?;

    let is_loopback = is_loopback_host(&parsed_url);

    // Must be HTTPS (localhost is allowed for development)
    if parsed_url.scheme() != "https" && !is_loopback {
        return Err(AppError::localized(
            "usage_script.base_url_https_required",
            "base_url must use the HTTPS protocol (except for localhost)",
        ));
    }

    // Check that the host name is valid
    let hostname = parsed_url.host_str().ok_or_else(|| {
        AppError::localized(
            "usage_script.base_url_hostname_missing",
            "base_url must contain a valid host name",
        )
    })?;

    // Basic host name format check
    if hostname.is_empty() {
        return Err(AppError::localized(
            "usage_script.base_url_hostname_empty",
            "base_url host name must not be empty",
        ));
    }

    Ok(())
}

fn should_validate_base_url(base_url: &str, is_custom_template: bool) -> bool {
    !base_url.is_empty() && !is_custom_template
}

/// Validate that the request URL is safe (HTTPS enforced + same-origin check)
fn validate_request_url(
    request_url: &str,
    base_url: &str,
    is_custom_template: bool,
) -> Result<(), AppError> {
    // Parse the request URL
    let parsed_request = Url::parse(request_url).map_err(|e| {
        AppError::localized(
            "usage_script.request_url_invalid",
            format!("Invalid request URL: {e}"),
        )
    })?;

    let is_request_loopback = is_loopback_host(&parsed_request);

    // Must use HTTPS (localhost is allowed for development)
    // In custom template mode the user decides whether to use HTTP (and accepts the risk)
    if !is_custom_template && parsed_request.scheme() != "https" && !is_request_loopback {
        return Err(AppError::localized(
            "usage_script.request_https_required",
            "Request URL must use the HTTPS protocol (except for localhost)",
        ));
    }

    // When a non-empty base_url was provided, run the same-origin check
    // In custom template mode the user may call any HTTPS domain, so the check is skipped
    if !base_url.is_empty() && !is_custom_template {
        // Parse the base URL
        let parsed_base = Url::parse(base_url).map_err(|e| {
            AppError::localized(
                "usage_script.base_url_invalid",
                format!("Invalid base_url: {e}"),
            )
        })?;

        // Core safety check: must be same-origin with base_url (same host and port)
        if parsed_request.host_str() != parsed_base.host_str() {
            return Err(AppError::localized(
                "usage_script.request_host_mismatch",
                format!(
                    "Request host {} must match base_url host {} (same-origin required)",
                    parsed_request.host_str().unwrap_or("unknown"),
                    parsed_base.host_str().unwrap_or("unknown")
                ),
            ));
        }

        // Check that the ports match (taking default ports into account)
        // port_or_known_default() handles the default ports automatically (http->80, https->443)
        match (
            parsed_request.port_or_known_default(),
            parsed_base.port_or_known_default(),
        ) {
            (Some(request_port), Some(base_port)) if request_port == base_port => {
                // Ports match, continue
            }
            (Some(request_port), Some(base_port)) => {
                return Err(AppError::localized(
                    "usage_script.request_port_mismatch",
                    format!("Request port {request_port} must match base_url port {base_port}"),
                ));
            }
            _ => {
                // Should not happen in practice, since port_or_known_default() always returns Some
                return Err(AppError::localized(
                    "usage_script.request_port_unknown",
                    "Unable to determine port number",
                ));
            }
        }
    }

    Ok(())
}

/// Decide whether a URL points at this machine (localhost / loopback)
fn is_loopback_host(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_https_bypass_prevention() {
        // HTTP on a non-local host must be rejected
        let result = validate_base_url("http://127.0.0.1.evil.com/api");
        assert!(
            result.is_err(),
            "Should reject HTTP for non-localhost domains"
        );
    }

    #[test]
    fn test_custom_template_allows_http_lan_request_with_different_base_url() {
        assert!(
            !should_validate_base_url("http://10.37.192.156:8090/anthropic", true),
            "Custom scripts should not validate an unused provider base_url fallback"
        );

        let result = validate_request_url(
            "http://10.37.192.156:18344/user/balance",
            "http://10.37.192.156:8090/anthropic",
            true,
        );
        assert!(
            result.is_ok(),
            "Custom usage scripts should be able to call an explicit HTTP quota endpoint"
        );
    }

    #[test]
    fn test_port_comparison() {
        // Check that the port comparison handles default and explicit ports correctly

        // Test cases: (base_url, request_url, should_match)
        let test_cases = vec![
            // HTTPS default port cases
            (
                "https://api.example.com",
                "https://api.example.com/v1/test",
                true,
            ),
            (
                "https://api.example.com",
                "https://api.example.com:443/v1/test",
                true,
            ),
            (
                "https://api.example.com:443",
                "https://api.example.com/v1/test",
                true,
            ),
            (
                "https://api.example.com:443",
                "https://api.example.com:443/v1/test",
                true,
            ),
            // Port mismatch cases
            (
                "https://api.example.com",
                "https://api.example.com:8443/v1/test",
                false,
            ),
            (
                "https://api.example.com:443",
                "https://api.example.com:8443/v1/test",
                false,
            ),
        ];

        for (base_url, request_url, should_match) in test_cases {
            let result = validate_request_url(request_url, base_url, false);

            if should_match {
                assert!(
                    result.is_ok(),
                    "URL that should match was rejected: base_url={}, request_url={}, error={}",
                    base_url,
                    request_url,
                    result.unwrap_err()
                );
            } else {
                assert!(
                    result.is_err(),
                    "URL that should not match was allowed: base_url={}, request_url={}",
                    base_url,
                    request_url
                );
            }
        }
    }

    #[test]
    fn infinite_loop_usage_script_is_interrupted_before_blocking_the_backend() {
        // Usage scripts come from untrusted input (deeplinks / synced DB rows), so CPU time
        // must be capped; otherwise `while(true)` hangs the execution thread (DoS).
        let script = r#"
            (function(){
                while (true) { Math.sqrt(Math.random()); }
            })();
            ({ request: { url: "https://example.com", method: "GET" } })
        "#;

        let start = std::time::Instant::now();
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("tokio runtime for test")
            .block_on(execute_usage_script(
                script,
                "sk-test",
                "https://api.example.com",
                30,
                None,
                None,
                None,
            ));
        let elapsed = start.elapsed();

        assert!(
            result.is_err(),
            "infinite loop script must be rejected, got: {result:?}"
        );
        // Must be clearly shorter than waiting forever; leave slack for CI jitter but stay well below the 30s network timeout.
        assert!(
            elapsed < std::time::Duration::from_secs(15),
            "interruption took too long: {elapsed:?}"
        );
    }
}
