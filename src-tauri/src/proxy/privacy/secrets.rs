//! Secret detectors: private keys, provider/API tokens, JWTs, URL passwords
//! and `NAME=value` assignments whose name says the value is a secret.
//!
//! Every pattern is bounded: a match that continues a longer token (a letter,
//! digit, `_` or `-` right before or after it) is not a key.

use std::sync::LazyLock;

use regex::Regex;

use super::placeholder::Kind;
use super::Detection;

fn compile(pattern: &str) -> Regex {
    Regex::new(pattern).expect("detector patterns are constants")
}

static PEM_PRIVATE_KEY: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----(?s:.)*?-----END [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----",
    )
});

static PREFIXED_KEY: LazyLock<Regex> = LazyLock::new(|| {
    compile(concat!(
        r"sk-(?:ant-|proj-|or-)?[A-Za-z0-9_\-]{20,}",
        r"|gh[pousr]_[A-Za-z0-9]{36,}",
        r"|github_pat_[A-Za-z0-9_]{22,}",
        r"|glpat-[A-Za-z0-9_\-]{20,}",
        r"|AIza[0-9A-Za-z_\-]{35}",
        r"|xox[abposr]-[A-Za-z0-9\-]{10,}",
        r"|(?:sk|rk)_live_[A-Za-z0-9]{16,}",
        r"|(?:AKIA|ASIA)[A-Z0-9]{16}",
        r"|hf_[A-Za-z0-9]{30,}",
        r"|gsk_[A-Za-z0-9]{20,}",
        r"|xai-[A-Za-z0-9]{20,}",
        r"|npm_[A-Za-z0-9]{36}",
        r"|pypi-[A-Za-z0-9_\-]{50,}",
        r"|dop_v1_[a-f0-9]{64}",
        r"|SG\.[A-Za-z0-9_\-]{22}\.[A-Za-z0-9_\-]{43}",
    ))
});

static JWT: LazyLock<Regex> =
    LazyLock::new(|| compile(r"eyJ[A-Za-z0-9_\-]{8,}\.eyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}"));

/// `scheme://user:password@host`; group 1 is the password.
static URL_PASSWORD: LazyLock<Regex> = LazyLock::new(|| {
    compile(r#"[A-Za-z][A-Za-z0-9+.\-]*://[^\s:/@"'<>]+:([^\s/@"'<>]+)@[A-Za-z0-9\[]"#)
});

/// A name containing a secret keyword, then `:`, `=` or `:=`, then the value
/// (group 1 double-quoted, group 2 single-quoted, group 3 bare). A keyword
/// must end the name or be followed by a separator or a camelCase capital, so
/// `max_tokens` and `tokenizer` are not secret names.
static ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    compile(concat!(
        r#"(?:^|[^A-Za-z0-9_])[A-Za-z0-9_.\-]*?"#,
        r#"(?i:password|passwd|secret|token|api[_\-]?key|access[_\-]?key|private[_\-]?key|credentials?)"#,
        r#"(?:[_.\-][A-Za-z0-9_.\-]*|[A-Z][A-Za-z0-9_]*)?"#,
        r#"["']?\s*(?::=|[:=])\s*"#,
        r#"(?:"([^"\r\n]{8,256})"|'([^'\r\n]{8,256})'|([^\s"'`,;]{8,256}))"#,
    ))
});

static DOTTED_REFERENCE: LazyLock<Regex> =
    LazyLock::new(|| compile(r"^[A-Za-z_$][A-Za-z0-9_$]*(?:\.[A-Za-z_$][A-Za-z0-9_$]*)+$"));

fn is_token_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '-'
}

/// True when neither neighbour of `start..end` continues the token.
pub(crate) fn bounded(text: &str, start: usize, end: usize, is_part: fn(char) -> bool) -> bool {
    let before = text[..start].chars().next_back();
    let after = text[end..].chars().next();
    !before.is_some_and(is_part) && !after.is_some_and(is_part)
}

fn has_letter_and_digit(value: &str) -> bool {
    value.chars().any(|c| c.is_ascii_alphabetic()) && value.chars().any(|c| c.is_ascii_digit())
}

