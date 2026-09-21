use super::mcp::extension_from_server;
use super::non_empty;
use super::prompt::extension_from_prompt;
use super::skill::{
    adoption_scopes, extension_from_skill, extension_from_unmanaged, visible_in_scope,
};
use crate::app_config::{AppType, InstalledSkill, McpApps, McpServer, SkillApps, UnmanagedSkill};
use crate::domain::{ExtensionKind, ExtensionManagement, ExtensionScope, ToolId};
use crate::prompt::Prompt;
use serde_json::json;

fn scope(tool: ToolId) -> ExtensionScope {
    ExtensionScope::tool(tool)
}

fn server(id: &str, name: &str, description: Option<&str>, apps: McpApps) -> McpServer {
    McpServer {
        id: id.to_string(),
        name: name.to_string(),
        // This big blob of free-form config is exactly what this phase keeps away from the user.
        server: json!({ "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem"] }),
        apps,
        description: description.map(str::to_string),
        homepage: None,
        docs: None,
        tags: Vec::new(),
    }
}

fn skill(id: &str, name: &str, description: Option<&str>, apps: SkillApps) -> InstalledSkill {
    InstalledSkill {
        id: id.to_string(),
        name: name.to_string(),
        description: description.map(str::to_string),
        directory: "code-review".to_string(),
        repo_owner: Some("anthropics".to_string()),
        repo_name: Some("skills".to_string()),
        repo_branch: Some("main".to_string()),
        readme_url: None,
        apps,
        installed_at: 1_700_000_000,
        content_hash: None,
        updated_at: 0,
    }
}

fn prompt(id: &str, name: &str, description: Option<&str>, enabled: bool) -> Prompt {
    Prompt {
        id: id.to_string(),
        name: name.to_string(),
        // The whole body of a CLAUDE.md. Likewise never allowed into the product wire format.
        content: "# House rules\nAlways run the tests.".to_string(),
        description: description.map(str::to_string),
        enabled,
        created_at: Some(1_700_000_000),
        updated_at: None,
    }
}

#[test]
fn an_mcp_server_reads_its_enabled_flag_out_of_the_scoped_tool() {
    let apps = McpApps {
        claude: true,
        codex: false,
        ..McpApps::default()
    };
    let raw = server("filesystem", "Filesystem", Some("Reads your files."), apps);

    let on = extension_from_server(scope(ToolId::ClaudeCode), &raw, &AppType::Claude);
    assert_eq!(on.kind, ExtensionKind::Mcp);
    assert_eq!(on.id, "filesystem");
    assert_eq!(on.scope, scope(ToolId::ClaudeCode));
    assert_eq!(on.name, "Filesystem");
    assert_eq!(on.description.as_deref(), Some("Reads your files."));
    assert!(on.enabled);
    assert!(on.can_disable);

    // The same record is off in another tool's scope — this is precisely where "a global record +
    // a per-tool switch" is projected into "a per-tool list".
    let off = extension_from_server(scope(ToolId::Codex), &raw, &AppType::Codex);
    assert_eq!(off.scope, scope(ToolId::Codex));
    assert!(!off.enabled);
}

#[test]
fn a_skill_reads_its_enabled_flag_out_of_the_scoped_tool() {
    let apps = SkillApps {
        claude: true,
        ..SkillApps::default()
    };
    let raw = skill(
        "anthropics/skills:code-review",
        "Code review",
        Some("Checks a diff before you push."),
        apps,
    );

    let on = extension_from_skill(ToolId::ClaudeCode, &raw, &AppType::Claude);
    assert_eq!(on.kind, ExtensionKind::Skill);
    assert_eq!(on.id, "anthropics/skills:code-review");
    assert_eq!(on.name, "Code review");
    assert_eq!(
        on.description.as_deref(),
        Some("Checks a diff before you push.")
    );
    assert!(on.enabled);
    assert!(on.can_disable);

    let off = extension_from_skill(ToolId::Codex, &raw, &AppType::Codex);
    assert!(!off.enabled);
}

#[test]
fn an_unmanaged_skill_is_visible_without_exposing_its_local_path() {
    let raw = UnmanagedSkill {
        directory: "mobile-app-release".to_string(),
        name: "Mobile App Release".to_string(),
        description: Some("Build and release apps safely.".to_string()),
        found_in: vec!["claude".to_string(), "codex".to_string()],
        path: "/Users/alice/.codex/skills/mobile-app-release".to_string(),
    };

    let extension = extension_from_unmanaged(ToolId::Codex, &raw);
    assert_eq!(extension.kind, ExtensionKind::Skill);
    assert_eq!(extension.id, "mobile-app-release");
    assert_eq!(extension.scope, scope(ToolId::Codex));
    assert_eq!(extension.management, ExtensionManagement::Detected);
    assert!(extension.enabled, "it already exists in the selected tool");
    assert!(
        !extension.can_disable,
        "AI Manager does not own this copy yet"
    );

    let wire = serde_json::to_string(&extension).expect("serialize local Skill");
    assert!(!wire.contains("/Users/alice"));
    assert!(!wire.contains("path"));
    assert!(!wire.contains("foundIn"));
}

