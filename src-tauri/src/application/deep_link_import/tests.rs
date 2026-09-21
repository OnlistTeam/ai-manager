use super::{block_reason, can_manage, mcp_draft, project, supported_tools};
use crate::application::deep_link_import::queue::{DeepLinkQueue, PendingDeepLink};
use crate::domain::deep_link::parse;
use crate::domain::{
    DeepLinkBlockReason, DeepLinkCredentialField, DeepLinkIntent, DeepLinkResource, LinkOrigin,
    McpConnectionDraft, ToolId,
};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

fn intent(link: &str) -> DeepLinkIntent {
    parse(link, LinkOrigin::Paste).unwrap_or_else(|error| panic!("{link}: {}", error.message_key))
}

fn pending(link: &str) -> PendingDeepLink {
    PendingDeepLink {
        id: "pending-1".to_string(),
        origin: LinkOrigin::Paste,
        intent: intent(link),
        expires_at: 600,
    }
}

const PROVIDER_WITH_KEY: &str = "aimanager://v1/import?resource=provider&app=claude&name=Example&endpoint=https://api.example.test/v1&apiKey=sk-ant-0123456789";

#[test]
fn every_resource_is_gated_by_the_capability_table_and_not_by_a_tool_list() {
    for tool in ToolId::ALL {
        let capabilities = crate::compat::ccswitch::tools::capabilities_for(tool);
        assert_eq!(
            can_manage(DeepLinkResource::Provider, tool),
            capabilities.can_manage_provider,
            "{tool:?}"
        );
        assert_eq!(
            can_manage(DeepLinkResource::Mcp, tool),
            capabilities.can_manage_mcp,
            "{tool:?}"
        );
        assert_eq!(
            can_manage(DeepLinkResource::Skill, tool),
            capabilities.can_manage_skills,
            "{tool:?}"
        );
        assert_eq!(
            can_manage(DeepLinkResource::Prompt, tool),
            capabilities.can_manage_prompts,
            "{tool:?}"
        );
    }
}

#[test]
fn the_preview_names_the_change_and_never_the_credential() {
    let preview = project(&pending(PROVIDER_WITH_KEY));

    assert_eq!(preview.id, "pending-1");
    assert_eq!(preview.origin, LinkOrigin::Paste);
    assert_eq!(preview.resource, DeepLinkResource::Provider);
    assert_eq!(preview.name.as_deref(), Some("Example"));
    assert_eq!(
        preview.endpoint.as_deref(),
        Some("https://api.example.test/v1")
    );
    assert_eq!(
        preview.credential_fields,
        vec![DeepLinkCredentialField::ApiKey]
    );
    assert_eq!(preview.targets.len(), 1);
    assert_eq!(preview.targets[0].tool, Some(ToolId::ClaudeCode));
    assert!(preview.targets[0].supported);
    assert!(preview.blocked.is_none());

    let wire = serde_json::to_string(&preview).expect("serialize the preview");
    assert!(!wire.contains("sk-ant-0123456789"), "{wire}");
}

#[test]
fn a_link_for_an_application_the_product_does_not_manage_is_blocked_without_echoing_it() {
    let preview = project(&pending(
        "aimanager://v1/import?resource=provider&app=someoneelse&name=Example&endpoint=https://api.example.test&apiKey=sk-ant-0123456789",
    ));

    assert_eq!(preview.blocked, Some(DeepLinkBlockReason::NoSupportedTool));
    assert_eq!(preview.targets.len(), 1);
    assert_eq!(preview.targets[0].tool, None);
    assert!(!preview.targets[0].supported);
    assert!(!serde_json::to_string(&preview)
        .expect("serialize")
        .contains("someoneelse"));
}

