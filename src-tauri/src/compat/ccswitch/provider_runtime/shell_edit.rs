//! Replacing one value on one line of a shell start-up file.
//!
//! This is the only place in the product that writes to a file the user owns
//! but this application does not, so the rules are stricter than anywhere else:
//!
//! * The line must still say what the user was shown. The dialog's view can go
//!   stale — an editor left open, a dotfile manager, a second window — and a
//!   blind write would then change a line the user never read. Every write
//!   therefore re-reads and re-checks first.
//! * Only the value changes. The keyword, the variable name, the quoting style,
//!   the indentation, any trailing comment, the line ending and the presence or
//!   absence of a final newline are all preserved, because the diff the user
//!   sees in their own dotfile repository should be one value.
//! * The original content is copied to the backup directory before anything is
//!   written, and the write is a temporary file plus a rename, so an
//!   interrupted write cannot leave a half-written profile — a truncated
//!   `.zshrc` breaks every future terminal.
//! * The result is read back and verified. Reporting success without checking
//!   would be the one failure the user discovers days later.
//!
//! Upstream's `services/env_manager.rs` does none of this: it filters out every
//! line matching the variable name regardless of which line was located,
//! rebuilds the file with `lines().join("\n")` — dropping a trailing newline
//! and silently converting CRLF to LF — writes non-atomically, and "backs up"
//! a JSON of names and values rather than the file, so a bad edit cannot be
//! undone. None of it is reused.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::domain::{AppError, ErrorCode};

use super::shell_files::{locate_variable, split_value_and_comment};

/// What a caller must already know for the write to be allowed, so that it can
/// only ever change the line it looked at.
pub(crate) struct ShellVariableEdit<'a> {
    pub variable: &'a str,
    /// 1-based, as reported by the locate step.
    pub line: u32,
    /// The value the caller last saw on that line.
    pub expected_value: &'a str,
    pub new_value: &'a str,
}

/// The write was refused before touching anything.
fn refused(message_key: &'static str, technical: &'static str) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, message_key)
        .with_technical(technical)
        .with_remediation("error.remediation.checkServiceSettings")
}

/// The line the caller looked at is not the line on disk any more, which is a
/// conflict rather than a failure: re-reading and looking again is the fix.
fn moved_on(technical: &'static str) -> AppError {
    AppError::new(ErrorCode::OperationConflict, "error.shellVariable.movedOn")
        .with_technical(technical)
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn failed(message_key: &'static str, technical: String) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, message_key)
        .with_technical(technical)
        .with_remediation("error.remediation.retryOrViewDetails")
}

/// Rejects a value that would change the *shape* of the line rather than its
/// content. A newline would split one assignment into two statements, and a
/// control character in a profile is never intentional.
fn validate_new_value(value: &str) -> Result<(), AppError> {
    if value.chars().any(|character| {
        character == '\n' || character == '\r' || (character.is_control() && character != '\t')
    }) {
        return Err(refused(
            "error.shellVariable.valueInvalid",
            "replacement value contains a line break or control character",
        ));
    }
    if value.chars().count() > 4096 {
        return Err(refused(
            "error.shellVariable.valueInvalid",
            "replacement value is too long for a profile line",
        ));
    }
    Ok(())
}

/// Splits a file into lines that remember their own terminator, so rewriting
/// one of them cannot normalise the others.
fn split_keeping_endings(contents: &str) -> Vec<(&str, &str)> {
    let mut parts = Vec::new();
    let mut rest = contents;
    while !rest.is_empty() {
        match rest.find('\n') {
            Some(index) => {
                let (body, ending) = if index > 0 && rest.as_bytes()[index - 1] == b'\r' {
                    (&rest[..index - 1], "\r\n")
                } else {
                    (&rest[..index], "\n")
                };
                parts.push((body, ending));
                rest = &rest[index + 1..];
            }
            None => {
                parts.push((rest, ""));
                rest = "";
            }
        }
    }
    parts
}

/// Rebuilds one assignment with a new value, keeping everything around it.
///
/// The quoting style is kept because it is a choice the user made, and single
/// quotes carry meaning: a value put back unquoted that contains a space would
/// stop being one word.
fn rewrite_line(line: &str, variable: &str, new_value: &str) -> Option<String> {
    let indent_len = line.len() - line.trim_start().len();
    let (indent, body) = line.split_at(indent_len);

    // fish: `set -x NAME value`
    if body.trim_start().starts_with("set ") {
        let after_set = body.trim_start().strip_prefix("set ")?;
        let name_at = after_set.find(variable)?;
        let (prefix, tail) = after_set.split_at(name_at + variable.len());
        let value_start = tail.len() - tail.trim_start().len();
        let (value, trailing) = split_value_and_comment(tail);
        let quote = quote_style(value);
        return Some(format!(
            "{indent}set {prefix}{}{}{trailing}",
            &tail[..value_start],
            wrap(new_value, quote),
        ));
    }

    let (head, raw) = body.split_once('=')?;
    if !head.trim_end().ends_with(variable) {
        return None;
    }
    let value_start = raw.len() - raw.trim_start().len();
    let (value, trailing) = split_value_and_comment(raw);
    let quote = quote_style(value);
    Some(format!(
        "{indent}{head}={}{}{trailing}",
        &raw[..value_start],
        wrap(new_value, quote),
    ))
}

fn quote_style(raw: &str) -> Option<char> {
    let trimmed = raw.trim();
    match (trimmed.chars().next(), trimmed.chars().last()) {
        (Some('\''), Some('\'')) if trimmed.len() >= 2 => Some('\''),
        (Some('"'), Some('"')) if trimmed.len() >= 2 => Some('"'),
        _ => None,
    }
}

/// An unquoted value that needs quoting gets it, because leaving a space
/// unquoted would change one argument into two.
fn wrap(value: &str, quote: Option<char>) -> String {
    match quote {
        Some('\'') => format!("'{value}'"),
        Some('"') => format!("\"{value}\""),
        _ if value.is_empty()
            || value.chars().any(|character| {
                character.is_whitespace() || "#$`\"'\\&|;<>()".contains(character)
            }) =>
        {
            format!("'{value}'")
        }
        _ => value.to_string(),
    }
}

/// Writes `contents` to `path` through a temporary file in the same directory,
/// preserving the original permissions.
fn write_atomically(path: &Path, contents: &str) -> Result<(), AppError> {
    let directory = path.parent().ok_or_else(|| {
        refused(
            "error.shellVariable.writeFailed",
            "start-up file has no parent directory",
        )
    })?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".ai-manager-")
        .suffix(".tmp")
        .tempfile_in(directory)
        .map_err(|error| failed("error.shellVariable.writeFailed", error.to_string()))?;
    temporary
        .write_all(contents.as_bytes())
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| failed("error.shellVariable.writeFailed", error.to_string()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            let mode = metadata.permissions().mode();
            let _ = temporary
                .as_file()
                .set_permissions(std::fs::Permissions::from_mode(mode));
        }
    }

    temporary
        .persist(path)
        .map_err(|error| failed("error.shellVariable.writeFailed", error.error.to_string()))?;
    Ok(())
}

