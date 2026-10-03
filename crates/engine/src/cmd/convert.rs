//! Convert to DNG: a raw photo's original is re-encoded as a lossless DNG (its develop settings
//! embedded as XMP) next to it, and the photo is relinked to the DNG. The original stays on
//! disk; undo relinks the photo to it.

use std::path::Path;

use lightcraft_catalog::{MediaKind, Op, Source};
use serde_json::{Value, json};

use super::{CommandSpec, bad, cmd, has_active, has_selection};
use crate::{Result, Session};

/// A free `<stem>.dng` (then `<stem>-2.dng`…) next to `path`.
fn dng_path(path: &str) -> String {
    let p = Path::new(path);
    let dir = p.parent().unwrap_or(Path::new(""));
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "photo".into());
    let mut out = dir.join(format!("{stem}.dng"));
    let mut n = 2;
    while out.exists() {
        out = dir.join(format!("{stem}-{n}.dng"));
        n += 1;
    }
    out.to_string_lossy().to_string()
}

/// Write the DNG for raw file `path` (develop settings in `packet`); returns the new path.
/// An empty `packet` writes no XMP.
pub(crate) fn write_dng_for(s: &Session, path: &str, packet: String) -> std::result::Result<String, String> {
    let bytes = match &s.media.file_bytes {
        Some(r) => r(path)?,
        None => std::fs::read(path).map_err(|e| format!("{path}: {e}"))?,
    };
    let raw = lightcraft_raw::decode(&bytes).map_err(|e| format!("{path}: {e}"))?;
    drop(bytes);
    let dng = lightcraft_raw::write_dng(&raw, &lightcraft_raw::DngWriteOptions { xmp: (!packet.is_empty()).then_some(packet), ..Default::default() })
        .map_err(|e| e.to_string())?;
    let out = dng_path(path);
    std::fs::write(&out, dng).map_err(|e| format!("{out}: {e}"))?;
    Ok(out)
}

fn convert(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "photo.convertToDng";
    let ids = s.targets(p);
    let mut ops = Vec::new();
    let mut converted = Vec::new();
    let mut skipped = Vec::new();
    for id in ids {
        let Some(ph) = s.catalog.photo(id).cloned() else { continue };
        let Source::File { path } = &ph.source else {
            skipped.push(json!([id.0, "not a file"]));
            continue;
        };
        if ph.kind != MediaKind::Raw || ph.format.eq_ignore_ascii_case("DNG") || ph.copy_of.is_some() {
            skipped.push(json!([
                id.0,
                if ph.kind != MediaKind::Raw {
                    "not a raw photo"
                } else if ph.copy_of.is_some() {
                    "a virtual copy"
                } else {
                    "already a DNG"
                }
            ]));
            continue;
        }
        let packet = crate::sidecar::sidecar_packet(&ph, &s.catalog);
        match write_dng_for(s, path, packet) {
            Ok(out) => {
                let file_name = Path::new(&out).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                // virtual copies of this photo follow it to the DNG
                for c in s.catalog.photos().filter(|c| c.copy_of == Some(id)) {
                    ops.push(Op::Relink {
                        id: c.id,
                        file_name: file_name.clone(),
                        source: Source::File { path: out.clone() },
                        format: Some("DNG".into()),
                    });
                }
                ops.push(Op::Relink { id, file_name, source: Source::File { path: out.clone() }, format: Some("DNG".into()) });
                converted.push(json!({"id": id.0, "path": out, "original": path}));
            }
            Err(e) => skipped.push(json!([id.0, e])),
        }
    }
    if !ops.is_empty() {
        let n = converted.len();
        s.commit(&format!("Convert {n} Photo{} to DNG", if n == 1 { "" } else { "s" }), Op::Batch { ops }).map_err(|e| bad(C, e.to_string()))?;
    }
    Ok(json!({"converted": converted, "skipped": skipped}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "photo.convertToDng",
        "Convert to DNG",
        ["Photo"],
        None,
        "{ids?} — write each raw photo as a lossless DNG next to it (settings embedded) and relink the photo to it; originals are kept → {converted: [{id, path, original}], skipped}",
        has_selection,
        convert
    )]
}

/// Edit in an external editor, the engine half: render the photo with its edits as a 16-bit
/// TIFF next to the original (`<name>-Edit.tif`, never overwriting), add it to the library and
/// stack it on top of the original. The host opens the file in the editor.
fn edit_external(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "photo.editExternal";
    let id = s.active().ok_or_else(|| bad(C, "no active photo"))?;
    let ph = s.catalog.photo(id).cloned().ok_or_else(|| bad(C, "no photo"))?;
    let dir = match &ph.source {
        Source::File { path } => Path::new(path).parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default(),
        Source::Demo { .. } => match super::str_param(p, "dir") {
            Some(d) => d.to_string(),
            None => return Err(bad(C, "a generated demo photo has no folder: give `dir`")),
        },
    };
    let space = match super::str_param(p, "colorSpace").unwrap_or("adobeRgb") {
        "srgb" => lightcraft_pipeline::OutputSpace::Srgb,
        "displayP3" => lightcraft_pipeline::OutputSpace::DisplayP3,
        "prophoto" | "proPhoto" => lightcraft_pipeline::OutputSpace::ProPhoto,
        _ => lightcraft_pipeline::OutputSpace::AdobeRgb,
    };
    let opts = crate::export::ExportOptions {
        format: crate::export::ExportFormat::Tiff,
        bit_depth: Some(16),
        color_space: space,
        naming: "{name}-Edit".into(),
        conflict: crate::export::Conflict::Unique,
        ..Default::default()
    };
    let mut written = Vec::new();
    let mut write = |path: &str, bytes: &[u8]| -> std::result::Result<(), String> {
        std::fs::write(path, bytes).map_err(|e| format!("{path}: {e}"))?;
        written.push(path.to_string());
        Ok(())
    };
    let to = crate::export::Destination { dir, exact: None };
    crate::export::export_batch(s, &[id], &opts, &to, &mut write, &|path| Path::new(path).exists()).map_err(|e| bad(C, e))?;
    let out = written.into_iter().find(|w| w.ends_with(".tif")).ok_or_else(|| bad(C, "nothing was written"))?;
    let r = s.execute("library.import", &json!({"paths": [out]}))?;
    let new = r["imported"].get(0).and_then(Value::as_u64).ok_or_else(|| bad(C, "the edit copy could not be added"))?;
    // stack: the edit on top of the original, expanded so both show
    let _ = s.execute("stack.group", &json!({"ids": [new, id.0], "top": new, "collapsed": false}));
    s.selection = crate::Selection::single(lightcraft_catalog::PhotoId(new));
    Ok(json!({"path": out, "id": new, "original": id.0}))
}

