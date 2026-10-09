//! Library-wide keyword operations: the keyword tree (hierarchical keywords are written
//! `parent|child|grandchild`), rename / delete / merge across every photo, and suggestions.
//!
//! Rename, delete and merge produce one [`Op::Batch`] of [`Op::SetMeta`]s — one per photo that
//! changes — so they are a single undo step and replay from the op log like any other edit.
//! Keyword names compare case-insensitively; renaming a keyword renames its children too
//! (`travel|italy` → `trips|italy`).
//!
//! The **keyword list** holds keywords on their own ([`Op::SetKeyword`]): created before any
//! photo has them, or given attributes ([`KeywordInfo`]: synonyms, export options, person). The
//! tree is the keyword list together with the keywords photos carry.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Catalog, CatalogError, Meta, Op, Result};

/// Separator of hierarchical keyword levels.
pub const SEP: char = '|';

/// Normalize a keyword: trim every level, drop empty levels.
pub fn clean(k: &str) -> String {
    k.split(SEP).map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("|")
}

/// `k` is `parent` or one of its descendants (`parent|…`), ignoring case.
pub fn is_under(k: &str, parent: &str) -> bool {
    let (k, p) = (k.to_lowercase(), parent.to_lowercase());
    k == p || k.strip_prefix(&p).is_some_and(|rest| rest.starts_with(SEP))
}

/// Replace the `from` prefix of `k` (which [`is_under`] `from`) with `to`. By levels, not bytes:
/// `from` may be written in another case, which can change a letter's length (ẞ / ß).
fn reparent(k: &str, from: &str, to: &str) -> String {
    let skip = from.split(SEP).filter(|s| !s.trim().is_empty()).count();
    let rest: Vec<&str> = k.split(SEP).filter(|s| !s.trim().is_empty()).skip(skip).collect();
    std::iter::once(to).filter(|t| !t.is_empty()).chain(rest).collect::<Vec<_>>().join("|")
}

/// Keep the first of case-insensitively equal keywords.
fn dedupe(v: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    v.retain(|k| !k.is_empty() && seen.insert(k.to_lowercase()));
}

/// What the keyword list knows about a keyword besides the photos that carry it: Lightroom
/// Classic's keyword tag options. A keyword is listed when it was created on its own or given
/// attributes; one that only photos carry has the defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeywordInfo {
    /// Other words for it, exported with it (when `export_synonyms`) and found by search.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub synonyms: Vec<String>,
    /// Written into exported files at all.
    #[serde(default = "yes")]
    pub include_on_export: bool,
    /// The keywords containing it are exported with it (`travel|italy` also gives `travel`).
    #[serde(default = "yes")]
    pub export_containing: bool,
    #[serde(default = "yes")]
    pub export_synonyms: bool,
    /// The keyword names a person.
    #[serde(default)]
    pub person: bool,
}

fn yes() -> bool {
    true
}

impl Default for KeywordInfo {
    fn default() -> Self {
        KeywordInfo { synonyms: Vec::new(), include_on_export: true, export_containing: true, export_synonyms: true, person: false }
    }
}

/// A keyword in the library's keyword list: its path as written and its attributes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListedKeyword {
    pub path: String,
    pub info: KeywordInfo,
}

/// One node of the keyword tree.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct KeywordNode {
    /// This level's name (`italy`).
    pub name: String,
    /// The full keyword (`travel|italy`).
    pub path: String,
    /// Photos (not deleted) with this keyword or one below it.
    pub count: usize,
    pub children: Vec<KeywordNode>,
}

