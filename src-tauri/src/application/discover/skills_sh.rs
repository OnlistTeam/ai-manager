//! Reading skills.sh: its front page lists the most installed Skills in the
//! page's own data, its search API finds more, and each Skill's page says
//! what it is for in its `<meta name="description">`.

use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// A Skill as skills.sh lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillsShEntry {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub skill_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub installs: u64,
    #[serde(default, rename = "isOfficial")]
    pub official: bool,
}

impl SkillsShEntry {
    pub fn id(&self) -> String {
        format!("{}/{}", self.source, self.skill_id)
    }
}

/// `owner/repo`, in GitHub's own character set.
pub(crate) fn valid_source(source: &str) -> bool {
    let Some((owner, repository)) = source.split_once('/') else {
        return false;
    };
    let part = |value: &str| {
        !value.is_empty()
            && value.len() <= 100
            && value != "."
            && value != ".."
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    part(owner) && part(repository)
}

/// A skills.sh id is one path segment of printable characters.
pub(crate) fn valid_skill_id(skill_id: &str) -> bool {
    !skill_id.is_empty()
        && skill_id.len() <= 128
        && skill_id != "."
        && skill_id != ".."
        && skill_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '@'))
}

/// Splits `owner/repo/skill` into its source and id.
pub(crate) fn split_id(id: &str) -> Option<(&str, &str)> {
    let (source, skill_id) = id.rsplit_once('/')?;
    (valid_source(source) && valid_skill_id(skill_id)).then_some((source, skill_id))
}

fn keep(entry: &SkillsShEntry) -> bool {
    valid_source(&entry.source) && valid_skill_id(&entry.skill_id)
}

fn clean(mut list: Vec<SkillsShEntry>) -> Vec<SkillsShEntry> {
    list.retain(keep);
    for entry in &mut list {
        if entry.name.trim().is_empty() {
            entry.name = entry.skill_id.clone();
        }
    }
    list
}

/// The list the front page carries for itself. It sits in the page's data as
/// JSON inside a JavaScript string, so its quotes arrive escaped.
pub(crate) fn extract_popular(page: &str) -> Result<Vec<SkillsShEntry>, String> {
    const ESCAPED: &str = "initialSkills\\\":";
    const PLAIN: &str = "\"initialSkills\":";
    let rest = if let Some(index) = page.find(ESCAPED) {
        unescape_js(&page[index + ESCAPED.len()..])
    } else if let Some(index) = page.find(PLAIN) {
        page[index + PLAIN.len()..].to_string()
    } else {
        return Err("the page carries no list of Skills".to_string());
    };
    let mut stream =
        serde_json::Deserializer::from_str(rest.trim_start()).into_iter::<Vec<SkillsShEntry>>();
    match stream.next() {
        Some(Ok(list)) => Ok(clean(list)),
        Some(Err(error)) => Err(format!("the list of Skills: {error}")),
        None => Err("the list of Skills is empty".to_string()),
    }
}

/// Undoes one level of `\"` and `\\` escaping.
fn unescape_js(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character == '\\' {
            match chars.next() {
                Some(next @ ('"' | '\\')) => out.push(next),
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(character);
        }
    }
    out
}

#[derive(Deserialize)]
struct SearchAnswer {
    #[serde(default)]
    skills: Vec<SkillsShEntry>,
}

pub(crate) fn parse_search(body: &[u8]) -> Result<Vec<SkillsShEntry>, String> {
    let answer: SearchAnswer =
        serde_json::from_slice(body).map_err(|error| format!("search answer: {error}"))?;
    Ok(clean(answer.skills))
}

fn meta_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<meta\s+name="description"\s+content="([^"]*)""#)
            .expect("constant pattern compiles")
    })
}

/// What a page says of itself in `<meta name="description">`.
pub(crate) fn meta_description(page: &str) -> Option<String> {
    let raw = meta_pattern().captures(page)?.get(1)?.as_str();
    let text = unescape_html(raw)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let text = text.chars().take(500).collect::<String>();
    (!text.is_empty()).then_some(text)
}

/// The entities a `content` attribute uses.
pub(crate) fn unescape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let Some(end) = tail.find(';').filter(|end| *end <= 12) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(character) => {
                out.push(character);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The Skills shown before anything is searched for: a few from each
/// repository, so the first screen is not one author's.
pub(crate) fn spread(list: Vec<SkillsShEntry>, per_source: usize) -> Vec<SkillsShEntry> {
    let mut counts = std::collections::HashMap::<String, usize>::new();
    list.into_iter()
        .filter(|entry| {
            let count = counts.entry(entry.source.to_lowercase()).or_default();
            *count += 1;
            *count <= per_source
        })
        .collect()
}

/// The snapshot shipped with the app, for when skills.sh cannot be reached
/// and nothing was kept from an earlier visit.
pub(crate) fn snapshot() -> Vec<SkillsShEntry> {
    const SNAPSHOT: &str = include_str!("skills_snapshot.json");
    serde_json::from_str::<Vec<SkillsShEntry>>(SNAPSHOT)
        .map(clean)
        .unwrap_or_default()
}
