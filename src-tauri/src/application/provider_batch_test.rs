//! Bounded, task-centre-backed checks for every testable saved service of one tool.

use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use futures::future::{BoxFuture, FutureExt};
use futures::stream::{self, StreamExt};

use crate::application::provider_directory::ProviderDirectory;
use crate::domain::operation::phase;
use crate::domain::{
    AppError, ErrorCode, OperationId, OperationKind, OperationOutput, OperationStatus,
    ProviderTestResult, ToolId, MAX_PROVIDER_TEST_ALL_RESULTS,
};
use crate::infrastructure::OperationManager;
use crate::platform::redact::{redact_secrets, truncate_tail};

pub const PROVIDER_TEST_ALL_CONCURRENCY: usize = crate::domain::PROVIDER_TEST_ALL_CONCURRENCY;

pub type ProviderTester = Arc<
    dyn Fn(ToolId, String) -> BoxFuture<'static, Result<ProviderTestResult, AppError>>
        + Send
        + Sync,
>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderBatchTestRequest {
    pub tool: ToolId,
    pub provider_ids: Vec<String>,
}

pub struct ProviderBatchTestService {
    operations: Arc<OperationManager>,
    tester: ProviderTester,
}

impl ProviderBatchTestService {
    pub fn new(operations: Arc<OperationManager>, tester: ProviderTester) -> Self {
        Self { operations, tester }
    }

    pub fn system(app_handle: tauri::AppHandle, operations: Arc<OperationManager>) -> Self {
        let tester: ProviderTester = Arc::new(move |tool, provider_id| {
            let app_handle = app_handle.clone();
            Box::pin(async move { ProviderDirectory::test(&app_handle, tool, &provider_id).await })
        });
        Self::new(operations, tester)
    }

    /// Resolves all targets from the native provider inventory. The renderer
    /// supplies neither provider ids nor probe URLs.
    pub fn resolve(
        app_handle: &tauri::AppHandle,
        tool: ToolId,
    ) -> Result<ProviderBatchTestRequest, AppError> {
        let mut provider_ids = ProviderDirectory::list(app_handle, tool)?
            .into_iter()
            .filter(|provider| provider.testable)
            .map(|provider| provider.id)
            .collect::<Vec<_>>();
        provider_ids.sort();
        provider_ids.dedup();
        let request = ProviderBatchTestRequest { tool, provider_ids };
        validate_request(&request)?;
        Ok(request)
    }

    pub fn begin(&self, request: &ProviderBatchTestRequest) -> Result<OperationId, AppError> {
        validate_request(request)?;
        let id = self.operations.begin_provider_tests(request.tool)?;
        let _ = self
            .operations
            .update_progress(&id, 5, Some(phase::PREPARING.to_string()));
        Ok(id)
    }

    pub async fn run(&self, id: OperationId, request: ProviderBatchTestRequest) {
        if let Err(reason) = self.verify_pairing(&id, &request) {
            log::error!(
                "refusing to run provider test operation {}: {reason}",
                id.as_str()
            );
            return;
        }

        let outcome = AssertUnwindSafe(self.execute(&id, request))
            .catch_unwind()
            .await
            .unwrap_or_else(|payload| {
                Err(
                    AppError::new(ErrorCode::Internal, "error.provider.testFailed")
                        .with_technical(panic_summary(payload))
                        .with_remediation("error.remediation.retryOrViewDetails")
                        .with_context_id(id.as_str()),
                )
            });

        if outcome.is_ok() {
            let _ = self
                .operations
                .update_progress(&id, 99, Some(phase::READY.to_string()));
        }
        if let Err(error) = self.operations.finish(&id, outcome.map(|_| ())) {
            log::warn!("failed to finish provider test operation: {error}");
        }
    }

