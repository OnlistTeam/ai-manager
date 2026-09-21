//! Product-safe projection of the retained CC Switch session scanners.
//!
//! The retained scanners understand each tool's on-disk format, but their
//! legacy DTO exposes local paths, provider session ids and shell commands.
//! This facade deliberately converts those records into the narrow product
//! domain before they can reach a command.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::compat::ccswitch::tools::tool_id_to_cli_name;
use crate::domain::{
    AppError, ErrorCode, SessionList, SessionMessage, SessionMessageRole, SessionSummary,
    SessionThread, ToolId,
};
use crate::session_manager::{self, SessionMeta};

const MAX_LIST_ITEMS: usize = 500;
const MAX_QUERY_CHARS: usize = 200;
const MAX_MESSAGES: usize = 500;
const MAX_MESSAGE_BYTES: usize = 64 * 1024;
const MAX_THREAD_BYTES: usize = 1024 * 1024;
/// A single scan opens and parses every session file on this machine, while one round of
/// interaction (typing to search, opening a session, resuming) fires several requests in a row.
/// The cache folds them into one disk scan; the TTL is short enough that pressing "refresh" always
/// rescans.
const SCAN_CACHE_TTL: Duration = Duration::from_secs(15);

struct ScanCache {
    scanned_at: Instant,
    sessions: Arc<Vec<SessionMeta>>,
}

static SCAN_CACHE: LazyLock<Mutex<Option<ScanCache>>> = LazyLock::new(|| Mutex::new(None));

fn scanned_sessions() -> Arc<Vec<SessionMeta>> {
    let mut cache = SCAN_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(cached) = cache.as_ref() {
        if cached.scanned_at.elapsed() < SCAN_CACHE_TTL {
            return Arc::clone(&cached.sessions);
        }
    }
    let sessions = Arc::new(session_manager::scan_sessions());
    *cache = Some(ScanCache {
        scanned_at: Instant::now(),
        sessions: Arc::clone(&sessions),
    });
    sessions
}

pub struct SessionStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionResumeTarget {
    pub tool: ToolId,
    pub args: Vec<String>,
    pub working_directory: PathBuf,
}

impl SessionStore {
    pub fn list(query: Option<&str>, tool: Option<ToolId>) -> Result<SessionList, AppError> {
        let query = normalize_query(query)?;
        let mut items = Vec::new();
        let mut seen = HashSet::new();
        let mut total_count = 0_usize;

        for raw in scanned_sessions().iter() {
            let Some(summary) = summary_from_meta(raw) else {
                continue;
            };
            if tool.is_some_and(|selected| selected != summary.tool)
                || !matches_query(raw, &summary, query.as_deref())
                || !seen.insert(summary.reference.clone())
            {
                continue;
            }
            total_count = total_count.saturating_add(1);
            if items.len() < MAX_LIST_ITEMS {
                items.push(summary);
            }
        }

        Ok(SessionList {
            limited: total_count > items.len(),
            total_count: bounded_count(total_count),
            items,
        })
    }

    pub fn thread(reference: &str) -> Result<SessionThread, AppError> {
        let raw = resolve(reference)?;
        let source_path = raw.source_path.as_deref().ok_or_else(session_not_found)?;
        let messages =
            session_manager::load_messages(&raw.provider_id, source_path).map_err(|e| {
                log::warn!(
                    "Session thread read failed for provider={} reference={reference}: {e}",
                    raw.provider_id
                );
                AppError::new(ErrorCode::Internal, "error.session.readFailed")
                    .with_remediation("error.remediation.retryOrViewDetails")
            })?;
        let total_count = messages.len();
        let mut selected = Vec::new();
        let mut used_bytes = 0_usize;
        let mut limited = false;

        for raw_message in messages.iter().rev() {
            if selected.len() >= MAX_MESSAGES || used_bytes >= MAX_THREAD_BYTES {
                limited = true;
                break;
            }
            let available = MAX_THREAD_BYTES - used_bytes;
            let limit = MAX_MESSAGE_BYTES.min(available);
            let (content, truncated) = sanitize_body(&raw_message.content, limit);
            used_bytes = used_bytes.saturating_add(content.len());
            limited |= truncated;
            selected.push(SessionMessage {
                role: message_role(&raw_message.role),
                content,
                timestamp: raw_message.ts,
                truncated,
            });
        }
        selected.reverse();
        limited |= selected.len() < total_count;

        // A failed resume only happens inside the terminal, where the manager cannot see the error
        // line. Handing over the working directory together with a command the user can type means
        // they can at least reproduce and investigate it themselves.
        let resume = resume_target_from_meta(&raw).ok();
        Ok(SessionThread {
            reference: reference.to_string(),
            messages: selected,
            total_count: bounded_count(total_count),
            limited,
            working_directory: resume
                .as_ref()
                .map(|target| target.working_directory.to_string_lossy().into_owned()),
            resume_command: resume.as_ref().map(resume_command),
        })
    }

