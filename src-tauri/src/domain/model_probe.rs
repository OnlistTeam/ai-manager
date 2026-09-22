//! Wire types for the model probe (ADR-0041).
//!
//! Unlike the reachability probe, this path deliberately sends the saved
//! credential and reads the response body, because a wrong key and a dead
//! address are indistinguishable otherwise. The compensating rule is that
//! nothing here may ever reach a log: the prompt and the reply are redacted
//! from every `Debug` rendering, and the address never crosses IPC at all.

use serde::{Deserialize, Serialize};

use super::tool::ToolId;

/// Prompts are a smoke test, not a conversation.
pub const MAX_PROBE_PROMPT_CHARS: usize = 200;
/// Enough to see that a model answered coherently; not a transcript.
pub const MAX_PROBE_REPLY_CHARS: usize = 2_000;
/// Base64 payload ceiling for one generated image.
pub const MAX_PROBE_IMAGE_BASE64_BYTES: usize = 8 * 1024 * 1024;
/// Aggregators routinely serve hundreds of models; a few serve absurd numbers.
pub const MAX_PROBE_MODELS: usize = 1_000;

/// The request dialect a tool's services speak.
///
/// Derived from a total `ToolId` table rather than a user choice, so a new tool
/// cannot compile without declaring one (spec §11: no ToolId conditionals in
/// the application layer or the renderer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderWireProtocol {
    OpenAi,
    /// OpenAI's Responses API, which Codex speaks and nothing else here does.
    ///
    /// Worth its own variant rather than a flag on `OpenAi` because it differs
    /// in the two things that decide whether a test tells the truth: the path
    /// (`responses`, not `chat/completions`) and the fact that Codex joins that
    /// path onto the saved base URL verbatim, inserting no version segment. A
    /// probe that normalised the address would pass on a base URL Codex itself
    /// cannot use.
    OpenAiResponses,
    Anthropic,
    Gemini,
}

impl ProviderWireProtocol {
    pub fn for_tool(tool: ToolId) -> Self {
        match tool {
            ToolId::ClaudeCode => Self::Anthropic,
            ToolId::GeminiCli => Self::Gemini,
            ToolId::Codex => Self::OpenAiResponses,
            ToolId::OpenCode
            | ToolId::GrokBuild
            | ToolId::OpenClaw
            | ToolId::Hermes
            | ToolId::Pi
            | ToolId::KimiCode
            | ToolId::DeepSeekDsh => Self::OpenAi,
        }
    }

    /// True for the dialects whose hosts also serve OpenAI's image route.
    ///
    /// This is a claim about the host, not about the tool: a relay that answers
    /// `chat/completions` or `responses` is an OpenAI-family host and usually
    /// serves `images/generations` too. The Anthropic and Gemini hosts do not,
    /// so the renderer hides the control instead of offering a button that is
    /// guaranteed to fail.
    pub fn supports_image_generation(self) -> bool {
        matches!(self, Self::OpenAi | Self::OpenAiResponses)
    }

    /// Whether a missing version segment may be supplied when joining a path.
    ///
    /// Only Codex answers no, and it is the reason this distinction exists: it
    /// concatenates `{base_url}/responses` literally, so a base URL without
    /// `/v1` reaches `/responses` and fails. Normalising here would hide
    /// exactly the failure the user needs to see.
    pub fn normalizes_version_segment(self) -> bool {
        !matches!(self, Self::OpenAiResponses)
    }
}

/// A guess, not a fact. Most catalogues return nothing but an id, so the
/// renderer presents this as an overridable default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProbeModelKind {
    Text,
    Image,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeModel {
    pub id: String,
    pub kind: ProbeModelKind,
}

/// Why an endpoint answered the catalogue request without a catalogue.
///
/// Carried on the successful shape rather than raised as an error: the dialog
/// still works with a typed model name, and the reason is what tells the user
/// their key is wrong rather than their address.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogRejection {
    pub status: u16,
    pub detail: String,
}

