//! Product-owned deep link model and parser (ADR-0029).
//!
//! One parser serves both paths. The only difference between them is the
//! credential policy: a link the operating system handed us as argv may not
//! carry a credential, a link the user pasted from their own clipboard may.
//!
//! This module is pure. It does not know which tools are installed, it never
//! writes anything, and it never logs the URL. Resolving the upstream `app`
//! token to a `ToolId` and deciding whether that tool can accept the import
//! belongs to the application layer.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{AppError, ErrorCode, OperationId, ToolId};

mod decode;
mod query;

pub use query::{COMPATIBLE_SCHEME, PRODUCT_SCHEME};

/// ADR-0029 decision 5. Windows hands a URL over as a command-line argument, so
/// the cap is deliberately far below any argv limit.
pub const MAX_DEEP_LINK_BYTES: usize = 8 * 1024;
/// One percent-decoded query value. Base64 payloads are the only large field.
const MAX_VALUE_BYTES: usize = 6 * 1024;
/// One decoded `config` / `content` payload.
const MAX_PAYLOAD_BYTES: usize = 4 * 1024;
const MAX_NAME_CHARS: usize = 80;
const MAX_DESCRIPTION_CHARS: usize = 500;
const MAX_URL_BYTES: usize = 2 * 1024;
const MAX_APPS: usize = 8;
const MAX_MCP_SERVERS: usize = 8;
const MAX_REPO_SEGMENT_CHARS: usize = 100;
const MAX_DIRECTORY_CHARS: usize = 200;

/// Where a link came from. The two paths have deliberately different
/// capabilities, so the origin travels with the intent all the way to the
/// confirmation dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkOrigin {
    /// Delivered by the operating system: cold start argv or a second instance.
    Argv,
    /// Typed or pasted by the user into a focused product window.
    Paste,
}

impl LinkOrigin {
    const fn allows_credentials(self) -> bool {
        matches!(self, Self::Paste)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepLinkResource {
    Provider,
    Mcp,
    Prompt,
    Skill,
}

/// Which field of a link carries a credential. Only the fact and the field
/// name ever reach the renderer; the value never does (ADR-0029 decision 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepLinkCredentialField {
    ApiKey,
    Config,
}

/// Why a queued link cannot be confirmed. The dialog disables its primary
/// action and says which of the two it is, instead of letting the user press a
/// button that can only fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepLinkBlockReason {
    /// The link names no application this product manages, or the one it names
    /// cannot manage this kind of resource.
    NoSupportedTool,
    /// The link creates a service but carries no key in any field, and a
    /// service without a key cannot be saved.
    CredentialRequired,
}

/// The safe view of one queued link. It carries the source, the target, and the
/// exact change that will happen — and never a credential value, only which
/// field holds one (ADR-0029 decision 5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepLinkPreview {
    /// Opaque; the renderer never sees the link itself.
    pub id: String,
    pub origin: LinkOrigin,
    pub resource: DeepLinkResource,
    /// The service, prompt or skill name. MCP links name their servers in
    /// `items` instead.
    pub name: Option<String>,
    /// A credential-free base URL, when the link names one.
    pub endpoint: Option<String>,
    /// One entry per concrete thing this import would create.
    pub items: Vec<String>,
    pub targets: Vec<DeepLinkTarget>,
    pub credential_fields: Vec<DeepLinkCredentialField>,
    pub blocked: Option<DeepLinkBlockReason>,
    /// Unix seconds. A queued link stops being confirmable at this point.
    pub expires_at: i64,
}

/// `tool` is `None` when the link names an application this product does not
/// manage: the untrusted token itself is never echoed back to the renderer, and
/// neither is a name derived from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepLinkTarget {
    pub tool: Option<ToolId>,
    /// The product's own display name for that tool, so the dialog does not
    /// have to load the whole inventory to say which application it means.
    pub name: Option<String>,
    pub supported: bool,
}

/// What confirming a link actually did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepLinkImportOutcome {
    pub resource: DeepLinkResource,
    /// The tools this import wrote to. The renderer refreshes exactly their
    /// caches, and the tray is rebuilt for them.
    pub tools: Vec<ToolId>,
    /// Background tasks this import started. Services and prompts are written
    /// synchronously and leave it empty.
    pub operations: Vec<OperationId>,
    /// How many things were created or queued for installation.
    pub applied: u32,
}

