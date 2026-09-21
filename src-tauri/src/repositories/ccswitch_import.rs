//! Read-only intake and allow-listed merge for a CC Switch database (ADR-0002).
//!
//! The file on disk is opened read-only and copied into memory. Callers may migrate that
//! in-memory copy, but no product path can obtain a writable handle to the source file.

use std::fs;
use std::path::Path;

use rusqlite::backup::{Backup, StepResult};
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OpenFlags, Transaction};

use crate::domain::{AppError, ErrorCode, ImportSummary};
use crate::platform::redact::{redact_secrets, truncate_tail};

const SUPPORTED_APP_TYPES: &[&str] = &["claude", "codex", "opencode", "gemini"];
const PROVIDER_COLUMNS: &str = "id, app_type, name, settings_config, website_url, category, \
    created_at, sort_index, notes, icon, icon_color, meta, is_current, in_failover_queue";
const MCP_COLUMNS: &str = "id, name, server_config, description, homepage, docs, tags, \
    enabled_claude, enabled_codex, enabled_gemini, enabled_grokbuild, enabled_opencode, \
    enabled_hermes";
const SKILL_COLUMNS: &str = "id, name, description, directory, repo_owner, repo_name, \
    repo_branch, readme_url, enabled_claude, enabled_codex, enabled_gemini, \
    enabled_grokbuild, enabled_opencode, enabled_hermes, installed_at, content_hash, updated_at";

fn supported_app_types_clause() -> String {
    SUPPORTED_APP_TYPES
        .iter()
        .map(|app_type| format!("'{app_type}'"))
        .collect::<Vec<_>>()
        .join(",")
}

fn source_error(message_key: &'static str, detail: impl Into<String>) -> AppError {
    let detail = detail.into();
    AppError::new(ErrorCode::ConfigParseFailed, message_key)
        .with_technical(truncate_tail(&redact_secrets(&detail), 20, 2000))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn merge_error(detail: impl Into<String>) -> AppError {
    let detail = detail.into();
    AppError::new(ErrorCode::UpstreamError, "error.import.mergeFailed")
        .with_technical(truncate_tail(&redact_secrets(&detail), 20, 2000))
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn table_exists(connection: &Connection, table: &str) -> rusqlite::Result<bool> {
    connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [table],
        |row| row.get(0),
    )
}

fn provider_signature_is_valid(connection: &Connection) -> rusqlite::Result<bool> {
    if !table_exists(connection, "providers")? {
        return Ok(false);
    }

    let mut statement = connection.prepare("PRAGMA table_info(providers)")?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(["id", "app_type", "name", "settings_config"]
        .iter()
        .all(|required| names.iter().any(|actual| actual == required)))
}

fn open_source_read_only(path: &Path) -> Result<Connection, AppError> {
    let source = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| source_error("error.import.readFailed", error.to_string()))?;
    source
        .execute_batch("PRAGMA query_only = ON;")
        .map_err(|error| source_error("error.import.readFailed", error.to_string()))?;
    Ok(source)
}

/// Open a CC Switch database without write capability and return a consistent memory snapshot.
/// A missing file is a normal `None`, because first-run discovery is optional.
pub fn snapshot_source_database(
    path: &Path,
    max_supported_version: i32,
) -> Result<Option<Connection>, AppError> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => {
            return Err(source_error(
                "error.import.readFailed",
                "CC Switch database path is not a file",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(source_error("error.import.readFailed", error.to_string()));
        }
    }

    let source = open_source_read_only(path)?;
    let integrity: String = source
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|error| source_error("error.import.invalidSource", error.to_string()))?;
    if integrity != "ok" {
        return Err(source_error(
            "error.import.invalidSource",
            "SQLite quick_check did not return ok",
        ));
    }

    let version: i32 = source
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| source_error("error.import.invalidSource", error.to_string()))?;
    if version > max_supported_version {
        return Err(source_error(
            "error.import.newerVersion",
            format!(
                "source schema version {version} exceeds supported version {max_supported_version}"
            ),
        ));
    }
    if !provider_signature_is_valid(&source)
        .map_err(|error| source_error("error.import.invalidSource", error.to_string()))?
    {
        return Err(source_error(
            "error.import.invalidSource",
            "required CC Switch provider columns are missing",
        ));
    }

    let mut snapshot = Connection::open_in_memory()
        .map_err(|error| source_error("error.import.readFailed", error.to_string()))?;
    {
        let backup = Backup::new(&source, &mut snapshot)
            .map_err(|error| source_error("error.import.readFailed", error.to_string()))?;
        let result = backup
            .step(-1)
            .map_err(|error| source_error("error.import.readFailed", error.to_string()))?;
        if result != StepResult::Done {
            return Err(source_error(
                "error.import.readFailed",
                format!("SQLite backup did not complete: {result:?}"),
            ));
        }
    }
    Ok(Some(snapshot))
}

