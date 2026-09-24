//! Endpoint arithmetic for the model probe: where to send a request, and what
//! kind of model an id most likely names.
//!
//! Both are pure. Neither logs, and neither is allowed to invent a host.

use crate::domain::{ProbeModelKind, ProviderWireProtocol};

/// Relative paths, expressed without a leading version segment except where
/// the tool itself spells one into the route.
pub const MODELS_PATH: &str = "models";
pub const CHAT_PATH: &str = "chat/completions";
pub const RESPONSES_PATH: &str = "responses";
/// Claude Code's Anthropic SDK appends `/v1/messages` to `ANTHROPIC_BASE_URL`
/// whatever the base already ends with, so the version belongs to the route.
pub const MESSAGES_PATH: &str = "v1/messages";
pub const IMAGES_PATH: &str = "images/generations";

/// The version segment assumed when a base URL does not carry one.
const DEFAULT_VERSION: &str = "v1";
const GEMINI_VERSION: &str = "v1beta";

/// True when a path segment is a version root such as `v1`, `v2`, `v10` or
/// `v1beta`. Exact-segment matching is the point: a naive
/// `base.ends_with("/v1")` sends `https://host/v1beta` to
/// `https://host/v1beta/v1/models`.
fn is_version_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    if chars.next() != Some('v') {
        return false;
    }
    let rest: String = chars.collect();
    if rest.is_empty() {
        return false;
    }
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return false;
    }
    rest[digits.len()..].chars().all(|c| c.is_ascii_lowercase())
}

fn trimmed_base(base: &str) -> &str {
    base.trim_end_matches('/')
}

fn last_segment(base: &str) -> &str {
    trimmed_base(base).rsplit('/').next().unwrap_or_default()
}

/// Joins a relative endpoint onto a saved base URL.
///
/// Three shapes are handled, in this order:
/// 1. the base already ends with the relative path (the user pasted a complete
///    endpoint into the base URL field), so it is returned untouched;
/// 2. the base ends with a version segment, so the relative path is appended to
///    it directly;
/// 3. the base carries no version, so the default version is inserted.
pub fn join_endpoint(base: &str, relative: &str, version: &str) -> String {
    let base = trimmed_base(base);
    if base.ends_with(&format!("/{relative}")) {
        return base.to_string();
    }
    if is_version_segment(last_segment(base)) {
        return format!("{base}/{relative}");
    }
    format!("{base}/{version}/{relative}")
}

/// Joins a relative endpoint the way the tool does: plain concatenation,
/// exactly what Codex's `url_for_path` and Claude Code's Anthropic SDK
/// perform.
///
/// Step 1 of [`join_endpoint`] is kept — a base that already names the endpoint
/// is still honoured — because that is about what the user typed, not about
/// what the tool adds.
pub fn join_endpoint_verbatim(base: &str, relative: &str) -> String {
    let base = trimmed_base(base);
    if base.ends_with(&format!("/{relative}")) {
        return base.to_string();
    }
    format!("{base}/{relative}")
}

/// The other spelling of a base URL: with a version segment when it has none,
/// without when it has one.
///
/// This is the whole vocabulary of suggestion the probe is allowed. It is a
/// pure function of the saved address, so a suggestion can never name a host
/// the user did not already save, whatever the upstream service answers.
/// Returns `None` when there is nothing to swap — a bare origin cannot lose a
/// segment it does not have.
pub fn alternate_base_url(base: &str) -> Option<String> {
    let base = trimmed_base(base);
    if base.is_empty() {
        return None;
    }
    if is_version_segment(last_segment(base)) {
        let shortened = base.rsplit_once('/').map(|(head, _)| head)?;
        // Stop at the origin: `https://host` must not shrink to `https:/`.
        let parsed = url::Url::parse(shortened).ok()?;
        return parsed.host_str().is_some().then(|| shortened.to_string());
    }
    Some(format!("{base}/{DEFAULT_VERSION}"))
}

pub fn default_version_for(protocol: ProviderWireProtocol) -> &'static str {
    match protocol {
        ProviderWireProtocol::Gemini => GEMINI_VERSION,
        ProviderWireProtocol::OpenAi
        | ProviderWireProtocol::OpenAiResponses
        | ProviderWireProtocol::Anthropic => DEFAULT_VERSION,
    }
}

