use std::path::Path;

use sha2::{Digest, Sha256};

use crate::compat::ccswitch::install_probe::{
    inspect_update, InspectedInstallation, InstalledEntry, LifecycleProbe, UpdateInspection,
};
use crate::compat::ccswitch::lifecycle::update_plan;
use crate::compat::ccswitch::tools::detect_tool;
use crate::compat::ccswitch::versioning::source_for_probe;
use crate::domain::{
    validate_observed_tool_version, validate_tool_version, validate_update_preview_fingerprint,
    AppError, ErrorCode, Tool, ToolId, ToolInstallSource, ToolStatus, ToolUpdateAttemptPreview,
    ToolUpdateBlockReason, ToolUpdateInstallation, ToolUpdateMethod, ToolUpdatePreview,
    ToolUpdateReadyPreview, MAX_UPDATE_ATTEMPTS, MAX_UPDATE_COMMANDS_PER_ATTEMPT,
    MAX_UPDATE_INSTALLATIONS,
};
use crate::platform::command::{AllowedProgram, CommandSpec};
use crate::platform::plan::LifecyclePlan;

/// A plan re-derived from a fresh inspection and observation and proven to
/// match the fingerprint the user approved. The registry round trip and the
/// installation enumeration behind it are the expensive part of an update, so
/// it is built once per click and handed through the lifecycle unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedUpdatePlan {
    pub plan: LifecyclePlan,
    pub target_version: String,
    /// The default installation the plan was built for; adapters that recover
    /// a failed plan (ADR-0033 native supply) act on exactly this entry.
    pub entry: InstalledEntry,
    /// The observation that authorized the target; the lifecycle reuses it as
    /// the pre-update baseline instead of asking the registry again.
    pub observed: Tool,
}

struct PreparedUpdatePreview {
    preview: ToolUpdateReadyPreview,
    plan: LifecyclePlan,
}

pub async fn inspect_and_preview(tool: ToolId) -> Result<ToolUpdatePreview, AppError> {
    let (inspection, observed) = tokio::join!(inspect_update(tool), detect_tool(tool));
    preview_from_inspection_and_observation(tool, inspection?, observed)
}

pub async fn checked_update_plan(
    tool: ToolId,
    expected_fingerprint: &str,
) -> Result<CheckedUpdatePlan, AppError> {
    let expected_fingerprint = validate_update_preview_fingerprint(expected_fingerprint)?;
    let (inspection, observed) = tokio::join!(inspect_update(tool), detect_tool(tool));
    checked_plan_from_inspection_and_observation(tool, inspection?, observed, &expected_fingerprint)
}

/// Only what the approved fingerprint covers can make a preview stale: the
/// observation no longer authorizing the target, the default installation
/// disappearing, or the re-derived preview hashing differently. A preparation
/// failure (a non-UTF-8 path, an unsupported plan) is reported as itself so the
/// user is not told to check again for something that will not change.
fn checked_plan_from_inspection_and_observation(
    tool: ToolId,
    inspection: UpdateInspection,
    observed: Tool,
    expected_fingerprint: &str,
) -> Result<CheckedUpdatePlan, AppError> {
    let target_version = target_version(&observed).map_err(|_| stale_preview(tool))?;
    let entry = inspection
        .lifecycle
        .entry
        .clone()
        .ok_or_else(|| stale_preview(tool))?;
    let prepared = prepared_from_inspection(tool, inspection, target_version.clone())?;
    if prepared.preview.preview_fingerprint != expected_fingerprint {
        return Err(stale_preview(tool));
    }
    Ok(CheckedUpdatePlan {
        plan: prepared.plan,
        target_version,
        entry,
        observed,
    })
}

fn preview_from_inspection_and_observation(
    tool: ToolId,
    inspection: UpdateInspection,
    observed: Tool,
) -> Result<ToolUpdatePreview, AppError> {
    let target_version = target_version(&observed)?;
    preview_from_inspection_with_target(tool, inspection, target_version)
}

#[cfg(test)]
fn preview_from_inspection(
    tool: ToolId,
    inspection: UpdateInspection,
) -> Result<ToolUpdatePreview, AppError> {
    preview_from_inspection_with_target(tool, inspection, "2.1.212".to_string())
}

