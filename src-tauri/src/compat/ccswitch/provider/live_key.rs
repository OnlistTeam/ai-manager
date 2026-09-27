//! Changing one key in a tool's own configuration file (ADR-0055).
//!
//! Home writes a model or an effort into files the tool owns and the user may
//! edit by hand, so only the one key changes: JSON keeps its key order, TOML its
//! comments and layout, `.env` every other line. Every write follows the same
//! steps: read, validate, back up, change the one key, write a temporary file,
//! validate it, replace the original atomically, read it back to verify, and
//! put the backup back when the read-back disagrees.
//!
//! Nothing here logs a value or a line of the file: settings files hold keys.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use toml_edit::DocumentMut;

use crate::domain::{AppError, ErrorCode};

/// Where one setting lives inside its file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConfigKey {
    /// A string at this path of a JSON object, e.g. `["env", "ANTHROPIC_MODEL"]`.
    Json(&'static [&'static str]),
    /// A top-level string in a TOML document.
    Toml(&'static str),
    /// A `KEY=value` line of a dotenv file.
    DotEnv(&'static str),
}

fn read_failed(technical: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::ConfigParseFailed, "error.modelChoice.readFailed")
        .with_technical(technical)
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn write_failed(technical: impl Into<String>) -> AppError {
    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.modelChoice.writeFailed",
    )
    .with_technical(technical)
    .with_remediation("error.remediation.retryOrViewDetails")
}

// ---------- reading and changing text ----------

