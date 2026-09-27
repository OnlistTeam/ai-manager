//! The one-line detail under a Skill or MCP row, and the portability checks
//! that decide whether a row carries a "may only work where it was set up"
//! hint (ADR-0062).
//!
//! Everything here is a pure function over strings: the compatibility layer
//! reads the upstream spec or the Skill folder and hands over plain values.
//! The rules that keep secrets off the screen live here so they are tested in
//! one place:
//!
//! - environment variable and header *values* are never part of a detail;
//! - an argument that follows a flag whose name hints at a credential, or a
//!   `NAME=value` argument whose name does, is shown as `••••`;
//! - anything shaped like a known credential (`sk-…`, `ghp_…`) is masked
//!   whatever its position;
//! - URL userinfo and credential-named query values are masked.

use crate::platform::redact::redact_secrets;

/// What a masked value looks like on screen.
pub const MASK: &str = "••••";

/// The longest detail line sent to the renderer, in characters. The row
/// truncates visually well before this; the bound keeps a pathological spec
/// from growing the IPC payload.
pub const DETAIL_MAX_CHARS: usize = 400;

/// Name fragments that mark a flag, variable or query parameter as a secret.
const SECRET_NAME_HINTS: [&str; 5] = ["key", "token", "secret", "password", "auth"];

fn names_a_secret(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    SECRET_NAME_HINTS.iter().any(|hint| lowered.contains(hint))
}

/// A value shaped like a credential on its own (`sk-…`, `ghp_…`, `AIza…`),
/// judged by the same rules the log redactor uses.
fn looks_like_secret_value(value: &str) -> bool {
    !value.is_empty() && redact_secrets(value) != value
}

/// Cut to `DETAIL_MAX_CHARS`, ending with an ellipsis when something was cut.
pub fn bounded(detail: String) -> String {
    if detail.chars().count() <= DETAIL_MAX_CHARS {
        return detail;
    }
    let mut cut = detail
        .chars()
        .take(DETAIL_MAX_CHARS - 1)
        .collect::<String>();
    cut.push('…');
    cut
}

/// `$HOME` abbreviated to `~`, so the account name never appears on screen.
/// `home` uses the platform's own separators; the comparison is literal.
pub fn abbreviate_home(value: &str, home: &str) -> String {
    let home = home.trim_end_matches(['/', '\\']);
    if home.is_empty() {
        return value.to_string();
    }
    match value.strip_prefix(home) {
        Some("") => "~".to_string(),
        Some(rest) if rest.starts_with(['/', '\\']) => format!("~{rest}"),
        _ => value.to_string(),
    }
}

/// POSIX-style quoting for display: a token made only of characters a shell
/// leaves alone is shown as is, anything else in single quotes.
pub fn shell_quote(token: &str) -> String {
    let plain = !token.is_empty()
        && token.chars().all(|character| {
            character.is_ascii_alphanumeric() || "@%+=:,./_-~^".contains(character)
        });
    if plain {
        token.to_string()
    } else {
        format!("'{}'", token.replace('\'', r"'\''"))
    }
}

/// `--api-key=value` or `API_KEY=value` with a credential-like name.
fn masked_assignment(argument: &str) -> Option<String> {
    let (name, value) = argument.split_once('=')?;
    let bare = name.trim_start_matches('-');
    let is_name = !bare.is_empty()
        && bare
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_-.".contains(character));
    (is_name && !value.is_empty() && names_a_secret(bare)).then(|| format!("{name}={MASK}"))
}

/// A flag that takes its value in the next argument, e.g. `--api-key sk-…`.
fn is_secret_flag(argument: &str) -> bool {
    argument.starts_with('-') && !argument.contains('=') && names_a_secret(argument)
}

/// The command line a local MCP server is started with, arguments quoted as
/// a shell would need them and secrets masked. Environment variables are
/// deliberately not part of it.
pub fn command_line(command: &str, arguments: &[String], home: &str) -> String {
    let mut tokens = Vec::with_capacity(arguments.len() + 1);
    tokens.push(display_token(command, home));
    let mut mask_next = false;
    for argument in arguments {
        let shown = if mask_next {
            MASK.to_string()
        } else if let Some(masked) = masked_assignment(argument) {
            masked
        } else {
            display_token(argument, home)
        };
        mask_next = is_secret_flag(argument);
        tokens.push(shown);
    }
    bounded(tokens.join(" "))
}

fn display_token(token: &str, home: &str) -> String {
    if looks_like_secret_value(token) {
        return MASK.to_string();
    }
    shell_quote(&abbreviate_home(token, home))
}

