//! Where a shell variable is actually written down.
//!
//! The upstream scanner (`services/env_checker.rs`) reads a fixed list of seven
//! start-up files and stops there. That is enough to say "something in your
//! shell sets this", but not enough to offer to change it: the common setup is
//! a small `.zshrc` that `source`s a secrets file, and the variable is in the
//! sourced file, which upstream never opens. A line this product cannot point
//! at is a line this product must not offer to edit, so following `source` is
//! the prerequisite for the edit action rather than a nicety.
//!
//! What this deliberately is not: a shell. It does not expand arbitrary
//! parameters, run command substitutions, or evaluate conditionals. It reads
//! assignments that are plain literals and reports everything else as located
//! but not rewritable — the login shell remains the authority on the *value*
//! (see `environment.rs`), and this module only answers "which line produced
//! it".

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// How deep a chain of `source` directives is followed. Two levels covers
/// `.zshrc` → `secrets.zsh` → `work.zsh`; beyond that a start-up file is doing
/// something this product should not try to model.
const MAX_DEPTH: usize = 3;
/// A start-up file is a few kilobytes. Anything larger is not a profile.
const MAX_FILE_BYTES: u64 = 512 * 1024;
/// Bounds a pathological dotfile tree without bounding any real one.
const MAX_FILES: usize = 64;

/// One assignment found in a start-up file.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ShellVariableSite {
    pub variable: String,
    /// The file the assignment is written in, as read from disk.
    pub path: PathBuf,
    /// 1-based, so it matches what an editor shows.
    pub line: u32,
    /// The value as written, with one layer of quoting removed.
    pub value: String,
    /// The value is a plain literal, so replacing it cannot change what the
    /// line *does*. False for command substitutions and interpolations, where
    /// rewriting the text would silently rewrite behaviour.
    pub rewritable: bool,
}

/// Redacted: a site names a file and a variable, and its value is a credential
/// as often as not.
impl std::fmt::Debug for ShellVariableSite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShellVariableSite")
            .field("variable", &self.variable)
            .field("path", &"<path>")
            .field("line", &self.line)
            .field("value", &"<redacted>")
            .field("rewritable", &self.rewritable)
            .finish()
    }
}

/// The start-up files a login shell reads, before any `source` directive.
///
/// Wider than upstream's list on purpose: `.zshenv` is where a variable that
/// must exist for non-interactive shells goes, `.zlogin` runs last and so wins,
/// and fish keeps its configuration somewhere else entirely.
fn base_files(home: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = [
        ".zshenv",
        ".zprofile",
        ".zshrc",
        ".zlogin",
        ".bash_profile",
        ".bashrc",
        ".profile",
    ]
    .iter()
    .map(|name| home.join(name))
    .collect();
    files.push(home.join(".config").join("fish").join("config.fish"));
    files.extend(
        [
            "/etc/zshenv",
            "/etc/zprofile",
            "/etc/profile",
            "/etc/bashrc",
        ]
        .iter()
        .map(PathBuf::from),
    );
    files
}