impl Catalog {
    /// The keyword tree: every keyword level with photo counts, sorted by name (case-insensitive).
    pub fn keyword_tree(&self) -> Vec<KeywordNode> {
        use std::collections::{HashMap, HashSet};
        // every keyword level (`travel`, `travel|italy`…) by its lower-case path: the name and
        // path as first written, and the photos counted (once per photo, even when two of its
        // keywords share a parent)
        struct Level {
            name: String,
            path: String,
            count: usize,
            parent: Option<String>,
        }
        let mut levels: HashMap<String, Level> = HashMap::new();
        let mut this_photo: HashSet<String> = HashSet::new();
        // the keyword list first (no photos counted): a listed keyword is there without photos
        let listed: Vec<String> = self.listed_keywords().map(|k| k.path.clone()).collect();
        let photos = self.photos().filter(|p| p.in_library()).map(|p| (true, p.meta.keywords.as_slice()));
        for (counts, keywords) in std::iter::once((false, listed.as_slice())).chain(photos) {
            this_photo.clear();
            for k in keywords {
                let mut path = String::new();
                let mut lower = String::new();
                let mut parent: Option<String> = None;
                for part in k.split(SEP).map(str::trim).filter(|s| !s.is_empty()) {
                    if !path.is_empty() {
                        path.push(SEP);
                        lower.push(SEP);
                    }
                    path.push_str(part);
                    lower.push_str(&part.to_lowercase());
                    let l = levels.entry(lower.clone()).or_insert_with(|| Level {
                        name: part.to_string(),
                        path: path.clone(),
                        count: 0,
                        parent: parent.clone(),
                    });
                    if counts && this_photo.insert(lower.clone()) {
                        l.count += 1;
                    }
                    parent = Some(lower.clone());
                }
            }
        }
        let mut kids: HashMap<Option<String>, Vec<String>> = HashMap::new();
        for (lower, l) in &levels {
            kids.entry(l.parent.clone()).or_default().push(lower.clone());
        }
        fn build(at: Option<String>, levels: &HashMap<String, Level>, kids: &mut HashMap<Option<String>, Vec<String>>) -> Vec<KeywordNode> {
            let Some(mut list) = kids.remove(&at) else { return Vec::new() };
            list.sort_by_key(|l| levels[l].name.to_lowercase());
            list.into_iter()
                .map(|l| {
                    let lv = &levels[&l];
                    let children = build(Some(l.clone()), levels, kids);
                    KeywordNode { name: lv.name.clone(), path: lv.path.clone(), count: lv.count, children }
                })
                .collect()
        }
        build(None, &levels, &mut kids)
    }

    /// `SetMeta` ops for every photo whose keywords `f` changes.
    fn keyword_ops(&self, f: impl Fn(&[String]) -> Vec<String>) -> Vec<Op> {
        self.photos()
            .filter_map(|p| {
                let mut kws = f(&p.meta.keywords);
                dedupe(&mut kws);
                (kws != p.meta.keywords).then(|| Op::SetMeta { id: p.id, meta: Box::new(Meta { keywords: kws, ..p.meta.clone() }) })
            })
            .collect()
    }

    /// `SetKeyword` ops turning the keyword list into what `f` makes of it (lower-case path →
    /// listing): what it no longer has is taken off first, then what is new or changed is set.
    fn list_ops(&self, f: impl FnOnce(&mut BTreeMap<String, ListedKeyword>)) -> Vec<Op> {
        let before = &self.keyword_list;
        let mut after = before.clone();
        f(&mut after);
        let gone = before.iter().filter(|(k, _)| !after.contains_key(*k)).map(|(_, l)| Op::SetKeyword { path: l.path.clone(), info: None });
        let set = after
            .iter()
            .filter(|(k, l)| before.get(*k) != Some(*l))
            .map(|(_, l)| Op::SetKeyword { path: l.path.clone(), info: Some(l.info.clone()) });
        gone.chain(set).collect()
    }

    /// The keyword list once the keywords under each of `from` move to `to`: a listing landing on
    /// one already there merges into it (that one's attributes stay, the synonyms gather).
    fn list_move_ops(&self, from: &[String], to: &str) -> Vec<Op> {
        self.list_ops(|list| {
            let moving: Vec<String> = list.iter().filter(|(_, l)| from.iter().any(|f| is_under(&l.path, f))).map(|(k, _)| k.clone()).collect();
            for key in moving {
                let Some(l) = list.remove(&key) else { continue };
                let Some(f) = from.iter().find(|f| is_under(&l.path, f)) else { continue };
                let path = reparent(&l.path, f, to);
                match list.get_mut(&path.to_lowercase()) {
                    Some(there) => {
                        for s in l.info.synonyms {
                            if !there.info.synonyms.iter().any(|x| x.eq_ignore_ascii_case(&s)) {
                                there.info.synonyms.push(s);
                            }
                        }
                    }
                    None => {
                        list.insert(path.to_lowercase(), ListedKeyword { path, info: l.info });
                    }
                }
            }
        })
    }