fn checked_count(connection: &Connection, sql: &str) -> Result<u32, AppError> {
    let count: i64 = connection
        .query_row(sql, [], |row| row.get(0))
        .map_err(|error| source_error("error.import.invalidSource", error.to_string()))?;
    u32::try_from(count).map_err(|_| {
        source_error(
            "error.import.invalidSource",
            "source row count does not fit the product contract",
        )
    })
}

/// Count only categories that the product import contract will actually merge.
pub fn preview_import(source: &Connection) -> Result<ImportSummary, AppError> {
    for table in ["providers", "provider_endpoints", "mcp_servers", "skills"] {
        if !table_exists(source, table)
            .map_err(|error| source_error("error.import.invalidSource", error.to_string()))?
        {
            return Err(source_error(
                "error.import.invalidSource",
                format!("required table {table} is missing after migration"),
            ));
        }
    }

    Ok(ImportSummary {
        services: checked_count(
            source,
            &format!(
                "SELECT COUNT(*) FROM providers WHERE app_type IN ({})",
                supported_app_types_clause()
            ),
        )?,
        mcp_servers: checked_count(source, "SELECT COUNT(*) FROM mcp_servers")?,
        skills: checked_count(source, "SELECT COUNT(*) FROM skills")?,
    })
}

fn row_values(row: &rusqlite::Row<'_>, count: usize) -> rusqlite::Result<Vec<Value>> {
    (0..count).map(|index| row.get(index)).collect()
}

fn text_value(values: &[Value], index: usize) -> Result<&str, AppError> {
    match values.get(index) {
        Some(Value::Text(value)) => Ok(value),
        _ => Err(merge_error("source identity column is not text")),
    }
}

fn source_app_types(source: &Connection) -> Result<Vec<String>, AppError> {
    let sql = format!(
        "SELECT DISTINCT app_type FROM providers WHERE app_type IN ({})",
        supported_app_types_clause()
    );
    let mut statement = source
        .prepare(&sql)
        .map_err(|error| merge_error(error.to_string()))?;
    let app_types = statement
        .query_map([], |row| row.get(0))
        .map_err(|error| merge_error(error.to_string()))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| merge_error(error.to_string()))?;
    Ok(app_types)
}