/// The catalogue is always addressed with a version segment, including for
/// Codex. It is not one of Codex's own routes — nothing in Codex ever calls
/// `/models` — so normalising it here is a convenience for finding model names,
/// not a claim about the address the tool will use.
pub fn catalog_url(base: &str, protocol: ProviderWireProtocol) -> String {
    join_endpoint(base, MODELS_PATH, default_version_for(protocol))
}

pub fn text_url(base: &str, protocol: ProviderWireProtocol, model: &str) -> String {
    let relative = match protocol {
        ProviderWireProtocol::OpenAi => CHAT_PATH,
        ProviderWireProtocol::OpenAiResponses => RESPONSES_PATH,
        ProviderWireProtocol::Anthropic => MESSAGES_PATH,
        // Gemini puts the model in the path and the verb in a suffix, so the
        // generic join cannot be reused.
        ProviderWireProtocol::Gemini => {
            let root = join_endpoint(base, MODELS_PATH, GEMINI_VERSION);
            return format!("{root}/{model}:generateContent");
        }
    };
    // The one request that must reproduce the tool's own arithmetic, including
    // when that arithmetic is "concatenate and hope".
    if protocol.normalizes_version_segment() {
        join_endpoint(base, relative, default_version_for(protocol))
    } else {
        join_endpoint_verbatim(base, relative)
    }
}

pub fn image_url(base: &str) -> String {
    join_endpoint(base, IMAGES_PATH, DEFAULT_VERSION)
}

/// The catalogue path retried when a relay mounts its API at the domain root
/// rather than under the saved base path. Returns `None` when the base already
/// is the origin, so the same request is never sent twice.
pub fn origin_catalog_url(base: &str, protocol: ProviderWireProtocol) -> Option<String> {
    let parsed = url::Url::parse(base).ok()?;
    if parsed.path().trim_matches('/').is_empty() {
        return None;
    }
    let origin = format!(
        "{}://{}",
        parsed.scheme(),
        parsed.host_str().map(|host| match parsed.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_string(),
        })?
    );
    Some(catalog_url(&origin, protocol))
}

/// Model ids that consume images, transcribe audio or produce vectors. They
/// are matched first so a name like `gpt-4-vision-preview` never lands in the
/// image-generation bucket just because it says "vision".
const NOT_IMAGE_MARKERS: &[&str] = &[
    "vision", "-vl", "vl-", "omni", "ocr", "embed", "rerank", "audio", "tts", "whisper",
    "realtime", "video",
];

const IMAGE_MARKERS: &[&str] = &[
    "image",
    "dall",
    "imagen",
    "cogview",
    "flux",
    "seedream",
    "kolors",
    "wanx",
    "stable-diffusion",
    "sdxl",
    "ideogram",
    "recraft",
    "nano-banana",
];

