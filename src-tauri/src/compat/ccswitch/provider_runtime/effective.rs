//! Effective-connection resolver: answers "where will the next launch connect" using each
//! tool's **own** precedence rules.
//!
//! The rules come from ADR-0035 and section 1 of the 2026-09-07 plan. Tool differences are
//! concentrated in the `match ToolId` here (AI_RULES rule 8); presentation only sees
//! `EffectiveConnection`, which carries whether a credential exists and never the credential
//! itself. The value is read for one other caller only — the model probe (ADR-0041), which
//! needs it to build a request in the backend — and `resolve_effective_connection` drops it.

use std::collections::HashMap;
use std::path::Path;

use indexmap::IndexMap;
use serde_json::Value;

use super::environment::{ResolvedVariable, ToolEnvironment, VariableOrigin};
use super::{app_type, display_path, display_source, safe_endpoint};
use crate::domain::{
    EffectiveConnection, EffectiveConnectionSource, EffectiveCredential, EffectiveSelection, ToolId,
};
use crate::provider::Provider as UpstreamProvider;

mod opencode;
#[cfg(test)]
use opencode::resolve_opencode;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Resolution {
    pub selection: EffectiveSelection,
    pub model: Option<String>,
    pub endpoint: Option<String>,
    pub endpoint_source: EffectiveConnectionSource,
    pub credential: EffectiveCredential,
    pub credential_source: EffectiveConnectionSource,
    /// The credential itself, when this resolver could read it.
    ///
    /// `Some` for a key written in a config file or exported by the shell;
    /// `None` when a credential exists but is not a usable bearer token (an
    /// OAuth login) or when there is none at all. It is kept only so the model
    /// probe can build one request in the backend: it never crosses IPC, never
    /// reaches a log, and `EffectiveConnection` drops it on the way out.
    pub credential_value: Option<String>,
    /// OpenCode learns the provider id in use directly from the `model` field.
    pub provider_hint: Option<String>,
}

/// Hand-written so a credential cannot reach a log through a `{:?}`.
impl std::fmt::Debug for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resolution")
            .field("selection", &self.selection)
            .field("model", &self.model)
            .field("endpoint", &self.endpoint)
            .field("endpoint_source", &self.endpoint_source)
            .field("credential", &self.credential)
            .field("credential_source", &self.credential_source)
            .field(
                "credential_value",
                &self.credential_value.as_ref().map(|_| "<redacted>"),
            )
            .field("provider_hint", &self.provider_hint)
            .finish()
    }
}

type Endpoint = Option<(String, EffectiveConnectionSource)>;
/// The credential's source, and its value when this resolver could read one.
type Credential = Option<(Option<String>, EffectiveConnectionSource)>;

/// Address and credential of the connection the next launch will use, for the
/// model probe (ADR-0041). Returns `None` when there is no address to test or
/// no readable credential to test it with.
pub(super) fn resolve_probe_target(
    tool: ToolId,
    environment: &ToolEnvironment,
) -> Option<(String, String)> {
    probe_target_from(resolve(tool, environment)?)
}

fn probe_target_from(resolution: Resolution) -> Option<(String, String)> {
    let endpoint = resolution.endpoint?;
    // An empty key is legitimate — a loopback gateway usually wants none — so
    // only a credential this resolver could not read at all disqualifies the
    // connection. A tool's own OAuth login is exactly that case: it is
    // `Configured` for display, but there is no bearer token to replay.
    let key = match resolution.credential {
        EffectiveCredential::Configured => resolution.credential_value?,
        EffectiveCredential::Missing => String::new(),
        EffectiveCredential::ToolLogin | EffectiveCredential::Unknown => return None,
    };
    Some((endpoint, key))
}

pub(super) fn resolve_effective_connection(
    tool: ToolId,
    environment: &ToolEnvironment,
    providers: &IndexMap<String, UpstreamProvider>,
    current_id: &str,
) -> Option<EffectiveConnection> {
    let resolution = resolve(tool, environment)?;
    let provider_id = match_provider(tool, &resolution, providers, current_id);
    Some(EffectiveConnection {
        selection: resolution.selection,
        model: resolution.model,
        endpoint: resolution.endpoint,
        endpoint_source: resolution.endpoint_source,
        credential: resolution.credential,
        credential_source: resolution.credential_source,
        provider_id,
        shell_inspected: environment.shell_inspected(),
    })
}

