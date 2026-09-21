use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::domain::operation::{log_message, phase};
use crate::domain::{
    AppError, DesktopAppId, ErrorCode, Operation, OperationExtension, OperationId, OperationKind,
    OperationLogEntry, OperationLogKind, OperationOutput, OperationStatus, ProviderTestResult,
    Tool, ToolId, ToolUpdateRecovery, MAX_PROVIDER_TEST_ALL_RESULTS,
};
use crate::platform::executor::CommandCancellation;

pub const OPERATION_CHANGED_EVENT: &str = "operation://changed";

/// The Task Center (spec §40) only needs "recent history". Beyond the limit, **terminal**
/// operations are dropped oldest-first by end time; not a single unfinished one is dropped —
/// silently deleting a running task would leave the frontend waiting forever for a terminal state.
pub const MAX_RETAINED_OPERATIONS: usize = 200;
/// Logs reach the renderer through operation events, so both the number of entries and the
/// size of each entry must be bounded. The last two slots are reserved for the "truncated"
/// marker and the terminal state, so a noisy command cannot drown out the real
/// completion/failure conclusion.
pub const MAX_OPERATION_LOG_ENTRIES: usize = 120;
pub const MAX_OPERATION_LOG_DETAIL_BYTES: usize = 4_096;

/// Event sink abstraction: the production implementation goes through Tauri and the test
/// implementation collects into memory, so the OperationManager unit tests do not depend on
/// the Tauri runtime.
pub trait OperationEvents: Send + Sync {
    fn emit(&self, operation: &Operation);
}

pub struct TauriOperationEvents {
    app: tauri::AppHandle,
}

impl TauriOperationEvents {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl OperationEvents for TauriOperationEvents {
    fn emit(&self, operation: &Operation) {
        use tauri::Emitter;
        if let Err(e) = self.app.emit(OPERATION_CHANGED_EVENT, operation) {
            log::warn!("failed to emit {OPERATION_CHANGED_EVENT}: {e}");
        }
    }
}

/// Spec §39/§41: long-running task state machine + per-tool write mutual exclusion.
/// Lock order convention: when several locks are needed at once, `ops` -> `tool_locks` ->
/// `desktop_app_locks` -> `cancellations`.
pub struct ToolMutationGuard<'a> {
    manager: &'a OperationManager,
    tool: ToolId,
}

impl Drop for ToolMutationGuard<'_> {
    fn drop(&mut self) {
        lock(&self.manager.tool_locks).remove(&self.tool);
    }
}

pub struct OperationManager {
    ops: Mutex<HashMap<OperationId, Operation>>,
    tool_locks: Mutex<HashSet<ToolId>>,
    desktop_app_locks: Mutex<HashSet<DesktopAppId>>,
    cancellations: Mutex<HashMap<OperationId, CommandCancellation>>,
    events: Box<dyn OperationEvents>,
}

impl OperationManager {
    pub fn new(events: Box<dyn OperationEvents>) -> Self {
        Self {
            ops: Mutex::new(HashMap::new()),
            tool_locks: Mutex::new(HashSet::new()),
            desktop_app_locks: Mutex::new(HashSet::new()),
            cancellations: Mutex::new(HashMap::new()),
            events,
        }
    }