/// The fallback for catalogues that publish an id and nothing else, which is
/// most of them. A service that declares what its models output is believed
/// instead; see `wire::declared_kind`.
pub fn classify_model(id: &str) -> ProbeModelKind {
    let lower = id.to_ascii_lowercase();
    if NOT_IMAGE_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return ProbeModelKind::Text;
    }
    if IMAGE_MARKERS.iter().any(|marker| lower.contains(marker)) {
        return ProbeModelKind::Image;
    }
    ProbeModelKind::Text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_base_without_a_version_gets_the_default_one() {
        assert_eq!(
            catalog_url("https://api.example.test", ProviderWireProtocol::OpenAi),
            "https://api.example.test/v1/models"
        );
        assert_eq!(
            catalog_url("https://api.example.test/", ProviderWireProtocol::OpenAi),
            "https://api.example.test/v1/models"
        );
        assert_eq!(
            catalog_url(
                "https://relay.example.test/openai",
                ProviderWireProtocol::OpenAi
            ),
            "https://relay.example.test/openai/v1/models"
        );
    }

    #[test]
    fn a_base_that_already_carries_a_version_is_not_given_a_second_one() {
        assert_eq!(
            catalog_url("https://api.example.test/v1", ProviderWireProtocol::OpenAi),
            "https://api.example.test/v1/models"
        );
        assert_eq!(
            catalog_url("https://api.example.test/v1/", ProviderWireProtocol::OpenAi),
            "https://api.example.test/v1/models"
        );
    }

    #[test]
    fn version_detection_matches_whole_segments_only() {
        // The bug an `ends_with("/v1")` check cannot see.
        assert_eq!(
            catalog_url(
                "https://api.example.test/v1beta",
                ProviderWireProtocol::OpenAi
            ),
            "https://api.example.test/v1beta/models"
        );
        assert_eq!(
            catalog_url("https://api.example.test/v10", ProviderWireProtocol::OpenAi),
            "https://api.example.test/v10/models"
        );
        // Not versions.
        assert!(!is_version_segment("v"));
        assert!(!is_version_segment("vertex"));
        assert!(!is_version_segment("openai"));
        assert!(!is_version_segment("v1Beta"));
        assert!(is_version_segment("v1"));
        assert!(is_version_segment("v1beta"));
        assert!(is_version_segment("v10"));
    }

    #[test]
    fn a_pasted_complete_endpoint_is_left_alone() {
        assert_eq!(
            catalog_url(
                "https://api.example.test/v1/models",
                ProviderWireProtocol::OpenAi
            ),
            "https://api.example.test/v1/models"
        );
        assert_eq!(
            text_url(
                "https://api.example.test/v1/chat/completions",
                ProviderWireProtocol::OpenAi,
                "gpt-5.2"
            ),
            "https://api.example.test/v1/chat/completions"
        );
    }

    #[test]
    fn each_protocol_reaches_its_own_verb() {
        assert_eq!(
            text_url(
                "https://api.example.test",
                ProviderWireProtocol::OpenAi,
                "gpt-5.2"
            ),
            "https://api.example.test/v1/chat/completions"
        );
        assert_eq!(
            text_url(
                "https://api.example.test",
                ProviderWireProtocol::Anthropic,
                "claude-opus-5"
            ),
            "https://api.example.test/v1/messages"
        );
        assert_eq!(
            text_url(
                "https://generativelanguage.example.test",
                ProviderWireProtocol::Gemini,
                "gemini-3-pro"
            ),
            "https://generativelanguage.example.test/v1beta/models/gemini-3-pro:generateContent"
        );
        assert_eq!(
            catalog_url(
                "https://generativelanguage.example.test",
                ProviderWireProtocol::Gemini
            ),
            "https://generativelanguage.example.test/v1beta/models"
        );
        assert_eq!(
            image_url("https://api.example.test"),
            "https://api.example.test/v1/images/generations"
        );
    }

    #[test]
    fn the_origin_retry_only_exists_when_the_base_has_a_path() {
        assert_eq!(
            origin_catalog_url(
                "https://relay.example.test/openai",
                ProviderWireProtocol::OpenAi
            )
            .as_deref(),
            Some("https://relay.example.test/v1/models")
        );
        assert_eq!(
            origin_catalog_url("https://relay.example.test", ProviderWireProtocol::OpenAi),
            None
        );
        assert_eq!(
            origin_catalog_url("https://relay.example.test/", ProviderWireProtocol::OpenAi),
            None
        );
        assert_eq!(
            origin_catalog_url(
                "https://relay.example.test:8443/openai",
                ProviderWireProtocol::OpenAi
            )
            .as_deref(),
            Some("https://relay.example.test:8443/v1/models")
        );
    }

    #[test]
    fn image_consuming_models_are_never_classified_as_image_generators() {
        for id in [
            "gpt-4o",
            "gpt-4o-mini",
            "gpt-4-vision-preview",
            "qwen-vl-max",
            "qwen2.5-vl-72b",
            "gpt-4o-audio-preview",
            "text-embedding-3-large",
            "bge-reranker-v2-m3",
            "whisper-1",
            "sora-2",
            "claude-opus-5",
            "deepseek-r1",
        ] {
            assert_eq!(
                classify_model(id),
                ProbeModelKind::Text,
                "{id} should default to a text probe"
            );
        }
    }

    #[test]
    fn image_generators_are_recognised_by_name() {
        for id in [
            "gpt-image-2",
            "GPT-Image-1",
            "dall-e-3",
            "imagen-4.0-generate-001",
            "cogview-4",
            "flux.1-schnell",
            "doubao-seedream-4.0",
            "kolors-2",
            "wanx-v1",
            "stable-diffusion-3.5-large",
            "ideogram-v3",
            "gemini-3-pro-image",
        ] {
            assert_eq!(
                classify_model(id),
                ProbeModelKind::Image,
                "{id} should default to an image probe"
            );
        }
    }

    /// The reported defect for Claude Code: a base URL ending in `/v1` passed
    /// the test at `/v1/messages` while Claude Code itself asked
    /// `/v1/v1/messages` and got 404.
    #[test]
    fn claude_code_is_probed_exactly_where_claude_code_sends_it() {
        assert_eq!(
            text_url(
                "https://api.example.test/v1",
                ProviderWireProtocol::Anthropic,
                "claude-opus-5"
            ),
            "https://api.example.test/v1/v1/messages"
        );
        assert_eq!(
            text_url(
                "https://api.example.test/api/anthropic/",
                ProviderWireProtocol::Anthropic,
                "claude-opus-5"
            ),
            "https://api.example.test/api/anthropic/v1/messages"
        );
        // The catalogue is not a Claude Code route, so it is still normalised.
        assert_eq!(
            catalog_url(
                "https://api.example.test/v1",
                ProviderWireProtocol::Anthropic
            ),
            "https://api.example.test/v1/models"
        );
    }

    /// The reported defect, as an assertion: a Codex service saved without
    /// `/v1` must be probed at `/responses`, because that is where Codex sends
    /// it and where a path-allowlisted relay answers 403.
    #[test]
    fn codex_is_probed_exactly_where_codex_sends_it() {
        assert_eq!(
            text_url(
                "https://api.example.test",
                ProviderWireProtocol::OpenAiResponses,
                "gpt-5.2"
            ),
            "https://api.example.test/responses"
        );
        assert_eq!(
            text_url(
                "https://api.example.test/v1",
                ProviderWireProtocol::OpenAiResponses,
                "gpt-5.2"
            ),
            "https://api.example.test/v1/responses"
        );
        // A provider whose API is mounted somewhere else entirely keeps its
        // path untouched, which is exactly what Codex does with it.
        assert_eq!(
            text_url(
                "https://api.example.test/api/codex/backend-api/codex",
                ProviderWireProtocol::OpenAiResponses,
                "gpt-5.2"
            ),
            "https://api.example.test/api/codex/backend-api/codex/responses"
        );
        // The catalogue is not a Codex route, so it is still normalised.
        assert_eq!(
            catalog_url(
                "https://api.example.test",
                ProviderWireProtocol::OpenAiResponses
            ),
            "https://api.example.test/v1/models"
        );
    }

    #[test]
    fn the_alternative_base_url_swaps_the_version_segment_both_ways() {
        assert_eq!(
            alternate_base_url("https://api.example.test").as_deref(),
            Some("https://api.example.test/v1")
        );
        assert_eq!(
            alternate_base_url("https://api.example.test/v1").as_deref(),
            Some("https://api.example.test")
        );
        assert_eq!(
            alternate_base_url("https://api.example.test/v1/").as_deref(),
            Some("https://api.example.test")
        );
        // A deeper path is not a version segment, so the alternative appends.
        assert_eq!(
            alternate_base_url("https://api.example.test/openai").as_deref(),
            Some("https://api.example.test/openai/v1")
        );
        assert_eq!(alternate_base_url(""), None);
    }

    /// The security property the suggestion rests on: it is a pure rewrite of
    /// the saved address, so it can never name a different host however the
    /// upstream service answers.
    #[test]
    fn an_alternative_never_leaves_the_saved_origin() {
        for base in [
            "https://api.example.test",
            "https://api.example.test/v1",
            "https://api.example.test:8443/openai",
            "http://127.0.0.1:11434/v1",
            "https://api.example.test/v1beta",
        ] {
            let Some(alternate) = alternate_base_url(base) else {
                continue;
            };
            let saved = url::Url::parse(base).expect("saved base parses");
            let suggested = url::Url::parse(&alternate).expect("alternative parses");
            assert_eq!(saved.scheme(), suggested.scheme(), "{base}");
            assert_eq!(saved.host_str(), suggested.host_str(), "{base}");
            assert_eq!(saved.port(), suggested.port(), "{base}");
            assert_ne!(alternate, base, "{base} suggested itself");
        }
        // Nothing to shorten: an origin has no segment to drop, so no
        // suggestion is invented for it.
        assert_eq!(alternate_base_url("https://v1"), None);
    }
}
