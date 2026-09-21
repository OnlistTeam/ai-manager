use futures::future::FutureExt;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::Arc;

use crate::adapters::{AdapterRegistry, LaunchContext, ToolAdapter};
use crate::domain::{AppError, ErrorCode, TerminalAppId, ToolId};
use crate::platform::{SystemTerminalLauncher, TerminalLauncher};

pub type LaunchAdapterResolver = Arc<dyn Fn(ToolId) -> Option<Box<dyn ToolAdapter>> + Send + Sync>;

/// The quick use case after a directory has been chosen. It creates no operation: success is an
/// immediate hand-off to the terminal, with no download/install progress to track.
pub struct ToolLaunchService {
    launcher: Arc<dyn TerminalLauncher>,
    adapters: LaunchAdapterResolver,
}

impl ToolLaunchService {
    pub fn new(launcher: Arc<dyn TerminalLauncher>, adapters: LaunchAdapterResolver) -> Self {
        Self { launcher, adapters }
    }

    pub fn system() -> Self {
        Self::new(
            Arc::new(SystemTerminalLauncher),
            Arc::new(AdapterRegistry::get),
        )
    }

    pub async fn launch(
        &self,
        id: ToolId,
        selected_directory: PathBuf,
        terminal_app: Option<TerminalAppId>,
    ) -> Result<(), AppError> {
        let adapter = self.resolve(id)?;
        if !adapter.capabilities().can_launch {
            return Err(
                AppError::new(ErrorCode::LaunchFailed, "error.tool.actionUnsupported")
                    .with_remediation("error.remediation.openToolManually"),
            );
        }

        let working_directory = canonical_project_directory(selected_directory).await?;
        let context = LaunchContext {
            launcher: self.launcher.clone(),
            terminal_app,
        };
        AssertUnwindSafe(async move { adapter.launch(context, working_directory).await })
            .catch_unwind()
            .await
            .unwrap_or_else(|_| {
                Err(
                    AppError::new(ErrorCode::LaunchFailed, "error.tool.actionPanicked")
                        .with_technical("tool launch adapter panicked")
                        .with_remediation("error.remediation.openToolManually"),
                )
            })
    }

    fn resolve(&self, id: ToolId) -> Result<Box<dyn ToolAdapter>, AppError> {
        (self.adapters)(id).ok_or_else(|| {
            AppError::new(ErrorCode::ToolNotFound, "error.tool.notFound")
                .with_technical(id.as_str().to_string())
        })
    }
}

async fn canonical_project_directory(selected: PathBuf) -> Result<PathBuf, AppError> {
    tokio::task::spawn_blocking(move || {
        if !selected.is_dir() {
            return Err(project_directory_unavailable(
                "selected path is not a directory",
            ));
        }
        std::fs::canonicalize(&selected).map_err(|error| {
            project_directory_unavailable(format!(
                "project directory canonicalization failed: {}",
                error.kind()
            ))
        })
    })
    .await
    .map_err(|error| {
        project_directory_unavailable(format!("project directory task failed: {error}"))
    })?
}

fn project_directory_unavailable(technical: impl Into<String>) -> AppError {
    AppError::new(
        ErrorCode::LaunchFailed,
        "error.tool.projectFolderUnavailable",
    )
    .with_technical(technical)
    .with_remediation("error.remediation.chooseAnotherFolder")
}

#[cfg(test)]
#[path = "tool_launch/tests.rs"]
mod tests;
