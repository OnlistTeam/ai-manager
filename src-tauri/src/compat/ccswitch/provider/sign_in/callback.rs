//! The page on this machine a vendor's browser sign-in comes back to
//! (ADR-0061). One request carries the code; everything else the browser
//! asks for (a favicon, a stale tab) gets a 404 and the wait goes on.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

use crate::domain::SignInFailure;

/// A request line and headers fit well inside this; a longer one is not a
/// browser coming back from a sign-in.
const MAX_REQUEST_BYTES: usize = 16 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) enum Arrival {
    /// The browser came back with a code; answer it once the code is spent.
    Code(String, TcpStream),
    Failed(SignInFailure),
    Canceled,
}

/// What a request to the callback asks.
#[derive(Debug, PartialEq, Eq)]
enum Request {
    Code(String),
    /// The vendor, or the user on its page, turned the sign-in down.
    Refused,
    /// A later `codex login` asking the one holding its port to stop.
    Cancel,
    /// Not ours: a favicon, another path, a stale tab's state.
    Other,
}

fn read_request(head: &str, paths: &[&str], state: &str) -> Request {
    let Some(target) = head
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("GET "))
        .and_then(|rest| rest.split(' ').next())
    else {
        return Request::Other;
    };
    let Ok(url) = url::Url::parse(&format!("http://localhost{target}")) else {
        return Request::Other;
    };
    if url.path() == "/cancel" {
        return Request::Cancel;
    }
    if !paths.contains(&url.path()) {
        return Request::Other;
    }
    let query = |name: &str| {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    };
    if query("state").as_deref() != Some(state) {
        return Request::Other;
    }
    if query("error").is_some() {
        return Request::Refused;
    }
    match query("code").filter(|code| !code.is_empty()) {
        Some(code) => Request::Code(code),
        None => Request::Other,
    }
}

async fn read_head(stream: &mut TcpStream) -> Option<String> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let read = async {
        loop {
            let n = stream.read(&mut chunk).await.ok()?;
            if n == 0 {
                return None;
            }
            buffer.extend_from_slice(&chunk[..n]);
            if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
                return Some(());
            }
            if buffer.len() > MAX_REQUEST_BYTES {
                return None;
            }
        }
    };
    tokio::time::timeout(READ_TIMEOUT, read).await.ok()??;
    String::from_utf8(buffer).ok()
}

/// Waits for the browser to come back, until `timeout`, or until canceled.
pub(super) async fn wait(
    listener: TcpListener,
    paths: &[&str],
    state: &str,
    mut cancel: watch::Receiver<bool>,
    timeout: Duration,
) -> Arrival {
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => return Arrival::Failed(SignInFailure::TimedOut),
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    return Arrival::Canceled;
                }
            }
            accepted = listener.accept() => {
                let Ok((mut stream, _)) = accepted else { continue };
                let Some(head) = read_head(&mut stream).await else { continue };
                match read_request(&head, paths, state) {
                    Request::Code(code) => return Arrival::Code(code, stream),
                    Request::Refused => {
                        reply(stream, false).await;
                        return Arrival::Failed(SignInFailure::Refused);
                    }
                    Request::Cancel => {
                        let _ = stream
                            .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                            .await;
                        return Arrival::Canceled;
                    }
                    Request::Other => {
                        let _ = stream
                            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                            .await;
                    }
                }
            }
        }
    }
}

/// The page the browser is left on: done, or not, and back to AI Manager.
/// It picks Chinese or English from the browser, the only languages a page
/// with no access to the app's setting can guess well.
pub(super) async fn reply(mut stream: TcpStream, ok: bool) {
    let body = page(ok);
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(body.as_bytes()).await;
    let _ = stream.shutdown().await;
}

fn page(ok: bool) -> String {
    let (mark, color) = if ok {
        ("✓", "#16a34a")
    } else {
        ("!", "#d97706")
    };
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>AI Manager</title>
<style>body{{margin:0;min-height:100vh;display:grid;place-items:center;font:15px/1.5 system-ui,-apple-system,"Segoe UI",sans-serif;background:#f6f7f9;color:#1f2328}}
@media (prefers-color-scheme:dark){{body{{background:#16181c;color:#e6e8eb}}}}
main{{text-align:center;padding:24px}}.mark{{font-size:28px;color:{color};font-weight:700}}h1{{font-size:17px;margin:8px 0 4px}}p{{margin:0;opacity:.7}}</style></head>
<body><main><div class="mark">{mark}</div><h1 id="t"></h1><p id="m"></p></main><script>
var zh=(navigator.language||"").toLowerCase().indexOf("zh")===0,ok={ok};
document.getElementById("t").textContent=ok?(zh?"已登录":"Signed in"):(zh?"没有登录成功":"Sign-in did not finish");
document.getElementById("m").textContent=ok?(zh?"可以关掉这个页面，回到 AI 管家。":"You can close this page and go back to AI Manager."):(zh?"回到 AI 管家再试一次。":"Go back to AI Manager and try again.");
</script></body></html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::{read_request, Request};

    const PATHS: &[&str] = &["/callback"];

    fn head(target: &str) -> String {
        format!("GET {target} HTTP/1.1\r\nHost: localhost\r\n\r\n")
    }

    #[test]
    fn only_the_callback_with_this_sign_ins_state_carries_a_code() {
        assert_eq!(
            read_request(&head("/callback?code=abc&state=s1"), PATHS, "s1"),
            Request::Code("abc".into())
        );
        assert_eq!(
            read_request(&head("/callback?code=abc&state=old"), PATHS, "s1"),
            Request::Other
        );
        assert_eq!(
            read_request(&head("/favicon.ico"), PATHS, "s1"),
            Request::Other
        );
        assert_eq!(
            read_request(&head("/callback?state=s1"), PATHS, "s1"),
            Request::Other
        );
        assert_eq!(
            read_request(
                "POST /callback?code=a&state=s1 HTTP/1.1\r\n\r\n",
                PATHS,
                "s1"
            ),
            Request::Other
        );
    }

    #[test]
    fn a_refusal_and_a_cancel_are_told_apart() {
        assert_eq!(
            read_request(&head("/callback?error=access_denied&state=s1"), PATHS, "s1"),
            Request::Refused
        );
        // An error without our state is someone else's page.
        assert_eq!(
            read_request(&head("/callback?error=access_denied"), PATHS, "s1"),
            Request::Other
        );
        assert_eq!(read_request(&head("/cancel"), PATHS, "s1"), Request::Cancel);
    }
}
