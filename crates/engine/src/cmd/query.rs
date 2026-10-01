//! Read-only queries (not journaled): catalog, photos, develop state, controls, presets, albums.

use lightcraft_catalog::{Album, Photo, PhotoId};
use lightcraft_develop::{CONTROLS, controls};
use serde_json::{Value, json};

use super::{CommandSpec, always, bad, cmd, has_active};
use crate::Session;

pub fn photo_summary(p: &Photo) -> Value {
    json!({
        "id": p.id.0,
        "fileName": p.file_name,
        "format": p.format,
        "kind": p.kind,
        "width": p.width,
        "height": p.height,
        "captured": p.captured,
        "rating": p.rating,
        "flag": p.flag,
        "label": p.label,
        "edited": p.is_edited(),
        "title": p.meta.title,
        "keywords": p.meta.keywords,
        "camera": p.meta.camera,
        "deleted": p.deleted,
    })
}

fn album_json(a: &Album, all: &[Album]) -> Value {
    json!({
        "id": a.id.0,
        "name": a.name,
        "folder": a.folder,
        "count": a.photos.len(),
        "cover": a.cover.map(|c| c.0),
        "children": all.iter().filter(|c| c.parent == Some(a.id)).map(|c| album_json(c, all)).collect::<Vec<_>>(),
    })
}