/// A validated link, ready for the application layer to resolve against the
/// installed tools. `Debug` is hand-written on every credential-bearing variant.
#[derive(Clone, PartialEq, Eq)]
pub enum DeepLinkIntent {
    Provider(DeepLinkProvider),
    Mcp(DeepLinkMcp),
    Prompt(DeepLinkPrompt),
    Skill(DeepLinkSkill),
}

impl DeepLinkIntent {
    pub fn resource(&self) -> DeepLinkResource {
        match self {
            Self::Provider(_) => DeepLinkResource::Provider,
            Self::Mcp(_) => DeepLinkResource::Mcp,
            Self::Prompt(_) => DeepLinkResource::Prompt,
            Self::Skill(_) => DeepLinkResource::Skill,
        }
    }

    /// The upstream `app` tokens this link targets, in link order.
    pub fn app_tokens(&self) -> Vec<&str> {
        match self {
            Self::Provider(value) => vec![value.app.as_str()],
            Self::Mcp(value) => value.apps.iter().map(String::as_str).collect(),
            Self::Prompt(value) => vec![value.app.as_str()],
            Self::Skill(value) => vec![value.app.as_str()],
        }
    }

    pub fn credential_fields(&self) -> Vec<DeepLinkCredentialField> {
        let (has_api_key, config_credential) = match self {
            Self::Provider(value) => (value.api_key.is_some(), value.config_credential),
            Self::Mcp(value) => (false, value.config_credential),
            Self::Prompt(_) | Self::Skill(_) => (false, false),
        };
        let mut fields = Vec::new();
        if has_api_key {
            fields.push(DeepLinkCredentialField::ApiKey);
        }
        if config_credential {
            fields.push(DeepLinkCredentialField::Config);
        }
        fields
    }
}

impl std::fmt::Debug for DeepLinkIntent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Provider(value) => value.fmt(formatter),
            Self::Mcp(value) => value.fmt(formatter),
            Self::Prompt(value) => value.fmt(formatter),
            Self::Skill(value) => value.fmt(formatter),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct DeepLinkProvider {
    /// The upstream application token exactly as the link spelled it.
    pub app: String,
    pub name: String,
    /// Already validated as a credential-free HTTP(S) base URL.
    pub endpoint: Option<String>,
    pub homepage: Option<String>,
    /// Some tools have no usable connection without a concrete model. The
    /// upstream links already carry this field, so it is read rather than
    /// forcing the user to finish the service by hand.
    pub model: Option<String>,
    pub api_key: Option<String>,
    /// The tool-native configuration, kept only for credential detection and as
    /// the endpoint fallback. It is never turned into a create draft: the
    /// product draft is a typed contract the compatibility layer expands from
    /// `ToolId` (`domain::provider::ProviderCustomCreateDraft`).
    pub config: Option<Value>,
    pub config_credential: bool,
}

impl std::fmt::Debug for DeepLinkProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeepLinkProvider")
            .field("app", &self.app)
            .field("name", &self.name)
            .field("endpoint", &self.endpoint.as_deref().map(|_| "<provided>"))
            .field("homepage", &self.homepage)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_deref().map(|_| "***"))
            .field("config", &self.config.as_ref().map(|_| "***"))
            .field("config_credential", &self.config_credential)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct DeepLinkMcp {
    pub apps: Vec<String>,
    pub servers: Vec<DeepLinkMcpServer>,
    pub enabled: Option<bool>,
    pub config_credential: bool,
}

impl std::fmt::Debug for DeepLinkMcp {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeepLinkMcp")
            .field("apps", &self.apps)
            .field(
                "servers",
                &self
                    .servers
                    .iter()
                    .map(|server| server.name.as_str())
                    .collect::<Vec<_>>(),
            )
            .field("enabled", &self.enabled)
            .field("config_credential", &self.config_credential)
            .finish()
    }
}

/// One entry of the link's `mcpServers` object, already reduced to the shape the
/// product's typed MCP draft accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepLinkMcpServer {
    pub name: String,
    pub connection: DeepLinkMcpConnection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepLinkMcpConnection {
    Stdio { command: String, args: Vec<String> },
    Http { url: String },
    Sse { url: String },
}

#[derive(Clone, PartialEq, Eq)]
pub struct DeepLinkPrompt {
    pub app: String,
    pub name: String,
    pub content: String,
    pub description: Option<String>,
}

