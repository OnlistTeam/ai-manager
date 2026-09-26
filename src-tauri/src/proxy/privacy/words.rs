//! The user's own words to hide: names, codenames, host names.
//!
//! Matching is case-sensitive. A word is matched as a whole token: where the
//! word starts or ends with a letter, digit or `_`, the character next to it
//! must not be one too, so `acme` is not found inside `acmeish` or `my_acme`.
//! Chinese, Japanese and Korean characters have no word boundaries, so a
//! word edge or a neighbour in those scripts never blocks a match: `张三` is
//! found inside `张三丰`, and `acme` inside `acme公司`.

use super::placeholder::Kind;
use super::Detection;

fn is_cjk(character: char) -> bool {
    matches!(
        u32::from(character),
        0x3040..=0x30FF   // Hiragana, Katakana
            | 0x3400..=0x4DBF // CJK Extension A
            | 0x4E00..=0x9FFF // CJK Unified Ideographs
            | 0xAC00..=0xD7AF // Hangul syllables
            | 0xF900..=0xFAFF // CJK Compatibility Ideographs
            | 0x20000..=0x3134F // CJK Extensions B to G
    )
}

/// A character that continues a token in a script with word boundaries.
fn is_word_char(character: char) -> bool {
    (character.is_alphanumeric() || character == '_') && !is_cjk(character)
}

/// Whether `text[start..end]`, an occurrence of `word`, stands on its own.
fn is_whole_token(text: &str, start: usize, end: usize, word: &str) -> bool {
    let starts_in_word = word.chars().next().is_some_and(is_word_char);
    let ends_in_word = word.chars().next_back().is_some_and(is_word_char);
    let joined_before =
        starts_in_word && text[..start].chars().next_back().is_some_and(is_word_char);
    let joined_after = ends_in_word && text[end..].chars().next().is_some_and(is_word_char);
    !joined_before && !joined_after
}

/// Appends a span for every whole-token occurrence of every word. `words`
/// comes from [`super::rules::Rules`], longest first, so a longer word wins
/// over a shorter one it contains.
pub(crate) fn detect_words(text: &str, words: &[String], detection: &mut Detection) {
    for word in words {
        for (start, _) in text.match_indices(word.as_str()) {
            let end = start + word.len();
            if is_whole_token(text, start, end, word) {
                detection.push(start, end, Kind::Word);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::placeholder::Kind;
    use super::super::rules::Rules;
    use super::super::{detect_with, Span};

    fn found<'a>(text: &'a str, words: &[&str]) -> Vec<(Kind, &'a str)> {
        let words: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
        detect_with(text, &Rules::new(false, false, &words), &[])
            .into_iter()
            .map(|Span { start, end, kind }| (kind, &text[start..end]))
            .collect()
    }

    #[test]
    fn ascii_words_match_only_as_whole_tokens() {
        assert_eq!(
            found("acme, acme-db and acme.internal", &["acme"]),
            vec![
                (Kind::Word, "acme"),
                (Kind::Word, "acme"),
                (Kind::Word, "acme")
            ]
        );
        assert!(found("acmeish my_acme acme2 xacme", &["acme"]).is_empty());
    }

    #[test]
    fn words_are_case_sensitive() {
        assert_eq!(
            found("Kite kite KITE", &["Kite"]),
            vec![(Kind::Word, "Kite")]
        );
    }

    #[test]
    fn cjk_words_match_as_substrings() {
        assert_eq!(
            found("张三丰和老张三说", &["张三"]),
            vec![(Kind::Word, "张三"), (Kind::Word, "张三")]
        );
        assert_eq!(
            found("acme公司的服务器", &["acme"]),
            vec![(Kind::Word, "acme")]
        );
        // A single character is below the minimum length and never a word.
        assert_eq!(found("プロジェクト桜です", &["桜"]), Vec::new());
        assert_eq!(
            found("プロジェクト桜です", &["桜で"]),
            vec![(Kind::Word, "桜で")]
        );
    }

    #[test]
    fn words_with_inner_spaces_and_punctuation_match_whole() {
        assert_eq!(
            found("ask Project Kite now", &["Project Kite", "Kite"]),
            vec![(Kind::Word, "Project Kite")]
        );
        assert_eq!(
            found("ssh db-01.corp.lan", &["db-01.corp.lan"]),
            vec![(Kind::Word, "db-01.corp.lan")]
        );
    }

    #[test]
    fn a_word_never_matches_inside_a_placeholder() {
        assert!(found("x {{WORD_abcdefgh}} y", &["{{WORD_abcdefgh}}"]).is_empty());
    }
}
