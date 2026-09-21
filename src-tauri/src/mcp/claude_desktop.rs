//! Claude Desktop MCP projection using the vendor-documented user config.
//!
//! This is distinct from Claude Code's `~/.claude.json` and from the 1P/3P
//! inference profile managed by `claude_desktop_config.rs` (ADR-0021).

use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::app_config::{McpApps, McpServer, MultiAppConfig};
use crate::config::{atomic_write_private, delete_file};
use crate::error::AppError;
use crate::platform::claude_desktop_mcp_config_path;

use super::validation::validate_server_spec;

const MAX_CONFIG_BYTES: u64 = 4 * 1024 * 1024;

pub fn import_from_claude_desktop(config: &mut MultiAppConfig) -> Result<usize, AppError> {
    let Some(path) = claude_desktop_mcp_config_path() else {
        return Ok(0);
    };
    let Some(bytes) = read_bounded(&path)? else {
        return Ok(0);
    };
    let root = parse_and_validate(&path, &bytes)?;
    let Some(live) = root.get("mcpServers").and_then(Value::as_object) else {
        return Ok(0);
    };
    let servers = config.mcp.servers.get_or_insert_with(HashMap::new);
    let mut changed = 0;
    for (id, spec) in live {
        if let Some(existing) = servers.get_mut(id) {
            if !existing.apps.claude_desktop {
                existing.apps.claude_desktop = true;
                changed += 1;
            }
        } else {
            servers.insert(
                id.clone(),
                McpServer {
                    id: id.clone(),
                    name: id.clone(),
                    server: spec.clone(),
                    apps: McpApps {
                        claude_desktop: true,
                        ..McpApps::default()
                    },
                    description: None,
                    homepage: None,
                    docs: None,
                    tags: Vec::new(),
                },
            );
            changed += 1;
        }
    }
    Ok(changed)
}

pub fn sync_single_server_to_claude_desktop(
    _config: &MultiAppConfig,
    id: &str,
    server_spec: &Value,
) -> Result<(), AppError> {
    validate_id(id)?;
    validate_server_spec(server_spec)?;
    let Some(path) = claude_desktop_mcp_config_path() else {
        return Ok(());
    };
    mutate_config(&path, |root| {
        let servers = ensure_servers(root)?;
        servers.insert(id.to_string(), server_spec.clone());
        Ok(())
    })
}

pub fn remove_server_from_claude_desktop(id: &str) -> Result<(), AppError> {
    validate_id(id)?;
    let Some(path) = claude_desktop_mcp_config_path() else {
        return Ok(());
    };
    if !path.exists() {
        return Ok(());
    }
    mutate_config(&path, |root| {
        if let Some(servers) = root.get_mut("mcpServers").and_then(Value::as_object_mut) {
            servers.remove(id);
        }
        Ok(())
    })
}

fn validate_id(id: &str) -> Result<(), AppError> {
    if id.trim().is_empty() || id.len() > 256 || id.chars().any(char::is_control) {
        return Err(AppError::McpValidation(
            "Claude Desktop MCP server id is invalid".to_string(),
        ));
    }
    Ok(())
}

fn ensure_servers(root: &mut Value) -> Result<&mut Map<String, Value>, AppError> {
    let object = root.as_object_mut().ok_or_else(|| {
        AppError::McpValidation("Claude Desktop config root must be an object".to_string())
    })?;
    if !object.contains_key("mcpServers") {
        object.insert("mcpServers".to_string(), Value::Object(Map::new()));
    }
    object
        .get_mut("mcpServers")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            AppError::McpValidation(
                "Claude Desktop config mcpServers must be an object".to_string(),
            )
        })
}

fn read_bounded(path: &Path) -> Result<Option<Vec<u8>>, AppError> {
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::metadata(path).map_err(|source| AppError::io(path, source))?;
    if !metadata.is_file() {
        return Err(AppError::Config(
            "Claude Desktop config path is not a regular file".to_string(),
        ));
    }
    if metadata.len() > MAX_CONFIG_BYTES {
        return Err(AppError::Config(
            "Claude Desktop config exceeds the 4 MiB safety limit".to_string(),
        ));
    }
    fs::read(path)
        .map(Some)
        .map_err(|source| AppError::io(path, source))
}

fn parse_and_validate(path: &Path, bytes: &[u8]) -> Result<Value, AppError> {
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err(AppError::Config(
            "Claude Desktop config exceeds the 4 MiB safety limit".to_string(),
        ));
    }
    let root: Value =
        serde_json::from_slice(bytes).map_err(|source| AppError::json(path, source))?;
    let object = root.as_object().ok_or_else(|| {
        AppError::McpValidation("Claude Desktop config root must be an object".to_string())
    })?;
    if let Some(raw_servers) = object.get("mcpServers") {
        let servers = raw_servers.as_object().ok_or_else(|| {
            AppError::McpValidation(
                "Claude Desktop config mcpServers must be an object".to_string(),
            )
        })?;
        for (id, spec) in servers {
            validate_id(id)?;
            validate_server_spec(spec)?;
        }
    }
    Ok(root)
}