impl std::fmt::Debug for DeepLinkPrompt {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeepLinkPrompt")
            .field("app", &self.app)
            .field("name", &self.name)
            .field("content", &"***")
            .field("description", &self.description)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepLinkSkill {
    pub app: String,
    pub owner: String,
    pub repository: String,
    pub branch: String,
    pub directory: String,
}

impl DeepLinkSkill {
    /// The stable catalog identity the Skill installation service expects.
    pub fn catalog_id(&self) -> String {
        format!("{}/{}:{}", self.owner, self.repository, self.directory)
    }

    /// The last path segment of the skill directory, which is the folder name a
    /// user recognizes.
    pub fn display_name(&self) -> String {
        self.directory
            .rsplit('/')
            .find(|segment| !segment.is_empty())
            .unwrap_or(self.directory.as_str())
            .to_string()
    }
}

/// Parses one link. Nothing is written and nothing is logged on any path.
pub fn parse(raw: &str, origin: LinkOrigin) -> Result<DeepLinkIntent, AppError> {
    let normalized = query::normalize(raw, origin.allows_credentials())?;
    let parsed = query::parse(&normalized)?;

    let intent = match parsed.required("resource")? {
        "provider" => parse_provider(&parsed)?,
        "mcp" => parse_mcp(&parsed)?,
        "prompt" => parse_prompt(&parsed)?,
        "skill" => parse_skill(&parsed)?,
        other => {
            return Err(invalid(
                "error.deepLink.unsupportedResource",
                format!("deep link requests the unsupported resource {other}"),
            ))
        }
    };

    if !origin.allows_credentials() && !intent.credential_fields().is_empty() {
        return Err(AppError::new(
            ErrorCode::PermissionDenied,
            "error.deepLink.credentialBlocked",
        )
        .with_technical("a link opened through the registered scheme carried a credential")
        .with_remediation("error.remediation.pasteDeepLink"));
    }

    Ok(intent)
}

const PROVIDER_KEYS: &[&str] = &[
    "resource",
    "app",
    "name",
    "endpoint",
    "homepage",
    "apiKey",
    "model",
    "config",
    "configFormat",
    "enabled",
];

fn parse_provider(parsed: &query::LinkQuery) -> Result<DeepLinkIntent, AppError> {
    parsed.reject_unknown(PROVIDER_KEYS)?;
    let _ = parsed.optional_bool("enabled")?;

    let config = match parsed.optional("config") {
        Some(raw) => Some(decode::decode_config(raw, parsed.optional("configFormat"))?),
        None => None,
    };
    let config_credential = config.as_ref().is_some_and(decode::carries_credential);

    // The upstream format allows a comma-separated endpoint list whose first
    // entry is the primary one. The product stores one base URL per service and
    // manages alternates in its own endpoint editor, so only the primary is read.
    let endpoint = match parsed.optional("endpoint") {
        Some(raw) => Some(safe_http_url(
            raw.split(',').next().unwrap_or_default(),
            "endpoint",
        )?),
        None => None,
    };
    let homepage = match parsed.optional("homepage") {
        Some(raw) => Some(safe_http_url(raw, "homepage")?),
        None => None,
    };

    Ok(DeepLinkIntent::Provider(DeepLinkProvider {
        app: app_token(parsed.required("app")?)?,
        name: name(parsed.required("name")?)?,
        endpoint,
        homepage,
        model: match parsed.optional("model") {
            Some(value) => Some(name(value)?),
            None => None,
        },
        api_key: parsed.optional("apiKey").map(str::to_string),
        config,
        config_credential,
    }))
}

const MCP_KEYS: &[&str] = &[
    "resource",
    "apps",
    "app",
    "config",
    "configFormat",
    "enabled",
];

fn parse_mcp(parsed: &query::LinkQuery) -> Result<DeepLinkIntent, AppError> {
    parsed.reject_unknown(MCP_KEYS)?;

    // Upstream spells the multi-target field `apps`; a single-target link in the
    // wild sometimes uses `app` instead, and both mean the same thing here.
    let raw_apps = parsed
        .optional("apps")
        .or_else(|| parsed.optional("app"))
        .ok_or_else(|| {
            invalid(
                "error.deepLink.missingParameter",
                "deep link is missing the required parameter apps",
            )
        })?;
    let mut apps = Vec::new();
    for token in raw_apps.split(',') {
        let token = app_token(token.trim())?;
        if !apps.contains(&token) {
            apps.push(token);
        }
    }
    if apps.is_empty() || apps.len() > MAX_APPS {
        return Err(invalid(
            "error.deepLink.invalidLink",
            "deep link names no usable target application",
        ));
    }

    let config =
        decode::decode_config(parsed.required("config")?, parsed.optional("configFormat"))?;
    let config_credential = decode::carries_credential(&config);

    Ok(DeepLinkIntent::Mcp(DeepLinkMcp {
        apps,
        servers: parse_mcp_servers(&config)?,
        enabled: parsed.optional_bool("enabled")?,
        config_credential,
    }))
}

fn parse_mcp_servers(config: &Value) -> Result<Vec<DeepLinkMcpServer>, AppError> {
    let entries = config
        .get("mcpServers")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            invalid(
                "error.deepLink.invalidLink",
                "deep link MCP config has no mcpServers object",
            )
        })?;
    if entries.is_empty() || entries.len() > MAX_MCP_SERVERS {
        return Err(invalid(
            "error.deepLink.invalidLink",
            "deep link MCP config names no usable server",
        ));
    }

    entries
        .iter()
        .map(|(id, spec)| {
            Ok(DeepLinkMcpServer {
                name: name(id)?,
                connection: parse_mcp_connection(spec)?,
            })
        })
        .collect()
}

