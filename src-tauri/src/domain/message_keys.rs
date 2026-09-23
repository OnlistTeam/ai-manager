//! The single inventory of user-visible message_keys (spec §42 / §56).
//!
//! If production code emits a new key without registering it here, the Rust test in this file
//! goes red; if a key is registered but one of the four locales lacks the copy,
//! `tests/i18n/messageKeys.test.ts` goes red. Together the two directions guarantee that
//! "every sentence the backend can say, the frontend can say in all four languages".
//!
//! Only keys the **production path** can emit are registered. Keys in test fixtures (e.g.
//! `error.remediation.retryInstall`) are not translated: copy nobody ever sees is dead copy.

pub const USER_FACING_MESSAGE_KEYS: &[&str] = &[
    "error.about.openLinkFailed",
    "error.appUpdate.installFailed",
    "error.appUpdate.notReady",
    "error.appUpdate.openDownloadFailed",
    "error.backup.createFailed",
    "error.backup.deleteFailed",
    "error.backup.exportFailed",
    "error.backup.exportLocationRefused",
    "error.backup.importFailed",
    "error.backup.importTooLarge",
    "error.backup.listFailed",
    "error.backup.nameExists",
    "error.backup.nameInvalid",
    "error.backup.nameRequired",
    "error.backup.nameTooLong",
    "error.backup.noDatabase",
    "error.backup.notFound",
    "error.backup.renameFailed",
    "error.backup.restoreFailed",
    "error.backup.scheduleSaveFailed",
    "error.backup.selectionUnavailable",
    "error.backup.storeUnavailable",
    "error.command.joinFailed",
    "error.command.permissionDenied",
    "error.command.programNotFound",
    "error.command.spawnFailed",
    "error.command.timeout",
    "error.commandSpec.argContainsNul",
    "error.commandSpec.bashRequiresSingleScript",
    "error.commandSpec.envContainsNul",
    "error.commandSpec.envKeyInvalid",
    "error.commandSpec.programPathMismatch",
    "error.commandSpec.programPathNotAbsolute",
    "error.commandSpec.scriptNotSealed",
    "error.commandSpec.sensitiveIndexOutOfRange",
    "error.commandSpec.terminalTargetInvalid",
    "error.commandSpec.terminalTargetNotAnchored",
    "error.commandSpec.terminalWorkingDirectoryNotAbsolute",
    "error.commandSpec.timeoutZero",
    "error.commandSpec.wslDistroInvalid",
    "error.deepLink.credentialRequired",
    "error.deepLink.duplicateParameter",
    "error.deepLink.endpointRequired",
    "error.deepLink.invalidEncoding",
    "error.deepLink.invalidLink",
    "error.deepLink.mcpUnsupportedFields",
    "error.deepLink.missingParameter",
    "error.deepLink.notPending",
    "error.deepLink.tooLarge",
    "error.deepLink.toolUnavailable",
    "error.deepLink.unknownParameter",
    "error.deepLink.unsupportedResource",
    "error.deepLink.unsupportedScheme",
    "error.deepLink.unsupportedVersion",
    "error.deepLink.valueTooLong",
    "error.desktopApp.inventoryFailed",
    "error.desktopApp.launchFailed",
    "error.desktopApp.launchUnsupported",
    "error.desktopApp.notFound",
    "error.desktopApp.officialDownloadFailed",
    "error.desktopApp.officialDownloadUnsupported",
    "error.desktopApp.uninstallHandoffFailed",
    "error.desktopApp.uninstallHandoffUnsupported",
    "error.desktopPreferences.invalid",
    "error.desktopPreferences.readFailed",
    "error.desktopPreferences.saveFailed",
    "error.extension.adoptFailed",
    "error.extension.adoptUnsupported",
    "error.extension.cannotDisable",
    "error.extension.copyFailed",
    "error.extension.detectedReadOnly",
    "error.extension.listFailed",
    "error.extension.notFound",
    "error.extension.resourceOpenFailed",
    "error.extension.storeUnavailable",
    "error.extension.toggleFailed",
    "error.extension.unknownKind",
    "error.extension.unsupportedScope",
    "error.health.snapshotFailed",
    "error.health.storeUnavailable",
    "error.import.invalidSource",
    "error.import.mergeFailed",
    "error.import.newerVersion",
    "error.import.notFound",
    "error.import.readFailed",
    "error.import.storeUnavailable",
    "error.mcp.actionPanicked",
    "error.mcp.argumentsInvalid",
    "error.mcp.commandInvalid",
    "error.mcp.commandRequired",
    "error.mcp.descriptionTooLong",
    "error.mcp.installCleanupFailed",
    "error.mcp.installFailed",
    "error.mcp.nameInvalid",
    "error.mcp.nameRequired",
    "error.mcp.nameTooLong",
    "error.mcp.notInstalled",
    "error.mcp.removeFailed",
    "error.mcp.removePanicked",
    "error.mcp.removeRestoreFailed",
    "error.mcp.removeVerifyFailed",
    "error.mcp.unsupportedScope",
    "error.mcp.urlCredentials",
    "error.mcp.urlInsecure",
    "error.mcp.urlInvalid",
    "error.mcp.urlRequired",
    "error.mcp.verifyFailed",
    "error.networkProxy.invalid",
    "error.networkProxy.readFailed",
    "error.networkProxy.saveFailed",
    "error.networkProxy.storeUnavailable",
    "error.openclawWorkspace.contentInvalid",
    "error.openclawWorkspace.contentTooLarge",
    "error.openclawWorkspace.dateInvalid",
    "error.openclawWorkspace.deleteFailed",
    "error.openclawWorkspace.directoryOpenFailed",
    "error.openclawWorkspace.invalidTarget",
    "error.openclawWorkspace.listFailed",
    "error.openclawWorkspace.queryInvalid",
    "error.openclawWorkspace.readFailed",
    "error.openclawWorkspace.restoreFailed",
    "error.openclawWorkspace.saveFailed",
    "error.openclawWorkspace.unsafeEntry",
    "error.openclawWorkspace.verifyFailed",
    "error.operation.cancelFailed",
    "error.operation.cancelNotRequested",
    "error.operation.cancelPending",
    "error.operation.cancelUnconfirmed",
    "error.operation.cancelled",
    "error.operation.desktopAppBusy",
    "error.operation.invalidId",
    "error.operation.invalidTransition",
    "error.operation.notCancellable",
    "error.operation.notCancellableNow",
    "error.operation.notFound",
    "error.operation.notRunning",
    "error.operation.toolBusy",
    "error.prompt.contentInvalid",
    "error.prompt.contentRequired",
    "error.prompt.contentTooLong",
    "error.prompt.descriptionInvalid",
    "error.prompt.descriptionTooLong",
    "error.prompt.importEmpty",
    "error.prompt.importFailed",
    "error.prompt.importMissing",
    "error.prompt.liveInvalid",
    "error.prompt.nameInvalid",
    "error.prompt.nameRequired",
    "error.prompt.nameTooLong",
    "error.prompt.notFound",
    "error.prompt.readFailed",
    "error.prompt.removeActive",
    "error.prompt.removeFailed",
    "error.prompt.removeRestoreFailed",
    "error.prompt.saveFailed",
    "error.prompt.saveRestoreFailed",
    "error.prompt.saveVerifyFailed",
    "error.prompt.unsupportedTool",
    "error.provider.createFailed",
    "error.provider.listFailed",
    "error.provider.modelProbeImageTooLarge",
    "error.provider.modelProbeModelInvalid",
    "error.provider.modelProbePromptInvalid",
    "error.provider.modelProbeTimeout",
    "error.provider.modelProbeUnreachable",
    "error.provider.modelProbeUnreadable",
    "error.provider.nameRequired",
    "error.provider.notFound",
    "error.provider.notTestable",
    "error.provider.removeActive",
    "error.provider.removeFailed",
    "error.provider.removePanicked",
    "error.provider.removeRestoreFailed",
    "error.provider.removeUnsupported",
    "error.provider.removeVerifyFailed",
    "error.provider.runtimeContextFailed",
    "error.provider.runtimeResourceNotFound",
    "error.provider.runtimeResourceOpenFailed",
    "error.provider.saveFailed",
    "error.provider.saveRestoreFailed",
    "error.provider.settingsPreserveFailed",
    "error.provider.storeUnavailable",
    "error.provider.switchFailed",
    "error.provider.testFailed",
    "error.provider.unsupportedTool",
    "error.remediation.allowTerminalAutomation",
    "error.remediation.checkConnectionSettings",
    "error.remediation.checkInternetConnection",
    "error.remediation.checkLocalFileAccess",
    "error.remediation.checkMcpSettings",
    "error.remediation.checkPermissions",
    "error.remediation.checkPromptSettings",
    "error.remediation.checkServiceRegion",
    "error.remediation.checkServiceSettings",
    "error.remediation.chooseAnotherFolder",
    "error.remediation.chooseAnotherPrompt",
    "error.remediation.chooseAnotherService",
    "error.remediation.chooseListedVersion",
    "error.remediation.configureInstallNetwork",
    "error.remediation.installManually",
    "error.remediation.openDesktopAppManually",
    "error.remediation.openToolManually",
    "error.remediation.recheckUpdate",
    "error.remediation.refreshSessions",
    "error.remediation.removeInTool",
    "error.remediation.retryOrViewDetails",
    "error.remediation.uninstallDesktopAppManually",
    "error.remediation.uninstallManually",
    "error.remediation.useOriginalUpdateChannel",
    "error.routing.changeFailed",
    "error.routing.providerNotEligible",
    "error.routing.providerUnavailable",
    "error.routing.queueLocked",
    "error.routing.readFailed",
    "error.routing.storeUnavailable",
    "error.routing.takeoverRequired",
    "error.routing.unsupportedTool",
    "error.session.notFound",
    "error.session.queryInvalid",
    "error.session.readFailed",
    "error.session.resumeUnsupported",
    "error.settings.loadFailed",
    "error.settings.saveFailed",
    "error.settings.storeUnavailable",
    "error.shellVariable.movedOn",
    "error.shellVariable.notAConnectionVariable",
    "error.shellVariable.notRewritable",
    "error.shellVariable.outsideHome",
    "error.shellVariable.valueInvalid",
    "error.shellVariable.writeFailed",
    "error.skill.actionPanicked",
    "error.skill.alreadyInstalled",
    "error.skill.backupConflict",
    "error.skill.backupDeleteFailed",
    "error.skill.backupListFailed",
    "error.skill.backupNotFound",
    "error.skill.backupRestoreFailed",
    "error.skill.backupVerifyFailed",
    "error.skill.catalogFailed",
    "error.skill.conflict",
    "error.skill.downloadFailed",
    "error.skill.installFailed",
    "error.skill.invalidSource",
    "error.skill.notInstalled",
    "error.skill.removeFailed",
    "error.skill.removePanicked",
    "error.skill.removeVerifyFailed",
    "error.skill.repositoryInvalid",
    "error.skill.repositoryListFailed",
    "error.skill.repositoryNotFound",
    "error.skill.repositoryRemoveFailed",
    "error.skill.repositorySaveFailed",
    "error.skill.storeUnavailable",
    "error.skill.unsupportedTool",
    "error.skill.updateCheckFailed",
    "error.skill.updateDownloadFailed",
    "error.skill.updateFailed",
    "error.skill.updatePanicked",
    "error.skill.updateSourceInvalid",
    "error.skill.updateUnsupported",
    "error.skill.updateVerifyFailed",
    "error.skill.verifyFailed",
    "error.skill.zipInstallFailed",
    "error.skill.zipInvalid",
    "error.skill.zipNoNewSkills",
    "error.skill.zipNoSkills",
    "error.skill.zipSelectionUnavailable",
    "error.skill.zipVerifyFailed",
    "error.system.openSettingsFailed",
    "error.system.revealFailed",
    "error.tool.actionPanicked",
    "error.tool.actionUnsupported",
    "error.tool.downloadAccessDenied",
    "error.tool.downloadNetworkFailed",
    "error.tool.downloadRegistryUnavailable",
    "error.tool.installFailed",
    "error.tool.launchFailed",
    "error.tool.nativeSupplyIntegrity",
    "error.tool.nativeSupplyLayout",
    "error.tool.nativeSupplySignature",
    "error.tool.nativeSupplySwitchFailed",
    "error.tool.nativeSupplyUnavailable",
    "error.tool.nativeSupplyUnsupported",
    "error.tool.nativeSupplyVerifyFailed",
    "error.tool.notFound",
    "error.tool.notInstalled",
    "error.tool.projectFolderUnavailable",
    "error.tool.removePathDenied",
    "error.tool.removePathRefused",
    "error.tool.serviceRegionUnavailable",
    "error.tool.terminalAutomationDenied",
    "error.tool.uninstallFailed",
    "error.tool.uninstallIncomplete",
    "error.tool.uninstallStrategyUnknown",
    "error.tool.updateFailed",
    "error.tool.updatePreviewStale",
    "error.tool.updatePreviewUnavailable",
    "error.tool.verifyFailed",
    "error.tool.versionCatalogFailed",
    "error.tool.versionHistoryReadFailed",
    "error.tool.versionHistoryWriteFailed",
    "error.tool.versionInvalid",
    "error.tool.versionSourceUnsupported",
    "error.usage.readFailed",
    "error.usage.storeUnavailable",
    "operation.log.cancellationRequested",
    "operation.log.cancelled",
    "operation.log.commandFailed",
    "operation.log.commandFailureIgnored",
    "operation.log.commandSucceeded",
    "operation.log.failed",
    "operation.log.nativeSupply.done",
    "operation.log.nativeSupply.start",
    "operation.log.started",
    "operation.log.succeeded",
    "operation.log.truncated",
    "operation.log.tryingFallback",
    "operation.log.usingCommunityMirror",
    "operation.log.usingConfiguredProxy",
    "operation.phase.cancelled",
    "operation.phase.cancelling",
    "operation.phase.checking",
    "operation.phase.configuring",
    "operation.phase.downloading",
    "operation.phase.installing",
    "operation.phase.preparing",
    "operation.phase.ready",
    "operation.phase.removed",
    "operation.phase.removing",
];