    pub fn guard_tool_mutation(&self, tool: ToolId) -> Result<ToolMutationGuard<'_>, AppError> {
        let mut tool_locks = lock(&self.tool_locks);
        if tool_locks.contains(&tool) {
            return Err(
                AppError::new(ErrorCode::OperationConflict, "error.operation.toolBusy")
                    .with_technical(format!("{} is already mutating", tool.as_str())),
            );
        }
        tool_locks.insert(tool);
        Ok(ToolMutationGuard {
            manager: self,
            tool,
        })
    }

    pub fn begin(
        &self,
        kind: OperationKind,
        tool: Option<ToolId>,
    ) -> Result<OperationId, AppError> {
        self.begin_inner(kind, tool, None, None, None, false)
    }

    pub fn begin_cancellable(
        &self,
        kind: OperationKind,
        tool: ToolId,
    ) -> Result<OperationId, AppError> {
        self.begin_inner(kind, Some(tool), None, None, None, true)
    }

    pub fn begin_extension(
        &self,
        kind: OperationKind,
        tool: ToolId,
        extension: OperationExtension,
    ) -> Result<OperationId, AppError> {
        self.begin_inner(kind, Some(tool), None, Some(extension), None, false)
    }

    pub fn begin_desktop_extension(
        &self,
        kind: OperationKind,
        desktop_app: DesktopAppId,
        extension: OperationExtension,
    ) -> Result<OperationId, AppError> {
        self.begin_inner(kind, None, Some(desktop_app), Some(extension), None, false)
    }

    /// Starts one output-bearing, read-only provider check. It deliberately
    /// does not take the per-tool mutation lock.
    pub fn begin_provider_tests(&self, tool: ToolId) -> Result<OperationId, AppError> {
        self.begin_inner(
            OperationKind::TestProviders,
            Some(tool),
            None,
            None,
            Some(OperationOutput::ProviderTests {
                results: Vec::new(),
            }),
            false,
        )
    }

    fn begin_inner(
        &self,
        kind: OperationKind,
        tool: Option<ToolId>,
        desktop_app: Option<DesktopAppId>,
        extension: Option<OperationExtension>,
        output: Option<OperationOutput>,
        cancellable: bool,
    ) -> Result<OperationId, AppError> {
        let has_provider_output =
            matches!(output.as_ref(), Some(OperationOutput::ProviderTests { .. }));
        if (kind == OperationKind::TestProviders) != has_provider_output
            || (output.is_some() && extension.is_some())
            || (has_provider_output && tool.is_none())
            || (tool.is_some() && desktop_app.is_some())
            || (desktop_app.is_some() && extension.is_none())
            || (has_provider_output && desktop_app.is_some())
        {
            return Err(AppError::new(
                ErrorCode::OperationConflict,
                "error.operation.invalidTransition",
            )
            .with_technical("operation kind, target, and typed output do not match"));
        }
        if cancellable && !kind.is_mutation() {
            return Err(AppError::new(
                ErrorCode::OperationConflict,
                "error.operation.notCancellable",
            ));
        }
        let mut ops = lock(&self.ops);
        let mut tool_locks = lock(&self.tool_locks);
        let mut desktop_app_locks = lock(&self.desktop_app_locks);

        if kind == OperationKind::TestProviders
            && ops.values().any(|operation| {
                operation.kind == OperationKind::TestProviders
                    && operation.tool == tool
                    && !operation.status.is_terminal()
            })
        {
            return Err(
                AppError::new(ErrorCode::OperationConflict, "error.provider.testFailed")
                    .with_technical("provider address checks are already running for this tool")
                    .with_remediation("error.remediation.retryOrViewDetails"),
            );
        }

        let mutation_target = tool.filter(|_| kind.is_mutation());
        if let Some(target) = mutation_target {
            if tool_locks.contains(&target) {
                return Err(AppError::new(
                    ErrorCode::OperationConflict,
                    "error.operation.toolBusy",
                )
                .with_technical(format!("{} is already mutating", target.as_str())));
            }
        }
        let desktop_mutation_target = desktop_app.filter(|_| kind.is_mutation());
        if let Some(target) = desktop_mutation_target {
            if desktop_app_locks.contains(&target) {
                return Err(AppError::new(
                    ErrorCode::OperationConflict,
                    "error.operation.desktopAppBusy",
                )
                .with_technical(format!("{} is already mutating", target.as_str())));
            }
        }

        let id = OperationId::new();
        let mut operation = Operation::new(id.clone(), kind, tool);
        operation.desktop_app = desktop_app;
        operation.extension = extension;
        operation.output = output;
        operation.transition(OperationStatus::Running)?;
        operation.started_at = Some(now_millis());
        push_log(
            &mut operation,
            OperationLogEntry::message(
                now_millis(),
                OperationLogKind::System,
                log_message::STARTED,
            ),
        );

        if let Some(target) = mutation_target {
            tool_locks.insert(target);
        }
        if let Some(target) = desktop_mutation_target {
            desktop_app_locks.insert(target);
        }
        if cancellable {
            lock(&self.cancellations).insert(id.clone(), CommandCancellation::default());
        }
        ops.insert(id.clone(), operation.clone());
        prune_terminal(&mut ops);
        drop(tool_locks);
        drop(desktop_app_locks);
        drop(ops);

        self.events.emit(&operation);
        Ok(id)
    }

    /// Only a Running operation can receive progress. Pushing progress after a terminal state
    /// would make the frontend progress bar go backwards and would mask caller bugs such as
    /// "who is writing to an already finished task".
    pub fn update_progress(
        &self,
        id: &OperationId,
        progress: u8,
        message_key: Option<String>,
    ) -> Result<(), AppError> {
        self.update_progress_inner(id, progress, message_key, None)
    }

    /// Atomically moves progress and the authoritative cancel gate. The gate
    /// opens only on work registered by `begin_cancellable`.
    pub fn update_cancellable_progress(
        &self,
        id: &OperationId,
        progress: u8,
        message_key: Option<String>,
        can_cancel: bool,
    ) -> Result<(), AppError> {
        let has_handle = lock(&self.cancellations).contains_key(id);
        if can_cancel && !has_handle {
            return Err(AppError::new(
                ErrorCode::OperationConflict,
                "error.operation.notCancellable",
            )
            .with_context_id(id.as_str()));
        }
        self.update_progress_inner(id, progress, message_key, Some(can_cancel))
    }

    fn update_progress_inner(
        &self,
        id: &OperationId,
        progress: u8,
        message_key: Option<String>,
        can_cancel: Option<bool>,
    ) -> Result<(), AppError> {
        let mut ops = lock(&self.ops);
        let operation = ops.get_mut(id).ok_or_else(not_found)?;
        if operation.status != OperationStatus::Running {
            let status = operation.status;
            return Err(
                AppError::new(ErrorCode::OperationConflict, "error.operation.notRunning")
                    .with_technical(format!("{status:?}"))
                    .with_context_id(id.as_str()),
            );
        }
        if operation.cancel_requested {
            return Err(cancel_pending(id));
        }
        operation.progress = progress.min(100);
        if let Some(can_cancel) = can_cancel {
            operation.can_cancel = can_cancel;
        }
        if let Some(message_key) = message_key {
            if operation.message_key.as_deref() != Some(message_key.as_str()) {
                push_log(
                    operation,
                    OperationLogEntry::message(
                        now_millis(),
                        OperationLogKind::Phase,
                        message_key.clone(),
                    ),
                );
            }
            operation.message_key = Some(message_key);
        }
        let snapshot = operation.clone();
        drop(ops);

        self.events.emit(&snapshot);
        Ok(())
    }

    /// Atomically publishes one completed provider measurement together with
    /// monotonic task progress. `None` records a completed-but-errored probe;
    /// the operation's terminal error remains the authoritative diagnosis.
    pub fn record_provider_test(
        &self,
        id: &OperationId,
        progress: u8,
        result: Option<ProviderTestResult>,
    ) -> Result<(), AppError> {
        let mut ops = lock(&self.ops);
        let operation = ops.get_mut(id).ok_or_else(not_found)?;
        ensure_running(operation, id)?;
        if operation.kind != OperationKind::TestProviders {
            return Err(AppError::new(
                ErrorCode::OperationConflict,
                "error.operation.invalidTransition",
            )
            .with_technical("provider test output requires a testProviders operation")
            .with_context_id(id.as_str()));
        }

        let Some(OperationOutput::ProviderTests { results }) = operation.output.as_mut() else {
            return Err(AppError::new(
                ErrorCode::OperationConflict,
                "error.operation.invalidTransition",
            )
            .with_technical("provider test operation has no typed output")
            .with_context_id(id.as_str()));
        };
        if let Some(result) = result {
            if results
                .iter()
                .any(|existing| existing.provider_id == result.provider_id)
            {
                return Err(AppError::new(
                    ErrorCode::OperationConflict,
                    "error.operation.invalidTransition",
                )
                .with_technical("provider test result was reported twice")
                .with_context_id(id.as_str()));
            }
            if results.len() >= MAX_PROVIDER_TEST_ALL_RESULTS {
                return Err(AppError::new(
                    ErrorCode::OperationConflict,
                    "error.operation.invalidTransition",
                )
                .with_technical("provider test output limit exceeded")
                .with_context_id(id.as_str()));
            }
            results.push(result);
            results.sort_by(|left, right| left.provider_id.cmp(&right.provider_id));
        }

        // Running work never advertises 100%; only the terminal transition is
        // allowed to do that. Concurrent reporters also cannot move backwards.
        operation.progress = operation.progress.max(progress.min(99));
        if operation.message_key.as_deref() != Some(phase::CHECKING) {
            push_log(
                operation,
                OperationLogEntry::message(now_millis(), OperationLogKind::Phase, phase::CHECKING),
            );
            operation.message_key = Some(phase::CHECKING.to_string());
        }
        let snapshot = operation.clone();
        drop(ops);

        self.events.emit(&snapshot);
        Ok(())
    }

    pub fn cancellation(&self, id: &OperationId) -> Result<CommandCancellation, AppError> {
        lock(&self.cancellations).get(id).cloned().ok_or_else(|| {
            AppError::new(
                ErrorCode::OperationConflict,
                "error.operation.notCancellable",
            )
            .with_context_id(id.as_str())
        })
    }

    /// A request keeps the operation Running and its tool locked. Only
    /// `finish_cancelled` may publish the terminal state after process exit.
    pub fn request_cancel(&self, id: &OperationId) -> Result<Operation, AppError> {
        let cancellation = self.cancellation(id)?;
        let mut ops = lock(&self.ops);
        let operation = ops.get_mut(id).ok_or_else(not_found)?;
        ensure_running(operation, id)?;
        if operation.cancel_requested {
            return Err(cancel_pending(id));
        }
        if !operation.can_cancel {
            return Err(AppError::new(
                ErrorCode::OperationConflict,
                "error.operation.notCancellableNow",
            )
            .with_context_id(id.as_str()));
        }
        if !cancellation.request() {
            return Err(cancel_pending(id));
        }
        operation.can_cancel = false;
        operation.cancel_requested = true;
        operation.message_key = Some(phase::CANCELLING.to_string());
        push_log(
            operation,
            OperationLogEntry::message(now_millis(), OperationLogKind::Phase, phase::CANCELLING),
        );
        push_log(
            operation,
            OperationLogEntry::message(
                now_millis(),
                OperationLogKind::System,
                log_message::CANCELLATION_REQUESTED,
            ),
        );
        let snapshot = operation.clone();
        drop(ops);

        self.events.emit(&snapshot);
        Ok(snapshot)
    }

    /// Append a structured log entry. Only Running operations are accepted, and nothing is written to disk.
    pub fn append_log_message(
        &self,
        id: &OperationId,
        kind: OperationLogKind,
        message_key: &'static str,
    ) -> Result<(), AppError> {
        self.append_log(
            id,
            OperationLogEntry::message(now_millis(), kind, message_key),
        )
    }

    /// Append technical detail the caller already redacted; an independent byte limit is applied here as a second boundary.
    pub fn append_log_detail(
        &self,
        id: &OperationId,
        kind: OperationLogKind,
        detail: String,
    ) -> Result<(), AppError> {
        let detail = bound_detail(detail);
        if detail.is_empty() {
            return Ok(());
        }
        self.append_log(id, OperationLogEntry::detail(now_millis(), kind, detail))
    }

    fn append_log(&self, id: &OperationId, entry: OperationLogEntry) -> Result<(), AppError> {
        let mut ops = lock(&self.ops);
        let operation = ops.get_mut(id).ok_or_else(not_found)?;
        ensure_running(operation, id)?;
        if !push_log(operation, entry) {
            return Ok(());
        }
        let snapshot = operation.clone();
        drop(ops);

        self.events.emit(&snapshot);
        Ok(())
    }

    /// Once a terminal transition succeeds, **the event is always emitted and the tool lock is
    /// always released**. Releasing the lock takes the poison-tolerant path: returning early on
    /// a poisoned lock would make every write operation for that tool fail with
    /// OPERATION_CONFLICT forever, and `HashSet<ToolId>` has no cross-field invariant, so
    /// recovering it is strictly better than deadlocking (Phase 1 final-review TD).
    pub fn finish(&self, id: &OperationId, outcome: Result<(), AppError>) -> Result<(), AppError> {
        self.finish_inner(id, outcome.map(|()| None), None)
    }

    /// Terminal transition for a tool lifecycle action. Success must carry the
    /// detection the action already verified, so the renderer can trust it
    /// without re-scanning; failure may carry a fail-closed recovery
    /// assessment. The signature makes "succeeded but published nothing
    /// trustworthy" unrepresentable.
    ///
    /// Intermediate running snapshots never advertise a recovery action before
    /// the original update has released its tool lock.
    pub fn finish_tool_lifecycle(
        &self,
        id: &OperationId,
        outcome: Result<Tool, AppError>,
        update_recovery: Option<ToolUpdateRecovery>,
    ) -> Result<(), AppError> {
        self.finish_inner(
            id,
            outcome.map(|tool| {
                Some(OperationOutput::ToolInventory {
                    tool: Box::new(tool),
                })
            }),
            update_recovery,
        )
    }

    fn finish_inner(
        &self,
        id: &OperationId,
        outcome: Result<Option<OperationOutput>, AppError>,
        update_recovery: Option<ToolUpdateRecovery>,
    ) -> Result<(), AppError> {
        let cancellation = lock(&self.cancellations).get(id).cloned();
        let mut ops = lock(&self.ops);
        let operation = ops.get_mut(id).ok_or_else(not_found)?;
        if operation.cancel_requested
            && !cancellation
                .as_ref()
                .is_some_and(CommandCancellation::is_failed)
        {
            return Err(cancel_pending(id));
        }
        if update_recovery.is_some() && (outcome.is_ok() || operation.kind != OperationKind::Update)
        {
            return Err(AppError::new(
                ErrorCode::OperationConflict,
                "error.operation.invalidTransition",
            )
            .with_technical("update recovery requires a failed update operation"));
        }
        match outcome {
            Ok(output) => {
                operation.transition(OperationStatus::Success)?;
                operation.can_cancel = false;
                operation.update_recovery = None;
                operation.progress = 100;
                operation.error = None;
                if let Some(output) = output {
                    operation.output = Some(output);
                }
                push_terminal_log(
                    operation,
                    OperationLogEntry::message(
                        now_millis(),
                        OperationLogKind::System,
                        log_message::SUCCEEDED,
                    ),
                );
            }
            Err(error) => {
                operation.transition(OperationStatus::Failed)?;
                operation.can_cancel = false;
                operation.update_recovery = update_recovery;
                operation.error = Some(error);
                push_terminal_log(
                    operation,
                    OperationLogEntry::message(
                        now_millis(),
                        OperationLogKind::System,
                        log_message::FAILED,
                    ),
                );
            }
        }
        operation.finished_at = Some(now_millis());
        let snapshot = operation.clone();
        let released = snapshot.tool.filter(|_| snapshot.kind.is_mutation());
        let released_desktop = snapshot.desktop_app.filter(|_| snapshot.kind.is_mutation());
        drop(ops);

        if let Some(target) = released {
            lock(&self.tool_locks).remove(&target);
        }
        if let Some(target) = released_desktop {
            lock(&self.desktop_app_locks).remove(&target);
        }
        lock(&self.cancellations).remove(id);
        self.events.emit(&snapshot);
        Ok(())
    }

    /// Completes a requested cancellation only after the executor proved that
    /// no child process remains. Until then the per-tool lock stays held.
    pub fn finish_cancelled(&self, id: &OperationId) -> Result<(), AppError> {
        let cancellation = self.cancellation(id)?;
        if !cancellation.is_confirmed() {
            return Err(AppError::new(
                ErrorCode::OperationConflict,
                "error.operation.cancelUnconfirmed",
            )
            .with_context_id(id.as_str()));
        }

        let mut ops = lock(&self.ops);
        let operation = ops.get_mut(id).ok_or_else(not_found)?;
        ensure_running(operation, id)?;
        if !operation.cancel_requested {
            return Err(AppError::new(
                ErrorCode::OperationConflict,
                "error.operation.cancelNotRequested",
            )
            .with_context_id(id.as_str()));
        }
        operation.transition(OperationStatus::Cancelled)?;
        operation.can_cancel = false;
        operation.message_key = Some(phase::CANCELLED.to_string());
        operation.update_recovery = None;
        operation.error = None;
        push_terminal_log(
            operation,
            OperationLogEntry::message(
                now_millis(),
                OperationLogKind::System,
                log_message::CANCELLED,
            ),
        );
        operation.finished_at = Some(now_millis());
        let snapshot = operation.clone();
        let released = snapshot.tool.filter(|_| snapshot.kind.is_mutation());
        let released_desktop = snapshot.desktop_app.filter(|_| snapshot.kind.is_mutation());
        drop(ops);

        if let Some(target) = released {
            lock(&self.tool_locks).remove(&target);
        }
        if let Some(target) = released_desktop {
            lock(&self.desktop_app_locks).remove(&target);
        }
        lock(&self.cancellations).remove(id);
        self.events.emit(&snapshot);
        Ok(())
    }

    pub fn list(&self) -> Vec<Operation> {
        let ops = lock(&self.ops);
        let mut operations: Vec<Operation> = ops.values().cloned().collect();
        drop(ops);
        operations.sort_by(|a, b| {
            a.started_at
                .cmp(&b.started_at)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
        operations
    }

    /// Read-only single lookup; it takes no part in the state machine or lock semantics. The
    /// use-case layer uses it to verify that the id and request are paired before running an
    /// action (spec §41), so a mismatched id cannot bypass the per-tool lock.
    pub fn get(&self, id: &OperationId) -> Option<Operation> {
        lock(&self.ops).get(id).cloned()
    }
}

/// A poisoned lock is always recovered rather than reported. The protected collections have
/// no cross-field invariants: at worst a panic leaves a half-written `Operation`, while a
/// poisoned lock would make **every** subsequent begin/finish fail.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn ensure_running(operation: &Operation, id: &OperationId) -> Result<(), AppError> {
    if operation.status == OperationStatus::Running {
        return Ok(());
    }
    Err(
        AppError::new(ErrorCode::OperationConflict, "error.operation.notRunning")
            .with_technical(format!("{:?}", operation.status))
            .with_context_id(id.as_str()),
    )
}