fn preview_from_inspection_with_target(
    tool: ToolId,
    inspection: UpdateInspection,
    target_version: String,
) -> Result<ToolUpdatePreview, AppError> {
    if inspection.installations.len() > MAX_UPDATE_INSTALLATIONS {
        return Err(update_preview_unavailable(
            "update inspection exceeded the installation limit",
        ));
    }
    if inspection.installations.is_empty() {
        return Ok(ToolUpdatePreview::Blocked {
            tool,
            reason: ToolUpdateBlockReason::NotInstalled,
        });
    }
    if inspection.lifecycle.entry.is_none() {
        return Ok(ToolUpdatePreview::Blocked {
            tool,
            reason: ToolUpdateBlockReason::AmbiguousInstallation,
        });
    }
    let plan = match update_plan(tool, &inspection.lifecycle, &target_version) {
        Ok(plan) => plan,
        Err(error) if error.code == ErrorCode::UpdateFailed => {
            return Ok(ToolUpdatePreview::Blocked {
                tool,
                reason: ToolUpdateBlockReason::UnsupportedInstallation,
            })
        }
        Err(error) => return Err(error),
    };
    let prepared = prepared_from_inspection_and_plan(tool, inspection, plan, target_version)?;
    Ok(ToolUpdatePreview::Ready {
        preview: prepared.preview,
    })
}

fn prepared_from_inspection(
    tool: ToolId,
    inspection: UpdateInspection,
    target_version: String,
) -> Result<PreparedUpdatePreview, AppError> {
    if inspection.installations.is_empty()
        || inspection.installations.len() > MAX_UPDATE_INSTALLATIONS
        || inspection.lifecycle.entry.is_none()
    {
        return Err(update_preview_unavailable(
            "update inspection cannot produce a complete plan",
        ));
    }
    let plan = update_plan(tool, &inspection.lifecycle, &target_version)?;
    prepared_from_inspection_and_plan(tool, inspection, plan, target_version)
}

