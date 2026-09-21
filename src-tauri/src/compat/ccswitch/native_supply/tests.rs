use super::{
    code_signing_team, decode_sha512_integrity, layout_for_home, official_update_probe_url,
    parse_packument_version, registry_url, supplies, target_for,
};
use crate::compat::ccswitch::install_probe::{InstallSource, InstalledEntry};
use crate::domain::ToolId;
use std::path::{Path, PathBuf};

fn entry(source: InstallSource, bin: &str, real: &str) -> InstalledEntry {
    InstalledEntry {
        bin_path: PathBuf::from(bin),
        real_path: PathBuf::from(real),
        source,
        brew_formula: None,
        runnable: true,
        npm_package: Some("@anthropic-ai/claude-code"),
        hermes_owner: None,
    }
}

const VALID_INTEGRITY: &str = "sha512-z4PhNX7vuL3xVChQ1m2AB9Yg5AULVxXcg/SpIdNs6c5H0NE8XYXysP+DGNKHfuwvY7kxvUdBeoGlODJ6+SfaPg==";

#[test]
fn the_registry_package_follows_the_current_platform_and_windows_fails_closed() {
    let target = target_for(ToolId::ClaudeCode, "2.1.261");
    let expected = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("@anthropic-ai/claude-code-darwin-arm64"),
        ("macos", "x86_64") => Some("@anthropic-ai/claude-code-darwin-x64"),
        ("linux", "aarch64") if !cfg!(target_env = "musl") => {
            Some("@anthropic-ai/claude-code-linux-arm64")
        }
        ("linux", "x86_64") if !cfg!(target_env = "musl") => {
            Some("@anthropic-ai/claude-code-linux-x64")
        }
        _ => None,
    };
    assert_eq!(
        target.as_ref().map(|target| target.package),
        expected,
        "platform package must match the running platform"
    );
    assert_eq!(supplies(ToolId::ClaudeCode), expected.is_some());
    if let Some(target) = target {
        assert_eq!(target.version, "2.1.261");
        assert_eq!(target.binary_name, "claude");
    }
    if cfg!(target_os = "windows") {
        assert!(target_for(ToolId::ClaudeCode, "2.1.261").is_none());
    }
}

#[test]
fn only_claude_code_with_a_validated_version_has_a_supply_target() {
    for id in ToolId::ALL {
        if id != ToolId::ClaudeCode {
            assert!(target_for(id, "2.1.261").is_none(), "{}", id.as_str());
            assert!(!supplies(id), "{}", id.as_str());
        }
    }
    for version in ["", "latest", "2.1.261 --force", "2.1.261/../x", "v2.1.261"] {
        assert!(
            target_for(ToolId::ClaudeCode, version).is_none(),
            "{version:?} must not become a registry path segment"
        );
    }
}

#[test]
fn the_official_layout_is_derived_from_the_probed_native_entry_inside_home() {
    let home = Path::new("/Users/a");
    let layout = layout_for_home(
        ToolId::ClaudeCode,
        &entry(
            InstallSource::Native,
            "/Users/a/.local/bin/claude",
            "/Users/a/.local/share/claude/versions/2.1.0",
        ),
        home,
    )
    .expect("official layout");
    assert_eq!(
        layout.versions_dir,
        PathBuf::from("/Users/a/.local/share/claude/versions")
    );
    assert_eq!(layout.launcher, PathBuf::from("/Users/a/.local/bin/claude"));
}

#[test]
fn layouts_outside_home_or_without_native_ownership_are_refused() {
    let home = Path::new("/Users/a");
    let refused = [
        entry(
            InstallSource::NodeManagerNpm,
            "/Users/a/.local/bin/claude",
            "/Users/a/.local/share/claude/versions/2.1.0",
        ),
        entry(
            InstallSource::Native,
            "/usr/local/bin/claude",
            "/Users/a/.local/share/claude/versions/2.1.0",
        ),
        entry(
            InstallSource::Native,
            "/Users/a/.local/bin/claude",
            "/opt/claude/versions/2.1.0",
        ),
        entry(
            InstallSource::Native,
            "/Users/a/.local/bin/claude",
            "/Users/a/.local/share/claude/2.1.0",
        ),
        entry(
            InstallSource::Native,
            "/Users/a/.local/bin/claude",
            "/Users/a/../b/.local/share/claude/versions/2.1.0",
        ),
        entry(
            InstallSource::Native,
            "/Users/a/.local/bin/../../../etc/claude",
            "/Users/a/.local/share/claude/versions/2.1.0",
        ),
    ];
    for candidate in &refused {
        assert!(
            layout_for_home(ToolId::ClaudeCode, candidate, home).is_none(),
            "{candidate:?}"
        );
    }
    assert!(layout_for_home(
        ToolId::Codex,
        &entry(
            InstallSource::Native,
            "/Users/a/.local/bin/codex",
            "/Users/a/.codex/packages/standalone/releases/1.0.0/codex",
        ),
        home,
    )
    .is_none());
}