fn resolve(tool: ToolId, environment: &ToolEnvironment) -> Option<Resolution> {
    let resolution = match tool {
        ToolId::ClaudeCode => {
            let path = crate::config::get_claude_settings_path();
            match read_json_or_empty(&path) {
                Some(config) => resolve_claude(&path, &config, environment),
                None => unknown(),
            }
        }
        ToolId::Codex => {
            let config_path = crate::codex_config::get_codex_config_path();
            let auth_path = crate::codex_config::get_codex_auth_path();
            let live = crate::codex_config::read_codex_live_settings().ok();
            if live.is_none()
                && (!matches!(config_path.try_exists(), Ok(false))
                    || !matches!(auth_path.try_exists(), Ok(false)))
            {
                return Some(unknown());
            }
            let auth = live
                .as_ref()
                .and_then(|value| value.get("auth"))
                .cloned()
                .unwrap_or(Value::Null);
            let config = live
                .as_ref()
                .and_then(|value| value.get("config"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            resolve_codex(&config_path, &auth_path, &config, &auth, environment)
        }
        ToolId::GeminiCli => {
            let path = crate::gemini_config::get_gemini_env_path();
            match crate::gemini_config::read_gemini_env() {
                Ok(config) => resolve_gemini(&path, &config, environment),
                Err(_) => unknown(),
            }
        }
        ToolId::GrokBuild => {
            let path = crate::grok_config::get_grok_config_path();
            let live = crate::grok_config::read_grok_live_settings().ok();
            if live.is_none() && !matches!(path.try_exists(), Ok(false)) {
                return Some(unknown());
            }
            let config = live
                .and_then(|value| {
                    value
                        .get("config")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_default();
            resolve_grok(&path, &config, environment)
        }
        ToolId::OpenCode => opencode::resolve_live(environment),
        ToolId::OpenClaw | ToolId::Hermes | ToolId::Pi | ToolId::KimiCode | ToolId::DeepSeekDsh => {
            return None
        }
    };
    Some(resolution)
}

fn read_json_or_empty(path: &Path) -> Option<Value> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes)
            .ok()
            .filter(Value::is_object),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(serde_json::json!({})),
        Err(_) => None,
    }
}

fn unknown() -> Resolution {
    let mut result = finish(None, None);
    result.selection = EffectiveSelection::Unknown;
    result.credential = EffectiveCredential::Unknown;
    result
}

fn live(path: &Path) -> EffectiveConnectionSource {
    EffectiveConnectionSource::LiveConfig {
        path: display_path(path),
    }
}

fn from_variable(variable: &ResolvedVariable) -> EffectiveConnectionSource {
    match &variable.origin {
        VariableOrigin::ShellFile(path) => EffectiveConnectionSource::ShellFile {
            variable: variable.name.clone(),
            path: display_source(path),
        },
        VariableOrigin::Shell => EffectiveConnectionSource::Environment {
            variable: variable.name.clone(),
        },
    }
}

/// A custom endpoint with no key -> Missing; neither endpoint nor key -> use the tool's own login.
fn finish(endpoint: Endpoint, credential: Credential) -> Resolution {
    let (endpoint, endpoint_source) = match endpoint {
        Some((url, source)) => (safe_endpoint(&url), source),
        None => (None, EffectiveConnectionSource::ToolDefault),
    };
    let (credential, credential_value, credential_source) = match credential {
        Some((value, source)) => (EffectiveCredential::Configured, value, source),
        None if matches!(endpoint_source, EffectiveConnectionSource::ToolDefault) => (
            EffectiveCredential::ToolLogin,
            None,
            EffectiveConnectionSource::ToolDefault,
        ),
        None => (
            EffectiveCredential::Missing,
            None,
            EffectiveConnectionSource::ToolDefault,
        ),
    };
    Resolution {
        selection: EffectiveSelection::Configuration,
        model: None,
        endpoint,
        endpoint_source,
        credential,
        credential_source,
        credential_value,
        provider_hint: None,
    }
}

// ---------- Claude Code: settings.json `env` beats the shell; an explicit empty string cancels ----------

pub(super) fn resolve_claude(
    settings_path: &Path,
    settings: &Value,
    environment: &ToolEnvironment,
) -> Resolution {
    let file_value = |name: &str| -> Option<Option<String>> {
        settings
            .get("env")
            .and_then(|env| env.get(name))
            .and_then(Value::as_str)
            .map(|value| {
                let trimmed = value.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            })
    };
    // Some(Some(v)) the file has a value; Some(None) the file explicitly leaves it empty (a shell variable of the same name is cancelled too); None the file does not mention it.
    let endpoint: Endpoint = match file_value("ANTHROPIC_BASE_URL") {
        Some(Some(url)) => Some((url, live(settings_path))),
        Some(None) => None,
        None => environment
            .lookup("ANTHROPIC_BASE_URL")
            .map(|variable| (variable.value.clone(), from_variable(&variable))),
    };
    let credential_for = |name: &str| -> Credential {
        match file_value(name) {
            Some(Some(value)) => Some((Some(value), live(settings_path))),
            Some(None) => None,
            None => environment
                .lookup(name)
                .map(|variable| (Some(variable.value.clone()), from_variable(&variable))),
        }
    };
    let credential: Credential =
        credential_for("ANTHROPIC_AUTH_TOKEN").or_else(|| credential_for("ANTHROPIC_API_KEY"));
    finish(endpoint, credential)
}

// ---------- Codex: CODEX_API_KEY > env_key > experimental_bearer_token > auth.json ----------

struct CodexProviderTable {
    base_url: Option<String>,
    env_key: Option<String>,
    bearer_token: Option<String>,
    requires_openai_auth: bool,
    builtin_openai: bool,
}

fn codex_provider_table(config_toml: &str) -> CodexProviderTable {
    let doc = config_toml.parse::<toml::Value>().ok();
    let provider_id = doc
        .as_ref()
        .and_then(|doc| doc.get("model_provider"))
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or("openai")
        .to_string();
    let table = doc
        .as_ref()
        .and_then(|doc| doc.get("model_providers"))
        .and_then(|providers| providers.get(&provider_id))
        .and_then(toml::Value::as_table)
        .cloned();
    match table {
        Some(table) => CodexProviderTable {
            base_url: table
                .get("base_url")
                .and_then(toml::Value::as_str)
                .map(str::to_string),
            env_key: table
                .get("env_key")
                .and_then(toml::Value::as_str)
                .map(str::to_string),
            bearer_token: table
                .get("experimental_bearer_token")
                .and_then(toml::Value::as_str)
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .map(str::to_string),
            requires_openai_auth: table
                .get("requires_openai_auth")
                .and_then(toml::Value::as_bool)
                .unwrap_or(false),
            builtin_openai: false,
        },
        // Built-in openai: codex model-provider-info `create_openai_provider` — the address
        // comes only from OPENAI_BASE_URL and the credential from OPENAI_API_KEY or the login.
        None => CodexProviderTable {
            base_url: None,
            env_key: (provider_id == "openai").then(|| "OPENAI_API_KEY".to_string()),
            bearer_token: None,
            requires_openai_auth: provider_id == "openai",
            builtin_openai: provider_id == "openai",
        },
    }
}

/// What `auth.json` can contribute: `Some(Some(key))` a readable key,
/// `Some(None)` an OAuth login that is present but cannot be replayed as a
/// bearer token, `None` nothing at all.
fn codex_auth(auth: &Value) -> Option<Option<String>> {
    if let Some(key) = auth
        .get("OPENAI_API_KEY")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
    {
        return Some(Some(key.to_string()));
    }
    auth.get("tokens")
        .filter(|tokens| !tokens.is_null())
        .map(|_| None)
}

pub(super) fn resolve_codex(
    config_path: &Path,
    auth_path: &Path,
    config_toml: &str,
    auth: &Value,
    environment: &ToolEnvironment,
) -> Resolution {
    let table = codex_provider_table(config_toml);

    let endpoint: Endpoint = match &table.base_url {
        Some(url) => Some((url.clone(), live(config_path))),
        None if table.builtin_openai => environment
            .lookup("OPENAI_BASE_URL")
            .map(|variable| (variable.value.clone(), from_variable(&variable))),
        None => None,
    };

    let credential: Credential = if let Some(variable) = environment.lookup("CODEX_API_KEY") {
        Some((Some(variable.value.clone()), from_variable(&variable)))
    } else if let Some(env_key) = table.env_key.as_deref() {
        match environment.lookup(env_key) {
            Some(variable) => Some((Some(variable.value.clone()), from_variable(&variable))),
            // A custom table names a variable but does not set it: Codex reports an EnvVar error outright and does not fall back to auth.json.
            None if !table.builtin_openai => None,
            None => codex_auth(auth).map(|value| (value, live(auth_path))),
        }
    } else if let Some(token) = table.bearer_token.clone() {
        Some((Some(token), live(config_path)))
    } else if table.requires_openai_auth {
        codex_auth(auth).map(|value| (value, live(auth_path)))
    } else {
        None
    };
    finish(endpoint, credential)
}

// ---------- Gemini CLI: the shell beats .env (loadEnvironment only fills keys process.env lacks) ----------

pub(super) fn resolve_gemini(
    env_path: &Path,
    dotenv: &HashMap<String, String>,
    environment: &ToolEnvironment,
) -> Resolution {
    let pick = |name: &str| -> Option<(String, EffectiveConnectionSource)> {
        if let Some(variable) = environment.lookup(name) {
            return Some((variable.value.clone(), from_variable(&variable)));
        }
        dotenv
            .get(name)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(|value| (value.to_string(), live(env_path)))
    };
    let endpoint = pick("GOOGLE_GEMINI_BASE_URL");
    let credential = pick("GEMINI_API_KEY")
        .or_else(|| pick("GOOGLE_API_KEY"))
        .map(|(value, source)| (Some(value), source));
    finish(endpoint, credential)
}

// ---------- Grok Build: only the api_key / env_key declared in the config count ----------

pub(super) fn resolve_grok(
    config_path: &Path,
    config_toml: &str,
    environment: &ToolEnvironment,
) -> Resolution {
    let Some(model) = crate::grok_config::extract_model_config(config_toml) else {
        return finish(None, None);
    };
    let endpoint: Endpoint = Some((model.base_url.clone(), live(config_path)));
    let credential: Credential = if let Some(key) = model
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
    {
        Some((Some(key.to_string()), live(config_path)))
    } else if let Some(env_key) = model.env_key.as_deref() {
        environment
            .lookup(env_key)
            .map(|variable| (Some(variable.value.clone()), from_variable(&variable)))
    } else {
        None
    };
    finish(endpoint, credential)
}

// ---------- Compare against the saved endpoint ----------

pub(super) fn match_provider(
    tool: ToolId,
    resolution: &Resolution,
    providers: &IndexMap<String, UpstreamProvider>,
    current_id: &str,
) -> Option<String> {
    if resolution.selection == EffectiveSelection::Unknown {
        return None;
    }
    if tool == ToolId::OpenCode {
        // OpenCode has no notion of a "current service", so when the `model` field does not
        // resolve to a provider there simply is no match — it must never fall through to the
        // endpoint / official fallback, which would light up an unrelated official card even
        // though the config never named a provider.
        return resolution
            .provider_hint
            .as_ref()
            .filter(|hint| {
                providers.get(*hint).is_some_and(|provider| {
                    let (saved, _) =
                        provider.resolve_usage_credentials(&crate::app_config::AppType::OpenCode);
                    match resolution.endpoint.as_deref() {
                        Some(endpoint) => same_endpoint(&saved, endpoint),
                        None => saved.is_empty(),
                    }
                })
            })
            .cloned();
    }
    let app_type = app_type(tool)?;
    let ordered = std::iter::once(current_id)
        .chain(
            providers
                .keys()
                .map(String::as_str)
                .filter(|id| *id != current_id),
        )
        .filter_map(|id| providers.get(id));
    match &resolution.endpoint {
        Some(endpoint) => ordered
            .filter(|provider| {
                // `resolve_usage_credentials` returns a bare `String` (as used by
                // `provider_from_upstream`); with no endpoint configured it is an empty
                // string, not an `Option`.
                let (base_url, _) = provider.resolve_usage_credentials(&app_type);
                !base_url.is_empty() && same_endpoint(&base_url, endpoint)
            })
            .map(|provider| provider.id.clone())
            .next(),
        None if matches!(
            resolution.endpoint_source,
            EffectiveConnectionSource::ToolDefault
        ) =>
        {
            ordered
                .filter(|provider| provider.category.as_deref() == Some("official"))
                .map(|provider| provider.id.clone())
                .next()
        }
        None => None,
    }
}

fn same_endpoint(left: &str, right: &str) -> bool {
    match (url::Url::parse(left.trim()), url::Url::parse(right.trim())) {
        (Ok(left), Ok(right)) => {
            left.scheme() == right.scheme()
                && left.host_str() == right.host_str()
                && left.port_or_known_default() == right.port_or_known_default()
                && left.path().trim_end_matches('/') == right.path().trim_end_matches('/')
                && left.query() == right.query()
                && left.fragment() == right.fragment()
                && left.username() == right.username()
                && left.password() == right.password()
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};
    use std::path::PathBuf;

    use indexmap::IndexMap;
    use serde_json::json;

    use super::{
        codex_provider_table, match_provider, probe_target_from, resolve_claude, resolve_codex,
        resolve_effective_connection, resolve_gemini, resolve_grok, resolve_opencode,
    };
    use crate::compat::ccswitch::provider_runtime::environment::ToolEnvironment;
    use crate::domain::{
        EffectiveConnection, EffectiveConnectionSource, EffectiveCredential, ToolId,
    };
    use crate::platform::{ShellEnvironment, ShellEnvironmentSource};
    use crate::provider::Provider as UpstreamProvider;
    use crate::services::env_checker::EnvConflict;

    // Every test that derives a path through `get_home_dir()` is marked
    // `#[serial_test::serial]` — that function reads the process-wide
    // `AI_MANAGER_TEST_HOME`, and tests running concurrently in other modules (such as
    // `provider/tests_create.rs`) would step on each other. Paths are computed here through
    // `get_home_dir()` rather than hardcoded, so `display_path` reliably yields `~/…`
    // whether or not the corresponding file exists on this machine.

    fn claude_settings_path() -> PathBuf {
        crate::config::get_home_dir()
            .join(".claude")
            .join("settings.json")
    }

    fn codex_config_path() -> PathBuf {
        crate::config::get_home_dir()
            .join(".codex")
            .join("config.toml")
    }

    fn codex_auth_path() -> PathBuf {
        crate::config::get_home_dir()
            .join(".codex")
            .join("auth.json")
    }

    fn gemini_env_path() -> PathBuf {
        crate::config::get_home_dir().join(".gemini").join(".env")
    }

    fn grok_config_path() -> PathBuf {
        crate::config::get_home_dir()
            .join(".grok")
            .join("config.toml")
    }

    fn opencode_config_path() -> PathBuf {
        crate::config::get_home_dir()
            .join(".config")
            .join("opencode")
            .join("opencode.json")
    }

    fn env(pairs: &[(&str, &str)]) -> ToolEnvironment {
        let mut vars: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        vars.insert("PATH".to_string(), "/bin".to_string());
        ToolEnvironment::from_parts(
            ShellEnvironment::new(vars, ShellEnvironmentSource::LoginShell),
            vec![],
        )
    }

    fn env_with_file(pairs: &[(&str, &str)], declared: (&str, &str)) -> ToolEnvironment {
        let mut vars: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        vars.insert("PATH".to_string(), "/bin".to_string());
        // `lookup()` only credits a file with a variable when its declared value
        // matches what the shell actually exports (environment.rs semantics,
        // e.g. `a_declaration_with_a_different_value_is_not_credited`), so the
        // fixture must declare the same value the shell carries for `declared.0`.
        let declared_value = pairs
            .iter()
            .find(|(name, _)| *name == declared.0)
            .map(|(_, value)| value.to_string())
            .unwrap_or_default();
        ToolEnvironment::from_parts(
            ShellEnvironment::new(vars, ShellEnvironmentSource::LoginShell),
            vec![EnvConflict {
                var_name: declared.0.to_string(),
                var_value: declared_value,
                source_type: "file".to_string(),
                source_path: format!("{}/{}", crate::config::get_home_dir().display(), declared.1),
            }],
        )
    }

    /// `display_path` renders `~` plus the *native* relative path, so a Windows user sees
    /// `~/.claude\settings.json`. Expectations are therefore spelled with POSIX separators
    /// and translated here instead of being hardcoded.
    fn live(path_suffix: &str) -> EffectiveConnectionSource {
        EffectiveConnectionSource::LiveConfig {
            path: format!(
                "~/{}",
                path_suffix.replace('/', std::path::MAIN_SEPARATOR_STR)
            ),
        }
    }

    /// The same RAII approach as `TestHome` in `provider/tests_create.rs`; a private copy
    /// lives here because that one is `pub(super)` for other modules.
    struct TestHome(Option<std::ffi::OsString>);

    impl TestHome {
        fn set(path: &std::path::Path) -> Self {
            let previous = std::env::var_os("AI_MANAGER_TEST_HOME");
            std::env::set_var("AI_MANAGER_TEST_HOME", path);
            Self(previous)
        }
    }

    impl Drop for TestHome {
        fn drop(&mut self) {
            match self.0.take() {
                Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
                None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
            }
        }
    }

    // ---------- Claude Code ----------

    #[test]
    #[serial_test::serial]
    fn claude_settings_file_beats_the_shell() {
        let path = claude_settings_path();
        let settings = json!({"env": {"ANTHROPIC_BASE_URL": "https://file.example.test/", "ANTHROPIC_AUTH_TOKEN": "sk-file"}});
        let resolution = resolve_claude(
            &path,
            &settings,
            &env(&[("ANTHROPIC_BASE_URL", "https://shell.example.test")]),
        );
        assert_eq!(
            resolution.endpoint.as_deref(),
            Some("https://file.example.test/")
        );
        assert_eq!(resolution.endpoint_source, live(".claude/settings.json"));
        assert_eq!(resolution.credential, EffectiveCredential::Configured);
        assert_eq!(resolution.credential_source, live(".claude/settings.json"));
    }

    #[test]
    #[serial_test::serial]
    fn claude_falls_through_to_the_shell_when_the_file_is_silent() {
        let path = claude_settings_path();
        let resolution = resolve_claude(
            &path,
            &json!({"hooks": {}}),
            &env_with_file(
                &[
                    ("ANTHROPIC_BASE_URL", "https://api.example.test"),
                    ("ANTHROPIC_AUTH_TOKEN", "sk-shell"),
                ],
                ("ANTHROPIC_BASE_URL", ".config/zsh/secrets.zsh:3"),
            ),
        );
        assert_eq!(
            resolution.endpoint.as_deref(),
            Some("https://api.example.test/")
        );
        assert_eq!(
            resolution.endpoint_source,
            EffectiveConnectionSource::ShellFile {
                variable: "ANTHROPIC_BASE_URL".to_string(),
                path: "~/.config/zsh/secrets.zsh:3".to_string(),
            }
        );
        assert_eq!(
            resolution.credential_source,
            EffectiveConnectionSource::Environment {
                variable: "ANTHROPIC_AUTH_TOKEN".to_string()
            }
        );
    }

    #[test]
    #[serial_test::serial]
    fn claude_empty_string_in_the_file_cancels_the_shell_variable() {
        let path = claude_settings_path();
        let resolution = resolve_claude(
            &path,
            &json!({"env": {"ANTHROPIC_BASE_URL": "", "ANTHROPIC_AUTH_TOKEN": ""}}),
            &env(&[
                ("ANTHROPIC_BASE_URL", "https://api.example.test"),
                ("ANTHROPIC_AUTH_TOKEN", "sk-shell"),
            ]),
        );
        assert_eq!(resolution.endpoint, None);
        assert_eq!(
            resolution.endpoint_source,
            EffectiveConnectionSource::ToolDefault
        );
        assert_eq!(resolution.credential, EffectiveCredential::ToolLogin);
    }

    #[test]
    #[serial_test::serial]
    fn a_custom_endpoint_without_any_key_is_missing_not_tool_login() {
        let path = claude_settings_path();
        let resolution = resolve_claude(
            &path,
            &json!({"env": {"ANTHROPIC_BASE_URL": "https://relay.example.test"}}),
            &env(&[]),
        );
        assert_eq!(resolution.credential, EffectiveCredential::Missing);
    }

    #[test]
    #[serial_test::serial]
    fn claude_an_empty_auth_token_in_the_file_does_not_cancel_the_api_key_fallback() {
        let path = claude_settings_path();
        // `ANTHROPIC_AUTH_TOKEN` is explicitly left empty in the file while
        // `ANTHROPIC_API_KEY` is not mentioned at all — only the AUTH_TOKEN chain should be
        // cancelled, without swallowing the shell fallback of API_KEY as well.
        let resolution = resolve_claude(
            &path,
            &json!({"env": {"ANTHROPIC_BASE_URL": "https://relay.example.test", "ANTHROPIC_AUTH_TOKEN": ""}}),
            &env(&[("ANTHROPIC_API_KEY", "sk-shell")]),
        );
        assert_eq!(resolution.credential, EffectiveCredential::Configured);
        assert_eq!(
            resolution.credential_source,
            EffectiveConnectionSource::Environment {
                variable: "ANTHROPIC_API_KEY".to_string()
            }
        );
    }

    // ---------- Codex ----------

    const CODEX_RELAY: &str = r#"
model = "gpt-5"
model_provider = "relay"

[model_providers.relay]
name = "Relay"
base_url = "https://relay.example.test/v1"
wire_api = "responses"
requires_openai_auth = true
"#;

    #[test]
    #[serial_test::serial]
    fn codex_api_key_env_outranks_everything() {
        let config_path = codex_config_path();
        let auth_path = codex_auth_path();
        let resolution = resolve_codex(
            &config_path,
            &auth_path,
            CODEX_RELAY,
            &json!({"OPENAI_API_KEY": "sk-auth"}),
            &env(&[("CODEX_API_KEY", "sk-env")]),
        );
        assert_eq!(
            resolution.endpoint.as_deref(),
            Some("https://relay.example.test/v1")
        );
        assert_eq!(resolution.endpoint_source, live(".codex/config.toml"));
        assert_eq!(
            resolution.credential_source,
            EffectiveConnectionSource::Environment {
                variable: "CODEX_API_KEY".to_string()
            }
        );
    }

    #[test]
    #[serial_test::serial]
    fn codex_requires_openai_auth_reads_auth_json() {
        let config_path = codex_config_path();
        let auth_path = codex_auth_path();
        let resolution = resolve_codex(
            &config_path,
            &auth_path,
            CODEX_RELAY,
            &json!({"OPENAI_API_KEY": "sk-auth"}),
            &env(&[]),
        );
        assert_eq!(resolution.credential, EffectiveCredential::Configured);
        assert_eq!(resolution.credential_source, live(".codex/auth.json"));
    }

    #[test]
    #[serial_test::serial]
    fn codex_declared_env_key_that_is_unset_is_missing() {
        let config_path = codex_config_path();
        let auth_path = codex_auth_path();
        let config = r#"
model_provider = "relay"
[model_providers.relay]
base_url = "https://relay.example.test/v1"
env_key = "RELAY_KEY"
"#;
        let resolution = resolve_codex(
            &config_path,
            &auth_path,
            config,
            &json!({"OPENAI_API_KEY": "sk-auth"}),
            &env(&[]),
        );
        assert_eq!(resolution.credential, EffectiveCredential::Missing);
    }

    // Important 8: when env_key is declared but not set, it must not fall back even if the
    // same table also carries experimental_bearer_token — once a custom table declares
    // env_key, Codex honours only that variable and the chain simply breaks here.
    #[test]
    #[serial_test::serial]
    fn codex_declared_env_key_that_is_unset_outranks_a_bearer_token_on_the_same_table() {
        let config_path = codex_config_path();
        let auth_path = codex_auth_path();
        let config = r#"
model_provider = "relay"
[model_providers.relay]
base_url = "https://relay.example.test/v1"
env_key = "RELAY_KEY"
experimental_bearer_token = "tok-inline"
"#;
        let resolution = resolve_codex(
            &config_path,
            &auth_path,
            config,
            &json!({"OPENAI_API_KEY": "sk-auth"}),
            &env(&[]),
        );
        assert_eq!(resolution.credential, EffectiveCredential::Missing);
    }

    #[test]
    #[serial_test::serial]
    fn codex_builtin_openai_uses_openai_base_url_and_login() {
        let config_path = codex_config_path();
        let auth_path = codex_auth_path();
        let resolution = resolve_codex(
            &config_path,
            &auth_path,
            "",
            &json!({"tokens": {"access_token": "t"}}),
            &env(&[("OPENAI_BASE_URL", "https://proxy.example.test/v1")]),
        );
        assert_eq!(
            resolution.endpoint.as_deref(),
            Some("https://proxy.example.test/v1")
        );
        assert_eq!(
            resolution.endpoint_source,
            EffectiveConnectionSource::Environment {
                variable: "OPENAI_BASE_URL".to_string()
            }
        );
        assert_eq!(resolution.credential_source, live(".codex/auth.json"));
    }

    #[test]
    #[serial_test::serial]
    fn codex_experimental_bearer_token_is_a_file_credential() {
        let config_path = codex_config_path();
        let auth_path = codex_auth_path();
        let config = r#"
model_provider = "relay"
[model_providers.relay]
base_url = "https://relay.example.test/v1"
experimental_bearer_token = "tok-inline"
requires_openai_auth = false
"#;
        let resolution = resolve_codex(&config_path, &auth_path, config, &json!({}), &env(&[]));
        assert_eq!(resolution.credential, EffectiveCredential::Configured);
        assert_eq!(resolution.credential_source, live(".codex/config.toml"));
    }

    #[test]
    fn codex_provider_table_treats_a_blank_model_provider_as_the_builtin() {
        let table = codex_provider_table("model_provider = \"  \"\n");
        assert!(table.builtin_openai);
        assert_eq!(table.env_key.as_deref(), Some("OPENAI_API_KEY"));
    }

    // ---------- Gemini ----------

    #[test]
    #[serial_test::serial]
    fn gemini_shell_beats_the_dotenv_file() {
        let env_path = gemini_env_path();
        let dotenv: HashMap<String, String> = HashMap::from([
            (
                "GOOGLE_GEMINI_BASE_URL".to_string(),
                "https://file.example.test".to_string(),
            ),
            ("GEMINI_API_KEY".to_string(), "k-file".to_string()),
        ]);
        let resolution = resolve_gemini(
            &env_path,
            &dotenv,
            &env(&[("GOOGLE_GEMINI_BASE_URL", "https://shell.example.test")]),
        );
        assert_eq!(
            resolution.endpoint.as_deref(),
            Some("https://shell.example.test/")
        );
        assert_eq!(
            resolution.endpoint_source,
            EffectiveConnectionSource::Environment {
                variable: "GOOGLE_GEMINI_BASE_URL".to_string()
            }
        );
        assert_eq!(resolution.credential_source, live(".gemini/.env"));
    }

    // ---------- Grok ----------

    #[test]
    #[serial_test::serial]
    fn grok_reads_only_what_the_config_declares() {
        let path = grok_config_path();
        let config = r#"
[models]
default = "grok-4"

[model.grok-4]
model = "grok-4"
name = "grok-4"
base_url = "https://grok.example.test/v1"
env_key = "MY_GROK_KEY"
api_backend = "responses"
context_window = 100000
"#;
        let with_key = resolve_grok(
            &path,
            config,
            &env(&[("MY_GROK_KEY", "k"), ("XAI_API_KEY", "x")]),
        );
        assert_eq!(with_key.endpoint_source, live(".grok/config.toml"));
        assert_eq!(
            with_key.credential_source,
            EffectiveConnectionSource::Environment {
                variable: "MY_GROK_KEY".to_string()
            }
        );
        let without = resolve_grok(&path, config, &env(&[("XAI_API_KEY", "x")]));
        assert_eq!(without.credential, EffectiveCredential::Missing);
        let official = resolve_grok(&path, "", &env(&[]));
        assert_eq!(
            official.endpoint_source,
            EffectiveConnectionSource::ToolDefault
        );
        assert_eq!(official.credential, EffectiveCredential::ToolLogin);
    }

    // ---------- OpenCode ----------

    #[test]
    #[serial_test::serial]
    fn opencode_takes_the_provider_from_the_model_field() {
        let path = opencode_config_path();
        let config = json!({
            "model": "myrelay/gpt-5",
            "provider": {"myrelay": {"options": {"baseURL": "https://relay.example.test/v1", "apiKey": "{env:RELAY_KEY}"}}}
        });
        let resolution = resolve_opencode(&path, &config, &env(&[("RELAY_KEY", "k")]));
        assert_eq!(resolution.provider_hint.as_deref(), Some("myrelay"));
        assert_eq!(
            resolution.endpoint_source,
            live(".config/opencode/opencode.json")
        );
        assert_eq!(
            resolution.credential_source,
            EffectiveConnectionSource::Environment {
                variable: "RELAY_KEY".to_string()
            }
        );
        let builtin = resolve_opencode(
            &path,
            &json!({"model": "anthropic/claude-sonnet-4-5"}),
            &env(&[]),
        );
        assert_eq!(builtin.provider_hint.as_deref(), Some("anthropic"));
        assert_eq!(builtin.credential, EffectiveCredential::Unknown);
    }

    #[test]
    fn opencode_without_a_usable_model_does_not_light_an_official_card() {
        let path = PathBuf::from("/nonexistent/opencode.json");
        let resolution = resolve_opencode(&path, &json!({}), &env(&[]));
        assert_eq!(resolution.provider_hint, None);

        let mut providers = IndexMap::new();
        let official = claude_provider("claude-official", "official", None);
        providers.insert(official.id.clone(), official);

        assert_eq!(
            match_provider(ToolId::OpenCode, &resolution, &providers, "claude-official"),
            None
        );
    }

    // ---------- resolve_effective_connection ----------

    #[test]
    fn resolve_effective_connection_returns_none_for_tools_without_a_resolver() {
        // These tools have no resolver branch at all: the assertion never touches the
        // filesystem and takes the very first `return None` arm of the match.
        let environment = env(&[]);
        let providers: IndexMap<String, UpstreamProvider> = IndexMap::new();
        for tool in [
            ToolId::OpenClaw,
            ToolId::Hermes,
            ToolId::Pi,
            ToolId::KimiCode,
            ToolId::DeepSeekDsh,
        ] {
            assert_eq!(
                resolve_effective_connection(tool, &environment, &providers, ""),
                None
            );
        }
    }

    #[test]
    #[serial_test::serial]
    fn resolve_effective_connection_reads_claude_and_reports_shell_inspection() {
        let temp = tempfile::tempdir().expect("temp home");
        let _home = TestHome::set(temp.path());
        let claude_dir = temp.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).expect("create .claude dir");
        std::fs::write(
            claude_dir.join("settings.json"),
            json!({"env": {"ANTHROPIC_BASE_URL": "https://relay.example.test", "ANTHROPIC_AUTH_TOKEN": "sk-file"}}).to_string(),
        )
        .expect("write settings.json");

        let mut providers = IndexMap::new();
        let provider = claude_provider("relay", "custom", Some("https://relay.example.test"));
        providers.insert(provider.id.clone(), provider);

        let environment = ToolEnvironment::from_parts(
            ShellEnvironment::new(BTreeMap::new(), ShellEnvironmentSource::ProcessFallback),
            vec![],
        );

        let resolution =
            resolve_effective_connection(ToolId::ClaudeCode, &environment, &providers, "relay");

        assert_eq!(
            resolution,
            Some(EffectiveConnection {
                selection: crate::domain::EffectiveSelection::Configuration,
                model: None,
                endpoint: Some("https://relay.example.test/".to_string()),
                endpoint_source: live(".claude/settings.json"),
                credential: EffectiveCredential::Configured,
                credential_source: live(".claude/settings.json"),
                provider_id: Some("relay".to_string()),
                shell_inspected: false,
            })
        );
    }

    // ---------- provider matching ----------

    #[test]
    fn endpoint_matching_preserves_case_sensitive_paths() {
        assert!(super::same_endpoint(
            "https://EXAMPLE.test:443/v1/",
            "https://example.test/v1"
        ));
        assert!(!super::same_endpoint(
            "https://example.test/API",
            "https://example.test/api"
        ));
        assert!(!super::same_endpoint(
            "https://example.test/v1",
            "https://example.test:8443/v1"
        ));
    }

    #[test]
    fn opencode_stale_saved_url_does_not_match_live_provider_with_same_id() {
        let config = json!({"model":"relay/m","provider":{"relay":{"options":{"baseURL":"https://live.example"}}}});
        let result = resolve_opencode(PathBuf::from("config").as_path(), &config, &env(&[]));
        let saved = UpstreamProvider::with_id(
            "relay".to_string(),
            "Relay".to_string(),
            json!({"options":{"baseURL":"https://old.example"}}),
            None,
        );
        let providers = IndexMap::from([("relay".to_string(), saved)]);
        assert_eq!(
            match_provider(ToolId::OpenCode, &result, &providers, ""),
            None
        );
    }

    #[test]
    #[serial_test::serial]
    fn broken_live_configs_never_fall_back_to_official_selection() {
        let temp = tempfile::tempdir().unwrap();
        let _home = TestHome::set(temp.path());
        let paths = [
            (
                ToolId::ClaudeCode,
                crate::config::get_claude_settings_path(),
            ),
            (ToolId::Codex, crate::codex_config::get_codex_config_path()),
            (
                ToolId::GrokBuild,
                crate::grok_config::get_grok_config_path(),
            ),
            (
                ToolId::OpenCode,
                crate::opencode_config::get_opencode_config_path(),
            ),
        ];
        for (tool, path) in paths {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "{invalid[").unwrap();
            let mut providers = IndexMap::new();
            providers.insert(
                "official".to_string(),
                claude_provider("official", "official", None),
            );
            let result =
                resolve_effective_connection(tool, &env(&[]), &providers, "official").unwrap();
            assert_eq!(
                result.selection,
                crate::domain::EffectiveSelection::Unknown,
                "{tool:?}"
            );
            assert_eq!(result.provider_id, None);
        }
    }

    fn claude_provider(id: &str, category: &str, base_url: Option<&str>) -> UpstreamProvider {
        let mut provider = UpstreamProvider::with_id(
            id.to_string(),
            id.to_string(),
            match base_url {
                Some(url) => {
                    json!({"env": {"ANTHROPIC_BASE_URL": url, "ANTHROPIC_AUTH_TOKEN": "sk"}})
                }
                None => json!({"env": {}}),
            },
            None,
        );
        provider.category = Some(category.to_string());
        provider
    }

    #[test]
    #[serial_test::serial]
    fn matching_prefers_the_current_record_then_any_record_with_the_same_endpoint() {
        let path = claude_settings_path();
        let mut providers = IndexMap::new();
        for provider in [
            claude_provider("default", "custom", None),
            claude_provider("claude-official", "official", None),
            claude_provider("relay-a", "custom", Some("https://relay.example.test")),
            claude_provider("relay-b", "custom", Some("https://relay.example.test/")),
        ] {
            providers.insert(provider.id.clone(), provider);
        }
        let relay = resolve_claude(
            &path,
            &json!({"env": {"ANTHROPIC_BASE_URL": "https://relay.example.test/", "ANTHROPIC_AUTH_TOKEN": "sk"}}),
            &env(&[]),
        );
        assert_eq!(
            match_provider(ToolId::ClaudeCode, &relay, &providers, "relay-b").as_deref(),
            Some("relay-b")
        );
        assert_eq!(
            match_provider(ToolId::ClaudeCode, &relay, &providers, "default").as_deref(),
            Some("relay-a")
        );
        let official = resolve_claude(&path, &json!({"env": {}}), &env(&[]));
        assert_eq!(
            match_provider(ToolId::ClaudeCode, &official, &providers, "default").as_deref(),
            Some("claude-official")
        );
        let shell_only = resolve_claude(
            &path,
            &json!({}),
            &env(&[("ANTHROPIC_BASE_URL", "https://api.example.test")]),
        );
        assert_eq!(
            match_provider(ToolId::ClaudeCode, &shell_only, &providers, "default"),
            None
        );
    }

    // ---------- The probe target (ADR-0041) ----------
    //
    // Everything a saved service's probe needs is in the database; this
    // connection's is spread across a shell profile and the tool's own
    // configuration file, so these tests pin the one rule that matters: the
    // probe must send the same key the tool itself would send, and must refuse
    // rather than guess when it cannot read one.

    #[test]
    #[serial_test::serial]
    fn the_probe_target_carries_the_key_the_shell_exports() {
        let path = claude_settings_path();
        let resolution = resolve_claude(
            &path,
            &json!({}),
            &env(&[
                ("ANTHROPIC_BASE_URL", "https://relay.example.test"),
                ("ANTHROPIC_AUTH_TOKEN", "sk-from-the-shell"),
            ]),
        );
        assert_eq!(
            probe_target_from(resolution),
            Some((
                "https://relay.example.test/".to_string(),
                "sk-from-the-shell".to_string()
            ))
        );
    }

    #[test]
    #[serial_test::serial]
    fn the_settings_file_beats_the_shell_for_the_probe_too() {
        let path = claude_settings_path();
        let resolution = resolve_claude(
            &path,
            &json!({"env": {
                "ANTHROPIC_BASE_URL": "https://from-the-file.example.test",
                "ANTHROPIC_AUTH_TOKEN": "sk-from-the-file",
            }}),
            &env(&[
                ("ANTHROPIC_BASE_URL", "https://from-the-shell.example.test"),
                ("ANTHROPIC_AUTH_TOKEN", "sk-from-the-shell"),
            ]),
        );
        // Claude Code reads settings.json first, so a probe that used the shell
        // value here would report on a connection the tool never uses.
        assert_eq!(
            probe_target_from(resolution),
            Some((
                "https://from-the-file.example.test/".to_string(),
                "sk-from-the-file".to_string()
            ))
        );
    }

    #[test]
    #[serial_test::serial]
    fn an_address_with_no_key_is_still_testable() {
        let path = claude_settings_path();
        let resolution = resolve_claude(
            &path,
            &json!({}),
            &env(&[("ANTHROPIC_BASE_URL", "http://127.0.0.1:8080")]),
        );
        // A loopback gateway usually wants no key at all; refusing to test it
        // would make the most local setup the only untestable one.
        assert_eq!(
            probe_target_from(resolution),
            Some(("http://127.0.0.1:8080/".to_string(), String::new()))
        );
    }

    #[test]
    #[serial_test::serial]
    fn an_oauth_login_gives_no_probe_target() {
        let resolution = resolve_codex(
            &codex_config_path(),
            &codex_auth_path(),
            "",
            &json!({"tokens": {"access_token": "at"}}),
            &env(&[("OPENAI_BASE_URL", "https://api.example.test")]),
        );
        // The credential is real enough to display as configured, but there is
        // no bearer token to replay, so no probe could succeed.
        assert_eq!(resolution.credential, EffectiveCredential::Configured);
        assert_eq!(probe_target_from(resolution), None);
    }

    #[test]
    #[serial_test::serial]
    fn a_bearer_token_in_the_codex_config_becomes_the_probe_key() {
        let resolution = resolve_codex(
            &codex_config_path(),
            &codex_auth_path(),
            "model_provider = \"relay\"\n[model_providers.relay]\nbase_url = \"https://relay.example.test/v1\"\nexperimental_bearer_token = \"sk-bearer\"\n",
            &json!({}),
            &env(&[]),
        );
        assert_eq!(
            probe_target_from(resolution),
            Some((
                "https://relay.example.test/v1".to_string(),
                "sk-bearer".to_string()
            ))
        );
    }

    #[test]
    #[serial_test::serial]
    fn a_tool_running_on_its_own_login_has_nothing_to_probe() {
        let path = claude_settings_path();
        let resolution = resolve_claude(&path, &json!({"env": {}}), &env(&[]));
        assert_eq!(resolution.credential, EffectiveCredential::ToolLogin);
        assert_eq!(probe_target_from(resolution), None);
    }

    #[test]
    #[serial_test::serial]
    fn a_resolution_debug_rendering_never_carries_the_key() {
        let path = claude_settings_path();
        let resolution = resolve_claude(
            &path,
            &json!({}),
            &env(&[
                ("ANTHROPIC_BASE_URL", "https://relay.example.test"),
                ("ANTHROPIC_AUTH_TOKEN", "sk-must-not-be-logged"),
            ]),
        );
        let rendered = format!("{resolution:?}");
        assert!(!rendered.contains("sk-must-not-be-logged"), "{rendered}");
        assert!(rendered.contains("<redacted>"), "{rendered}");
    }
}