#[test]
fn a_service_link_without_a_key_anywhere_is_blocked_before_the_button_is_offered() {
    let without_key = intent(
        "aimanager://v1/import?resource=provider&app=claude&name=Example&endpoint=https://api.example.test",
    );
    assert_eq!(
        block_reason(&without_key),
        Some(DeepLinkBlockReason::CredentialRequired)
    );

    // The key may sit in the tool-native config instead of the URL parameter.
    let config = STANDARD.encode(r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"sk-ant-0123456789"}}"#);
    let in_config = intent(&format!(
        "aimanager://v1/import?resource=provider&app=claude&name=Example&endpoint=https://api.example.test&config={config}"
    ));
    assert_eq!(block_reason(&in_config), None);

    assert_eq!(block_reason(&intent(PROVIDER_WITH_KEY)), None);
}

#[test]
fn a_config_only_service_link_still_shows_its_address_in_the_preview() {
    let config = STANDARD.encode(
        r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"sk-ant-0123456789","ANTHROPIC_BASE_URL":"https://api.example.test"}}"#,
    );
    let preview = project(&pending(&format!(
        "aimanager://v1/import?resource=provider&app=claude&name=Example&config={config}"
    )));

    assert_eq!(
        preview.endpoint.as_deref(),
        Some("https://api.example.test")
    );
    assert_eq!(
        preview.credential_fields,
        vec![DeepLinkCredentialField::Config]
    );
    assert!(!serde_json::to_string(&preview)
        .expect("serialize")
        .contains("sk-ant-0123456789"));
}

#[test]
fn an_mcp_link_lists_its_servers_and_every_managed_target() {
    let config = STANDARD
        .encode(r#"{"mcpServers":{"files":{"command":"npx","args":["-y","server-filesystem"]}}}"#);
    let link = format!("aimanager://v1/import?resource=mcp&apps=claude,kimi&config={config}");
    let preview = project(&pending(&link));

    assert_eq!(preview.items, vec!["files".to_string()]);
    assert_eq!(preview.targets.len(), 2);
    assert!(preview.targets[0].supported, "Claude Code manages MCP");
    assert!(
        !preview.targets[1].supported,
        "a lifecycle-only tool cannot accept an MCP server"
    );
    assert_eq!(supported_tools(&intent(&link)), vec![ToolId::ClaudeCode]);
    assert!(preview.blocked.is_none(), "one usable target is enough");
}

#[test]
fn a_skill_preview_shows_the_skill_and_the_repository_it_comes_from() {
    let preview = project(&pending(
        "aimanager://v1/import?resource=skill&repo=anthropics/skills&directory=skills/code-review",
    ));

    assert_eq!(preview.resource, DeepLinkResource::Skill);
    assert_eq!(preview.name.as_deref(), Some("code-review"));
    assert_eq!(preview.items, vec!["anthropics/skills".to_string()]);
    assert!(preview.credential_fields.is_empty());
}

#[test]
fn mcp_servers_become_the_product_typed_draft_without_a_free_form_payload() {
    let DeepLinkIntent::Mcp(mcp) = intent(&format!(
        "aimanager://v1/import?resource=mcp&apps=claude&config={}",
        STANDARD.encode(
            r#"{"mcpServers":{"docs":{"url":"https://mcp.example.test/v1","type":"sse"}}}"#
        )
    )) else {
        panic!("expected an MCP intent");
    };

    let draft = mcp_draft(&mcp.servers[0]);
    assert_eq!(draft.name, "docs");
    assert!(draft.description.is_none());
    // `McpConnectionDraft` stays inbound-only and has no `Debug`, so compare by
    // shape rather than by formatting it.
    assert!(matches!(
        &draft.connection,
        McpConnectionDraft::Sse { url } if url == "https://mcp.example.test/v1"
    ));
    draft.validate().expect("the product draft accepts it");
}

#[test]
fn a_queued_link_is_projected_the_same_way_through_the_queue() {
    let queue = DeepLinkQueue::new();
    let id = queue.push(LinkOrigin::Paste, intent(PROVIDER_WITH_KEY), 0);

    let listed = queue.project(0, project);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, id);
    assert_eq!(listed[0].expires_at, 600);
}