    pub(crate) fn resume_target(reference: &str) -> Result<SessionResumeTarget, AppError> {
        resume_target_from_meta(&resolve(reference)?)
    }

    pub(crate) fn reveal_target(reference: &str) -> Result<PathBuf, AppError> {
        reveal_target_from_meta(&resolve(reference)?)
    }
}

/// A manual resume command line the user can paste straight into a terminal. It is for display
/// only: the real resume still starts from the opaque reference and the backend rebuilds the argv
/// from the per-tool allowlist, so a command string handed back by the renderer is never executed.
fn resume_command(target: &SessionResumeTarget) -> String {
    let program = tool_id_to_cli_name(target.tool);
    let directory = target.working_directory.to_string_lossy();
    let arguments = target
        .args
        .iter()
        .map(|argument| shell_word(argument))
        .collect::<Vec<_>>()
        .join(" ");
    if cfg!(windows) {
        format!(
            "Set-Location -LiteralPath '{}'; {program} {arguments}",
            directory.replace('\'', "''")
        )
    } else {
        format!("cd {} && {program} {arguments}", shell_word(&directory))
    }
}

/// A single word that is safe in a POSIX shell: harmless characters are kept verbatim and everything else is wrapped in single quotes.
fn shell_word(value: &str) -> String {
    let plain = value.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | '/' | ':' | '=')
    });
    if plain && !value.is_empty() {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', r"'\''"))
}

fn reveal_target_from_meta(session: &SessionMeta) -> Result<PathBuf, AppError> {
    session
        .source_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(session_not_found)
}

fn normalize_query(query: Option<&str>) -> Result<Option<String>, AppError> {
    let query = query.unwrap_or_default().trim();
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(AppError::new(
            ErrorCode::ConfigParseFailed,
            "error.session.queryInvalid",
        ));
    }
    Ok((!query.is_empty()).then(|| query.to_lowercase()))
}

fn resolve(reference: &str) -> Result<SessionMeta, AppError> {
    if !valid_reference(reference) {
        return Err(session_not_found());
    }
    scanned_sessions()
        .iter()
        .find(|session| reference_for(session) == reference)
        .cloned()
        .ok_or_else(session_not_found)
}

fn session_not_found() -> AppError {
    AppError::new(ErrorCode::SessionNotFound, "error.session.notFound")
        .with_remediation("error.remediation.refreshSessions")
}

