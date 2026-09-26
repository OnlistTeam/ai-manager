//! Live routing: one switch over every routing target (ADR-0050).
//!
//! On takes over each target that has a current service, through the same
//! takeover path, backups and lock as the per-tool switch. A tool that cannot
//! be taken over is reported and left direct; the others stay routed. Off is
//! the existing stop-all-and-restore path. "Live routing is on" is derived
//! from the overview, never stored.

use crate::domain::{
    AppError, ErrorCode, RoutingLiveFailure, RoutingLiveModeOutcome, RoutingOverview, ToolId,
};

use super::{app_for_tool, conflict, RoutingStore};

/// Targets live routing would take over: every tool with a current service.
fn live_candidates(overview: &RoutingOverview) -> Vec<ToolId> {
    overview
        .targets
        .iter()
        .filter(|target| target.current_provider.is_some())
        .map(|target| target.tool)
        .collect()
}

fn tool_failed(tool: ToolId, error: AppError) -> RoutingLiveFailure {
    let mut wrapped = AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.routing.liveTakeoverFailed",
    )
    .with_remediation("error.remediation.retryOrViewDetails");
    wrapped.technical_message = error.technical_message;
    RoutingLiveFailure {
        tool,
        error: wrapped,
    }
}

impl RoutingStore {
    pub async fn set_live_mode(&self, enabled: bool) -> Result<RoutingLiveModeOutcome, AppError> {
        let _guard = self.mutation_lock.lock().await;
        if !enabled {
            self.stop_all_unlocked().await?;
            return Ok(RoutingLiveModeOutcome {
                overview: self.overview().await?,
                failures: Vec::new(),
            });
        }

        let candidates = live_candidates(&self.overview().await?);
        if candidates.is_empty() {
            return Err(conflict(
                "error.routing.noLiveTargets",
                "no routing target has a current service",
            ));
        }

        let mut failures = Vec::new();
        for tool in candidates {
            let app = app_for_tool(tool)?;
            if let Err(error) = self.set_takeover_unlocked(app, true).await {
                failures.push(tool_failed(tool, error));
            }
        }

        let mut overview = self.overview().await?;
        if !overview
            .targets
            .iter()
            .any(|target| target.takeover_enabled)
        {
            // Nothing was taken over: leave the tools as they were instead of
            // a started route that routes nothing.
            self.stop_all_unlocked().await?;
            overview = self.overview().await?;
        }
        Ok(RoutingLiveModeOutcome { overview, failures })
    }
}

#[cfg(test)]
#[path = "tests_live.rs"]
mod tests;