/// Template values, masks, references and calls are not secrets.
fn looks_like_placeholder(value: &str) -> bool {
    let trimmed = value.trim();
    let first = trimmed.chars().next().unwrap_or(' ');
    let repeated = trimmed
        .chars()
        .all(|c| c == first || matches!(c, '*' | '•' | '.' | 'x' | 'X'));
    trimmed.starts_with("${")
        || trimmed.starts_with("$(")
        || trimmed.starts_with("{{")
        || trimmed.starts_with("#{")
        || trimmed.starts_with("%(")
        || (trimmed.starts_with('<') && trimmed.ends_with('>'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || repeated
}

fn plausible_assignment_value(value: &str) -> bool {
    value.chars().count() >= 8
        && has_letter_and_digit(value)
        && !looks_like_placeholder(value)
        && !value.contains('(')
        && !value.starts_with('$')
        && !DOTTED_REFERENCE.is_match(value)
}

fn plausible_url_password(value: &str) -> bool {
    !looks_like_placeholder(value) && !value.starts_with('$')
}

/// Appends secret spans in priority order; later detectors lose overlaps.
pub(crate) fn detect_secrets(text: &str, detection: &mut Detection) {
    for found in PEM_PRIVATE_KEY.find_iter(text) {
        detection.push(found.start(), found.end(), Kind::PrivateKey);
    }
    if text.contains("eyJ") {
        for found in JWT.find_iter(text) {
            if bounded(text, found.start(), found.end(), |c| {
                is_token_char(c) || c == '.'
            }) {
                detection.push(found.start(), found.end(), Kind::Token);
            }
        }
    }
    for found in PREFIXED_KEY.find_iter(text) {
        if bounded(text, found.start(), found.end(), is_token_char)
            && found.as_str().chars().any(|c| c.is_ascii_digit())
        {
            detection.push(found.start(), found.end(), Kind::ApiKey);
        }
    }
    if text.contains("://") {
        for captures in URL_PASSWORD.captures_iter(text) {
            if let Some(password) = captures.get(1) {
                if plausible_url_password(password.as_str()) {
                    detection.push(password.start(), password.end(), Kind::Password);
                }
            }
        }
    }
    for captures in ASSIGNMENT.captures_iter(text) {
        let value = captures
            .get(1)
            .or_else(|| captures.get(2))
            .or_else(|| captures.get(3));
        if let Some(value) = value {
            if plausible_assignment_value(value.as_str()) {
                detection.push(value.start(), value.end(), Kind::Secret);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::placeholder::Kind;
    use super::super::{detect, Span};

    fn found(text: &str) -> Vec<(Kind, &str)> {
        detect(text)
            .into_iter()
            .map(|Span { start, end, kind }| (kind, &text[start..end]))
            .collect()
    }

    fn single(text: &str, kind: Kind, value: &str) {
        assert_eq!(found(text), vec![(kind, value)], "{text}");
    }

    const ANTHROPIC: &str = "sk-ant-api03-Ab3dEf9hIjKlMnOpQrStUvWxYz0123456789";

    #[test]
    fn pem_private_keys_are_masked_whole() {
        let pem = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAA\n-----END OPENSSH PRIVATE KEY-----";
        single(&format!("key:\n{pem}\nend"), Kind::PrivateKey, pem);
        let rsa = "-----BEGIN RSA PRIVATE KEY-----\nMIIEow\n-----END RSA PRIVATE KEY-----";
        single(rsa, Kind::PrivateKey, rsa);
        assert!(found("-----BEGIN PUBLIC KEY-----\nMIIB\n-----END PUBLIC KEY-----").is_empty());
        assert!(found("-----BEGIN PRIVATE KEY----- truncated").is_empty());
    }

    #[test]
    fn provider_keys_with_known_prefixes_are_masked() {
        for key in [
            ANTHROPIC,
            "sk-proj-Ab3dEf9hIjKlMnOpQrStUvWx",
            "sk-or-v1-0123456789abcdef0123456789abcdef",
            "sk-Ab3dEf9hIjKlMnOpQrStUvWx",
            "ghp_Ab3dEf9hIjKlMnOpQrStUvWxYz0123456789",
            "gho_Ab3dEf9hIjKlMnOpQrStUvWxYz0123456789",
            "github_pat_11ABCDEFG0123456789_abcdefghijklmnopqrstuvwxyz",
            "glpat-Ab3dEf9hIjKlMnOpQrSt",
            "AIzaSyAb3dEf9hIjKlMnOpQrStUvWxYz0123456",
            "xoxb-1234567890-abcdefghij",
            "sk_live_Ab3dEf9hIjKlMnOp12",
            "rk_live_Ab3dEf9hIjKlMnOp12",
            "AKIAIOSFODNN7EXAMPLE",
            "ASIAIOSFODNN7EXAMPLE",
            "hf_Ab3dEf9hIjKlMnOpQrStUvWxYz01234",
            "gsk_Ab3dEf9hIjKlMnOpQrSt12",
            "xai-Ab3dEf9hIjKlMnOpQrSt12",
            "npm_Ab3dEf9hIjKlMnOpQrStUvWxYz0123456789",
            "pypi-AgEIcHlwaS5vcmcCJGFiY2RlZjAxLTIzNDUtNjc4OS1hYmNkLWVmMDEyMzQ1Njc4OQ",
            "dop_v1_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "SG.Ab3dEf9hIjKlMnOpQrStUv.Ab3dEf9hIjKlMnOpQrStUvWxYz0123456789abcdefg",
        ] {
            single(&format!("use {key} now"), Kind::ApiKey, key);
        }
    }

    #[test]
    fn a_key_inside_a_longer_token_is_not_a_key() {
        assert!(found(&format!("task-{ANTHROPIC}")).is_empty());
        assert!(found(&format!("x{ANTHROPIC}")).is_empty());
        assert!(found("AIzaSyAb3dEf9hIjKlMnOpQrStUvWxYz0123456789").is_empty());
        assert!(found("AKIAIOSFODNN7EXAMPLE2").is_empty());
    }

    #[test]
    fn prefix_shaped_words_without_digits_are_not_keys() {
        assert!(found("sk-this-is-just-a-long-slug-name").is_empty());
        assert!(found("sk-short1").is_empty());
        assert!(found("the ghp_ prefix is for GitHub tokens").is_empty());
    }

    #[test]
    fn jwts_are_masked_as_tokens() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        single(&format!("Bearer {jwt}"), Kind::Token, jwt);
        assert!(found("eyJhbGciOiJIUzI1NiJ9 alone").is_empty());
    }

    #[test]
    fn only_the_password_of_a_url_is_masked() {
        single(
            "postgres://admin:s3cr3tPass@db.internal:5432/app",
            Kind::Password,
            "s3cr3tPass",
        );
        single(
            "https://bob:hunter2@example.org/x",
            Kind::Password,
            "hunter2",
        );
        assert!(found("postgres://admin:${DB_PASSWORD}@db/app").is_empty());
        assert!(found("postgres://admin:<password>@db/app").is_empty());
        assert!(found("postgres://admin:****@db/app").is_empty());
        assert!(found("ssh://git@github.com:22/org/repo").is_empty());
        assert!(found("https://example.org:8443/path").is_empty());
    }

    #[test]
    fn secret_assignments_mask_only_the_value() {
        single("API_KEY=abc123def456", Kind::Secret, "abc123def456");
        single(
            "export DB_PASSWORD=Tr0ub4dor&3",
            Kind::Secret,
            "Tr0ub4dor&3",
        );
        single(
            r#""password": "hunter2hunter2""#,
            Kind::Secret,
            "hunter2hunter2",
        );
        single(
            "client_secret: 'q1w2e3r4t5y6'",
            Kind::Secret,
            "q1w2e3r4t5y6",
        );
        single("GITHUB_TOKEN := abcd1234efgh", Kind::Secret, "abcd1234efgh");
        single(
            "const secretKey = \"zx9cv8bn7m\";",
            Kind::Secret,
            "zx9cv8bn7m",
        );
        single(
            "aws_access_key_id=Q8w7E6r5T4y3",
            Kind::Secret,
            "Q8w7E6r5T4y3",
        );
        single("credentials=pa55word99", Kind::Secret, "pa55word99");
    }

    #[test]
    fn a_known_key_in_an_assignment_keeps_its_specific_kind() {
        single(
            &format!("ANTHROPIC_API_KEY={ANTHROPIC}"),
            Kind::ApiKey,
            ANTHROPIC,
        );
    }

    #[test]
    fn assignments_that_are_not_secrets_are_left_alone() {
        for text in [
            "max_tokens: 4096000",
            "tokenizer=cl100k_base",
            "password=short1",
            "password=onlyletters",
            "token_count=12345678",
            "API_KEY=${OPENAI_API_KEY}",
            "API_KEY=$OPENAI_API_KEY1",
            "password: <your-password-1>",
            "password = \"********\"",
            "token = getToken(user1)",
            "token = config.auth.token2",
            "secret: xxxxxxxxxxxx",
            "the password is not here1",
            "api_key={{API_KEY_abcdefgh}}",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }
}
