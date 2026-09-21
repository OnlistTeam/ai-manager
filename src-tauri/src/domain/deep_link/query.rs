//! Strict URL and query-string parsing for deep links (ADR-0029 decision 5).
//!
//! Everything here is structural: shape of the URL, shape of the query. Nothing
//! in this file knows what a tool or a credential is.

use std::collections::BTreeMap;
use url::Url;

use super::{invalid, MAX_DEEP_LINK_BYTES, MAX_VALUE_BYTES};
use crate::domain::AppError;

pub const PRODUCT_SCHEME: &str = "aimanager";
/// Accepted by paste only. Registering it would hijack the import links of users
/// who also have the upstream application installed (ADR-0029 decision 1).
pub const COMPATIBLE_SCHEME: &str = "ccswitch";
const LINK_HOST: &str = "v1";
const LINK_PATH: &str = "/import";

/// The query of one `v1/import` link: every key appears at most once.
pub struct LinkQuery {
    pairs: BTreeMap<String, String>,
}

/// Normalizes the three accepted spellings into one absolute URL string.
///
/// `allow_compatible_scheme` is true only on the paste path. A pasted link may
/// also omit the scheme entirely, which is what users get when they copy the
/// visible part of a vendor's instructions.
pub fn normalize(raw: &str, allow_compatible_scheme: bool) -> Result<String, AppError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(invalid("error.deepLink.invalidLink", "deep link is empty"));
    }
    if raw.len() > MAX_DEEP_LINK_BYTES {
        return Err(invalid(
            "error.deepLink.tooLarge",
            "deep link exceeds the accepted size",
        ));
    }
    if raw.chars().any(char::is_control) {
        return Err(invalid(
            "error.deepLink.invalidLink",
            "deep link contains control characters",
        ));
    }

    let Some((scheme, rest)) = split_scheme(raw) else {
        // No scheme at all: only a pasted bare `v1/import?...` query is accepted.
        return if allow_compatible_scheme && raw.starts_with(LINK_HOST) {
            Ok(format!("{PRODUCT_SCHEME}://{raw}"))
        } else {
            Err(invalid(
                "error.deepLink.unsupportedScheme",
                "deep link has no recognized scheme",
            ))
        };
    };

    let lowered = scheme.to_ascii_lowercase();
    if lowered == PRODUCT_SCHEME {
        return Ok(raw.to_string());
    }
    if lowered == COMPATIBLE_SCHEME && allow_compatible_scheme {
        return Ok(format!("{PRODUCT_SCHEME}://{rest}"));
    }
    Err(invalid(
        "error.deepLink.unsupportedScheme",
        "deep link scheme is not accepted on this path",
    ))
}

/// Splits `scheme://rest`. Only the `//` form exists in this format, so a bare
/// `scheme:opaque` is deliberately not recognized as a scheme at all.
fn split_scheme(raw: &str) -> Option<(&str, &str)> {
    let separator = raw.find("://")?;
    let scheme = &raw[..separator];
    if scheme.is_empty()
        || !scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return None;
    }
    Some((scheme, &raw[separator + 3..]))
}

/// Parses a normalized `aimanager://v1/import?...` URL into its query pairs.
pub fn parse(normalized: &str) -> Result<LinkQuery, AppError> {
    let url = Url::parse(normalized)
        .map_err(|_| invalid("error.deepLink.invalidLink", "deep link is not a valid URL"))?;

    if url.host_str() != Some(LINK_HOST) {
        return Err(invalid(
            "error.deepLink.unsupportedVersion",
            "deep link version segment is not v1",
        ));
    }
    if url.path() != LINK_PATH {
        return Err(invalid(
            "error.deepLink.invalidLink",
            "deep link path is not /import",
        ));
    }
    if url.port().is_some() {
        return Err(invalid(
            "error.deepLink.invalidLink",
            "deep link carries a port",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid(
            "error.deepLink.invalidLink",
            "deep link carries userinfo",
        ));
    }
    if url.fragment().is_some() {
        return Err(invalid(
            "error.deepLink.invalidLink",
            "deep link carries a fragment",
        ));
    }

    let mut pairs = BTreeMap::new();
    for (key, value) in url.query_pairs() {
        let key = key.into_owned();
        let value = value.into_owned();
        if key.is_empty() || key.len() > 64 || !key.chars().all(is_key_character) {
            return Err(invalid(
                "error.deepLink.unknownParameter",
                "deep link parameter name is not a plain identifier",
            ));
        }
        if value.len() > MAX_VALUE_BYTES {
            return Err(invalid(
                "error.deepLink.valueTooLong",
                "deep link parameter value is too long",
            ));
        }
        if value.chars().any(char::is_control) {
            return Err(invalid(
                "error.deepLink.invalidLink",
                "deep link parameter value contains control characters",
            ));
        }
        if pairs.insert(key, value).is_some() {
            return Err(invalid(
                "error.deepLink.duplicateParameter",
                "deep link repeats a parameter",
            ));
        }
    }

    Ok(LinkQuery { pairs })
}

fn is_key_character(value: char) -> bool {
    value.is_ascii_alphanumeric() || matches!(value, '_' | '-')
}

impl LinkQuery {
    /// Rejects anything outside the allowlist for this resource. `x-` prefixed
    /// keys are the agreed forward-compatible extension space and are ignored
    /// rather than rejected (ADR-0029 decision 2).
    pub fn reject_unknown(&self, allowed: &[&str]) -> Result<(), AppError> {
        for key in self.pairs.keys() {
            if key.starts_with("x-") || allowed.contains(&key.as_str()) {
                continue;
            }
            return Err(invalid(
                "error.deepLink.unknownParameter",
                format!("deep link carries the unsupported parameter {key}"),
            ));
        }
        Ok(())
    }

    /// Present-and-non-blank values only: a vendor template that leaves a field
    /// empty means "not supplied", never "supplied as the empty string".
    pub fn optional(&self, key: &str) -> Option<&str> {
        self.pairs
            .get(key)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
    }

    pub fn required(&self, key: &'static str) -> Result<&str, AppError> {
        self.optional(key).ok_or_else(|| {
            invalid(
                "error.deepLink.missingParameter",
                format!("deep link is missing the required parameter {key}"),
            )
        })
    }

    /// The upstream format spells booleans `true` / `false`; anything else is a
    /// typo we refuse rather than silently read as `false`.
    pub fn optional_bool(&self, key: &'static str) -> Result<Option<bool>, AppError> {
        match self.optional(key) {
            None => Ok(None),
            Some("true") => Ok(Some(true)),
            Some("false") => Ok(Some(false)),
            Some(_) => Err(invalid(
                "error.deepLink.invalidLink",
                format!("deep link parameter {key} is not a boolean"),
            )),
        }
    }
}
