//! Log error code definitions for the proxy module
//!
//! Format: [module-number] message
//! - CB: Circuit Breaker
//! - SRV: Server
//! - FWD: Forwarder
//! - FO: Failover
//! - RSP: Response (response handling)
//! - USG: Usage

/// Circuit breaker log codes
pub mod cb {
    pub const OPEN_TO_HALF_OPEN: &str = "CB-001";
    pub const HALF_OPEN_TO_CLOSED: &str = "CB-002";
    pub const HALF_OPEN_PROBE_FAILED: &str = "CB-003";
    pub const TRIGGERED_FAILURES: &str = "CB-004";
    pub const TRIGGERED_ERROR_RATE: &str = "CB-005";
    pub const MANUAL_RESET: &str = "CB-006";
}

/// Server log codes
pub mod srv {
    pub const STARTED: &str = "SRV-001";
    pub const STOPPED: &str = "SRV-002";
    pub const STOP_TIMEOUT: &str = "SRV-003";
    pub const TASK_ERROR: &str = "SRV-004";
    pub const ACCEPT_ERR: &str = "SRV-005";
    pub const CONN_ERR: &str = "SRV-006";
}

/// Forwarder log codes
pub mod fwd {
    pub const PROVIDER_FAILED_RETRY: &str = "FWD-001";
    pub const ALL_PROVIDERS_FAILED: &str = "FWD-002";
    pub const SINGLE_PROVIDER_FAILED: &str = "FWD-003";
}

/// Failover log codes
pub mod fo {}

/// Response handling log codes
pub mod rsp {}

/// Usage log codes
pub mod usg {}
