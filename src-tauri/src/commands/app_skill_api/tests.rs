use std::sync::Arc;

use super::{start_skill_update, start_skill_zip_install};
use crate::application::skill_update::{SkillUpdateRequest, SkillUpdateService, SkillUpdater};
use crate::application::skill_zip_installation::{
    SkillZipInstallRequest, SkillZipInstallationService, SkillZipInstaller,
};
use crate::domain::{
    Extension, ExtensionKind, ExtensionManagement, ExtensionScope, OperationStatus, ToolId,
};
use crate::infrastructure::{OperationEvents, OperationManager};

struct SilentEvents;

impl OperationEvents for SilentEvents {
    fn emit(&self, _operation: &crate::domain::Operation) {}
}

#[tokio::test]
async fn a_successful_start_schedules_the_skill_update_to_completion() {
    let operations = Arc::new(OperationManager::new(Box::new(SilentEvents)));
    let updater: SkillUpdater = Arc::new(|tool, id| {
        Box::pin(async move {
            Ok(Extension {
                kind: ExtensionKind::Skill,
                id,
                scope: ExtensionScope::tool(tool),
                name: "Code review".to_string(),
                description: None,
                management: ExtensionManagement::Managed,
                enabled: true,
                can_disable: true,
            })
        })
    });
    let service = SkillUpdateService::new(operations.clone(), updater);
    let request = SkillUpdateRequest {
        tool: ToolId::ClaudeCode,
        id: "anthropics/skills:skills/code-review".to_string(),
        name: "Code review".to_string(),
    };
    let id = start_skill_update(service, request).expect("start update");

    for _ in 0..200 {
        if operations
            .get(&id)
            .is_some_and(|operation| operation.status != OperationStatus::Running)
        {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(
        operations.get(&id).expect("operation").status,
        OperationStatus::Success
    );
}

#[tokio::test]
async fn a_successful_zip_start_schedules_the_background_install() {
    let archive = tempfile::Builder::new()
        .suffix(".zip")
        .tempfile()
        .expect("temporary ZIP");
    let request =
        SkillZipInstallRequest::selected(ToolId::ClaudeCode, archive.path().to_path_buf())
            .expect("native ZIP selection");
    let operations = Arc::new(OperationManager::new(Box::new(SilentEvents)));
    let installer: SkillZipInstaller = Arc::new(|tool, _| {
        Box::pin(async move {
            Ok(vec![Extension {
                kind: ExtensionKind::Skill,
                id: "local:review".to_string(),
                scope: ExtensionScope::tool(tool),
                name: "Review".to_string(),
                description: None,
                management: ExtensionManagement::Managed,
                enabled: true,
                can_disable: true,
            }])
        })
    });
    let service = SkillZipInstallationService::new(operations.clone(), installer);
    let id = start_skill_zip_install(service, request).expect("start ZIP install");

    for _ in 0..200 {
        if operations
            .get(&id)
            .is_some_and(|operation| operation.status != OperationStatus::Running)
        {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(
        operations.get(&id).expect("operation").status,
        OperationStatus::Success
    );
}
