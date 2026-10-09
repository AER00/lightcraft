//! Library-wide keyword commands: list (tree with counts), suggestions, rename, delete, merge;
//! keyword sets (nine keywords a keystroke away: ⌥1–⌥9) and Recent Keywords.

use lightcraft_catalog::keywords::{KeywordInfo, clean, closest, is_under, reparent};
use serde_json::{Value, json};

use super::{CommandSpec, always, bad, bool_or, cmd, str_param};
use crate::{Result, Session};

fn strs(p: &Value, key: &str) -> Vec<String> {
    match p.get(key) {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => Vec::new(),
    }
}

/// Commit a keyword batch; returns how many changes it made. The keyword filter follows a renamed
/// keyword and is cleared when its keyword is deleted.
fn commit_keywords(s: &mut Session, label: &str, op: lightcraft_catalog::Op, follow: impl Fn(&str) -> Option<String>) -> Result<Value> {
    let n = match &op {
        lightcraft_catalog::Op::Batch { ops } => ops.len(),
        _ => 1,
    };
    if n > 0 {
        s.commit(label, op)?;
    }
    if let Some(k) = s.filter.keyword.clone() {
        s.filter.keyword = follow(&k);
    }
    Ok(json!({"changed": n}))
}

/// A keyword's attributes from command params over `base`: those not given keep their value.
fn info_from(p: &Value, base: KeywordInfo) -> KeywordInfo {
    KeywordInfo {
        synonyms: if p.get("synonyms").is_some() {
            strs(p, "synonyms").into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        } else {
            base.synonyms
        },
        include_on_export: bool_or(p, "includeOnExport", base.include_on_export),
        export_containing: bool_or(p, "exportContaining", base.export_containing),
        export_synonyms: bool_or(p, "exportSynonyms", base.export_synonyms),
        person: bool_or(p, "person", base.person),
        new_keywords_inside: base.new_keywords_inside,
    }
}

/// The keyword a command names (`keyword`), as the library writes it.
fn named_keyword(s: &Session, p: &Value, command: &str) -> Result<String> {
    let k = str_param(p, "keyword").ok_or_else(|| bad(command, "missing `keyword`"))?;
    s.catalog.keyword_path(k).ok_or_else(|| bad(command, format!("no keyword “{}”", clean(k))))
}

/// A catalog refusal as a command error; a taken name says how to merge.
fn refused(command: &str, e: lightcraft_catalog::CatalogError, how_to_merge: &str) -> crate::EngineError {
    match e {
        lightcraft_catalog::CatalogError::KeywordExists(k) => bad(command, format!("there is a keyword “{k}” already{how_to_merge}")),
        e => bad(command, e.to_string()),
    }
}

/// A named set of up to nine keywords.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct KeywordSet {
    pub name: String,
    pub keywords: Vec<String>,
}

/// The set name meaning "the nine most recently added keywords".
pub const RECENT: &str = "Recent Keywords";

/// Remember `added` as the most recent keywords (newest first, nine kept).
pub fn note_recent(s: &mut Session, added: &[String]) {
    for k in added.iter().rev() {
        let k = clean(k);
        if k.is_empty() {
            continue;
        }
        s.recent_keywords.retain(|x| !x.eq_ignore_ascii_case(&k));
        s.recent_keywords.insert(0, k);
    }
    s.recent_keywords.truncate(9);
    let _ = s.save_prefs();
}

/// The nine keywords ⌥1–⌥9 apply: the current set's, or the recent ones.
pub fn current_keywords(s: &Session) -> Vec<String> {
    let set = s.keyword_set.as_deref().and_then(|n| s.keyword_sets.iter().find(|x| x.name.eq_ignore_ascii_case(n)));
    let mut v = set.map_or_else(|| s.recent_keywords.clone(), |x| x.keywords.clone());
    v.truncate(9);
    v
}

