use futures::future::BoxFuture;

use crate::application::tool_version_history::{
    history_read_failed, history_write_failed, ToolVersionEventRepository,
};
use crate::compat::ccswitch::tool_version_storage::{StoredToolVersionEvent, ToolVersionStore};
use crate::domain::{AppError, OperationKind, ToolId, ToolInstallSource, ToolVersionEvent};
use crate::platform::redact::{redact_secrets, truncate_tail};

#[derive(Clone)]
pub struct SqliteToolVersionEventRepository {
    store: ToolVersionStore,
}

impl SqliteToolVersionEventRepository {
    pub fn new(store: ToolVersionStore) -> Self {
        Self { store }
    }
}

impl ToolVersionEventRepository for SqliteToolVersionEventRepository {
    fn append(&self, event: ToolVersionEvent) -> BoxFuture<'static, Result<(), AppError>> {
        let store = self.store.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || append_on_store(&store, &event))
                .await
                .map_err(|error| write_error(error.to_string()))?
        })
    }

    fn list_newest(
        &self,
        tool: ToolId,
        limit: usize,
    ) -> BoxFuture<'static, Result<Vec<ToolVersionEvent>, AppError>> {
        let store = self.store.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || list_on_store(&store, tool, limit))
                .await
                .map_err(|error| read_error(error.to_string()))?
        })
    }
}

fn append_on_store(store: &ToolVersionStore, event: &ToolVersionEvent) -> Result<(), AppError> {
    let operation_kind = operation_kind_id(event.operation_kind)
        .ok_or_else(|| write_error("unsupported operation kind"))?;
    store
        .append_version_event(&StoredToolVersionEvent {
            tool_id: event.tool.as_str().to_string(),
            from_version: event.from_version.clone(),
            to_version: event.to_version.clone(),
            source: event.source.as_str_id().to_string(),
            operation_kind: operation_kind.to_string(),
            occurred_at: event.occurred_at,
        })
        .map_err(write_error)
}

fn list_on_store(
    store: &ToolVersionStore,
    tool: ToolId,
    limit: usize,
) -> Result<Vec<ToolVersionEvent>, AppError> {
    let bounded_limit =
        i64::try_from(limit.min(crate::domain::tool_version::MAX_TOOL_VERSION_HISTORY))
            .map_err(|error| read_error(error.to_string()))?;
    let rows = store
        .list_version_events(tool.as_str(), bounded_limit)
        .map_err(read_error)?;

    let mut events = Vec::new();
    for row in rows {
        let stored_tool = ToolId::from_str_id(&row.tool_id)
            .ok_or_else(|| read_error("unknown stored tool id"))?;
        let stored_source = ToolInstallSource::from_str_id(&row.source)
            .ok_or_else(|| read_error("unknown stored install source"))?;
        let stored_kind = operation_kind_from_id(&row.operation_kind)
            .ok_or_else(|| read_error("unknown stored operation kind"))?;
        let event = ToolVersionEvent::verified(
            stored_tool,
            row.from_version,
            row.to_version,
            stored_source,
            stored_kind,
            row.occurred_at,
        )
        .map_err(|error| {
            read_error(
                error
                    .technical_message
                    .unwrap_or_else(|| "invalid stored version event".to_string()),
            )
        })?;
        events.push(event);
    }
    Ok(events)
}

fn operation_kind_id(kind: OperationKind) -> Option<&'static str> {
    match kind {
        OperationKind::Install => Some("install"),
        OperationKind::Update => Some("update"),
        OperationKind::ChangeVersion => Some("changeVersion"),
        _ => None,
    }
}

fn operation_kind_from_id(raw: &str) -> Option<OperationKind> {
    match raw {
        "install" => Some(OperationKind::Install),
        "update" => Some(OperationKind::Update),
        "changeVersion" => Some(OperationKind::ChangeVersion),
        _ => None,
    }
}

fn safe_detail(detail: impl Into<String>) -> String {
    truncate_tail(&redact_secrets(&detail.into()), 8, 512)
}

fn write_error(detail: impl Into<String>) -> AppError {
    history_write_failed(safe_detail(detail))
}

fn read_error(detail: impl Into<String>) -> AppError {
    history_read_failed(safe_detail(detail))
}

#[cfg(test)]
mod tests {
    use super::SqliteToolVersionEventRepository;
    use crate::application::tool_version_history::ToolVersionEventRepository;
    use crate::compat::ccswitch::tool_version_storage::{StoredToolVersionEvent, ToolVersionStore};
    use crate::domain::{OperationKind, ToolId, ToolInstallSource, ToolVersionEvent};

    fn event(from: &str, to: &str, occurred_at: i64) -> ToolVersionEvent {
        ToolVersionEvent::verified(
            ToolId::Codex,
            Some(from.to_string()),
            to.to_string(),
            ToolInstallSource::Npm,
            OperationKind::Update,
            occurred_at,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn sqlite_repository_orders_same_millisecond_events_by_insert_id() {
        let store = ToolVersionStore::memory().unwrap();
        let repository = SqliteToolVersionEventRepository::new(store);
        repository
            .append(event("1.0.0", "1.1.0", 10))
            .await
            .unwrap();
        repository
            .append(event("1.1.0", "1.2.0", 10))
            .await
            .unwrap();
        repository
            .append(
                ToolVersionEvent::verified(
                    ToolId::ClaudeCode,
                    None,
                    "2.0.0".to_string(),
                    ToolInstallSource::Npm,
                    OperationKind::Install,
                    20,
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let events = repository.list_newest(ToolId::Codex, 100).await.unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].to_version, "1.2.0");
        assert_eq!(events[1].to_version, "1.1.0");
    }

    #[tokio::test]
    async fn sqlite_repository_fails_closed_on_an_unsafe_stored_version() {
        let store = ToolVersionStore::memory().unwrap();
        store
            .append_version_event(&StoredToolVersionEvent {
                tool_id: "codex".to_string(),
                from_version: None,
                to_version: "1.0.0\nunsafe".to_string(),
                source: "npm".to_string(),
                operation_kind: "install".to_string(),
                occurred_at: 1,
            })
            .unwrap();
        let repository = SqliteToolVersionEventRepository::new(store);
        let error = repository
            .list_newest(ToolId::Codex, 100)
            .await
            .unwrap_err();
        assert_eq!(error.message_key, "error.tool.versionHistoryReadFailed");
    }
}
