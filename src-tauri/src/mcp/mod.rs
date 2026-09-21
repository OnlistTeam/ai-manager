//! MCP (Model Context Protocol) server management module
//!
//! Handles validation, sync, import and export of MCP server configuration.
//!
//! ## Module layout
//!
//! - `validation` - server configuration validation
//! - `claude` - Claude MCP sync and import
//! - `codex` - Codex MCP sync and import (including TOML conversion)
//! - `gemini` - Gemini MCP sync and import
//! - `opencode` - OpenCode MCP sync and import (including local/remote format conversion)
//! - `hermes` - Hermes MCP sync and import

mod claude;
mod claude_desktop;
mod codex;
mod gemini;
mod grokbuild;
mod hermes;
mod opencode;
mod validation;

// Re-export the public API
pub use claude::{
    import_from_claude, remove_server_from_claude, sync_enabled_to_claude,
    sync_single_server_to_claude,
};
pub use claude_desktop::{
    import_from_claude_desktop, remove_server_from_claude_desktop,
    sync_single_server_to_claude_desktop,
};
pub use codex::{
    import_from_codex, remove_server_from_codex, sync_enabled_to_codex, sync_single_server_to_codex,
};
pub use gemini::{
    import_from_gemini, remove_server_from_gemini, sync_enabled_to_gemini,
    sync_single_server_to_gemini,
};
pub use grokbuild::{
    import_from_grokbuild, remove_server_from_grokbuild, sync_single_server_to_grokbuild,
};
pub use hermes::{import_from_hermes, remove_server_from_hermes, sync_single_server_to_hermes};
pub use opencode::{
    import_from_opencode, remove_server_from_opencode, sync_single_server_to_opencode,
};
