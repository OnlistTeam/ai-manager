//! Discover tests: every answer comes from a fixture through a fake fetcher,
//! so nothing here touches the network or the user's home.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;

use super::catalog::{featured, MarketServer, ServerSpec};
use super::fetch::{FetchError, FetchResult, Fetcher};
use super::install::{draft_for, link_url, mcp_targets};
use super::matching::{added_server, added_skill, run_key, server_key};
use super::{icons, registry, skills_sh, DiscoverService, DiscoverState};
use crate::compat::ccswitch::discover::{McpHolding, SkillHolding};
use crate::domain::{
    DesktopAppId, DiscoverInputTarget, DiscoverInputValue, DiscoverLink, DiscoverRunner,
    DiscoverTransport, ExtensionKind, ExtensionScope, McpConnectionDraft, ToolId,
};

const REGISTRY: &str = "https://registry.test/v0/servers";
const SKILLS: &str = "https://skills.test";

#[derive(Default)]
struct FakeFetcher {
    answers: HashMap<String, Result<Vec<u8>, String>>,
    asked: Mutex<Vec<String>>,
}

impl FakeFetcher {
    fn answer(mut self, url: &str, body: &str) -> Self {
        self.answers
            .insert(url.to_string(), Ok(body.as_bytes().to_vec()));
        self
    }

    fn bytes(mut self, url: &str, body: &[u8]) -> Self {
        self.answers.insert(url.to_string(), Ok(body.to_vec()));
        self
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().expect("asked lock").clone()
    }
}

impl Fetcher for FakeFetcher {
    fn get(
        &self,
        url: String,
        _accept: &'static str,
        limit: usize,
    ) -> BoxFuture<'static, FetchResult> {
        self.asked.lock().expect("asked lock").push(url.clone());
        let answer = self
            .answers
            .get(&url)
            .cloned()
            .unwrap_or_else(|| Err("offline".to_string()));
        Box::pin(async move {
            match answer {
                Ok(body) if body.len() > limit => Err(FetchError("too large".to_string())),
                Ok(body) => Ok(body),
                Err(reason) => Err(FetchError(reason)),
            }
        })
    }
}

fn service(fetcher: Arc<FakeFetcher>, cache: &tempfile::TempDir) -> DiscoverService {
    DiscoverService::new(
        fetcher,
        REGISTRY.to_string(),
        SKILLS.to_string(),
        cache.path().to_path_buf(),
        Arc::new(DiscoverState::default()),
    )
}

fn registry_url(query: &str) -> String {
    url::Url::parse_with_params(
        REGISTRY,
        &[("search", query), ("limit", "100"), ("version", "latest")],
    )
    .expect("url")
    .to_string()
}

const REGISTRY_PAGE: &str = r#"{"servers":[
  {"server":{"name":"ai.smithery/someone-postgres","title":"Postgres (wrapped)",
    "remotes":[{"type":"streamable-http","url":"https://server.smithery.ai/pg/mcp"}]},
   "_meta":{"io.modelcontextprotocol.registry/official":{"status":"active"}}},
  {"server":{"name":"io.github.acme/postgres-mcp","description":"Query Postgres.",
    "repository":{"url":"https://github.com/acme/postgres-mcp"},
    "icons":[{"src":"https://acme.test/logo.svg"}],
    "packages":[{"registryType":"npm","identifier":"@acme/postgres-mcp",
      "transport":{"type":"stdio"},
      "environmentVariables":[
        {"name":"PG_URL","description":"Connection string","isRequired":true,"isSecret":true},
        {"name":"PG_DEBUG","description":"Optional flag"}],
      "packageArguments":[{"type":"positional","valueHint":"schema","isRequired":true}]}]}},
  {"server":{"name":"com.example/retired","packages":[{"registryType":"npm","identifier":"retired"}]},
   "_meta":{"io.modelcontextprotocol.registry/official":{"status":"deleted"}}},
  {"server":{"name":"com.example/named-only","packages":[{"registryType":"npm","identifier":"named",
      "packageArguments":[{"type":"named","name":"--token","isRequired":true}]}]}},
  {"server":{"name":"com.example/pg-remote","title":"Hosted Postgres",
    "remotes":[
      {"type":"sse","url":"https://pg.example.com/sse"},
      {"type":"streamable-http","url":"https://pg.example.com/mcp",
       "headers":[{"name":"Authorization","value":"Bearer {token}","isRequired":true,"isSecret":true},
                  {"name":"X-Client","value":"desktop"}]}]}},
  {"server":{"name":"com.example/pg-copy","packages":[{"registryType":"npm","identifier":"@acme/postgres-mcp@2.0.0"}]}},
  {"server":{"name":"io.github.dock/pg-image","packages":[{"registryType":"oci","identifier":"dock/pg:1",
      "environmentVariables":[{"name":"PG_PASSWORD","isSecret":true}]}]}}
]}"#;