fn read_start_up_file(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// Strips one layer of matched quotes and reports whether what is left is a
/// literal.
///
/// A double-quoted value still interpolates, so `"$TOKEN"` and
/// `"$(pass show x)"` are located but not rewritable; a single-quoted or bare
/// value with no shell metacharacters is safe to replace.
fn unquote(raw: &str) -> (String, bool) {
    let trimmed = raw.trim();
    let (inner, was_single) = match (trimmed.chars().next(), trimmed.chars().last()) {
        (Some('\''), Some('\'')) if trimmed.len() >= 2 => (&trimmed[1..trimmed.len() - 1], true),
        (Some('"'), Some('"')) if trimmed.len() >= 2 => (&trimmed[1..trimmed.len() - 1], false),
        _ => (trimmed, false),
    };
    let dynamic = if was_single {
        // Single quotes suppress everything, so only an embedded quote would
        // make a rewrite ambiguous.
        inner.contains('\'')
    } else {
        inner.contains('$')
            || inner.contains('`')
            || inner.contains('"')
            || inner.contains('\'')
            || inner.contains('\\')
    };
    (inner.to_string(), !dynamic && !inner.is_empty())
}

/// Splits the right-hand side of an assignment into the value token and
/// whatever follows it.
///
/// A quoted value ends at its closing quote, so `"https://x" # relay` is a
/// value and a comment rather than one long string — getting this wrong made
/// the value unrecognisable and the line un-editable, which is exactly the
/// failure this module exists to avoid.
pub(super) fn split_value_and_comment(raw: &str) -> (&str, &str) {
    let trimmed = raw.trim_start();
    let quote = match trimmed.chars().next() {
        Some(character @ ('\'' | '"')) => character,
        _ => {
            return match trimmed.split_once(" #") {
                Some((value, rest)) => (value.trim_end(), rest),
                None => (trimmed.trim_end(), ""),
            }
        }
    };
    match trimmed[1..].find(quote) {
        Some(offset) => {
            let end = offset + 2;
            (&trimmed[..end], &trimmed[end..])
        }
        // An unbalanced quote is not something to guess at.
        None => (trimmed.trim_end(), ""),
    }
}

/// A trailing `# comment` is not part of the value.
fn strip_trailing_comment(raw: &str) -> &str {
    split_value_and_comment(raw).0
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        && !name.starts_with(|character: char| character.is_ascii_digit())
}

/// `export NAME=value`, `NAME=value`, or fish's `set -x NAME value`.
fn parse_assignment(line: &str) -> Option<(String, String, bool)> {
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        return None;
    }

    // fish: `set -x NAME value`, `set -gx NAME value`
    if let Some(rest) = trimmed.strip_prefix("set ") {
        let mut parts = rest.split_whitespace();
        let flags = parts.next()?;
        if !flags.starts_with('-') || !flags.contains('x') {
            return None;
        }
        let name = parts.next()?;
        if !valid_name(name) {
            return None;
        }
        let value = rest
            .split_once(name)
            .map(|(_, tail)| tail.trim())
            .unwrap_or_default();
        let (value, rewritable) = unquote(strip_trailing_comment(value));
        return Some((name.to_string(), value, rewritable));
    }

    let body = trimmed
        .strip_prefix("export ")
        .or_else(|| trimmed.strip_prefix("declare -x "))
        .or_else(|| trimmed.strip_prefix("typeset -x "))
        .unwrap_or(trimmed);
    let (name, raw) = body.split_once('=')?;
    let name = name.trim();
    if !valid_name(name) {
        return None;
    }
    let (value, rewritable) = unquote(strip_trailing_comment(raw));
    Some((name.to_string(), value, rewritable))
}

/// Resolves the argument of a `source` directive far enough to open it.
///
/// Only the parameters a profile realistically uses are expanded; anything else
/// yields `None`, which means that branch of the tree simply is not followed.
fn resolve_sourced(raw: &str, home: &Path, parent: &Path) -> Option<PathBuf> {
    let (argument, _) = unquote(strip_trailing_comment(raw));
    if argument.is_empty() || argument.contains('`') || argument.contains('*') {
        return None;
    }

    let xdg_config = home.join(".config");
    let xdg_cache = home.join(".cache");
    let expanded = argument
        // `${VAR:-fallback}` is the other shape a real profile uses. Only the
        // variables expanded below are recognised; anything else still leaves a
        // `$` behind and the directive is not followed.
        .replace(
            "${XDG_CONFIG_HOME:-$HOME/.config}",
            &xdg_config.to_string_lossy(),
        )
        .replace(
            "${XDG_CACHE_HOME:-$HOME/.cache}",
            &xdg_cache.to_string_lossy(),
        )
        .replace("${XDG_CONFIG_HOME}", &xdg_config.to_string_lossy())
        .replace("$XDG_CONFIG_HOME", &xdg_config.to_string_lossy())
        .replace("${HOME}", &home.to_string_lossy())
        .replace("$HOME", &home.to_string_lossy());
    if expanded.contains('$') {
        return None;
    }

    let path = if let Some(suffix) = expanded.strip_prefix("~/") {
        home.join(suffix)
    } else if expanded == "~" {
        home.to_path_buf()
    } else {
        PathBuf::from(&expanded)
    };
    if path.is_absolute() {
        return Some(path);
    }
    // A relative `source` is relative to the sourcing file's directory.
    Some(parent.parent()?.join(path))
}