impl std::fmt::Debug for ModelCatalogRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelCatalogRejection")
            .field("status", &self.status)
            .field("detail", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalog {
    pub protocol: ProviderWireProtocol,
    pub models: Vec<ProbeModel>,
    /// True when the endpoint served more than `MAX_PROBE_MODELS` entries.
    pub truncated: bool,
    /// Present when the endpoint refused. `models` is then empty.
    pub rejection: Option<ModelCatalogRejection>,
}

/// What the renderer asks for. Input only: the prompt never travels back.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ModelProbeRequest {
    pub model: String,
    pub kind: ProbeModelKind,
    pub prompt: String,
}

impl std::fmt::Debug for ModelProbeRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelProbeRequest")
            .field("model", &self.model)
            .field("kind", &self.kind)
            .field("prompt", &"<redacted>")
            .finish()
    }
}

/// The part of the response the user is allowed to see, and the part that must
/// never be logged.
///
/// A service that answers "no" has still answered: a rejected key is the result
/// of the test, not a failure of it, so it is a reply rather than an error.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ModelProbeReply {
    Text {
        text: String,
    },
    Image {
        mime: String,
        base64: String,
    },
    /// A 2xx that carried no assistant content. Answered, but not usable.
    Empty,
    /// A non-2xx, carrying the upstream service's own explanation.
    Rejected {
        detail: String,
    },
    /// A 2xx that answered with a link instead of image data. The application
    /// CSP is `img-src 'self' data:`, so it cannot be shown, and fetching it
    /// here would point an arbitrary URL at the user's machine.
    ImageLinkOnly,
}

impl std::fmt::Debug for ModelProbeReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text { .. } => f.write_str("Text(<redacted>)"),
            Self::Image { mime, .. } => write!(f, "Image({mime}, <redacted>)"),
            Self::Empty => f.write_str("Empty"),
            Self::Rejected { .. } => f.write_str("Rejected(<redacted>)"),
            Self::ImageLinkOnly => f.write_str("ImageLinkOnly"),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProbeOutcome {
    pub model: String,
    pub latency_ms: u64,
    pub http_status: Option<u16>,
    pub reply: ModelProbeReply,
    /// A different spelling of the saved base URL that was **tried and found to
    /// work** after this one failed.
    ///
    /// Only ever derived from the saved base URL by adding or removing a
    /// version segment, never read from a redirect or a response body: a
    /// suggestion the upstream service could influence would be a way to walk
    /// the user onto someone else's host. It is also never set without a
    /// successful request behind it — an unverified guess is the one thing this
    /// field must not become.
    pub suggested_base_url: Option<String>,
}