/// Re-read photos whose files changed on disk (an external editor saved them): new size,
/// dimensions and content hash, cached sources dropped. → {reloaded: [ids]}
fn reload(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = s.targets(p);
    let paths: Vec<(lightcraft_catalog::PhotoId, String)> = ids
        .iter()
        .filter_map(|id| match s.catalog.photo(*id).map(|ph| ph.source.clone()) {
            Some(Source::File { path }) => Some((*id, path)),
            _ => None,
        })
        .collect();
    let probed = crate::import::probe_paths(s, &paths.iter().map(|(_, p)| p.clone()).collect::<Vec<_>>());
    let mut ops = Vec::new();
    let mut reloaded = Vec::new();
    for ((id, _), info) in paths.into_iter().zip(probed) {
        let (Ok(info), Some(ph)) = (info, s.catalog.photo(id)) else { continue };
        if info.content_hash == ph.content_hash && info.file_size == ph.file_size && (info.width, info.height) == (ph.width, ph.height) {
            continue;
        }
        ops.push(Op::SetContent { id, width: info.width, height: info.height, file_size: info.file_size, content_hash: info.content_hash });
        // virtual copies share the file
        for c in s.catalog.photos().filter(|c| c.copy_of == Some(id)) {
            reloaded.push(c.id);
        }
        reloaded.push(id);
    }
    for id in &reloaded {
        s.media.forget(*id);
    }
    if !ops.is_empty() {
        s.commit("Reload Changed Files", Op::Batch { ops })?;
    }
    Ok(json!({"reloaded": reloaded.iter().map(|i| i.0).collect::<Vec<_>>()}))
}

/// Duplicate: copy the file next to itself (`<name>-copy`, never overwriting) and add it with the
/// same settings, metadata, rating, flag, label and albums. → {ids}
fn duplicate(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "photo.duplicate";
    let mut made = Vec::new();
    for id in s.targets(p) {
        let Some(ph) = s.catalog.photo(id).map(|p| (**p).clone()) else { continue };
        let Source::File { path } = &ph.source else { return Err(bad(C, "a generated demo photo has no file to duplicate")) };
        let src = Path::new(path);
        let stem = src.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "photo".into());
        let ext = src.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        let dir = src.parent().unwrap_or(Path::new(""));
        let mut dst = dir.join(format!("{stem}-copy{ext}"));
        let mut n = 2;
        while dst.exists() {
            dst = dir.join(format!("{stem}-copy-{n}{ext}"));
            n += 1;
        }
        std::fs::copy(src, &dst).map_err(|e| bad(C, format!("{path}: {e}")))?;
        let new = s.catalog.alloc_photo_id();
        let mut q = ph.clone();
        q.id = new;
        q.source = Source::File { path: dst.to_string_lossy().to_string() };
        q.file_name = dst.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
        // the same bytes, but its own photo: no duplicate-content clash, no shared history
        q.content_hash = q.content_hash.map(|h| format!("{h}:dup{}", new.0));
        q.copy_of = None;
        q.copy_name = None;
        q.history.clear();
        q.versions.retain(|v| !v.auto);
        let mut ops = vec![Op::AddPhoto { photo: Box::new(q) }];
        for a in s.catalog.albums().filter(|a| !a.is_smart() && !a.folder && a.photos.contains(&id)) {
            let mut photos = a.photos.clone();
            photos.push(new);
            ops.push(Op::SetAlbumPhotos { id: a.id, photos });
        }
        s.commit("Duplicate", Op::Batch { ops })?;
        made.push(new);
    }
    s.merge_undo(made.len(), "Duplicate");
    if let Some(last) = made.last() {
        s.selection = crate::Selection { ids: made.clone(), active: Some(*last) };
    }
    Ok(json!({"ids": made.iter().map(|i| i.0).collect::<Vec<_>>()}))
}

pub fn edit_specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "photo.duplicate",
            "Duplicate",
            ["Photo"],
            None,
            "{ids?} — copy each photo's file next to it (`-copy`) and add it with the same settings, metadata and albums (a real file, unlike a virtual copy) → {ids}",
            has_selection,
            duplicate
        ),
        cmd!(
            "photo.reload",
            "Reload from Disk",
            [],
            None,
            "{ids?} — re-read photos whose files changed on disk (e.g. saved by an external editor) → {reloaded}",
            has_selection,
            reload
        ),
        cmd!(
            "photo.editExternal",
            "Edit Copy for External Editor",
            [],
            None,
            "{colorSpace?: adobeRgb (default) | proPhoto | displayP3 | srgb, dir?} — render the active photo with its edits as a 16-bit TIFF `<name>-Edit.tif` next to it, add it stacked on the original and select it → {path, id, original} (the app then opens it in the external editor)",
            has_active,
            edit_external
        ),
    ]
}