#[cfg(test)]
mod tests {
    use super::USER_FACING_MESSAGE_KEYS;
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    /// Product-layer directories plus the small product command files. Upstream directories
    /// (`database/`, `services/`, `commands/misc.rs`, ...) have their own message_key system and
    /// are out of this product's translation scope.
    const SCANNED_DIRS: &[&str] = &[
        "src/domain",
        "src/platform",
        "src/repositories",
        "src/adapters",
        "src/application",
        "src/compat",
        "src/infrastructure",
    ];
    const SCANNED_FILES: &[&str] = &[
        "src/commands/app_about_api.rs",
        "src/commands/app_api.rs",
        "src/commands/app_deeplink_api.rs",
        "src/commands/app_desktop_api.rs",
        "src/commands/app_network_api.rs",
        "src/commands/app_routing_api.rs",
        "src/commands/app_session_api.rs",
        "src/commands/app_skill_api.rs",
        "src/commands/app_system_api.rs",
        "src/commands/app_usage_api.rs",
        "src/commands/app_workspace_api.rs",
    ];

    fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_rust_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    /// Production code only. Tests in this repository are either a `mod tests` at the end of a
    /// file or referenced by `#[cfg(test)] #[path = "…/tests*.rs"] mod …;` pointing at a subfile
    /// whose name starts with `tests` (tests.rs / tests_write.rs / tests_panics.rs, ...); those
    /// subfiles often have no `#[cfg(test)]` line of their own and are excluded wholesale by file
    /// name. Everything after the first `#[cfg(test)]` is then cut off to remove inline fixtures.
    fn production_source(path: &Path) -> Option<String> {
        let name = path.file_name()?.to_str()?;
        if name.starts_with("tests") || name.ends_with("_tests.rs") || name == "message_keys.rs" {
            return None;
        }
        let content = std::fs::read_to_string(path).ok()?;
        Some(match content.find("\n#[cfg(test)]") {
            Some(cut) => content[..cut].to_string(),
            None => content,
        })
    }

