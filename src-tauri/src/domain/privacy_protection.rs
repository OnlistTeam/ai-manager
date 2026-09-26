//! What privacy protection hides in proxied traffic (ADR-0049).
//!
//! Only the user's choices cross the wire: no masked value, placeholder,
//! count per value or hash key ever does. The words are the user's own
//! content, so they are never logged: `Debug` prints only how many there are.

use serde::{Deserialize, Serialize};

use super::{AppError, ErrorCode};

pub const MIN_PRIVACY_WORD_CHARS: usize = 2;
pub const MAX_PRIVACY_WORD_CHARS: usize = 128;
pub const MAX_PRIVACY_WORDS: usize = 100;

/// Characters that separate words in the "Words to hide" field: ASCII and
/// fullwidth commas, the ideographic enumeration comma, and line breaks.
const WORD_SEPARATORS: [char; 5] = [',', '，', '、', '\n', '\r'];

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyProtection {
    /// API keys, private keys, tokens and passwords. On by default.
    pub mask_secrets: bool,
    /// Emails, phone numbers, ID numbers and bank card numbers. Off by
    /// default: test data trips it far more often than keys do.
    pub mask_personal: bool,
    /// The user's own words, normalized by [`normalize_privacy_words`].
    pub words: Vec<String>,
}

impl Default for PrivacyProtection {
    fn default() -> Self {
        Self {
            mask_secrets: true,
            mask_personal: false,
            words: Vec::new(),
        }
    }
}

impl std::fmt::Debug for PrivacyProtection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PrivacyProtection")
            .field("mask_secrets", &self.mask_secrets)
            .field("mask_personal", &self.mask_personal)
            .field("words", &self.words.len())
            .finish()
    }
}

/// A partial change; absent fields keep their stored value.
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrivacyProtectionPatch {
    pub mask_secrets: Option<bool>,
    pub mask_personal: Option<bool>,
    pub words: Option<Vec<String>>,
}

impl std::fmt::Debug for PrivacyProtectionPatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PrivacyProtectionPatch")
            .field("mask_secrets", &self.mask_secrets)
            .field("mask_personal", &self.mask_personal)
            .field("words", &self.words.as_ref().map(Vec::len))
            .finish()
    }
}

impl PrivacyProtection {
    /// The stored settings with `patch` applied; words are normalized.
    pub fn apply(self, patch: PrivacyProtectionPatch) -> Result<Self, AppError> {
        Ok(Self {
            mask_secrets: patch.mask_secrets.unwrap_or(self.mask_secrets),
            mask_personal: patch.mask_personal.unwrap_or(self.mask_personal),
            words: match patch.words {
                Some(words) => normalize_privacy_words(&words)?,
                None => self.words,
            },
        })
    }
}

fn invalid(message_key: &'static str, technical: &'static str) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, message_key).with_technical(technical)
}