fn push_log(operation: &mut Operation, entry: OperationLogEntry) -> bool {
    if operation.logs.iter().any(is_truncation_entry) {
        return false;
    }
    if operation.logs.len() >= MAX_OPERATION_LOG_ENTRIES.saturating_sub(2) {
        operation.logs.push(OperationLogEntry::message(
            now_millis(),
            OperationLogKind::System,
            log_message::TRUNCATED,
        ));
        return true;
    }
    operation.logs.push(entry);
    true
}

fn push_terminal_log(operation: &mut Operation, entry: OperationLogEntry) {
    if operation.logs.len() >= MAX_OPERATION_LOG_ENTRIES {
        operation.logs.truncate(MAX_OPERATION_LOG_ENTRIES - 1);
    }
    operation.logs.push(entry);
}

fn is_truncation_entry(entry: &OperationLogEntry) -> bool {
    entry.message_key.as_deref() == Some(log_message::TRUNCATED)
}

fn bound_detail(detail: String) -> String {
    let detail = detail.trim();
    if detail.len() <= MAX_OPERATION_LOG_DETAIL_BYTES {
        return detail.to_string();
    }
    let mut start = detail.len() - MAX_OPERATION_LOG_DETAIL_BYTES;
    while start < detail.len() && !detail.is_char_boundary(start) {
        start += 1;
    }
    detail[start..].to_string()
}