/// Copies the file as it is now into the backup directory, so a bad edit can be
/// undone by hand. Returns the backup's own path.
fn back_up(path: &Path, backups: &Path) -> Result<PathBuf, AppError> {
    std::fs::create_dir_all(backups)
        .map_err(|error| failed("error.shellVariable.writeFailed", error.to_string()))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "profile".to_string());
    let target = backups.join(format!("{name}.{stamp}.bak"));
    std::fs::copy(path, &target)
        .map_err(|error| failed("error.shellVariable.writeFailed", error.to_string()))?;
    Ok(target)
}

/// Replaces one value on one line, after confirming the line still says what
/// the caller was shown.
///
/// `home` bounds what may be written: a start-up file outside the user's home
/// is a system file that this application will locate and display but never
/// modify.
pub(crate) fn write_variable(
    home: &Path,
    backups: &Path,
    edit: ShellVariableEdit<'_>,
) -> Result<PathBuf, AppError> {
    validate_new_value(edit.new_value)?;

    let site = locate_variable(home, edit.variable)
        .into_iter()
        .find(|candidate| candidate.line == edit.line && candidate.value == edit.expected_value)
        .ok_or_else(|| moved_on("the located line no longer holds the expected value"))?;
    if !site.rewritable {
        return Err(refused(
            "error.shellVariable.notRewritable",
            "the assignment is not a plain literal",
        ));
    }
    let real = std::fs::canonicalize(&site.path)
        .map_err(|error| failed("error.shellVariable.writeFailed", error.to_string()))?;
    let real_home = std::fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
    if !real.starts_with(&real_home) {
        return Err(refused(
            "error.shellVariable.outsideHome",
            "the assignment lives outside the user's home directory",
        ));
    }

    let contents = std::fs::read_to_string(&real)
        .map_err(|error| failed("error.shellVariable.writeFailed", error.to_string()))?;
    let mut parts = split_keeping_endings(&contents);
    let index = usize::try_from(edit.line)
        .ok()
        .and_then(|line| line.checked_sub(1))
        .filter(|index| *index < parts.len())
        .ok_or_else(|| moved_on("the located line is past the end of the file"))?;

    let replacement = rewrite_line(parts[index].0, &site.variable, edit.new_value)
        .ok_or_else(|| moved_on("the located line is no longer an assignment of this variable"))?;
    let backup = back_up(&real, backups)?;
    let ending = parts[index].1;
    parts[index] = (replacement.as_str(), ending);
    let rebuilt: String = parts
        .iter()
        .map(|(body, ending)| format!("{body}{ending}"))
        .collect();

    write_atomically(&real, &rebuilt)?;

    // Read back rather than trust the write: the edit is only done when the
    // file on disk says so.
    let confirmed = locate_variable(home, edit.variable)
        .into_iter()
        .any(|candidate| candidate.line == edit.line && candidate.value == edit.new_value);
    if !confirmed {
        return Err(failed(
            "error.shellVariable.writeFailed",
            format!(
                "the rewritten line did not read back as expected; the original is at {}",
                backup.display()
            ),
        ));
    }
    Ok(site.path)
}

#[cfg(test)]
mod tests;
