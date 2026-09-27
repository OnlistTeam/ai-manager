//! Reading the official MCP Registry: which entries to show, and what an
//! entry becomes when it is added.
//!
//! An entry is taken by its remote first (streamable HTTP, then SSE, HTTPS
//! only), else by the package it is published as (npm through `npx -y`, PyPI
//! through `uvx`, an OCI image through `docker run -i --rm`). What it needs
//! from the user becomes inputs: required or secret environment variables and
//! headers, and required positional arguments. An entry that needs a named
//! argument the dialog cannot fill is left out.

use std::collections::HashSet;

use regex::Regex;
use serde::Deserialize;

use super::catalog::{owner_avatar, MarketInput, MarketServer, ServerSpec};
use super::matching::server_key;
use crate::domain::{DiscoverInput, DiscoverInputTarget, DiscoverRunner, DiscoverTransport};

/// At most this many registry entries follow the featured ones.
pub(crate) const MAX_REGISTRY_RESULTS: usize = 30;

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RegistryServer {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub website_url: String,
    #[serde(default)]
    pub repository: RegistryRepository,
    #[serde(default)]
    pub icons: Vec<RegistryIcon>,
    #[serde(default)]
    pub remotes: Vec<RegistryRemote>,
    #[serde(default)]
    pub packages: Vec<RegistryPackage>,
}

#[derive(Deserialize, Default)]
pub(crate) struct RegistryRepository {
    #[serde(default)]
    pub url: String,
}

#[derive(Deserialize)]
pub(crate) struct RegistryIcon {
    #[serde(default)]
    pub src: String,
}

