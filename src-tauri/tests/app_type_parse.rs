use std::str::FromStr;

use ai_manager_lib::{AppError, AppType};

#[test]
fn parse_known_apps_case_insensitive_and_trim() {
    assert!(matches!(AppType::from_str("claude"), Ok(AppType::Claude)));
    assert!(matches!(AppType::from_str("codex"), Ok(AppType::Codex)));
    assert!(matches!(
        AppType::from_str("grokbuild"),
        Ok(AppType::GrokBuild)
    ));
    assert!(matches!(
        AppType::from_str("Grok-Build"),
        Ok(AppType::GrokBuild)
    ));
    assert!(matches!(
        AppType::from_str(" ClAuDe \n"),
        Ok(AppType::Claude)
    ));
    assert!(matches!(AppType::from_str("\tcoDeX\t"), Ok(AppType::Codex)));
}

#[test]
fn parse_unknown_app_returns_localized_error_message() {
    let err = AppType::from_str("unknown").unwrap_err();
    match &err {
        AppError::Localized { key, .. } => assert_eq!(*key, "unsupported_app"),
        other => panic!("expected localized error, got {other:?}"),
    }
    assert!(err.to_string().contains("unknown"));
}
