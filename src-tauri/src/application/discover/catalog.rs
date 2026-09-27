//! The native shape of a server the Discover section offers, and the
//! featured list shown before anything is searched for.
//!
//! Every featured entry was checked by hand: its package is published under
//! that name, or its endpoint answers. Descriptions are product copy and live
//! in the locales (`discover.mcp.featured.<id>`), not here.

// The builders below read as table rows; naming every column would double
// the length of the list without making a row easier to check.
#![allow(clippy::too_many_arguments)]

use std::sync::OnceLock;

use crate::domain::{
    DiscoverInput, DiscoverInputKind, DiscoverInputTarget, DiscoverMcpServer, DiscoverRunner,
    DiscoverTransport,
};

/// What the connection runs: never sent to the renderer.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ServerSpec {
    pub transport: DiscoverTransport,
    pub url: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
    /// Headers with a fixed value, sent as they are.
    pub headers: Vec<(String, String)>,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct MarketInput {
    pub view: DiscoverInput,
    /// A header's value around the typed input, such as `Bearer {}`.
    pub format: String,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct MarketServer {
    pub id: String,
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub publisher: Option<String>,
    pub icon: Option<String>,
    pub homepage: Option<String>,
    pub runs: Option<DiscoverRunner>,
    pub sign_in: bool,
    pub featured: bool,
    pub inputs: Vec<MarketInput>,
    pub spec: ServerSpec,
}

impl MarketServer {
    pub fn view(&self, added: Option<String>) -> DiscoverMcpServer {
        DiscoverMcpServer {
            id: self.id.clone(),
            name: self.name.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            publisher: self.publisher.clone(),
            icon: self.icon.clone(),
            homepage: self.homepage.is_some(),
            transport: self.spec.transport,
            runs: self.runs,
            sign_in: self.sign_in,
            featured: self.featured,
            inputs: self.inputs.iter().map(|input| input.view.clone()).collect(),
            added,
        }
    }
}

/// A GitHub owner's avatar, the picture used when a project has no logo.
pub(crate) fn owner_avatar(owner: &str) -> String {
    format!("https://github.com/{owner}.png?size=96")
}

fn remote(
    id: &str,
    title: &str,
    publisher: &str,
    icon: String,
    homepage: &str,
    url: &str,
    sign_in: bool,
    inputs: Vec<MarketInput>,
) -> MarketServer {
    MarketServer {
        id: id.to_string(),
        name: id.to_string(),
        title: title.to_string(),
        description: None,
        publisher: Some(publisher.to_string()),
        icon: Some(icon),
        homepage: Some(homepage.to_string()),
        runs: None,
        sign_in,
        featured: true,
        inputs,
        spec: ServerSpec {
            transport: DiscoverTransport::Http,
            url: Some(url.to_string()),
            command: None,
            args: Vec::new(),
            headers: Vec::new(),
        },
    }
}

fn local(
    id: &str,
    title: &str,
    publisher: &str,
    icon: String,
    homepage: &str,
    runner: DiscoverRunner,
    args: &[&str],
    inputs: Vec<MarketInput>,
) -> MarketServer {
    let command = match runner {
        DiscoverRunner::Npx => "npx",
        DiscoverRunner::Uvx => "uvx",
        DiscoverRunner::Docker => "docker",
    };
    MarketServer {
        id: id.to_string(),
        name: id.to_string(),
        title: title.to_string(),
        description: None,
        publisher: Some(publisher.to_string()),
        icon: Some(icon),
        homepage: Some(homepage.to_string()),
        runs: Some(runner),
        sign_in: false,
        featured: true,
        inputs,
        spec: ServerSpec {
            transport: DiscoverTransport::Stdio,
            url: None,
            command: Some(command.to_string()),
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            headers: Vec::new(),
        },
    }
}

fn input(
    key: &str,
    target: DiscoverInputTarget,
    kind: DiscoverInputKind,
    site: Option<&str>,
    placeholder: Option<&str>,
    secret: bool,
    required: bool,
    format: &str,
) -> MarketInput {
    MarketInput {
        view: DiscoverInput {
            key: key.to_string(),
            target,
            label: key.to_string(),
            kind: Some(kind),
            description: None,
            site: site.map(str::to_string),
            placeholder: placeholder.map(str::to_string),
            secret,
            required,
        },
        format: format.to_string(),
    }
}

fn env_key(key: &str, site: &str) -> Vec<MarketInput> {
    vec![input(
        key,
        DiscoverInputTarget::Env,
        DiscoverInputKind::ApiKey,
        Some(site),
        None,
        true,
        true,
        "{}",
    )]
}

const MCP_SERVERS_REPO: &str = "https://github.com/modelcontextprotocol/servers/tree/main/src";

fn reference(id: &str, title: &str, runner: DiscoverRunner, args: &[&str]) -> MarketServer {
    let folder = if id == "sequential-thinking" {
        "sequentialthinking"
    } else {
        id
    };
    local(
        id,
        title,
        "Model Context Protocol",
        owner_avatar("modelcontextprotocol"),
        &format!("{MCP_SERVERS_REPO}/{folder}"),
        runner,
        args,
        Vec::new(),
    )
}

fn build() -> Vec<MarketServer> {
    use DiscoverRunner::{Npx, Uvx};
    let gh = owner_avatar;
    let mut filesystem = reference(
        "filesystem",
        "Filesystem",
        Npx,
        &["-y", "@modelcontextprotocol/server-filesystem"],
    );
    filesystem.inputs = vec![input(
        "dir",
        DiscoverInputTarget::Argument,
        DiscoverInputKind::Folder,
        None,
        Some("~/code"),
        false,
        true,
        "{}",
    )];

    vec![
        remote(
            "context7",
            "Context7",
            "Upstash",
            gh("upstash"),
            "https://context7.com",
            "https://mcp.context7.com/mcp",
            false,
            vec![input(
                "CONTEXT7_API_KEY",
                DiscoverInputTarget::Header,
                DiscoverInputKind::ApiKey,
                Some("context7.com/dashboard"),
                None,
                true,
                false,
                "{}",
            )],
        ),
        local(
            "playwright",
            "Playwright",
            "Microsoft",
            "https://playwright.dev/img/playwright-logo.svg".to_string(),
            "https://github.com/microsoft/playwright-mcp",
            Npx,
            &["-y", "@playwright/mcp@latest"],
            Vec::new(),
        ),
        local(
            "chrome-devtools",
            "Chrome DevTools",
            "Google",
            gh("ChromeDevTools"),
            "https://github.com/ChromeDevTools/chrome-devtools-mcp",
            Npx,
            &["-y", "chrome-devtools-mcp@latest"],
            Vec::new(),
        ),
        remote(
            "github",
            "GitHub",
            "GitHub",
            gh("github"),
            "https://github.com/github/github-mcp-server",
            "https://api.githubcopilot.com/mcp/",
            false,
            vec![input(
                "Authorization",
                DiscoverInputTarget::Header,
                DiscoverInputKind::AccessToken,
                Some("github.com/settings/personal-access-tokens"),
                Some("github_pat_…"),
                true,
                true,
                "Bearer {}",
            )],
        ),
        remote(
            "deepwiki",
            "DeepWiki",
            "Cognition",
            gh("CognitionAI"),
            "https://deepwiki.com",
            "https://mcp.deepwiki.com/mcp",
            false,
            Vec::new(),
        ),
        remote(
            "exa",
            "Exa",
            "Exa",
            gh("exa-labs"),
            "https://docs.exa.ai/reference/exa-mcp",
            "https://mcp.exa.ai/mcp",
            false,
            Vec::new(),
        ),
        filesystem,
        reference("fetch", "Fetch", Uvx, &["mcp-server-fetch"]),
        reference(
            "memory",
            "Memory",
            Npx,
            &["-y", "@modelcontextprotocol/server-memory"],
        ),
        reference(
            "sequential-thinking",
            "Sequential Thinking",
            Npx,
            &["-y", "@modelcontextprotocol/server-sequential-thinking"],
        ),
        reference("git", "Git", Uvx, &["mcp-server-git"]),
        reference("time", "Time", Uvx, &["mcp-server-time"]),
        remote(
            "sentry",
            "Sentry",
            "Sentry",
            gh("getsentry"),
            "https://docs.sentry.io/product/sentry-mcp/",
            "https://mcp.sentry.dev/mcp",
            true,
            Vec::new(),
        ),
        remote(
            "linear",
            "Linear",
            "Linear",
            gh("linear"),
            "https://linear.app/docs/mcp",
            "https://mcp.linear.app/mcp",
            true,
            Vec::new(),
        ),
        remote(
            "notion",
            "Notion",
            "Notion",
            gh("makenotion"),
            "https://developers.notion.com/docs/mcp",
            "https://mcp.notion.com/mcp",
            true,
            Vec::new(),
        ),
        remote(
            "atlassian",
            "Atlassian",
            "Atlassian",
            gh("atlassian"),
            "https://www.atlassian.com/platform/remote-mcp-server",
            "https://mcp.atlassian.com/v1/mcp",
            true,
            Vec::new(),
        ),
        remote(
            "supabase",
            "Supabase",
            "Supabase",
            gh("supabase"),
            "https://supabase.com/docs/guides/getting-started/mcp",
            "https://mcp.supabase.com/mcp",
            true,
            Vec::new(),
        ),
        remote(
            "stripe",
            "Stripe",
            "Stripe",
            gh("stripe"),
            "https://docs.stripe.com/mcp",
            "https://mcp.stripe.com",
            true,
            Vec::new(),
        ),
        remote(
            "vercel",
            "Vercel",
            "Vercel",
            gh("vercel"),
            "https://vercel.com/docs/mcp/vercel-mcp",
            "https://mcp.vercel.com",
            true,
            Vec::new(),
        ),
        remote(
            "neon",
            "Neon",
            "Neon",
            gh("neondatabase"),
            "https://neon.com/docs/ai/neon-mcp-server",
            "https://mcp.neon.tech/mcp",
            true,
            Vec::new(),
        ),
        remote(
            "cloudflare-docs",
            "Cloudflare Docs",
            "Cloudflare",
            gh("cloudflare"),
            "https://github.com/cloudflare/mcp-server-cloudflare",
            "https://docs.mcp.cloudflare.com/mcp",
            false,
            Vec::new(),
        ),
        remote(
            "microsoft-learn",
            "Microsoft Learn",
            "Microsoft",
            gh("microsoft"),
            "https://github.com/MicrosoftDocs/mcp",
            "https://learn.microsoft.com/api/mcp",
            false,
            Vec::new(),
        ),
        remote(
            "huggingface",
            "Hugging Face",
            "Hugging Face",
            gh("huggingface"),
            "https://huggingface.co/settings/mcp",
            "https://huggingface.co/mcp",
            false,
            Vec::new(),
        ),
        local(
            "brave-search",
            "Brave Search",
            "Brave",
            gh("brave"),
            "https://github.com/brave/brave-search-mcp-server",
            Npx,
            &["-y", "@brave/brave-search-mcp-server"],
            env_key("BRAVE_API_KEY", "brave.com/search/api"),
        ),
        local(
            "firecrawl",
            "Firecrawl",
            "Firecrawl",
            gh("firecrawl"),
            "https://github.com/firecrawl/firecrawl-mcp-server",
            Npx,
            &["-y", "firecrawl-mcp"],
            env_key("FIRECRAWL_API_KEY", "firecrawl.dev/app/api-keys"),
        ),
        local(
            "tavily",
            "Tavily",
            "Tavily",
            gh("tavily-ai"),
            "https://github.com/tavily-ai/tavily-mcp",
            Npx,
            &["-y", "tavily-mcp"],
            env_key("TAVILY_API_KEY", "app.tavily.com"),
        ),
    ]
}

/// The featured servers, built once.
pub(crate) fn featured() -> &'static [MarketServer] {
    static FEATURED: OnceLock<Vec<MarketServer>> = OnceLock::new();
    FEATURED.get_or_init(build)
}

pub(crate) fn featured_server(id: &str) -> Option<&'static MarketServer> {
    featured().iter().find(|server| server.id == id)
}
