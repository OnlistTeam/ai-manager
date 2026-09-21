use std::fs;

use rusqlite::{params, Connection};
use tempfile::tempdir;

use super::{
    merge_import, open_source_read_only, preview_import, snapshot_source_database,
    SUPPORTED_APP_TYPES,
};
use crate::domain::{ErrorCode, ImportSummary};

const TEST_SCHEMA_VERSION: i32 = 17;

fn schema(connection: &Connection) {
    connection
        .execute_batch(
            "CREATE TABLE providers (
                id TEXT NOT NULL, app_type TEXT NOT NULL, name TEXT NOT NULL,
                settings_config TEXT NOT NULL, website_url TEXT, category TEXT,
                created_at INTEGER, sort_index INTEGER, notes TEXT, icon TEXT,
                icon_color TEXT, meta TEXT NOT NULL DEFAULT '{}',
                is_current BOOLEAN NOT NULL DEFAULT 0,
                in_failover_queue BOOLEAN NOT NULL DEFAULT 0,
                PRIMARY KEY (id, app_type)
             );
             CREATE TABLE provider_endpoints (
                id INTEGER PRIMARY KEY AUTOINCREMENT, provider_id TEXT NOT NULL,
                app_type TEXT NOT NULL, url TEXT NOT NULL, added_at INTEGER
             );
             CREATE TABLE mcp_servers (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, server_config TEXT NOT NULL,
                description TEXT, homepage TEXT, docs TEXT, tags TEXT NOT NULL DEFAULT '[]',
                enabled_claude BOOLEAN NOT NULL DEFAULT 0,
                enabled_codex BOOLEAN NOT NULL DEFAULT 0,
                enabled_gemini BOOLEAN NOT NULL DEFAULT 0,
                enabled_grokbuild BOOLEAN NOT NULL DEFAULT 0,
                enabled_opencode BOOLEAN NOT NULL DEFAULT 0,
                enabled_hermes BOOLEAN NOT NULL DEFAULT 0
             );
             CREATE TABLE skills (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT,
                directory TEXT NOT NULL, repo_owner TEXT, repo_name TEXT,
                repo_branch TEXT DEFAULT 'main', readme_url TEXT,
                enabled_claude BOOLEAN NOT NULL DEFAULT 0,
                enabled_codex BOOLEAN NOT NULL DEFAULT 0,
                enabled_gemini BOOLEAN NOT NULL DEFAULT 0,
                enabled_grokbuild BOOLEAN NOT NULL DEFAULT 0,
                enabled_opencode BOOLEAN NOT NULL DEFAULT 0,
                enabled_hermes BOOLEAN NOT NULL DEFAULT 0,
                installed_at INTEGER NOT NULL DEFAULT 0, content_hash TEXT,
                updated_at INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);",
        )
        .expect("create import fixture schema");
    connection
        .pragma_update(None, "user_version", TEST_SCHEMA_VERSION)
        .expect("set schema version");
}

fn database_at(path: &std::path::Path) -> Connection {
    let connection = Connection::open(path).expect("open fixture database");
    schema(&connection);
    connection
}

fn provider(connection: &Connection, id: &str, app_type: &str, name: &str, current: bool) {
    connection
        .execute(
            "INSERT INTO providers
             (id, app_type, name, settings_config, meta, is_current, in_failover_queue)
             VALUES (?1,?2,?3,'{}','{}',?4,0)",
            params![id, app_type, name, current],
        )
        .expect("insert provider");
}

fn mcp(connection: &Connection, id: &str, name: &str) {
    connection
        .execute(
            "INSERT INTO mcp_servers
             (id,name,server_config,tags,enabled_claude,enabled_codex,enabled_gemini,
              enabled_grokbuild,enabled_opencode,enabled_hermes)
             VALUES (?1,?2,'{}','[]',1,1,1,1,1,1)",
            params![id, name],
        )
        .expect("insert MCP server");
}

fn skill(connection: &Connection, id: &str, name: &str) {
    connection
        .execute(
            "INSERT INTO skills
             (id,name,directory,enabled_claude,enabled_codex,enabled_gemini,
              enabled_grokbuild,enabled_opencode,enabled_hermes)
             VALUES (?1,?2,?3,1,1,1,1,1,1)",
            params![id, name, format!("/fixture/{id}")],
        )
        .expect("insert skill");
}