fn parse_mcp_connection(spec: &Value) -> Result<DeepLinkMcpConnection, AppError> {
    let spec = spec.as_object().ok_or_else(|| {
        invalid(
            "error.deepLink.invalidLink",
            "deep link MCP server entry is not an object",
        )
    })?;

    // `env` and `headers` are exactly where an MCP credential lives, and the
    // product's typed install draft has no field for either (ARCHITECTURE 6.6).
    // Accepting the link and silently dropping them would install a server that
    // cannot authenticate, so refuse instead.
    if spec.contains_key("env") || spec.contains_key("headers") {
        return Err(invalid(
            "error.deepLink.mcpUnsupportedFields",
            "deep link MCP server carries environment variables or headers",
        ));
    }

    if let Some(url) = spec.get("url").and_then(Value::as_str) {
        let url = safe_http_url(url, "url")?;
        let transport = spec
            .get("type")
            .or_else(|| spec.get("transport"))
            .and_then(Value::as_str)
            .unwrap_or("http");
        return match transport {
            "http" | "streamable-http" | "streamableHttp" => {
                Ok(DeepLinkMcpConnection::Http { url })
            }
            "sse" => Ok(DeepLinkMcpConnection::Sse { url }),
            other => Err(invalid(
                "error.deepLink.invalidLink",
                format!("deep link MCP transport {other} is not supported"),
            )),
        };
    }

    let command = spec
        .get("command")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            invalid(
                "error.deepLink.invalidLink",
                "deep link MCP server has neither a URL nor a command",
            )
        })?;
    let args = match spec.get("args") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str().map(str::to_string).ok_or_else(|| {
                    invalid(
                        "error.deepLink.invalidLink",
                        "deep link MCP argument is not a string",
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => {
            return Err(invalid(
                "error.deepLink.invalidLink",
                "deep link MCP args is not a list",
            ))
        }
    };

    Ok(DeepLinkMcpConnection::Stdio {
        command: command.to_string(),
        args,
    })
}

const PROMPT_KEYS: &[&str] = &[
    "resource",
    "app",
    "name",
    "content",
    "description",
    "enabled",
];

fn parse_prompt(parsed: &query::LinkQuery) -> Result<DeepLinkIntent, AppError> {
    parsed.reject_unknown(PROMPT_KEYS)?;
    let _ = parsed.optional_bool("enabled")?;

    let content = decode::decode_payload("content", parsed.required("content")?)?;
    if content.trim().is_empty() {
        return Err(invalid(
            "error.deepLink.invalidLink",
            "deep link prompt content is empty",
        ));
    }

    let description = match parsed.optional("description") {
        Some(value) if value.chars().count() > MAX_DESCRIPTION_CHARS => {
            return Err(invalid(
                "error.deepLink.valueTooLong",
                "deep link prompt description is too long",
            ))
        }
        Some(value) => Some(value.to_string()),
        None => None,
    };

    Ok(DeepLinkIntent::Prompt(DeepLinkPrompt {
        app: app_token(parsed.required("app")?)?,
        name: name(parsed.required("name")?)?,
        content,
        description,
    }))
}

const SKILL_KEYS: &[&str] = &[
    "resource",
    "app",
    "repo",
    "branch",
    "directory",
    "skills_path",
    "enabled",
];

fn parse_skill(parsed: &query::LinkQuery) -> Result<DeepLinkIntent, AppError> {
    parsed.reject_unknown(SKILL_KEYS)?;
    let _ = parsed.optional_bool("enabled")?;

    let repo = parsed.required("repo")?;
    let mut segments = repo.split('/');
    let owner = segments.next().unwrap_or_default().trim();
    let repository = segments.next().unwrap_or_default().trim();
    if owner.is_empty() || repository.is_empty() || segments.next().is_some() {
        return Err(invalid(
            "error.deepLink.invalidLink",
            "deep link repo is not owner/repository",
        ));
    }
    for segment in [owner, repository] {
        if segment.chars().count() > MAX_REPO_SEGMENT_CHARS {
            return Err(invalid(
                "error.deepLink.valueTooLong",
                "deep link repository coordinate is too long",
            ));
        }
    }

    // `skills_path` names the folder inside the repository that holds skills and
    // `directory` names one skill inside it. Upstream reads only `directory`;
    // accepting `skills_path` as its fallback keeps single-parameter links from
    // vendors working without inventing a second meaning for either name. The
    // GitHub reference itself is validated by the Skill service before any
    // download starts (ADR-0029 decision 6).
    let directory = parsed
        .optional("directory")
        .or_else(|| parsed.optional("skills_path"))
        .ok_or_else(|| {
            invalid(
                "error.deepLink.missingParameter",
                "deep link is missing the skill directory",
            )
        })?;
    if directory.chars().count() > MAX_DIRECTORY_CHARS {
        return Err(invalid(
            "error.deepLink.valueTooLong",
            "deep link skill directory is too long",
        ));
    }

    Ok(DeepLinkIntent::Skill(DeepLinkSkill {
        // Skills were Claude-only upstream. The product gates them per tool, so
        // the token is optional and defaults to the historical target.
        app: match parsed.optional("app") {
            Some(value) => app_token(value)?,
            None => "claude".to_string(),
        },
        owner: owner.to_string(),
        repository: repository.to_string(),
        branch: parsed.optional("branch").unwrap_or("main").to_string(),
        directory: directory.trim_matches('/').to_string(),
    }))
}

/// Structural check only. Whether the product knows this token is decided by the
/// application layer against the compatibility layer's tool registry.
fn app_token(raw: &str) -> Result<String, AppError> {
    let token = raw.trim().to_ascii_lowercase();
    if token.is_empty()
        || token.len() > 32
        || !token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(invalid(
            "error.deepLink.invalidLink",
            "deep link application token is not a plain identifier",
        ));
    }
    Ok(token)
}

