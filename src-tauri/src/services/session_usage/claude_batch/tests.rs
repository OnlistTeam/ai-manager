use super::*;
use crate::services::session_usage::get_sync_state;

fn message(index: usize) -> ParsedAssistantUsage {
    ParsedAssistantUsage {
        message_id: format!("batch-{index}"),
        model: "claude-sonnet-4-5".into(),
        input_tokens: 100,
        output_tokens: 0,
        cache_read_tokens: 10,
        cache_creation_tokens: 0,
        stop_reason: None,
        timestamp: Some("2026-09-20T00:00:00Z".into()),
        session_id: Some("synthetic-session".into()),
    }
}

fn count(db: &Database) -> i64 {
    db.conn
        .lock()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM proxy_request_logs", [], |row| {
            row.get(0)
        })
        .unwrap()
}

#[test]
fn multiple_batches_preserve_billable_tokens_and_deduplicate_replays() {
    let db = Database::memory().unwrap();
    let messages: Vec<_> = (0..600).map(message).collect();
    assert_eq!(
        persist(&db, "synthetic.jsonl", 42, 600, messages.iter()).unwrap(),
        (600, 0)
    );
    assert_eq!(count(&db), 600);
    assert_eq!(get_sync_state(&db, "synthetic.jsonl").unwrap(), (42, 600));
    assert_eq!(
        persist(&db, "synthetic.jsonl", 43, 600, messages.iter()).unwrap(),
        (0, 600)
    );
    assert_eq!(count(&db), 600);
}

#[test]
fn failed_later_batch_does_not_advance_cursor_and_retry_recovers() {
    let db = Database::memory().unwrap();
    let messages: Vec<_> = (0..260).map(message).collect();
    db.conn
        .lock()
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER fail_batch BEFORE INSERT ON proxy_request_logs
        WHEN NEW.request_id = 'session:batch-258'
        BEGIN SELECT RAISE(ABORT, 'synthetic insert failure'); END;",
        )
        .unwrap();
    assert!(persist(&db, "synthetic.jsonl", 42, 260, messages.iter()).is_err());
    assert_eq!(count(&db), 256);
    assert_eq!(get_sync_state(&db, "synthetic.jsonl").unwrap(), (0, 0));
    db.conn
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_batch;")
        .unwrap();
    assert_eq!(
        persist(&db, "synthetic.jsonl", 42, 260, messages.iter()).unwrap(),
        (4, 256)
    );
    assert_eq!(count(&db), 260);
    assert_eq!(get_sync_state(&db, "synthetic.jsonl").unwrap(), (42, 260));
}

#[test]
fn cursor_write_failure_rolls_back_final_batch() {
    let db = Database::memory().unwrap();
    let messages = [message(1), message(2)];
    db.conn
        .lock()
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER fail_cursor BEFORE INSERT ON session_log_sync
        BEGIN SELECT RAISE(ABORT, 'synthetic cursor failure'); END;",
        )
        .unwrap();
    assert!(persist(&db, "synthetic.jsonl", 42, 2, messages.iter()).is_err());
    assert_eq!(count(&db), 0);
    assert_eq!(get_sync_state(&db, "synthetic.jsonl").unwrap(), (0, 0));
}

#[test]
fn empty_usage_advances_cursor_without_fabricating_records() {
    let db = Database::memory().unwrap();
    let mut empty = message(0);
    empty.input_tokens = 0;
    empty.cache_read_tokens = 0;
    assert_eq!(
        persist(&db, "synthetic.jsonl", 42, 1, [&empty].into_iter()).unwrap(),
        (0, 0)
    );
    assert_eq!(count(&db), 0);
    assert_eq!(get_sync_state(&db, "synthetic.jsonl").unwrap(), (42, 1));
}
