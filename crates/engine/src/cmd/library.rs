//! Library commands: view source, filter/sort, selection, ratings/flags/labels, rotate, delete,
//! metadata, albums, import.

use lightcraft_catalog::{Album, AlbumId, ColorLabel, Flag, GroupBy, Op, PhotoId, Sort, SortKey};
use serde_json::{Value, json};

use super::{CommandSpec, always, bad, bool_or, cmd, has_active, has_selection, ok, str_param};
use crate::{LibrarySource, Result, Selection, Session};

fn album_param(p: &Value, key: &str, c: &str) -> Result<AlbumId> {
    p.get(key).and_then(Value::as_u64).map(AlbumId).ok_or_else(|| bad(c, format!("missing album `{key}`")))
}

fn strs(p: &Value, key: &str) -> Vec<String> {
    p.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

fn ids_param(p: &Value) -> Option<Vec<PhotoId>> {
    p.get("ids").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(PhotoId).collect())
}

/// Apply one op per target as a single undo step.
fn for_targets(s: &mut Session, p: &Value, label: &str, f: impl Fn(PhotoId) -> Option<Op>) -> Result<Value> {
    let targets = s.targets(p);
    let ops: Vec<Op> = targets.iter().filter_map(|id| f(*id)).collect();
    let n = ops.len();
    if n > 0 {
        s.commit(label, Op::Batch { ops })?;
    }
    Ok(json!({"changed": n}))
}

fn advance_if(s: &mut Session, p: &Value) {
    if bool_or(p, "advance", false) {
        let _ = step(s, 1);
    }
}

fn step(s: &mut Session, d: isize) -> Result<Value> {
    let vis = s.visible_cloned();
    if vis.is_empty() {
        return ok();
    }
    let cur = s.selection.active.and_then(|a| vis.iter().position(|x| *x == a));
    let next = match cur {
        Some(i) => (i as isize + d).clamp(0, vis.len() as isize - 1) as usize,
        None => 0,
    };
    s.end_interaction()?;
    s.selection = Selection::single(vis[next]);
    s.active_mask = None;
    s.active_spot = None;
    Ok(json!({"active": vis[next].0}))
}

fn rotate(s: &mut Session, p: &Value, cw: bool) -> Result<Value> {
    let targets = s.targets(p);
    let mut ops = Vec::new();
    for id in targets {
        if let Some(d) = s.develop_of(id) {
            let mut d = (*d).clone();
            d.orientation = d.orientation.rotated(cw);
            ops.extend(s.develop_op(id, d, "Rotate"));
        }
    }
    let n = ops.len();
    s.commit(if cw { "Rotate Right" } else { "Rotate Left" }, Op::Batch { ops })?;
    Ok(json!({"changed": n}))
}

