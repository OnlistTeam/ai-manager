use std::sync::Arc;

use futures::future::BoxFuture;

use crate::domain::tool_version::MAX_TOOL_VERSION_HISTORY;
use crate::domain::{
    AppError, ErrorCode, OperationKind, ToolId, ToolInstallSource, ToolVersionEvent,
    ToolVersionHistory,
};

/// Storage port for the product-owned version event ledger. Implementations
/// own their blocking boundary; Application never reaches into SQLite.
pub trait ToolVersionEventRepository: Send + Sync {
    fn append(&self, event: ToolVersionEvent) -> BoxFuture<'static, Result<(), AppError>>;

    fn list_newest(
        &self,
        tool: ToolId,
        limit: usize,
    ) -> BoxFuture<'static, Result<Vec<ToolVersionEvent>, AppError>>;
}

#[derive(Clone)]
pub struct ToolVersionHistoryService {
    repository: Arc<dyn ToolVersionEventRepository>,
}

impl ToolVersionHistoryService {
    pub fn new(repository: Arc<dyn ToolVersionEventRepository>) -> Self {
        Self { repository }
    }

    pub async fn record_verified(
        &self,
        tool: ToolId,
        from_version: Option<String>,
        to_version: String,
        source: ToolInstallSource,
        operation_kind: OperationKind,
    ) -> Result<(), AppError> {
        let event = ToolVersionEvent::verified(
            tool,
            from_version,
            to_version,
            source,
            operation_kind,
            chrono::Utc::now().timestamp_millis(),
        )?;
        self.repository.append(event).await
    }

    pub async fn history(&self, tool: ToolId) -> Result<ToolVersionHistory, AppError> {
        let events = self
            .repository
            .list_newest(tool, MAX_TOOL_VERSION_HISTORY)
            .await?;
        if events.len() > MAX_TOOL_VERSION_HISTORY || events.iter().any(|event| event.tool != tool)
        {
            return Err(history_read_failed(
                "version event repository violated the bounded tool query",
            ));
        }
        Ok(ToolVersionHistory::from_newest_first(tool, events))
    }
}

pub fn history_read_failed(detail: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::Internal, "error.tool.versionHistoryReadFailed")
        .with_technical(detail)
        .with_remediation("error.remediation.retryOrViewDetails")
}

pub fn history_write_failed(detail: impl Into<String>) -> AppError {
    AppError::new(
        ErrorCode::UpdateFailed,
        "error.tool.versionHistoryWriteFailed",
    )
    .with_technical(detail)
    .with_remediation("error.remediation.retryOrViewDetails")
}

#[cfg(test)]
impl ToolVersionHistoryService {
    pub fn discarding() -> Self {
        struct DiscardingRepository;
        impl ToolVersionEventRepository for DiscardingRepository {
            fn append(&self, _event: ToolVersionEvent) -> BoxFuture<'static, Result<(), AppError>> {
                Box::pin(async { Ok(()) })
            }

            fn list_newest(
                &self,
                _tool: ToolId,
                _limit: usize,
            ) -> BoxFuture<'static, Result<Vec<ToolVersionEvent>, AppError>> {
                Box::pin(async { Ok(Vec::new()) })
            }
        }
        Self::new(Arc::new(DiscardingRepository))
    }
}

#[cfg(test)]
mod tests {
    use super::{ToolVersionEventRepository, ToolVersionHistoryService};
    use crate::domain::{AppError, OperationKind, ToolId, ToolInstallSource, ToolVersionEvent};
    use futures::future::BoxFuture;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct MemoryRepository {
        events: Mutex<Vec<ToolVersionEvent>>,
    }

    impl ToolVersionEventRepository for MemoryRepository {
        fn append(&self, event: ToolVersionEvent) -> BoxFuture<'static, Result<(), AppError>> {
            self.events.lock().expect("events lock").push(event);
            Box::pin(async { Ok(()) })
        }

        fn list_newest(
            &self,
            tool: ToolId,
            limit: usize,
        ) -> BoxFuture<'static, Result<Vec<ToolVersionEvent>, AppError>> {
            let events = self
                .events
                .lock()
                .expect("events lock")
                .iter()
                .rev()
                .filter(|event| event.tool == tool)
                .take(limit)
                .cloned()
                .collect();
            Box::pin(async move { Ok(events) })
        }
    }

    #[tokio::test]
    async fn record_and_query_derive_previous_version_from_product_events() {
        let repository = Arc::new(MemoryRepository::default());
        let service = ToolVersionHistoryService::new(repository);
        service
            .record_verified(
                ToolId::ClaudeCode,
                Some("1.0.0".to_string()),
                "2.0.0".to_string(),
                ToolInstallSource::Npm,
                OperationKind::Update,
            )
            .await
            .unwrap();

        let history = service.history(ToolId::ClaudeCode).await.unwrap();
        assert_eq!(history.previous_version.as_deref(), Some("1.0.0"));
        assert_eq!(history.events.len(), 1);
        assert_eq!(history.events[0].to_version, "2.0.0");
    }
}
