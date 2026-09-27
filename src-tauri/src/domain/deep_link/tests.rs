use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use super::{
    parse, DeepLinkCredentialField, DeepLinkIntent, DeepLinkMcpConnection, DeepLinkResource,
    MAX_DEEP_LINK_BYTES,
};

fn encode(value: &str) -> String {
    STANDARD.encode(value)
}

fn provider_link() -> String {
    "aimanager://v1/import?resource=provider&app=claude&name=Example&endpoint=https://api.example.test/v1"
        .to_string()
}

#[test]
fn a_minimal_provider_link_parses() {
    let intent = parse(&provider_link()).expect("valid provider link");
    assert_eq!(intent.resource(), DeepLinkResource::Provider);
    assert_eq!(intent.app_tokens(), vec!["claude"]);
    assert!(intent.credential_fields().is_empty());
    let DeepLinkIntent::Provider(provider) = intent else {
        panic!("expected a provider intent");
    };
    assert_eq!(provider.name, "Example");
    assert_eq!(
        provider.endpoint.as_deref(),
        Some("https://api.example.test/v1")
    );
    assert!(provider.api_key.is_none());
}

#[test]
fn a_credential_is_accepted_and_reported_as_a_field() {
    // ADR-0029 decision 3 (revised 2026-09-23, owner-directed): links used to
    // be refused outright when they carried a credential, which made a vendor
    // one-click import impossible — a service link without a key cannot be
    // stored at all, so every provider link was refused on one ground or the
    // other. The confirmation dialog and the "opened from outside" notice
    // carry the weight instead.
    let link = format!("{}&apiKey=sk-ant-0123456789", provider_link());

    let intent = parse(&link).expect("a link may carry a credential");
    assert_eq!(
        intent.credential_fields(),
        vec![DeepLinkCredentialField::ApiKey]
    );
    let DeepLinkIntent::Provider(provider) = intent else {
        panic!("expected a provider intent");
    };
    assert_eq!(provider.api_key.as_deref(), Some("sk-ant-0123456789"));
}

#[test]
fn a_credential_hidden_inside_config_is_reported() {
    let config = encode(r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"sk-ant-0123456789"}}"#);
    let link = format!("{}&config={config}", provider_link());

    // Detection still runs — the dialog names the field that holds the key, it
    // just does not refuse the link because of it.
    assert_eq!(
        parse(&link)
            .expect("a config credential does not block the link")
            .credential_fields(),
        vec![DeepLinkCredentialField::Config]
    );
}

#[test]
fn a_vendor_template_whose_credential_slot_is_blank_reports_no_credential() {
    let config = encode(
        r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"","ANTHROPIC_BASE_URL":"https://api.example.test"}}"#,
    );
    let link = format!("{}&config={config}", provider_link());

    let intent = parse(&link).expect("an empty slot is not a credential");
    assert!(intent.credential_fields().is_empty());
}

#[test]
fn only_the_product_scheme_is_accepted() {
    // ADR-0029 decision 1, and decision 4 withdrawn 2026-09-27: the upstream
    // scheme and a bare query were once accepted by paste; nothing pastes now.
    let bare =
        "v1/import?resource=provider&app=claude&name=Example&endpoint=https://api.example.test";
    let compatible = "ccswitch://v1/import?resource=provider&app=claude&name=Example&endpoint=https://api.example.test";

    for raw in [bare, compatible] {
        let refused = parse(raw).expect_err("only aimanager:// is accepted");
        assert_eq!(refused.message_key, "error.deepLink.unsupportedScheme");
    }
    parse("AIMANAGER://v1/import?resource=provider&app=claude&name=Example&endpoint=https://api.example.test")
        .expect("the scheme is case-insensitive");
}

#[test]
fn malicious_and_malformed_links_are_refused_with_stable_keys() {
    let oversized = format!(
        "{}&name={}",
        provider_link(),
        "x".repeat(MAX_DEEP_LINK_BYTES)
    );
    let cases: [(&str, &str); 9] = [
        (&oversized, "error.deepLink.tooLarge"),
        (
            "aimanager://v1/import?resource=provider&app=claude&name=A&name=B&endpoint=https://a.test",
            "error.deepLink.duplicateParameter",
        ),
        (
            "aimanager://v1/import?resource=provider&app=claude&name=A&endpoint=https://a.test&payload=1",
            "error.deepLink.unknownParameter",
        ),
        (
            "aimanager://v1/import?resource=provider&app=claude&name=A&endpoint=https://a.test#frag",
            "error.deepLink.invalidLink",
        ),
        (
            "aimanager://user:pass@v1/import?resource=provider&app=claude&name=A&endpoint=https://a.test",
            "error.deepLink.invalidLink",
        ),
        (
            "aimanager://v2/import?resource=provider&app=claude&name=A&endpoint=https://a.test",
            "error.deepLink.unsupportedVersion",
        ),
        (
            "aimanager://v1/install?resource=provider&app=claude&name=A&endpoint=https://a.test",
            "error.deepLink.invalidLink",
        ),
        (
            "aimanager://v1/import?resource=wallet&app=claude&name=A",
            "error.deepLink.unsupportedResource",
        ),
        (
            "aimanager://v1/import?resource=provider&app=claude&name=A&endpoint=ftp://a.test",
            "error.deepLink.invalidLink",
        ),
    ];

    for (raw, expected) in cases {
        let error = parse(raw).expect_err(raw);
        assert_eq!(error.message_key, expected, "{raw}");
    }
}

