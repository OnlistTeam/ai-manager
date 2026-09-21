use serde::{Deserialize, Serialize};

use crate::domain::{
    validate_tool_version, AppError, DesktopAppId, ErrorCode, ExtensionKind, ProviderTestResult,
    Tool, ToolId,
};

/// The install-process narrative of spec §30. The UI recognizes only these keys; pushing technical
/// copy such as `Running npm...` to the frontend as a message_key is forbidden.
pub mod phase {
    pub const PREPARING: &str = "operation.phase.preparing";
    pub const DOWNLOADING: &str = "operation.phase.downloading";
    pub const INSTALLING: &str = "operation.phase.installing";
    pub const CONFIGURING: &str = "operation.phase.configuring";
    /// Uninstall only. Throughout 3a everything reported INSTALLING, so a user clicking "remove" saw "installing".
    pub const REMOVING: &str = "operation.phase.removing";
    pub const CHECKING: &str = "operation.phase.checking";
    pub const CANCELLING: &str = "operation.phase.cancelling";
    pub const CANCELLED: &str = "operation.phase.cancelled";
    pub const READY: &str = "operation.phase.ready";
    pub const REMOVED: &str = "operation.phase.removed";
}

/// The structured narrative inside install/update logs. Commands and subprocess output go through
/// `detail`; these keys only describe product state, so untranslatable technical copy never ends up
/// in `message_key`.
pub mod log_message {
    pub const STARTED: &str = "operation.log.started";
    pub const COMMAND_SUCCEEDED: &str = "operation.log.commandSucceeded";
    pub const COMMAND_FAILED: &str = "operation.log.commandFailed";
    pub const COMMAND_FAILURE_IGNORED: &str = "operation.log.commandFailureIgnored";
    pub const TRYING_FALLBACK: &str = "operation.log.tryingFallback";
    pub const USING_CONFIGURED_PROXY: &str = "operation.log.usingConfiguredProxy";
    pub const USING_COMMUNITY_MIRROR: &str = "operation.log.usingCommunityMirror";
    pub const NATIVE_SUPPLY_STARTED: &str = "operation.log.nativeSupply.start";
    pub const NATIVE_SUPPLY_DONE: &str = "operation.log.nativeSupply.done";
    pub const CANCELLATION_REQUESTED: &str = "operation.log.cancellationRequested";
    pub const CANCELLED: &str = "operation.log.cancelled";
    pub const SUCCEEDED: &str = "operation.log.succeeded";
    pub const FAILED: &str = "operation.log.failed";
    pub const TRUNCATED: &str = "operation.log.truncated";
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct OperationId(pub String);

impl OperationId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn parse(raw: &str) -> Result<Self, AppError> {
        let parsed = uuid::Uuid::parse_str(raw).map_err(|_| {
            AppError::new(ErrorCode::OperationConflict, "error.operation.invalidId")
                .with_technical("operation id must be a canonical UUID")
        })?;
        let canonical = parsed.hyphenated().to_string();
        if canonical != raw {
            return Err(
                AppError::new(ErrorCode::OperationConflict, "error.operation.invalidId")
                    .with_technical("operation id must be a canonical lowercase UUID"),
            );
        }
        Ok(Self(canonical))
    }
}

impl Default for OperationId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OperationKind {
    Install,
    Update,
    Uninstall,
    Repair,
    ChangeVersion,
    TestProviders,
    Scan,
}

impl OperationKind {
    /// Write operations need per-tool mutual exclusion (spec §41); both kinds of probe are read-only tasks and may run concurrently.
    pub fn is_mutation(&self) -> bool {
        !matches!(self, Self::TestProviders | Self::Scan)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OperationStatus {
    Queued,
    Running,
    Success,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OperationLogKind {
    Phase,
    Command,
    Stdout,
    Stderr,
    System,
}

/// A bounded, redacted task log that exists in memory only. `message_key` and `detail` are mutually
/// exclusive: the former carries the structured product narrative, the latter the command and
/// subprocess output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationLogEntry {
    pub timestamp: i64,
    pub kind: OperationLogKind,
    pub message_key: Option<String>,
    pub detail: Option<String>,
}

impl OperationLogEntry {
    pub fn message(timestamp: i64, kind: OperationLogKind, message_key: impl Into<String>) -> Self {
        Self {
            timestamp,
            kind,
            message_key: Some(message_key.into()),
            detail: None,
        }
    }

    pub fn detail(timestamp: i64, kind: OperationLogKind, detail: impl Into<String>) -> Self {
        Self {
            timestamp,
            kind,
            message_key: None,
            detail: Some(detail.into()),
        }
    }
}

/// Optional product resource affected by an operation. `tool` remains the
/// concurrency target; this metadata keeps user-facing task names honest when
/// the work modifies something inside that tool rather than the tool itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationExtension {
    pub kind: ExtensionKind,
    pub id: String,
    pub name: String,
}

/// Strongly typed, replayable output from a background operation. Provider
/// checks are intentionally the first output-bearing operation: task logs are
/// for diagnostics and must never be parsed as product data.
///
/// `ToolInventory` carries the detection a finished lifecycle action already
/// verified before it was allowed to succeed. Publishing it lets a card show
/// the real new version at once instead of holding a "done" panel over a stale
/// one until a full re-scan of every tool — which also asks every registry for
/// its latest version — comes back.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum OperationOutput {
    ProviderTests {
        results: Vec<ProviderTestResult>,
    },
    /// Boxed: a `Tool` dwarfs the other variant, and an operation carries at
    /// most one output.
    ToolInventory {
        tool: Box<Tool>,
    },
}

