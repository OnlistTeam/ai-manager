//! The per-install key behind privacy-protection placeholders (ADR-0049).
//!
//! 32 random bytes, hex encoded, in a private file in the product data
//! directory. Keeping it across restarts keeps placeholders stable, so a
//! conversation that continues after a restart still hits the provider's
//! prompt cache. The key never enters the database, backups, logs or IPC.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::paths::product_data_dir;

const KEY_FILE_NAME: &str = "privacy-protection.key";
const KEY_BYTES: usize = 32;

pub fn key_path() -> PathBuf {
    product_data_dir().join(KEY_FILE_NAME)
}

/// Reads the key, creating it when it is missing or unreadable. A replaced
/// key only changes future placeholders; nothing already sent depends on it.
pub fn load_or_create(path: &Path) -> io::Result<[u8; KEY_BYTES]> {
    if let Some(key) = fs::read_to_string(path)
        .ok()
        .and_then(|text| decode_hex(text.trim()))
    {
        return Ok(key);
    }
    let key = random_key();
    write_private(path, &encode_hex(&key))?;
    Ok(key)
}

/// OS randomness through the `uuid` v4 generator already in the dependency
/// tree: two version-4 UUIDs carry 244 random bits.
pub fn random_key() -> [u8; KEY_BYTES] {
    let mut key = [0u8; KEY_BYTES];
    key[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    key[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    key
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(text: &str) -> Option<[u8; KEY_BYTES]> {
    if text.len() != KEY_BYTES * 2 || !text.is_ascii() {
        return None;
    }
    let mut key = [0u8; KEY_BYTES];
    for (index, slot) in key.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(key)
}

/// Temp file (owner-only on Unix) plus rename, so a crash never leaves a
/// half-written key.
fn write_private(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("key.tmp");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temp, path)
}

#[cfg(test)]
mod tests {
    use super::{decode_hex, encode_hex, load_or_create, random_key};

    #[test]
    fn the_key_is_created_once_and_then_reused() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("nested").join("privacy-protection.key");
        let first = load_or_create(&path).expect("create");
        assert_eq!(load_or_create(&path).expect("reuse"), first);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path)
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn a_corrupt_key_file_is_replaced() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("privacy-protection.key");
        std::fs::write(&path, "not hex").expect("seed");
        let key = load_or_create(&path).expect("replace");
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            encode_hex(&key)
        );
    }

    #[test]
    fn keys_are_random_and_hex_round_trips() {
        let key = random_key();
        assert_ne!(key, random_key());
        assert_eq!(decode_hex(&encode_hex(&key)), Some(key));
        assert_eq!(decode_hex("zz"), None);
    }
}