/// The address of a remote MCP server with its userinfo and any
/// credential-named query value masked. A value that does not parse as a URL
/// is still shown, masked by the generic credential rules.
pub fn remote_address(raw: &str) -> String {
    let raw = raw.trim();
    let Ok(mut parsed) = url::Url::parse(raw) else {
        return bounded(redact_secrets(raw).replace("***", MASK));
    };
    let had_userinfo = !parsed.username().is_empty() || parsed.password().is_some();
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);

    let pairs = parsed
        .query_pairs()
        .map(|(name, value)| {
            let masked = names_a_secret(&name) || looks_like_secret_value(&value);
            (
                name.into_owned(),
                if masked {
                    None
                } else {
                    Some(value.into_owned())
                },
            )
        })
        .collect::<Vec<_>>();
    let masked_any = pairs.iter().any(|(_, value)| value.is_none());
    if !had_userinfo && !masked_any {
        // Nothing to hide in the structure: show it as it was written
        // rather than as the URL serializer normalizes it.
        return bounded(mask_secret_segments(raw));
    }
    if masked_any {
        parsed.set_query(None);
    }

    let mut shown = parsed.to_string();
    if masked_any {
        // Rebuilt by hand so the mask stays readable instead of being
        // percent-encoded by the URL serializer.
        let query = pairs
            .iter()
            .map(|(name, value)| format!("{name}={}", value.as_deref().unwrap_or(MASK)))
            .collect::<Vec<_>>()
            .join("&");
        let fragment_at = shown.find('#').unwrap_or(shown.len());
        shown.insert_str(fragment_at, &format!("?{query}"));
    }
    if had_userinfo {
        if let Some(scheme_end) = shown.find("://") {
            shown.insert_str(scheme_end + 3, &format!("{MASK}@"));
        }
    }
    bounded(mask_secret_segments(&shown))
}

