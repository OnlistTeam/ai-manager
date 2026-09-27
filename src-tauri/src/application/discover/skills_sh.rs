//! Reading skills.sh: its front page lists the most installed Skills, its
//! search API finds more, and each Skill's page says what it is for in its
//! `<meta name="description">`.

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

/// The front page's list of the most installed Skills. Some versions of the
/// page carry it as data (JSON inside a JavaScript string, its quotes
/// escaped); others only render it, one link per Skill. Both are read.
pub(crate) fn extract_popular(page: &str) -> Result<Vec<SkillsShEntry>, String> {
    const ESCAPED: &str = "initialSkills\\\":";
    const PLAIN: &str = "\"initialSkills\":";
    let rest = if let Some(index) = page.find(ESCAPED) {
        unescape_js(&page[index + ESCAPED.len()..])
    } else if let Some(index) = page.find(PLAIN) {
        page[index + PLAIN.len()..].to_string()
    } else {
        let rendered = rendered_rows(page);
        return if rendered.is_empty() {
            Err("the page carries no list of Skills".to_string())
        } else {
            Ok(rendered)
        };
    };
    let mut stream =
        serde_json::Deserializer::from_str(rest.trim_start()).into_iter::<Vec<SkillsShEntry>>();
    match stream.next() {
        Some(Ok(list)) => Ok(clean(list)),
        Some(Err(error)) => Err(format!("the list of Skills: {error}")),
        None => Err("the list of Skills is empty".to_string()),
    }
}

fn rendered_patterns() -> &'static (Regex, Regex, Regex) {
    static RE: OnceLock<(Regex, Regex, Regex)> = OnceLock::new();
    RE.get_or_init(|| {
        (
            Regex::new(
                r#"(?s)<a\s[^>]*href="/([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+)/([^"/?#]+)"[^>]*>(.*?)</a>"#,
            )
            .expect("constant pattern compiles"),
            Regex::new(r"(?s)<h3[^>]*>([^<]*)</h3>").expect("constant pattern compiles"),
            Regex::new(r#"<span class="font-mono[^"]*">\s*([0-9][0-9.,]*\s*[KkMmBb]?)\s*</span>"#)
                .expect("constant pattern compiles"),
        )
    })
}

/// The rendered list: each Skill is a link to `/owner/repo/skill` holding
/// its name in a heading and its install count as `3.6M`.
fn rendered_rows(page: &str) -> Vec<SkillsShEntry> {
    let (row, heading, count) = rendered_patterns();
    let mut seen = std::collections::HashSet::new();
    let list = row
        .captures_iter(page)
        .filter_map(|captures| {
            let source = format!("{}/{}", &captures[1], &captures[2]);
            let skill_id = captures[3].to_string();
            let body = &captures[4];
            let name = heading.captures(body)?.get(1)?.as_str();
            let installs = count
                .captures_iter(body)
                .last()
                .and_then(|found| compact_number(&found[1]))
                .unwrap_or(0);
            seen.insert(format!("{source}/{skill_id}"))
                .then(|| SkillsShEntry {
                    source,
                    skill_id,
                    name: unescape_html(name.trim()),
                    installs,
                    official: false,
                })
        })
        .collect();
    clean(list)
}

/// `3.6M`, `954.8K`, `1,204` as a number.
fn compact_number(text: &str) -> Option<u64> {
    let text = text.trim().replace(',', "");
    let (digits, scale) = match text.chars().last()? {
        'k' | 'K' => (&text[..text.len() - 1], 1e3),
        'm' | 'M' => (&text[..text.len() - 1], 1e6),
        'b' | 'B' => (&text[..text.len() - 1], 1e9),
        _ => (text.as_str(), 1.0),
    };
    let value = digits.trim().parse::<f64>().ok()? * scale;
    (value.is_finite() && value >= 0.0).then(|| value.round() as u64)
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
