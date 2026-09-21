use super::{
    AdapterResolver, LifecycleRequest, NeverExecutor, RecordingEvents, SharedAdapter, StubAdapter,
    ToolLifecycleService,
};
use crate::adapters::ToolAdapter;
use crate::application::tool_version_history::ToolVersionHistoryService;
use crate::domain::operation::phase;
use crate::domain::{OperationKind, OperationStatus, ToolId, ToolStatus};
use crate::infrastructure::OperationManager;
use std::sync::{Arc, Mutex};

type CancellationHarness = (
    ToolLifecycleService,
    Arc<OperationManager>,
    Arc<RecordingEvents>,
    Arc<Mutex<Vec<String>>>,
);

fn harness() -> CancellationHarness {
    let events = Arc::new(RecordingEvents::default());
    let operations = Arc::new(OperationManager::new(Box::new(events.clone())));
    let adapter = Arc::new(StubAdapter::new(ToolId::Codex, ToolStatus::Installed));
    let calls = adapter.calls.clone();
    let resolver: AdapterResolver = Arc::new(move |id| {
        (id == ToolId::Codex)
            .then(|| Box::new(SharedAdapter(adapter.clone())) as Box<dyn ToolAdapter>)
    });
    (
        ToolLifecycleService::new(
            operations.clone(),
            Arc::new(NeverExecutor),
            resolver,
            ToolVersionHistoryService::discarding(),
        ),
        operations,
        events,
        calls,
    )
}

#[tokio::test]
async fn cancelling_during_preparation_prevents_the_action_and_releases_after_confirmation() {
    let (service, operations, _events, calls) = harness();
    let request = LifecycleRequest::Install(ToolId::Codex);
    let id = service.begin(&request).expect("install begins");
    assert!(operations.get(&id).unwrap().can_cancel);
    operations
        .request_cancel(&id)
        .expect("preparing is safely cancellable");

    service.run(id.clone(), request).await;

    assert!(
        calls.lock().unwrap().is_empty(),
        "adapter action never starts"
    );
    let done = operations.get(&id).expect("operation remains in history");
    assert_eq!(done.status, OperationStatus::Cancelled);
    assert_eq!(done.message_key.as_deref(), Some(phase::CANCELLED));
    operations
        .begin(OperationKind::Uninstall, Some(ToolId::Codex))
        .expect("confirmed cancellation released the tool lock");
}

#[tokio::test]
async fn the_irreversible_phase_closes_the_cancel_gate_before_adapter_work() {
    let (service, operations, events, calls) = harness();
    let request = LifecycleRequest::Install(ToolId::Codex);
    let id = service.begin(&request).expect("install begins");

    service.run(id.clone(), request).await;

    assert_eq!(calls.lock().unwrap().as_slice(), ["install"]);
    let seen = events.seen.lock().unwrap();
    let installing = seen
        .iter()
        .find(|operation| operation.message_key.as_deref() == Some(phase::INSTALLING))
        .expect("adapter reported its irreversible phase");
    assert!(!installing.can_cancel);
    assert_eq!(seen.last().unwrap().status, OperationStatus::Success);
    assert!(!operations.get(&id).unwrap().can_cancel);
}