/// A recovery hint exists only on a failed Update whose post-failure detector
/// proved the tool is Broken. The tagged shape makes an actionable target
/// impossible to confuse with an advisory-only outcome.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ToolUpdateRecovery {
    Available { target_version: String },
    HistoryUnavailable,
    OwnershipChanged,
    OwnerUnsupported,
    InspectionFailed,
}

impl ToolUpdateRecovery {
    pub fn available(target_version: String) -> Result<Self, AppError> {
        Ok(Self::Available {
            target_version: validate_tool_version(&target_version)?,
        })
    }
}

impl OperationStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Success | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub id: OperationId,
    pub kind: OperationKind,
    pub tool: Option<ToolId>,
    /// Desktop configuration mutation target. Mutually exclusive with `tool`;
    /// it never borrows a related CLI identity for locking (ADR-0021).
    #[serde(default)]
    pub desktop_app: Option<DesktopAppId>,
    pub extension: Option<OperationExtension>,
    pub status: OperationStatus,
    pub progress: u8,
    pub message_key: Option<String>,
    #[serde(default)]
    pub can_cancel: bool,
    /// Native-only race guard. The wire contract exposes `canCancel` and the
    /// cancelling phase; it never asks the renderer to authorize completion.
    #[serde(skip)]
    pub(crate) cancel_requested: bool,
    #[serde(default)]
    pub logs: Vec<OperationLogEntry>,
    #[serde(default)]
    pub update_recovery: Option<ToolUpdateRecovery>,
    #[serde(default)]
    pub output: Option<OperationOutput>,
    pub error: Option<AppError>,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

impl Operation {
    pub fn new(id: OperationId, kind: OperationKind, tool: Option<ToolId>) -> Self {
        Self {
            id,
            kind,
            tool,
            desktop_app: None,
            extension: None,
            status: OperationStatus::Queued,
            progress: 0,
            message_key: None,
            can_cancel: false,
            cancel_requested: false,
            logs: Vec::new(),
            update_recovery: None,
            output: None,
            error: None,
            started_at: None,
            finished_at: None,
        }
    }

    pub fn with_extension(mut self, extension: OperationExtension) -> Self {
        self.extension = Some(extension);
        self
    }

    /// A pure state machine (spec §39): queued -> running -> success/failed/cancelled.
    /// An illegal transition modifies no field.
    pub fn transition(&mut self, next: OperationStatus) -> Result<(), AppError> {
        let allowed = matches!(
            (self.status, next),
            (OperationStatus::Queued, OperationStatus::Running)
                | (OperationStatus::Running, OperationStatus::Success)
                | (OperationStatus::Running, OperationStatus::Failed)
                | (OperationStatus::Running, OperationStatus::Cancelled)
        );
        if !allowed {
            return Err(AppError::new(
                ErrorCode::OperationConflict,
                "error.operation.invalidTransition",
            )
            .with_technical(format!("{:?} -> {:?}", self.status, next)));
        }
        self.status = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        log_message, phase, Operation, OperationExtension, OperationId, OperationKind,
        OperationLogEntry, OperationLogKind, OperationOutput, OperationStatus, ToolUpdateRecovery,
    };
    use crate::domain::{
        AppError, ErrorCode, ExtensionKind, ProviderReachability, ProviderTestResult, ToolId,
    };

    const ALL_STATUSES: [OperationStatus; 5] = [
        OperationStatus::Queued,
        OperationStatus::Running,
        OperationStatus::Success,
        OperationStatus::Failed,
        OperationStatus::Cancelled,
    ];

    fn legal(from: OperationStatus, to: OperationStatus) -> bool {
        matches!(
            (from, to),
            (OperationStatus::Queued, OperationStatus::Running)
                | (OperationStatus::Running, OperationStatus::Success)
                | (OperationStatus::Running, OperationStatus::Failed)
                | (OperationStatus::Running, OperationStatus::Cancelled)
        )
    }

