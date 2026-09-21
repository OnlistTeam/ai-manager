use std::path::{Path, PathBuf};

use crate::compat::ccswitch::install_probe::{probe, LifecycleProbe};
use crate::compat::ccswitch::lifecycle::uninstall_plan;
use crate::compat::ccswitch::tool_paths::{cache_paths, is_removable, settings_paths};
use crate::domain::{
    AppError, ToolId, ToolUninstallPreview, ToolUninstallTarget, ToolUninstallTargetKind,
};
use crate::platform::plan::UninstallPlan;

/// Produces the exact read-only facts shown by the uninstall modal. Execution
/// deliberately does not consume this DTO; it probes and plans again so a
/// stale or modified renderer response can never choose a deletion target.
pub struct ToolUninstallPreviewService;

impl ToolUninstallPreviewService {
    pub async fn preview(tool: ToolId) -> Result<ToolUninstallPreview, AppError> {
        let lifecycle_probe = probe(tool).await?;
        Self::preview_for_probe(tool, &lifecycle_probe)
    }

    fn preview_for_probe(
        tool: ToolId,
        lifecycle_probe: &LifecycleProbe,
    ) -> Result<ToolUninstallPreview, AppError> {
        let plan = uninstall_plan(tool, lifecycle_probe)?;
        Self::from_plan(tool, plan)
    }

    fn from_plan(tool: ToolId, plan: UninstallPlan) -> Result<ToolUninstallPreview, AppError> {
        let app = match plan {
            UninstallPlan::Command(plan) => {
                let mut targets = Vec::new();
                for attempt in plan.attempts {
                    for step in attempt.steps {
                        step.spec.validate()?;
                        targets.push(ToolUninstallTarget {
                            kind: ToolUninstallTargetKind::Command,
                            value: step.spec.redacted_display(),
                            can_remove_automatically: true,
                        });
                    }
                }
                targets
            }
            UninstallPlan::RemovePaths(paths) => paths.iter().map(path_target).collect(),
        };

        Ok(ToolUninstallPreview {
            tool,
            app,
            settings: settings_paths(tool).iter().map(path_target).collect(),
            cache: cache_paths(tool).iter().map(path_target).collect(),
        })
    }
}

fn path_target(path: &PathBuf) -> ToolUninstallTarget {
    let kind = match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => ToolUninstallTargetKind::SymbolicLink,
        Ok(metadata) if metadata.is_dir() => ToolUninstallTargetKind::Directory,
        Ok(_) => ToolUninstallTargetKind::File,
        Err(_) => ToolUninstallTargetKind::MissingPath,
    };
    ToolUninstallTarget {
        kind,
        value: path.display().to_string(),
        can_remove_automatically: is_removable(Path::new(path)),
    }
}

#[cfg(test)]
mod tests {
    use super::ToolUninstallPreviewService;
    use crate::domain::{ToolId, ToolUninstallTargetKind};
    use crate::platform::command::{AllowedProgram, CommandSpec};
    use crate::platform::plan::{LifecyclePlan, UninstallPlan};

    #[test]
    #[serial_test::serial]
    fn command_preview_exposes_the_anchored_action_and_fixed_data_paths() {
        // `CommandSpec::validate` demands an absolute anchor, and a bare POSIX path carries
        // no drive prefix, so it is relative on Windows.
        let npm = if cfg!(target_os = "windows") {
            std::path::PathBuf::from(r"C:\Users\test\AppData\Roaming\npm\npm.cmd")
        } else {
            std::path::PathBuf::from("/Users/test/.nvm/versions/node/v22/bin/npm")
        };
        let plan = UninstallPlan::Command(LifecyclePlan::single(
            CommandSpec::new(
                AllowedProgram::Npm,
                vec![
                    "uninstall".to_string(),
                    "-g".to_string(),
                    "@google/gemini-cli".to_string(),
                ],
            )
            .with_program_path(npm.clone()),
        ));

        let preview =
            ToolUninstallPreviewService::from_plan(ToolId::GeminiCli, plan).expect("preview");

        assert_eq!(preview.tool, ToolId::GeminiCli);
        assert_eq!(preview.app.len(), 1);
        assert_eq!(preview.app[0].kind, ToolUninstallTargetKind::Command);
        assert!(preview.app[0].value.starts_with(&npm.display().to_string()));
        assert!(preview.app[0]
            .value
            .ends_with("uninstall -g @google/gemini-cli"));
        assert!(preview.app[0].can_remove_automatically);
        assert_eq!(preview.settings.len(), 1);
        assert!(preview.cache.is_empty());
    }

    #[test]
    #[serial_test::serial]
    fn native_preview_marks_paths_outside_home_as_protected() {
        let preview = ToolUninstallPreviewService::from_plan(
            ToolId::ClaudeCode,
            UninstallPlan::RemovePaths(vec!["/Applications/Claude.app".into()]),
        )
        .expect("preview");

        assert_eq!(preview.app.len(), 1);
        assert!(!preview.app[0].can_remove_automatically);
    }
}