fn copy_providers(source: &Connection, target: &Transaction<'_>) -> Result<u32, AppError> {
    for app_type in source_app_types(source)? {
        target
            .execute(
                "UPDATE providers SET is_current = 0 WHERE app_type = ?1",
                [&app_type],
            )
            .map_err(|error| merge_error(error.to_string()))?;
    }

    let select = format!(
        "SELECT {PROVIDER_COLUMNS} FROM providers
         WHERE app_type IN ({})",
        supported_app_types_clause()
    );
    let insert = format!(
        "INSERT OR REPLACE INTO providers ({PROVIDER_COLUMNS})
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)"
    );
    let mut source_statement = source
        .prepare(&select)
        .map_err(|error| merge_error(error.to_string()))?;
    let mut rows = source_statement
        .query([])
        .map_err(|error| merge_error(error.to_string()))?;
    let mut insert_statement = target
        .prepare(&insert)
        .map_err(|error| merge_error(error.to_string()))?;
    let mut count = 0_u32;
    while let Some(row) = rows
        .next()
        .map_err(|error| merge_error(error.to_string()))?
    {
        let values = row_values(row, 14).map_err(|error| merge_error(error.to_string()))?;
        let provider_id = text_value(&values, 0)?;
        let app_type = text_value(&values, 1)?;
        target
            .execute(
                "DELETE FROM provider_endpoints WHERE provider_id = ?1 AND app_type = ?2",
                params![provider_id, app_type],
            )
            .map_err(|error| merge_error(error.to_string()))?;
        insert_statement
            .execute(params_from_iter(values.iter()))
            .map_err(|error| merge_error(error.to_string()))?;
        count = count
            .checked_add(1)
            .ok_or_else(|| merge_error("provider count overflow"))?;
    }

    let endpoint_select = format!(
        "SELECT e.provider_id, e.app_type, e.url, e.added_at
             FROM provider_endpoints e
             INNER JOIN providers p ON p.id = e.provider_id AND p.app_type = e.app_type
             WHERE p.app_type IN ({})",
        supported_app_types_clause()
    );
    let mut source_statement = source
        .prepare(&endpoint_select)
        .map_err(|error| merge_error(error.to_string()))?;
    let mut rows = source_statement
        .query([])
        .map_err(|error| merge_error(error.to_string()))?;
    let mut insert_statement = target
        .prepare(
            "INSERT INTO provider_endpoints (provider_id, app_type, url, added_at)
             VALUES (?1,?2,?3,?4)",
        )
        .map_err(|error| merge_error(error.to_string()))?;
    while let Some(row) = rows
        .next()
        .map_err(|error| merge_error(error.to_string()))?
    {
        let values = row_values(row, 4).map_err(|error| merge_error(error.to_string()))?;
        insert_statement
            .execute(params_from_iter(values.iter()))
            .map_err(|error| merge_error(error.to_string()))?;
    }
    Ok(count)
}

fn copy_rows(
    source: &Connection,
    target: &Transaction<'_>,
    table: &str,
    columns: &str,
    column_count: usize,
    cleared_indexes: &[usize],
) -> Result<u32, AppError> {
    let select = format!("SELECT {columns} FROM {table}");
    let placeholders = (1..=column_count)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(",");
    let insert = format!("INSERT OR REPLACE INTO {table} ({columns}) VALUES ({placeholders})");
    let mut source_statement = source
        .prepare(&select)
        .map_err(|error| merge_error(error.to_string()))?;
    let mut rows = source_statement
        .query([])
        .map_err(|error| merge_error(error.to_string()))?;
    let mut insert_statement = target
        .prepare(&insert)
        .map_err(|error| merge_error(error.to_string()))?;
    let mut count = 0_u32;
    while let Some(row) = rows
        .next()
        .map_err(|error| merge_error(error.to_string()))?
    {
        let mut values =
            row_values(row, column_count).map_err(|error| merge_error(error.to_string()))?;
        for index in cleared_indexes {
            values[*index] = Value::Integer(0);
        }
        insert_statement
            .execute(params_from_iter(values.iter()))
            .map_err(|error| merge_error(error.to_string()))?;
        count = count
            .checked_add(1)
            .ok_or_else(|| merge_error(format!("{table} count overflow")))?;
    }
    Ok(count)
}

/// Merge the allow-listed source rows into a staged target in one transaction.
pub fn merge_import(
    source: &Connection,
    target: &mut Connection,
) -> Result<ImportSummary, AppError> {
    let transaction = target
        .transaction()
        .map_err(|error| merge_error(error.to_string()))?;
    let summary = ImportSummary {
        services: copy_providers(source, &transaction)?,
        mcp_servers: copy_rows(
            source,
            &transaction,
            "mcp_servers",
            MCP_COLUMNS,
            13,
            &[10, 12],
        )?,
        skills: copy_rows(source, &transaction, "skills", SKILL_COLUMNS, 17, &[11, 13])?,
    };
    transaction
        .commit()
        .map_err(|error| merge_error(error.to_string()))?;
    Ok(summary)
}

#[cfg(test)]
#[path = "tests_ccswitch_import.rs"]
mod tests;
