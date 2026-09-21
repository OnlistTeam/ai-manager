//! Proxy server module
//!
//! Provides a local HTTP proxy with multi-provider failover and request pass-through

pub mod body_filter;
pub mod cache_injector;
pub mod circuit_breaker;
pub(crate) mod content_encoding;
pub mod copilot_optimizer;
pub mod error;
pub mod error_mapper;
pub(crate) mod failover_switch;
mod forwarder;
pub mod gemini_url;
pub mod handler_config;
pub mod handler_context;
mod handlers;
pub mod http_client;
pub mod hyper_client;
pub(crate) mod json_canonical;
pub mod log_codes;
pub mod media_sanitizer;
pub mod model_mapper;
pub mod provider_router;
pub mod providers;
pub mod response_processor;
pub(crate) mod server;
pub mod session;
pub(crate) mod sse;
pub(crate) mod switch_lock;
pub mod thinking_budget_rectifier;
pub mod thinking_optimizer;
pub mod thinking_rectifier;
pub(crate) mod tool_media;
pub(crate) mod types;
pub mod usage;

// Public re-exports for external use (needed by the commands and services modules)
pub use circuit_breaker::CircuitBreakerConfig;
pub use error::ProxyError;
pub use session::extract_session_id;

// Shared across internal modules (used by submodules)
// Note: this export is for internal use; the compiler may warn it is unused even though submodules do use it
