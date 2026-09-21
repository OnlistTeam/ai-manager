use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use crate::domain::{AppError, ErrorCode};

const ACTIVE: u8 = 0;
const REQUESTED: u8 = 1;
const CONFIRMED: u8 = 2;
const FAILED: u8 = 3;

/// One-shot cooperative cancellation shared by an Operation and the process
/// executor. A request is not a completed cancellation: only the executor may
/// confirm it after proving that no child was started or the child tree exited.
#[derive(Clone, Debug, Default)]
pub struct CommandCancellation {
    state: Arc<AtomicU8>,
}

impl CommandCancellation {
    pub fn request(&self) -> bool {
        self.state
            .compare_exchange(ACTIVE, REQUESTED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn is_requested(&self) -> bool {
        self.state.load(Ordering::Acquire) == REQUESTED
    }

    pub fn is_confirmed(&self) -> bool {
        self.state.load(Ordering::Acquire) == CONFIRMED
    }

    pub fn is_failed(&self) -> bool {
        self.state.load(Ordering::Acquire) == FAILED
    }

    /// Safe checkpoints before a child/mutation starts may confirm directly.
    pub fn confirm_if_requested(&self) -> bool {
        self.state
            .compare_exchange(REQUESTED, CONFIRMED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// A mutation that runs without a child process (path removal, launcher
    /// staging) has no executor in front of it to honour a pending request.
    /// Callers place this immediately before such a mutation: a pending request
    /// is confirmed here and the mutation never starts.
    pub fn checkpoint(&self) -> Result<(), AppError> {
        if self.confirm_if_requested() {
            return Err(cancelled_error());
        }
        Ok(())
    }

    pub(super) fn fail_if_requested(&self) -> bool {
        self.state
            .compare_exchange(REQUESTED, FAILED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

pub(super) fn cancelled_error() -> AppError {
    AppError::new(ErrorCode::Internal, "error.operation.cancelled")
}

pub(super) fn cancellation_failed_error() -> AppError {
    AppError::new(ErrorCode::Internal, "error.operation.cancelFailed")
        .with_remediation("error.remediation.retryOrViewDetails")
}

#[cfg(test)]
mod tests {
    use super::CommandCancellation;

    #[test]
    fn cancellation_is_one_shot_and_confirmation_is_explicit() {
        let cancellation = CommandCancellation::default();
        assert!(cancellation.request());
        assert!(!cancellation.request());
        assert!(cancellation.is_requested());
        assert!(!cancellation.is_confirmed());
        assert!(cancellation.confirm_if_requested());
        assert!(cancellation.is_confirmed());
        assert!(!cancellation.confirm_if_requested());
        assert!(!cancellation.fail_if_requested());
    }

    #[test]
    fn a_checkpoint_confirms_a_pending_request_and_is_a_no_op_otherwise() {
        let cancellation = CommandCancellation::default();
        assert!(cancellation.checkpoint().is_ok());
        assert!(cancellation.request());
        let error = cancellation.checkpoint().expect_err("pending request");
        assert_eq!(error.message_key, "error.operation.cancelled");
        assert!(cancellation.is_confirmed());
        // Already confirmed: nothing left to confirm, and no second error.
        assert!(cancellation.checkpoint().is_ok());
    }

    #[test]
    fn a_failed_termination_cannot_be_reported_as_cancelled() {
        let cancellation = CommandCancellation::default();
        assert!(cancellation.request());
        assert!(cancellation.fail_if_requested());
        assert!(cancellation.is_failed());
        assert!(!cancellation.is_confirmed());
    }
}