#[test]
fn supported_app_type_allow_list_matches_adr_0004() {
    assert_eq!(
        SUPPORTED_APP_TYPES,
        ["claude", "codex", "opencode", "gemini"]
    );
}

#[test]
fn a_missing_source_is_not_an_error() {
    let directory = tempdir().expect("temporary directory");
    assert!(
        snapshot_source_database(&directory.path().join("missing.db"), TEST_SCHEMA_VERSION,)
            .expect("missing source")
            .is_none()
    );
}

#[test]
fn corrupt_and_non_cc_switch_sources_are_rejected() {
    let directory = tempdir().expect("temporary directory");
    let corrupt = directory.path().join("corrupt.db");
    fs::write(&corrupt, b"this is not sqlite").expect("write corrupt fixture");
    let error =
        snapshot_source_database(&corrupt, TEST_SCHEMA_VERSION).expect_err("reject corrupt");
    assert_eq!(error.message_key, "error.import.invalidSource");

    let unrelated = directory.path().join("unrelated.db");
    Connection::open(&unrelated)
        .expect("create unrelated database")
        .execute("CREATE TABLE unrelated (id TEXT)", [])
        .expect("create unrelated table");
    let error =
        snapshot_source_database(&unrelated, TEST_SCHEMA_VERSION).expect_err("reject unrelated");
    assert_eq!(error.message_key, "error.import.invalidSource");
}

#[test]
fn a_future_schema_is_rejected_before_copying() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("future.db");
    let connection = database_at(&path);
    connection
        .pragma_update(None, "user_version", TEST_SCHEMA_VERSION + 1)
        .expect("set future version");
    drop(connection);

    let error =
        snapshot_source_database(&path, TEST_SCHEMA_VERSION).expect_err("reject future schema");
    assert_eq!(error.code, ErrorCode::ConfigParseFailed);
    assert_eq!(error.message_key, "error.import.newerVersion");
}

#[test]
fn the_disk_source_rejects_writes_and_remains_byte_identical() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("source.db");
    let connection = database_at(&path);
    provider(&connection, "source", "claude", "Source", true);
    drop(connection);
    let before = fs::read(&path).expect("read before");

    let read_only = open_source_read_only(&path).expect("open read-only");
    assert!(read_only
        .execute("DELETE FROM providers", [])
        .expect_err("writes must fail")
        .to_string()
        .contains("readonly"));
    drop(read_only);
    let snapshot = snapshot_source_database(&path, TEST_SCHEMA_VERSION)
        .expect("snapshot")
        .expect("source exists");
    assert_eq!(
        snapshot
            .query_row("SELECT name FROM providers", [], |row| {
                row.get::<_, String>(0)
            })
            .expect("snapshot row"),
        "Source"
    );
    drop(snapshot);
    assert_eq!(fs::read(&path).expect("read after"), before);
}

#[test]
fn preview_counts_only_supported_services_and_all_extensions() {
    let source = Connection::open_in_memory().expect("source");
    schema(&source);
    provider(&source, "claude", "claude", "Claude", true);
    provider(&source, "codex", "codex", "Codex", true);
    provider(&source, "legacy", "grokbuild", "Legacy", true);
    mcp(&source, "mcp-one", "MCP one");
    skill(&source, "skill-one", "Skill one");

    assert_eq!(
        preview_import(&source).expect("preview"),
        ImportSummary {
            services: 2,
            mcp_servers: 1,
            skills: 1,
        }
    );
}