fn flip(s: &mut Session, p: &Value, horizontal: bool) -> Result<Value> {
    let targets = s.targets(p);
    let mut ops = Vec::new();
    for id in targets {
        if let Some(d) = s.develop_of(id) {
            let mut d = (*d).clone();
            if horizontal {
                d.crop.flip_h = !d.crop.flip_h;
            } else {
                d.crop.flip_v = !d.crop.flip_v;
            }
            ops.extend(s.develop_op(id, d, "Flip"));
        }
    }
    let n = ops.len();
    s.commit("Flip", Op::Batch { ops })?;
    Ok(json!({"changed": n}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        // ---- view source / filter / sort
        cmd!("library.source", "Show Source", [], None, "{kind: all|recentlyAdded|album|recentlyDeleted|picks, id?: albumId}", always, |s, p| {
            let kind = str_param(p, "kind").unwrap_or("all");
            s.source = match kind {
                "all" => LibrarySource::All,
                "recentlyAdded" => LibrarySource::RecentlyAdded,
                "recentlyDeleted" => LibrarySource::RecentlyDeleted,
                "picks" => LibrarySource::Picks,
                "album" => {
                    let a = album_param(p, "id", "library.source")?;
                    if s.catalog.album(a).is_none_or(|a| a.folder) {
                        return Err(bad("library.source", "no such album"));
                    }
                    LibrarySource::Album(a)
                }
                other => return Err(bad("library.source", format!("unknown source `{other}`"))),
            };
            let vis = s.visible_cloned();
            if s.selection.active.is_none_or(|a| !vis.contains(&a)) {
                s.selection = vis.first().map(|f| Selection::single(*f)).unwrap_or_default();
            }
            Ok(json!({"count": vis.len()}))
        }),
        cmd!(
            "library.filter",
            "Filter",
            [],
            None,
            "partial Filter: {text?, rating?, ratingOp?: atLeast|exactly|atMost, flag?: pick|reject|none|null, label?, kind?, edited?, date?, keyword?, camera?}",
            always,
            |s, p| {
                let mut v = serde_json::to_value(&s.filter).unwrap_or_default();
                lightcraft_develop::presets::deep_merge(&mut v, p);
                s.filter = serde_json::from_value(v).map_err(|e| bad("library.filter", e.to_string()))?;
                Ok(json!({"count": s.visible().len()}))
            }
        ),
        cmd!("library.clearFilter", "Clear Filters", ["View"], None, "{}", always, |s, _| {
            s.filter = Default::default();
            Ok(json!({"count": s.visible().len()}))
        }),
        cmd!(
            "library.sort",
            "Sort",
            ["View", "Sort"],
            None,
            "{key?: captureDate|importDate|editDate|fileName|rating|fileSize, ascending?: bool, group?: auto|none|day|month|year}",
            always,
            |s, p| {
                let key: SortKey = match p.get("key") {
                    Some(k) => serde_json::from_value(k.clone()).map_err(|e| bad("library.sort", e.to_string()))?,
                    None => s.sort.key,
                };
                let group = match str_param(p, "group") {
                    Some(g) => GroupBy::parse(g).ok_or_else(|| bad("library.sort", "group must be auto|none|day|month|year"))?,
                    None => s.sort.group,
                };
                s.sort = Sort { key, ascending: bool_or(p, "ascending", s.sort.ascending), group };
                ok()
            }
        ),
        cmd!(
            query "library.groups",
            "Date Groups",
            [],
            None,
            "{by?: day|month|year (default: the sort's grouping; auto = day)} → [{key, label, start, count}] date headers of the grid",
            always,
            |s, p| {
                let by = match str_param(p, "by") {
                    Some(g) => GroupBy::parse(g).ok_or_else(|| bad("library.groups", "by must be auto|none|day|month|year"))?,
                    None => s.sort.group,
                };
                let vis = s.visible_cloned();
                Ok(serde_json::to_value(s.catalog.date_runs(&vis, s.sort.key, by)).unwrap_or_default())
            }
        ),
        // ---- selection
        cmd!("library.select", "Select Photos", [], None, "{ids: [id], active?: id, mode?: replace|add|toggle|range}", always, |s, p| {
            let ids = ids_param(p).unwrap_or_default();
            let mode = str_param(p, "mode").unwrap_or("replace");
            s.end_interaction()?;
            match mode {
                "replace" => {
                    s.selection =
                        Selection { ids: ids.clone(), active: p.get("active").and_then(Value::as_u64).map(PhotoId).or(ids.first().copied()) };
                }
                "add" => {
                    for id in ids {
                        if !s.selection.contains(id) {
                            s.selection.ids.push(id);
                        }
                        s.selection.active = Some(id);
                    }
                }
                "toggle" => ids.into_iter().for_each(|id| s.selection.toggle(id)),
                "range" => {
                    let vis = s.visible_cloned();
                    if let Some(id) = ids.first() {
                        s.selection.extend_to(*id, &vis);
                    }
                }
                other => return Err(bad("library.select", format!("unknown mode `{other}`"))),
            }
            s.active_mask = None;
            s.active_spot = None;
            Ok(json!({"selected": s.selection.ids.len()}))
        }),
        cmd!("library.selectAll", "Select All", ["Edit"], Some("Cmd+A"), "{}", always, |s, _| {
            let vis = s.visible_cloned();
            s.selection = Selection { active: s.selection.active.filter(|a| vis.contains(a)).or(vis.first().copied()), ids: vis };
            Ok(json!({"selected": s.selection.ids.len()}))
        }),
        cmd!("library.selectNone", "Deselect All", ["Edit"], Some("Cmd+Shift+A"), "{}", always, |s, _| {
            let a = s.selection.active;
            s.selection = Selection { ids: a.into_iter().collect(), active: a };
            ok()
        }),
        cmd!("library.next", "Next Photo", [], Some("Right"), "{}", always, |s, _| step(s, 1)),
        cmd!("library.previous", "Previous Photo", [], Some("Left"), "{}", always, |s, _| step(s, -1)),
        // ---- rating / flags / labels
        cmd!("photo.rate", "Set Rating", ["Photo", "Set Rating"], None, "{rating: 0..5, ids?, advance?: bool}", has_selection, |s, p| {
            let r = super::f64_req(p, "rating", "photo.rate")? as i64;
            if !(0..=5).contains(&r) {
                return Err(bad("photo.rate", "rating must be 0..5"));
            }
            let v = for_targets(s, p, "Set Rating", |id| Some(Op::SetRating { id, rating: r as u8 }))?;
            advance_if(s, p);
            Ok(v)
        }),
        cmd!("photo.flag", "Set Flag", ["Photo", "Set Flag"], None, "{flag: pick|reject|none, ids?, advance?: bool}", has_selection, |s, p| {
            let f = str_param(p, "flag").and_then(Flag::parse).ok_or_else(|| bad("photo.flag", "flag must be pick|reject|none"))?;
            let v = for_targets(s, p, "Set Flag", |id| Some(Op::SetFlag { id, flag: f }))?;
            advance_if(s, p);
            Ok(v)
        }),
        cmd!("photo.pick", "Flag as Pick", ["Photo", "Set Flag"], Some("P"), "{ids?}", has_selection, |s, p| {
            for_targets(s, p, "Flag as Pick", |id| Some(Op::SetFlag { id, flag: Flag::Pick }))
        }),
        cmd!("photo.reject", "Flag as Reject", ["Photo", "Set Flag"], Some("X"), "{ids?}", has_selection, |s, p| {
            for_targets(s, p, "Flag as Reject", |id| Some(Op::SetFlag { id, flag: Flag::Reject }))
        }),
        cmd!("photo.unflag", "Unflag", ["Photo", "Set Flag"], Some("U"), "{ids?}", has_selection, |s, p| {
            for_targets(s, p, "Unflag", |id| Some(Op::SetFlag { id, flag: Flag::None }))
        }),
        cmd!(
            "photo.label",
            "Set Color Label",
            ["Photo", "Set Color Label"],
            None,
            "{label: red|yellow|green|blue|purple|none, ids?}",
            has_selection,
            |s, p| {
                let l = match str_param(p, "label") {
                    None | Some("none") => None,
                    Some(x) => Some(ColorLabel::parse(x).ok_or_else(|| bad("photo.label", "unknown label"))?),
                };
                for_targets(s, p, "Set Color Label", |id| Some(Op::SetLabel { id, label: l }))
            }
        ),
        // ---- orientation
        cmd!("photo.rotateLeft", "Rotate Left", ["Photo"], Some("Cmd+["), "{ids?}", has_selection, |s, p| rotate(s, p, false)),
        cmd!("photo.rotateRight", "Rotate Right", ["Photo"], Some("Cmd+]"), "{ids?}", has_selection, |s, p| rotate(s, p, true)),
        cmd!("photo.flipHorizontal", "Flip Horizontal", ["Photo"], None, "{ids?}", has_selection, |s, p| flip(s, p, true)),
        cmd!("photo.flipVertical", "Flip Vertical", ["Photo"], None, "{ids?}", has_selection, |s, p| flip(s, p, false)),
        // ---- delete / restore
        cmd!("photo.delete", "Delete Photo", ["Photo"], Some("Delete"), "{ids?} — moves to Recently Deleted", has_selection, |s, p| {
            let v = for_targets(s, p, "Delete", |id| Some(Op::SetDeleted { id, deleted: true }))?;
            let vis = s.visible_cloned();
            s.selection = vis.first().map(|f| Selection::single(*f)).unwrap_or_default();
            Ok(v)
        }),
        cmd!("photo.restore", "Restore", [], None, "{ids?}", has_selection, |s, p| for_targets(s, p, "Restore", |id| Some(Op::SetDeleted {
            id,
            deleted: false
        }))),
        cmd!("photo.deletePermanently", "Delete Permanently", [], None, "{ids?}", has_selection, |s, p| {
            let t = s.targets(p);
            let ops = t.iter().map(|id| s.catalog.delete_permanently_ops(*id)).collect::<Vec<_>>();
            s.commit("Delete Permanently", Op::Batch { ops })?;
            s.selection = Selection::default();
            Ok(json!({"deleted": t.len()}))
        }),
        // ---- metadata
        cmd!(
            "photo.setMeta",
            "Edit Info",
            [],
            None,
            "{ids?, title?, caption?, copyright?, creator?, location?, keywords?: [..], addKeywords?: [..], removeKeywords?: [..]}",
            has_selection,
            |s, p| {
                let strs =
                    |k: &str| p.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>());
                let targets = s.targets(p);
                let mut ops = Vec::new();
                for id in targets {
                    let Some(ph) = s.catalog.photo(id) else { continue };
                    let mut m = ph.meta.clone();
                    for (k, field) in [
                        ("title", &mut m.title),
                        ("caption", &mut m.caption),
                        ("copyright", &mut m.copyright),
                        ("creator", &mut m.creator),
                        ("location", &mut m.location),
                    ] {
                        if let Some(v) = str_param(p, k) {
                            *field = v.to_string();
                        }
                    }
                    if let Some(k) = strs("keywords") {
                        m.keywords = k;
                    }
                    for k in strs("addKeywords").unwrap_or_default() {
                        if !m.keywords.iter().any(|x| x.eq_ignore_ascii_case(&k)) {
                            m.keywords.push(k);
                        }
                    }
                    for k in strs("removeKeywords").unwrap_or_default() {
                        m.keywords.retain(|x| !x.eq_ignore_ascii_case(&k));
                    }
                    ops.push(Op::SetMeta { id, meta: Box::new(m) });
                }
                let n = ops.len();
                s.commit("Edit Info", Op::Batch { ops })?;
                Ok(json!({"changed": n}))
            }
        ),
        // ---- albums
        cmd!("album.create", "New Album", ["File"], None, "{name, parent?: folderId, folder?: bool, addSelected?: bool}", always, |s, p| {
            let name = str_param(p, "name").unwrap_or("Untitled Album").trim().to_string();
            if name.is_empty() {
                return Err(bad("album.create", "empty name"));
            }
            let folder = bool_or(p, "folder", false);
            let parent = p.get("parent").and_then(Value::as_u64).map(AlbumId);
            let photos = if !folder && bool_or(p, "addSelected", false) { s.targets(&Value::Null) } else { vec![] };
            let id = s.catalog.alloc_album_id();
            let cover = photos.first().copied();
            s.commit(
                if folder { "New Folder" } else { "New Album" },
                Op::AddAlbum { album: Album { id, name, parent, folder, photos, cover, smart: None } },
            )?;
            Ok(json!({"id": id.0}))
        }),
        cmd!(
            "album.createSmart",
            "New Smart Album…",
            [],
            None,
            "{name, rules?: partial Filter (rating, ratingOp, flag, label, kind, edited, keyword, camera, lens, dateFrom, dateTo, date, text, album), parent?: folderId} — without `rules`, saves the current view (source + filter)",
            always,
            |s, p| {
                let name = str_param(p, "name").unwrap_or("Smart Album").trim().to_string();
                if name.is_empty() {
                    return Err(bad("album.createSmart", "empty name"));
                }
                let rules = match p.get("rules") {
                    Some(r) => merge_rules(&lightcraft_catalog::Filter::default(), r, "album.createSmart")?,
                    None => view_rules(s),
                };
                let parent = p.get("parent").and_then(Value::as_u64).map(AlbumId);
                let id = s.catalog.alloc_album_id();
                let album = Album { parent, smart: Some(Box::new(rules)), ..Album::new(id, name) };
                s.commit("New Smart Album", Op::AddAlbum { album })?;
                Ok(json!({"id": id.0, "count": s.catalog.album_count(id)}))
            }
        ),
        cmd!(
            "album.setRules",
            "Edit Smart Album",
            [],
            None,
            "{id, rules?: partial Filter merged onto the current rules (null clears a field), replace?: bool, fromView?: bool (use the current view)}",
            always,
            |s, p| {
                let id = album_param(p, "id", "album.setRules")?;
                let cur = s.catalog.album(id).and_then(|a| a.smart.as_deref().cloned()).ok_or_else(|| bad("album.setRules", "not a smart album"))?;
                let rules = if bool_or(p, "fromView", false) {
                    view_rules(s)
                } else {
                    let base = if bool_or(p, "replace", false) { Default::default() } else { cur };
                    merge_rules(&base, p.get("rules").unwrap_or(&Value::Null), "album.setRules")?
                };
                s.commit("Edit Smart Album", Op::SetAlbumRules { id, rules: Box::new(rules) })?;
                Ok(json!({"count": s.catalog.album_count(id)}))
            }
        ),
        cmd!("album.rename", "Rename Album", [], None, "{id, name}", always, |s, p| {
            let id = album_param(p, "id", "album.rename")?;
            let name = str_param(p, "name").ok_or_else(|| bad("album.rename", "missing name"))?.to_string();
            s.commit("Rename Album", Op::RenameAlbum { id, name })?;
            ok()
        }),
        cmd!("album.delete", "Delete Album", [], None, "{id}", always, |s, p| {
            let id = album_param(p, "id", "album.delete")?;
            s.commit("Delete Album", Op::RemoveAlbum { id })?;
            if s.source == LibrarySource::Album(id) {
                s.source = LibrarySource::All;
            }
            ok()
        }),
        cmd!("album.move", "Move Album", [], None, "{id, parent?: folderId|null}", always, |s, p| {
            let id = album_param(p, "id", "album.move")?;
            let parent = p.get("parent").and_then(Value::as_u64).map(AlbumId);
            s.commit("Move Album", Op::MoveAlbum { id, parent })?;
            ok()
        }),
        cmd!("album.addPhotos", "Add to Album", ["Photo"], None, "{id: albumId, ids?: [photoIds]} (default: selection)", has_selection, |s, p| {
            let id = album_param(p, "id", "album.addPhotos")?;
            let targets = ids_param(p).unwrap_or_else(|| s.targets(&Value::Null));
            let al = s.catalog.album(id).ok_or_else(|| bad("album.addPhotos", "no such album"))?;
            if al.is_smart() || al.folder {
                return Err(bad("album.addPhotos", "smart albums and folders can't hold photos"));
            }
            let mut photos = al.photos.clone();
            let before = photos.len();
            for t in targets {
                if !photos.contains(&t) {
                    photos.push(t);
                }
            }
            let added = photos.len() - before;
            let cover = al.cover.or(photos.first().copied());
            s.commit("Add to Album", Op::Batch { ops: vec![Op::SetAlbumPhotos { id, photos }, Op::SetAlbumCover { id, cover }] })?;
            Ok(json!({"added": added}))
        }),
        cmd!("album.removePhotos", "Remove from Album", [], None, "{id: albumId, ids?}", has_selection, |s, p| {
            let id = album_param(p, "id", "album.removePhotos")?;
            let targets = ids_param(p).unwrap_or_else(|| s.targets(&Value::Null));
            let al = s.catalog.album(id).ok_or_else(|| bad("album.removePhotos", "no such album"))?;
            if al.is_smart() {
                return Err(bad("album.removePhotos", "smart albums update automatically: change their rules"));
            }
            let photos: Vec<PhotoId> = al.photos.iter().copied().filter(|x| !targets.contains(x)).collect();
            s.commit("Remove from Album", Op::SetAlbumPhotos { id, photos })?;
            ok()
        }),
        cmd!("album.setCover", "Set as Album Cover", [], None, "{id: albumId, photo?: photoId}", has_active, |s, p| {
            let id = album_param(p, "id", "album.setCover")?;
            let photo = p.get("photo").and_then(Value::as_u64).map(PhotoId).or(s.active());
            s.commit("Set Album Cover", Op::SetAlbumCover { id, cover: photo })?;
            ok()
        }),
        // ---- import
        cmd!(
            query "library.importPreview",
            "Review Import",
            [],
            None,
            "{paths: [file or folder (recursive)]} → {candidates: [{path, name, format, kind, width, height, fileSize, captured, duplicate?: path|content, existing?, error?}], duplicates, scanned} — nothing is added",
            always,
            |s, p| {
                let paths = strs(p, "paths");
                if paths.is_empty() {
                    return Err(bad("library.importPreview", "no paths"));
                }
                let c = crate::import::scan(s, &paths);
                let dups = c.iter().filter(|c| c.duplicate.is_some()).count();
                Ok(json!({"scanned": c.len(), "duplicates": dups, "candidates": c}))
            }
        ),
        cmd!(
            "library.import",
            "Add Photos…",
            ["File"],
            Some("Cmd+Shift+I"),
            "{paths: [file or folder (recursive)], mode?: add|copy (add = reference the files in place; copy = into the library's Originals/YYYY/YYYY-MM-DD/), album?: albumId, albumName?: new album, preset?: presetId, keywords?: [..]} → {imported, duplicates, failed, album?}",
            always,
            |s, p| {
                let paths = strs(p, "paths");
                if paths.is_empty() {
                    return Err(bad("library.import", "no paths"));
                }
                let mode = match str_param(p, "mode").unwrap_or("add") {
                    "add" => crate::import::ImportMode::Add,
                    "copy" => crate::import::ImportMode::Copy,
                    other => return Err(bad("library.import", format!("unknown mode `{other}` (add|copy)"))),
                };
                let preset = match str_param(p, "preset").filter(|x| !x.is_empty()) {
                    Some(id) => {
                        Some(s.presets.iter().find(|x| x.id == id).cloned().ok_or_else(|| bad("library.import", format!("unknown preset `{id}`")))?)
                    }
                    None => None,
                };
                let mut album = p.get("album").and_then(Value::as_u64);
                if let Some(a) = album
                    && s.catalog.album(AlbumId(a)).is_none_or(|al| al.folder || al.is_smart())
                {
                    return Err(bad("library.import", "album must be a regular album"));
                }
                let opts = crate::import::ImportOptions { mode, preset, keywords: strs(p, "keywords") };
                let undo0 = s.undo.len();
                let mut report = serde_json::to_value(crate::import::import_with(s, &paths, &opts)?).unwrap_or_default();
                let imported: Vec<u64> = report["imported"].as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
                if album.is_none()
                    && let Some(name) = str_param(p, "albumName").map(str::trim).filter(|n| !n.is_empty())
                    && !imported.is_empty()
                {
                    let r = s.execute("album.create", &json!({"name": name}))?;
                    album = r["id"].as_u64();
                }
                if let Some(a) = album
                    && !imported.is_empty()
                {
                    s.execute("album.addPhotos", &json!({"id": a, "ids": imported}))?;
                    report["album"] = json!(a);
                }
                // one undo step for the whole import
                let n = s.undo.len().saturating_sub(undo0);
                s.merge_undo(n, &format!("Add {} Photo{}", imported.len(), if imported.len() == 1 { "" } else { "s" }));
                if let Some(f) = imported.first() {
                    s.selection = Selection::single(PhotoId(*f));
                }
                Ok(report)
            }
        ),
        // ---- persistence
        cmd!(query "library.info", "Library Info", [], None, "{}", always, |s, _| {
            let photos = s.catalog.len();
            let albums = s.catalog.albums().count();
            let (rendered_n, rendered_bytes) = s.media.rendered.mem_usage();
            let (sources_n, sources_bytes) = s.media.source_usage();
            let disk = s.media.rendered.disk().map(|d| {
                use std::sync::atomic::Ordering::Relaxed;
                json!({
                    "path": d.dir().display().to_string(),
                    "bytes": d.size(),
                    "hits": d.hits.load(Relaxed),
                    "misses": d.misses.load(Relaxed),
                    "writes": d.writes.load(Relaxed),
                })
            });
            let cache = json!({
                "renderedInMemory": rendered_n, "renderedBytes": rendered_bytes,
                "sourcesInMemory": sources_n, "sourceBytes": sources_bytes,
                "disk": disk,
            });
            let Some(lib) = &s.library else {
                return Ok(json!({"persistent": false, "photos": photos, "albums": albums, "cache": cache}));
            };
            let j = lib.journal();
            Ok(json!({
                "persistent": true,
                "path": lib.dir.display().to_string(),
                "photos": photos,
                "albums": albums,
                "seq": j.seq(),
                "snapshotSeq": j.snapshot_seq(),
                "logRecords": j.log_records(),
                "logBytes": j.log_bytes(),
                "lastError": lib.last_error,
                "cache": cache,
                "load": {
                    "created": lib.report.created,
                    "replayed": lib.report.replayed,
                    "tornBytes": lib.report.torn_bytes,
                    "damaged": lib.report.damaged,
                },
            }))
        }),
        cmd!("library.compact", "Optimize Library", ["File"], None, "{}", has_library, |s, _| {
            s.compact_library()?;
            ok()
        }),
        cmd!("library.clearPreviews", "Clear Preview Cache", ["File"], None, "{}", always, |s, _| {
            s.media.rendered.clear();
            ok()
        }),
    ]
}