/// A path segment shaped like a credential (`/sse/sk-…`) is masked.
fn mask_secret_segments(address: &str) -> String {
    address
        .split('/')
        .map(|segment| {
            if looks_like_secret_value(segment) {
                MASK
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Whether a command or argument points at a file relative to wherever the
/// tool happens to start the server, so it breaks as soon as the server is
/// used from another project.
///
/// Explicit `./` and `../` forms always count. A bare `dir/file.ext` counts
/// when its last segment has an extension; `owner/repo` without one is too
/// easily a package or repository shorthand to warn about. Package specs
/// (`@scope/pkg`, `pkg@1.2`), URLs and `scheme:` references
/// (`github:owner/repo`), absolute and home-relative paths never count. The
/// value after `--flag=` is checked the same way.
pub fn is_relative_path(token: &str) -> bool {
    let token = token.trim();
    let value = match token.split_once('=') {
        Some((flag, value)) if flag.starts_with('-') => value,
        _ => token,
    };
    if ["./", "../", ".\\", "..\\"]
        .iter()
        .any(|prefix| value.starts_with(prefix))
    {
        return true;
    }
    if value.is_empty()
        || value.starts_with(['/', '\\', '~', '$', '%', '@', '-'])
        || value.contains(':')
        || value.contains('@')
        || !value.contains(['/', '\\'])
    {
        return false;
    }
    let last = value.rsplit(['/', '\\']).next().unwrap_or_default();
    match last.rsplit_once('.') {
        Some((stem, extension)) => {
            !stem.is_empty()
                && !extension.is_empty()
                && extension
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        }
        None => false,
    }
}

/// A `${NAME}` reference that some tools expand from the environment when
/// they start the server and others pass through literally.
pub fn has_env_reference(value: &str) -> bool {
    value.contains("${")
}

/// The ways a document can spell one tool's home folder, `/`-separated.
/// `relative_to_home` is the folder below `$HOME` (`.claude`), when it is
/// below it; `absolute` is its full path.
pub fn home_spellings(relative_to_home: Option<&str>, absolute: &str) -> Vec<String> {
    let mut spellings = Vec::new();
    if let Some(relative) = relative_to_home {
        let relative = relative.replace('\\', "/");
        let relative = relative.trim_matches('/');
        if !relative.is_empty() {
            for home in ["~", "$HOME", "${HOME}", "%USERPROFILE%"] {
                spellings.push(format!("{home}/{relative}"));
            }
        }
    }
    let absolute = absolute.replace('\\', "/");
    let absolute = absolute.trim_end_matches('/');
    if !absolute.is_empty() {
        spellings.push(absolute.to_string());
    }
    spellings
}

/// Whether `text` names one of `spellings` as a folder: `~/.claude` followed
/// by `/`, a quote, a space or the end, but not `~/.claude-desktop` or
/// `~/.claude.json`, which are different places.
pub fn mentions_folder(text: &str, spellings: &[String]) -> bool {
    let text = text.replace('\\', "/");
    spellings.iter().any(|spelling| {
        text.match_indices(spelling.as_str()).any(|(at, found)| {
            text[at + found.len()..].chars().next().is_none_or(|next| {
                !(next.is_alphanumeric() || next == '-' || next == '_' || next == '.')
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn a_command_line_quotes_what_a_shell_would_need_quoted() {
        assert_eq!(
            command_line(
                "npx",
                &strings(&["-y", "@playwright/mcp@latest"]),
                "/Users/a"
            ),
            "npx -y @playwright/mcp@latest"
        );
        assert_eq!(
            command_line("uvx", &strings(&["my server", "it's"]), "/Users/a"),
            r"uvx 'my server' 'it'\''s'"
        );
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn a_command_line_abbreviates_the_home_folder() {
        assert_eq!(
            command_line(
                "/Users/a/.local/bin/server",
                &strings(&["--root", "/Users/a/work", "/Users/ab/x"]),
                "/Users/a"
            ),
            "~/.local/bin/server --root ~/work /Users/ab/x"
        );
    }

    #[test]
    fn a_command_line_masks_values_that_follow_or_carry_a_secret_name() {
        let shown = command_line(
            "npx",
            &strings(&[
                "server",
                "--api-key",
                "abc123",
                "--token=xyz",
                "GITHUB_TOKEN=ghp_notreallyatoken00",
                "--auth",
                "plain",
                "--port",
                "8080",
                "sk-ant-api03-longsecretvalue",
                "PASSWORD=",
            ]),
            "/Users/a",
        );
        assert_eq!(
            shown,
            "npx server --api-key •••• --token=•••• GITHUB_TOKEN=•••• --auth •••• --port 8080 •••• PASSWORD="
        );
        for secret in ["abc123", "xyz", "ghp_", "plain", "sk-ant"] {
            assert!(!shown.contains(secret), "{secret} leaked into {shown}");
        }
    }

    #[test]
    fn a_long_command_line_is_bounded() {
        let arguments = vec!["x".repeat(300), "y".repeat(300)];
        let shown = command_line("node", &arguments, "/Users/a");
        assert_eq!(shown.chars().count(), DETAIL_MAX_CHARS);
        assert!(shown.ends_with('…'));
    }

    #[test]
    fn a_remote_address_hides_userinfo_and_credential_query_values() {
        assert_eq!(
            remote_address("https://mcp.example.com/v1?region=eu&api_key=abc&token=t"),
            "https://mcp.example.com/v1?region=eu&api_key=••••&token=••••"
        );
        assert_eq!(
            remote_address("https://user:pw@mcp.example.com/sse"),
            "https://••••@mcp.example.com/sse"
        );
        assert_eq!(
            remote_address("https://mcp.example.com/sse/sk-ant-longsecretvalue"),
            "https://mcp.example.com/sse/••••"
        );
        assert_eq!(
            remote_address(" https://mcp.example.com/mcp "),
            "https://mcp.example.com/mcp"
        );
    }

    #[test]
    fn relative_paths_are_told_apart_from_packages_urls_and_absolute_paths() {
        for relative in [
            "./server.js",
            "../tools/run.py",
            ".\\server.exe",
            "..\\bin\\run.cmd",
            "dist/index.js",
            "server/main.py",
            "--config=./mcp.json",
        ] {
            assert!(is_relative_path(relative), "{relative}");
        }
        for not_relative in [
            "npx",
            "-y",
            "@playwright/mcp@latest",
            "@modelcontextprotocol/server-filesystem",
            "mcp-server@1.2.0",
            "owner/repo",
            "github:owner/repo.js",
            "https://example.com/a.js",
            "/usr/local/bin/server.js",
            "~/bin/server.js",
            "$HOME/bin/server.js",
            "C:\\tools\\server.exe",
            "--port=8080",
            "",
        ] {
            assert!(!is_relative_path(not_relative), "{not_relative}");
        }
    }

    #[test]
    fn env_references_are_the_dollar_brace_form() {
        assert!(has_env_reference("Bearer ${API_TOKEN}"));
        assert!(has_env_reference("${HOME}/x"));
        assert!(!has_env_reference("$HOME/x"));
        assert!(!has_env_reference("{env:TOKEN}"));
    }

    #[test]
    fn a_home_folder_is_found_in_every_common_spelling_and_only_as_a_folder() {
        let spellings = home_spellings(Some(".claude"), "/Users/a/.claude");
        for text in [
            "Read ~/.claude/settings.json first.",
            "cd $HOME/.claude/skills",
            "open ${HOME}/.claude",
            "C: %USERPROFILE%\\.claude\\agents",
            "`/Users/a/.claude/CLAUDE.md`",
            "~/.claude",
        ] {
            assert!(mentions_folder(text, &spellings), "{text}");
        }
        for text in [
            "Edit ~/.claude.json",
            "~/.claude-desktop/config",
            "~/.claudette/",
            "~/.codex/config.toml",
        ] {
            assert!(!mentions_folder(text, &spellings), "{text}");
        }
    }

    #[test]
    fn a_nested_home_folder_keeps_its_full_relative_path() {
        let spellings = home_spellings(Some(".config/opencode"), "/Users/a/.config/opencode");
        assert!(mentions_folder("~/.config/opencode/agents", &spellings));
        assert!(!mentions_folder("~/.config/other", &spellings));
    }

    #[test]
    fn the_home_abbreviation_needs_a_separator_boundary() {
        assert_eq!(abbreviate_home("/Users/a", "/Users/a/"), "~");
        assert_eq!(abbreviate_home("/Users/a/x", "/Users/a"), "~/x");
        assert_eq!(abbreviate_home("/Users/ab/x", "/Users/a"), "/Users/ab/x");
        assert_eq!(abbreviate_home("C:\\Users\\a\\x", "C:\\Users\\a"), "~\\x");
    }
}