    fn operation_in(status: OperationStatus) -> Operation {
        let mut operation = Operation::new(
            OperationId::new(),
            OperationKind::Install,
            Some(ToolId::ClaudeCode),
        );
        operation.status = status;
        operation
    }

    #[test]
    fn operation_id_is_unique_and_transparent() {
        let id = OperationId::new();
        assert_ne!(id, OperationId::new());
        assert_eq!(id.as_str().len(), 36);
        let json = serde_json::to_string(&id).expect("serialize operation id");
        assert_eq!(json, format!("\"{}\"", id.as_str()));
        assert_eq!(OperationId::parse(id.as_str()).unwrap(), id);
        assert!(OperationId::parse("op-1").is_err());
        assert!(OperationId::parse("AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA").is_err());
    }

    #[test]
    fn new_operation_starts_queued_at_zero_progress() {
        let operation = Operation::new(OperationId::new(), OperationKind::Scan, None);
        assert_eq!(operation.status, OperationStatus::Queued);
        assert_eq!(operation.progress, 0);
        assert_eq!(operation.tool, None);
        assert_eq!(operation.extension, None);
        assert_eq!(operation.message_key, None);
        assert!(!operation.can_cancel);
        assert!(operation.logs.is_empty());
        assert_eq!(operation.output, None);
        assert_eq!(operation.error, None);
        assert_eq!(operation.started_at, None);
        assert_eq!(operation.finished_at, None);
    }

    #[test]
    fn every_transition_pair_matches_the_state_machine() {
        for from in ALL_STATUSES {
            for to in ALL_STATUSES {
                let mut operation = operation_in(from);
                let result = operation.transition(to);
                if legal(from, to) {
                    assert!(result.is_ok(), "{from:?} -> {to:?} should be allowed");
                    assert_eq!(operation.status, to);
                } else {
                    let error = result.expect_err("illegal transition must fail");
                    assert_eq!(error.code, ErrorCode::OperationConflict);
                    assert_eq!(error.message_key, "error.operation.invalidTransition");
                    assert_eq!(
                        operation.status, from,
                        "{from:?} -> {to:?} must not mutate status"
                    );
                }
            }
        }
    }

    #[test]
    fn terminal_states_and_mutation_kinds_are_classified() {
        assert!(!OperationStatus::Queued.is_terminal());
        assert!(!OperationStatus::Running.is_terminal());
        assert!(OperationStatus::Success.is_terminal());
        assert!(OperationStatus::Failed.is_terminal());
        assert!(OperationStatus::Cancelled.is_terminal());

        assert!(OperationKind::Install.is_mutation());
        assert!(OperationKind::Update.is_mutation());
        assert!(OperationKind::Uninstall.is_mutation());
        assert!(OperationKind::Repair.is_mutation());
        assert!(OperationKind::ChangeVersion.is_mutation());
        assert!(!OperationKind::TestProviders.is_mutation());
        assert!(!OperationKind::Scan.is_mutation());
    }

    #[test]
    fn operation_round_trips_with_camel_case_wire_format() {
        let mut operation = Operation::new(
            OperationId("op-1".to_string()),
            OperationKind::Update,
            Some(ToolId::OpenCode),
        );
        operation
            .transition(OperationStatus::Running)
            .expect("queued -> running");
        operation.progress = 40;
        operation.message_key = Some("operation.update.downloading".to_string());
        operation.logs.push(OperationLogEntry::message(
            1_760_000_000_100,
            OperationLogKind::Phase,
            phase::DOWNLOADING,
        ));
        operation.logs.push(OperationLogEntry::detail(
            1_760_000_000_200,
            OperationLogKind::Command,
            "npm install -g @openai/codex@latest",
        ));
        operation.started_at = Some(1_760_000_000_000);
        operation
            .transition(OperationStatus::Failed)
            .expect("running -> failed");
        operation.error = Some(AppError::new(
            ErrorCode::UpdateFailed,
            "error.tool.updateFailed",
        ));
        operation.finished_at = Some(1_760_000_005_000);

        let json = serde_json::to_string(&operation).expect("serialize operation");
        assert_eq!(
            json,
            r#"{"id":"op-1","kind":"update","tool":"opencode","desktopApp":null,"extension":null,"status":"failed","progress":40,"messageKey":"operation.update.downloading","canCancel":false,"logs":[{"timestamp":1760000000100,"kind":"phase","messageKey":"operation.phase.downloading","detail":null},{"timestamp":1760000000200,"kind":"command","messageKey":null,"detail":"npm install -g @openai/codex@latest"}],"updateRecovery":null,"output":null,"error":{"code":"UPDATE_FAILED","messageKey":"error.tool.updateFailed","technicalMessage":null,"remediation":null,"contextId":null},"startedAt":1760000000000,"finishedAt":1760000005000}"#
        );
        let parsed: Operation = serde_json::from_str(&json).expect("deserialize operation");
        assert_eq!(parsed, operation);
    }