fn photo_arg(s: &Session, p: &Value, c: &str) -> crate::Result<PhotoId> {
    p.get("id").and_then(Value::as_u64).map(PhotoId).or(s.active()).ok_or_else(|| bad(c, "no photo"))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "catalog.query", "Query Photos", [], None, "{filter?: Filter, sort?: Sort, offset?, limit?} — omit filter to list the current view", always, |s, p| {
            let ids = if p.get("filter").is_some() || p.get("sort").is_some() {
                let f = p.get("filter").map(|f| serde_json::from_value(f.clone())).transpose().map_err(|e| bad("catalog.query", e.to_string()))?.unwrap_or_default();
                let so = p.get("sort").map(|f| serde_json::from_value(f.clone())).transpose().map_err(|e| bad("catalog.query", e.to_string()))?.unwrap_or_default();
                s.catalog.query(&f, &so)
            } else {
                s.visible_cloned()
            };
            let off = p.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
            let lim = p.get("limit").and_then(Value::as_u64).unwrap_or(200) as usize;
            let items: Vec<Value> = ids.iter().skip(off).take(lim).filter_map(|id| s.catalog.photo(*id)).map(|p| photo_summary(p)).collect();
            Ok(json!({"total": ids.len(), "photos": items}))
        }),
        cmd!(query "catalog.stats", "Catalog Statistics", [], None, "{}", always, |s, _| {
            let all: Vec<_> = s.catalog.photos().filter(|p| !p.deleted).collect();
            Ok(json!({
                "photos": all.len(),
                "edited": all.iter().filter(|p| p.is_edited()).count(),
                "picks": all.iter().filter(|p| p.flag == lightcraft_catalog::Flag::Pick).count(),
                "rejects": all.iter().filter(|p| p.flag == lightcraft_catalog::Flag::Reject).count(),
                "deleted": s.catalog.photos().filter(|p| p.deleted).count(),
                "albums": s.catalog.albums().filter(|a| !a.folder).count(),
                "byDate": s.catalog.date_groups(),
                "keywords": s.catalog.keywords(),
            }))
        }),
        cmd!(query "library.state", "Library State", [], None, "{}", always, |s, _| {
            let label = s.source.label(&s.catalog);
            let n = s.visible().len();
            Ok(json!({
                "source": s.source,
                "sourceLabel": label,
                "filter": s.filter,
                "sort": s.sort,
                "selection": s.selection,
                "visibleCount": n,
                "activeMask": s.active_mask,
                "undo": s.undo.last().map(|u| u.label.clone()),
                "redo": s.redo.last().map(|u| u.label.clone()),
                "clipboard": s.clipboard.is_some(),
            }))
        }),
        cmd!(query "albums.list", "List Albums", [], None, "{}", always, |s, _| {
            let all: Vec<Album> = s.catalog.albums().cloned().collect();
            Ok(Value::Array(all.iter().filter(|a| a.parent.is_none()).map(|a| album_json(a, &all)).collect()))
        }),
        cmd!(query "photo.inspect", "Inspect Photo", [], None, "{id?}", always, |s, p| {
            let id = photo_arg(s, p, "photo.inspect")?;
            let ph = s.catalog.photo(id).ok_or_else(|| bad("photo.inspect", "no such photo"))?;
            let mut v = serde_json::to_value(ph.as_ref()).unwrap_or_default();
            v["albums"] = json!(s.catalog.albums_of(id).iter().map(|a| a.0).collect::<Vec<_>>());
            v["history"] = json!(ph.history.iter().map(|h| h.label.clone()).collect::<Vec<_>>());
            Ok(v)
        }),
        cmd!(query "develop.get", "Get Develop Settings", [], None, "{id?}", always, |s, p| {
            let id = photo_arg(s, p, "develop.get")?;
            Ok(s.develop_of(id).map(|d| d.to_json()).unwrap_or(Value::Null))
        }),
        cmd!(query "develop.controls", "List Develop Controls", [], None, "{section?} — every slider with range, default and current value", always, |s, p| {
            let d = s.active().and_then(|id| s.develop_of(id)).unwrap_or_default();
            let sec = p.get("section").and_then(|v| serde_json::from_value::<lightcraft_develop::Section>(v.clone()).ok());
            Ok(Value::Array(
                CONTROLS
                    .iter()
                    .filter(|c| sec.is_none_or(|x| x == c.section))
                    .map(|c| {
                        let mut v = serde_json::to_value(c).unwrap_or_default();
                        v["value"] = json!(controls::get(&d, c.id));
                        v
                    })
                    // indexed controls of the list elements that exist (`pointColor.0.hueShift`, …)
                    .chain(controls::indexed_instances(&d).into_iter().filter(|(_, c)| sec.is_none_or(|x| x == c.section)).map(|(id, c)| {
                        let mut v = serde_json::to_value(c).unwrap_or_default();
                        v["value"] = json!(controls::get(&d, &id));
                        v["id"] = json!(id);
                        v
                    }))
                    .collect(),
            ))
        }),
        cmd!(query "presets.list", "List Presets", [], None, "{}", always, |s, _| {
            Ok(Value::Array(s.presets.iter().map(|p| json!({"id": p.id, "name": p.name, "group": p.group, "favorite": p.favorite, "builtin": p.builtin})).collect()))
        }),
        cmd!(query "profiles.list", "List Profiles", [], None, "{}", always, |_, _| Ok(serde_json::to_value(crate::presets::PROFILES).unwrap_or_default())),
        cmd!(query "history.list", "List History", [], None, "{id?}", has_active, |s, p| {
            let id = photo_arg(s, p, "history.list")?;
            let ph = s.catalog.photo(id).ok_or_else(|| bad("history.list", "no such photo"))?;
            Ok(json!({
                "history": ph.history.iter().map(|h| h.label.clone()).collect::<Vec<_>>(),
                "versions": ph.versions.iter().map(|v| json!({"name": v.name, "created": v.created})).collect::<Vec<_>>(),
            }))
        }),
        cmd!(query "app.gpu", "GPU Rendering", [], None, "{enabled?: bool} — allow/forbid GPU rendering (CPU fallback; LIGHTCRAFT_GPU=0 forbids it for the process)", always, |_, p| {
            if let Some(on) = p.get("enabled").and_then(Value::as_bool) {
                lightcraft_gpu::set_enabled(on);
            }
            Ok(json!({"enabled": lightcraft_gpu::enabled(), "available": lightcraft_gpu::available(), "adapter": lightcraft_gpu::adapter_name()}))
        }),
        cmd!(query "journal.list", "Command Journal", [], None, "{limit?}", always, |s, p| {
            let lim = p.get("limit").and_then(Value::as_u64).unwrap_or(100) as usize;
            let start = s.journal.len().saturating_sub(lim);
            Ok(Value::Array(s.journal[start..].iter().map(|(c, p)| json!({"command": c, "params": p})).collect()))
        }),
    ]
}
