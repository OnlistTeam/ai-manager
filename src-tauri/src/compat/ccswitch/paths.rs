//! Narrow product-facing access to inherited path resolution.

use std::path::PathBuf;

/// Resolves the same user home as the inherited engine, including its
/// test-only isolated-home override. Product platform code must not import the
/// upstream `config` module directly.
pub fn home_dir() -> PathBuf {
    crate::config::get_home_dir()
}
