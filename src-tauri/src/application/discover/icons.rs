//! Pictures for Discover cards.
//!
//! The window's content policy allows only same-origin and `data:` images, so
//! a logo is fetched natively and handed over as a data URL. Only a picture
//! the Discover section itself gave out is fetched: a featured server's logo,
//! a GitHub owner's avatar, or a registry icon seen in this session. Anything
//! else is refused before a request is made, so the renderer cannot turn this
//! into a way to fetch arbitrary addresses.

use std::path::{Path, PathBuf};

use base64::Engine;
use sha2::{Digest, Sha256};

/// Largest picture accepted.
pub(crate) const MAX_ICON_BYTES: usize = 1024 * 1024;

/// `https://github.com/<owner>.png?size=96`, the avatar form the Discover
/// section itself builds.
pub(crate) fn is_owner_avatar(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://github.com/") else {
        return false;
    };
    let Some(owner) = rest.strip_suffix(".png?size=96") else {
        return false;
    };
    !owner.is_empty()
        && owner.len() <= 100
        && owner
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Whether `url` is a picture the Discover section offered.
pub(crate) fn allowed<'a>(url: &str, offered: impl IntoIterator<Item = &'a str>) -> bool {
    if !url.starts_with("https://") || url.len() > 2048 || url.chars().any(char::is_control) {
        return false;
    }
    is_owner_avatar(url) || offered.into_iter().any(|known| known == url)
}

/// The picture's type, read from its first bytes rather than trusted from
/// the server.
pub(crate) fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        return Some("image/x-icon");
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_lowercase();
    let trimmed = head.trim_start_matches('\u{feff}').trim_start();
    if (trimmed.starts_with('<') && head.contains("<svg")) && !head.contains("<html") {
        return Some("image/svg+xml");
    }
    None
}

pub(crate) fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// Pictures are kept by the SHA-256 of their address.
pub(crate) fn cache_path(dir: &Path, url: &str) -> PathBuf {
    let digest = Sha256::digest(url.as_bytes());
    let name = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    dir.join("icons").join(format!("{name}.txt"))
}

/// A data URL read back from the cache, if it still looks like one.
pub(crate) fn read_cached(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    (text.starts_with("data:image/") && text.len() <= MAX_ICON_BYTES * 2).then_some(text)
}
