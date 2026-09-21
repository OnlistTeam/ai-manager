use super::{
    apply_official_latest_version, claude_desktop_mcp_config_path,
    claude_desktop_mcp_config_path_for, compare_desktop_versions, inspect_macos,
    official_download_target, parse_codex_macos_appcast, parse_codex_windows_version,
    parse_windows_probe, plist_string, valid_windows_app_id, windows_inspect_spec,
    windows_launch_spec, windows_uninstall_settings_spec,
};
use crate::domain::{
    DesktopAppId, DesktopAppInstallerHandoff, DesktopAppStatus, DesktopAppUninstallHandoff,
};
use crate::platform::{AllowedProgram, DesktopAppPlatformState, Platform};
use std::cmp::Ordering;
use std::fs;
use std::path::PathBuf;

#[test]
fn claude_desktop_mcp_paths_follow_the_vendor_documented_roaming_locations() {
    assert_eq!(
        claude_desktop_mcp_config_path_for(
            Platform::MacOs,
            Some(PathBuf::from("/Users/Ada")),
            None,
        ),
        Some(PathBuf::from(
            "/Users/Ada/Library/Application Support/Claude/claude_desktop_config.json"
        ))
    );
    assert_eq!(
        claude_desktop_mcp_config_path_for(
            Platform::Windows,
            Some(PathBuf::from(r"C:\Users\Ada")),
            Some(PathBuf::from(r"C:\Users\Ada\AppData\Roaming")),
        ),
        Some(
            PathBuf::from(r"C:\Users\Ada\AppData\Roaming")
                .join("Claude")
                .join("claude_desktop_config.json")
        )
    );
    assert_eq!(
        claude_desktop_mcp_config_path_for(Platform::Linux, Some(PathBuf::from("/home/ada")), None,),
        None
    );
}

#[test]
#[serial_test::serial]
fn public_claude_desktop_mcp_path_honors_the_isolated_test_home() {
    struct EnvGuard(Option<std::ffi::OsString>);
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.0.take() {
                Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
                None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
            }
        }
    }

    let temp = tempfile::tempdir().expect("test home");
    let _guard = EnvGuard(std::env::var_os("AI_MANAGER_TEST_HOME"));
    std::env::set_var("AI_MANAGER_TEST_HOME", temp.path());

    let actual = claude_desktop_mcp_config_path();
    #[cfg(target_os = "macos")]
    assert_eq!(
        actual,
        Some(
            temp.path()
                .join("Library/Application Support/Claude/claude_desktop_config.json")
        )
    );
    #[cfg(target_os = "windows")]
    assert_eq!(
        actual,
        Some(
            temp.path()
                .join("AppData/Roaming/Claude/claude_desktop_config.json")
        )
    );
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    assert_eq!(actual, None);
}

#[test]
fn macos_probe_requires_the_fixed_bundle_executable_and_reads_version() {
    let root = tempfile::tempdir().expect("temp root");
    let applications = root.path().join("Applications");
    let bundle = applications.join("Codex.app");
    fs::create_dir_all(bundle.join("Contents/MacOS")).expect("create bundle");
    fs::write(bundle.join("Contents/MacOS/Codex"), b"binary").expect("write executable");
    fs::write(
        bundle.join("Contents/Info.plist"),
        br#"<?xml version="1.0"?><plist><dict>
        <key>CFBundleShortVersionString</key><string>26.527.60818</string>
        </dict></plist>"#,
    )
    .expect("write plist");

    let state = inspect_macos(DesktopAppId::CodexApp, &applications, None);
    assert_eq!(state.status, DesktopAppStatus::Installed);
    assert_eq!(state.version.as_deref(), Some("26.527.60818"));
    assert!(state.can_launch);
    assert_eq!(
        state.uninstall_handoff,
        DesktopAppUninstallHandoff::RevealApplication
    );
    assert!(state.updates_managed_by_vendor);

    let claude = inspect_macos(DesktopAppId::ClaudeDesktop, &applications, None);
    assert_eq!(claude.status, DesktopAppStatus::NotInstalled);
    assert!(!claude.can_launch);
}