#[test]
fn shared_skill_roots_are_visible_without_pretending_to_be_a_private_tool_root() {
    let shared = UnmanagedSkill {
        directory: "shared-release".to_string(),
        name: "Shared release".to_string(),
        description: None,
        found_in: vec!["agents".to_string()],
        path: "/Users/alice/.agents/skills/shared-release".to_string(),
    };
    assert!(visible_in_scope(&shared, "claude"));
    assert!(visible_in_scope(&shared, "codex"));
    let (apps, scopes) = adoption_scopes(&shared, &AppType::Claude);
    assert!(apps.claude, "the explicitly selected scope is materialized");
    assert!(!apps.codex, "unconfirmed tool scopes are not invented");
    assert_eq!(scopes, vec![AppType::Claude]);

    let tool_only = UnmanagedSkill {
        found_in: vec!["codex".to_string()],
        ..shared
    };
    assert!(visible_in_scope(&tool_only, "codex"));
    assert!(!visible_in_scope(&tool_only, "claude"));
}

#[test]
fn a_prompt_is_never_offered_as_something_you_can_switch_off() {
    // The upstream enable_prompt is single-select, and turning off the last one would write that
    // tool's instruction file as an empty file (services/prompt.rs:82-93). So this kind only offers
    // "switch to which one".
    let active = extension_from_prompt(
        ToolId::ClaudeCode,
        "house-rules",
        &prompt(
            "house-rules",
            "House rules",
            Some("What every session starts with."),
            true,
        ),
    );
    assert_eq!(active.kind, ExtensionKind::Prompt);
    assert_eq!(active.id, "house-rules");
    assert!(active.enabled);
    assert!(!active.can_disable);

    let idle = extension_from_prompt(
        ToolId::ClaudeCode,
        "minimal",
        &prompt("minimal", "Minimal", None, false),
    );
    assert!(!idle.enabled);
    assert!(!idle.can_disable);
    assert_eq!(idle.description, None);
}

#[test]
fn prompts_are_the_only_kind_that_cannot_be_switched_off() {
    let mcp = extension_from_server(
        scope(ToolId::ClaudeCode),
        &server("a", "A", None, McpApps::default()),
        &AppType::Claude,
    );
    let skill_ext = extension_from_skill(
        ToolId::ClaudeCode,
        &skill("o/r:d", "D", None, SkillApps::default()),
        &AppType::Claude,
    );
    let prompt_ext = extension_from_prompt(ToolId::ClaudeCode, "p", &prompt("p", "P", None, false));

    let switchable: Vec<(ExtensionKind, bool)> = vec![
        (mcp.kind, mcp.can_disable),
        (skill_ext.kind, skill_ext.can_disable),
        (prompt_ext.kind, prompt_ext.can_disable),
    ];
    assert_eq!(
        switchable,
        vec![
            (ExtensionKind::Mcp, true),
            (ExtensionKind::Skill, true),
            (ExtensionKind::Prompt, false),
        ]
    );
}

#[test]
fn no_conversion_ever_lets_a_configuration_payload_reach_the_wire() {
    // All three records carry free-form config or bodies. After the conversion the wire format must not contain a single byte of it.
    let mcp = extension_from_server(
        scope(ToolId::ClaudeCode),
        &server("filesystem", "Filesystem", None, McpApps::default()),
        &AppType::Claude,
    );
    let skill_ext = extension_from_skill(
        ToolId::ClaudeCode,
        &skill("o/r:code-review", "Code review", None, SkillApps::default()),
        &AppType::Claude,
    );
    let prompt_ext = extension_from_prompt(
        ToolId::ClaudeCode,
        "house-rules",
        &prompt("house-rules", "House rules", None, true),
    );

    for extension in [mcp, skill_ext, prompt_ext] {
        let json = serde_json::to_string(&extension).expect("serialize extension");
        for leaked in [
            "modelcontextprotocol",
            "npx",
            "House rules\\nAlways run the tests",
            "code-review\",\"repo",
        ] {
            assert!(
                !json.contains(leaked),
                "a configuration payload reached the wire format: {json}"
            );
        }
    }
}

#[test]
fn a_blank_description_is_absent_rather_than_an_empty_line() {
    assert_eq!(non_empty(Some("  ".to_string())), None);
    assert_eq!(non_empty(Some(String::new())), None);
    assert_eq!(non_empty(None), None);
    assert_eq!(
        non_empty(Some("  Reads your files.  ".to_string())),
        Some("Reads your files.".to_string())
    );

    let raw = server("filesystem", "Filesystem", Some("   "), McpApps::default());
    let extension = extension_from_server(scope(ToolId::ClaudeCode), &raw, &AppType::Claude);
    assert_eq!(extension.description, None);
}
