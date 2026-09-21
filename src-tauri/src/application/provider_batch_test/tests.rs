use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use tokio::sync::Semaphore;

use super::{
    ProviderBatchTestRequest, ProviderBatchTestService, ProviderTester,
    PROVIDER_TEST_ALL_CONCURRENCY,
};
use crate::domain::{
    AppError, ErrorCode, Operation, OperationOutput, OperationStatus, ProviderReachability,
    ProviderTestResult, ToolId, MAX_PROVIDER_TEST_ALL_RESULTS,
};
use crate::infrastructure::{OperationEvents, OperationManager};

#[derive(Default)]
struct RecordingEvents(std::sync::Mutex<Vec<Operation>>);

impl OperationEvents for Arc<RecordingEvents> {
    fn emit(&self, operation: &Operation) {
        self.0.lock().expect("events lock").push(operation.clone());
    }
}

fn manager() -> (Arc<OperationManager>, Arc<RecordingEvents>) {
    let events = Arc::new(RecordingEvents::default());
    (
        Arc::new(OperationManager::new(Box::new(events.clone()))),
        events,
    )
}

fn result(id: String) -> ProviderTestResult {
    ProviderTestResult {
        provider_id: id,
        reachability: ProviderReachability::Operational,
        response_time_ms: Some(40),
        http_status: Some(401),
    }
}

fn request(count: usize) -> ProviderBatchTestRequest {
    ProviderBatchTestRequest {
        tool: ToolId::ClaudeCode,
        provider_ids: (0..count)
            .map(|index| format!("provider-{index:03}"))
            .collect(),
    }
}