#[test]
fn a_registry_version_document_yields_only_its_https_tarball_and_sha512_integrity() {
    let json = format!(
        r#"{{"name":"@anthropic-ai/claude-code-darwin-arm64","version":"2.1.261","dist":{{"tarball":"https://registry.npmjs.org/@anthropic-ai/claude-code-darwin-arm64/-/claude-code-darwin-arm64-2.1.261.tgz","integrity":"{VALID_INTEGRITY}","shasum":"abc"}}}}"#
    );
    let dist = parse_packument_version(&json, "2.1.261").expect("dist");
    assert_eq!(
        dist.tarball,
        "https://registry.npmjs.org/@anthropic-ai/claude-code-darwin-arm64/-/claude-code-darwin-arm64-2.1.261.tgz"
    );
    assert_eq!(dist.integrity, VALID_INTEGRITY);
    assert_eq!(
        decode_sha512_integrity(&dist.integrity).map(|d| d.len()),
        Some(64)
    );

    // A loopback registry (local mirror or test double) may serve over plain HTTP.
    let loopback = format!(
        r#"{{"version":"2.1.261","dist":{{"tarball":"http://127.0.0.1:8080/x.tgz","integrity":"{VALID_INTEGRITY}"}}}}"#
    );
    assert_eq!(
        parse_packument_version(&loopback, "2.1.261")
            .expect("loopback http tarball")
            .tarball,
        "http://127.0.0.1:8080/x.tgz"
    );
}

#[test]
fn only_claude_code_has_a_vendor_signing_team() {
    assert_eq!(code_signing_team(ToolId::ClaudeCode), Some("Q6L2SF6YDW"));
    for id in ToolId::ALL {
        if id != ToolId::ClaudeCode {
            assert!(code_signing_team(id).is_none(), "{}", id.as_str());
        }
    }
}

#[test]
fn registry_documents_that_cannot_anchor_trust_are_rejected() {
    let rejected = [
        format!(
            r#"{{"version":"2.1.261","dist":{{"tarball":"http://registry.npmjs.org/x.tgz","integrity":"{VALID_INTEGRITY}"}}}}"#
        ),
        r#"{"version":"2.1.261","dist":{"tarball":"https://registry.npmjs.org/x.tgz","integrity":"sha1-2jmj7l5rSw0yVb/vlWAYkK/YBwk="}}"#.to_string(),
        r#"{"version":"2.1.261","dist":{"tarball":"https://registry.npmjs.org/x.tgz","integrity":"sha512-notbase64!!"}}"#.to_string(),
        r#"{"version":"2.1.261","dist":{"tarball":"https://registry.npmjs.org/x.tgz","integrity":"sha512-AAAA"}}"#.to_string(),
        r#"{"version":"2.1.261","dist":{"tarball":"https://registry.npmjs.org/x.tgz"}}"#.to_string(),
        format!(r#"{{"version":"2.1.261","dist":{{"integrity":"{VALID_INTEGRITY}"}}}}"#),
        format!(
            r#"{{"version":"2.1.260","dist":{{"tarball":"https://registry.npmjs.org/x.tgz","integrity":"{VALID_INTEGRITY}"}}}}"#
        ),
        r#"{"version":"2.1.261"}"#.to_string(),
        "not json".to_string(),
        r#"{"error":"Not found"}"#.to_string(),
    ];
    for json in &rejected {
        let error =
            parse_packument_version(json, "2.1.261").expect_err(&format!("must reject {json}"));
        assert_eq!(error.message_key, "error.tool.nativeSupplyUnavailable");
    }
}

#[test]
fn registry_urls_encode_the_scope_separator_and_tolerate_a_trailing_slash() {
    assert_eq!(
        registry_url(
            "https://registry.npmjs.org",
            "@anthropic-ai/claude-code-darwin-arm64",
            "2.1.261"
        ),
        "https://registry.npmjs.org/@anthropic-ai%2fclaude-code-darwin-arm64/2.1.261"
    );
    assert_eq!(
        registry_url("https://registry.npmmirror.com/", "left-pad", "1.3.0"),
        "https://registry.npmmirror.com/left-pad/1.3.0"
    );
}

/// Only tools whose official channel shape this product has verified have a probe address; the rest
/// return `None`, so the caller never decides on any tool's behalf that "the official channel is
/// unreachable".
#[test]
fn only_a_tool_with_a_verified_official_channel_has_a_probe_url() {
    assert_eq!(
        official_update_probe_url(ToolId::ClaudeCode),
        Some("https://downloads.claude.ai/claude-code-releases/latest")
    );
    for id in [
        ToolId::Codex,
        ToolId::OpenCode,
        ToolId::GeminiCli,
        ToolId::GrokBuild,
        ToolId::OpenClaw,
        ToolId::Hermes,
        ToolId::Pi,
        ToolId::KimiCode,
        ToolId::DeepSeekDsh,
    ] {
        assert_eq!(official_update_probe_url(id), None, "{id:?}");
    }
}
