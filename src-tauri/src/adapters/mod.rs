pub mod lifecycle_runner;
pub mod model_probe;
mod native_supply;
pub mod registry;
pub mod tool_adapter;
mod uninstall_runner;
mod version_catalog_runner;

pub use registry::{AdapterRegistry, UpstreamToolAdapter};
pub use tool_adapter::{
    AuthorizedUpdate, LaunchContext, LifecycleContext, LifecycleNetworkPolicy, ProgressReporter,
    ToolAdapter, VersionQueryContext,
};
