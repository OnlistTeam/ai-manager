//! Bounded in-memory map from placeholder back to the real value.
//!
//! Every masked request refreshes the entries it uses, so the values of the
//! conversation in flight are never the ones evicted. When the map is full
//! the least recently used half is dropped; a dropped value is simply masked
//! again, to the same placeholder, the next time it is sent.

use std::collections::HashMap;

use super::placeholder::Kind;
use super::KnownValue;

pub(crate) const DEFAULT_CAPACITY: usize = 4096;
/// Context-recognized values kept for verbatim matching. Short values are
/// not kept because they would match ordinary words.
const KNOWN_CAPACITY: usize = 256;
const KNOWN_MIN_CHARS: usize = 6;

pub(crate) struct Vault {
    capacity: usize,
    clock: u64,
    entries: HashMap<String, (String, u64)>,
    known: Vec<KnownValue>,
}

impl Vault {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(2),
            clock: 0,
            entries: HashMap::new(),
            known: Vec::new(),
        }
    }

    pub(crate) fn insert(&mut self, placeholder: String, value: String) {
        self.clock += 1;
        let stamp = self.clock;
        match self.entries.get_mut(&placeholder) {
            // A 40-bit collision between two different values keeps the first
            // one; the second is still masked, only its restore would be wrong.
            Some(entry) => entry.1 = stamp,
            None => {
                if self.entries.len() >= self.capacity {
                    self.evict_oldest_half();
                }
                self.entries.insert(placeholder, (value, stamp));
            }
        }
    }

    /// Remembers a value found by its context; the most recent ones stay.
    pub(crate) fn remember_known(&mut self, value: &str, kind: Kind) {
        if !kind.is_contextual() || value.chars().count() < KNOWN_MIN_CHARS {
            return;
        }
        self.known.retain(|known| known.value != value);
        if self.known.len() >= KNOWN_CAPACITY {
            self.known.remove(0);
        }
        self.known.push(KnownValue {
            value: value.to_owned(),
            kind,
        });
    }

    pub(crate) fn known_values(&self) -> Vec<KnownValue> {
        self.known
            .iter()
            .map(|known| KnownValue {
                value: known.value.clone(),
                kind: known.kind,
            })
            .collect()
    }

    pub(crate) fn get(&self, placeholder: &str) -> Option<&str> {
        self.entries
            .get(placeholder)
            .map(|(value, _)| value.as_str())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    fn evict_oldest_half(&mut self) {
        let mut stamps: Vec<u64> = self.entries.values().map(|(_, stamp)| *stamp).collect();
        stamps.sort_unstable();
        let cutoff = stamps[stamps.len() / 2];
        self.entries.retain(|_, (_, stamp)| *stamp >= cutoff);
    }
}

#[cfg(test)]
mod tests {
    use super::super::placeholder::Kind;
    use super::Vault;

    #[test]
    fn a_full_vault_drops_the_least_recently_used_half() {
        let mut vault = Vault::new(4);
        for index in 0..4 {
            vault.insert(format!("p{index}"), format!("v{index}"));
        }
        // Touch p0 so it is the most recent entry.
        vault.insert("p0".into(), "v0".into());
        vault.insert("p4".into(), "v4".into());
        assert_eq!(vault.len(), 3);
        assert_eq!(vault.get("p0"), Some("v0"));
        assert_eq!(vault.get("p4"), Some("v4"));
        assert_eq!(vault.get("p1"), None);
        assert_eq!(vault.get("p2"), None);
    }

    #[test]
    fn the_first_value_of_a_placeholder_wins() {
        let mut vault = Vault::new(8);
        vault.insert("p".into(), "first".into());
        vault.insert("p".into(), "second".into());
        assert_eq!(vault.get("p"), Some("first"));
    }

    #[test]
    fn only_contextual_values_of_useful_length_are_remembered() {
        let mut vault = Vault::new(8);
        vault.remember_known("abc123def456", Kind::Secret);
        vault.remember_known("hunter2", Kind::Password);
        vault.remember_known("abc12", Kind::Secret);
        vault.remember_known("sk-abc123def456", Kind::ApiKey);
        vault.remember_known("abc123def456", Kind::Secret);
        let known: Vec<String> = vault
            .known_values()
            .into_iter()
            .map(|known| known.value)
            .collect();
        assert_eq!(known, vec!["hunter2".to_owned(), "abc123def456".to_owned()]);
    }
}
