use super::{LaunchAdapterResolver, ToolLaunchService};
use crate::adapters::{AuthorizedUpdate, LaunchContext, LifecycleContext, ToolAdapter};
use crate::domain::{AppError, ErrorCode, Tool, ToolCapabilities, ToolId, UninstallOptions};
use crate::platform::{AllowedProgram, CommandSpec, TerminalLaunchSpec, TerminalLauncher};
use futures::future::BoxFuture;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

type SeenDirectories = Arc<Mutex<Vec<PathBuf>>>;
type SeenLaunchSpecs = Arc<Mutex<Vec<TerminalLaunchSpec>>>;
type TestService = (ToolLaunchService, SeenDirectories, SeenLaunchSpecs);

struct RecordingLauncher {
    specs: SeenLaunchSpecs,
    outcome: Result<(), AppError>,
}

impl TerminalLauncher for RecordingLauncher {
    fn launch(&self, spec: TerminalLaunchSpec) -> BoxFuture<'static, Result<(), AppError>> {
        self.specs.lock().expect("spec lock").push(spec);
        let outcome = self.outcome.clone();
        Box::pin(async move { outcome })
    }
}

struct StubAdapter {
    id: ToolId,
    can_launch: bool,
    seen_directories: SeenDirectories,
    panic_on_launch: bool,
}

impl ToolAdapter for StubAdapter {
    fn id(&self) -> ToolId {
        self.id
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            can_launch: self.can_launch,
            ..ToolCapabilities::default()
        }
    }

    fn detect(&self) -> BoxFuture<'_, Result<Tool, AppError>> {
        Box::pin(async { panic!("detect is not part of launch") })
    }

    fn install(&self, _ctx: LifecycleContext) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async { panic!("install is not part of launch") })
    }

    fn validate_update(
        &self,
        _preview_fingerprint: String,
    ) -> BoxFuture<'_, Result<AuthorizedUpdate, AppError>> {
        Box::pin(async { panic!("update is not part of launch") })
    }

    fn update(
        &self,
        _ctx: LifecycleContext,
        _authorized: AuthorizedUpdate,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async { panic!("update is not part of launch") })
    }

    fn uninstall(
        &self,
        _ctx: LifecycleContext,
        _options: UninstallOptions,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        Box::pin(async { panic!("uninstall is not part of launch") })
    }

    fn launch(
        &self,
        ctx: LaunchContext,
        working_directory: PathBuf,
    ) -> BoxFuture<'_, Result<(), AppError>> {
        if self.panic_on_launch {
            panic!("adapter launch panic with hidden details");
        }
        self.seen_directories
            .lock()
            .expect("directory lock")
            .push(working_directory.clone());
        let spec = TerminalLaunchSpec::native(
            working_directory,
            CommandSpec::new(AllowedProgram::Codex, Vec::new())
                .with_program_path(std::env::temp_dir().join("codex")),
        );
        Box::pin(async move { ctx.launcher.launch(spec).await })
    }
}

fn service(
    can_launch: bool,
    panic_on_launch: bool,
    launcher_outcome: Result<(), AppError>,
) -> TestService {
    let directories = Arc::new(Mutex::new(Vec::new()));
    let specs = Arc::new(Mutex::new(Vec::new()));
    let adapter_directories = directories.clone();
    let resolver: LaunchAdapterResolver = Arc::new(move |id| {
        (id == ToolId::Codex).then(|| {
            Box::new(StubAdapter {
                id,
                can_launch,
                seen_directories: adapter_directories.clone(),
                panic_on_launch,
            }) as Box<dyn ToolAdapter>
        })
    });
    let launcher = RecordingLauncher {
        specs: specs.clone(),
        outcome: launcher_outcome,
    };
    (
        ToolLaunchService::new(Arc::new(launcher), resolver),
        directories,
        specs,
    )
}

#[tokio::test]
async fn a_valid_directory_is_canonicalized_then_delegated_once() {
    let project = tempfile::tempdir().expect("project directory");
    let (service, directories, specs) = service(true, false, Ok(()));
    service
        .launch(ToolId::Codex, project.path().to_path_buf(), None)
        .await
        .expect("launch succeeds");

    let canonical = std::fs::canonicalize(project.path()).expect("canonical path");
    assert_eq!(
        *directories.lock().expect("directory lock"),
        vec![canonical.clone()]
    );
    let seen = specs.lock().expect("spec lock");
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].working_directory, canonical);
}

#[tokio::test]
async fn capability_refusal_happens_before_touching_the_directory_or_launcher() {
    let (service, directories, specs) = service(false, false, Ok(()));
    let error = service
        .launch(ToolId::Codex, PathBuf::from("does-not-exist"), None)
        .await
        .expect_err("unsupported launch fails first");
    assert_eq!(error.code, ErrorCode::LaunchFailed);
    assert_eq!(error.message_key, "error.tool.actionUnsupported");
    assert!(directories.lock().expect("directory lock").is_empty());
    assert!(specs.lock().expect("spec lock").is_empty());
}

#[tokio::test]
async fn missing_and_non_directory_paths_have_actionable_errors() {
    let (service, _, specs) = service(true, false, Ok(()));
    let missing = service
        .launch(ToolId::Codex, PathBuf::from("does-not-exist"), None)
        .await
        .expect_err("missing folder fails");
    assert_eq!(missing.code, ErrorCode::LaunchFailed);
    assert_eq!(missing.message_key, "error.tool.projectFolderUnavailable");
    assert_eq!(
        missing.remediation.as_deref(),
        Some("error.remediation.chooseAnotherFolder")
    );

    let file = tempfile::NamedTempFile::new().expect("project file");
    let not_directory = service
        .launch(ToolId::Codex, file.path().to_path_buf(), None)
        .await
        .expect_err("file is not a project folder");
    assert_eq!(
        not_directory.message_key,
        "error.tool.projectFolderUnavailable"
    );
    assert!(specs.lock().expect("spec lock").is_empty());
}

#[tokio::test]
async fn terminal_failures_are_preserved_for_the_command_layer() {
    let project = tempfile::tempdir().expect("project directory");
    let expected = AppError::new(ErrorCode::LaunchFailed, "error.tool.launchFailed")
        .with_remediation("error.remediation.openToolManually");
    let (service, _, specs) = service(true, false, Err(expected.clone()));
    let actual = service
        .launch(ToolId::Codex, project.path().to_path_buf(), None)
        .await
        .expect_err("launcher failure surfaces");
    assert_eq!(actual, expected);
    assert_eq!(specs.lock().expect("spec lock").len(), 1);
}

#[tokio::test]
async fn adapter_panics_become_structured_launch_errors() {
    let project = tempfile::tempdir().expect("project directory");
    let (service, _, specs) = service(true, true, Ok(()));
    let error = service
        .launch(ToolId::Codex, project.path().to_path_buf(), None)
        .await
        .expect_err("panic is contained");
    assert_eq!(error.code, ErrorCode::LaunchFailed);
    assert_eq!(error.message_key, "error.tool.actionPanicked");
    assert_eq!(
        error.technical_message.as_deref(),
        Some("tool launch adapter panicked")
    );
    assert!(specs.lock().expect("spec lock").is_empty());
}

#[tokio::test]
async fn unknown_tools_are_rejected_before_file_system_work() {
    let (service, _, _) = service(true, false, Ok(()));
    let error = service
        .launch(ToolId::ClaudeCode, PathBuf::from("does-not-exist"), None)
        .await
        .expect_err("unregistered adapter fails");
    assert_eq!(error.code, ErrorCode::ToolNotFound);
}
