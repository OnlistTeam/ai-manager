//! The settings a route wrote into a tool's own files, and putting back only
//! those (ADR-0054).
//!
//! The inherited takeover backs up a tool's whole live config and, when the
//! route ends, writes the whole backup back. Anything the user or the tool
//! changed in those files while the route ran would be lost. Instead, when a
//! tool is routed we note which settings the route changed and to what; when
//! it ends we give the inherited restore a backup that is the *current* file
//! with only those settings put back. A setting the route wrote that has
//! since been changed by someone else is left as it is now, unless it still
//! points at the local route.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use toml_edit::{DocumentMut, Item, TableLike};

/// How one part of a tool's live config (as the inherited proxy reads it) is
/// compared and restored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LivePart {
    /// The whole live config is one JSON document.
    Document,
    /// The named field holds TOML text.
    Toml(&'static str),
    /// The named field is restored exactly as it was backed up; the
    /// inherited restore owns it (a login that must never be merged).
    AsBackedUp(&'static str),
}

/// One setting the route changed: where, and what it wrote there. `None`
/// means the route removed a setting the tool had.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct OwnedSetting {
    pub path: Vec<String>,
    pub value: Option<Value>,
}

type Leaves = BTreeMap<Vec<String>, Value>;

fn collect(value: &Value, path: &mut Vec<String>, out: &mut Leaves) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                path.push(key.clone());
                collect(child, path, out);
                path.pop();
            }
        }
        leaf => {
            out.insert(path.clone(), leaf.clone());
        }
    }
}

fn toml_as_json(text: &str) -> Option<Value> {
    let parsed: toml::Value = toml::from_str(text).ok()?;
    serde_json::to_value(parsed).ok()
}

/// Every leaf setting of the compared parts, keyed by its path. A TOML part's
/// paths start with the field name.
fn leaves(config: &Value, layout: &[LivePart]) -> Leaves {
    let mut out = Leaves::new();
    for part in layout {
        match part {
            LivePart::Document => collect(config, &mut Vec::new(), &mut out),
            LivePart::Toml(field) => {
                if let Some(tree) = config
                    .get(*field)
                    .and_then(Value::as_str)
                    .and_then(toml_as_json)
                {
                    collect(&tree, &mut vec![(*field).to_string()], &mut out);
                }
            }
            LivePart::AsBackedUp(_) => {}
        }
    }
    out
}

/// What the route changed between the tool's own config and the routed one.
pub(crate) fn owned_settings(
    backup: &Value,
    routed: &Value,
    layout: &[LivePart],
) -> Vec<OwnedSetting> {
    let before = leaves(backup, layout);
    let after = leaves(routed, layout);
    let paths: BTreeSet<&Vec<String>> = before.keys().chain(after.keys()).collect();
    paths
        .into_iter()
        .filter(|path| before.get(*path) != after.get(*path))
        .map(|path| OwnedSetting {
            path: path.clone(),
            value: after.get(path).cloned(),
        })
        .collect()
}

fn holds_marker(value: &Value, markers: &[String]) -> bool {
    value
        .as_str()
        .is_some_and(|text| markers.iter().any(|marker| text.contains(marker.as_str())))
}

/// The paths to put back: settings still as the route wrote them, and any
/// setting that still names the local route.
fn paths_to_restore(
    current: &Leaves,
    owned: &[OwnedSetting],
    markers: &[String],
) -> BTreeSet<Vec<String>> {
    let mut paths: BTreeSet<Vec<String>> = owned
        .iter()
        .filter(|setting| current.get(&setting.path) == setting.value.as_ref())
        .map(|setting| setting.path.clone())
        .collect();
    paths.extend(
        current
            .iter()
            .filter(|(_, value)| holds_marker(value, markers))
            .map(|(path, _)| path.clone()),
    );
    paths
}

/// The config to restore: the current one with only the route's own settings
/// put back as they were in `backup`.
pub(crate) fn restore_own_settings(
    backup: &Value,
    current: &Value,
    owned: &[OwnedSetting],
    layout: &[LivePart],
    markers: &[String],
) -> Value {
    let before = leaves(backup, layout);
    let restore = paths_to_restore(&leaves(current, layout), owned, markers);
    let whole_document = layout.contains(&LivePart::Document);
    // Fields outside the compared parts follow the backup, as the inherited
    // restore would; a whole-document layout has no such fields.
    let mut result = if whole_document {
        current.clone()
    } else {
        backup.clone()
    };

    for part in layout {
        match part {
            LivePart::Document => {
                for path in &restore {
                    json_put(&mut result, path, before.get(path), backup);
                }
            }
            LivePart::Toml(field) => {
                let text = toml_restore(backup, current, field, &restore);
                if let Some(object) = result.as_object_mut() {
                    object.insert((*field).to_string(), Value::String(text));
                }
            }
            LivePart::AsBackedUp(_) => {}
        }
    }
    result
}

