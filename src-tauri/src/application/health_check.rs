//! The Quick Check use case: normalize the set of installed tools, then read one read-only snapshot.

use std::collections::HashSet;

use crate::compat::ccswitch::health::HealthStore;
use crate::domain::{AppError, HealthSnapshot, ToolId};

fn deduplicate(tools: &[ToolId]) -> Vec<ToolId> {
    let wanted: HashSet<ToolId> = tools.iter().copied().collect();
    ToolId::ALL
        .into_iter()
        .filter(|tool| {
            wanted.contains(tool)
                && crate::compat::ccswitch::tools::capabilities_for(*tool).can_manage_provider
        })
        .collect()
}

pub struct HealthCheck;

impl HealthCheck {
    pub fn snapshot(
        app_handle: &tauri::AppHandle,
        installed: &[ToolId],
    ) -> Result<HealthSnapshot, AppError> {
        HealthStore::open(app_handle)?.snapshot(&deduplicate(installed))
    }
}

#[cfg(test)]
mod tests {
    use super::deduplicate;
    use crate::domain::ToolId;

    #[test]
    fn installed_tools_are_deduplicated_in_registry_order() {
        assert_eq!(
            deduplicate(&[
                ToolId::GeminiCli,
                ToolId::ClaudeCode,
                ToolId::GeminiCli,
                ToolId::Codex,
            ]),
            vec![ToolId::ClaudeCode, ToolId::Codex, ToolId::GeminiCli]
        );
    }
}
