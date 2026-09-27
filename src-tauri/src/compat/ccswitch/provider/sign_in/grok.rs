//! Grok Build's own `grok login --device-auth`, run from AI Manager
//! (ADR-0061). The CLI prints a link (and a code) and finishes once the
//! browser does; the link goes to the dialog, and to the browser.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use regex::Regex;
use serde_json::Value;

use crate::compat::ccswitch::install_probe::probe;
use crate::compat::ccswitch::lifecycle_specs::{anchored, tool_program};
use crate::compat::ccswitch::tools::tool_id_to_cli_name;
use crate::domain::{SignInFailure, ToolId};
use crate::platform::executor::{
    CommandCancellation, CommandChunk, CommandExecutor, CommandObserver, SystemExecutor,
};

use super::super::ProviderStore;
use super::{Flow, Opener};

const LOGIN_TIMEOUT: Duration = Duration::from_secs(600);

fn link_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r#"https://[^\s"'<>\x1b]+"#).expect("static regex"))
}

fn code_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)code\W+([A-Z0-9]{4,}(?:-[A-Z0-9]{4,})+)").expect("static regex")
    })
}

fn ansi_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\x1b\[[0-9;?]*[A-Za-z]").expect("static regex"))
}

/// The link and the code in what the CLI printed, if it printed them.
pub(super) fn read_prompt(text: &str) -> (Option<String>, Option<String>) {
    let text = ansi_pattern().replace_all(text, "");
    let link = link_pattern()
        .find(&text)
        .map(|found| found.as_str().trim_end_matches(['.', ',', ')']).to_string());
    let code = code_pattern()
        .captures(&text)
        .and_then(|captures| captures.get(1))
        .map(|found| found.as_str().to_string());
    (link, code)
}

/// The account the CLI signed in, from its `auth.json`.
fn signed_in_email() -> Option<String> {
    let path = crate::grok_config::get_grok_config_dir().join("auth.json");
    let all: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    all.as_object()?.values().find_map(|entry| {
        let has_key = entry
            .get("key")
            .and_then(Value::as_str)
            .is_some_and(|key| !key.is_empty());
        has_key.then(|| {
            entry
                .get("email")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        })
    })
}

pub(super) async fn run(store: ProviderStore, flow: Arc<Flow>, open: Opener) {
    let probed = match probe(ToolId::GrokBuild).await {
        Ok(probed) => probed,
        Err(_) => return flow.finish(Err(SignInFailure::NotInstalled)),
    };
    let Some(entry) = probed.entry.as_ref().filter(|entry| entry.runnable) else {
        return flow.finish(Err(SignInFailure::NotInstalled));
    };
    let args = vec!["login".to_string(), "--device-auth".to_string()];
    let Some(spec) = anchored(
        tool_program(ToolId::GrokBuild),
        entry,
        tool_id_to_cli_name(ToolId::GrokBuild),
        args,
        &probed,
        true,
    ) else {
        return flow.finish(Err(SignInFailure::NotInstalled));
    };
    let spec = spec.with_timeout(LOGIN_TIMEOUT);

    let cancellation = CommandCancellation::default();
    flow.hold_command(cancellation.clone());
    let seen = flow.clone();
    let observer: CommandObserver = Arc::new(move |chunk: CommandChunk| {
        let (link, code) = read_prompt(&chunk.text);
        if let Some(code) = code {
            seen.set_code(code);
        }
        if let Some(link) = link {
            if seen.set_url(link.clone()) {
                open(&link);
            }
        }
    });
    let result = SystemExecutor
        .execute_streaming_cancellable(spec, observer, cancellation)
        .await;
    match result {
        Ok(output) if output.success => {
            let account = signed_in_email().unwrap_or_default();
            let card = {
                let _mutation = store.lock_mutation();
                super::super::tool_login::restore(&store, ToolId::GrokBuild)
            };
            match card {
                Ok(created) => flow.finish(Ok((account, created.created_provider_id))),
                Err(_) => flow.finish(Err(SignInFailure::SaveFailed)),
            }
        }
        // Canceled from the dialog: the flow already says so.
        _ if flow.is_over() => {}
        Ok(_) => flow.finish(Err(SignInFailure::Refused)),
        Err(_) => flow.finish(Err(SignInFailure::TimedOut)),
    }
}

#[cfg(test)]
mod tests {
    use super::read_prompt;

    #[test]
    fn the_link_and_code_are_read_through_the_colours() {
        let (link, code) = read_prompt(
            "\x1b[1mOpen\x1b[0m https://accounts.x.ai/device?user_code=ABCD-EFGH.\nand enter code: ABCD-EFGH\n",
        );
        assert_eq!(
            link.as_deref(),
            Some("https://accounts.x.ai/device?user_code=ABCD-EFGH")
        );
        assert_eq!(code.as_deref(), Some("ABCD-EFGH"));
    }

    #[test]
    fn plain_progress_carries_neither() {
        assert_eq!(read_prompt("Waiting for authorization..."), (None, None));
    }
}