    /// Rename `from` (and the keywords below it) to `to` on every photo and in the keyword list —
    /// one batch. Renaming onto an existing keyword merges the two.
    pub fn rename_keyword_ops(&self, from: &str, to: &str) -> Result<Op> {
        let (from, to) = (clean(from), clean(to));
        if from.is_empty() || to.is_empty() {
            return Err(CatalogError::Invalid("keyword names can't be empty".into()));
        }
        if is_under(&to, &from) && !to.eq_ignore_ascii_case(&from) {
            return Err(CatalogError::Invalid("can't move a keyword below itself".into()));
        }
        let mut ops = self.keyword_ops(|kws| kws.iter().map(|k| if is_under(k, &from) { reparent(k, &from, &to) } else { k.clone() }).collect());
        ops.extend(self.list_move_ops(std::slice::from_ref(&from), &to));
        Ok(Op::Batch { ops })
    }

    /// Remove `keyword` and the keywords below it from every photo and from the keyword list.
    pub fn delete_keyword_ops(&self, keyword: &str) -> Result<Op> {
        let k = clean(keyword);
        if k.is_empty() {
            return Err(CatalogError::Invalid("empty keyword".into()));
        }
        let mut ops = self.keyword_ops(|kws| kws.iter().filter(|x| !is_under(x, &k)).cloned().collect());
        ops.extend(self.list_ops(|list| list.retain(|_, l| !is_under(&l.path, &k))));
        Ok(Op::Batch { ops })
    }

    /// Merge several keywords (with their children) into `into`.
    pub fn merge_keywords_ops(&self, from: &[String], into: &str) -> Result<Op> {
        let into = clean(into);
        let from: Vec<String> = from.iter().map(|f| clean(f)).filter(|f| !f.is_empty() && !f.eq_ignore_ascii_case(&into)).collect();
        if into.is_empty() || from.is_empty() {
            return Err(CatalogError::Invalid("merge needs keywords and a target".into()));
        }
        if from.iter().any(|f| is_under(&into, f)) {
            return Err(CatalogError::Invalid("can't merge a keyword into one below it".into()));
        }
        let mut ops = self.keyword_ops(|kws| {
            kws.iter().map(|k| from.iter().find(|f| is_under(k, f)).map(|f| reparent(k, f, &into)).unwrap_or_else(|| k.clone())).collect()
        });
        ops.extend(self.list_move_ops(&from, &into));
        Ok(Op::Batch { ops })
    }

    /// The keyword is in the tree: listed, or on a photo (itself or one below it).
    pub fn has_keyword(&self, path: &str) -> bool {
        let k = clean(path);
        !k.is_empty() && (self.keyword_info(&k).is_some() || self.photos().any(|p| p.meta.keywords.iter().any(|x| is_under(x, &k))))
    }

    /// The keyword as written in the library (whatever the case of `path`): its listing, or the
    /// levels a photo's keyword spells it with. `None` when there is no such keyword.
    pub fn keyword_path(&self, path: &str) -> Option<String> {
        let k = clean(path);
        if k.is_empty() {
            return None;
        }
        if let Some(l) = self.keyword_list.get(&k.to_lowercase()) {
            return Some(l.path.clone());
        }
        let levels = k.split(SEP).count();
        self.photos()
            .flat_map(|p| p.meta.keywords.iter())
            .find(|x| is_under(x, &k))
            .map(|x| x.split(SEP).map(str::trim).filter(|s| !s.is_empty()).take(levels).collect::<Vec<_>>().join("|"))
    }

    /// Create a keyword with these attributes, and give it to `photos` — one batch. A keyword that
    /// exists already is refused.
    pub fn create_keyword_ops(&self, path: &str, info: KeywordInfo, photos: &[crate::PhotoId]) -> Result<Op> {
        let path = clean(path);
        if path.is_empty() {
            return Err(CatalogError::Invalid("keyword names can't be empty".into()));
        }
        if self.has_keyword(&path) {
            return Err(CatalogError::KeywordExists(path));
        }
        let mut ops = vec![Op::SetKeyword { path: path.clone(), info: Some(info) }];
        for id in photos {
            let p = self.photo(*id).ok_or(CatalogError::NoPhoto(*id))?;
            let mut meta = p.meta.clone();
            meta.keywords.push(path.clone());
            dedupe(&mut meta.keywords);
            ops.push(Op::SetMeta { id: *id, meta: Box::new(meta) });
        }
        Ok(Op::Batch { ops })
    }

