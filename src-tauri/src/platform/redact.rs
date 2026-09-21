pub const REDACTED_PLACEHOLDER: &str = "***";

/// Known credential prefixes. A match replaces the whole word — more conservative than guessing which values are secrets, safer than not redacting.
const SECRET_PREFIXES: [&str; 6] = ["sk-", "sk_", "ghp_", "gho_", "github_pat_", "xai-"];
/// The Google API key prefix is case-sensitive, so it is listed separately from the lowercase prefix table.
const GOOGLE_API_KEY_PREFIX: &str = "AIza";
/// In a `<name>=<value>` form, a name containing one of these substrings masks the whole segment regardless of what the value looks like.
const SECRET_KEY_HINTS: [&str; 5] = ["token", "key", "secret", "password", "authorization"];
/// Wrapping characters stripped before the prefix check (quotes, brackets, commas, semicolons, colons, backticks).
const WRAPPERS: [char; 8] = ['"', '\'', ',', ';', ':', '(', ')', '`'];

/// Replace credential-looking fragments in subprocess output with `***` (AI_RULES rule 6).
///
/// Processing happens per "word + trailing whitespace" and the parts are reassembled
/// verbatim, so newlines, indentation and tabs are all preserved — the caller still gets a
/// readable log, just without the credentials.
pub fn redact_secrets(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut redact_next = false;
    for chunk in input.split_inclusive(char::is_whitespace) {
        let trailing = chunk.len() - chunk.trim_end().len();
        let (core, whitespace) = chunk.split_at(chunk.len() - trailing);
        if core.is_empty() {
            // Consecutive whitespace: does not change the "mask the next word" state.
            out.push_str(whitespace);
            continue;
        }
        if redact_next {
            let trimmed = core.trim_matches(|c| WRAPPERS.contains(&c));
            if trimmed.eq_ignore_ascii_case("bearer") {
                // `Authorization: Bearer <token>` — `Bearer` itself is not a credential, so
                // let the pending "mask the next word" pass through it and land on the real value.
                out.push_str(core);
            } else {
                out.push_str(REDACTED_PLACEHOLDER);
                redact_next = false;
            }
        } else {
            let trimmed = core.trim_matches(|c| WRAPPERS.contains(&c));
            // "Mask the next word" is not triggered by the literal `Bearer` alone:
            // colon-separated labels such as `Authorization:`, `token:` and `key:` are
            // followed by credentials too, and whether a word is such a label is decided by
            // the same case-insensitive SECRET_KEY_HINTS substring match as the `name=value`
            // branch of `redact_core`. Words containing `=` are left to that name=value
            // branch and skipped here, otherwise `mykey=value` would also mask the next,
            // unrelated word.
            redact_next = trimmed.eq_ignore_ascii_case("bearer")
                || (!trimmed.contains('=') && is_secret_label(trimmed));
            out.push_str(&redact_core(core));
        }
        out.push_str(whitespace);
    }
    out
}

/// Labels such as `token`, `key` and `Authorization` — decided the same way as the
/// `name=value` branch of `redact_core`: a case-insensitive `SECRET_KEY_HINTS` substring.
fn is_secret_label(token: &str) -> bool {
    let lowered = token.to_ascii_lowercase();
    SECRET_KEY_HINTS.iter().any(|hint| lowered.contains(hint))
}

fn redact_core(core: &str) -> String {
    if let Some(masked) = redact_url_userinfo(core) {
        return masked;
    }
    if let Some((name, value)) = core.split_once('=') {
        let lowered = name.to_ascii_lowercase();
        if !value.is_empty() && SECRET_KEY_HINTS.iter().any(|hint| lowered.contains(hint)) {
            return format!("{name}={REDACTED_PLACEHOLDER}");
        }
    }
    let trimmed = core.trim_matches(|c| WRAPPERS.contains(&c));
    let looks_secret = trimmed.len() > 8
        && (SECRET_PREFIXES
            .iter()
            .any(|prefix| trimmed.starts_with(prefix))
            || trimmed.starts_with(GOOGLE_API_KEY_PREFIX));
    if looks_secret {
        return core.replace(trimmed, REDACTED_PLACEHOLDER);
    }
    core.to_string()
}

/// Masks `scheme://user:password@host` even when the credential has no known
/// key prefix. Installer errors may echo a configured proxy URL verbatim, so
/// environment values must be protected independently of CommandSpec display.
fn redact_url_userinfo(core: &str) -> Option<String> {
    let scheme_end = core.find("://")? + 3;
    let authority = &core[scheme_end..];
    let at = authority.find('@')?;
    let authority_end = authority.find(['/', '?', '#']).unwrap_or(authority.len());
    if at == 0 || at >= authority_end {
        return None;
    }
    Some(format!(
        "{}{REDACTED_PLACEHOLDER}{}",
        &core[..scheme_end],
        &authority[at..]
    ))
}

