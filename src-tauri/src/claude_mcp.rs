use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::{atomic_write, get_claude_mcp_path};
use crate::error::AppError;

/// Commands that need to be wrapped with `cmd /c` on Windows
/// These commands are actually .cmd batch files on Windows and must be run via `cmd /c`
#[cfg(windows)]
const WINDOWS_WRAP_COMMANDS: &[&str] = &["npx", "npm", "yarn", "pnpm", "node", "bun", "deno"];

/// On Windows: convert `npx args...` into `cmd /c npx args...`
/// Fixes the "Windows requires 'cmd /c' wrapper to execute npx" warning reported by Claude Code /doctor
#[cfg(windows)]
fn wrap_command_for_windows(obj: &mut Map<String, Value>) {
    // Only handle the stdio type (default or explicit)
    let server_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("stdio");
    if server_type != "stdio" {
        return;
    }

    let Some(cmd) = obj.get("command").and_then(|v| v.as_str()) else {
        return;
    };

    // Already wrapped in cmd, don't wrap again
    if cmd.eq_ignore_ascii_case("cmd") || cmd.eq_ignore_ascii_case("cmd.exe") {
        return;
    }

    // Extract the command name (strip the .cmd suffix and path)
    let cmd_name = Path::new(cmd)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(cmd);

    let needs_wrap = WINDOWS_WRAP_COMMANDS
        .iter()
        .any(|&c| cmd_name.eq_ignore_ascii_case(c));

    if !needs_wrap {
        return;
    }

    // Build new args: ["/c", "<original command>", ...original args]
    let original_args = obj
        .get("args")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut new_args = vec![Value::String("/c".into()), Value::String(cmd.into())];
    new_args.extend(original_args);

    obj.insert("command".into(), Value::String("cmd".into()));
    obj.insert("args".into(), Value::Array(new_args));
}

/// No-op on non-Windows platforms
#[cfg(not(windows))]
fn wrap_command_for_windows(_obj: &mut Map<String, Value>) {
    // Nothing to do on non-Windows platforms
}

/// Detect whether a path is a WSL network path (e.g. \\wsl$\Ubuntu\... or \\wsl.localhost\Ubuntu\...)
/// WSL runs Linux, so it does not need the cmd /c wrapper
/// Note: only detects direct UNC paths; mapped drive letters (e.g. Z: -> \\wsl$\...) cannot be detected
#[cfg(windows)]
fn is_wsl_path(path: &Path) -> bool {
    use std::path::{Component, Prefix};
    if let Some(Component::Prefix(prefix)) = path.components().next() {
        match prefix.kind() {
            Prefix::UNC(server, _) | Prefix::VerbatimUNC(server, _) => {
                let s = server.to_string_lossy();
                s.eq_ignore_ascii_case("wsl$") || s.eq_ignore_ascii_case("wsl.localhost")
            }
            _ => false,
        }
    } else {
        false
    }
}

#[cfg(not(windows))]
fn is_wsl_path(_path: &Path) -> bool {
    false
}

fn user_config_path() -> PathBuf {
    get_claude_mcp_path()
}

fn read_json_value(path: &Path) -> Result<Value, AppError> {
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let content = fs::read_to_string(path).map_err(|e| AppError::io(path, e))?;
    let value: Value = serde_json::from_str(&content).map_err(|e| AppError::json(path, e))?;
    Ok(value)
}

fn write_json_value(path: &Path, value: &Value) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
    }
    let json =
        serde_json::to_string_pretty(value).map_err(|e| AppError::JsonSerialize { source: e })?;
    atomic_write(path, json.as_bytes())
}

pub fn read_mcp_json() -> Result<Option<String>, AppError> {
    let path = user_config_path();
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;
    Ok(Some(content))
}

/// Read the mcpServers map from ~/.claude.json
pub fn read_mcp_servers_map() -> Result<std::collections::HashMap<String, Value>, AppError> {
    let path = user_config_path();
    if !path.exists() {
        return Ok(std::collections::HashMap::new());
    }

    let root = read_json_value(&path)?;
    let servers = root
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    Ok(servers)
}

