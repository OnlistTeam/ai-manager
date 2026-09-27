//! Deciding what the user already has.
//!
//! A server is the same server whatever it is called: the same URL, or the
//! same launcher starting the same package (the version left out). A Skill is
//! the same Skill when it comes from the same GitHub repository and its
//! directory or name is the skills.sh id.

use super::catalog::ServerSpec;
use crate::compat::ccswitch::discover::{McpHolding, SkillHolding};

/// What a server runs, as a comparable key.
pub(crate) fn run_key(url: Option<&str>, command: Option<&str>, args: &[String]) -> String {
    if let Some(url) = url.map(str::trim).filter(|url| !url.is_empty()) {
        return url.to_lowercase().trim_end_matches('/').to_string();
    }
    let command = launcher(command.unwrap_or_default());
    let mut skip_next = false;
    for arg in args {
        let arg = arg.trim();
        if skip_next {
            skip_next = false;
            continue;
        }
        // `docker run -e NAME image`: the variable name is not the package.
        if matches!(arg, "-e" | "--env") {
            skip_next = true;
            continue;
        }
        if arg.is_empty() || arg.starts_with('-') || arg == "run" {
            continue;
        }
        // `@scope/pkg@latest` and `pkg@1.2.3` are `@scope/pkg` and `pkg`.
        let package = match arg.rfind('@') {
            Some(index) if index > 0 => &arg[..index],
            _ => arg,
        };
        return format!("{command} {}", package.to_lowercase());
    }
    command
}

/// `/usr/local/bin/npx`, `npx.cmd` and `NPX.EXE` all start the same thing.
fn launcher(command: &str) -> String {
    let file = command
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_lowercase();
    for suffix in [".cmd", ".exe", ".bat", ".ps1"] {
        if let Some(stem) = file.strip_suffix(suffix) {
            return stem.to_string();
        }
    }
    file
}

pub(crate) fn server_key(spec: &ServerSpec) -> String {
    run_key(spec.url.as_deref(), spec.command.as_deref(), &spec.args)
}

/// The name of the connection that already runs `spec`, if any.
pub(crate) fn added_server(spec: &ServerSpec, holdings: &[McpHolding]) -> Option<String> {
    let key = server_key(spec);
    holdings
        .iter()
        .find(|holding| {
            run_key(
                holding.url.as_deref(),
                holding.command.as_deref(),
                &holding.args,
            ) == key
        })
        .map(|holding| holding.name.clone())
}

/// The name of the Skill that already is `source`'s `skill_id`, if any.
pub(crate) fn added_skill(
    source: &str,
    skill_id: &str,
    name: &str,
    holdings: &[SkillHolding],
) -> Option<String> {
    let (owner, repository) = source.split_once('/')?;
    holdings
        .iter()
        .find(|holding| {
            let Some((held_owner, held_repository)) = holding.repo.as_ref() else {
                return false;
            };
            if !held_owner.eq_ignore_ascii_case(owner)
                || !held_repository.eq_ignore_ascii_case(repository)
            {
                return false;
            }
            let folder = holding
                .directory
                .rsplit('/')
                .next()
                .unwrap_or(&holding.directory);
            folder.eq_ignore_ascii_case(skill_id)
                || holding.name.eq_ignore_ascii_case(skill_id)
                || holding.name.eq_ignore_ascii_case(name)
        })
        .map(|holding| holding.name.clone())
}