fn mutate_config<F>(path: &Path, modify: F) -> Result<(), AppError>
where
    F: FnOnce(&mut Value) -> Result<(), AppError>,
{
    // Read + Validate + Backup.
    let snapshot = read_bounded(path)?;
    let mut root = match snapshot.as_deref() {
        Some(bytes) => parse_and_validate(path, bytes)?,
        None => Value::Object(Map::new()),
    };
    let before = root.clone();

    // Modify and validate the in-memory result before any external write.
    modify(&mut root)?;
    let bytes =
        serde_json::to_vec_pretty(&root).map_err(|source| AppError::JsonSerialize { source })?;
    parse_and_validate(path, &bytes)?;
    if root == before && snapshot.is_some() {
        return Ok(());
    }

    // Temp Write + Validate. The validation file is private and uniquely named;
    // the final helper performs the platform-specific atomic replacement.
    let temp = validation_temp_path(path)?;
    let write_result = (|| {
        atomic_write_private(&temp, &bytes)?;
        let staged = read_bounded(&temp)?.ok_or_else(|| {
            AppError::Config("Claude Desktop validation file disappeared".to_string())
        })?;
        let staged_root = parse_and_validate(&temp, &staged)?;
        if staged_root != root {
            return Err(AppError::Conflict(
                "Claude Desktop validation file changed before replace".to_string(),
            ));
        }

        // Atomic Replace + Verify.
        atomic_write_private(path, &staged)?;
        let written = read_bounded(path)?.ok_or_else(|| {
            AppError::Config("Claude Desktop config is absent after write".to_string())
        })?;
        let verified = parse_and_validate(path, &written)?;
        if verified != root {
            return Err(AppError::Conflict(
                "Claude Desktop config did not match after write".to_string(),
            ));
        }
        Ok(())
    })();
    let _ = delete_file(&temp);

    if let Err(error) = write_result {
        return match restore_snapshot(path, snapshot.as_deref()) {
            Ok(()) => Err(error),
            Err(rollback) => Err(AppError::Config(format!(
                "Claude Desktop MCP write failed and rollback failed: {rollback}"
            ))),
        };
    }
    Ok(())
}

fn validation_temp_path(path: &Path) -> Result<PathBuf, AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Config("Claude Desktop config path has no parent".to_string()))?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::Config("Claude Desktop config filename is invalid".to_string()))?;
    Ok(parent.join(format!(
        ".{name}.ai-manager-validate-{}.tmp",
        uuid::Uuid::new_v4().simple()
    )))
}

fn restore_snapshot(path: &Path, snapshot: Option<&[u8]>) -> Result<(), AppError> {
    match snapshot {
        Some(bytes) => {
            atomic_write_private(path, bytes)?;
            let restored = read_bounded(path)?.ok_or_else(|| {
                AppError::Config("Claude Desktop config is absent after rollback".to_string())
            })?;
            if restored != bytes {
                return Err(AppError::Conflict(
                    "Claude Desktop config rollback did not restore exact bytes".to_string(),
                ));
            }
        }
        None => delete_file(path)?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn transaction_preserves_unknown_roots_and_can_add_then_remove_a_server() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("claude_desktop_config.json");
        fs::write(
            &path,
            br#"{"deploymentMode":"3p","mcpServers":{"old":{"command":"old"}}}"#,
        )
        .expect("seed config");

        mutate_config(&path, |root| {
            ensure_servers(root)?.insert(
                "new".to_string(),
                json!({"type":"http","url":"https://example.com/mcp"}),
            );
            Ok(())
        })
        .expect("add server");
        let after_add = parse_and_validate(&path, &fs::read(&path).expect("read config after add"))
            .expect("parse config after add");
        assert_eq!(after_add["deploymentMode"], "3p");
        assert!(after_add["mcpServers"].get("old").is_some());
        assert!(after_add["mcpServers"].get("new").is_some());

        mutate_config(&path, |root| {
            ensure_servers(root)?.remove("new");
            Ok(())
        })
        .expect("remove server");
        let after_remove: Value =
            serde_json::from_slice(&fs::read(&path).expect("read after remove"))
                .expect("parse after remove");
        assert!(after_remove["mcpServers"].get("new").is_none());
        assert_eq!(after_remove["deploymentMode"], "3p");
    }

    #[test]
    fn malformed_existing_config_is_rejected_without_replacing_it() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("claude_desktop_config.json");
        let original = br#"{"mcpServers":[]}"#;
        fs::write(&path, original).expect("seed malformed config");

        assert!(mutate_config(&path, |_| Ok(())).is_err());
        assert_eq!(fs::read(&path).expect("read unchanged config"), original);
    }

    #[test]
    fn every_live_server_is_validated_before_it_can_be_imported_or_rewritten() {
        let path = Path::new("claude_desktop_config.json");
        assert!(parse_and_validate(path, br#"{"mcpServers":{"bad":{"type":"stdio"}}}"#).is_err());
        assert!(parse_and_validate(
            path,
            br#"{"mcpServers":{"ok":{"command":"npx","args":["-y","server"]}}}"#
        )
        .is_ok());
    }
}