/// Beyond the limit, terminal operations are dropped oldest-first by "end time -> id". Unfinished ones are excluded.
fn prune_terminal(ops: &mut HashMap<OperationId, Operation>) {
    if ops.len() <= MAX_RETAINED_OPERATIONS {
        return;
    }
    let mut terminal: Vec<(i64, OperationId)> = ops
        .values()
        .filter(|operation| operation.status.is_terminal())
        .map(|operation| {
            (
                operation.finished_at.or(operation.started_at).unwrap_or(0),
                operation.id.clone(),
            )
        })
        .collect();
    terminal.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.as_str().cmp(b.1.as_str())));

    let excess = ops.len() - MAX_RETAINED_OPERATIONS;
    for (_, id) in terminal.into_iter().take(excess) {
        ops.remove(&id);
    }
}

fn not_found() -> AppError {
    AppError::new(ErrorCode::Internal, "error.operation.notFound")
}

fn cancel_pending(id: &OperationId) -> AppError {
    AppError::new(
        ErrorCode::OperationConflict,
        "error.operation.cancelPending",
    )
    .with_context_id(id.as_str())
}

fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::{
        OperationEvents, OperationManager, MAX_OPERATION_LOG_DETAIL_BYTES,
        MAX_OPERATION_LOG_ENTRIES, OPERATION_CHANGED_EVENT,
    };
    use crate::domain::operation::{log_message, phase};
    use crate::domain::{
        AppError, DesktopAppId, ErrorCode, ExtensionKind, Operation, OperationExtension,
        OperationKind, OperationLogKind, OperationOutput, OperationStatus, ProviderReachability,
        ProviderTestResult, ToolId, ToolUpdateRecovery,
    };
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingEvents {
        seen: Mutex<Vec<Operation>>,
    }

    impl RecordingEvents {
        fn statuses(&self) -> Vec<OperationStatus> {
            self.seen
                .lock()
                .expect("recording events lock")
                .iter()
                .map(|operation| operation.status)
                .collect()
        }
    }

    impl OperationEvents for std::sync::Arc<RecordingEvents> {
        fn emit(&self, operation: &Operation) {
            self.seen
                .lock()
                .expect("recording events lock")
                .push(operation.clone());
        }
    }

    fn manager() -> (OperationManager, std::sync::Arc<RecordingEvents>) {
        let events = std::sync::Arc::new(RecordingEvents::default());
        (OperationManager::new(Box::new(events.clone())), events)
    }

    /// The frontend tests/native/client.test.ts asserts the same literal independently; both
    /// sides are pinned down so a rename on either side cannot leave the tests green while the
    /// production event send/receive no longer matches.
    #[test]
    fn operation_changed_event_name_is_pinned() {
        assert_eq!(OPERATION_CHANGED_EVENT, "operation://changed");
    }

    #[test]
    fn lifecycle_drives_the_state_machine_and_emits_each_change() {
        let (manager, events) = manager();
        let id = manager
            .begin(OperationKind::Update, Some(ToolId::OpenCode))
            .expect("update begins");

        let running = manager.list();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].id, id);
        assert_eq!(running[0].status, OperationStatus::Running);
        assert_eq!(running[0].kind, OperationKind::Update);
        assert_eq!(running[0].tool, Some(ToolId::OpenCode));
        assert!(running[0].started_at.is_some());
        assert_eq!(running[0].finished_at, None);
        assert_eq!(running[0].logs.len(), 1);
        assert_eq!(
            running[0].logs[0].message_key.as_deref(),
            Some(log_message::STARTED)
        );

        manager
            .update_progress(&id, 55, Some("operation.update.downloading".to_string()))
            .expect("progress updates");
        manager.finish(&id, Ok(())).expect("finish succeeds");

        let done = manager.list();
        assert_eq!(done[0].status, OperationStatus::Success);
        assert_eq!(done[0].progress, 100);
        assert_eq!(done[0].error, None);
        assert_eq!(
            done[0].message_key.as_deref(),
            Some("operation.update.downloading")
        );
        assert!(done[0].finished_at.is_some());
        assert_eq!(
            done[0]
                .logs
                .last()
                .and_then(|entry| entry.message_key.as_deref()),
            Some(log_message::SUCCEEDED)
        );
        assert_eq!(
            events.statuses(),
            vec![
                OperationStatus::Running,
                OperationStatus::Running,
                OperationStatus::Success
            ]
        );
    }

    #[test]
    fn explicit_tool_mutation_guard_shares_the_lifecycle_lock() {
        let (manager, _events) = manager();
        let guard = manager
            .guard_tool_mutation(ToolId::ClaudeCode)
            .expect("pin mutation takes the tool lock");
        let error = manager
            .begin(OperationKind::Update, Some(ToolId::ClaudeCode))
            .expect_err("lifecycle cannot race the pin mutation");
        assert_eq!(error.message_key, "error.operation.toolBusy");

        drop(guard);
        manager
            .begin(OperationKind::Update, Some(ToolId::ClaudeCode))
            .expect("dropping the guard releases the tool lock");
    }

    #[test]
    fn concurrent_mutation_on_the_same_tool_is_rejected() {
        let (manager, _events) = manager();
        manager
            .begin(OperationKind::Install, Some(ToolId::ClaudeCode))
            .expect("first install begins");
        let error = manager
            .begin(OperationKind::Update, Some(ToolId::ClaudeCode))
            .expect_err("second mutation must conflict");
        assert_eq!(error.code, ErrorCode::OperationConflict);
        assert_eq!(error.message_key, "error.operation.toolBusy");
        assert_eq!(manager.list().len(), 1);

        // Different tools do not block each other
        manager
            .begin(OperationKind::Install, Some(ToolId::Codex))
            .expect("codex install begins");
    }

    #[test]
    fn extension_work_keeps_its_metadata_and_uses_the_same_tool_lock() {
        let (manager, events) = manager();
        let target = OperationExtension {
            kind: ExtensionKind::Skill,
            id: "anthropics/skills:code-review".to_string(),
            name: "Code review".to_string(),
        };
        let id = manager
            .begin_extension(OperationKind::Install, ToolId::ClaudeCode, target.clone())
            .expect("skill install begins");

        let operation = manager.get(&id).expect("operation exists");
        assert_eq!(operation.tool, Some(ToolId::ClaudeCode));
        assert_eq!(operation.extension, Some(target.clone()));
        assert_eq!(
            events.seen.lock().expect("events")[0].extension,
            Some(target)
        );

        let error = manager
            .begin(OperationKind::Update, Some(ToolId::ClaudeCode))
            .expect_err("tool update must not race its skill install");
        assert_eq!(error.code, ErrorCode::OperationConflict);
    }

    #[test]
    fn desktop_extension_work_uses_an_independent_per_app_lock() {
        let (manager, _events) = manager();
        let target = OperationExtension {
            kind: ExtensionKind::Mcp,
            id: "filesystem".to_string(),
            name: "Filesystem".to_string(),
        };
        let first = manager
            .begin_desktop_extension(
                OperationKind::Install,
                DesktopAppId::ClaudeDesktop,
                target.clone(),
            )
            .expect("desktop MCP install begins");
        let operation = manager.get(&first).expect("operation exists");
        assert_eq!(operation.tool, None);
        assert_eq!(operation.desktop_app, Some(DesktopAppId::ClaudeDesktop));
        assert_eq!(operation.extension, Some(target.clone()));

        let error = manager
            .begin_desktop_extension(
                OperationKind::Uninstall,
                DesktopAppId::ClaudeDesktop,
                target.clone(),
            )
            .expect_err("the same desktop app cannot mutate twice");
        assert_eq!(error.code, ErrorCode::OperationConflict);
        assert_eq!(error.message_key, "error.operation.desktopAppBusy");

        manager
            .begin(OperationKind::Update, Some(ToolId::ClaudeCode))
            .expect("the related CLI tool has an independent lock");
        manager
            .finish(&first, Ok(()))
            .expect("release desktop lock");
        manager
            .begin_desktop_extension(
                OperationKind::Uninstall,
                DesktopAppId::ClaudeDesktop,
                target,
            )
            .expect("a completed desktop operation releases its app lock");
    }

    #[test]
    fn read_only_scans_never_take_the_tool_lock() {
        let (manager, _events) = manager();
        manager
            .begin(OperationKind::Scan, Some(ToolId::OpenCode))
            .expect("first scan begins");
        manager
            .begin(OperationKind::Scan, Some(ToolId::OpenCode))
            .expect("second scan begins");
        manager
            .begin(OperationKind::Update, Some(ToolId::OpenCode))
            .expect("update is not blocked by scans");
    }

    #[test]
    fn provider_tests_emit_typed_partial_results_without_taking_the_tool_lock() {
        let (manager, events) = manager();
        let id = manager
            .begin_provider_tests(ToolId::ClaudeCode)
            .expect("provider tests begin");
        let started = manager.get(&id).expect("operation exists");
        assert_eq!(started.kind, OperationKind::TestProviders);
        assert_eq!(started.tool, Some(ToolId::ClaudeCode));
        assert_eq!(
            started.output,
            Some(OperationOutput::ProviderTests {
                results: Vec::new()
            })
        );

        manager
            .begin(OperationKind::Update, Some(ToolId::ClaudeCode))
            .expect("read-only checks do not block tool mutation");
        manager
            .record_provider_test(
                &id,
                60,
                Some(ProviderTestResult {
                    provider_id: "z-relay".to_string(),
                    reachability: ProviderReachability::Failed,
                    response_time_ms: None,
                    http_status: None,
                }),
            )
            .expect("first partial result records");
        manager
            .record_provider_test(
                &id,
                40,
                Some(ProviderTestResult {
                    provider_id: "a-official".to_string(),
                    reachability: ProviderReachability::Operational,
                    response_time_ms: Some(80),
                    http_status: Some(401),
                }),
            )
            .expect("second partial result records");

        let running = manager.get(&id).expect("operation exists");
        assert_eq!(running.progress, 60, "concurrent progress cannot regress");
        assert_eq!(running.message_key.as_deref(), Some(phase::CHECKING));
        let Some(OperationOutput::ProviderTests { ref results }) = running.output else {
            panic!("provider tests keep typed output");
        };
        assert_eq!(
            results
                .iter()
                .map(|result| result.provider_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a-official", "z-relay"]
        );
        assert_eq!(events.seen.lock().expect("events").last(), Some(&running));
    }

    #[test]
    fn only_one_provider_test_batch_runs_per_tool_but_other_read_work_stays_free() {
        let (manager, _events) = manager();
        manager
            .begin_provider_tests(ToolId::ClaudeCode)
            .expect("first provider batch begins");
        assert_eq!(
            manager
                .begin_provider_tests(ToolId::ClaudeCode)
                .expect_err("duplicate batch is rejected")
                .code,
            ErrorCode::OperationConflict
        );
        manager
            .begin_provider_tests(ToolId::Codex)
            .expect("a different tool checks concurrently");
        manager
            .begin(OperationKind::Scan, Some(ToolId::ClaudeCode))
            .expect("an unrelated read-only scan still runs");
    }

    #[test]
    fn provider_test_output_rejects_duplicates_and_the_wrong_operation_kind() {
        let (manager, _events) = manager();
        let id = manager
            .begin_provider_tests(ToolId::Codex)
            .expect("provider tests begin");
        let result = ProviderTestResult {
            provider_id: "relay".to_string(),
            reachability: ProviderReachability::Degraded,
            response_time_ms: Some(900),
            http_status: Some(200),
        };
        manager
            .record_provider_test(&id, 50, Some(result.clone()))
            .expect("first result records");
        assert_eq!(
            manager
                .record_provider_test(&id, 75, Some(result))
                .expect_err("duplicates fail closed")
                .code,
            ErrorCode::OperationConflict
        );
        assert_eq!(
            manager
                .begin(OperationKind::TestProviders, Some(ToolId::Codex))
                .expect_err("generic begin cannot create an output-less provider task")
                .code,
            ErrorCode::OperationConflict
        );

        let scan = manager
            .begin(OperationKind::Scan, Some(ToolId::Codex))
            .expect("scan begins");
        assert_eq!(
            manager
                .record_provider_test(&scan, 50, None)
                .expect_err("ordinary scan cannot carry provider results")
                .code,
            ErrorCode::OperationConflict
        );
    }

    #[test]
    fn finishing_releases_the_tool_lock_and_records_failures() {
        let (manager, _events) = manager();
        let id = manager
            .begin(OperationKind::Install, Some(ToolId::Codex))
            .expect("install begins");
        let failure = AppError::new(ErrorCode::InstallFailed, "error.tool.installFailed");
        manager
            .finish(&id, Err(failure.clone()))
            .expect("finish records failure");

        let operations = manager.list();
        assert_eq!(operations[0].status, OperationStatus::Failed);
        assert_eq!(operations[0].error, Some(failure));

        manager
            .begin(OperationKind::Uninstall, Some(ToolId::Codex))
            .expect("lock released after finish");
    }

    #[test]
    fn cancellation_keeps_the_tool_locked_until_process_exit_is_confirmed() {
        let (manager, events) = manager();
        let id = manager
            .begin_cancellable(OperationKind::Update, ToolId::Codex)
            .expect("cancellable update begins");
        manager
            .update_cancellable_progress(&id, 20, Some(phase::DOWNLOADING.to_string()), true)
            .expect("safe phase opens cancellation");
        let cancellation = manager.cancellation(&id).expect("signal exists");
        let requested = manager.request_cancel(&id).expect("cancel is requested");
        assert_eq!(requested.status, OperationStatus::Running);
        assert!(!requested.can_cancel);
        assert_eq!(requested.message_key.as_deref(), Some(phase::CANCELLING));
        assert_eq!(
            manager
                .finish_cancelled(&id)
                .expect_err("an unconfirmed process cannot be called cancelled")
                .message_key,
            "error.operation.cancelUnconfirmed"
        );
        assert_eq!(
            manager
                .begin(OperationKind::Uninstall, Some(ToolId::Codex))
                .expect_err("the original tool lock remains held")
                .code,
            ErrorCode::OperationConflict
        );

        assert!(cancellation.confirm_if_requested());
        manager
            .finish_cancelled(&id)
            .expect("confirmed cancellation finishes");
        let done = manager.get(&id).expect("operation remains in history");
        assert_eq!(done.status, OperationStatus::Cancelled);
        assert_eq!(done.message_key.as_deref(), Some(phase::CANCELLED));
        assert_eq!(done.error, None);
        assert!(done.finished_at.is_some());
        assert_eq!(
            done.logs
                .last()
                .and_then(|entry| entry.message_key.as_deref()),
            Some(log_message::CANCELLED)
        );
        assert_eq!(events.statuses().last(), Some(&OperationStatus::Cancelled));
        manager
            .begin(OperationKind::Uninstall, Some(ToolId::Codex))
            .expect("the lock releases only after confirmation");
    }

    #[test]
    fn unsafe_and_unregistered_operations_reject_cancel_requests() {
        let (manager, _events) = manager();
        let id = manager
            .begin_cancellable(OperationKind::Update, ToolId::OpenCode)
            .expect("update begins");
        manager
            .update_cancellable_progress(&id, 55, Some(phase::INSTALLING.to_string()), false)
            .expect("irreversible phase closes the gate");
        let error = manager
            .request_cancel(&id)
            .expect_err("installing cannot be cancelled");
        assert_eq!(error.message_key, "error.operation.notCancellableNow");
        assert_eq!(manager.get(&id).unwrap().status, OperationStatus::Running);
        manager
            .finish(&id, Ok(()))
            .expect("normal finish still works");

        let scan = manager
            .begin(OperationKind::Scan, None)
            .expect("ordinary operation begins");
        assert_eq!(
            manager
                .request_cancel(&scan)
                .expect_err("no executor handle was registered")
                .message_key,
            "error.operation.notCancellable"
        );
    }

    #[test]
    fn only_a_failed_update_can_publish_an_atomic_recovery_assessment() {
        let (manager, events) = manager();
        let id = manager
            .begin(OperationKind::Update, Some(ToolId::Codex))
            .expect("update begins");
        let recovery = ToolUpdateRecovery::available("1.2.3".to_string()).unwrap();
        manager
            .finish_tool_lifecycle(
                &id,
                Err(AppError::new(
                    ErrorCode::UpdateFailed,
                    "error.tool.updateFailed",
                )),
                Some(recovery.clone()),
            )
            .expect("failed update finishes with recovery");

        let operation = manager.get(&id).unwrap();
        assert_eq!(operation.status, OperationStatus::Failed);
        assert_eq!(operation.update_recovery, Some(recovery));
        assert_eq!(events.seen.lock().unwrap().last(), Some(&operation));
        manager
            .finish_tool_lifecycle(
                &id,
                Err(AppError::new(
                    ErrorCode::UpdateFailed,
                    "error.tool.updateFailed",
                )),
                Some(ToolUpdateRecovery::OwnershipChanged),
            )
            .expect_err("a terminal operation cannot be rewritten");
        assert_eq!(manager.get(&id).unwrap(), operation);

        let install = manager
            .begin(OperationKind::Install, Some(ToolId::ClaudeCode))
            .unwrap();
        let error = manager
            .finish_tool_lifecycle(
                &install,
                Err(AppError::new(
                    ErrorCode::InstallFailed,
                    "error.tool.installFailed",
                )),
                Some(ToolUpdateRecovery::HistoryUnavailable),
            )
            .expect_err("non-update recovery metadata must fail closed");
        assert_eq!(error.code, ErrorCode::OperationConflict);
        assert_eq!(
            manager.get(&install).unwrap().status,
            OperationStatus::Running
        );
    }

    #[test]
    fn progress_is_clamped_to_one_hundred() {
        let (manager, _events) = manager();
        let id = manager
            .begin(OperationKind::Scan, None)
            .expect("scan begins");
        manager
            .update_progress(&id, 200, None)
            .expect("progress updates");
        assert_eq!(manager.list()[0].progress, 100);
    }

    #[test]
    fn phase_and_technical_logs_emit_only_while_running() {
        let (manager, _events) = manager();
        let id = manager
            .begin(OperationKind::Update, Some(ToolId::Codex))
            .expect("update begins");
        manager
            .update_progress(&id, 10, Some("operation.phase.downloading".to_string()))
            .expect("phase updates");
        manager
            .update_progress(&id, 20, Some("operation.phase.downloading".to_string()))
            .expect("same phase updates progress without duplicating the log");
        manager
            .append_log_detail(
                &id,
                OperationLogKind::Command,
                "npm install -g @openai/codex@latest".to_string(),
            )
            .expect("command log appends");

        let running = manager.get(&id).expect("operation exists");
        assert_eq!(
            running
                .logs
                .iter()
                .filter(|entry| {
                    entry.message_key.as_deref() == Some("operation.phase.downloading")
                })
                .count(),
            1
        );
        assert_eq!(
            running
                .logs
                .last()
                .and_then(|entry| entry.detail.as_deref()),
            Some("npm install -g @openai/codex@latest")
        );

        manager.finish(&id, Ok(())).expect("finish succeeds");
        let error = manager
            .append_log_message(
                &id,
                OperationLogKind::System,
                log_message::COMMAND_SUCCEEDED,
            )
            .expect_err("terminal operation rejects late logs");
        assert_eq!(error.message_key, "error.operation.notRunning");
    }

    #[test]
    fn log_details_and_history_are_bounded_and_keep_the_terminal_result() {
        let (manager, _events) = manager();
        let id = manager
            .begin(OperationKind::Install, Some(ToolId::ClaudeCode))
            .expect("install begins");
        manager
            .append_log_detail(
                &id,
                OperationLogKind::Stdout,
                "€".repeat(MAX_OPERATION_LOG_DETAIL_BYTES),
            )
            .expect("large UTF-8 detail appends");
        for index in 0..(MAX_OPERATION_LOG_ENTRIES + 30) {
            manager
                .append_log_detail(&id, OperationLogKind::Stdout, format!("line {index}"))
                .expect("bounded log append succeeds");
        }

        let running = manager.get(&id).expect("operation exists");
        assert!(running.logs.len() < MAX_OPERATION_LOG_ENTRIES);
        assert!(running
            .logs
            .iter()
            .filter_map(|entry| entry.detail.as_ref())
            .all(|detail| detail.len() <= MAX_OPERATION_LOG_DETAIL_BYTES));
        assert_eq!(
            running
                .logs
                .last()
                .and_then(|entry| entry.message_key.as_deref()),
            Some(log_message::TRUNCATED)
        );

        manager.finish(&id, Ok(())).expect("finish succeeds");
        let done = manager.get(&id).expect("operation exists");
        assert!(done.logs.len() <= MAX_OPERATION_LOG_ENTRIES);
        assert_eq!(
            done.logs
                .last()
                .and_then(|entry| entry.message_key.as_deref()),
            Some(log_message::SUCCEEDED)
        );
    }

    #[test]
    fn unknown_ids_and_double_finish_are_rejected() {
        let (manager, _events) = manager();
        let unknown = crate::domain::OperationId("missing".to_string());
        let progress_error = manager
            .update_progress(&unknown, 10, None)
            .expect_err("unknown id must fail");
        assert_eq!(progress_error.code, ErrorCode::Internal);
        assert_eq!(progress_error.message_key, "error.operation.notFound");
        assert_eq!(
            manager
                .finish(&unknown, Ok(()))
                .expect_err("unknown id must fail")
                .code,
            ErrorCode::Internal
        );

        let id = manager
            .begin(OperationKind::Install, Some(ToolId::Codex))
            .expect("install begins");
        manager.finish(&id, Ok(())).expect("first finish succeeds");
        let error = manager
            .finish(&id, Ok(()))
            .expect_err("second finish fails");
        assert_eq!(error.code, ErrorCode::OperationConflict);
        assert_eq!(error.message_key, "error.operation.invalidTransition");
    }

    #[test]
    fn progress_updates_are_rejected_after_the_operation_finished() {
        let (manager, events) = manager();
        let id = manager
            .begin(OperationKind::Install, Some(ToolId::Codex))
            .expect("install begins");
        manager.finish(&id, Ok(())).expect("finish succeeds");
        let before = events.statuses().len();

        let error = manager
            .update_progress(&id, 50, Some("operation.phase.installing".to_string()))
            .expect_err("a finished operation must not accept progress");
        assert_eq!(error.code, ErrorCode::OperationConflict);
        assert_eq!(error.message_key, "error.operation.notRunning");
        assert_eq!(
            events.statuses().len(),
            before,
            "a rejected progress update must not emit an event"
        );
        let stored = manager.list();
        assert_eq!(stored[0].progress, 100);
        assert_eq!(stored[0].message_key, None);
    }

    #[test]
    fn a_poisoned_tool_lock_still_emits_the_terminal_event_and_releases_the_lock() {
        let (manager, events) = manager();
        let id = manager
            .begin(OperationKind::Install, Some(ToolId::Codex))
            .expect("install begins");

        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = manager.tool_locks.lock().expect("acquire tool locks");
            panic!("poison the tool lock");
        }));
        assert!(poisoned.is_err(), "the helper closure must have panicked");
        assert!(manager.tool_locks.is_poisoned());

        manager
            .finish(&id, Ok(()))
            .expect("finish must survive a poisoned tool lock");
        assert_eq!(
            events.statuses().last().copied(),
            Some(OperationStatus::Success),
            "the terminal event must still reach the UI"
        );
        manager
            .begin(OperationKind::Uninstall, Some(ToolId::Codex))
            .expect("the tool lock must have been released");
    }

    #[test]
    fn terminal_operations_are_pruned_beyond_the_retention_cap() {
        let (manager, _events) = manager();
        for _ in 0..(super::MAX_RETAINED_OPERATIONS + 50) {
            let id = manager
                .begin(OperationKind::Scan, None)
                .expect("scan begins");
            manager.finish(&id, Ok(())).expect("scan finishes");
        }
        assert_eq!(manager.list().len(), super::MAX_RETAINED_OPERATIONS);
    }

    #[test]
    fn get_returns_the_current_snapshot_or_none_for_an_unknown_id() {
        let (manager, _events) = manager();
        let id = manager
            .begin(OperationKind::Install, Some(ToolId::Codex))
            .expect("install begins");

        let running = manager.get(&id).expect("operation exists");
        assert_eq!(running.status, OperationStatus::Running);

        manager.finish(&id, Ok(())).expect("finish succeeds");
        let finished = manager.get(&id).expect("operation still exists");
        assert_eq!(finished.status, OperationStatus::Success);

        let unknown = crate::domain::OperationId("missing".to_string());
        assert!(manager.get(&unknown).is_none());
    }

    #[test]
    fn unfinished_operations_are_never_pruned() {
        let (manager, _events) = manager();
        let total = super::MAX_RETAINED_OPERATIONS + 50;
        for _ in 0..total {
            manager
                .begin(OperationKind::Scan, None)
                .expect("scan begins");
        }
        let operations = manager.list();
        assert_eq!(operations.len(), total);
        assert!(operations
            .iter()
            .all(|operation| operation.status == OperationStatus::Running));
    }
}