    /// Move `keyword` (with the keywords below it) inside `parent`, or to the top level (`None`).
    /// It keeps its name. Onto a keyword that is there already the two merge, which only happens
    /// with `merge`; otherwise [`CatalogError::KeywordExists`].
    pub fn move_keyword_ops(&self, keyword: &str, parent: Option<&str>, merge: bool) -> Result<Op> {
        let Some(from) = self.keyword_path(keyword) else {
            return Err(CatalogError::Invalid(format!("no keyword “{}”", clean(keyword))));
        };
        let leaf = from.rsplit(SEP).next().unwrap_or(&from);
        let to = match parent.map(clean).filter(|p| !p.is_empty()) {
            Some(p) if is_under(&p, &from) => return Err(CatalogError::Invalid("can't move a keyword inside itself".into())),
            Some(p) => format!("{}{SEP}{leaf}", self.keyword_path(&p).unwrap_or(p)),
            None => leaf.to_string(),
        };
        if to.eq_ignore_ascii_case(&from) {
            return Ok(Op::Batch { ops: vec![] });
        }
        if !merge && self.has_keyword(&to) {
            return Err(CatalogError::KeywordExists(to));
        }
        self.rename_keyword_ops(&from, &to)
    }

    /// Rename `keyword`'s last level to `name` (it stays where it is) and set its attributes — one
    /// batch. A name that is taken is refused ([`CatalogError::KeywordExists`]): merging is its own
    /// action.
    pub fn edit_keyword_ops(&self, keyword: &str, name: &str, info: KeywordInfo) -> Result<Op> {
        let Some(from) = self.keyword_path(keyword) else {
            return Err(CatalogError::Invalid(format!("no keyword “{}”", clean(keyword))));
        };
        let name = name.trim();
        if name.is_empty() || name.contains(SEP) {
            return Err(CatalogError::Invalid("a keyword's name is one level, without “|”".into()));
        }
        let to = match from.rsplit_once(SEP) {
            Some((parent, _)) => format!("{parent}{SEP}{name}"),
            None => name.to_string(),
        };
        let mut ops = Vec::new();
        if to != from {
            if !to.eq_ignore_ascii_case(&from) && self.has_keyword(&to) {
                return Err(CatalogError::KeywordExists(to));
            }
            if let Op::Batch { ops: renamed } = self.rename_keyword_ops(&from, &to)? {
                ops = renamed;
            }
        }
        ops.push(Op::SetKeyword { path: to, info: Some(info) });
        Ok(Op::Batch { ops })
    }

    /// Take off the keyword list the keywords no photo has, nor any keyword below them — one batch.
    pub fn purge_unused_keywords_ops(&self) -> Op {
        let used: Vec<&String> = self.photos().flat_map(|p| p.meta.keywords.iter()).collect();
        Op::Batch { ops: self.list_ops(|list| list.retain(|_, l| used.iter().any(|k| is_under(k, &l.path)))) }
    }