#[test]
fn merge_is_source_wins_but_preserves_target_only_rows_and_settings() {
    let source = Connection::open_in_memory().expect("source");
    schema(&source);
    provider(&source, "shared", "claude", "Source shared", true);
    provider(&source, "new", "codex", "Source new", true);
    provider(&source, "ignored", "grokbuild", "Ignored", true);
    source
        .execute(
            "INSERT INTO provider_endpoints (provider_id,app_type,url,added_at)
             VALUES ('shared','claude','https://source.example',2)",
            [],
        )
        .expect("source endpoint");
    mcp(&source, "shared-mcp", "Source MCP");
    skill(&source, "shared-skill", "Source Skill");

    let mut target = Connection::open_in_memory().expect("target");
    schema(&target);
    provider(&target, "shared", "claude", "Target shared", false);
    provider(&target, "target-only", "claude", "Target only", true);
    target
        .execute(
            "INSERT INTO provider_endpoints (provider_id,app_type,url,added_at)
             VALUES ('shared','claude','https://target.example',1)",
            [],
        )
        .expect("target endpoint");
    mcp(&target, "shared-mcp", "Target MCP");
    mcp(&target, "target-mcp", "Target only MCP");
    skill(&target, "shared-skill", "Target Skill");
    skill(&target, "target-skill", "Target only Skill");
    target
        .execute(
            "INSERT INTO settings (key,value) VALUES ('aimgr.importPromptSeen','true')",
            [],
        )
        .expect("product setting");

    assert_eq!(
        merge_import(&source, &mut target).expect("merge"),
        ImportSummary {
            services: 2,
            mcp_servers: 1,
            skills: 1,
        }
    );
    assert_eq!(
        target
            .query_row(
                "SELECT name FROM providers WHERE id='shared' AND app_type='claude'",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("shared provider"),
        "Source shared"
    );
    assert_eq!(
        target
            .query_row(
                "SELECT url FROM provider_endpoints WHERE provider_id='shared' AND app_type='claude'",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("replacement endpoint"),
        "https://source.example"
    );
    assert!(!target
        .query_row(
            "SELECT is_current FROM providers WHERE id='target-only' AND app_type='claude'",
            [],
            |row| row.get::<_, bool>(0),
        )
        .expect("target current cleared"));
    assert_eq!(
        target
            .query_row(
                "SELECT name FROM providers WHERE id='target-only' AND app_type='claude'",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("target-only provider"),
        "Target only"
    );
    let ignored: i64 = target
        .query_row(
            "SELECT COUNT(*) FROM providers WHERE id='ignored'",
            [],
            |row| row.get(0),
        )
        .expect("ignored count");
    assert_eq!(ignored, 0);
    let mcp_row: (String, bool, bool) = target
        .query_row(
            "SELECT name,enabled_grokbuild,enabled_hermes FROM mcp_servers WHERE id='shared-mcp'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("MCP collision");
    assert_eq!(mcp_row, ("Source MCP".to_string(), false, false));
    let skill_row: (String, bool, bool) = target
        .query_row(
            "SELECT name,enabled_grokbuild,enabled_hermes FROM skills WHERE id='shared-skill'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("Skill collision");
    assert_eq!(skill_row, ("Source Skill".to_string(), false, false));
    for table_and_id in [("mcp_servers", "target-mcp"), ("skills", "target-skill")] {
        let sql = format!("SELECT COUNT(*) FROM {} WHERE id=?1", table_and_id.0);
        let count: i64 = target
            .query_row(&sql, [table_and_id.1], |row| row.get(0))
            .expect("target-only extension");
        assert_eq!(count, 1);
    }
    assert_eq!(
        target
            .query_row(
                "SELECT value FROM settings WHERE key='aimgr.importPromptSeen'",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("product preference"),
        "true"
    );
}

#[test]
fn a_mid_merge_error_rolls_back_every_category() {
    let source = Connection::open_in_memory().expect("source");
    schema(&source);
    provider(&source, "would-be-partial", "claude", "Partial", true);
    mcp(&source, "blocked", "Blocked");

    let mut target = Connection::open_in_memory().expect("target");
    schema(&target);
    target
        .execute_batch(
            "CREATE TRIGGER reject_fixture BEFORE INSERT ON mcp_servers
             WHEN NEW.id = 'blocked'
             BEGIN SELECT RAISE(ABORT, 'fixture rejection'); END;",
        )
        .expect("failure trigger");

    let error = merge_import(&source, &mut target).expect_err("merge fails");
    assert_eq!(error.message_key, "error.import.mergeFailed");
    let providers: i64 = target
        .query_row(
            "SELECT COUNT(*) FROM providers WHERE id='would-be-partial'",
            [],
            |row| row.get(0),
        )
        .expect("provider count");
    assert_eq!(
        providers, 0,
        "provider insert must roll back with MCP failure"
    );
}
