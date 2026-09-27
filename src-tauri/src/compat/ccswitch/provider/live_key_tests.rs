use super::*;

const CLAUDE_MODEL: ConfigKey = ConfigKey::Json(&["env", "ANTHROPIC_MODEL"]);
const CLAUDE_EFFORT: ConfigKey = ConfigKey::Json(&["effortLevel"]);
const CODEX_EFFORT: ConfigKey = ConfigKey::Toml("model_reasoning_effort");
const GEMINI_MODEL: ConfigKey = ConfigKey::DotEnv("GEMINI_MODEL");

#[test]
fn json_changes_one_key_and_keeps_the_order_of_the_rest() {
    let text = "{\n  \"$schema\": \"x\",\n  \"env\": {\n    \"OTHER\": \"1\"\n  },\n  \"effortLevel\": \"xhigh\",\n  \"permissions\": {}\n}\n";
    assert_eq!(get(text, CLAUDE_EFFORT).unwrap().as_deref(), Some("xhigh"));

    let changed = set(text, CLAUDE_EFFORT, Some("high")).unwrap();
    let keys: Vec<String> = serde_json::from_str::<Value>(&changed)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(keys, ["$schema", "env", "effortLevel", "permissions"]);
    assert_eq!(
        get(&changed, CLAUDE_EFFORT).unwrap().as_deref(),
        Some("high")
    );
    assert!(changed.ends_with("}\n"));

    let with_model = set(&changed, CLAUDE_MODEL, Some("opus")).unwrap();
    let root: Value = serde_json::from_str(&with_model).unwrap();
    assert_eq!(root["env"]["OTHER"], "1");
    assert_eq!(root["env"]["ANTHROPIC_MODEL"], "opus");

    let cleared = set(&with_model, CLAUDE_MODEL, None).unwrap();
    assert_eq!(get(&cleared, CLAUDE_MODEL).unwrap(), None);
    assert_eq!(
        serde_json::from_str::<Value>(&cleared).unwrap()["env"]["OTHER"],
        "1"
    );
}

#[test]
fn json_removing_a_key_under_a_missing_object_changes_nothing() {
    let text = "{\"effortLevel\":\"low\"}";
    let changed = set(text, CLAUDE_MODEL, None).unwrap();
    assert!(serde_json::from_str::<Value>(&changed)
        .unwrap()
        .get("env")
        .is_none());
}

#[test]
fn an_unreadable_file_is_refused_rather_than_replaced() {
    assert!(get("{ not json", CLAUDE_EFFORT).is_err());
    assert!(get("[1, 2]", CLAUDE_EFFORT).is_err());
    assert!(get("model = ", CODEX_EFFORT).is_err());
    assert_eq!(get("", CLAUDE_EFFORT).unwrap(), None);
}

#[test]
fn toml_keeps_comments_tables_and_the_comment_after_the_value() {
    let text = "# my settings\nmodel = \"gpt-5.5\"\nmodel_reasoning_effort = \"low\" # fast\n\n[projects.\"/x\"]\ntrust_level = \"trusted\"\n";
    let changed = set(text, CODEX_EFFORT, Some("high")).unwrap();
    assert_eq!(
        changed,
        "# my settings\nmodel = \"gpt-5.5\"\nmodel_reasoning_effort = \"high\" # fast\n\n[projects.\"/x\"]\ntrust_level = \"trusted\"\n"
    );

    let removed = set(&changed, CODEX_EFFORT, None).unwrap();
    assert!(!removed.contains("model_reasoning_effort"));
    assert!(removed.contains("# my settings\nmodel = \"gpt-5.5\""));
    assert!(removed.contains("trust_level = \"trusted\""));
}

#[test]
fn toml_adds_a_missing_key_at_the_top_level_not_inside_a_table() {
    let text = "model = \"gpt-5.5\"\n\n[model_providers.relay]\nname = \"relay\"\n";
    let changed = set(text, CODEX_EFFORT, Some("xhigh")).unwrap();
    let doc: DocumentMut = changed.parse().unwrap();
    assert_eq!(doc["model_reasoning_effort"].as_str(), Some("xhigh"));
    assert!(doc["model_providers"]["relay"]
        .get("model_reasoning_effort")
        .is_none());
}

#[test]
fn dotenv_rewrites_its_own_line_and_leaves_the_rest() {
    let text = "# gemini\nGEMINI_API_KEY=secret\nexport GEMINI_MODEL=\"pro\"\nOTHER=1";
    assert_eq!(get(text, GEMINI_MODEL).unwrap().as_deref(), Some("pro"));

    let changed = set(text, GEMINI_MODEL, Some("flash")).unwrap();
    assert_eq!(
        changed,
        "# gemini\nGEMINI_API_KEY=secret\nexport GEMINI_MODEL=flash\nOTHER=1"
    );

    let removed = set(&changed, GEMINI_MODEL, None).unwrap();
    assert_eq!(removed, "# gemini\nGEMINI_API_KEY=secret\nOTHER=1");

    let appended = set(&removed, GEMINI_MODEL, Some("auto")).unwrap();
    assert_eq!(
        appended,
        "# gemini\nGEMINI_API_KEY=secret\nOTHER=1\nGEMINI_MODEL=auto\n"
    );
}

#[test]
fn a_write_backs_up_the_original_and_verifies_the_result() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let backups = dir.path().join("backups");
    std::fs::write(&path, "{\n  \"effortLevel\": \"xhigh\"\n}\n").unwrap();

    write(&path, &backups, CLAUDE_EFFORT, Some("medium")).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        get(&text, CLAUDE_EFFORT).unwrap().as_deref(),
        Some("medium")
    );
    let saved: Vec<_> = std::fs::read_dir(&backups).unwrap().collect();
    assert_eq!(saved.len(), 1);
    let backup = std::fs::read_to_string(saved[0].as_ref().unwrap().path()).unwrap();
    assert!(backup.contains("xhigh"));
}

#[test]
fn a_write_that_changes_nothing_touches_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let backups = dir.path().join("backups");
    std::fs::write(&path, "model_reasoning_effort = \"low\"\n").unwrap();

    write(&path, &backups, CODEX_EFFORT, Some("low")).unwrap();

    assert!(!backups.exists());
}

#[test]
fn a_missing_file_is_created_private() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gemini").join(".env");
    write(
        &path,
        &dir.path().join("backups"),
        GEMINI_MODEL,
        Some("pro"),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "GEMINI_MODEL=pro\n"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn a_file_that_does_not_parse_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, "{ broken").unwrap();
    let error = write(
        &path,
        &dir.path().join("backups"),
        CLAUDE_EFFORT,
        Some("low"),
    )
    .expect_err("refuse an unreadable file");
    assert_eq!(error.message_key, "error.modelChoice.readFailed");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ broken");
}