    /// Keyword suggestions for a photo that has `current` keywords: with a typed `prefix`, the
    /// library's keywords containing it (those starting with it first); otherwise keywords that
    /// appear together with `current` on other photos, then the most used ones. Most frequent
    /// first, at most `n`, never one of `current`.
    pub fn keyword_suggestions(&self, current: &[String], prefix: &str, n: usize) -> Vec<String> {
        let all = self.keywords();
        let has = |k: &str| current.iter().any(|c| c.eq_ignore_ascii_case(k));
        let q = prefix.trim().to_lowercase();
        let mut scored: Vec<(i64, String)> = if !q.is_empty() {
            all.into_iter()
                .filter(|(k, _)| !has(k))
                .filter_map(|(k, c)| {
                    let l = k.to_lowercase();
                    let leaf = l.rsplit(SEP).next().unwrap_or(&l).to_string();
                    let rank = if l.starts_with(&q) || leaf.starts_with(&q) {
                        2
                    } else if l.contains(&q) {
                        1
                    } else {
                        return None;
                    };
                    Some((rank * 1_000_000 + c as i64, k))
                })
                .collect()
        } else {
            let mut co: std::collections::HashMap<String, i64> = Default::default();
            if !current.is_empty() {
                for p in self.photos().filter(|p| p.in_library()) {
                    if p.meta.keywords.iter().any(|k| has(k)) {
                        for k in p.meta.keywords.iter().filter(|k| !has(k)) {
                            *co.entry(k.clone()).or_default() += 1;
                        }
                    }
                }
            }
            all.into_iter().filter(|(k, _)| !has(k)).map(|(k, c)| (co.get(&k).copied().unwrap_or(0) * 1_000_000 + c as i64, k)).collect()
        };
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())));
        scored.into_iter().take(n).map(|(_, k)| k).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Filter, Photo, PhotoId, Source};

    fn lib(kws: &[&[&str]]) -> (Catalog, Vec<PhotoId>) {
        let mut c = Catalog::new();
        let mut ids = Vec::new();
        for k in kws {
            let id = c.alloc_photo_id();
            let mut p = Photo::new(id, Source::Demo { scene: 1 }, "a.jpg", "JPEG", 3, 2, "2026-01-01");
            p.meta.keywords = k.iter().map(|s| s.to_string()).collect();
            c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
            ids.push(id);
        }
        (c, ids)
    }
    fn kws(c: &Catalog, id: PhotoId) -> Vec<String> {
        c.photo(id).unwrap().meta.keywords.clone()
    }

    #[test]
    fn tree_counts_photos_per_level() {
        let (c, _) = lib(&[&["travel|Italy|Rome", "beach"], &["Travel|italy"], &["travel|France", "travel|italy|rome"], &["beach"]]);
        let t = c.keyword_tree();
        assert_eq!(t.iter().map(|n| (n.name.as_str(), n.count)).collect::<Vec<_>>(), vec![("beach", 2), ("travel", 3)]);
        let travel = &t[1];
        assert_eq!(travel.children.iter().map(|n| (n.path.as_str(), n.count)).collect::<Vec<_>>(), vec![("travel|France", 1), ("travel|Italy", 3)]);
        assert_eq!(travel.children[1].children[0].path, "travel|Italy|Rome");
        assert_eq!(travel.children[1].children[0].count, 2);
        // filtering by a parent finds its children
        let f = Filter { keyword: Some("travel|italy".into()), ..Default::default() };
        assert_eq!(c.query(&f, &Default::default()).len(), 3);
    }

    #[test]
    fn rename_delete_merge_are_single_undoable_batches() {
        let (mut c, ids) = lib(&[&["travel|italy|rome", "beach"], &["Travel|Italy"], &["italia", "travel|italy"], &["sea"]]);
        let before = c.to_snapshot();
        let op = c.rename_keyword_ops("travel|italy", "Europe|Italy").unwrap();
        let Op::Batch { ops } = &op else { panic!() };
        assert_eq!(ops.len(), 3, "only photos that change");
        let inv = c.apply(op).unwrap();
        assert_eq!(kws(&c, ids[0]), ["Europe|Italy|rome", "beach"]);
        assert_eq!(kws(&c, ids[1]), ["Europe|Italy"]);
        c.apply(inv).unwrap();
        assert_eq!(c.to_snapshot(), before);
        // merge `italia` into the existing hierarchical keyword: duplicates collapse
        let op = c.merge_keywords_ops(&["italia".into()], "travel|italy").unwrap();
        c.apply(op).unwrap();
        assert_eq!(kws(&c, ids[2]), ["travel|italy"]);
        // delete removes the keyword and its children everywhere
        let op = c.delete_keyword_ops("TRAVEL").unwrap();
        c.apply(op).unwrap();
        assert_eq!(kws(&c, ids[0]), ["beach"]);
        assert!(kws(&c, ids[1]).is_empty());
        assert_eq!(c.keywords(), vec![("beach".to_string(), 1), ("sea".to_string(), 1)]);
        // invalid requests
        assert!(c.rename_keyword_ops("beach", " ").is_err());
        assert!(c.rename_keyword_ops("a", "a|b").is_err());
        assert!(c.merge_keywords_ops(&["a".into()], "a|b").is_err());
        assert_eq!(clean(" a | |b "), "a|b");
    }

    fn tree_paths(nodes: &[KeywordNode]) -> Vec<(String, usize)> {
        let mut out = Vec::new();
        for n in nodes {
            out.push((n.path.clone(), n.count));
            out.extend(tree_paths(&n.children));
        }
        out
    }

    /// A keyword can be listed before any photo has it (Lightroom Classic's Create Keyword Tag):
    /// it is in the tree with no photos, its parents too, and undo takes it away.
    #[test]
    fn a_keyword_listed_with_no_photos_is_in_the_tree() {
        let (mut c, _) = lib(&[&["travel|France"]]);
        let undo = c.apply(Op::SetKeyword { path: " travel | Italy ".into(), info: Some(KeywordInfo::default()) }).unwrap();
        assert_eq!(tree_paths(&c.keyword_tree()), [("travel".to_string(), 1), ("travel|France".to_string(), 1), ("travel|Italy".to_string(), 0)]);
        assert_eq!(c.keyword_info("TRAVEL|italy"), Some(&KeywordInfo::default()), "found whatever the case");
        c.apply(undo).unwrap();
        assert_eq!(tree_paths(&c.keyword_tree()), [("travel".to_string(), 1), ("travel|France".to_string(), 1)]);
        assert_eq!(c.keyword_info("travel|Italy"), None);
    }

    /// A listed keyword needs a name, and its attributes start as Lightroom Classic's do: exported,
    /// with the keywords containing it and its synonyms; not a person.
    #[test]
    fn a_listed_keyword_needs_a_name_and_starts_exported() {
        let mut c = Catalog::new();
        assert!(c.apply(Op::SetKeyword { path: " | ".into(), info: Some(KeywordInfo::default()) }).is_err());
        let info = KeywordInfo::default();
        assert!(info.include_on_export && info.export_containing && info.export_synonyms && !info.person && info.synonyms.is_empty());
    }

    /// Changing a listed keyword's attributes is undone to the attributes it had, under the name as
    /// it was written.
    #[test]
    fn changing_a_listed_keyword_is_undone_to_what_it_was() {
        let mut c = Catalog::new();
        c.apply(Op::SetKeyword { path: "Weddings".into(), info: Some(KeywordInfo::default()) }).unwrap();
        let person = KeywordInfo { person: true, synonyms: vec!["Marriage".into()], ..KeywordInfo::default() };
        let undo = c.apply(Op::SetKeyword { path: "weddings".into(), info: Some(person.clone()) }).unwrap();
        assert_eq!(c.keyword_info("Weddings"), Some(&person));
        assert_eq!(undo, Op::SetKeyword { path: "Weddings".into(), info: Some(KeywordInfo::default()) });
        c.apply(undo).unwrap();
        assert_eq!(c.keyword_info("weddings"), Some(&KeywordInfo::default()));
    }

    fn listed(c: &Catalog) -> Vec<(String, KeywordInfo)> {
        let mut v: Vec<(String, KeywordInfo)> = c.listed_keywords().map(|k| (k.path.clone(), k.info.clone())).collect();
        v.sort_by_key(|(p, _)| p.to_lowercase());
        v
    }

    fn with_synonyms(words: &[&str]) -> KeywordInfo {
        KeywordInfo { synonyms: words.iter().map(|w| w.to_string()).collect(), ..KeywordInfo::default() }
    }

    /// Renaming (or moving) a keyword takes its attributes and its listed children along, photos
    /// or not, in the same undo step.
    #[test]
    fn renaming_a_keyword_carries_its_listing_along() {
        let (mut c, ids) = lib(&[&["travel|italy"]]);
        c.apply(Op::SetKeyword { path: "travel".into(), info: Some(with_synonyms(&["trip"])) }).unwrap();
        c.apply(Op::SetKeyword { path: "travel|Spain".into(), info: Some(KeywordInfo::default()) }).unwrap();
        let before = c.to_snapshot();
        let op = c.rename_keyword_ops("Travel", "Places|Europe").unwrap();
        let undo = c.apply(op).unwrap();
        assert_eq!(kws(&c, ids[0]), ["Places|Europe|italy"]);
        assert_eq!(
            listed(&c),
            [("Places|Europe".to_string(), with_synonyms(&["trip"])), ("Places|Europe|Spain".to_string(), KeywordInfo::default())]
        );
        c.apply(undo).unwrap();
        assert_eq!(c.to_snapshot(), before);
    }

    /// A keyword only the list holds (no photos) can be renamed too.
    #[test]
    fn a_keyword_without_photos_can_be_renamed() {
        let mut c = Catalog::new();
        c.apply(Op::SetKeyword { path: "weddings".into(), info: Some(KeywordInfo::default()) }).unwrap();
        let op = c.rename_keyword_ops("weddings", "Events|Weddings").unwrap();
        c.apply(op).unwrap();
        assert_eq!(listed(&c), [("Events|Weddings".to_string(), KeywordInfo::default())]);
    }

    /// Merging keeps the target's attributes and adds the merged keyword's synonyms to them; a target
    /// not listed yet takes the merged keyword's attributes.
    #[test]
    fn merging_keeps_the_targets_attributes_and_gathers_synonyms() {
        let (mut c, _) = lib(&[&["holiday"], &["travel"]]);
        let person = KeywordInfo { person: true, ..with_synonyms(&["trip"]) };
        c.apply(Op::SetKeyword { path: "travel".into(), info: Some(person.clone()) }).unwrap();
        c.apply(Op::SetKeyword {
            path: "holiday".into(),
            info: Some(KeywordInfo { include_on_export: false, ..with_synonyms(&["vacation", "Trip"]) }),
        })
        .unwrap();
        let op = c.merge_keywords_ops(&["holiday".into()], "travel").unwrap();
        c.apply(op).unwrap();
        assert_eq!(listed(&c), [("travel".to_string(), KeywordInfo { person: true, ..with_synonyms(&["trip", "vacation"]) })]);
        // onto a keyword that isn't listed: it takes the merged one's attributes
        let mut c = Catalog::new();
        c.apply(Op::SetKeyword { path: "holiday".into(), info: Some(with_synonyms(&["vacation"])) }).unwrap();
        let op = c.merge_keywords_ops(&["holiday".into()], "Travel").unwrap();
        c.apply(op).unwrap();
        assert_eq!(listed(&c), [("Travel".to_string(), with_synonyms(&["vacation"]))]);
    }

    /// Deleting a keyword takes it, and the keywords below it, off the list as well as off photos.
    #[test]
    fn deleting_a_keyword_takes_it_off_the_list() {
        let (mut c, _) = lib(&[&["travel|italy"], &["beach"]]);
        for path in ["travel", "travel|spain", "beach"] {
            c.apply(Op::SetKeyword { path: path.into(), info: Some(KeywordInfo::default()) }).unwrap();
        }
        let before = c.to_snapshot();
        let op = c.delete_keyword_ops("travel").unwrap();
        let undo = c.apply(op).unwrap();
        assert_eq!(listed(&c), [("beach".to_string(), KeywordInfo::default())]);
        assert_eq!(tree_paths(&c.keyword_tree()), [("beach".to_string(), 1)]);
        c.apply(undo).unwrap();
        assert_eq!(c.to_snapshot(), before);
    }

    /// Creating a keyword lists it with its attributes, and can tag photos with it in the same undo
    /// step. A keyword that exists already (listed, or on a photo) isn't created again.
    #[test]
    fn creating_a_keyword_lists_it_and_can_tag_photos() {
        let (mut c, ids) = lib(&[&["beach"], &[]]);
        let before = c.to_snapshot();
        let op = c.create_keyword_ops("Events | Weddings", with_synonyms(&["marriage"]), &[ids[1]]).unwrap();
        let undo = c.apply(op).unwrap();
        assert_eq!(listed(&c), [("Events|Weddings".to_string(), with_synonyms(&["marriage"]))]);
        assert_eq!(kws(&c, ids[1]), ["Events|Weddings"]);
        c.apply(undo).unwrap();
        assert_eq!(c.to_snapshot(), before);
        for exists in ["BEACH", "events|weddings"] {
            if exists == "events|weddings" {
                let op = c.create_keyword_ops("Events|Weddings", KeywordInfo::default(), &[]).unwrap();
                c.apply(op).unwrap();
            }
            assert!(c.create_keyword_ops(exists, KeywordInfo::default(), &[]).is_err(), "{exists}");
        }
        assert!(c.create_keyword_ops(" | ", KeywordInfo::default(), &[]).is_err());
    }

    /// Moving a keyword nests it inside another (or takes it to the top level), with its children,
    /// on photos and in the list. It can't go inside itself or one of its children, and moving it
    /// where it already is changes nothing.
    #[test]
    fn moving_a_keyword_nests_it_inside_another() {
        let (mut c, ids) = lib(&[&["Italy|Rome"], &["Europe"]]);
        // typed in any case, the keywords keep the spelling the library has
        let op = c.move_keyword_ops("italy", Some("EUROPE"), false).unwrap();
        c.apply(op).unwrap();
        assert_eq!(kws(&c, ids[0]), ["Europe|Italy|Rome"]);
        let op = c.move_keyword_ops("europe|italy", None, false).unwrap();
        c.apply(op).unwrap();
        assert_eq!(kws(&c, ids[0]), ["Italy|Rome"], "back at the top level");
        assert!(c.move_keyword_ops("italy", Some("italy|rome"), false).is_err(), "inside its own child");
        assert!(c.move_keyword_ops("italy", Some("Italy"), false).is_err(), "inside itself");
        assert_eq!(c.move_keyword_ops("italy|rome", Some("italy"), false).unwrap(), Op::Batch { ops: vec![] }, "already there");
        assert!(c.move_keyword_ops("lisbon", Some("europe"), false).is_err(), "no such keyword");
    }

    /// Moving onto a name that is taken (Europe already has a Rome) merges the two, so the move
    /// asks first: refused unless merging is allowed.
    #[test]
    fn moving_onto_a_taken_name_merges_only_when_allowed() {
        let (mut c, ids) = lib(&[&["rome"], &["europe|rome"]]);
        let err = c.move_keyword_ops("rome", Some("europe"), false).unwrap_err();
        assert!(matches!(err, CatalogError::KeywordExists(ref k) if k == "europe|rome"), "{err:?}");
        let op = c.move_keyword_ops("rome", Some("europe"), true).unwrap();
        c.apply(op).unwrap();
        assert_eq!((kws(&c, ids[0]), kws(&c, ids[1])), (vec!["europe|rome".to_string()], vec!["europe|rome".to_string()]));
    }

    /// Editing a keyword renames it (its last level: it stays where it is) and sets its attributes,
    /// in one undo step. A name that is taken is refused: merging is its own action.
    #[test]
    fn editing_a_keyword_renames_it_and_sets_its_attributes() {
        let (mut c, ids) = lib(&[&["travel|italy"], &["travel|spain"]]);
        let before = c.to_snapshot();
        let person = KeywordInfo { person: true, ..KeywordInfo::default() };
        let op = c.edit_keyword_ops("travel|italy", "Italia", person.clone()).unwrap();
        let undo = c.apply(op).unwrap();
        assert_eq!(kws(&c, ids[0]), ["travel|Italia"]);
        assert_eq!(c.keyword_info("travel|italia"), Some(&person));
        c.apply(undo).unwrap();
        assert_eq!(c.to_snapshot(), before);
        // attributes alone
        let op = c.edit_keyword_ops("travel", "travel", with_synonyms(&["trip"])).unwrap();
        c.apply(op).unwrap();
        assert_eq!(listed(&c), [("travel".to_string(), with_synonyms(&["trip"]))]);
        assert!(matches!(c.edit_keyword_ops("travel|italy", "Spain", KeywordInfo::default()), Err(CatalogError::KeywordExists(_))));
        assert!(c.edit_keyword_ops("travel|italy", "a|b", KeywordInfo::default()).is_err(), "a name, not a path");
        assert!(c.edit_keyword_ops("travel|italy", "  ", KeywordInfo::default()).is_err());
    }

    /// Purging takes off the list the keywords no photo has (nor any keyword below them), in one
    /// undo step; keywords photos carry stay.
    #[test]
    fn purging_takes_unused_keywords_off_the_list() {
        let (mut c, _) = lib(&[&["travel|italy"]]);
        for path in ["travel", "travel|italy", "travel|spain", "weddings"] {
            c.apply(Op::SetKeyword { path: path.into(), info: Some(KeywordInfo::default()) }).unwrap();
        }
        let op = c.purge_unused_keywords_ops();
        c.apply(op).unwrap();
        assert_eq!(listed(&c).iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(), ["travel", "travel|italy"]);
        assert_eq!(tree_paths(&c.keyword_tree()), [("travel".to_string(), 1), ("travel|italy".to_string(), 1)]);
    }

    /// Renaming matches names whatever their case, also where a letter's case changes its length
    /// (ẞ is three bytes, ß two): the levels below are kept whole, never cut mid-letter.
    #[test]
    fn renaming_keeps_the_levels_below_whatever_the_case() {
        let (mut c, ids) = lib(&[&["straße|nord"], &["ßa|ü"]]);
        let op = c.rename_keyword_ops("STRAẞE", "Road").unwrap();
        c.apply(op).unwrap();
        assert_eq!(kws(&c, ids[0]), ["Road|nord"]);
        let op = c.rename_keyword_ops("ẞA", "Weg").unwrap();
        c.apply(op).unwrap();
        assert_eq!(kws(&c, ids[1]), ["Weg|ü"]);
    }

    #[test]
    fn suggestions_rank_co_occurrence_and_prefixes() {
        let (c, _) = lib(&[&["dog", "park"], &["dog", "park", "ball"], &["dog", "beach"], &["cat"], &["cat"], &["cat"], &["parade"], &["eagle"]]);
        // co-occurring with `dog` first (park twice), then the most used
        let s = c.keyword_suggestions(&["dog".into()], "", 3);
        assert_eq!(s, ["park", "ball", "beach"]);
        let s = c.keyword_suggestions(&[], "", 2);
        assert_eq!(s, ["cat", "dog"]);
        let s = c.keyword_suggestions(&["park".into()], "pa", 5);
        assert_eq!(s, ["parade"]);
        let s = c.keyword_suggestions(&[], "E", 5);
        assert_eq!(s, ["eagle", "beach", "parade"], "prefix matches before substring matches");
    }
}