#[tokio::test]
async fn checks_are_bounded_concurrent_and_emit_replayable_partial_results() {
    let (operations, events) = manager();
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(Semaphore::new(0));
    let tester: ProviderTester = Arc::new({
        let active = active.clone();
        let maximum = maximum.clone();
        let gate = gate.clone();
        move |_tool, id| {
            let active = active.clone();
            let maximum = maximum.clone();
            let gate = gate.clone();
            Box::pin(async move {
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(now, Ordering::SeqCst);
                let permit = gate.acquire_owned().await.expect("test gate");
                drop(permit);
                active.fetch_sub(1, Ordering::SeqCst);
                Ok(result(id))
            }) as BoxFuture<'static, _>
        }
    });
    let service = ProviderBatchTestService::new(operations.clone(), tester);
    let request = request(20);
    let id = service.begin(&request).expect("batch begins");
    let task = tokio::spawn(async move { service.run(id, request).await });

    tokio::time::timeout(Duration::from_secs(2), async {
        while maximum.load(Ordering::SeqCst) < PROVIDER_TEST_ALL_CONCURRENCY {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the first bounded wave starts");
    assert_eq!(
        maximum.load(Ordering::SeqCst),
        PROVIDER_TEST_ALL_CONCURRENCY
    );
    gate.add_permits(20);
    task.await.expect("batch task joins");

    let operation = operations.list().pop().expect("operation remains");
    assert_eq!(operation.status, OperationStatus::Success);
    assert_eq!(operation.progress, 100);
    let Some(OperationOutput::ProviderTests { results }) = operation.output else {
        panic!("typed provider results are retained");
    };
    assert_eq!(results.len(), 20);
    assert_eq!(results[0].provider_id, "provider-000");
    assert_eq!(results[19].provider_id, "provider-019");

    let snapshots = events.0.lock().expect("events");
    let running_progress = snapshots
        .iter()
        .filter(|snapshot| snapshot.status == OperationStatus::Running)
        .map(|snapshot| snapshot.progress)
        .collect::<Vec<_>>();
    assert!(running_progress.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(running_progress.iter().all(|progress| *progress < 100));
    assert!(snapshots.iter().any(|snapshot| {
        matches!(
            snapshot.output,
            Some(OperationOutput::ProviderTests { ref results }) if !results.is_empty()
        )
    }));
}

#[tokio::test]
async fn one_internal_probe_error_does_not_hide_other_completed_results() {
    let (operations, _events) = manager();
    let tester: ProviderTester = Arc::new(|_tool, id| {
        Box::pin(async move {
            if id == "provider-001" {
                return Err(
                    AppError::new(ErrorCode::NetworkError, "error.provider.testFailed")
                        .with_technical("client construction failed"),
                );
            }
            Ok(result(id))
        })
    });
    let service = ProviderBatchTestService::new(operations.clone(), tester);
    let request = request(3);
    let id = service.begin(&request).expect("batch begins");
    service.run(id.clone(), request).await;

    let operation = operations.get(&id).expect("operation remains");
    assert_eq!(operation.status, OperationStatus::Failed);
    assert_eq!(
        operation.error.as_ref().map(|error| error.code),
        Some(ErrorCode::NetworkError)
    );
    assert_eq!(
        operation
            .error
            .as_ref()
            .and_then(|error| error.context_id.as_deref()),
        Some(id.as_str())
    );
    let Some(OperationOutput::ProviderTests { results }) = operation.output else {
        panic!("partial output remains available on failure");
    };
    assert_eq!(results.len(), 2);
    assert!(results
        .iter()
        .all(|result| result.provider_id != "provider-001"));
}

#[tokio::test]
async fn a_probe_panic_becomes_a_terminal_error_instead_of_a_stuck_task() {
    let (operations, _events) = manager();
    let tester: ProviderTester = Arc::new(|_tool, _id| {
        Box::pin(async move {
            panic!("token=must-not-survive");
            #[allow(unreachable_code)]
            Ok(result(String::new()))
        })
    });
    let service = ProviderBatchTestService::new(operations.clone(), tester);
    let request = request(1);
    let id = service.begin(&request).expect("batch begins");
    service.run(id.clone(), request).await;

    let operation = operations.get(&id).expect("operation remains");
    assert_eq!(operation.status, OperationStatus::Failed);
    let technical = operation
        .error
        .and_then(|error| error.technical_message)
        .unwrap_or_default();
    assert!(!technical.contains("must-not-survive"));
}

#[test]
fn begin_rejects_empty_duplicate_and_unbounded_native_target_sets() {
    let (operations, _events) = manager();
    let tester: ProviderTester = Arc::new(|_tool, id| Box::pin(async move { Ok(result(id)) }));
    let service = ProviderBatchTestService::new(operations, tester);

    let empty = request(0);
    assert_eq!(
        service.begin(&empty).expect_err("empty batches fail").code,
        ErrorCode::ProviderUnreachable
    );
    let duplicate = ProviderBatchTestRequest {
        tool: ToolId::Codex,
        provider_ids: vec!["same".to_string(), "same".to_string()],
    };
    assert_eq!(
        service.begin(&duplicate).expect_err("duplicates fail").code,
        ErrorCode::ProviderNotFound
    );
    let too_many = request(MAX_PROVIDER_TEST_ALL_RESULTS + 1);
    assert_eq!(
        service
            .begin(&too_many)
            .expect_err("unbounded batches fail")
            .code,
        ErrorCode::ProviderUnreachable
    );
}

#[tokio::test]
async fn mismatched_results_fail_closed_and_are_not_published() {
    let (operations, _events) = manager();
    let tester: ProviderTester = Arc::new(|_tool, _id| {
        Box::pin(async move { Ok(result("different-provider".to_string())) })
    });
    let service = ProviderBatchTestService::new(operations.clone(), tester);
    let request = request(1);
    let id = service.begin(&request).expect("batch begins");
    service.run(id.clone(), request).await;

    let operation = operations.get(&id).expect("operation remains");
    assert_eq!(operation.status, OperationStatus::Failed);
    assert!(matches!(
        operation.output,
        Some(OperationOutput::ProviderTests { ref results }) if results.is_empty()
    ));
}
