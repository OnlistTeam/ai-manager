//! Placeholder vocabulary: which kinds exist, how a value becomes a stable
//! placeholder, and how placeholders (whole or cut in half) are recognized.
//!
//! A placeholder is `{{KIND_xxxxxxxx}}`: the kind label plus the first 40 bits
//! of HMAC-SHA256(install key, value) in lowercase base32. The same value
//! always yields the same placeholder on this install, so conversation history
//! and provider prompt caches stay stable, while a provider that sees the
//! placeholder cannot confirm a guessed value without the install key.

use std::sync::LazyLock;

use regex::Regex;
use sha2::{Digest, Sha256};

pub(crate) const HASH_CHARS: usize = 8;
const BASE32_ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// Ordered so that a value recognised as two different contextual kinds in
/// one request always takes the same one (the first in declaration order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
    PrivateKey,
    ApiKey,
    Token,
    Password,
    Secret,
    Email,
    IdNumber,
    Phone,
    BankCard,
    Word,
}

impl Kind {
    pub(crate) const ALL: [Kind; 10] = [
        Kind::PrivateKey,
        Kind::ApiKey,
        Kind::Token,
        Kind::Password,
        Kind::Secret,
        Kind::Email,
        Kind::IdNumber,
        Kind::Phone,
        Kind::BankCard,
        Kind::Word,
    ];

    /// Kinds recognized only by their surroundings (`password=…`,
    /// `user:…@host`). Their values are remembered so that the same value is
    /// still masked when it later appears without that context.
    pub(crate) fn is_contextual(self) -> bool {
        matches!(self, Kind::Secret | Kind::Password)
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Kind::PrivateKey => "PRIVATE_KEY",
            Kind::ApiKey => "API_KEY",
            Kind::Token => "TOKEN",
            Kind::Password => "PASSWORD",
            Kind::Secret => "SECRET",
            Kind::Email => "EMAIL",
            Kind::IdNumber => "ID_NUMBER",
            Kind::Phone => "PHONE",
            Kind::BankCard => "BANK_CARD",
            Kind::Word => "WORD",
        }
    }
}

/// The per-install secret that keys the placeholder hash.
#[derive(Clone)]
pub(crate) struct HashKey([u8; 32]);

impl HashKey {
    pub(crate) fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub(crate) fn placeholder(&self, kind: Kind, value: &str) -> String {
        let digest = hmac_sha256(&self.0, value.as_bytes());
        format!("{{{{{}_{}}}}}", kind.label(), base32_prefix(&digest))
    }
}

/// HMAC-SHA256 as specified in RFC 2104, for keys of at most one block.
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    debug_assert!(key.len() <= BLOCK, "install keys are 32 bytes");
    let mut inner_pad = [0x36u8; BLOCK];
    let mut outer_pad = [0x5cu8; BLOCK];
    for (index, byte) in key.iter().enumerate() {
        inner_pad[index] ^= byte;
        outer_pad[index] ^= byte;
    }
    let inner = Sha256::new()
        .chain_update(inner_pad)
        .chain_update(message)
        .finalize();
    Sha256::new()
        .chain_update(outer_pad)
        .chain_update(inner)
        .finalize()
        .into()
}

/// The first `HASH_CHARS` base32 characters (40 bits) of a digest.
fn base32_prefix(digest: &[u8; 32]) -> String {
    let bits = digest[..5]
        .iter()
        .fold(0u64, |acc, byte| (acc << 8) | u64::from(*byte));
    (0..HASH_CHARS)
        .map(|index| {
            let shift = 35 - index * 5;
            BASE32_ALPHABET[((bits >> shift) & 0x1f) as usize] as char
        })
        .collect()
}

pub(crate) static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| {
    let kinds = Kind::ALL.map(Kind::label).join("|");
    Regex::new(&format!(
        r"\{{\{{(?:{kinds})_[a-z2-7]{{{HASH_CHARS}}}\}}\}}"
    ))
    .expect("the placeholder pattern is a constant")
});

/// Byte ranges of every placeholder-shaped substring. Used to keep detectors
/// away from text that is already masked.
pub(crate) fn placeholder_spans(text: &str) -> Vec<(usize, usize)> {
    if !text.contains("{{") {
        return Vec::new();
    }
    PLACEHOLDER
        .find_iter(text)
        .map(|found| (found.start(), found.end()))
        .collect()
}