fn valid_reference(reference: &str) -> bool {
    reference.len() == 64
        && reference
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn reference_for(session: &SessionMeta) -> String {
    let mut hasher = Sha256::new();
    hasher.update(session.provider_id.as_bytes());
    hasher.update([0]);
    hasher.update(session.session_id.as_bytes());
    hasher.update([0]);
    hasher.update(
        session
            .source_path
            .as_deref()
            .unwrap_or_default()
            .as_bytes(),
    );
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn summary_from_meta(session: &SessionMeta) -> Option<SessionSummary> {
    let tool = tool_for_provider(&session.provider_id)?;
    Some(SessionSummary {
        reference: reference_for(session),
        tool,
        title: session
            .title
            .as_deref()
            .and_then(|value| sanitize_inline(value, 160)),
        preview: session
            .summary
            .as_deref()
            .and_then(|value| sanitize_inline(value, 280)),
        project_name: session
            .project_dir
            .as_deref()
            .and_then(project_basename)
            .and_then(|value| sanitize_inline(value, 120)),
        created_at: session.created_at,
        last_active_at: session.last_active_at,
        resumable: supports_resume(tool),
    })
}

fn matches_query(raw: &SessionMeta, summary: &SessionSummary, query: Option<&str>) -> bool {
    let Some(query) = query else {
        return true;
    };
    summary
        .title
        .iter()
        .chain(summary.preview.iter())
        .chain(summary.project_name.iter())
        .any(|value| value.to_lowercase().contains(query))
        || raw.session_id.to_lowercase().contains(query)
}

fn tool_for_provider(provider: &str) -> Option<ToolId> {
    match provider {
        "claude" => Some(ToolId::ClaudeCode),
        "codex" => Some(ToolId::Codex),
        "opencode" => Some(ToolId::OpenCode),
        "gemini" => Some(ToolId::GeminiCli),
        "grokbuild" => Some(ToolId::GrokBuild),
        "openclaw" => Some(ToolId::OpenClaw),
        "hermes" => Some(ToolId::Hermes),
        "pi" => Some(ToolId::Pi),
        _ => None,
    }
}

fn supports_resume(tool: ToolId) -> bool {
    matches!(
        tool,
        ToolId::ClaudeCode
            | ToolId::Codex
            | ToolId::OpenCode
            | ToolId::GeminiCli
            | ToolId::GrokBuild
            | ToolId::Pi
    )
}

fn resume_target_from_meta(session: &SessionMeta) -> Result<SessionResumeTarget, AppError> {
    let tool = tool_for_provider(&session.provider_id).ok_or_else(session_not_found)?;
    let args = match tool {
        ToolId::ClaudeCode => vec!["--resume".to_string(), session.session_id.clone()],
        ToolId::Codex => vec!["resume".to_string(), session.session_id.clone()],
        ToolId::OpenCode => vec!["-s".to_string(), session.session_id.clone()],
        ToolId::GeminiCli | ToolId::GrokBuild => {
            vec!["--resume".to_string(), session.session_id.clone()]
        }
        ToolId::Pi => vec![
            "--session".to_string(),
            session.source_path.clone().ok_or_else(session_not_found)?,
        ],
        ToolId::OpenClaw | ToolId::Hermes | ToolId::KimiCode | ToolId::DeepSeekDsh => {
            return Err(AppError::new(
                ErrorCode::LaunchFailed,
                "error.session.resumeUnsupported",
            ))
        }
    };

    Ok(SessionResumeTarget {
        tool,
        args,
        working_directory: safe_working_directory(session.project_dir.as_deref()),
    })
}

fn safe_working_directory(project: Option<&str>) -> PathBuf {
    if let Some(path) = project.map(PathBuf::from) {
        if path.is_dir() {
            if let Ok(canonical) = path.canonicalize() {
                return canonical;
            }
        }
    }
    let home = crate::config::get_home_dir();
    home.canonicalize().unwrap_or(home)
}

fn project_basename(path: &str) -> Option<&str> {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
}

fn sanitize_inline(value: &str, max_chars: usize) -> Option<String> {
    let clean = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let clean = clean.chars().take(max_chars).collect::<String>();
    (!clean.is_empty()).then_some(clean)
}

fn sanitize_body(value: &str, max_bytes: usize) -> (String, bool) {
    let mut clean = String::new();
    let mut truncated = false;
    for character in value.chars() {
        let character = match character {
            '\r' => '\n',
            '\n' | '\t' => character,
            value if value.is_control() => continue,
            value => value,
        };
        if clean.len() + character.len_utf8() > max_bytes {
            truncated = true;
            break;
        }
        clean.push(character);
    }
    (clean, truncated)
}

fn message_role(role: &str) -> SessionMessageRole {
    match role.trim().to_ascii_lowercase().as_str() {
        "user" => SessionMessageRole::User,
        "assistant" => SessionMessageRole::Assistant,
        "system" => SessionMessageRole::System,
        "tool" | "function" => SessionMessageRole::Tool,
        _ => SessionMessageRole::Other,
    }
}

fn bounded_count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
#[path = "session/tests.rs"]
mod tests;
