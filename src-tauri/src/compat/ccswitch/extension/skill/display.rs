//! The display line and portability hint for one Skill (ADR-0062).
//!
//! A Skill is a folder, so its detail is where that folder is: the single
//! stored copy for a managed Skill, the folder in the tool for a found one,
//! plus the GitHub repository it came from. The portability check reads only
//! the Skill's own SKILL.md, bounded, and looks for another tool's home folder
//! (`~/.claude/…`). The list of home folders is data: each Skill-capable
//! tool's skills folder, one level up, as upstream resolves it (including a
//! user's override directory), so no tool is named here.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::app_config::{AppType, InstalledSkill};
use crate::compat::ccswitch::tools::{capabilities_for, tool_id_to_app_type};
use crate::domain::extension_detail::{bounded, home_spellings, mentions_folder};
use crate::domain::{ExtensionPortability, ExtensionScope, PortabilityReason, ToolId};
use crate::services::SkillService;

use super::super::location::display_path;

/// Enough for any real SKILL.md; the rest of a larger file is not read.
const SKILL_DOCUMENT_READ_LIMIT: u64 = 64 * 1024;

struct ToolHome {
    spellings: Vec<String>,
    tools: Vec<ToolId>,
}

/// Everything a Skill list needs to describe its rows, resolved once per
/// list rather than once per row.
pub(in crate::compat::ccswitch::extension) struct SkillDisplay {
    /// The single stored copy managed Skills live in.
    ssot: Option<PathBuf>,
    homes: Vec<ToolHome>,
}

impl SkillDisplay {
    pub(in crate::compat::ccswitch::extension) fn current() -> Self {
        let home = crate::config::get_home_dir();
        Self {
            ssot: SkillService::get_ssot_dir().ok(),
            homes: tool_homes(&home),
        }
    }

    pub(in crate::compat::ccswitch::extension) fn ssot(&self) -> Option<&Path> {
        self.ssot.as_deref()
    }

    /// No store and no tool homes: rows get no folder line and no hint.
    #[cfg(test)]
    pub(in crate::compat::ccswitch::extension) fn empty() -> Self {
        Self {
            ssot: None,
            homes: Vec::new(),
        }
    }

    pub(in crate::compat::ccswitch::extension) fn managed(
        &self,
        raw: &InstalledSkill,
    ) -> (Option<String>, Option<ExtensionPortability>) {
        let folder = self
            .ssot
            .as_ref()
            .filter(|_| is_single_component(&raw.directory))
            .map(|root| root.join(&raw.directory));
        let repository = match (raw.repo_owner.as_deref(), raw.repo_name.as_deref()) {
            (Some(owner), Some(name)) if !owner.is_empty() && !name.is_empty() => {
                Some(format!("{owner}/{name}"))
            }
            _ => None,
        };
        self.describe(folder.as_deref(), repository)
    }

    pub(in crate::compat::ccswitch::extension) fn found(
        &self,
        folder: Option<&Path>,
    ) -> (Option<String>, Option<ExtensionPortability>) {
        self.describe(folder, None)
    }

    fn describe(
        &self,
        folder: Option<&Path>,
        repository: Option<String>,
    ) -> (Option<String>, Option<ExtensionPortability>) {
        let location = folder.map(display_path);
        let detail = match (location, repository) {
            (Some(location), Some(repository)) => Some(format!("{location} · {repository}")),
            (Some(location), None) => Some(location),
            (None, repository) => repository,
        };
        let portability = folder.and_then(|folder| self.portability(&folder.join("SKILL.md")));
        (detail.map(bounded), portability)
    }

    fn portability(&self, document: &Path) -> Option<ExtensionPortability> {
        let text = read_bounded(document)?;
        let mut works_in = Vec::new();
        for home in &self.homes {
            if mentions_folder(&text, &home.spellings) {
                works_in.extend(home.tools.iter().copied().map(ExtensionScope::tool));
            }
        }
        (!works_in.is_empty()).then_some(ExtensionPortability {
            reason: PortabilityReason::ToolHome,
            works_in,
        })
    }
}