/// `base` with a partial Filter (JSON) merged on top.
fn merge_rules(base: &lightcraft_catalog::Filter, patch: &Value, c: &str) -> Result<lightcraft_catalog::Filter> {
    let mut v = serde_json::to_value(base).unwrap_or_default();
    lightcraft_develop::presets::deep_merge(&mut v, patch);
    serde_json::from_value(v).map_err(|e| bad(c, e.to_string()))
}

/// The current view (source + filter) as smart-album rules. Viewing a smart album starts from
/// its rules with the filter bar's settings on top.
fn view_rules(s: &Session) -> lightcraft_catalog::Filter {
    use lightcraft_catalog::Filter;
    let smart = match s.source {
        LibrarySource::Album(a) => s.catalog.album(a).and_then(|a| a.smart.as_deref().cloned()),
        _ => None,
    };
    match smart {
        Some(base) => {
            // overlay the fields the filter bar changed
            let cur = serde_json::to_value(&s.filter).unwrap_or_default();
            let def = serde_json::to_value(Filter::default()).unwrap_or_default();
            let mut patch = serde_json::Map::new();
            if let (Some(c), Some(d)) = (cur.as_object(), def.as_object()) {
                for (k, v) in c {
                    if d.get(k) != Some(v) {
                        patch.insert(k.clone(), v.clone());
                    }
                }
            }
            merge_rules(&base, &Value::Object(patch), "").unwrap_or(base)
        }
        None => {
            let mut f = s.source.to_filter(&s.filter, &s.catalog);
            f.deleted = false;
            f
        }
    }
}

impl Session {
    /// The current view (source + filter) as smart-album rules (see `album.createSmart`).
    pub fn view_rules(&self) -> lightcraft_catalog::Filter {
        view_rules(self)
    }
}

fn has_library(s: &Session) -> std::result::Result<(), String> {
    if s.library.is_some() { Ok(()) } else { Err("no library is open (in-memory session)".into()) }
}
