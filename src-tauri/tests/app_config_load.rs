use std::fs;
use std::path::PathBuf;

use ai_manager_lib::{AppError, MultiAppConfig};

mod support;
use support::{product_data_dir, reset_test_fs, test_mutex};

fn cfg_path() -> PathBuf {
    product_data_dir().join("config.json")
}

#[test]
fn load_v1_config_returns_error_and_does_not_write() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let product_dir = product_data_dir();
    let path = cfg_path();
    fs::create_dir_all(path.parent().unwrap()).expect("create cfg dir");

    // Minimal v1 shape: providers + current, with no version/apps/mcp.
    let v1_json = r#"{"providers":{},"current":""}"#;
    fs::write(&path, v1_json).expect("seed v1 json");
    let before = fs::read_to_string(&path).expect("read before");

    let err = MultiAppConfig::load().expect_err("v1 should not be auto-migrated");
    match err {
        AppError::Localized { key, .. } => assert_eq!(key, "config.unsupported_v1"),
        other => panic!("expected Localized v1 error, got {other:?}"),
    }

    // The file must be untouched and no .bak may be produced.
    let after = fs::read_to_string(&path).expect("read after");
    assert_eq!(before, after, "config.json should not be modified");
    let bak = product_dir.join("config.json.bak");
    assert!(!bak.exists(), ".bak should not be created on load error");
}

#[test]
fn load_v1_with_extra_version_still_treated_as_v1() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let product_dir = product_data_dir();
    let path = cfg_path();
    std::fs::create_dir_all(path.parent().unwrap()).expect("create cfg dir");

    // Malformed: providers + current + version but no apps, so treat it as v1.
    let v1_like = r#"{"providers":{},"current":"","version":2}"#;
    std::fs::write(&path, v1_like).expect("seed v1-like json");
    let before = std::fs::read_to_string(&path).expect("read before");

    let err = MultiAppConfig::load().expect_err("v1-like should not be parsed as v2");
    match err {
        AppError::Localized { key, .. } => assert_eq!(key, "config.unsupported_v1"),
        other => panic!("expected Localized v1 error, got {other:?}"),
    }

    let after = std::fs::read_to_string(&path).expect("read after");
    assert_eq!(before, after, "config.json should not be modified");
    let bak = product_dir.join("config.json.bak");
    assert!(!bak.exists(), ".bak should not be created on v1-like error");
}

#[test]
fn load_invalid_json_returns_parse_error_and_does_not_write() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let product_dir = product_data_dir();
    let path = cfg_path();
    fs::create_dir_all(path.parent().unwrap()).expect("create cfg dir");

    fs::write(&path, "{not json").expect("seed invalid json");
    let before = fs::read_to_string(&path).expect("read before");

    let err = MultiAppConfig::load().expect_err("invalid json should error");
    match err {
        AppError::Json { .. } => {}
        other => panic!("expected Json error, got {other:?}"),
    }

    let after = fs::read_to_string(&path).expect("read after");
    assert_eq!(before, after, "config.json should remain unchanged");
    let bak = product_dir.join("config.json.bak");
    assert!(!bak.exists(), ".bak should not be created on parse error");
}

#[test]
fn load_valid_v2_config_succeeds() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _product_dir = product_data_dir();
    let path = cfg_path();
    fs::create_dir_all(path.parent().unwrap()).expect("create cfg dir");

    // Serialize the default struct as v2.
    let default_cfg = MultiAppConfig::default();
    let json = serde_json::to_string_pretty(&default_cfg).expect("serialize default cfg");
    fs::write(&path, json).expect("write v2 json");

    let loaded = MultiAppConfig::load().expect("v2 should load successfully");
    assert_eq!(loaded.version, 2);
    assert!(loaded
        .get_manager(&ai_manager_lib::AppType::Claude)
        .is_some());
    assert!(loaded
        .get_manager(&ai_manager_lib::AppType::Codex)
        .is_some());
}
