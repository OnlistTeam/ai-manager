//! Endpoint arithmetic for the model probe: where to send a request, and what
//! kind of model an id most likely names.
//!
//! Both are pure. Neither logs, and neither is allowed to invent a host.

use crate::domain::{ProbeModelKind, ProviderWireProtocol};

/// Relative paths, always expressed without a leading version segment.
pub const MODELS_PATH: &str = "models";
pub const CHAT_PATH: &str = "chat/completions";
pub const MESSAGES_PATH: &str = "messages";
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

pub fn default_version_for(protocol: ProviderWireProtocol) -> &'static str {
    match protocol {
        ProviderWireProtocol::Gemini => GEMINI_VERSION,
        ProviderWireProtocol::OpenAi | ProviderWireProtocol::Anthropic => DEFAULT_VERSION,
    }
}

pub fn catalog_url(base: &str, protocol: ProviderWireProtocol) -> String {
    join_endpoint(base, MODELS_PATH, default_version_for(protocol))
}

pub fn text_url(base: &str, protocol: ProviderWireProtocol, model: &str) -> String {
    match protocol {
        ProviderWireProtocol::OpenAi => join_endpoint(base, CHAT_PATH, DEFAULT_VERSION),
        ProviderWireProtocol::Anthropic => join_endpoint(base, MESSAGES_PATH, DEFAULT_VERSION),
        // Gemini puts the model in the path and the verb in a suffix, so the
        // generic join cannot be reused.
        ProviderWireProtocol::Gemini => {
            let root = join_endpoint(base, MODELS_PATH, GEMINI_VERSION);
            format!("{root}/{model}:generateContent")
        }
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

/// A guess used only as the dialog's default. Most catalogues return an id and
/// nothing else, so there is no capability field to read.
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
}