/// Splits every entry at commas and line breaks, trims each piece, drops
/// pieces shorter than two characters and repeats (the first one stays), and
/// rejects a piece longer than 128 characters or more than 100 words.
pub fn normalize_privacy_words(entries: &[String]) -> Result<Vec<String>, AppError> {
    let mut words: Vec<String> = Vec::new();
    for piece in entries
        .iter()
        .flat_map(|entry| entry.split(WORD_SEPARATORS))
    {
        let word = piece.trim();
        if word.chars().count() < MIN_PRIVACY_WORD_CHARS || words.iter().any(|w| w == word) {
            continue;
        }
        if word.chars().count() > MAX_PRIVACY_WORD_CHARS {
            return Err(invalid(
                "error.privacy.wordTooLong",
                "a privacy word is longer than the limit",
            ));
        }
        words.push(word.to_owned());
    }
    if words.len() > MAX_PRIVACY_WORDS {
        return Err(invalid(
            "error.privacy.tooManyWords",
            "too many privacy words",
        ));
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_privacy_words, PrivacyProtection, PrivacyProtectionPatch, MAX_PRIVACY_WORDS,
        MAX_PRIVACY_WORD_CHARS,
    };

    fn owned(list: &[&str]) -> Vec<String> {
        list.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn defaults_hide_keys_only() {
        let defaults = PrivacyProtection::default();
        assert!(defaults.mask_secrets);
        assert!(!defaults.mask_personal);
        assert!(defaults.words.is_empty());
    }

    #[test]
    fn the_wire_carries_only_the_choices() {
        assert_eq!(
            serde_json::to_string(&PrivacyProtection {
                words: owned(&["acme"]),
                ..PrivacyProtection::default()
            })
            .expect("serialize"),
            r#"{"maskSecrets":true,"maskPersonal":false,"words":["acme"]}"#
        );
    }

    #[test]
    fn debug_output_never_contains_the_words() {
        let settings = PrivacyProtection {
            words: owned(&["Project Kite"]),
            ..PrivacyProtection::default()
        };
        let patch = PrivacyProtectionPatch {
            words: Some(owned(&["Project Kite"])),
            ..PrivacyProtectionPatch::default()
        };
        for text in [format!("{settings:?}"), format!("{patch:?}")] {
            assert!(!text.contains("Kite"), "{text}");
        }
    }

    #[test]
    fn commas_fullwidth_commas_and_line_breaks_all_separate_words() {
        assert_eq!(
            normalize_privacy_words(&owned(&[
                "acme, Project Kite，张三、李四\nhost-01\r\nx, , acme"
            ]))
            .expect("valid"),
            owned(&["acme", "Project Kite", "张三", "李四", "host-01"])
        );
        assert_eq!(
            normalize_privacy_words(&owned(&["  ", "a"])).expect("valid"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn words_over_the_limits_are_rejected() {
        let long = "x".repeat(MAX_PRIVACY_WORD_CHARS + 1);
        assert_eq!(
            normalize_privacy_words(&[long])
                .expect_err("too long")
                .message_key,
            "error.privacy.wordTooLong"
        );
        let exact = "字".repeat(MAX_PRIVACY_WORD_CHARS);
        assert_eq!(
            normalize_privacy_words(std::slice::from_ref(&exact)).expect("at the limit"),
            vec![exact]
        );
        let many: Vec<String> = (0..=MAX_PRIVACY_WORDS).map(|i| format!("w{i}")).collect();
        assert_eq!(
            normalize_privacy_words(&many)
                .expect_err("too many")
                .message_key,
            "error.privacy.tooManyWords"
        );
        assert_eq!(
            normalize_privacy_words(&many[..MAX_PRIVACY_WORDS])
                .expect("at the limit")
                .len(),
            MAX_PRIVACY_WORDS
        );
    }

    #[test]
    fn a_patch_changes_only_what_it_names() {
        let stored = PrivacyProtection {
            words: owned(&["acme"]),
            ..PrivacyProtection::default()
        };
        let patched = stored
            .clone()
            .apply(PrivacyProtectionPatch {
                mask_personal: Some(true),
                ..PrivacyProtectionPatch::default()
            })
            .expect("apply");
        assert_eq!(
            patched,
            PrivacyProtection {
                mask_personal: true,
                ..stored.clone()
            }
        );
        let cleared = stored
            .apply(PrivacyProtectionPatch {
                words: Some(Vec::new()),
                mask_secrets: Some(false),
                ..PrivacyProtectionPatch::default()
            })
            .expect("apply");
        assert!(cleared.words.is_empty());
        assert!(!cleared.mask_secrets);
    }

    #[test]
    fn a_patch_rejects_unknown_fields() {
        assert!(serde_json::from_str::<PrivacyProtectionPatch>(r#"{"enabled":true}"#).is_err());
        assert_eq!(
            serde_json::from_str::<PrivacyProtectionPatch>(r#"{"maskPersonal":true}"#)
                .expect("patch"),
            PrivacyProtectionPatch {
                mask_personal: Some(true),
                ..PrivacyProtectionPatch::default()
            }
        );
    }
}