/// The singular matches while the plural does not — the key is split on `_` and each
/// segment is matched exactly. This aligns with
/// `services/provider/mod.rs::is_sensitive_config_key` (whose `SENSITIVE_SUFFIXES` also
/// uses the singular `_TOKEN` rather than `_TOKENS`): the `TOKENS` in
/// `CLAUDE_CODE_MAX_OUTPUT_TOKENS`/`MAX_THINKING_TOKENS` is a unit of quantity, not a
/// credential; a substring match would lump them together with `MY_PROXY_TOKEN`, while
/// exact matching of split segments tells singular from plural.
const CREDENTIAL_KEY_SEGMENT_HINTS: [&str; 6] =
    ["KEY", "AUTH", "TOKEN", "SECRET", "PASSWORD", "PASSWD"];

/// Decide whether an environment variable "looks like a credential", used when switching
/// providers to determine whether a key from previous may be pasted back into written (the
/// Claude env loop in `live_preservation.rs` and `merge_gemini_env`).
///
/// The key name is split on `_` and matched exactly against
/// `CREDENTIAL_KEY_SEGMENT_HINTS`, rather than substring-matching the whole key name —
/// names like `AUTHOR_NAME` and `MONKEY_PATCH` contain the substrings `AUTH`/`KEY` but are
/// clearly not credentials, and a substring match would likewise conflate `*_TOKENS`
/// (plural, a unit of quantity) with `*_TOKEN` (singular, a credential). `CREDENTIAL` is
/// the one exception: both singular and plural count, because the official spelling of
/// `GOOGLE_APPLICATION_CREDENTIALS` is plural. Besides the key name, a value that itself
/// has a known credential shape (one `redact_secrets` would rewrite) counts as well — a key
/// with no hint in its name but a value like `sk-...`/`ghp_...` must still be caught.
pub fn looks_like_credential(key: &str, value: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    let key_hints_hit = upper.split('_').any(|part| {
        CREDENTIAL_KEY_SEGMENT_HINTS.contains(&part)
            || part == "CREDENTIAL"
            || part == "CREDENTIALS"
    });
    key_hints_hit || (!value.is_empty() && redact_secrets(value) != value)
}

/// Take at most `max_lines` lines from the end of the text and squeeze the result under
/// `max_bytes` (cutting from the front, keeping the tail — the key errors of npm / pip are
/// at the end). The cut point is aligned to a UTF-8 character boundary.
pub fn truncate_tail(text: &str, max_lines: usize, max_bytes: usize) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let lines: Vec<&str> = trimmed.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    let tail = lines[start..].join("\n");
    if tail.len() <= max_bytes {
        return tail;
    }
    let mut cut = tail.len() - max_bytes;
    while cut < tail.len() && !tail.is_char_boundary(cut) {
        cut += 1;
    }
    tail[cut..].to_string()
}

#[cfg(test)]
mod tests {
    use super::{looks_like_credential, redact_secrets, truncate_tail, REDACTED_PLACEHOLDER};

    #[test]
    fn looks_like_credential_matches_by_key_name_or_by_value_shape() {
        for key in [
            "AWS_SECRET_ACCESS_KEY",
            "MY_PROXY_TOKEN",
            "AWS_SESSION_TOKEN",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "SOME_PASSWORD",
            "SOME_PASSWD",
            "API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
        ] {
            assert!(
                looks_like_credential(key, "whatever"),
                "{key} should be flagged by name alone"
            );
        }

        // Boundary matching: `AUTH`/`KEY` only count as whole segments, never as a substring of the full string.
        for key in ["AUTHOR_NAME", "MONKEY_PATCH", "DISABLE_AUTO_COMPACT"] {
            assert!(
                !looks_like_credential(key, "plain value"),
                "{key} must not be flagged just for containing AUTH/KEY as a substring"
            );
        }

        // Singular/plural boundary: `_TOKENS` (plural, a unit of quantity) must not be hit
        // by the `_TOKEN` (singular, credential) rule — the same note as on
        // `SENSITIVE_SUFFIXES` in `services/provider/mod.rs::is_sensitive_config_key`.
        for key in [
            "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
            "CLAUDE_CODE_MAX_CONTEXT_TOKENS",
            "MAX_THINKING_TOKENS",
        ] {
            assert!(
                !looks_like_credential(key, "30000"),
                "{key} is a token-count limit, not a credential"
            );
        }

        // Common non-credential config entries must not be caught even when their value is non-empty.
        for key in [
            "API_TIMEOUT_MS",
            "GOOGLE_CLOUD_PROJECT",
            "DISABLE_AUTO_COMPACT",
        ] {
            assert!(
                !looks_like_credential(key, "1000"),
                "{key} is not a credential"
            );
        }

        // The key name gives no hint at all, but the value itself has a known credential shape — still caught.
        assert!(looks_like_credential(
            "MY_CUSTOM_VAR",
            "sk-ant-abcdefghijklmnop"
        ));
        assert!(!looks_like_credential("MY_CUSTOM_VAR", ""));
    }