#[test]
fn macos_probe_accepts_the_current_chatgpt_bundle_identity() {
    let root = tempfile::tempdir().expect("temp root");
    let applications = root.path().join("Applications");
    let bundle = applications.join("ChatGPT.app");
    fs::create_dir_all(bundle.join("Contents/MacOS")).expect("create bundle");
    fs::write(bundle.join("Contents/MacOS/ChatGPT"), b"binary").expect("write executable");
    fs::write(
        bundle.join("Contents/Info.plist"),
        br#"<?xml version="1.0"?><plist><dict>
        <key>CFBundleShortVersionString</key><string>27.1.0</string>
        </dict></plist>"#,
    )
    .expect("write plist");

    let state = inspect_macos(DesktopAppId::CodexApp, &applications, None);
    assert_eq!(state.status, DesktopAppStatus::Installed);
    assert_eq!(state.version.as_deref(), Some("27.1.0"));
    assert!(state.can_launch);
}

#[test]
fn an_app_shaped_directory_without_the_expected_executable_is_not_installed() {
    let root = tempfile::tempdir().expect("temp root");
    let applications = root.path().join("Applications");
    fs::create_dir_all(applications.join("Claude.app/Contents/MacOS"))
        .expect("create bundle shell");
    let state = inspect_macos(DesktopAppId::ClaudeDesktop, &applications, None);
    assert_eq!(state.status, DesktopAppStatus::NotInstalled);
}

#[test]
fn standalone_apps_require_the_audited_macos_bundle_identifiers() {
    let root = tempfile::tempdir().expect("temp root");
    let applications = root.path().join("Applications");
    for (id, bundle_name, executable, bundle_id, version) in [
        (
            DesktopAppId::Cursor,
            "Cursor.app",
            "Cursor",
            "com.todesktop.230313mzl4w4u92",
            "3.17.21",
        ),
        (
            DesktopAppId::ZCode,
            "ZCode.app",
            "ZCode",
            "dev.zcode.app",
            "3.10.1",
        ),
        (
            DesktopAppId::CherryStudio,
            "Cherry Studio.app",
            "Cherry Studio",
            "com.kangfenmao.CherryStudio",
            "2.0.10",
        ),
    ] {
        let bundle = applications.join(bundle_name);
        fs::create_dir_all(bundle.join("Contents/MacOS")).expect("create bundle");
        fs::write(bundle.join("Contents/MacOS").join(executable), b"binary")
            .expect("write executable");
        fs::write(
            bundle.join("Contents/Info.plist"),
            format!(
                "<?xml version=\"1.0\"?><plist><dict><key>CFBundleIdentifier</key><string>{bundle_id}</string><key>CFBundleShortVersionString</key><string>{version}</string></dict></plist>"
            ),
        )
        .expect("write plist");

        let state = inspect_macos(id, &applications, None);
        assert_eq!(state.status, DesktopAppStatus::Installed, "{id:?}");
        assert_eq!(state.version.as_deref(), Some(version));
        assert!(state.can_launch);
    }

    fs::write(
        applications.join("ZCode.app/Contents/Info.plist"),
        "<?xml version=\"1.0\"?><plist><dict><key>CFBundleIdentifier</key><string>example.impostor</string></dict></plist>",
    )
    .expect("replace plist");
    assert_eq!(
        inspect_macos(DesktopAppId::ZCode, &applications, None).status,
        DesktopAppStatus::NotInstalled
    );
}

#[test]
fn plist_parser_is_bounded_to_plain_safe_versions() {
    assert_eq!(
        plist_string(
            "<key>CFBundleVersion</key><string>3437</string>",
            "CFBundleVersion"
        )
        .as_deref(),
        Some("3437")
    );
    assert_eq!(
        plist_string(
            "<key>CFBundleVersion</key><string>&lt;script&gt;</string>",
            "CFBundleVersion"
        ),
        None
    );
}

