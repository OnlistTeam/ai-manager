//! Narrow database facade for product-owned tool version history.
//!
//! Product repositories depend on this facade instead of reaching the inherited
//! `Database` type directly. Values are deliberately returned in their stored
//! wire shape so the product repository remains responsible for validation.

use std::sync::Arc;

use rusqlite::params;

use crate::database::Database;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredToolVersionEvent {
    pub tool_id: String,
    pub from_version: Option<String>,
    pub to_version: String,
    pub source: String,
    pub operation_kind: String,
    pub occurred_at: i64,
}

#[derive(Clone)]
pub struct ToolVersionStore {
    database: Arc<Database>,
}

impl ToolVersionStore {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    pub fn append_version_event(&self, event: &StoredToolVersionEvent) -> Result<(), String> {
        let connection = self
            .database
            .conn
            .lock()
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "INSERT INTO tool_version_events
                 (tool_id, from_version, to_version, source, operation_kind, occurred_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    event.tool_id.as_str(),
                    event.from_version.as_deref(),
                    event.to_version.as_str(),
                    event.source.as_str(),
                    event.operation_kind.as_str(),
                    event.occurred_at,
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn list_version_events(
        &self,
        tool_id: &str,
        limit: i64,
    ) -> Result<Vec<StoredToolVersionEvent>, String> {
        let connection = self
            .database
            .conn
            .lock()
            .map_err(|error| error.to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT tool_id, from_version, to_version, source, operation_kind, occurred_at
                 FROM tool_version_events
                 WHERE tool_id = ?1
                 ORDER BY occurred_at DESC, id DESC
                 LIMIT ?2",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![tool_id, limit], |row| {
                Ok(StoredToolVersionEvent {
                    tool_id: row.get(0)?,
                    from_version: row.get(1)?,
                    to_version: row.get(2)?,
                    source: row.get(3)?,
                    operation_kind: row.get(4)?,
                    occurred_at: row.get(5)?,
                })
            })
            .map_err(|error| error.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    #[cfg(test)]
    pub fn memory() -> Result<Self, String> {
        Database::memory()
            .map(|database| Self::new(Arc::new(database)))
            .map_err(|error| error.to_string())
    }
}
