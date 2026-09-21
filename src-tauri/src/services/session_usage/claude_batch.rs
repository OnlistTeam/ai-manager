//! Bounded transactions avoid one disk journal/fsync per imported message.
//! Keep upstream parsing, pricing and proxy/session deduplication unchanged.

use super::{
    insert_session_log_entry_on_conn, update_sync_state, update_sync_state_on_conn,
    ParsedAssistantUsage,
};
use crate::database::{lock_conn, Database};
use crate::error::AppError;
use std::collections::HashMap;

const BATCH_SIZE: usize = 256;

pub(super) fn persist<'a>(
    db: &Database,
    file_path: &str,
    modified: i64,
    offset: i64,
    messages: impl Iterator<Item = &'a ParsedAssistantUsage>,
) -> Result<(u32, u32), AppError> {
    // Input-only and cache-only snapshots are billable, even without stop_reason.
    let billable: Vec<_> = messages
        .filter(|msg| {
            msg.input_tokens > 0
                || msg.output_tokens > 0
                || msg.cache_read_tokens > 0
                || msg.cache_creation_tokens > 0
        })
        .collect();
    if billable.is_empty() {
        update_sync_state(db, file_path, modified, offset)?;
        return Ok((0, 0));
    }
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let mut pricing = HashMap::new();
    let batch_count = billable.len().div_ceil(BATCH_SIZE);
    for (index, batch) in billable.chunks(BATCH_SIZE).enumerate() {
        let conn = lock_conn!(db.conn);
        let tx = conn.unchecked_transaction()?;
        for msg in batch {
            let request_id = format!(
                "{}{}",
                crate::proxy::usage::parser::SESSION_REQUEST_ID_PREFIX,
                msg.message_id
            );
            if insert_session_log_entry_on_conn(&tx, &request_id, msg, &mut pricing)? {
                imported = imported.saturating_add(1);
            } else {
                skipped = skipped.saturating_add(1);
            }
        }
        // A failed insert/commit must not move the cursor past missing records.
        // Earlier committed batches replay safely through the existing dedup key.
        if index + 1 == batch_count {
            update_sync_state_on_conn(&tx, file_path, modified, offset)?;
        }
        tx.commit()?;
    }
    Ok((imported, skipped))
}

#[cfg(test)]
mod tests;