    /// The frontend parses these fields with Zod `.nullable()` rather than `.optional()` (spec §21);
    /// pinning the literal null keeps a stray `skip_serializing_if` on the Rust side from leaving the
    /// Rust tests green while frontend parsing fails.
    #[test]
    fn operation_serializes_all_optional_fields_as_literal_null() {
        let operation = Operation::new(
            OperationId("op-null".to_string()),
            OperationKind::Scan,
            None,
        );
        let json = serde_json::to_string(&operation).expect("serialize operation");
        assert!(json.contains(r#""tool":null"#));
        assert!(json.contains(r#""desktopApp":null"#));
        assert!(json.contains(r#""extension":null"#));
        assert!(json.contains(r#""messageKey":null"#));
        assert!(json.contains(r#""canCancel":false"#));
        assert!(json.contains(r#""logs":[]"#));
        assert!(json.contains(r#""updateRecovery":null"#));
        assert!(json.contains(r#""output":null"#));
        assert!(json.contains(r#""error":null"#));
        assert!(json.contains(r#""startedAt":null"#));
        assert!(json.contains(r#""finishedAt":null"#));
    }

    #[test]
    fn provider_test_output_is_typed_and_contains_no_endpoint_or_credential() {
        let output = OperationOutput::ProviderTests {
            results: vec![ProviderTestResult {
                provider_id: "relay".to_string(),
                reachability: ProviderReachability::Operational,
                response_time_ms: Some(125),
                http_status: Some(200),
            }],
        };
        let json = serde_json::to_string(&output).expect("serialize provider tests");
        assert_eq!(
            json,
            r#"{"kind":"providerTests","results":[{"providerId":"relay","reachability":"operational","responseTimeMs":125,"httpStatus":200}]}"#
        );
        assert!(!json.contains("url"));
        assert!(!json.contains("key"));
    }

    #[test]
    fn update_recovery_wire_shape_cannot_attach_a_target_to_an_advisory() {
        let available = ToolUpdateRecovery::available("1.2.3".to_string()).unwrap();
        assert_eq!(
            serde_json::to_string(&available).unwrap(),
            r#"{"kind":"available","targetVersion":"1.2.3"}"#
        );
        assert!(ToolUpdateRecovery::available("latest; unsafe".to_string()).is_err());
        assert_eq!(
            serde_json::to_string(&ToolUpdateRecovery::OwnershipChanged).unwrap(),
            r#"{"kind":"ownershipChanged"}"#
        );
    }

    #[test]
    fn an_extension_target_keeps_the_locking_tool_and_task_name_separate() {
        let operation = Operation::new(
            OperationId("op-skill".to_string()),
            OperationKind::Install,
            Some(ToolId::ClaudeCode),
        )
        .with_extension(OperationExtension {
            kind: ExtensionKind::Skill,
            id: "anthropics/skills:code-review".to_string(),
            name: "Code review".to_string(),
        });
        let json = serde_json::to_string(&operation).expect("serialize skill operation");
        assert!(json.contains(r#""tool":"claude-code""#));
        assert!(json.contains(
            r#""extension":{"kind":"skill","id":"anthropics/skills:code-review","name":"Code review"}"#
        ));
    }

    #[test]
    fn phase_keys_are_stable_translation_keys() {
        assert_eq!(phase::PREPARING, "operation.phase.preparing");
        assert_eq!(phase::DOWNLOADING, "operation.phase.downloading");
        assert_eq!(phase::INSTALLING, "operation.phase.installing");
        assert_eq!(phase::CONFIGURING, "operation.phase.configuring");
        assert_eq!(phase::REMOVING, "operation.phase.removing");
        assert_eq!(phase::CHECKING, "operation.phase.checking");
        assert_eq!(phase::CANCELLING, "operation.phase.cancelling");
        assert_eq!(phase::CANCELLED, "operation.phase.cancelled");
        assert_eq!(phase::READY, "operation.phase.ready");
        assert_eq!(phase::REMOVED, "operation.phase.removed");
    }

    #[test]
    fn log_message_keys_and_entry_shapes_are_stable() {
        assert_eq!(log_message::STARTED, "operation.log.started");
        assert_eq!(log_message::TRUNCATED, "operation.log.truncated");

        let message = OperationLogEntry::message(7, OperationLogKind::System, log_message::STARTED);
        assert_eq!(message.message_key.as_deref(), Some(log_message::STARTED));
        assert_eq!(message.detail, None);

        let detail = OperationLogEntry::detail(8, OperationLogKind::Stderr, "network timeout");
        assert_eq!(detail.message_key, None);
        assert_eq!(detail.detail.as_deref(), Some("network timeout"));
    }
}
