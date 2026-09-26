//! Personal-data detectors: email addresses, Chinese mainland ID numbers,
//! Chinese mobile numbers and bank card numbers. Numbers are only accepted
//! when their checksum (ID) or Luhn check (card) holds and they are not part
//! of a longer run of digits or letters.

use std::sync::LazyLock;

use regex::Regex;

use super::placeholder::Kind;
use super::secrets::bounded;
use super::Detection;

fn compile(pattern: &str) -> Regex {
    Regex::new(pattern).expect("detector patterns are constants")
}

static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    compile(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9\-]+(?:\.[A-Za-z0-9\-]+)*\.[A-Za-z]{2,24}")
});

static ID_NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    compile(r"[1-9]\d{5}(?:18|19|20)\d{2}(?:0[1-9]|1[0-2])(?:0[1-9]|[12]\d|3[01])\d{3}[\dXx]")
});

static PHONE: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(?:(?:\+|00)86[ \-]?)?1[3-9]\d(?:[ \-]?\d{4}){2}"));

static BANK_CARD: LazyLock<Regex> =
    LazyLock::new(|| compile(r"[2-6]\d{3}(?:[ \-]?\d{4}){2,3}(?:[ \-]?\d{1,3})?"));

static IMAGE_SCALE_DOMAIN: LazyLock<Regex> = LazyLock::new(|| compile(r"^\d+(?:\.\d+)?x\."));

const EXAMPLE_DOMAINS: [&str; 3] = ["example.com", "example.org", "example.net"];
const RESERVED_SUFFIXES: [&str; 4] = [".example", ".test", ".invalid", ".localhost"];
const SERVICE_LOCAL_PARTS: [&str; 6] = [
    "noreply",
    "no-reply",
    "donotreply",
    "do-not-reply",
    "git",
    "mailer-daemon",
];

fn is_alphanumeric(character: char) -> bool {
    character.is_ascii_alphanumeric()
}

fn is_number_part(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '.'
}

fn is_real_email(address: &str) -> bool {
    let Some((local, domain)) = address.rsplit_once('@') else {
        return false;
    };
    let local = local.to_ascii_lowercase();
    let domain = domain.to_ascii_lowercase();
    let example = EXAMPLE_DOMAINS
        .iter()
        .any(|example| domain == *example || domain.ends_with(&format!(".{example}")));
    !example
        && !RESERVED_SUFFIXES
            .iter()
            .any(|suffix| domain.ends_with(suffix))
        && !SERVICE_LOCAL_PARTS.contains(&local.as_str())
        && !domain.contains("noreply")
        && !IMAGE_SCALE_DOMAIN.is_match(&domain)
}

fn digits(text: &str) -> Vec<u32> {
    text.chars().filter_map(|c| c.to_digit(10)).collect()
}

fn valid_id_checksum(text: &str) -> bool {
    const WEIGHTS: [u32; 17] = [7, 9, 10, 5, 8, 4, 2, 1, 6, 3, 7, 9, 10, 5, 8, 4, 2];
    const CHECK: [char; 11] = ['1', '0', 'X', '9', '8', '7', '6', '5', '4', '3', '2'];
    let values = digits(&text[..17]);
    if values.len() != 17 {
        return false;
    }
    let sum: u32 = values.iter().zip(WEIGHTS).map(|(v, w)| v * w).sum();
    let last = text[17..].chars().next().map(|c| c.to_ascii_uppercase());
    last == Some(CHECK[(sum % 11) as usize])
}

fn valid_luhn(values: &[u32]) -> bool {
    let sum: u32 = values
        .iter()
        .rev()
        .enumerate()
        .map(|(index, digit)| {
            if index % 2 == 1 {
                let doubled = digit * 2;
                if doubled > 9 {
                    doubled - 9
                } else {
                    doubled
                }
            } else {
                *digit
            }
        })
        .sum();
    sum % 10 == 0
}

pub(crate) fn detect_personal(text: &str, detection: &mut Detection) {
    if text.contains('@') {
        for found in EMAIL.find_iter(text) {
            if bounded(text, found.start(), found.end(), |c| {
                is_alphanumeric(c) || c == '-'
            }) && is_real_email(found.as_str())
            {
                detection.push(found.start(), found.end(), Kind::Email);
            }
        }
    }
    if !text.bytes().any(|byte| byte.is_ascii_digit()) {
        return;
    }
    for found in ID_NUMBER.find_iter(text) {
        if bounded(text, found.start(), found.end(), is_alphanumeric)
            && valid_id_checksum(found.as_str())
        {
            detection.push(found.start(), found.end(), Kind::IdNumber);
        }
    }
    for found in BANK_CARD.find_iter(text) {
        let values = digits(found.as_str());
        if bounded(text, found.start(), found.end(), is_number_part)
            && (13..=19).contains(&values.len())
            && valid_luhn(&values)
        {
            detection.push(found.start(), found.end(), Kind::BankCard);
        }
    }
    for found in PHONE.find_iter(text) {
        let before_is_plus = text[..found.start()].ends_with('+');
        if !before_is_plus && bounded(text, found.start(), found.end(), is_number_part) {
            detection.push(found.start(), found.end(), Kind::Phone);
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

    #[test]
    fn real_email_addresses_are_masked() {
        single(
            "mail zhang.san+ai@company.cn now",
            Kind::Email,
            "zhang.san+ai@company.cn",
        );
        single("<li.si@mail.qq.com>", Kind::Email, "li.si@mail.qq.com");
    }

    #[test]
    fn obviously_unreal_addresses_are_left_alone() {
        for text in [
            "user@example.com",
            "a@mail.example.org",
            "someone@site.test",
            "noreply@company.com",
            "12345+bot@users.noreply.github.com",
            "git@github.com:org/repo.git",
            "icon@2x.png",
            "logo@1.5x.webp",
            "npm i left-pad@1.3.0",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn id_numbers_need_a_valid_checksum() {
        single(
            "身份证 11010519491231002X 号",
            Kind::IdNumber,
            "11010519491231002X",
        );
        single(
            "id=440524188001010014",
            Kind::IdNumber,
            "440524188001010014",
        );
        assert!(found("110105194912310021").is_empty());
        assert!(found("a11010519491231002X").is_empty());
        assert!(found("11010519491231002X9").is_empty());
    }

    #[test]
    fn mainland_mobile_numbers_are_masked() {
        single("电话 13812345678。", Kind::Phone, "13812345678");
        single("call +86 139-1234-5678", Kind::Phone, "+86 139-1234-5678");
        single("tel:+8615912345678", Kind::Phone, "+8615912345678");
        single("138 1234 5678", Kind::Phone, "138 1234 5678");
    }

    #[test]
    fn digit_runs_that_are_not_phone_numbers_are_left_alone() {
        for text in [
            "1727312345678",
            "12812345678",
            "v13812345678",
            "0.13812345678",
            "id_13812345678",
            "138123456789",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn bank_cards_need_the_luhn_check() {
        single(
            "card 4111111111111111 exp",
            Kind::BankCard,
            "4111111111111111",
        );
        single(
            "6222 0212 3456 7890 128",
            Kind::BankCard,
            "6222 0212 3456 7890 128",
        );
        single("5500-0000-0000-0004", Kind::BankCard, "5500-0000-0000-0004");
        assert!(found("4111111111111112").is_empty());
        assert!(found("1727312345678901").is_empty());
        assert!(found("build 4111111111111111a").is_empty());
        assert!(found("0.4111111111111111").is_empty());
    }
}