/// The value of `key` in `text`; an empty file has none. A file that does not
/// parse is an error, so nothing is ever written over a file this product
/// could not read.
pub(super) fn get(text: &str, key: ConfigKey) -> Result<Option<String>, AppError> {
    let value = match key {
        ConfigKey::Json(path) => json_get(&parse_json(text)?, path),
        ConfigKey::Toml(name) => parse_toml(text)?
            .get(name)
            .and_then(toml_edit::Item::as_str)
            .map(str::to_string),
        ConfigKey::DotEnv(name) => dotenv_lines(text)
            .into_iter()
            .rev()
            .find_map(|line| dotenv_value(line, name)),
    };
    Ok(value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

/// `text` with `key` set to `value`, or removed when `value` is `None`.
pub(super) fn set(text: &str, key: ConfigKey, value: Option<&str>) -> Result<String, AppError> {
    match key {
        ConfigKey::Json(path) => {
            let mut root = parse_json(text)?;
            json_set(&mut root, path, value)?;
            let mut rendered = serde_json::to_string_pretty(&root)
                .map_err(|error| write_failed(format!("serialize JSON: {error}")))?;
            if text.is_empty() || text.ends_with('\n') {
                rendered.push('\n');
            }
            Ok(rendered)
        }
        ConfigKey::Toml(name) => {
            let mut doc = parse_toml(text)?;
            toml_set(&mut doc, name, value);
            Ok(doc.to_string())
        }
        ConfigKey::DotEnv(name) => Ok(dotenv_set(text, name, value)),
    }
}

fn parse_json(text: &str) -> Result<Value, AppError> {
    if text.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let value: Value = serde_json::from_str(text)
        .map_err(|error| read_failed(format!("JSON did not parse at line {}", error.line())))?;
    if !value.is_object() {
        return Err(read_failed("JSON root is not an object"));
    }
    Ok(value)
}

fn parse_toml(text: &str) -> Result<DocumentMut, AppError> {
    text.parse::<DocumentMut>()
        .map_err(|_| read_failed("TOML did not parse"))
}

pub(super) fn json_get(root: &Value, path: &[&str]) -> Option<String> {
    path.iter()
        .try_fold(root, |node, segment| node.get(*segment))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Sets or removes a string at `path`, creating the objects on the way when
/// setting. Removing leaves the parent objects in place.
pub(super) fn json_set(
    root: &mut Value,
    path: &[&str],
    value: Option<&str>,
) -> Result<(), AppError> {
    let (last, parents) = path
        .split_last()
        .ok_or_else(|| write_failed("empty JSON path"))?;
    let mut node = root;
    for segment in parents {
        let object = node
            .as_object_mut()
            .ok_or_else(|| write_failed("a JSON parent is not an object"))?;
        if value.is_none() && !object.contains_key(*segment) {
            return Ok(());
        }
        node = object
            .entry(segment.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
    }
    let object = node
        .as_object_mut()
        .ok_or_else(|| write_failed("a JSON parent is not an object"))?;
    match value {
        Some(value) => {
            object.insert(last.to_string(), Value::String(value.to_string()));
        }
        None => {
            object.remove(*last);
        }
    }
    Ok(())
}

/// Replaces the value in place, so a comment after it on the same line stays.
pub(super) fn toml_set(doc: &mut DocumentMut, name: &str, value: Option<&str>) {
    let Some(value) = value else {
        doc.remove(name);
        return;
    };
    match doc.get_mut(name).and_then(toml_edit::Item::as_value_mut) {
        Some(existing) => {
            let decor = existing.decor().clone();
            *existing = toml_edit::Value::from(value);
            *existing.decor_mut() = decor;
        }
        None => {
            doc.insert(name, toml_edit::value(value));
        }
    }
}

/// Lines with their terminators, so rewriting one cannot normalise the rest.
fn dotenv_lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// The name a `KEY=value` or `export KEY=value` line assigns.
fn dotenv_name(line: &str) -> Option<&str> {
    let body = line.trim_start();
    let body = body.strip_prefix("export ").unwrap_or(body);
    let (name, _) = body.split_once('=')?;
    Some(name.trim())
}

fn dotenv_value(line: &str, name: &str) -> Option<String> {
    if dotenv_name(line)? != name {
        return None;
    }
    let (_, raw) = line.split_once('=')?;
    let raw = raw.trim();
    let unquoted = ['"', '\'']
        .into_iter()
        .find_map(|quote| {
            raw.strip_prefix(quote)
                .and_then(|rest| rest.strip_suffix(quote))
        })
        .unwrap_or(raw);
    Some(unquoted.to_string())
}

fn dotenv_set(text: &str, name: &str, value: Option<&str>) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut written = false;
    for line in dotenv_lines(text) {
        if dotenv_name(line) != Some(name) {
            lines.push(line.to_string());
            continue;
        }
        // The first assignment is rewritten in place; later duplicates would
        // override it, so they go.
        if let (Some(value), false) = (value, written) {
            let ending = if line.ends_with("\r\n") {
                "\r\n"
            } else if line.ends_with('\n') {
                "\n"
            } else {
                ""
            };
            let indent = &line[..line.len() - line.trim_start().len()];
            let export = if line.trim_start().starts_with("export ") {
                "export "
            } else {
                ""
            };
            lines.push(format!("{indent}{export}{name}={value}{ending}"));
            written = true;
        }
    }
    if let (Some(value), false) = (value, written) {
        if lines.last().is_some_and(|line| !line.ends_with('\n')) {
            lines.push("\n".to_string());
        }
        lines.push(format!("{name}={value}\n"));
    }
    lines.concat()
}

// ---------- writing the file ----------

/// The file as it is now; a missing file reads as empty.
pub(super) fn read_file(path: &Path) -> Result<Option<String>, AppError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(read_failed(format!("read failed: {:?}", error.kind()))),
    }
}

/// Sets one key in one file, or removes it, and proves the file says so.
pub(super) fn write(
    path: &Path,
    backups: &Path,
    key: ConfigKey,
    value: Option<&str>,
) -> Result<(), AppError> {
    let original = read_file(path)?;
    let before = original.as_deref().unwrap_or("");
    // Refuses a file that does not parse before anything is touched.
    get(before, key)?;
    let after = set(before, key, value)?;
    if after == before {
        return Ok(());
    }
    if get(&after, key)?.as_deref() != value {
        return Err(write_failed("the changed text does not hold the new value"));
    }
    let backup = match &original {
        Some(_) => Some(back_up(path, backups)?),
        None => None,
    };
    replace_atomically(path, &after)?;

    let confirmed = read_file(path)
        .ok()
        .flatten()
        .is_some_and(|text| get(&text, key).ok().flatten().as_deref() == value);
    if confirmed {
        return Ok(());
    }
    let restored = match &original {
        Some(text) => replace_atomically(path, text).is_ok(),
        None => std::fs::remove_file(path).is_ok(),
    };
    Err(write_failed(format!(
        "the file did not read back with the new value; original {}{}",
        if restored { "restored" } else { "not restored" },
        backup
            .map(|backup| format!(", backup at {}", backup.display()))
            .unwrap_or_default()
    )))
}

/// Copies the file as it is now into the backup directory, so a bad edit can
/// be undone by hand.
fn back_up(path: &Path, backups: &Path) -> Result<PathBuf, AppError> {
    std::fs::create_dir_all(backups)
        .map_err(|error| write_failed(format!("backup directory: {:?}", error.kind())))?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "config".to_string());
    let target = backups.join(format!("{name}.{stamp}.bak"));
    std::fs::copy(path, &target)
        .map_err(|error| write_failed(format!("backup copy: {:?}", error.kind())))?;
    Ok(target)
}

/// A temporary file in the same directory, renamed over the original. The
/// original's permissions are kept; a new file is private, because these files
/// hold keys.
fn replace_atomically(path: &Path, contents: &str) -> Result<(), AppError> {
    let directory = path
        .parent()
        .ok_or_else(|| write_failed("the file has no parent directory"))?;
    std::fs::create_dir_all(directory)
        .map_err(|error| write_failed(format!("create directory: {:?}", error.kind())))?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".ai-manager-")
        .suffix(".tmp")
        .tempfile_in(directory)
        .map_err(|error| write_failed(format!("temporary file: {:?}", error.kind())))?;
    temporary
        .write_all(contents.as_bytes())
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| write_failed(format!("temporary write: {:?}", error.kind())))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path)
            .map(|metadata| metadata.permissions().mode())
            .unwrap_or(0o600);
        temporary
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(mode))
            .map_err(|error| write_failed(format!("permissions: {:?}", error.kind())))?;
    }

    temporary
        .persist(path)
        .map_err(|error| write_failed(format!("replace: {:?}", error.error.kind())))?;
    Ok(())
}

#[cfg(test)]
#[path = "live_key_tests.rs"]
mod tests;