    fn keys_in_line(line: &str, out: &mut BTreeSet<String>) {
        let mut rest = line;
        while let Some(open) = rest.find('"') {
            let after = &rest[open + 1..];
            let Some(close) = after.find('"') else {
                break;
            };
            let literal = &after[..close];
            let looks_like_key = (literal.starts_with("error.")
                || literal.starts_with("operation."))
                && literal
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_');
            if looks_like_key {
                out.insert(literal.to_string());
            }
            rest = &after[close + 1..];
        }
    }

    fn emitted_keys() -> BTreeSet<String> {
        let mut files = Vec::new();
        for dir in SCANNED_DIRS {
            collect_rust_files(Path::new(dir), &mut files);
        }
        files.extend(SCANNED_FILES.iter().map(PathBuf::from));

        let mut keys = BTreeSet::new();
        for file in files {
            let Some(source) = production_source(&file) else {
                continue;
            };
            for line in source.lines() {
                // Doc comments legitimately mention key names, so only executable lines count.
                if line.trim_start().starts_with("//") {
                    continue;
                }
                keys_in_line(line, &mut keys);
            }
        }
        keys
    }

    #[test]
    fn the_registry_covers_every_key_production_code_emits() {
        let registry: BTreeSet<String> = USER_FACING_MESSAGE_KEYS
            .iter()
            .map(|k| k.to_string())
            .collect();
        let missing: Vec<_> = emitted_keys().difference(&registry).cloned().collect();
        assert!(
            missing.is_empty(),
            "these message_keys are emitted but not registered (add them here and to all four locales): {missing:?}"
        );
    }

    #[test]
    fn the_registry_has_no_dead_entries() {
        let emitted = emitted_keys();
        let dead: Vec<_> = USER_FACING_MESSAGE_KEYS
            .iter()
            .filter(|key| !emitted.contains(**key))
            .collect();
        assert!(
            dead.is_empty(),
            "these registered keys are never emitted; delete them instead of translating them: {dead:?}"
        );
    }

    #[test]
    fn the_registry_is_sorted_and_free_of_duplicates() {
        let mut sorted = USER_FACING_MESSAGE_KEYS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.as_slice(), USER_FACING_MESSAGE_KEYS);
        assert_eq!(USER_FACING_MESSAGE_KEYS.len(), 329);
    }
}