// ---- registry conversion, ranking and deduplication -------------------------

#[test]
fn registry_entries_convert_by_remote_first_then_package() {
    let found = registry::parse_search(REGISTRY_PAGE.as_bytes(), "postgres").expect("parse");
    let ids = found
        .iter()
        .map(|server| server.id.as_str())
        .collect::<Vec<_>>();
    assert!(
        !ids.contains(&"com.example/retired"),
        "inactive entries are skipped"
    );
    assert!(
        !ids.contains(&"com.example/named-only"),
        "unfillable named arguments drop it"
    );
    assert!(
        !ids.contains(&"com.example/pg-copy"),
        "same package, different version, is a duplicate"
    );

    let hosted = found
        .iter()
        .find(|s| s.id == "com.example/pg-remote")
        .expect("hosted");
    assert_eq!(
        hosted.spec.transport,
        DiscoverTransport::Http,
        "streamable HTTP beats SSE"
    );
    assert_eq!(
        hosted.spec.url.as_deref(),
        Some("https://pg.example.com/mcp")
    );
    assert_eq!(
        hosted.spec.headers,
        vec![("X-Client".to_string(), "desktop".to_string())]
    );
    assert_eq!(hosted.inputs.len(), 1);
    assert_eq!(hosted.inputs[0].format, "Bearer {}");
    assert_eq!(hosted.inputs[0].view.target, DiscoverInputTarget::Header);
    assert_eq!(hosted.publisher.as_deref(), Some("example.com"));

    let npm = found
        .iter()
        .find(|s| s.id == "io.github.acme/postgres-mcp")
        .expect("npm");
    assert_eq!(npm.name, "postgres");
    assert_eq!(npm.runs, Some(DiscoverRunner::Npx));
    assert_eq!(npm.spec.command.as_deref(), Some("npx"));
    assert_eq!(
        npm.spec.args,
        vec!["-y".to_string(), "@acme/postgres-mcp".to_string()]
    );
    let keys = npm
        .inputs
        .iter()
        .map(|input| input.view.key.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        vec!["PG_URL", "arg0"],
        "optional plain variables are not asked for"
    );
    assert_eq!(npm.inputs[1].view.label, "schema");
    assert_eq!(npm.publisher.as_deref(), Some("acme"));
    assert_eq!(npm.icon.as_deref(), Some("https://acme.test/logo.svg"));
    assert_eq!(
        npm.homepage.as_deref(),
        Some("https://github.com/acme/postgres-mcp")
    );

    let oci = found
        .iter()
        .find(|s| s.id == "io.github.dock/pg-image")
        .expect("oci");
    assert_eq!(oci.runs, Some(DiscoverRunner::Docker));
    assert_eq!(
        oci.spec.args,
        vec!["run", "-i", "--rm", "-e", "PG_PASSWORD", "dock/pg:1"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        oci.icon.as_deref(),
        Some("https://github.com/dock.png?size=96")
    );
}

#[test]
fn registry_results_rank_names_first_and_wrappers_last() {
    let found = registry::parse_search(REGISTRY_PAGE.as_bytes(), "postgres").expect("parse");
    assert_eq!(
        found.first().map(|s| s.id.as_str()),
        Some("io.github.acme/postgres-mcp")
    );
    let position = |id: &str| found.iter().position(|s| s.id == id).expect(id);
    assert!(
        position("ai.smithery/someone-postgres") > position("com.example/pg-remote"),
        "a wrapper named after the query still ranks below a plain match"
    );
}

#[test]
fn registry_results_are_capped() {
    let entries = (0..40)
        .map(|index| {
            format!(
                r#"{{"server":{{"name":"com.example/s{index}","packages":[{{"registryType":"npm","identifier":"pkg-{index}"}}]}}}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let found = registry::parse_search(format!(r#"{{"servers":[{entries}]}}"#).as_bytes(), "s")
        .expect("parse");
    assert_eq!(found.len(), registry::MAX_REGISTRY_RESULTS);
}

#[test]
fn short_names_drop_the_mcp_decoration() {
    assert_eq!(
        registry::short_name("io.github.upstash/context7-mcp"),
        "context7"
    );
    assert_eq!(registry::short_name("com.example/mcp-server-git"), "git");
    assert_eq!(registry::short_name("com.example/My Server!"), "my-server");
    assert_eq!(registry::short_name("com.example/mcp"), "mcp");
}

#[test]
fn run_keys_ignore_versions_launch_paths_and_trailing_slashes() {
    let args = |values: &[&str]| {
        values
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        run_key(
            None,
            Some("/usr/local/bin/npx"),
            &args(&["-y", "@scope/pkg@latest"])
        ),
        run_key(None, Some("npx.cmd"), &args(&["@scope/pkg"]))
    );
    assert_eq!(
        run_key(Some("https://A.example/mcp/"), None, &[]),
        run_key(Some("https://a.example/mcp"), Some("ignored"), &[])
    );
    assert_eq!(
        run_key(
            None,
            Some("docker"),
            &args(&["run", "-i", "--rm", "-e", "KEY", "img"])
        ),
        "docker img"
    );
    assert_ne!(
        run_key(None, Some("uvx"), &args(&["mcp-server-git"])),
        run_key(None, Some("npx"), &args(&["mcp-server-git"]))
    );
}

// ---- "added" ----------------------------------------------------------------

fn holding(name: &str, url: Option<&str>, command: Option<&str>, args: &[&str]) -> McpHolding {
    McpHolding {
        name: name.to_string(),
        url: url.map(str::to_string),
        command: command.map(str::to_string),
        args: args.iter().map(|arg| arg.to_string()).collect(),
    }
}

#[test]
fn a_server_is_added_when_something_runs_the_same_thing_under_any_name() {
    let playwright = featured()
        .iter()
        .find(|s| s.id == "playwright")
        .expect("featured");
    let holdings = vec![
        holding(
            "browser",
            None,
            Some("npx"),
            &["-y", "@playwright/mcp@0.0.30"],
        ),
        holding("docs", Some("https://mcp.context7.com/mcp/"), None, &[]),
    ];
    assert_eq!(
        added_server(&playwright.spec, &holdings).as_deref(),
        Some("browser")
    );
    let context7 = featured()
        .iter()
        .find(|s| s.id == "context7")
        .expect("featured");
    assert_eq!(
        added_server(&context7.spec, &holdings).as_deref(),
        Some("docs")
    );
    let exa = featured().iter().find(|s| s.id == "exa").expect("featured");
    assert_eq!(added_server(&exa.spec, &holdings), None);
}

#[test]
fn a_skill_is_added_only_from_the_same_repository() {
    let holdings = vec![
        SkillHolding {
            name: "Frontend Design".to_string(),
            directory: "frontend-design".to_string(),
            repo: Some(("Anthropics".to_string(), "Skills".to_string())),
        },
        SkillHolding {
            name: "tdd".to_string(),
            directory: "tdd".to_string(),
            repo: None,
        },
    ];
    assert_eq!(
        added_skill(
            "anthropics/skills",
            "frontend-design",
            "frontend-design",
            &holdings
        )
        .as_deref(),
        Some("Frontend Design")
    );
    assert_eq!(
        added_skill("someone/else", "frontend-design", "x", &holdings),
        None
    );
    assert_eq!(
        added_skill("mattpocock/skills", "tdd", "tdd", &holdings),
        None,
        "a found Skill with no known source is not claimed"
    );
}

// ---- lists ------------------------------------------------------------------

#[tokio::test]
async fn an_empty_query_shows_featured_servers_without_a_request() {
    let cache = tempfile::tempdir().expect("cache");
    let fetcher = Arc::new(FakeFetcher::default());
    let list = service(fetcher.clone(), &cache)
        .mcp_list("", &[])
        .await
        .expect("list");
    assert_eq!(list.items.len(), featured().len());
    assert!(list
        .items
        .iter()
        .all(|item| item.featured && item.description.is_none()));
    assert!(fetcher.asked().is_empty());
    assert!(list.source_error.is_none());
}

#[tokio::test]
async fn a_search_puts_featured_matches_first_and_marks_what_is_added() {
    let cache = tempfile::tempdir().expect("cache");
    let fetcher = Arc::new(FakeFetcher::default().answer(&registry_url("git"), REGISTRY_PAGE));
    let discover = service(fetcher.clone(), &cache);
    let holdings = vec![holding("my git", None, Some("uvx"), &["mcp-server-git"])];
    let list = discover.mcp_list("Git", &holdings).await.expect("list");
    let first = list.items.first().expect("an item");
    assert!(first.featured);
    let git = list
        .items
        .iter()
        .find(|item| item.id == "git")
        .expect("git");
    assert_eq!(git.added.as_deref(), Some("my git"));
    assert!(list.items.iter().any(|item| !item.featured));
    assert!(
        discover.server("io.github.acme/postgres-mcp").is_some(),
        "shown servers can be added"
    );

    discover.mcp_list("git", &[]).await.expect("again");
    assert_eq!(
        fetcher.asked().len(),
        1,
        "a repeated search is answered from memory"
    );
}

#[tokio::test]
async fn an_unreachable_registry_still_shows_featured_matches_with_one_error() {
    let cache = tempfile::tempdir().expect("cache");
    let list = service(Arc::new(FakeFetcher::default()), &cache)
        .mcp_list("github", &[])
        .await
        .expect("list");
    assert!(list.items.iter().any(|item| item.id == "github"));
    let error = list.source_error.expect("error");
    assert_eq!(error.message_key, "error.discover.registryUnreachable");
}

const FRONT_PAGE: &str = r#"<html><script>self.__next_f.push([1,"{\"initialSkills\":[{\"source\":\"a/one\",\"skillId\":\"s1\",\"name\":\"s1\",\"installs\":9},{\"source\":\"a/one\",\"skillId\":\"s2\",\"name\":\"s2\",\"installs\":8},{\"source\":\"a/one\",\"skillId\":\"s3\",\"name\":\"s3\",\"installs\":7},{\"source\":\"a/one\",\"skillId\":\"s4\",\"name\":\"s4\",\"installs\":6},{\"source\":\"a/one\",\"skillId\":\"s5\",\"name\":\"s5\",\"installs\":5},{\"source\":\"b/two\",\"skillId\":\"pdf\",\"name\":\"pdf \\\"tools\\\"\",\"installs\":4,\"isOfficial\":true},{\"source\":\"not a repo\",\"skillId\":\"x\",\"name\":\"x\",\"installs\":1}],\"more\":1}"])</script></html>"#;

#[test]
fn the_front_page_list_is_read_out_of_its_escaped_data() {
    let list = skills_sh::extract_popular(FRONT_PAGE).expect("list");
    assert_eq!(
        list.len(),
        6,
        "an entry whose source is not owner/repo is dropped"
    );
    let pdf = list
        .iter()
        .find(|entry| entry.skill_id == "pdf")
        .expect("pdf");
    assert!(pdf.official);
    assert_eq!(pdf.name, "pdf \"tools\"");
    assert!(skills_sh::extract_popular("<html>nothing</html>").is_err());
}

#[tokio::test]
async fn the_first_skill_screen_takes_at_most_four_per_repository_and_is_kept_on_disk() {
    let cache = tempfile::tempdir().expect("cache");
    let fetcher = Arc::new(FakeFetcher::default().answer(&format!("{SKILLS}/"), FRONT_PAGE));
    let list = service(fetcher, &cache)
        .skill_list("", &[])
        .await
        .expect("list");
    let from_one = list
        .items
        .iter()
        .filter(|item| item.source == "a/one")
        .count();
    assert_eq!(from_one, 4);
    assert!(list.items.iter().any(|item| item.skill_id == "pdf"));
    assert_eq!(list.items[0].icon, "https://github.com/a.png?size=96");
    assert!(cache.path().join("skills.json").is_file());

    // Offline next time: the copy on disk is used, and the page says so once.
    let offline = service(Arc::new(FakeFetcher::default()), &cache)
        .skill_list("", &[])
        .await
        .expect("offline list");
    assert!(offline.items.iter().any(|item| item.skill_id == "pdf"));
    assert_eq!(
        offline.source_error.map(|error| error.message_key),
        Some("error.discover.skillsUnreachable".to_string())
    );
}

#[tokio::test]
async fn with_nothing_cached_the_shipped_snapshot_is_shown() {
    let cache = tempfile::tempdir().expect("cache");
    let list = service(Arc::new(FakeFetcher::default()), &cache)
        .skill_list("", &[])
        .await
        .expect("list");
    assert!(!list.items.is_empty());
    assert!(list.source_error.is_some());
}

#[tokio::test]
async fn a_skill_search_appends_skills_sh_results_after_local_matches() {
    let cache = tempfile::tempdir().expect("cache");
    let search = format!("{SKILLS}/api/search?q=pdf&limit=40");
    let fetcher = Arc::new(
        FakeFetcher::default()
            .answer(&format!("{SKILLS}/"), FRONT_PAGE)
            .answer(
                &search,
                r#"{"skills":[{"source":"b/two","skillId":"pdf","name":"pdf","installs":4},
                   {"source":"c/three","skillId":"pdf-forms","name":"pdf-forms","installs":2}]}"#,
            ),
    );
    let list = service(fetcher, &cache)
        .skill_list("pdf", &[])
        .await
        .expect("list");
    let ids = list
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["b/two/pdf", "c/three/pdf-forms"]);
}

#[tokio::test]
async fn descriptions_come_from_each_page_and_are_kept() {
    let cache = tempfile::tempdir().expect("cache");
    let fetcher = Arc::new(FakeFetcher::default().answer(
        &format!("{SKILLS}/b/two/pdf"),
        r#"<head><meta name="description" content="Fill &amp; sign PDF forms &#8212; fast."></head>"#,
    ));
    let discover = service(fetcher.clone(), &cache);
    let found = discover
        .descriptions(vec!["b/two/pdf".to_string(), "../../etc".to_string()])
        .await;
    assert_eq!(
        found.get("b/two/pdf").map(String::as_str),
        Some("Fill & sign PDF forms \u{2014} fast.")
    );
    assert_eq!(found.len(), 1);

    let again = service(Arc::new(FakeFetcher::default()), &cache)
        .descriptions(vec!["b/two/pdf".to_string()])
        .await;
    assert_eq!(again.len(), 1, "a known description is read from disk");
}

// ---- icons ------------------------------------------------------------------

#[test]
fn only_pictures_the_section_offered_are_allowed() {
    assert!(icons::allowed("https://github.com/acme.png?size=96", []));
    assert!(!icons::allowed(
        "https://github.com/acme/repo.png?size=96",
        []
    ));
    assert!(!icons::allowed("http://github.com/acme.png?size=96", []));
    assert!(!icons::allowed("https://evil.test/x.png", []));
    assert!(icons::allowed(
        "https://acme.test/logo.svg",
        ["https://acme.test/logo.svg"]
    ));
}

#[test]
fn a_picture_is_recognised_by_its_bytes() {
    assert_eq!(icons::sniff(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
    assert_eq!(
        icons::sniff(b"<?xml version=\"1.0\"?><svg xmlns=\"x\"/>"),
        Some("image/svg+xml")
    );
    assert_eq!(icons::sniff(b"<!doctype html><html><svg/></html>"), None);
    assert_eq!(icons::sniff(b"{\"not\":\"an image\"}"), None);
}

#[tokio::test]
async fn icons_are_refused_fetched_sniffed_and_cached() {
    let cache = tempfile::tempdir().expect("cache");
    let avatar = "https://github.com/acme.png?size=96";
    let fetcher = Arc::new(
        FakeFetcher::default()
            .bytes(avatar, b"\x89PNG\r\n\x1a\n0000")
            .answer("https://github.com/html.png?size=96", "<html>login</html>"),
    );
    let discover = service(fetcher.clone(), &cache);
    let refused = discover
        .icon("https://evil.test/x.png")
        .await
        .expect_err("refused");
    assert_eq!(refused.message_key, "error.discover.iconRefused");
    assert!(
        fetcher.asked().is_empty(),
        "a refused address is never requested"
    );

    let data = discover.icon(avatar).await.expect("icon");
    assert!(data.starts_with("data:image/png;base64,"));
    discover.icon(avatar).await.expect("cached icon");
    assert_eq!(fetcher.asked().len(), 1, "the second read comes from disk");

    let not_image = discover
        .icon("https://github.com/html.png?size=96")
        .await
        .expect_err("not an image");
    assert_eq!(not_image.message_key, "error.discover.iconUnavailable");
}

// ---- adding -------------------------------------------------------------------

fn value(key: &str, value: &str) -> DiscoverInputValue {
    DiscoverInputValue {
        key: key.to_string(),
        value: value.to_string(),
    }
}

#[test]
fn a_draft_places_each_value_where_the_server_reads_it() {
    let github = featured()
        .iter()
        .find(|s| s.id == "github")
        .expect("github");
    let draft = draft_for(
        github,
        &[value("Authorization", " ghp_x ")],
        Some("Repos and issues."),
    )
    .expect("draft");
    assert_eq!(draft.name, "GitHub");
    assert_eq!(draft.description.as_deref(), Some("Repos and issues."));
    match draft.connection {
        McpConnectionDraft::Http { url, headers } => {
            assert_eq!(url, "https://api.githubcopilot.com/mcp/");
            assert_eq!(headers.len(), 1);
            assert_eq!(headers[0].value, "Bearer ghp_x");
        }
        _ => panic!("GitHub is remote"),
    }
    let missing = draft_for(github, &[], None).err().expect("required");
    assert_eq!(missing.message_key, "error.discover.inputRequired");
    let unknown = draft_for(github, &[value("Other", "x")], None)
        .err()
        .expect("unknown");
    assert_eq!(unknown.message_key, "error.discover.inputInvalid");

    let brave = featured()
        .iter()
        .find(|s| s.id == "brave-search")
        .expect("brave");
    let draft = draft_for(brave, &[value("BRAVE_API_KEY", "k")], None).expect("draft");
    match draft.connection {
        McpConnectionDraft::Stdio {
            command,
            arguments,
            env,
        } => {
            assert_eq!(command, "npx");
            assert_eq!(arguments, vec!["-y", "@brave/brave-search-mcp-server"]);
            assert_eq!(env.len(), 1);
            assert_eq!(env[0].name, "BRAVE_API_KEY");
        }
        _ => panic!("Brave is local"),
    }
    draft_for(brave, &[value("BRAVE_API_KEY", "k")], None)
        .expect("draft")
        .validate()
        .expect("a Discover draft passes the guided install validation");
}

#[test]
fn a_registry_server_keeps_its_own_description() {
    let found = registry::parse_search(REGISTRY_PAGE.as_bytes(), "postgres").expect("parse");
    let npm = found
        .iter()
        .find(|s| s.id == "io.github.acme/postgres-mcp")
        .expect("npm");
    let draft = draft_for(
        npm,
        &[value("PG_URL", "postgres://x"), value("arg0", "public")],
        Some("ignored"),
    )
    .expect("draft");
    assert_eq!(draft.description.as_deref(), Some("Query Postgres."));
    match draft.connection {
        McpConnectionDraft::Stdio { arguments, .. } => {
            assert_eq!(arguments.last().map(String::as_str), Some("public"));
        }
        _ => panic!("npm packages run locally"),
    }
}

#[test]
fn targets_must_take_mcp_and_reach_the_transport() {
    let codex = ExtensionScope::tool(ToolId::Codex);
    let claude = ExtensionScope::tool(ToolId::ClaudeCode);
    let desktop = ExtensionScope::desktop_app(DesktopAppId::ClaudeDesktop);
    let reachable = [codex, claude, desktop];
    assert_eq!(
        mcp_targets(
            DiscoverTransport::Http,
            vec![claude, codex, claude],
            &reachable
        )
        .expect("ok"),
        vec![claude, codex]
    );
    let error =
        mcp_targets(DiscoverTransport::Http, vec![desktop], &reachable).expect_err("remote");
    assert_eq!(error.message_key, "error.discover.transportUnsupported");
    let error = mcp_targets(DiscoverTransport::Sse, vec![codex], &reachable).expect_err("sse");
    assert_eq!(error.message_key, "error.discover.transportUnsupported");
    let error = mcp_targets(DiscoverTransport::Stdio, Vec::new(), &reachable).expect_err("none");
    assert_eq!(error.message_key, "error.discover.targetsInvalid");
    let error =
        mcp_targets(DiscoverTransport::Stdio, vec![claude], &[codex]).expect_err("unsupported");
    assert_eq!(error.message_key, "error.discover.targetsInvalid");
}

#[test]
fn links_are_built_natively_from_validated_ids() {
    let cache = tempfile::tempdir().expect("cache");
    let discover = service(Arc::new(FakeFetcher::default()), &cache);
    assert_eq!(
        link_url(
            &discover,
            ExtensionKind::Mcp,
            "sentry",
            DiscoverLink::Homepage
        )
        .as_deref(),
        Some("https://docs.sentry.io/product/sentry-mcp/")
    );
    assert_eq!(
        link_url(
            &discover,
            ExtensionKind::Skill,
            "b/two/pdf",
            DiscoverLink::Page
        )
        .as_deref(),
        Some("https://skills.sh/b/two/pdf")
    );
    assert_eq!(
        link_url(
            &discover,
            ExtensionKind::Skill,
            "b/two/pdf",
            DiscoverLink::Repository
        )
        .as_deref(),
        Some("https://github.com/b/two")
    );
    assert_eq!(
        link_url(
            &discover,
            ExtensionKind::Skill,
            "b/../x",
            DiscoverLink::Page
        ),
        None
    );
    assert_eq!(
        link_url(
            &discover,
            ExtensionKind::Mcp,
            "unknown",
            DiscoverLink::Homepage
        ),
        None
    );
}

#[test]
fn every_featured_server_is_complete_and_unique() {
    let mut keys = std::collections::HashSet::new();
    for server in featured() {
        assert!(
            server
                .homepage
                .as_deref()
                .is_some_and(|url| url.starts_with("https://")),
            "{}",
            server.id
        );
        assert!(
            server
                .icon
                .as_deref()
                .is_some_and(|url| url.starts_with("https://")),
            "{}",
            server.id
        );
        assert!(
            keys.insert(server_key(&server.spec)),
            "{} runs what another does",
            server.id
        );
        let spec: &ServerSpec = &server.spec;
        match spec.transport {
            DiscoverTransport::Stdio => assert!(spec.command.is_some() && server.runs.is_some()),
            _ => assert!(spec
                .url
                .as_deref()
                .is_some_and(|url| url.starts_with("https://"))),
        }
    }
    let _: &[MarketServer] = featured();
}
