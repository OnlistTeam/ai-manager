//! Data Access Object layer
//!
//! Database access operations for each domain

pub mod failover;
pub mod mcp;
pub mod profiles;
pub mod prompts;
pub mod providers;
pub mod providers_seed;
pub mod proxy;
pub mod settings;
pub mod skills;
pub mod stream_check;
pub mod universal_providers;
pub mod usage_rollup;

// All DAO methods are exposed through the Database impl; no separate exports needed
// Re-export FailoverQueueItem / Profile for external use
pub use profiles::Profile;