/// Every `source` / `.` directive on a line.
///
/// A directive is very often not the first thing on its line. The idiom
/// everybody uses for an optional file is
/// `[[ -r ~/.config/zsh/secrets.zsh ]] && source ~/.config/zsh/secrets.zsh`,
/// and requiring `source` at the start of the line missed exactly the setup
/// this module was written for. The line is split on the shell's own separators
/// and each segment is tested, so a guard, a chain, or several directives on
/// one line all work.
///
/// The guard itself is not evaluated — this is not a shell. Following a
/// directive that may not have run is safe because attribution still requires
/// the value to match what the login shell exported.
fn sourced_arguments(line: &str) -> Vec<&str> {
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        return Vec::new();
    }
    trimmed
        // Split on the separator characters rather than the two-character
        // operators, which keeps the borrow and costs nothing: a segment after
        // a single `&` or a pipe simply will not begin with `source`.
        .split([';', '&', '|'])
        .filter_map(|segment| {
            let segment = segment.trim();
            segment
                .strip_prefix("source ")
                .or_else(|| segment.strip_prefix(". "))
                .map(str::trim)
                .filter(|argument| !argument.is_empty())
        })
        .collect()
}

/// Every assignment of `variable` the start-up tree contains, in the order the
/// files are read, so the last entry is the one that wins.
///
/// Files that cannot be read are skipped silently: a profile referencing a
/// machine-specific file that does not exist here is completely normal, and is
/// not something to report as a fault.
pub(crate) fn locate_variable(home: &Path, variable: &str) -> Vec<ShellVariableSite> {
    let mut sites = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut opened = 0_usize;
    let mut queue: Vec<(PathBuf, usize)> = base_files(home)
        .into_iter()
        .rev()
        .map(|path| (path, 0))
        .collect();

    while let Some((path, depth)) = queue.pop() {
        if opened >= MAX_FILES {
            break;
        }
        let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if !seen.insert(canonical) {
            continue;
        }
        let Some(contents) = read_start_up_file(&path) else {
            continue;
        };
        opened += 1;

        // A file's own `source` directives are read after its assignments in
        // file order, which `enumerate` already gives us; they are pushed in
        // reverse so the stack pops them in the order they appear.
        let mut nested = Vec::new();
        for (index, line) in contents.lines().enumerate() {
            if let Some((name, value, rewritable)) = parse_assignment(line) {
                if name.eq_ignore_ascii_case(variable) {
                    sites.push(ShellVariableSite {
                        variable: name,
                        path: path.clone(),
                        line: u32::try_from(index + 1).unwrap_or(u32::MAX),
                        value,
                        rewritable,
                    });
                }
                continue;
            }
            if depth + 1 >= MAX_DEPTH {
                continue;
            }
            for argument in sourced_arguments(line) {
                if let Some(target) = resolve_sourced(argument, home, &path) {
                    nested.push((target, depth + 1));
                }
            }
        }
        for entry in nested.into_iter().rev() {
            queue.push(entry);
        }
    }

    sites
}

/// The assignment responsible for a value the login shell reports.
///
/// The shell is the authority: a file is only credited when the value it writes
/// matches what the shell actually exported, which rules out a line inside a
/// conditional that never ran, or one overridden further down. When several
/// lines match, the last one read wins — the same way the shell resolves it.
pub(crate) fn responsible_site(
    home: &Path,
    variable: &str,
    shell_value: &str,
) -> Option<ShellVariableSite> {
    locate_variable(home, variable)
        .into_iter()
        .rfind(|site| site.value == shell_value)
}

#[cfg(test)]
mod tests;