fn json_get<'a>(root: &'a Value, path: &[String]) -> Option<&'a Value> {
    path.iter().try_fold(root, |node, key| node.get(key))
}

/// Sets `path` to `value`, or removes it (and any object left empty that the
/// backup did not have) when `value` is `None`.
fn json_put(root: &mut Value, path: &[String], value: Option<&Value>, backup: &Value) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    match value {
        Some(value) => {
            let mut node = root;
            for key in parents {
                if !node.get(key).is_some_and(Value::is_object) {
                    if let Some(object) = node.as_object_mut() {
                        object.insert(key.clone(), Value::Object(Map::new()));
                    }
                }
                let Some(next) = node.get_mut(key) else {
                    return;
                };
                node = next;
            }
            if let Some(object) = node.as_object_mut() {
                object.insert(last.clone(), value.clone());
            }
        }
        None => {
            for depth in (0..path.len()).rev() {
                let (prefix, key) = (&path[..depth], &path[depth]);
                let Some(parent) = json_get_mut(root, prefix).and_then(Value::as_object_mut) else {
                    return;
                };
                let removable = depth == path.len() - 1
                    || (parent
                        .get(key)
                        .is_some_and(|child| child.as_object().is_some_and(Map::is_empty))
                        && json_get(backup, &path[..=depth]).is_none());
                if !removable {
                    return;
                }
                parent.remove(key);
            }
        }
    }
}

fn json_get_mut<'a>(root: &'a mut Value, path: &[String]) -> Option<&'a mut Value> {
    path.iter().try_fold(root, |node, key| node.get_mut(key))
}

fn toml_get<'a>(table: &'a dyn TableLike, path: &[String]) -> Option<&'a Item> {
    let (first, rest) = path.split_first()?;
    let item = table.get(first)?;
    if rest.is_empty() {
        return Some(item);
    }
    toml_get(item.as_table_like()?, rest)
}

fn toml_get_mut<'a>(table: &'a mut dyn TableLike, path: &[String]) -> Option<&'a mut Item> {
    let (first, rest) = path.split_first()?;
    let item = table.get_mut(first)?;
    if rest.is_empty() {
        return Some(item);
    }
    toml_get_mut(item.as_table_like_mut()?, rest)
}

fn toml_set(table: &mut dyn TableLike, path: &[String], value: Item) {
    let Some((first, rest)) = path.split_first() else {
        return;
    };
    if rest.is_empty() {
        table.insert(first, value);
        return;
    }
    if !table.get(first).is_some_and(|item| item.is_table_like()) {
        let mut fresh = toml_edit::Table::new();
        fresh.set_implicit(true);
        table.insert(first, Item::Table(fresh));
    }
    if let Some(child) = table.get_mut(first).and_then(Item::as_table_like_mut) {
        toml_set(child, rest, value);
    }
}

fn toml_remove(doc: &mut DocumentMut, path: &[String], backup: &DocumentMut) {
    for depth in (0..path.len()).rev() {
        let (prefix, key) = (&path[..depth], &path[depth]);
        let parent: &mut dyn TableLike = if prefix.is_empty() {
            doc.as_table_mut()
        } else {
            match toml_get_mut(doc.as_table_mut(), prefix).and_then(Item::as_table_like_mut) {
                Some(parent) => parent,
                None => return,
            }
        };
        let removable = depth == path.len() - 1
            || (parent
                .get(key)
                .and_then(Item::as_table_like)
                .is_some_and(|child| child.is_empty())
                && toml_get(backup.as_table(), &path[..=depth]).is_none());
        if !removable {
            return;
        }
        parent.remove(key);
    }
}

fn parse_doc(config: &Value, field: &str) -> DocumentMut {
    config
        .get(field)
        .and_then(Value::as_str)
        .and_then(|text| text.parse::<DocumentMut>().ok())
        .unwrap_or_default()
}

/// The current TOML text with the restored settings taken from the backup,
/// keeping the current file's layout and comments.
fn toml_restore(
    backup: &Value,
    current: &Value,
    field: &str,
    restore: &BTreeSet<Vec<String>>,
) -> String {
    let before = parse_doc(backup, field);
    let mut doc = parse_doc(current, field);
    for path in restore {
        let Some((head, rest)) = path.split_first() else {
            continue;
        };
        if head != field || rest.is_empty() {
            continue;
        }
        match toml_get(before.as_table(), rest) {
            Some(item) => toml_set(doc.as_table_mut(), rest, item.clone()),
            None => toml_remove(&mut doc, rest, &before),
        }
    }
    doc.to_string()
}

#[cfg(test)]
#[path = "tests_owned.rs"]
mod tests;
