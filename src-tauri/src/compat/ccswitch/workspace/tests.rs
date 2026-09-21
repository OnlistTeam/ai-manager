use std::fs;

use super::{WorkspaceStore, MAX_WORKSPACE_CONTENT_BYTES};
use crate::domain::{
    OpenClawWorkspaceDirectory, OpenClawWorkspaceFileId, OpenClawWorkspaceFileStatus,
};

#[test]
fn overview_projects_only_fixed_files_and_valid_daily_memories() {
    let temp = tempfile::tempdir().expect("temp workspace");
    let store = WorkspaceStore::at(temp.path().to_path_buf());
    fs::write(temp.path().join("AGENTS.md"), "# Agents").expect("seed agents");
    fs::create_dir_all(temp.path().join("memory")).expect("memory directory");
    fs::write(temp.path().join("memory/2026-08-26.md"), "Today").expect("seed memory");
    fs::write(temp.path().join("memory/not-a-date.md"), "ignore").expect("seed junk");

    let overview = store.overview().expect("workspace overview");
    assert_eq!(overview.files.len(), 9);
    assert_eq!(overview.existing_files, 1);
    assert_eq!(overview.daily_memory_count, 1);
    assert_eq!(overview.daily_memory_bytes, 5);
    assert_eq!(overview.total_bytes, 13);
    assert_eq!(overview.files[0].id, OpenClawWorkspaceFileId::Agents);
    assert_eq!(overview.files[0].status, OpenClawWorkspaceFileStatus::Ready);
    assert!(!overview.limited);
}

#[test]
fn document_save_is_verified_and_keeps_a_private_recovery_copy() {
    let temp = tempfile::tempdir().expect("temp workspace");
    let store = WorkspaceStore::at(temp.path().to_path_buf());
    fs::write(temp.path().join("SOUL.md"), b"original\r\n").expect("seed soul");

    let outcome = store
        .save_document(OpenClawWorkspaceFileId::Soul, "updated\n")
        .expect("save workspace file");
    assert!(outcome.backup_created);
    assert_eq!(
        store
            .document(OpenClawWorkspaceFileId::Soul)
            .expect("read updated document")
            .content,
        "updated\n"
    );
    let copies = fs::read_dir(temp.path().join(".ai-manager-backups"))
        .expect("recovery directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("recovery copies");
    assert_eq!(copies.len(), 1);
    assert_eq!(
        fs::read(copies[0].path()).expect("recovery bytes"),
        b"original\r\n"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(temp.path().join("SOUL.md"))
                .expect("saved metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn daily_memory_search_handles_unicode_without_byte_slicing() {
    let temp = tempfile::tempdir().expect("temp workspace");
    let store = WorkspaceStore::at(temp.path().to_path_buf());
    fs::create_dir_all(temp.path().join("memory")).expect("memory directory");
    fs::write(
        temp.path().join("memory/2026-08-26.md"),
        "Первая строка\nСегодня починил поиск по памяти OpenClaw.\nКонец",
    )
    .expect("seed memory");

    let found = store.memories(Some("памяти")).expect("search memories");
    assert_eq!(found.items.len(), 1);
    assert_eq!(found.items[0].date, "2026-08-26");
    assert_eq!(found.items[0].match_count, 1);
    assert!(found.items[0].preview.contains("поиск по памяти"));
}

#[test]
fn daily_memory_delete_is_idempotent_and_recoverable() {
    let temp = tempfile::tempdir().expect("temp workspace");
    let store = WorkspaceStore::at(temp.path().to_path_buf());
    fs::create_dir_all(temp.path().join("memory")).expect("memory directory");
    let path = temp.path().join("memory/2026-08-26.md");
    fs::write(&path, "keep a copy").expect("seed memory");

    let removed = store.delete_memory("2026-08-26").expect("delete memory");
    assert!(removed.backup_created);
    assert!(!path.exists());
    let again = store.delete_memory("2026-08-26").expect("repeat delete");
    assert!(!again.backup_created);
}

#[test]
fn invalid_dates_content_and_queries_fail_before_touching_disk() {
    let temp = tempfile::tempdir().expect("temp workspace");
    let store = WorkspaceStore::at(temp.path().to_path_buf());
    assert_eq!(
        store
            .memory_document("2026-02-31")
            .expect_err("impossible date")
            .message_key,
        "error.openclawWorkspace.dateInvalid"
    );
    assert_eq!(
        store
            .save_document(
                OpenClawWorkspaceFileId::Agents,
                &"a".repeat(MAX_WORKSPACE_CONTENT_BYTES + 1),
            )
            .expect_err("oversized document")
            .message_key,
        "error.openclawWorkspace.contentTooLarge"
    );
    assert_eq!(
        store
            .memories(Some(&"q".repeat(201)))
            .expect_err("oversized query")
            .message_key,
        "error.openclawWorkspace.queryInvalid"
    );
    assert!(!temp.path().join("AGENTS.md").exists());
}

#[cfg(unix)]
#[test]
fn symbolic_link_entries_are_never_read_or_replaced() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("temp workspace");
    let outside = tempfile::NamedTempFile::new().expect("outside file");
    fs::write(outside.path(), "outside secret").expect("seed outside file");
    symlink(outside.path(), temp.path().join("MEMORY.md")).expect("workspace symlink");
    let store = WorkspaceStore::at(temp.path().to_path_buf());

    let error = store
        .document(OpenClawWorkspaceFileId::Memory)
        .expect_err("symlink must be rejected");
    assert_eq!(error.message_key, "error.openclawWorkspace.unsafeEntry");
    assert!(!error
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains(&temp.path().display().to_string()));
    assert_eq!(
        store
            .overview()
            .expect("overview remains available")
            .files
            .into_iter()
            .find(|file| file.id == OpenClawWorkspaceFileId::Memory)
            .expect("memory row")
            .status,
        OpenClawWorkspaceFileStatus::Unavailable
    );
    assert_eq!(
        fs::read_to_string(outside.path()).expect("outside unchanged"),
        "outside secret"
    );
}

#[test]
fn directory_targets_are_native_fixed_values() {
    let temp = tempfile::tempdir().expect("temp workspace");
    let store = WorkspaceStore::at(temp.path().to_path_buf());
    let daily = store
        .directory(OpenClawWorkspaceDirectory::DailyMemory)
        .expect("create daily directory");
    assert_eq!(daily, temp.path().join("memory"));
    assert!(daily.is_dir());
}