fn name(raw: &str) -> Result<String, AppError> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(invalid(
            "error.deepLink.missingParameter",
            "deep link name is empty",
        ));
    }
    if value.chars().count() > MAX_NAME_CHARS {
        return Err(invalid(
            "error.deepLink.valueTooLong",
            "deep link name is too long",
        ));
    }
    Ok(value.to_string())
}

/// The same guarantees the endpoint editor enforces: HTTP(S), a real host, and
/// no embedded credential.
fn safe_http_url(raw: &str, field: &'static str) -> Result<String, AppError> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > MAX_URL_BYTES {
        return Err(invalid(
            "error.deepLink.invalidLink",
            format!("deep link {field} is empty or too long"),
        ));
    }
    let parsed = url::Url::parse(raw).map_err(|_| {
        invalid(
            "error.deepLink.invalidLink",
            format!("deep link {field} is not a URL"),
        )
    })?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host().is_none()
        || parsed.fragment().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(invalid(
            "error.deepLink.invalidLink",
            format!("deep link {field} is not a credential-free HTTP address"),
        ));
    }
    Ok(raw.trim_end_matches('/').to_string())
}

fn invalid(message_key: &'static str, technical: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::ConfigParseFailed, message_key)
        .with_technical(technical)
        .with_remediation("error.remediation.retryOrViewDetails")
}

#[cfg(test)]
#[path = "deep_link/tests.rs"]
mod tests;