    async fn execute(
        &self,
        id: &OperationId,
        request: ProviderBatchTestRequest,
    ) -> Result<Vec<ProviderTestResult>, AppError> {
        let total = request.provider_ids.len();
        let completed = Arc::new(AtomicUsize::new(0));
        let id = id.clone();
        let tool = request.tool;

        let mut outcomes = stream::iter(request.provider_ids.into_iter().enumerate().map(
            |(index, provider_id)| {
                let tester = self.tester.clone();
                let operations = self.operations.clone();
                let completed = completed.clone();
                let operation_id = id.clone();
                async move {
                    let expected_id = provider_id.clone();
                    let outcome =
                        AssertUnwindSafe(async move { (tester)(tool, provider_id).await })
                            .catch_unwind()
                            .await
                            .unwrap_or_else(|payload| {
                                Err(
                                    AppError::new(ErrorCode::Internal, "error.provider.testFailed")
                                        .with_technical(panic_summary(payload))
                                        .with_remediation("error.remediation.retryOrViewDetails"),
                                )
                            })
                            .and_then(|result| verify_result(&expected_id, result));

                    let done = completed.fetch_add(1, Ordering::SeqCst) + 1;
                    let progress = 5 + ((done * 90) / total) as u8;
                    let recorded = operations.record_provider_test(
                        &operation_id,
                        progress,
                        outcome.as_ref().ok().cloned(),
                    );
                    (index, outcome, recorded)
                }
            },
        ))
        .buffer_unordered(PROVIDER_TEST_ALL_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
        outcomes.sort_by_key(|(index, _, _)| *index);

        let mut results = Vec::with_capacity(total);
        let mut first_error = None;
        for (_, outcome, recorded) in outcomes {
            if let Err(error) = recorded {
                first_error.get_or_insert(error);
            }
            match outcome {
                Ok(result) => results.push(result),
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        if let Some(mut error) = first_error {
            if error.context_id.is_none() {
                error.context_id = Some(id.as_str().to_string());
            }
            return Err(error);
        }
        Ok(results)
    }

    fn verify_pairing(
        &self,
        id: &OperationId,
        request: &ProviderBatchTestRequest,
    ) -> Result<(), String> {
        validate_request(request).map_err(|error| error.to_string())?;
        let operation = self
            .operations
            .get(id)
            .ok_or_else(|| "no such operation".to_string())?;
        let output_matches = matches!(
            operation.output,
            Some(OperationOutput::ProviderTests { ref results }) if results.is_empty()
        );
        if operation.status != OperationStatus::Running
            || operation.kind != OperationKind::TestProviders
            || operation.tool != Some(request.tool)
            || !output_matches
        {
            return Err("operation and provider test request do not match".to_string());
        }
        Ok(())
    }
}

fn validate_request(request: &ProviderBatchTestRequest) -> Result<(), AppError> {
    if request.provider_ids.is_empty() {
        return Err(
            AppError::new(ErrorCode::ProviderUnreachable, "error.provider.notTestable")
                .with_remediation("error.remediation.checkServiceSettings"),
        );
    }
    if request.provider_ids.len() > MAX_PROVIDER_TEST_ALL_RESULTS {
        return Err(
            AppError::new(ErrorCode::ProviderUnreachable, "error.provider.testFailed")
                .with_technical("testable provider limit exceeded")
                .with_remediation("error.remediation.retryOrViewDetails"),
        );
    }
    if request.provider_ids.iter().any(|id| id.trim().is_empty()) {
        return Err(
            AppError::new(ErrorCode::ProviderNotFound, "error.provider.notFound")
                .with_technical("resolved provider id is empty"),
        );
    }
    let mut unique = request.provider_ids.clone();
    unique.sort();
    unique.dedup();
    if unique.len() != request.provider_ids.len() {
        return Err(
            AppError::new(ErrorCode::ProviderNotFound, "error.provider.notFound")
                .with_technical("resolved provider ids are not unique"),
        );
    }
    Ok(())
}

fn verify_result(
    expected_id: &str,
    result: ProviderTestResult,
) -> Result<ProviderTestResult, AppError> {
    if result.provider_id == expected_id {
        return Ok(result);
    }
    Err(
        AppError::new(ErrorCode::Internal, "error.provider.testFailed")
            .with_technical("provider test returned a result for a different saved service")
            .with_remediation("error.remediation.retryOrViewDetails"),
    )
}

fn panic_summary(payload: Box<dyn Any + Send>) -> String {
    let raw = if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "provider test panicked with a non-string payload".to_string()
    };
    truncate_tail(&redact_secrets(&raw), 8, 512)
}

#[cfg(test)]
#[path = "provider_batch_test/tests.rs"]
mod tests;
