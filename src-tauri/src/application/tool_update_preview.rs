use std::collections::HashSet;
use std::sync::Arc;

use futures::future::{join_all, BoxFuture};

use crate::adapters::AdapterRegistry;
use crate::domain::{AppError, ErrorCode, ToolId, ToolUpdateBlockReason, ToolUpdatePreview};

pub type UpdatePreviewResolver =
    Arc<dyn Fn(ToolId) -> BoxFuture<'static, Result<ToolUpdatePreview, AppError>> + Send + Sync>;

pub struct ToolUpdatePreviewService {
    preview: UpdatePreviewResolver,
}

impl ToolUpdatePreviewService {
    pub fn system() -> Self {
        Self::new(Arc::new(|tool| {
            Box::pin(async move {
                crate::compat::ccswitch::update_preview::inspect_and_preview(tool).await
            })
        }))
    }

    pub fn new(preview: UpdatePreviewResolver) -> Self {
        Self { preview }
    }

    pub async fn preview(&self, tools: Vec<ToolId>) -> Result<Vec<ToolUpdatePreview>, AppError> {
        let mut seen = HashSet::new();
        let tools: Vec<_> = tools
            .into_iter()
            .filter(|tool| seen.insert(*tool))
            .collect();
        if tools.is_empty() || tools.len() > ToolId::ALL.len() {
            return Err(update_preview_unavailable(
                "update preview requires one to ten unique tools",
            ));
        }
        let futures = tools.into_iter().map(|tool| {
            let preview = self.preview.clone();
            async move {
                let supported = AdapterRegistry::get(tool)
                    .is_some_and(|adapter| adapter.capabilities().can_update);
                if !supported {
                    return ToolUpdatePreview::Blocked {
                        tool,
                        reason: ToolUpdateBlockReason::UnsupportedInstallation,
                    };
                }
                match preview(tool).await {
                    Ok(item) => item,
                    Err(_) => ToolUpdatePreview::Blocked {
                        tool,
                        reason: ToolUpdateBlockReason::InspectionFailed,
                    },
                }
            }
        });
        Ok(join_all(futures).await)
    }
}

fn update_preview_unavailable(detail: impl Into<String>) -> AppError {
    AppError::new(
        ErrorCode::UpdateFailed,
        "error.tool.updatePreviewUnavailable",
    )
    .with_technical(detail)
    .with_remediation("error.remediation.recheckUpdate")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::ToolUpdatePreviewService;
    use crate::domain::{
        AppError, ToolId, ToolInstallSource, ToolUpdateBlockReason, ToolUpdatePreview,
        ToolUpdateReadyPreview,
    };

    fn ready_fixture(tool: ToolId) -> ToolUpdatePreview {
        ToolUpdatePreview::Ready {
            preview: ToolUpdateReadyPreview {
                tool,
                preview_fingerprint: "a".repeat(64),
                target_version: "2.0.0".to_string(),
                source: ToolInstallSource::Npm,
                installations: Vec::new(),
                attempts: Vec::new(),
                multiple_installations: false,
            },
        }
    }

    #[tokio::test]
    async fn batch_preview_preserves_input_order_and_deduplicates_tools() {
        let service = ToolUpdatePreviewService::new(Arc::new(|tool| {
            Box::pin(async move { Ok(ready_fixture(tool)) })
        }));
        let result = service
            .preview(vec![ToolId::Codex, ToolId::ClaudeCode, ToolId::Codex])
            .await
            .expect("preview");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].tool(), ToolId::Codex);
        assert_eq!(result[1].tool(), ToolId::ClaudeCode);
    }

    #[tokio::test]
    async fn one_failed_inspection_becomes_blocked_without_losing_other_tools() {
        let service = ToolUpdatePreviewService::new(Arc::new(|tool| {
            Box::pin(async move {
                if tool == ToolId::Codex {
                    Err(AppError::new(
                        crate::domain::ErrorCode::UpdateFailed,
                        "error.tool.updatePreviewUnavailable",
                    ))
                } else {
                    Ok(ready_fixture(tool))
                }
            })
        }));
        let result = service
            .preview(vec![ToolId::ClaudeCode, ToolId::Codex])
            .await
            .expect("partial preview");
        assert!(matches!(result[0], ToolUpdatePreview::Ready { .. }));
        assert!(matches!(
            result[1],
            ToolUpdatePreview::Blocked {
                reason: ToolUpdateBlockReason::InspectionFailed,
                ..
            }
        ));
    }
}