#[test]
fn a_nul_byte_never_survives_the_url_or_the_payload() {
    let embedded = parse(
        "aimanager://v1/import?resource=provider&app=claude&name=A%00B&endpoint=https://a.test",
    )
    .expect_err("a percent-encoded NUL is still a control character");
    assert_eq!(embedded.message_key, "error.deepLink.invalidLink");

    let payload = encode("standing\0instructions");
    let prompt =
        format!("aimanager://v1/import?resource=prompt&app=claude&name=Rules&content={payload}");
    let error = parse(&prompt).expect_err("a NUL inside a payload is refused");
    assert_eq!(error.message_key, "error.deepLink.invalidEncoding");
}

#[test]
fn a_prompt_link_decodes_its_content_and_redacts_it_from_debug() {
    let content = encode("# Team rules\n\nRun the tests.");
    let link =
        format!("aimanager://v1/import?resource=prompt&app=codex&name=Team%20rules&content={content}&description=Shared");

    let intent = parse(&link).expect("a prompt carries no credential");
    let DeepLinkIntent::Prompt(prompt) = &intent else {
        panic!("expected a prompt intent");
    };
    assert_eq!(prompt.name, "Team rules");
    assert_eq!(prompt.content, "# Team rules\n\nRun the tests.");
    assert_eq!(prompt.description.as_deref(), Some("Shared"));
    assert!(!format!("{intent:?}").contains("Run the tests"));
}

#[test]
fn an_mcp_link_maps_every_server_onto_the_typed_product_draft() {
    let config = encode(
        r#"{"mcpServers":{"files":{"command":"npx","args":["-y","server-filesystem"]},"docs":{"url":"https://mcp.example.test/v1","type":"sse"}}}"#,
    );
    let link = format!(
        "aimanager://v1/import?resource=mcp&apps=claude,codex,claude&config={config}&enabled=true"
    );

    let intent = parse(&link).expect("a credential-free MCP link");
    assert_eq!(intent.app_tokens(), vec!["claude", "codex"]);
    let DeepLinkIntent::Mcp(mcp) = intent else {
        panic!("expected an MCP intent");
    };
    assert_eq!(mcp.enabled, Some(true));
    assert_eq!(mcp.servers.len(), 2);
    let docs = mcp
        .servers
        .iter()
        .find(|server| server.name == "docs")
        .expect("the remote server survives");
    assert_eq!(
        docs.connection,
        DeepLinkMcpConnection::Sse {
            url: "https://mcp.example.test/v1".to_string()
        }
    );
}

#[test]
fn mcp_environment_headers_and_argument_secrets_do_not_slip_through() {
    let with_env = encode(r#"{"mcpServers":{"files":{"command":"npx","env":{"TOKEN":"abc"}}}}"#);
    let error = parse(&format!(
        "aimanager://v1/import?resource=mcp&apps=claude&config={with_env}"
    ))
    .expect_err("the product draft has no env field");
    assert_eq!(error.message_key, "error.deepLink.mcpUnsupportedFields");

    let with_arg_secret =
        encode(r#"{"mcpServers":{"files":{"command":"npx","args":["--api-key","zq83ndkwpe"]}}}"#);
    // The secret is still *detected* (it is reported as a credential field so
    // the dialog can name it); since the 2026-09-23 revision of decision 3 it
    // no longer refuses the link.
    let intent = parse(&format!(
        "aimanager://v1/import?resource=mcp&apps=claude&config={with_arg_secret}"
    ))
    .expect("an argument secret no longer blocks the link");
    assert_eq!(
        intent.credential_fields(),
        vec![DeepLinkCredentialField::Config]
    );
}

#[test]
fn a_skill_link_produces_the_catalog_identity_the_installer_expects() {
    let link = "aimanager://v1/import?resource=skill&repo=anthropics/skills&branch=main&directory=skills/code-review";
    let intent = parse(link).expect("a skill link carries no credential");
    let DeepLinkIntent::Skill(skill) = intent else {
        panic!("expected a skill intent");
    };

    assert_eq!(skill.app, "claude");
    assert_eq!(skill.catalog_id(), "anthropics/skills:skills/code-review");
    assert_eq!(skill.display_name(), "code-review");

    let without_directory = parse("aimanager://v1/import?resource=skill&repo=anthropics/skills")
        .expect_err("a repository alone does not name a skill");
    assert_eq!(
        without_directory.message_key,
        "error.deepLink.missingParameter"
    );

    let bad_repo = parse("aimanager://v1/import?resource=skill&repo=anthropics&directory=a")
        .expect_err("owner/repository is required");
    assert_eq!(bad_repo.message_key, "error.deepLink.invalidLink");
}

#[test]
fn base64_spellings_vendors_actually_emit_all_decode() {
    let payloads = [
        "eyJtY3BTZXJ2ZXJzIjp7ImEiOnsiY29tbWFuZCI6Im5weCJ9fX0=",
        "eyJtY3BTZXJ2ZXJzIjp7ImEiOnsiY29tbWFuZCI6Im5weCJ9fX0",
    ];
    for payload in payloads {
        parse(&format!(
            "aimanager://v1/import?resource=mcp&apps=claude&config={payload}"
        ))
        .unwrap_or_else(|error| panic!("{payload} must decode: {}", error.message_key));
    }

    let broken = parse("aimanager://v1/import?resource=mcp&apps=claude&config=not-base64!!")
        .expect_err("garbage is refused");
    assert_eq!(broken.message_key, "error.deepLink.invalidEncoding");
}

#[test]
fn forward_compatible_extension_parameters_are_ignored_not_rejected() {
    let link = format!("{}&x-vendor-campaign=spring", provider_link());
    parse(&link).expect("x- prefixed keys must be ignorable");
}