/// A stored directory name is one plain path component; anything else is
/// not joined onto a root.
pub(in crate::compat::ccswitch::extension) fn is_single_component(directory: &str) -> bool {
    let mut components = Path::new(directory).components();
    matches!(
        (components.next(), components.next()),
        (Some(std::path::Component::Normal(_)), None)
    )
}

fn read_bounded(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(SKILL_DOCUMENT_READ_LIMIT)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Each Skill-capable tool's home folder, one entry per distinct folder.
fn tool_homes(home: &Path) -> Vec<ToolHome> {
    let mut roots: Vec<(PathBuf, Vec<ToolId>)> = Vec::new();
    for tool in ToolId::ALL {
        if !capabilities_for(tool).can_manage_skills {
            continue;
        }
        let Some(app_type) =
            tool_id_to_app_type(tool).and_then(|value| value.parse::<AppType>().ok())
        else {
            continue;
        };
        let Ok(skills) = SkillService::get_app_skills_dir(&app_type) else {
            continue;
        };
        // A skills folder directly in the home folder would make every
        // home-relative path look tool-specific.
        let Some(root) = skills.parent().filter(|root| *root != home) else {
            continue;
        };
        match roots.iter_mut().find(|(known, _)| known == root) {
            Some((_, tools)) => tools.push(tool),
            None => roots.push((root.to_path_buf(), vec![tool])),
        }
    }
    roots
        .into_iter()
        .map(|(root, tools)| {
            let relative = root
                .strip_prefix(home)
                .ok()
                .map(|relative| relative.to_string_lossy().into_owned());
            ToolHome {
                spellings: home_spellings(relative.as_deref(), &root.to_string_lossy()),
                tools,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{is_single_component, tool_homes, SkillDisplay};
    use crate::domain::{ExtensionScope, PortabilityReason, ToolId};
    use std::fs;

    #[test]
    fn only_a_plain_directory_name_is_joined_onto_the_store() {
        assert!(is_single_component("pdf"));
        for unsafe_name in ["", "..", "a/b", "/abs", "./x"] {
            assert!(!is_single_component(unsafe_name), "{unsafe_name}");
        }
    }

    #[test]
    #[serial_test::serial]
    fn a_skill_that_names_one_tools_home_works_only_there() {
        let temp = tempfile::tempdir().expect("temp home");
        let previous = std::env::var_os("AI_MANAGER_TEST_HOME");
        std::env::set_var("AI_MANAGER_TEST_HOME", temp.path());

        let homes = tool_homes(temp.path());
        let claude = homes
            .iter()
            .find(|home| home.tools.contains(&ToolId::ClaudeCode))
            .expect("Claude Code has a home folder");
        assert!(claude.spellings.contains(&"~/.claude".to_string()));

        let folder = temp.path().join("skill");
        fs::create_dir_all(&folder).expect("create Skill folder");
        fs::write(
            folder.join("SKILL.md"),
            "---\nname: x\n---\nRead ~/.claude/settings.json before you start.\n",
        )
        .expect("write SKILL.md");
        let display = SkillDisplay { ssot: None, homes };
        let (detail, portability) = display.found(Some(&folder));
        assert_eq!(detail.as_deref(), Some("~/skill"));
        let portability = portability.expect("home reference flagged");
        assert_eq!(portability.reason, PortabilityReason::ToolHome);
        assert_eq!(
            portability.works_in,
            vec![ExtensionScope::tool(ToolId::ClaudeCode)]
        );

        fs::write(folder.join("SKILL.md"), "Plain instructions.\n").expect("rewrite");
        assert!(display.found(Some(&folder)).1.is_none());

        match previous {
            Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
            None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
        }
    }
}