/// `{sets: [{name, keywords}], current, keywords}` (Recent Keywords first).
pub fn keyword_sets_json(s: &Session) -> Value {
    let mut sets = vec![json!({"name": RECENT, "keywords": s.recent_keywords})];
    sets.extend(s.keyword_sets.iter().map(|x| json!({"name": x.name, "keywords": x.keywords})));
    json!({"sets": sets, "current": s.keyword_set.clone().unwrap_or_else(|| RECENT.into()), "keywords": current_keywords(s)})
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "keyword.sets", "Keyword Sets", [], None, "{} → {sets: [{name, keywords}], current, keywords: the nine ⌥1–⌥9 apply}", always, |s, _| Ok(keyword_sets_json(s))),
        cmd!("keyword.useSet", "Use Keyword Set", [], None, "{name} (\"Recent Keywords\" = the recently added ones)", always, |s, p| {
            let name = str_param(p, "name").map(str::trim).unwrap_or(RECENT);
            s.keyword_set = if name.eq_ignore_ascii_case(RECENT) || name.is_empty() {
                None
            } else {
                Some(
                    s.keyword_sets
                        .iter()
                        .find(|x| x.name.eq_ignore_ascii_case(name))
                        .ok_or_else(|| bad("keyword.useSet", format!("no keyword set `{name}`")))?
                        .name
                        .clone(),
                )
            };
            s.save_prefs()?;
            Ok(keyword_sets_json(s))
        }),
        cmd!(
            "keyword.saveSet",
            "Save Keyword Set",
            [],
            None,
            "{name, keywords?: [up to 9] (default: the current nine)} — replaces a set of that name and makes it current",
            always,
            |s, p| {
                let name = str_param(p, "name")
                    .map(str::trim)
                    .filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case(RECENT))
                    .ok_or_else(|| bad("keyword.saveSet", "missing or reserved `name`"))?
                    .to_string();
                let mut keywords: Vec<String> = if p.get("keywords").is_some() {
                    strs(p, "keywords").iter().map(|k| clean(k)).filter(|k| !k.is_empty()).collect()
                } else {
                    current_keywords(s)
                };
                keywords.truncate(9);
                let set = KeywordSet { name: name.clone(), keywords };
                match s.keyword_sets.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&name)) {
                    Some(x) => *x = set,
                    None => s.keyword_sets.push(set),
                }
                s.keyword_set = Some(name);
                s.save_prefs()?;
                Ok(keyword_sets_json(s))
            }
        ),
        cmd!("keyword.deleteSet", "Delete Keyword Set", [], None, "{name}", always, |s, p| {
            let name = str_param(p, "name").ok_or_else(|| bad("keyword.deleteSet", "missing `name`"))?;
            let before = s.keyword_sets.len();
            s.keyword_sets.retain(|x| !x.name.eq_ignore_ascii_case(name));
            if s.keyword_sets.len() == before {
                return Err(bad("keyword.deleteSet", format!("no keyword set `{name}`")));
            }
            if s.keyword_set.as_deref().is_some_and(|c| c.eq_ignore_ascii_case(name)) {
                s.keyword_set = None;
            }
            s.save_prefs()?;
            Ok(keyword_sets_json(s))
        }),
        cmd!(
            "keyword.toggleFromSet",
            "Toggle Keyword from Set",
            [],
            None,
            "{index: 1..9, ids?} — the set's keyword N: added to the target photos, or removed when they all have it",
            super::has_selection,
            |s, p| {
                let i = p
                    .get("index")
                    .and_then(Value::as_u64)
                    .filter(|i| (1..=9).contains(i))
                    .ok_or_else(|| bad("keyword.toggleFromSet", "`index` must be 1..9"))?;
                let Some(k) = current_keywords(s).get(i as usize - 1).cloned() else {
                    return Ok(json!({"changed": 0}));
                };
                let ids = s.targets(p);
                let all = !ids.is_empty()
                    && ids.iter().all(|id| s.catalog.photo(*id).is_some_and(|ph| ph.meta.keywords.iter().any(|x| x.eq_ignore_ascii_case(&k))));
                let ids: Vec<u64> = ids.iter().map(|i| i.0).collect();
                let key = if all { "removeKeywords" } else { "addKeywords" };
                // applying from Recent Keywords mustn't reshuffle the numbers under the keys
                let recent = s.recent_keywords.clone();
                let mut r = s.execute("photo.setMeta", &json!({"ids": ids, key: [k.clone()]}))?;
                if s.keyword_set.is_none() {
                    s.recent_keywords = recent;
                }
                r["keyword"] = json!(k);
                r["added"] = json!(!all);
                Ok(r)
            }
        ),
        cmd!(query "keyword.list", "Keywords", [], None, "{} → [{name, path, count, children}] keyword tree (`a|b|c` keywords are hierarchical)", always, |s, _| {
            Ok(serde_json::to_value(s.catalog.keyword_tree()).unwrap_or_default())
        }),
        cmd!(
            query "keyword.suggest",
            "Keyword Suggestions",
            [],
            None,
            "{prefix?: typed text, ids?, limit?: 12} → keywords to suggest for the photos (co-occurring / most used, or matching the prefix)",
            always,
            |s, p| {
                let mut current: Vec<String> = Vec::new();
                for id in s.targets(p) {
                    if let Some(ph) = s.catalog.photo(id) {
                        current.extend(ph.meta.keywords.iter().cloned());
                    }
                }
                let n = p.get("limit").and_then(Value::as_u64).unwrap_or(12) as usize;
                Ok(json!(s.catalog.keyword_suggestions(&current, str_param(p, "prefix").unwrap_or(""), n)))
            }
        ),
        cmd!(
            query "keyword.info",
            "Keyword Info",
            [],
            None,
            "{keyword} → {path, listed, count, synonyms, includeOnExport, exportContaining, exportSynonyms, person}",
            always,
            |s, p| {
                let path = named_keyword(s, p, "keyword.info")?;
                let info = s.catalog.keyword_info(&path).cloned();
                let count = s.catalog.photos().filter(|ph| ph.in_library() && ph.meta.keywords.iter().any(|k| is_under(k, &path))).count();
                let mut v = serde_json::to_value(info.clone().unwrap_or_default()).unwrap_or_default();
                if let Some(o) = v.as_object_mut() {
                    o.insert("path".into(), json!(path));
                    o.insert("listed".into(), json!(info.is_some()));
                    o.insert("count".into(), json!(count));
                    o.entry("synonyms").or_insert(json!([]));
                }
                Ok(v)
            }
        ),
        cmd!(
            "keyword.create",
            "Create Keyword",
            [],
            None,
            "{name, parent?: keyword | null (default: the default parent; null = the top level), synonyms?, includeOnExport?, exportContaining?, exportSynonyms?, person?, addToSelected?: bool, ids?} → {keyword}",
            always,
            |s, p| {
                let name = str_param(p, "name").map(clean).filter(|n| !n.is_empty()).ok_or_else(|| bad("keyword.create", "missing `name`"))?;
                let parent = match p.get("parent") {
                    Some(Value::String(k)) => Some(s.catalog.keyword_path(k).unwrap_or_else(|| clean(k))),
                    Some(_) => None,
                    None => s.catalog.default_keyword_parent(),
                };
                let path = clean(&match parent {
                    Some(parent) => format!("{parent}|{name}"),
                    None => name,
                });
                let photos = if bool_or(p, "addToSelected", false) { s.targets(p) } else { Vec::new() };
                let op = s
                    .catalog
                    .create_keyword_ops(&path, info_from(p, KeywordInfo::default()), &photos)
                    .map_err(|e| refused("keyword.create", e, ""))?;
                s.commit("Create Keyword", op)?;
                if !photos.is_empty() {
                    note_recent(s, std::slice::from_ref(&path));
                }
                Ok(json!({"keyword": path}))
            }
        ),
        cmd!(
            "keyword.edit",
            "Edit Keyword",
            [],
            None,
            "{keyword, name?: its new name (one level), synonyms?, includeOnExport?, exportContaining?, exportSynonyms?, person?} — what isn't given keeps its value",
            always,
            |s, p| {
                let from = named_keyword(s, p, "keyword.edit")?;
                let leaf = from.rsplit('|').next().unwrap_or(&from).to_string();
                let name = str_param(p, "name").unwrap_or(&leaf).to_string();
                let info = info_from(p, s.catalog.keyword_info(&from).cloned().unwrap_or_default());
                let op =
                    s.catalog.edit_keyword_ops(&from, &name, info).map_err(|e| refused("keyword.edit", e, " (to merge them, use keyword.merge)"))?;
                let to = match from.rsplit_once('|') {
                    Some((parent, _)) => format!("{parent}|{}", name.trim()),
                    None => name.trim().to_string(),
                };
                commit_keywords(s, "Edit Keyword", op, |k| Some(if is_under(k, &from) { reparent(k, &from, &to) } else { k.to_string() }))
            }
        ),
        cmd!(
            "keyword.move",
            "Move Keyword",
            [],
            None,
            "{keyword, parent: keyword | null (the top level), merge?: bool} — with the keywords below it; onto a name that is taken only with `merge: true`",
            always,
            |s, p| {
                let from = named_keyword(s, p, "keyword.move")?;
                let parent = match p.get("parent") {
                    Some(Value::String(k)) => Some(k.clone()),
                    Some(Value::Null) => None,
                    _ => return Err(bad("keyword.move", "`parent` is a keyword, or null for the top level")),
                };
                // where it lands, as the catalog spells it: a parent that doesn't exist yet is made
                let leaf = from.rsplit('|').next().unwrap_or(&from).to_string();
                let to = match parent.as_deref().map(|k| s.catalog.keyword_path(k).unwrap_or_else(|| clean(k))).filter(|k| !k.is_empty()) {
                    Some(parent) => format!("{parent}|{leaf}"),
                    None => leaf,
                };
                let op = s
                    .catalog
                    .move_keyword_ops(&from, parent.as_deref(), bool_or(p, "merge", false))
                    .map_err(|e| refused("keyword.move", e, ": moving would merge the two (merge: true)"))?;
                commit_keywords(s, "Move Keyword", op, |k| Some(if is_under(k, &from) { reparent(k, &from, &to) } else { k.to_string() }))
            }
        ),
        cmd!(
            "keyword.purgeUnused",
            "Purge Unused Keywords",
            [],
            None,
            "{} — takes off the keyword list the keywords no photo has → {purged}",
            always,
            |s, _| {
                let op = s.catalog.purge_unused_keywords_ops();
                let r = commit_keywords(s, "Purge Unused Keywords", op, |k| Some(k.to_string()))?;
                Ok(json!({"purged": r["changed"]}))
            }
        ),
        cmd!(
            "keyword.setDefaultParent",
            "Put New Keywords Inside",
            [],
            None,
            "{keyword: keyword | null} — the parent keyword.create puts new keywords inside (null: the top level)",
            always,
            |s, p| {
                let keyword = match p.get("keyword") {
                    Some(Value::Null) | None => None,
                    Some(_) => Some(named_keyword(s, p, "keyword.setDefaultParent")?),
                };
                let op = s.catalog.set_default_parent_ops(keyword.as_deref()).map_err(|e| bad("keyword.setDefaultParent", e.to_string()))?;
                commit_keywords(s, "Put New Keywords Inside", op, |k| Some(k.to_string()))?;
                Ok(json!({"keyword": s.catalog.default_keyword_parent()}))
            }
        ),
        cmd!(
            "keyword.rename",
            "Rename Keyword",
            [],
            None,
            "{from, to} — on every photo, children included (`a` → `b` renames `a|x` to `b|x`); renaming onto an existing keyword merges them",
            always,
            |s, p| {
                let from = str_param(p, "from").ok_or_else(|| bad("keyword.rename", "missing `from`"))?.to_string();
                let to = str_param(p, "to").ok_or_else(|| bad("keyword.rename", "missing `to`"))?.to_string();
                let op = s.catalog.rename_keyword_ops(&from, &to).map_err(|e| bad("keyword.rename", e.to_string()))?;
                let (f, t) = (clean(&from), clean(&to));
                commit_keywords(s, "Rename Keyword", op, |k| Some(if is_under(k, &f) { reparent(k, &f, &t) } else { k.to_string() }))
            }
        ),
        cmd!(
            "keyword.delete",
            "Delete Keyword",
            [],
            None,
            "{keyword} — removes it (and the keywords below it) from every photo",
            always,
            |s, p| {
                let k = str_param(p, "keyword").ok_or_else(|| bad("keyword.delete", "missing `keyword`"))?.to_string();
                let op = s.catalog.delete_keyword_ops(&k).map_err(|e| bad("keyword.delete", e.to_string()))?;
                let c = clean(&k);
                commit_keywords(s, "Delete Keyword", op, |f| (!is_under(f, &c)).then(|| f.to_string()))
            }
        ),
        cmd!(
            "keyword.merge",
            "Merge Keywords",
            [],
            None,
            "{from: [keyword], into: keyword} — replaces each `from` keyword (children included) with `into` on every photo",
            always,
            |s, p| {
                let from = strs(p, "from");
                let into = str_param(p, "into").ok_or_else(|| bad("keyword.merge", "missing `into`"))?.to_string();
                let op = s.catalog.merge_keywords_ops(&from, &into).map_err(|e| bad("keyword.merge", e.to_string()))?;
                let (from, into) = (from.iter().map(|f| clean(f)).collect::<Vec<_>>(), clean(&into));
                commit_keywords(s, "Merge Keywords", op, |k| {
                    Some(match closest(k, &from) {
                        Some(f) => reparent(k, f, &into),
                        None => k.to_string(),
                    })
                })
            }
        ),
    ]
}
