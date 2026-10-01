//! Library-wide keyword commands: list (tree with counts), suggestions, rename, delete, merge.

use lightcraft_catalog::keywords::{clean, is_under};
use serde_json::{Value, json};

use super::{CommandSpec, always, bad, cmd, str_param};
use crate::{Result, Session};

fn strs(p: &Value, key: &str) -> Vec<String> {
    match p.get(key) {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => Vec::new(),
    }
}

/// Commit a keyword batch; returns how many photos changed. The keyword filter follows a renamed
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

pub fn specs() -> Vec<CommandSpec> {
    vec![
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
                commit_keywords(s, "Rename Keyword", op, |k| {
                    Some(if is_under(k, &f) { format!("{t}{}", &k[f.len().min(k.len())..]) } else { k.to_string() })
                })
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
                    Some(match from.iter().find(|f| is_under(k, f)) {
                        Some(f) => format!("{into}{}", &k[f.len().min(k.len())..]),
                        None => k.to_string(),
                    })
                })
            }
        ),
    ]
}