/// Whether `text` is a strict prefix of some placeholder, e.g. `{{API_K` or
/// `{{EMAIL_ab3`. A complete placeholder is not a strict prefix.
pub(crate) fn is_partial_placeholder(text: &str) -> bool {
    let Some(rest) = text.strip_prefix('{') else {
        return false;
    };
    let Some(rest) = rest.strip_prefix('{') else {
        return rest.is_empty();
    };
    Kind::ALL.iter().any(|kind| {
        let head = format!("{}_", kind.label());
        if rest.len() <= head.len() {
            return head.starts_with(rest);
        }
        let Some(tail) = rest.strip_prefix(head.as_str()) else {
            return false;
        };
        let hash_len = tail
            .bytes()
            .take_while(|byte| BASE32_ALPHABET.contains(byte))
            .count()
            .min(HASH_CHARS);
        let closing = &tail[hash_len..];
        if hash_len < HASH_CHARS {
            closing.is_empty()
        } else {
            closing.is_empty() || closing == "}"
        }
    })
}

/// The longest suffix of `text` that could be the start of a placeholder cut
/// off by the end of a streamed chunk. Empty when the text ends cleanly.
pub(crate) fn partial_placeholder_suffix(text: &str) -> &str {
    const LONGEST: usize = 2 + 11 + 1 + HASH_CHARS + 2;
    let window_start = text.len().saturating_sub(LONGEST - 1);
    text.char_indices()
        .skip_while(|(index, _)| *index < window_start)
        .filter(|(_, character)| *character == '{')
        .map(|(index, _)| &text[index..])
        .find(|suffix| is_partial_placeholder(suffix))
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn hmac_matches_the_rfc_4231_vector() {
        let digest = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex(&digest),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn placeholders_are_stable_per_key_and_differ_across_keys() {
        let key = HashKey::new([7; 32]);
        let first = key.placeholder(Kind::ApiKey, "sk-value");
        assert_eq!(first, key.placeholder(Kind::ApiKey, "sk-value"));
        assert_ne!(first, key.placeholder(Kind::ApiKey, "sk-other"));
        assert_ne!(
            first,
            HashKey::new([8; 32]).placeholder(Kind::ApiKey, "sk-value")
        );
        assert!(PLACEHOLDER.is_match(&first));
        assert_eq!(first.len(), "{{API_KEY_}}".len() + HASH_CHARS);
    }

    #[test]
    fn every_kind_produces_a_recognized_placeholder() {
        let key = HashKey::new([1; 32]);
        for kind in Kind::ALL {
            let placeholder = key.placeholder(kind, "value");
            assert_eq!(
                placeholder_spans(&placeholder),
                vec![(0, placeholder.len())]
            );
            for cut in 1..placeholder.len() {
                assert!(
                    is_partial_placeholder(&placeholder[..cut]),
                    "{}",
                    &placeholder[..cut]
                );
            }
            assert!(!is_partial_placeholder(&placeholder));
        }
    }

    #[test]
    fn partial_suffix_finds_only_real_placeholder_starts() {
        assert_eq!(partial_placeholder_suffix("hello {{API_K"), "{{API_K");
        assert_eq!(partial_placeholder_suffix("x {"), "{");
        assert_eq!(partial_placeholder_suffix("x {{"), "{{");
        assert_eq!(partial_placeholder_suffix("x {{EMAIL_abcd"), "{{EMAIL_abcd");
        assert_eq!(
            partial_placeholder_suffix("x {{EMAIL_abcdefgh}"),
            "{{EMAIL_abcdefgh}"
        );
        assert_eq!(partial_placeholder_suffix("x {{EMAIL_abcdefgh}}"), "");
        assert_eq!(partial_placeholder_suffix("fn main() {}"), "");
        assert_eq!(partial_placeholder_suffix("{{NOPE"), "");
        assert_eq!(partial_placeholder_suffix("{{EMAIL_ab1"), "");
        assert_eq!(partial_placeholder_suffix("plain text"), "");
        assert_eq!(partial_placeholder_suffix("中文 {{PH"), "{{PH");
    }
}