impl std::fmt::Debug for ModelProbeOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelProbeOutcome")
            .field("model", &self.model)
            .field("latencyMs", &self.latency_ms)
            .field("httpStatus", &self.http_status)
            .field("reply", &self.reply)
            .field(
                "suggestedBaseUrl",
                &self.suggested_base_url.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_declares_a_wire_protocol() {
        for tool in ToolId::ALL {
            let protocol = ProviderWireProtocol::for_tool(tool);
            assert_eq!(
                protocol.supports_image_generation(),
                matches!(
                    protocol,
                    ProviderWireProtocol::OpenAi | ProviderWireProtocol::OpenAiResponses
                ),
                "{tool:?} image support drifted from its protocol"
            );
        }
        assert_eq!(
            ProviderWireProtocol::for_tool(ToolId::ClaudeCode),
            ProviderWireProtocol::Anthropic
        );
        assert_eq!(
            ProviderWireProtocol::for_tool(ToolId::GeminiCli),
            ProviderWireProtocol::Gemini
        );
    }

    /// The regression that produced this variant: a Codex service saved without
    /// `/v1` was reported as working, because the probe supplied the version
    /// segment Codex does not.
    #[test]
    fn only_codex_speaks_responses_and_only_codex_keeps_its_base_url_verbatim() {
        assert_eq!(
            ProviderWireProtocol::for_tool(ToolId::Codex),
            ProviderWireProtocol::OpenAiResponses
        );
        for tool in ToolId::ALL {
            let protocol = ProviderWireProtocol::for_tool(tool);
            assert_eq!(
                protocol == ProviderWireProtocol::OpenAiResponses,
                tool == ToolId::Codex,
                "{tool:?} claims the Responses dialect"
            );
            assert_eq!(
                protocol.normalizes_version_segment(),
                tool != ToolId::Codex,
                "{tool:?} disagrees with its dialect about version segments"
            );
        }
    }

    #[test]
    fn a_probe_request_never_shows_its_prompt_in_debug() {
        let request: ModelProbeRequest = serde_json::from_str(
            r#"{"model":"gpt-5.2","kind":"text","prompt":"my secret question"}"#,
        )
        .expect("deserialize probe request");

        let debug = format!("{request:?}");
        assert!(debug.contains("gpt-5.2"));
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("my secret question"));
    }

    #[test]
    fn a_reply_never_shows_its_content_in_debug() {
        let text = ModelProbeReply::Text {
            text: "the model said something".to_string(),
        };
        let image = ModelProbeReply::Image {
            mime: "image/png".to_string(),
            base64: "AAAABBBBCCCC".to_string(),
        };

        let text_debug = format!("{text:?}");
        assert_eq!(text_debug, "Text(<redacted>)");
        assert!(!text_debug.contains("the model said something"));

        let image_debug = format!("{image:?}");
        assert!(image_debug.contains("image/png"));
        assert!(!image_debug.contains("AAAABBBBCCCC"));

        let rejected = ModelProbeReply::Rejected {
            detail: "invalid api key for account 12345".to_string(),
        };
        let rejected_debug = format!("{rejected:?}");
        assert_eq!(rejected_debug, "Rejected(<redacted>)");
        assert!(!rejected_debug.contains("12345"));
    }

    #[test]
    fn an_outcome_debug_carries_diagnostics_but_no_content() {
        let outcome = ModelProbeOutcome {
            model: "gpt-5.2".to_string(),
            latency_ms: 1_800,
            http_status: Some(200),
            reply: ModelProbeReply::Text {
                text: "hello there".to_string(),
            },
            suggested_base_url: Some("https://api.example.test/v1".to_string()),
        };

        let debug = format!("{outcome:?}");
        assert!(debug.contains("gpt-5.2"));
        assert!(debug.contains("1800"));
        assert!(debug.contains("200"));
        assert!(!debug.contains("hello there"));
        assert!(!debug.contains("api.example.test"));
    }

    #[test]
    fn reply_wire_format_is_tagged_by_kind() {
        assert_eq!(
            serde_json::to_string(&ModelProbeReply::Text {
                text: "hi".to_string()
            })
            .unwrap(),
            r#"{"kind":"text","text":"hi"}"#
        );
        assert_eq!(
            serde_json::to_string(&ModelProbeReply::Image {
                mime: "image/png".to_string(),
                base64: "AAAA".to_string()
            })
            .unwrap(),
            r#"{"kind":"image","mime":"image/png","base64":"AAAA"}"#
        );
        assert_eq!(
            serde_json::to_string(&ModelProbeReply::Empty).unwrap(),
            r#"{"kind":"empty"}"#
        );
        assert_eq!(
            serde_json::to_string(&ModelProbeReply::Rejected {
                detail: "invalid api key".to_string()
            })
            .unwrap(),
            r#"{"kind":"rejected","detail":"invalid api key"}"#
        );
        assert_eq!(
            serde_json::to_string(&ModelProbeReply::ImageLinkOnly).unwrap(),
            r#"{"kind":"imageLinkOnly"}"#
        );
    }

    #[test]
    fn catalog_wire_format_is_stable() {
        let catalog = ModelCatalog {
            protocol: ProviderWireProtocol::OpenAi,
            models: vec![ProbeModel {
                id: "gpt-image-2".to_string(),
                kind: ProbeModelKind::Image,
            }],
            truncated: false,
            rejection: None,
        };

        assert_eq!(
            serde_json::to_string(&catalog).unwrap(),
            r#"{"protocol":"openAi","models":[{"id":"gpt-image-2","kind":"image"}],"truncated":false,"rejection":null}"#
        );
    }

    #[test]
    fn a_catalog_rejection_never_shows_its_detail_in_debug() {
        let rejection = ModelCatalogRejection {
            status: 401,
            detail: "key sk-live-123 is not valid".to_string(),
        };
        let debug = format!("{rejection:?}");
        assert!(debug.contains("401"));
        assert!(!debug.contains("sk-live-123"));
    }
}
