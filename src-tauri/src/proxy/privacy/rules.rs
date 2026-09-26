//! What the user chose to mask (ADR-0049): keys and passwords, personal
//! information, and a list of their own words.

/// Words shorter than this would match ordinary text everywhere.
pub(crate) const MIN_WORD_CHARS: usize = 2;

/// The masking rules for one request. Built once per settings change, so
/// every request masked under the same settings uses exactly the same rules
/// in exactly the same order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rules {
    pub secrets: bool,
    pub personal: bool,
    /// Distinct words, longest (in bytes) first, then in byte order, so overlapping
    /// words always resolve the same way whatever order the user typed them.
    words: Vec<String>,
}

impl Rules {
    pub(crate) fn new(secrets: bool, personal: bool, words: &[String]) -> Self {
        let mut words: Vec<String> = words
            .iter()
            .map(|word| word.trim())
            .filter(|word| word.chars().count() >= MIN_WORD_CHARS)
            .map(str::to_owned)
            .collect();
        words.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        words.dedup();
        Self {
            secrets,
            personal,
            words,
        }
    }

    /// Nothing is masked.
    #[cfg(test)]
    pub(crate) fn off() -> Self {
        Self::new(false, false, &[])
    }

    /// Keys, passwords and personal information, no words.
    #[cfg(test)]
    pub(crate) fn detectors() -> Self {
        Self::new(true, true, &[])
    }

    pub(crate) fn words(&self) -> &[String] {
        &self.words
    }

    pub(crate) fn is_active(&self) -> bool {
        self.secrets || self.personal || !self.words.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::Rules;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|word| (*word).to_owned()).collect()
    }

    #[test]
    fn words_are_trimmed_deduplicated_and_ordered_longest_first() {
        let rules = Rules::new(
            false,
            false,
            &words(&["acme", " Project Kite ", "a", "", "acme", "张三", "kite"]),
        );
        assert_eq!(
            rules.words(),
            words(&["Project Kite", "张三", "acme", "kite"]).as_slice()
        );
        assert!(rules.is_active());
    }

    #[test]
    fn the_order_the_user_typed_does_not_change_the_rules() {
        let first = Rules::new(true, false, &words(&["beta", "alpha", "gamma-ray"]));
        let second = Rules::new(true, false, &words(&["gamma-ray", "alpha", "beta"]));
        assert_eq!(first, second);
    }

    #[test]
    fn rules_with_nothing_chosen_are_inactive() {
        assert!(!Rules::off().is_active());
        assert!(!Rules::new(false, false, &words(&["x", " "])).is_active());
        assert!(Rules::new(true, false, &[]).is_active());
        assert!(Rules::new(false, true, &[]).is_active());
    }
}