    #[test]
    fn known_secret_prefixes_are_masked_in_place() {
        let masked = redact_secrets("npm ERR! auth sk-ant-abcdefghijklmnop failed");
        assert_eq!(
            masked,
            format!("npm ERR! auth {REDACTED_PLACEHOLDER} failed")
        );
        assert!(!masked.contains("abcdefghijklmnop"));

        for raw in [
            "ghp_0123456789abcdef",
            "gho_0123456789abcdef",
            "github_pat_0123456789",
            "xai-0123456789abcdef",
            "AIzaSyD0123456789abc",
            "sk_live_0123456789",
        ] {
            assert_eq!(
                redact_secrets(raw),
                REDACTED_PLACEHOLDER,
                "{raw} must be masked"
            );
        }
    }

    #[test]
    fn surrounding_punctuation_and_quotes_survive_masking() {
        assert_eq!(
            redact_secrets("(sk-ant-0123456789)"),
            format!("({REDACTED_PLACEHOLDER})")
        );
        assert_eq!(
            redact_secrets("'sk-ant-0123456789'"),
            format!("'{REDACTED_PLACEHOLDER}'")
        );
    }

    #[test]
    fn secret_looking_assignments_are_masked_by_key_name() {
        assert_eq!(
            redact_secrets("ANTHROPIC_API_KEY=zzzzzzzzzzzz"),
            format!("ANTHROPIC_API_KEY={REDACTED_PLACEHOLDER}")
        );
        assert_eq!(
            redact_secrets("authorization=whatever"),
            format!("authorization={REDACTED_PLACEHOLDER}")
        );
        assert_eq!(redact_secrets("PATH=/usr/local/bin"), "PATH=/usr/local/bin");
    }

    #[test]
    fn a_bearer_credential_is_masked_even_without_a_known_prefix() {
        assert_eq!(
            redact_secrets("Authorization: Bearer abc123xyz"),
            format!("Authorization: Bearer {REDACTED_PLACEHOLDER}")
        );
        assert_eq!(
            redact_secrets("Bearer\n  abc123xyz"),
            format!("Bearer\n  {REDACTED_PLACEHOLDER}")
        );
    }

    #[test]
    fn proxy_url_userinfo_is_masked_even_with_an_arbitrary_password() {
        let masked =
            redact_secrets("proxy failed: http://alice:do-not-show@127.0.0.1:7890/path?retry=1");
        assert_eq!(
            masked,
            "proxy failed: http://***@127.0.0.1:7890/path?retry=1"
        );
        assert!(!masked.contains("alice"));
        assert!(!masked.contains("do-not-show"));
        assert_eq!(
            redact_secrets("https://example.test/@public-name"),
            "https://example.test/@public-name"
        );
    }

    #[test]
    fn colon_separated_secret_labels_mask_the_following_value() {
        assert_eq!(
            redact_secrets("Authorization: abc123xyz"),
            format!("Authorization: {REDACTED_PLACEHOLDER}")
        );
        assert_eq!(
            redact_secrets("token: sk-xxx"),
            format!("token: {REDACTED_PLACEHOLDER}")
        );
        // A label that isn't a secret hint must not trigger masking.
        assert_eq!(redact_secrets("status: ready"), "status: ready");
    }

    #[test]
    fn ordinary_output_and_whitespace_are_preserved_byte_for_byte() {
        let raw = "added 2 packages in 1s\n\n  npm notice\tdone\n";
        assert_eq!(redact_secrets(raw), raw);
        assert_eq!(redact_secrets(""), "");
    }

    #[test]
    fn truncate_tail_keeps_the_last_lines_within_the_byte_budget() {
        let text = (1..=20)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(truncate_tail(&text, 3, 512), "line 18\nline 19\nline 20");

        let wide = "x".repeat(1000);
        let capped = truncate_tail(&wide, 8, 64);
        assert!(capped.len() <= 64, "byte budget must win over line budget");
        assert!(capped.ends_with('x'));

        assert_eq!(truncate_tail("   \n  \n", 8, 512), "");
    }

    #[test]
    fn truncate_tail_never_splits_a_multi_byte_character() {
        let text = "érrör-öutpüt".repeat(20);
        let capped = truncate_tail(&text, 8, 20);
        assert!(capped.len() <= 20);
        assert!(capped.chars().all(|c| "érrör-öutpüt".contains(c)));
    }
}