#[test]
fn official_codex_version_payloads_are_strictly_parsed_and_compared() {
    let appcast = r#"<rss xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
        <channel>
            <item><sparkle:shortVersionString>26.825.41651</sparkle:shortVersionString></item>
            <item><sparkle:shortVersionString>26.825.51511</sparkle:shortVersionString></item>
            <item><sparkle:shortVersionString>latest</sparkle:shortVersionString></item>
        </channel>
    </rss>"#;
    assert_eq!(
        parse_codex_macos_appcast(appcast).as_deref(),
        Some("26.825.51511")
    );
    assert_eq!(
        compare_desktop_versions("26.825.51511", "26.527.60818"),
        Some(Ordering::Greater)
    );
    assert_eq!(compare_desktop_versions("latest", "26.1.0"), None);

    let windows = r#"{
        "schemaVersion": 1,
        "buildVersion": "26.825.6671.0",
        "storeProductId": "9PLM9XGG6VKS",
        "packageIdentity": "OpenAI.Codex"
    }"#;
    assert_eq!(
        parse_codex_windows_version(windows).as_deref(),
        Some("26.825.6671.0")
    );
    assert!(parse_codex_windows_version(
        r#"{"schemaVersion":1,"buildVersion":"26.825.6671.0","packageIdentity":"Impostor"}"#
    )
    .is_none());
}

#[test]
fn a_newer_official_codex_version_changes_the_safe_inventory_status() {
    let mut state = DesktopAppPlatformState {
        status: DesktopAppStatus::Installed,
        version: Some("26.527.60818".to_string()),
        latest_version: None,
        can_launch: true,
        environment: "macos".to_string(),
        installer_handoff: DesktopAppInstallerHandoff::DirectOfficialPackage,
        uninstall_handoff: DesktopAppUninstallHandoff::RevealApplication,
        updates_managed_by_vendor: true,
    };
    apply_official_latest_version(&mut state, "26.825.51511".to_string());
    assert_eq!(state.status, DesktopAppStatus::UpdateAvailable);
    assert_eq!(state.latest_version.as_deref(), Some("26.825.51511"));
    assert!(state.can_launch);

    state.status = DesktopAppStatus::Installed;
    apply_official_latest_version(&mut state, "26.100.1".to_string());
    assert_eq!(state.status, DesktopAppStatus::Installed);
}

#[test]
fn windows_probe_accepts_only_the_last_json_line() {
    let probe = parse_windows_probe(
        "informational line\n{\"installed\":true,\"version\":\"1.2.3.4\",\"appId\":\"OpenAI.Codex_2p2nqsd0c76g0!App\"}\n",
    )
    .expect("probe");
    assert!(probe.installed);
    assert_eq!(probe.version.as_deref(), Some("1.2.3.4"));
    assert_eq!(
        probe.app_id.as_deref(),
        Some("OpenAI.Codex_2p2nqsd0c76g0!App")
    );
    assert!(parse_windows_probe("not json").is_none());
}

#[test]
fn windows_app_ids_are_strictly_validated_before_launch() {
    assert!(valid_windows_app_id("OpenAI.Codex_2p2nqsd0c76g0!App"));
    for invalid in [
        "OpenAI.Codex_2p2nqsd0c76g0",
        "OpenAI.Codex!App;Remove-Item",
        "OpenAI.Codex!App value",
        "",
    ] {
        assert!(!valid_windows_app_id(invalid));
        assert!(windows_launch_spec(invalid).is_err());
    }
}