#[derive(Deserialize)]
pub(crate) struct RegistryRemote {
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub headers: Vec<RegistryVariable>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RegistryPackage {
    #[serde(default)]
    pub registry_type: String,
    #[serde(default)]
    pub identifier: String,
    #[serde(default)]
    pub transport: RegistryPackageTransport,
    #[serde(default)]
    pub environment_variables: Vec<RegistryVariable>,
    #[serde(default)]
    pub package_arguments: Vec<RegistryArgument>,
}

#[derive(Deserialize, Default)]
pub(crate) struct RegistryPackageTransport {
    #[serde(default, rename = "type")]
    pub kind: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RegistryVariable {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub is_required: bool,
    #[serde(default)]
    pub is_secret: bool,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RegistryArgument {
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub default: String,
    #[serde(default)]
    pub value_hint: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub is_required: bool,
}

#[derive(Deserialize)]
struct RegistryPage {
    #[serde(default)]
    servers: Vec<RegistryEntry>,
}

#[derive(Deserialize)]
struct RegistryEntry {
    server: RegistryServer,
    #[serde(default, rename = "_meta")]
    meta: Option<serde_json::Value>,
}

impl RegistryEntry {
    /// The registry marks retired entries; only active ones (or unmarked
    /// ones) are offered.
    fn active(&self) -> bool {
        let status = self
            .meta
            .as_ref()
            .and_then(|meta| meta.get("io.modelcontextprotocol.registry/official"))
            .and_then(|official| official.get("status"))
            .and_then(|status| status.as_str())
            .unwrap_or("");
        status.is_empty() || status == "active"
    }
}

fn placeholder() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{[^{}]*\}").expect("constant pattern compiles"))
}

/// A search answer, converted, ranked, deduplicated by what each entry runs,
/// and cut to [`MAX_REGISTRY_RESULTS`].
pub(crate) fn parse_search(body: &[u8], query: &str) -> Result<Vec<MarketServer>, String> {
    let page: RegistryPage =
        serde_json::from_slice(body).map_err(|error| format!("registry answer: {error}"))?;
    let query = query.trim().to_lowercase();
    let mut seen = HashSet::new();
    let mut scored = Vec::new();
    for entry in page.servers {
        if !entry.active() {
            continue;
        }
        let Some(server) = from_registry(&entry.server) else {
            continue;
        };
        if !seen.insert(server_key(&server.spec)) {
            continue;
        }
        let score = rank(&entry.server, &server, &query);
        scored.push((score, server));
    }
    // Stable: equal scores keep the registry's own (alphabetical) order.
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    Ok(scored
        .into_iter()
        .take(MAX_REGISTRY_RESULTS)
        .map(|(_, server)| server)
        .collect())
}

/// First what is called what was searched for, then what says it is, with
/// an icon and a repository. Wrappers that republish others' servers sink.
pub(crate) fn rank(raw: &RegistryServer, server: &MarketServer, query: &str) -> i32 {
    let short = last_segment(&raw.name).to_lowercase();
    let title = raw.title.to_lowercase();
    let mut score = 0;
    if server.name == query || title == query || short == query {
        score += 100;
    } else if !query.is_empty() && short.contains(query) {
        score += 40;
    }
    if !query.is_empty() && title.contains(query) {
        score += 20;
    }
    if !query.is_empty() && raw.description.to_lowercase().contains(query) {
        score += 10;
    }
    if !raw.icons.is_empty() {
        score += 8;
    }
    if !raw.repository.url.is_empty() {
        score += 5;
    }
    if raw.name.starts_with("ai.smithery/") {
        score -= 60;
    }
    score
}

fn last_segment(value: &str) -> &str {
    value.rsplit('/').next().unwrap_or(value)
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn registry_input(
    key: &str,
    target: DiscoverInputTarget,
    label: &str,
    description: &str,
    secret: bool,
    required: bool,
    format: String,
) -> MarketInput {
    MarketInput {
        view: DiscoverInput {
            key: key.to_string(),
            target,
            label: label.to_string(),
            kind: None,
            description: non_empty(description),
            site: None,
            placeholder: None,
            secret,
            required,
        },
        format,
    }
}

/// What a registry entry becomes, or `None` when it offers nothing the app
/// can add.
pub(crate) fn from_registry(raw: &RegistryServer) -> Option<MarketServer> {
    if raw.name.trim().is_empty() {
        return None;
    }
    let owner = github_owner(&raw.repository.url).or_else(|| {
        raw.name
            .strip_prefix("io.github.")
            .and_then(|rest| rest.split('/').next())
            .filter(|owner| !owner.is_empty())
            .map(str::to_string)
    });
    let publisher = owner.clone().or_else(|| {
        let namespace = raw.name.split('/').next().unwrap_or_default();
        let mut parts = namespace.split('.').collect::<Vec<_>>();
        parts.reverse();
        non_empty(&parts.join("."))
    });
    let icon = raw
        .icons
        .iter()
        .find(|icon| icon.src.starts_with("https://"))
        .map(|icon| icon.src.clone())
        .or_else(|| owner.as_deref().map(owner_avatar));
    let name = short_name(&raw.name);
    let mut server = MarketServer {
        id: raw.name.clone(),
        title: non_empty(&raw.title).unwrap_or_else(|| name.clone()),
        name,
        description: non_empty(&raw.description),
        publisher,
        icon,
        homepage: non_empty(&raw.website_url).or_else(|| non_empty(&raw.repository.url)),
        runs: None,
        sign_in: false,
        featured: false,
        inputs: Vec::new(),
        spec: ServerSpec {
            transport: DiscoverTransport::Stdio,
            url: None,
            command: None,
            args: Vec::new(),
            headers: Vec::new(),
        },
    };
    if take_remote(raw, &mut server) || take_package(raw, &mut server)? {
        Some(server)
    } else {
        None
    }
}

fn take_remote(raw: &RegistryServer, server: &mut MarketServer) -> bool {
    for (wanted, transport) in [
        ("streamable-http", DiscoverTransport::Http),
        ("sse", DiscoverTransport::Sse),
    ] {
        let Some(remote) = raw.remotes.iter().find(|remote| {
            remote.kind == wanted && remote.url.starts_with("https://") && !remote.url.contains('{')
        }) else {
            continue;
        };
        server.spec.transport = transport;
        server.spec.url = Some(remote.url.clone());
        for header in &remote.headers {
            if header.name.trim().is_empty() {
                continue;
            }
            if !header.value.is_empty() && !placeholder().is_match(&header.value) {
                server
                    .spec
                    .headers
                    .push((header.name.clone(), header.value.clone()));
            } else if header.is_required || header.is_secret {
                let format = if header.value.is_empty() {
                    "{}".to_string()
                } else {
                    placeholder().replace_all(&header.value, "{}").into_owned()
                };
                server.inputs.push(registry_input(
                    &header.name,
                    DiscoverInputTarget::Header,
                    &header.name,
                    &header.description,
                    header.is_secret,
                    header.is_required,
                    format,
                ));
            }
        }
        return true;
    }
    false
}

/// `Some(true)` when a package was taken, `Some(false)` when there was none
/// to take, `None` when the entry needs something the dialog cannot fill.
fn take_package(raw: &RegistryServer, server: &mut MarketServer) -> Option<bool> {
    for (wanted, runner) in [
        ("npm", DiscoverRunner::Npx),
        ("pypi", DiscoverRunner::Uvx),
        ("oci", DiscoverRunner::Docker),
    ] {
        let Some(package) = raw.packages.iter().find(|package| {
            package.registry_type == wanted
                && !package.identifier.trim().is_empty()
                && (package.transport.kind.is_empty() || package.transport.kind == "stdio")
        }) else {
            continue;
        };
        let spec = &mut server.spec;
        spec.transport = DiscoverTransport::Stdio;
        let (command, mut args) = match runner {
            DiscoverRunner::Npx => ("npx", vec!["-y".to_string(), package.identifier.clone()]),
            DiscoverRunner::Uvx => ("uvx", vec![package.identifier.clone()]),
            DiscoverRunner::Docker => (
                "docker",
                vec!["run".to_string(), "-i".to_string(), "--rm".to_string()],
            ),
        };
        spec.command = Some(command.to_string());
        server.runs = Some(runner);
        for variable in &package.environment_variables {
            if variable.name.trim().is_empty() || !(variable.is_required || variable.is_secret) {
                continue;
            }
            server.inputs.push(registry_input(
                &variable.name,
                DiscoverInputTarget::Env,
                &variable.name,
                &variable.description,
                variable.is_secret,
                variable.is_required,
                "{}".to_string(),
            ));
            if runner == DiscoverRunner::Docker {
                args.push("-e".to_string());
                args.push(variable.name.clone());
            }
        }
        if runner == DiscoverRunner::Docker {
            args.push(package.identifier.clone());
        }
        for (index, argument) in package.package_arguments.iter().enumerate() {
            let mut value = if argument.value.is_empty() {
                argument.default.clone()
            } else {
                argument.value.clone()
            };
            if placeholder().is_match(&value) {
                value.clear();
            }
            let named = argument.kind == "named";
            if !value.is_empty() && named {
                args.push(argument.name.clone());
                args.push(value);
            } else if !value.is_empty() {
                args.push(value);
            } else if argument.is_required && !named {
                let label =
                    non_empty(&argument.value_hint).unwrap_or_else(|| format!("#{}", index + 1));
                server.inputs.push(registry_input(
                    &format!("arg{index}"),
                    DiscoverInputTarget::Argument,
                    &label,
                    &argument.description,
                    false,
                    true,
                    "{}".to_string(),
                ));
            } else if argument.is_required {
                return None;
            }
        }
        server.spec.args = args;
        return Some(true);
    }
    Some(false)
}

/// A registry name as a connection would be called:
/// `io.github.upstash/context7-mcp` is `context7`.
pub(crate) fn short_name(name: &str) -> String {
    let original = last_segment(name).to_lowercase();
    let mut short = original.as_str();
    for prefix in ["mcp-server-", "server-", "mcp-"] {
        short = short.strip_prefix(prefix).unwrap_or(short);
    }
    for suffix in ["-mcp-server", "-mcp", "-server", "_mcp", ".mcp"] {
        short = short.strip_suffix(suffix).unwrap_or(short);
    }
    let short = if short.is_empty() {
        original.as_str()
    } else {
        short
    };
    let cleaned = short
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let cleaned = cleaned.trim_matches(|character| character == '-' || character == '_');
    let cleaned = cleaned.chars().take(64).collect::<String>();
    if cleaned.is_empty() {
        "server".to_string()
    } else {
        cleaned
    }
}

/// The owner of a `github.com` repository URL.
pub(crate) fn github_owner(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    if !matches!(parsed.host_str(), Some("github.com" | "www.github.com")) {
        return None;
    }
    parsed
        .path_segments()?
        .find(|segment| !segment.is_empty())
        .map(str::to_string)
}
