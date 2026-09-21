//! Update checks that can tell "checked, nothing new" from "could not check".
//!
//! Upstream `SkillService::check_updates` downloads every repository that
//! backs an installed Skill and, when a download fails or times out, logs and
//! `continue`s — the caller receives an empty list either way, and the
//! renderer would show "everything is up to date" for a check that never
//! happened. The product runs the same per-repository check itself so that an
//! unreachable repository surfaces as `error.skill.updateCheckFailed`.
//!
//! The upstream helpers used here (`download_repo`, `scan_dir_recursive`,
//! `resolve_skill_source_dir`, `local_hash_for_update_check` and the
//! `SkillRepoDelivery` type in `download_repo`'s signature) were opened from
//! private to `pub(crate)`; their logic is unchanged.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use tokio::time::timeout;

use crate::app_config::InstalledSkill;
use crate::database::Database;
use crate::domain::{AppError, SkillUpdate};
use crate::services::skill::{skill_state_read_guard, SkillRepo, SkillService};

use super::update_check_failed;

/// Same budget upstream gives one repository download.
const REPOSITORY_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub(super) struct RepositoryGroup {
    pub(super) repo: SkillRepo,
    pub(super) skills: Vec<InstalledSkill>,
}

/// Group repository-backed Skills by (owner, name, branch) so each repository
/// is downloaded once; local Skills have nothing to check. The order is
/// deterministic so a failure reads the same way every time.
pub(super) fn group_by_repository(
    skills: impl IntoIterator<Item = InstalledSkill>,
) -> Vec<RepositoryGroup> {
    let mut groups: BTreeMap<(String, String, String), Vec<InstalledSkill>> = BTreeMap::new();
    for skill in skills {
        let (owner, name, branch) = match (&skill.repo_owner, &skill.repo_name, &skill.repo_branch)
        {
            (Some(owner), Some(name), Some(branch)) => {
                (owner.clone(), name.clone(), branch.clone())
            }
            (Some(owner), Some(name), None) => (owner.clone(), name.clone(), "main".to_string()),
            _ => continue,
        };
        groups.entry((owner, name, branch)).or_default().push(skill);
    }
    groups
        .into_iter()
        .map(|((owner, name, branch), skills)| RepositoryGroup {
            repo: SkillRepo {
                owner,
                name,
                branch,
                enabled: true,
            },
            skills,
        })
        .collect()
}

pub(super) async fn check(
    service: &SkillService,
    db: &Arc<Database>,
) -> Result<Vec<SkillUpdate>, AppError> {
    let skills = db.get_all_installed_skills().map_err(update_check_failed)?;
    let groups = group_by_repository(skills.into_values());
    if groups.is_empty() {
        return Ok(Vec::new());
    }
    let ssot_dir = SkillService::get_ssot_dir().map_err(update_check_failed)?;

    let mut updates = Vec::new();
    let mut unreachable = Vec::new();
    for group in &groups {
        match check_repository(service, db, &ssot_dir, group).await {
            Ok(found) => updates.extend(found),
            Err(error) => {
                log::warn!(
                    "[Skill updates] {}/{} could not be checked: {error:#}",
                    group.repo.owner,
                    group.repo.name
                );
                unreachable.push(format!("{}/{}", group.repo.owner, group.repo.name));
            }
        }
    }
    require_every_repository_checked(&unreachable, groups.len())?;
    Ok(updates)
}

/// One unreachable repository fails the whole check: the domain result is a
/// plain list, and a partial list would still read as "everything else is
/// current" for the Skills that were never compared.
fn require_every_repository_checked(unreachable: &[String], total: usize) -> Result<(), AppError> {
    if unreachable.is_empty() {
        return Ok(());
    }
    Err(update_check_failed(format!(
        "{} of {total} repositories could not be checked: {}",
        unreachable.len(),
        unreachable.join(", ")
    )))
}