fn prepared_from_inspection_and_plan(
    tool: ToolId,
    inspection: UpdateInspection,
    plan: LifecyclePlan,
    target_version: String,
) -> Result<PreparedUpdatePreview, AppError> {
    if plan.is_empty() || plan.attempts.len() > MAX_UPDATE_ATTEMPTS {
        return Err(update_preview_unavailable(
            "update plan has an invalid attempt count",
        ));
    }
    let source = source_for_probe(&inspection.lifecycle);
    if source == ToolInstallSource::NotInstalled {
        return Err(update_preview_unavailable(
            "update plan has no selected installation source",
        ));
    }
    let ordered_installations = ordered_installations(&inspection.installations);
    for installation in &ordered_installations {
        if installation.entry.bin_path.to_str().is_none()
            || installation.entry.real_path.to_str().is_none()
        {
            return Err(update_preview_unavailable(
                "update installation path is not valid UTF-8",
            ));
        }
    }
    for attempt in &plan.attempts {
        for step in &attempt.steps {
            if step
                .spec
                .program_path
                .as_deref()
                .is_some_and(|path| path.to_str().is_none())
            {
                return Err(update_preview_unavailable(
                    "update command path is not valid UTF-8",
                ));
            }
        }
    }
    let installations = ordered_installations
        .iter()
        .map(|installation| project_installation(installation))
        .collect::<Result<Vec<_>, _>>()?;
    let attempts = plan
        .attempts
        .iter()
        .map(|attempt| {
            if attempt.steps.is_empty() || attempt.steps.len() > MAX_UPDATE_COMMANDS_PER_ATTEMPT {
                return Err(update_preview_unavailable(
                    "update attempt has an invalid command count",
                ));
            }
            let method = method_for_spec(&attempt.steps[0].spec, source)?;
            let commands = attempt
                .steps
                .iter()
                .map(|step| {
                    step.spec.validate()?;
                    Ok(step.spec.redacted_display())
                })
                .collect::<Result<Vec<_>, AppError>>()?;
            Ok(ToolUpdateAttemptPreview { method, commands })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let preview_fingerprint = fingerprint(tool, &target_version, &ordered_installations, &plan)?;
    Ok(PreparedUpdatePreview {
        preview: ToolUpdateReadyPreview {
            tool,
            preview_fingerprint,
            target_version,
            source,
            multiple_installations: installations.len() > 1,
            installations,
            attempts,
        },
        plan,
    })
}

fn ordered_installations(installations: &[InspectedInstallation]) -> Vec<&InspectedInstallation> {
    let mut ordered: Vec<_> = installations.iter().collect();
    ordered.sort_by(|left, right| {
        right
            .is_default
            .cmp(&left.is_default)
            .then_with(|| left.entry.bin_path.cmp(&right.entry.bin_path))
            .then_with(|| left.entry.real_path.cmp(&right.entry.real_path))
            .then_with(|| {
                source_for_entry(left)
                    .as_str_id()
                    .cmp(source_for_entry(right).as_str_id())
            })
            .then_with(|| left.version.cmp(&right.version))
            .then_with(|| left.entry.runnable.cmp(&right.entry.runnable))
    });
    ordered
}

fn project_installation(
    installation: &InspectedInstallation,
) -> Result<ToolUpdateInstallation, AppError> {
    let version = installation
        .version
        .as_deref()
        .map(validate_observed_tool_version)
        .transpose()?;
    let location = installation
        .entry
        .bin_path
        .to_str()
        .ok_or_else(|| update_preview_unavailable("update installation path is not valid UTF-8"))?
        .to_string();
    if location.is_empty() || location.len() > 4096 {
        return Err(update_preview_unavailable(
            "update installation location is empty or too long",
        ));
    }
    Ok(ToolUpdateInstallation {
        source: source_for_entry(installation),
        version,
        runnable: installation.entry.runnable,
        is_default: installation.is_default,
        location,
    })
}

fn source_for_entry(installation: &InspectedInstallation) -> ToolInstallSource {
    source_for_probe(&LifecycleProbe {
        entry: Some(installation.entry.clone()),
        path_env: None,
    })
}

fn method_for_spec(
    spec: &CommandSpec,
    source: ToolInstallSource,
) -> Result<ToolUpdateMethod, AppError> {
    let method = match spec.program {
        AllowedProgram::Npm => ToolUpdateMethod::Npm,
        AllowedProgram::Pnpm => ToolUpdateMethod::Pnpm,
        AllowedProgram::Bun => ToolUpdateMethod::Bun,
        AllowedProgram::Brew => ToolUpdateMethod::Homebrew,
        AllowedProgram::Volta => ToolUpdateMethod::Volta,
        AllowedProgram::Uv => ToolUpdateMethod::Uv,
        AllowedProgram::Pipx => ToolUpdateMethod::Pipx,
        AllowedProgram::Bash | AllowedProgram::Powershell => ToolUpdateMethod::OfficialInstaller,
        program
            if program.is_tool()
                && matches!(
                    spec.args.first().map(String::as_str),
                    Some("update" | "upgrade")
                ) =>
        {
            if source == ToolInstallSource::NativeInstaller {
                ToolUpdateMethod::NativeSelfUpdate
            } else {
                ToolUpdateMethod::ToolSelfUpdate
            }
        }
        _ => {
            return Err(update_preview_unavailable(
                "update plan contains an unsupported display method",
            ))
        }
    };
    Ok(method)
}

fn fingerprint(
    tool: ToolId,
    target_version: &str,
    installations: &[&InspectedInstallation],
    plan: &LifecyclePlan,
) -> Result<String, AppError> {
    let mut writer = FingerprintWriter::default();
    writer.text(tool.as_str());
    writer.text(target_version);
    writer.usize(installations.len());
    for installation in installations {
        writer.path(&installation.entry.bin_path);
        writer.path(&installation.entry.real_path);
        writer.text(source_for_entry(installation).as_str_id());
        writer.optional_text(installation.version.as_deref());
        writer.boolean(installation.entry.runnable);
        writer.boolean(installation.is_default);
    }
    writer.usize(plan.attempts.len());
    for attempt in &plan.attempts {
        writer.usize(attempt.steps.len());
        for step in &attempt.steps {
            step.spec.validate()?;
            writer.text(step.spec.program.as_str());
            writer.optional_path(step.spec.program_path.as_deref());
            writer.strings(&step.spec.args);
            writer.usize(step.spec.env.len());
            for (key, value) in &step.spec.env {
                writer.text(key);
                writer.text(value);
            }
            writer.u128(step.spec.timeout.as_millis());
            writer.usize(step.spec.sensitive_arg_indices.len());
            for index in &step.spec.sensitive_arg_indices {
                writer.usize(*index);
            }
            writer.boolean(step.ignore_failure);
        }
    }
    Ok(format!("{:x}", writer.finish()))
}

#[derive(Default)]
struct FingerprintWriter(Sha256);

impl FingerprintWriter {
    fn bytes(&mut self, value: &[u8]) {
        self.0.update((value.len() as u64).to_le_bytes());
        self.0.update(value);
    }

    fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn optional_text(&mut self, value: Option<&str>) {
        self.boolean(value.is_some());
        if let Some(value) = value {
            self.text(value);
        }
    }

    fn strings(&mut self, values: &[String]) {
        self.usize(values.len());
        for value in values {
            self.text(value);
        }
    }

    fn boolean(&mut self, value: bool) {
        self.bytes(&[u8::from(value)]);
    }

    fn usize(&mut self, value: usize) {
        self.bytes(&(value as u64).to_le_bytes());
    }

    fn u128(&mut self, value: u128) {
        self.bytes(&value.to_le_bytes());
    }

    #[cfg(unix)]
    fn path(&mut self, value: &Path) {
        use std::os::unix::ffi::OsStrExt;

        self.text("unix-bytes");
        self.bytes(value.as_os_str().as_bytes());
    }

    #[cfg(windows)]
    fn path(&mut self, value: &Path) {
        use std::os::windows::ffi::OsStrExt;

        self.text("windows-utf16le");
        let units = value.as_os_str().encode_wide().collect::<Vec<_>>();
        self.usize(units.len());
        for unit in units {
            self.bytes(&unit.to_le_bytes());
        }
    }

    fn optional_path(&mut self, value: Option<&Path>) {
        self.boolean(value.is_some());
        if let Some(value) = value {
            self.path(value);
        }
    }

    fn finish(self) -> impl std::fmt::LowerHex {
        self.0.finalize()
    }
}

fn target_version(tool: &Tool) -> Result<String, AppError> {
    if tool.status != ToolStatus::UpdateAvailable || !tool.capabilities.can_update {
        return Err(update_preview_unavailable(
            "tool inventory does not authorize an update",
        ));
    }
    let target = tool
        .latest_version
        .as_deref()
        .ok_or_else(|| update_preview_unavailable("update target version is unavailable"))?;
    validate_tool_version(target)
}

fn stale_preview(tool: ToolId) -> AppError {
    AppError::new(
        ErrorCode::UpdatePreviewStale,
        "error.tool.updatePreviewStale",
    )
    .with_technical(format!("update preview changed for {}", tool.as_str()))
    .with_remediation("error.remediation.recheckUpdate")
}

fn update_preview_unavailable(detail: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::UpdateFailed, "error.tool.actionUnsupported")
        .with_technical(detail)
        .with_remediation("error.remediation.recheckUpdate")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        checked_plan_from_inspection_and_observation, prepared_from_inspection,
        preview_from_inspection,
    };
    use crate::compat::ccswitch::install_probe::{
        InspectedInstallation, InstallSource, InstalledEntry, LifecycleProbe, UpdateInspection,
    };
    use crate::domain::{
        ErrorCode, Tool, ToolCapabilities, ToolDiscovery, ToolId, ToolStatus,
        ToolUpdateBlockReason, ToolUpdateMethod, ToolUpdatePreview,
    };

    fn installation(path: &str, real_path: &str, is_default: bool) -> InspectedInstallation {
        InspectedInstallation {
            entry: InstalledEntry {
                bin_path: PathBuf::from(path),
                real_path: PathBuf::from(real_path),
                source: InstallSource::Native,
                brew_formula: None,
                runnable: true,
                npm_package: Some("@anthropic-ai/claude-code"),
                hermes_owner: None,
            },
            version: Some("2.1.211".to_string()),
            is_default,
        }
    }

    fn prepared(inspection: UpdateInspection) -> super::PreparedUpdatePreview {
        prepared_from_inspection(ToolId::ClaudeCode, inspection, "2.1.212".to_string())
            .expect("prepared preview")
    }

    fn native_claude_inspection_at(path: &str) -> UpdateInspection {
        let default = installation(
            path,
            "/Users/test/.local/share/claude/versions/2.1.211/claude",
            true,
        );
        UpdateInspection {
            lifecycle: LifecycleProbe {
                entry: Some(default.entry.clone()),
                path_env: Some(("PATH".to_string(), "/usr/bin".to_string())),
            },
            installations: vec![default],
        }
    }

    fn native_claude_inspection() -> UpdateInspection {
        native_claude_inspection_at("/Users/test/.local/bin/claude")
    }

    fn observed(status: ToolStatus) -> Tool {
        Tool {
            id: ToolId::ClaudeCode,
            name: "Claude Code".to_string(),
            description_key: "tool.claude-code.description".to_string(),
            discovery: ToolDiscovery::for_tool(ToolId::ClaudeCode),
            status,
            version: Some("2.1.211".to_string()),
            latest_version: Some("2.1.212".to_string()),
            capabilities: ToolCapabilities {
                can_update: true,
                ..ToolCapabilities::default()
            },
            sessions_inside_settings: false,
            environment: Some("macos".to_string()),
            configuration_shared_with: None,
        }
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn a_matching_fingerprint_yields_the_plan_together_with_its_observation() {
        let fingerprint = prepared(native_claude_inspection())
            .preview
            .preview_fingerprint;
        let checked = checked_plan_from_inspection_and_observation(
            ToolId::ClaudeCode,
            native_claude_inspection(),
            observed(ToolStatus::UpdateAvailable),
            &fingerprint,
        )
        .expect("checked plan");
        assert_eq!(checked.target_version, "2.1.212");
        assert_eq!(checked.observed, observed(ToolStatus::UpdateAvailable));
        assert_eq!(
            checked.entry,
            native_claude_inspection().lifecycle.entry.expect("entry")
        );
        assert!(!checked.plan.is_empty());
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn only_what_the_fingerprint_covers_makes_the_preview_stale() {
        let fingerprint = prepared(native_claude_inspection())
            .preview
            .preview_fingerprint;
        let stale = |inspection, tool, fingerprint: &str| {
            checked_plan_from_inspection_and_observation(
                ToolId::ClaudeCode,
                inspection,
                tool,
                fingerprint,
            )
            .expect_err("stale preview")
        };

        let changed = stale(
            native_claude_inspection(),
            observed(ToolStatus::UpdateAvailable),
            &"0".repeat(64),
        );
        assert_eq!(changed.code, ErrorCode::UpdatePreviewStale);

        let unauthorized = stale(
            native_claude_inspection(),
            observed(ToolStatus::Installed),
            &fingerprint,
        );
        assert_eq!(unauthorized.code, ErrorCode::UpdatePreviewStale);

        let mut vanished = native_claude_inspection();
        vanished.lifecycle.entry = None;
        let vanished = stale(
            vanished,
            observed(ToolStatus::UpdateAvailable),
            &fingerprint,
        );
        assert_eq!(vanished.code, ErrorCode::UpdatePreviewStale);
    }

    #[test]
    #[cfg(unix)]
    fn a_preparation_failure_is_reported_as_itself_not_as_a_stale_preview() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let fingerprint = prepared(native_claude_inspection())
            .preview
            .preview_fingerprint;
        let mut inspection = native_claude_inspection();
        let unsafe_path = PathBuf::from(OsString::from_vec(vec![b'/', b'a', 0x80]));
        inspection.installations[0].entry.bin_path = unsafe_path;

        let error = checked_plan_from_inspection_and_observation(
            ToolId::ClaudeCode,
            inspection,
            observed(ToolStatus::UpdateAvailable),
            &fingerprint,
        )
        .expect_err("a lossy path cannot be planned");
        assert_eq!(error.code, ErrorCode::UpdateFailed);
        assert_ne!(error.message_key, "error.tool.updatePreviewStale");
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    /// ADR-0033: where the registry can supply the official layout the preview
    /// is the single self-update; the layout supply is an adapter-level recovery
    /// and never appears as a second command the user must approve.
    fn native_claude_preview_shows_the_official_self_update() {
        let preview = preview_from_inspection(ToolId::ClaudeCode, native_claude_inspection())
            .expect("native Claude preview");
        let ToolUpdatePreview::Ready { preview } = preview else {
            panic!("expected ready preview")
        };
        assert_eq!(
            preview.attempts[0].method,
            ToolUpdateMethod::NativeSelfUpdate
        );
        assert!(preview.attempts[0].commands[0].ends_with("claude update"));
        if crate::compat::ccswitch::native_supply::supplies(ToolId::ClaudeCode) {
            assert_eq!(preview.attempts.len(), 1);
        } else {
            assert_eq!(
                preview.attempts[1].method,
                ToolUpdateMethod::OfficialInstaller
            );
        }
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn fingerprint_changes_when_target_or_installation_set_changes() {
        let original = prepared(native_claude_inspection())
            .preview
            .preview_fingerprint;
        let moved = prepared(native_claude_inspection_at("/Users/test/bin/claude"))
            .preview
            .preview_fingerprint;
        let mut second = native_claude_inspection();
        second.installations.push(installation(
            "/opt/homebrew/bin/claude",
            "/opt/homebrew/bin/claude",
            false,
        ));
        let second = prepared(second).preview.preview_fingerprint;
        let changed_target = prepared_from_inspection(
            ToolId::ClaudeCode,
            native_claude_inspection(),
            "3.0.0".to_string(),
        )
        .expect("changed target preview")
        .preview
        .preview_fingerprint;
        assert_ne!(original, moved);
        assert_ne!(original, second);
        assert_ne!(original, changed_target);
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn fingerprint_and_display_ignore_non_default_enumeration_order() {
        let default = installation(
            "/Users/test/.local/bin/claude",
            "/Users/test/.local/share/claude/versions/2.1.211/claude",
            true,
        );
        let first = installation("/a/claude", "/real/a/claude", false);
        let second = installation("/b/claude", "/real/b/claude", false);
        let inspect = |installations| UpdateInspection {
            lifecycle: LifecycleProbe {
                entry: Some(default.entry.clone()),
                path_env: Some(("PATH".to_string(), "/usr/bin".to_string())),
            },
            installations,
        };
        let left = prepared(inspect(vec![
            default.clone(),
            first.clone(),
            second.clone(),
        ]))
        .preview;
        let right = prepared(inspect(vec![default.clone(), second, first])).preview;

        assert_eq!(left.preview_fingerprint, right.preview_fingerprint);
        assert_eq!(left.installations, right.installations);
    }

    #[test]
    #[cfg(unix)]
    fn raw_non_utf8_paths_cannot_collide_in_the_fingerprint() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let left = PathBuf::from(OsString::from_vec(vec![b'/', b'a', 0x80]));
        let right = PathBuf::from(OsString::from_vec(vec![b'/', b'a', 0x81]));
        assert_eq!(left.to_string_lossy(), right.to_string_lossy());

        let digest = |path: &std::path::Path| {
            let mut writer = super::FingerprintWriter::default();
            writer.path(path);
            format!("{:x}", writer.finish())
        };
        assert_ne!(digest(&left), digest(&right));
    }

    #[test]
    #[cfg(unix)]
    fn non_utf8_installation_locations_fail_closed() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let mut inspection = native_claude_inspection();
        let unsafe_path = PathBuf::from(OsString::from_vec(vec![
            b'/', b'U', b's', b'e', b'r', b's', b'/', 0x80, b'/', b'c', b'l', b'a', b'u', b'd',
            b'e',
        ]));
        inspection.lifecycle.entry.as_mut().unwrap().bin_path = unsafe_path.clone();
        inspection.installations[0].entry.bin_path = unsafe_path;

        preview_from_inspection(ToolId::ClaudeCode, inspection)
            .expect_err("a lossy display path must not cross the wire");
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn invalid_observed_version_is_an_inspection_failure_not_an_unsupported_install() {
        let mut inspection = native_claude_inspection();
        inspection.installations[0].version = Some("nightly build".to_string());
        let error = preview_from_inspection(ToolId::ClaudeCode, inspection)
            .expect_err("unsafe observed version must not be projected");
        assert_ne!(error.message_key, "error.tool.actionUnsupported");
    }

    #[test]
    fn ambiguous_installation_is_blocked_instead_of_using_static_fallback() {
        let first = installation("/a/claude", "/a/claude", false);
        let second = installation("/b/claude", "/b/claude", false);
        let preview = preview_from_inspection(
            ToolId::ClaudeCode,
            UpdateInspection {
                lifecycle: LifecycleProbe {
                    entry: None,
                    path_env: None,
                },
                installations: vec![first, second],
            },
        )
        .expect("blocked preview");
        assert_eq!(
            preview,
            ToolUpdatePreview::Blocked {
                tool: ToolId::ClaudeCode,
                reason: ToolUpdateBlockReason::AmbiguousInstallation,
            }
        );
    }
}
