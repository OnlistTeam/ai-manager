use crate::compat::ccswitch::install_probe::{InstallSource, LifecycleProbe};
use crate::compat::ccswitch::lifecycle_specs::{
    codex_repair_plan, compute_install_plan, compute_uninstall_plan, compute_update_plan,
};
use crate::domain::{AppError, ErrorCode, ToolId};
use crate::platform::plan::{LifecyclePlan, UninstallPlan};
// Used only by the POSIX `mod tests` below through `super::tool_program`; the Windows test
// target does not compile that module, so the import must use the same condition.
#[cfg(all(test, not(target_os = "windows")))]
use crate::compat::ccswitch::lifecycle_specs::tool_program;

/// Official installer scripts. The product versions for Claude / OpenCode only add
/// `curl --connect-timeout 10 --max-time 120` on top of the de-shelled scripts in upstream
/// `commands/misc.rs:446`/`:448` (ADR-0033: a blocked domain must fail within seconds and move on
/// to the next attempt; the drift test pins that relationship down). The Codex script is a copy
/// from the openai/codex repository identical to the published install.sh, with
/// `CODEX_INSTALLER_USE_RELEASES_OPENAI_COM=false` forced so it goes through GitHub Releases. The
/// planning logic of the three actions lives in `lifecycle_specs` (split out purely to stay under
/// the per-file line limit; it has no independent public contract and the `pub fn`s here forward
/// to it verbatim).
pub const CLAUDE_INSTALL_SCRIPT: &str =
    "tmp=$(mktemp) && curl -fsSL --connect-timeout 10 --max-time 120 https://claude.ai/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status";
pub const OPENCODE_INSTALL_SCRIPT: &str =
    "tmp=$(mktemp) && curl -fsSL --connect-timeout 10 --max-time 120 https://opencode.ai/install -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status";
pub const CODEX_INSTALL_SCRIPT: &str =
    "tmp=$(mktemp) && curl -fsSL --connect-timeout 10 --max-time 120 https://raw.githubusercontent.com/openai/codex/main/scripts/install/install.sh -o $tmp && CODEX_NON_INTERACTIVE=1 CODEX_INSTALLER_USE_RELEASES_OPENAI_COM=false sh $tmp; status=$?; rm -f $tmp; exit $status";
pub const GROK_INSTALL_SCRIPT: &str =
    "tmp=$(mktemp) && curl -fsSL https://x.ai/cli/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status";
pub const HERMES_INSTALL_SCRIPT: &str =
    "tmp=$(mktemp) && curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status";
pub const KIMI_INSTALL_SCRIPT: &str =
    "tmp=$(mktemp) && curl -fsSL https://code.kimi.com/kimi-code/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status";
pub(crate) const OFFICIAL_NPM_REGISTRY: &str = "https://registry.npmjs.org";
pub(crate) const COMMUNITY_NPM_REGISTRY: &str = "https://registry.npmmirror.com";

pub fn install_plan(id: ToolId, probe: &LifecycleProbe) -> Result<LifecyclePlan, AppError> {
    compute_install_plan(id, probe)
}

/// `target_version` is the version the checked preview authorised; package
/// managers install exactly that, so the post-update verification can hold.
pub fn update_plan(
    id: ToolId,
    probe: &LifecycleProbe,
    target_version: &str,
) -> Result<LifecyclePlan, AppError> {
    compute_update_plan(id, probe, target_version)
}

pub fn install_version_plan(
    id: ToolId,
    probe: &LifecycleProbe,
    version: &str,
) -> Result<LifecyclePlan, AppError> {
    crate::compat::ccswitch::versioning::install_version_plan(id, probe, version)
}

/// A repair is deliberately narrower than an update. Today the only proven,
/// reversible recovery is the CC Switch POSIX Codex npm repair: when the
/// launcher is present but its platform package is missing, remove that same
/// npm package and reinstall it through the npm executable beside the broken
/// launcher. Windows file-lock/staging failures remain fail-closed.
pub fn repair_plan(id: ToolId, probe: &LifecycleProbe) -> Result<LifecyclePlan, AppError> {
    let Some(entry) = probe.entry.as_ref() else {
        return Err(
            AppError::new(ErrorCode::ToolNotFound, "error.tool.notInstalled")
                .with_technical(format!("no default install for {}", id.as_str())),
        );
    };
    if id == ToolId::Codex && !entry.runnable && entry.source == InstallSource::NodeManagerNpm {
        if let Some(plan) = codex_repair_plan(entry, probe) {
            return Ok(plan);
        }
    }
    Err(
        AppError::new(ErrorCode::UpdateFailed, "error.tool.actionUnsupported")
            .with_technical(format!(
                "no safe repair plan for {} from {:?} (runnable={})",
                id.as_str(),
                entry.source,
                entry.runnable
            ))
            .with_remediation("error.remediation.installManually"),
    )
}

pub fn uninstall_plan(id: ToolId, probe: &LifecycleProbe) -> Result<UninstallPlan, AppError> {
    compute_uninstall_plan(id, probe)
}

/// Compatibility boundary for the inherited global proxy state. The
/// application layer receives only the current URL value and never reaches
/// into the legacy proxy module directly.
pub fn configured_proxy_url() -> Option<String> {
    crate::proxy::http_client::get_current_proxy_url()
}

// The `program_path` values in the assertions are all POSIX absolute paths and the Windows version
// of `sibling_program` reads the filesystem; the whole module is therefore compiled on non-Windows
// platforms only, consistent with `install_probe`/`executor`.
#[cfg(all(test, not(target_os = "windows")))]
#[path = "lifecycle/tests.rs"]
mod tests;

#[cfg(all(test, not(target_os = "windows")))]
#[path = "lifecycle_expanded_tests.rs"]
mod expanded_tests;
