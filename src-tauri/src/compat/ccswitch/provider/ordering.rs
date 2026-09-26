//! The user's own order of saved services.
//!
//! Upstream keeps it in `providers.sort_index` per app type and lists by it
//! (`ORDER BY COALESCE(sort_index, 999999), created_at, id`); its own renderer
//! writes the whole list back after a drag. This wrapper does the same through
//! the upstream `update_sort_order`, so switching the current service never
//! moves a card and the routing failover queue keeps following the same order.

use std::collections::HashSet;

use crate::domain::{AppError, ErrorCode, Provider, ToolId};
use crate::services::provider::ProviderSortUpdate;
use crate::services::ProviderService;

use super::ProviderStore;
use super::{app_type_for, is_import_placeholder, provider_from_upstream, upstream_detail};

/// The message already says the list went back and to try again; the
/// renderer shows it as a toast, where a "View Details" hint has nowhere to go.
fn reorder_failed() -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.provider.reorderFailed")
}

/// `ordered` must name exactly the services the list shows, each once. Anything
/// else means the renderer is looking at a stale list, and writing a partial
/// order would silently shuffle rows the user never saw.
pub(super) fn reorder(
    store: &ProviderStore,
    tool: ToolId,
    ordered: &[String],
) -> Result<Vec<Provider>, AppError> {
    let (rows, current) = store.raw_inventory(tool)?;
    let visible: Vec<&str> = rows
        .values()
        .filter(|raw| !is_import_placeholder(&provider_from_upstream(tool, raw, &current)))
        .map(|raw| raw.id.as_str())
        .collect();

    let requested: HashSet<&str> = ordered.iter().map(String::as_str).collect();
    let shown: HashSet<&str> = visible.iter().copied().collect();
    if requested.len() != ordered.len() || requested != shown {
        return Err(reorder_failed().with_technical(format!(
            "{}: the requested order does not match the saved services",
            tool.as_str()
        )));
    }

    // The hidden import placeholder keeps a slot after everything the user sees.
    let full = ordered.iter().map(String::as_str).chain(
        rows.keys()
            .map(String::as_str)
            .filter(|id| !shown.contains(id)),
    );
    let updates: Vec<ProviderSortUpdate> = full
        .enumerate()
        .filter(|(index, id)| rows.get(*id).and_then(|raw| raw.sort_index) != Some(*index))
        .map(|(sort_index, id)| ProviderSortUpdate {
            id: id.to_string(),
            sort_index,
        })
        .collect();

    if !updates.is_empty() {
        ProviderService::update_sort_order(&store.state, app_type_for(tool), updates)
            .map_err(|error| reorder_failed().with_technical(upstream_detail(&error)))?;
    }
    store.list(tool)
}