#[test]
fn windows_plans_keep_scripts_constant_and_values_out_of_argv() {
    let inspect = windows_inspect_spec(DesktopAppId::CodexApp).expect("audited package");
    assert_eq!(inspect.program, AllowedProgram::Powershell);
    assert_eq!(
        inspect.env,
        vec![(
            "AI_MANAGER_PACKAGE_NAME".to_string(),
            "OpenAI.Codex".to_string()
        )]
    );
    assert!(!inspect.args.join(" ").contains("OpenAI.Codex"));
    assert!(inspect.validate().is_ok());

    let app_id = "OpenAI.Codex_2p2nqsd0c76g0!App";
    let launch = windows_launch_spec(app_id).expect("launch plan");
    assert_eq!(launch.program, AllowedProgram::Powershell);
    assert_eq!(
        launch.env,
        vec![("AI_MANAGER_APP_ID".to_string(), app_id.to_string())]
    );
    assert!(!launch.args.join(" ").contains(app_id));
    assert!(launch.validate().is_ok());

    let uninstall = windows_uninstall_settings_spec();
    assert_eq!(uninstall.program, AllowedProgram::Powershell);
    assert!(uninstall.env.is_empty());
    assert!(uninstall
        .args
        .join(" ")
        .contains("ms-settings:appsfeatures"));
    assert!(uninstall.validate().is_ok());
    assert!(windows_inspect_spec(DesktopAppId::ZCode).is_none());
    assert!(windows_inspect_spec(DesktopAppId::CherryStudio).is_none());
    assert!(windows_inspect_spec(DesktopAppId::Cursor).is_none());
}

#[test]
fn official_package_targets_are_fixed_by_app_platform_and_architecture() {
    let cases = [
        (
            DesktopAppId::CodexApp,
            Platform::MacOs,
            "aarch64",
            "https://persistent.oaistatic.com/codex-app-prod/Codex.dmg",
        ),
        (
            DesktopAppId::CodexApp,
            Platform::Windows,
            "x86_64",
            "https://persistent.oaistatic.com/codex-app-prod/ChatGPT-x64.msix",
        ),
        (
            DesktopAppId::CodexApp,
            Platform::Windows,
            "aarch64",
            "https://persistent.oaistatic.com/codex-app-prod/ChatGPT-arm64.msix",
        ),
        (
            DesktopAppId::ClaudeDesktop,
            Platform::MacOs,
            "x86_64",
            "https://claude.ai/api/desktop/darwin/universal/pkg/latest/redirect",
        ),
        (
            DesktopAppId::ClaudeDesktop,
            Platform::MacOs,
            "aarch64",
            "https://claude.ai/api/desktop/darwin/universal/pkg/latest/redirect",
        ),
        (
            DesktopAppId::ClaudeDesktop,
            Platform::Windows,
            "x86_64",
            "https://claude.ai/api/desktop/win32/x64/msix/latest/redirect",
        ),
        (
            DesktopAppId::ClaudeDesktop,
            Platform::Windows,
            "aarch64",
            "https://claude.ai/api/desktop/win32/arm64/msix/latest/redirect",
        ),
    ];
    for (id, platform, architecture, expected) in cases {
        let target = official_download_target(id, platform, architecture);
        assert_eq!(target.url, expected);
        assert_eq!(
            target.handoff,
            DesktopAppInstallerHandoff::DirectOfficialPackage
        );
        assert!(target.url.starts_with("https://"));
        assert!(!target.url.contains("dl.aimanager.tools"));
    }
}

#[test]
fn unsupported_direct_package_targets_fall_back_to_vendor_pages() {
    let intel_openai = official_download_target(DesktopAppId::CodexApp, Platform::MacOs, "x86_64");
    assert_eq!(intel_openai.url, "https://chatgpt.com/download/");
    assert_eq!(
        intel_openai.handoff,
        DesktopAppInstallerHandoff::OfficialDownloadPage
    );

    let unknown =
        official_download_target(DesktopAppId::ClaudeDesktop, Platform::Unknown, "unknown");
    assert_eq!(unknown.url, "https://claude.com/download");
    assert_eq!(unknown.handoff, DesktopAppInstallerHandoff::Unsupported);

    for (id, expected) in [
        (DesktopAppId::Cursor, "https://www.cursor.com/downloads"),
        (DesktopAppId::ZCode, "https://zcode.z.ai/cn"),
        (
            DesktopAppId::CherryStudio,
            "https://cherryai.com.cn/download",
        ),
    ] {
        for platform in [Platform::MacOs, Platform::Windows, Platform::Linux] {
            let target = official_download_target(id, platform, "aarch64");
            assert_eq!(target.url, expected);
            assert_eq!(
                target.handoff,
                DesktopAppInstallerHandoff::OfficialDownloadPage
            );
        }
    }
}