/// The body of one upstream `check_updates` iteration, with every failure
/// returned instead of skipped.
async fn check_repository(
    service: &SkillService,
    db: &Arc<Database>,
    ssot_dir: &Path,
    group: &RepositoryGroup,
) -> anyhow::Result<Vec<SkillUpdate>> {
    let (temp_guard, _resolved_branch, _delivery) = timeout(
        REPOSITORY_DOWNLOAD_TIMEOUT,
        service.download_repo(&group.repo),
    )
    .await
    .map_err(|_| {
        anyhow!(
            "download timed out after {}s",
            REPOSITORY_DOWNLOAD_TIMEOUT.as_secs()
        )
    })??;
    let temp_dir = temp_guard.path();

    let mut remote = Vec::new();
    service.scan_dir_recursive(temp_dir, temp_dir, &group.repo, &mut remote)?;

    // Remote I/O is done; keep the local SSOT and rows stable while hashing.
    let _state_guard = skill_state_read_guard();
    let mut updates = Vec::new();
    for skill in &group.skills {
        let remote_dir = remote
            .iter()
            .find(|candidate| {
                let install_name = candidate
                    .directory
                    .rsplit('/')
                    .next()
                    .unwrap_or(&candidate.directory);
                install_name.eq_ignore_ascii_case(&skill.directory)
            })
            .and_then(|candidate| {
                SkillService::resolve_skill_source_dir(temp_dir, &candidate.directory)
            });
        // A Skill the repository no longer ships is not an update.
        let Some(remote_dir) = remote_dir else {
            continue;
        };
        let remote_hash = SkillService::compute_dir_hash(&remote_dir)?;
        let local_hash = SkillService::local_hash_for_update_check(
            ssot_dir,
            &skill.directory,
            skill.content_hash.as_deref(),
        )
        .map(|(hash, freshly_computed)| {
            if freshly_computed {
                let _ = db.update_skill_hash(&skill.id, &hash, 0);
            }
            hash
        });
        if local_hash.as_deref() != Some(remote_hash.as_str()) {
            updates.push(SkillUpdate {
                id: skill.id.clone(),
                name: skill.name.clone(),
            });
        }
    }
    Ok(updates)
}

#[cfg(test)]
mod tests {
    use super::{group_by_repository, require_every_repository_checked};
    use crate::app_config::{InstalledSkill, SkillApps};
    use crate::domain::ErrorCode;

    fn skill(
        id: &str,
        owner: Option<&str>,
        name: Option<&str>,
        branch: Option<&str>,
    ) -> InstalledSkill {
        InstalledSkill {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            directory: id.to_string(),
            repo_owner: owner.map(str::to_string),
            repo_name: name.map(str::to_string),
            repo_branch: branch.map(str::to_string),
            readme_url: None,
            apps: SkillApps::default(),
            installed_at: 1,
            content_hash: None,
            updated_at: 0,
        }
    }

    #[test]
    fn skills_are_grouped_per_repository_and_local_ones_are_skipped() {
        let groups = group_by_repository([
            skill("review", Some("anthropics"), Some("skills"), None),
            skill("docs", Some("anthropics"), Some("skills"), Some("main")),
            skill("local", None, None, None),
            skill("deploy", Some("acme"), Some("ops"), Some("release")),
        ]);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].repo.owner, "acme");
        assert_eq!(groups[0].repo.branch, "release");
        assert_eq!(groups[0].skills.len(), 1);
        assert_eq!(groups[1].repo.owner, "anthropics");
        assert_eq!(
            groups[1].repo.branch, "main",
            "missing branch defaults to main"
        );
        assert_eq!(groups[1].skills.len(), 2);
    }

    #[test]
    fn an_unchecked_repository_fails_the_check_with_a_stable_key() {
        require_every_repository_checked(&[], 3).expect("every repository checked");
        let error = require_every_repository_checked(&["acme/ops".to_string()], 3)
            .expect_err("one unreachable repository");
        assert_eq!(error.code, ErrorCode::UpstreamError);
        assert_eq!(error.message_key, "error.skill.updateCheckFailed");
        assert!(error
            .technical_message
            .expect("technical detail")
            .contains("1 of 3 repositories"));
    }
}