/// Write the given enabled MCP server map to the mcpServers field of the user-level ~/.claude.json
/// Only overwrites mcpServers; other fields are left unchanged
pub fn set_mcp_servers_map(
    servers: &std::collections::HashMap<String, Value>,
) -> Result<(), AppError> {
    let path = user_config_path();
    let mut root = if path.exists() {
        read_json_value(&path)?
    } else {
        serde_json::json!({})
    };

    // Build the mcpServers object: strip UI-only fields (enabled/source), keep only the actual MCP spec
    // Detect whether the target path is WSL; if so, skip the cmd /c wrapper
    let is_wsl_target = is_wsl_path(&path);
    if is_wsl_target {
        log::info!(
            "Detected WSL path, skipping cmd /c wrapper: {}",
            path.display()
        );
    }
    let mut out: Map<String, Value> = Map::new();
    for (id, spec) in servers.iter() {
        let mut obj = if let Some(map) = spec.as_object() {
            map.clone()
        } else {
            return Err(AppError::McpValidation(format!(
                "MCP server '{id}' is not an object"
            )));
        };

        if let Some(server_val) = obj.remove("server") {
            let server_obj = server_val.as_object().cloned().ok_or_else(|| {
                AppError::McpValidation(format!("MCP server '{id}' has a non-object server field"))
            })?;
            obj = server_obj;
        }

        obj.remove("enabled");
        obj.remove("source");
        obj.remove("id");
        obj.remove("name");
        obj.remove("description");
        obj.remove("tags");
        obj.remove("homepage");
        obj.remove("docs");

        // Automatically wrap npx/npm etc. commands as cmd /c on Windows (except for WSL paths)
        if !is_wsl_target {
            wrap_command_for_windows(&mut obj);
        }

        out.insert(id.clone(), Value::Object(obj));
    }

    {
        let obj = root
            .as_object_mut()
            .ok_or_else(|| AppError::Config("~/.claude.json root must be an object".into()))?;
        obj.insert("mcpServers".into(), Value::Object(out));
    }

    write_json_value(&path, &root)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Test Windows command wrapping
    /// Due to conditional compilation, this tests the no-op function on non-Windows platforms
    #[test]
    fn test_wrap_command_for_windows_npx() {
        let mut obj = json!({"command": "npx", "args": ["-y", "@upstash/context7-mcp"]})
            .as_object()
            .unwrap()
            .clone();
        wrap_command_for_windows(&mut obj);

        #[cfg(windows)]
        {
            assert_eq!(obj["command"], "cmd");
            assert_eq!(
                obj["args"],
                json!(["/c", "npx", "-y", "@upstash/context7-mcp"])
            );
        }

        #[cfg(not(windows))]
        {
            // Nothing happens on non-Windows platforms
            assert_eq!(obj["command"], "npx");
        }
    }

    #[test]
    fn test_wrap_command_for_windows_npm() {
        let mut obj = json!({"command": "npm", "args": ["run", "start"]})
            .as_object()
            .unwrap()
            .clone();
        wrap_command_for_windows(&mut obj);

        #[cfg(windows)]
        {
            assert_eq!(obj["command"], "cmd");
            assert_eq!(obj["args"], json!(["/c", "npm", "run", "start"]));
        }
    }

    #[test]
    fn test_wrap_command_for_windows_already_cmd() {
        // Already cmd, should not be wrapped again
        let mut obj = json!({"command": "cmd", "args": ["/c", "npx", "-y", "foo"]})
            .as_object()
            .unwrap()
            .clone();
        wrap_command_for_windows(&mut obj);

        assert_eq!(obj["command"], "cmd");
        // args should remain unchanged, not become ["/c", "cmd", "/c", "npx", ...]
        assert_eq!(obj["args"], json!(["/c", "npx", "-y", "foo"]));
    }

    #[test]
    fn test_wrap_command_for_windows_http_type_skipped() {
        // http type should not be processed
        let mut obj = json!({"type": "http", "url": "https://example.com/mcp"})
            .as_object()
            .unwrap()
            .clone();
        wrap_command_for_windows(&mut obj);

        assert!(!obj.contains_key("command"));
        assert_eq!(obj["url"], "https://example.com/mcp");
    }

    #[test]
    fn test_wrap_command_for_windows_other_command_skipped() {
        // Non-target commands (e.g. python) should not be wrapped
        let mut obj = json!({"command": "python", "args": ["server.py"]})
            .as_object()
            .unwrap()
            .clone();
        wrap_command_for_windows(&mut obj);

        // python is not in the WINDOWS_WRAP_COMMANDS list, should not be wrapped
        assert_eq!(obj["command"], "python");
        assert_eq!(obj["args"], json!(["server.py"]));
    }

    #[test]
    fn test_wrap_command_for_windows_no_args() {
        // Case with no args
        let mut obj = json!({"command": "npx"}).as_object().unwrap().clone();
        wrap_command_for_windows(&mut obj);

        #[cfg(windows)]
        {
            assert_eq!(obj["command"], "cmd");
            assert_eq!(obj["args"], json!(["/c", "npx"]));
        }
    }

    #[test]
    fn test_wrap_command_for_windows_with_cmd_suffix() {
        // Handle the npx.cmd format
        let mut obj = json!({"command": "npx.cmd", "args": ["-y", "foo"]})
            .as_object()
            .unwrap()
            .clone();
        wrap_command_for_windows(&mut obj);

        #[cfg(windows)]
        {
            assert_eq!(obj["command"], "cmd");
            assert_eq!(obj["args"], json!(["/c", "npx.cmd", "-y", "foo"]));
        }
    }

    #[test]
    fn test_wrap_command_for_windows_case_insensitive() {
        // Case-insensitive
        let mut obj = json!({"command": "NPX", "args": ["-y", "foo"]})
            .as_object()
            .unwrap()
            .clone();
        wrap_command_for_windows(&mut obj);

        #[cfg(windows)]
        {
            assert_eq!(obj["command"], "cmd");
            assert_eq!(obj["args"], json!(["/c", "NPX", "-y", "foo"]));
        }
    }

    /// Test WSL path detection
    #[test]
    fn test_is_wsl_path_wsl_dollar() {
        // wsl$ format - various distros
        #[cfg(windows)]
        {
            assert!(is_wsl_path(Path::new(r"\\wsl$\Ubuntu\home\user\.claude")));
            assert!(is_wsl_path(Path::new(r"\\wsl$\Debian\home\user\.claude")));
            assert!(is_wsl_path(Path::new(
                r"\\wsl$\openSUSE-Leap-15.2\home\user"
            )));
            assert!(is_wsl_path(Path::new(r"\\wsl$\kali-linux\home\user")));
            assert!(is_wsl_path(Path::new(r"\\wsl$\Arch\home\user")));
            assert!(is_wsl_path(Path::new(r"\\wsl$\Alpine\home\user")));
            assert!(is_wsl_path(Path::new(r"\\wsl$\Fedora\home\user")));
        }

        #[cfg(not(windows))]
        {
            // Always returns false on non-Windows platforms
            assert!(!is_wsl_path(Path::new(r"\\wsl$\Ubuntu\home\user\.claude")));
        }
    }

    #[test]
    fn test_is_wsl_path_wsl_localhost() {
        // wsl.localhost format
        #[cfg(windows)]
        {
            assert!(is_wsl_path(Path::new(
                r"\\wsl.localhost\Ubuntu\home\user\.claude"
            )));
            assert!(is_wsl_path(Path::new(r"\\wsl.localhost\Debian\home\user")));
            assert!(is_wsl_path(Path::new(
                r"\\wsl.localhost\openSUSE-Leap-15.2\home\user"
            )));
        }
    }

    #[test]
    fn test_is_wsl_path_case_insensitive() {
        // Case-insensitive
        #[cfg(windows)]
        {
            assert!(is_wsl_path(Path::new(r"\\WSL$\Ubuntu\home\user")));
            assert!(is_wsl_path(Path::new(r"\\Wsl$\Ubuntu\home\user")));
            assert!(is_wsl_path(Path::new(r"\\WSL.LOCALHOST\Ubuntu\home\user")));
            assert!(is_wsl_path(Path::new(r"\\Wsl.Localhost\Ubuntu\home\user")));
        }
    }

    #[test]
    fn test_is_wsl_path_non_wsl() {
        // Non-WSL paths
        assert!(!is_wsl_path(Path::new(r"C:\Users\user\.claude")));
        assert!(!is_wsl_path(Path::new(r"D:\Workspace\project")));
        #[cfg(windows)]
        {
            assert!(!is_wsl_path(Path::new(r"\\server\share\path")));
            assert!(!is_wsl_path(Path::new(r"\\localhost\c$\Users")));
            assert!(!is_wsl_path(Path::new(r"\\192.168.1.1\share")));
        }
    }
}
