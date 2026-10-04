//! Batch rename: a file-name template applied to the selected photos, renaming the files on
//! disk (with their XMP sidecars) safely.
//!
//! Template tokens (shared with import renaming and export file naming): `{name}` (current name
//! without extension), `{num}` (the number at the end of the name: `IMG_0042` → `0042`), `{seq}` /
//! `{seq:3}` (sequence number, zero-padded to 3 digits), `{date}` / `{date:%Y%m%d}` (capture date;
//! `%Y %y %m %d %H %M %S`), `{folder}` (the original's folder name), `{camera}`, `{lens}`, `{iso}`,
//! `{rating}`, `{title}`, `{creator}`, `{ext}` (extension, without the dot). Unknown tokens stay as
//! typed. Characters that aren't allowed in file names become `-`. The original extension is always
//! kept.
//!
//! Safety:
//! - a target that exists on disk, or that another photo in the batch gets, receives a `-1`, `-2`…
//!   suffix — no file is ever overwritten;
//! - the moves happen before the catalog changes; if one fails, the ones already done are moved
//!   back and nothing is committed;
//! - the catalog op ([`Op::SetFile`]) is undoable: undo/redo move the files back and forth (see
//!   [`crate::Session::undo_step`]);
//! - virtual copies follow their master's file.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use lightcraft_catalog::{Op, Photo, PhotoId, Source};
use serde::Serialize;

use crate::sidecar::SidecarNaming;
use crate::{EngineError, Result, Session};

/// One planned rename.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RenamePlan {
    pub id: u64,
    pub from: String,
    pub to: String,
    /// File sources: the old and new paths.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_path: Option<String>,
}

fn split_ext(name: &str) -> (&str, &str) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, ext),
        _ => (name, ""),
    }
}

fn sanitize(s: &str) -> String {
    let t: String =
        s.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '-' } else { c }).collect();
    t.trim().trim_matches('.').trim().to_string()
}

/// `%Y%m%d`-style formatting of an ISO local time.
fn format_date(iso: &str, fmt: &str) -> String {
    let part = |a: usize, b: usize| iso.get(a..b).unwrap_or("00").to_string();
    let mut out = String::new();
    let mut it = fmt.chars();
    while let Some(c) = it.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('Y') => out.push_str(&part(0, 4)),
            Some('y') => out.push_str(&part(2, 4)),
            Some('m') => out.push_str(&part(5, 7)),
            Some('d') => out.push_str(&part(8, 10)),
            Some('H') => out.push_str(&part(11, 13)),
            Some('M') => out.push_str(&part(14, 16)),
            Some('S') => out.push_str(&part(17, 19)),
            Some('%') => out.push('%'),
            Some(o) => {
                out.push('%');
                out.push(o);
            }
            None => out.push('%'),
        }
    }
    out
}

/// Expand `template` for `p` (sequence number `seq`) into a file name with `p`'s extension.
pub fn expand(template: &str, p: &Photo, seq: usize) -> String {
    let (stem, ext) = split_ext(&p.file_name);
    let out = expand_tokens(template, p, seq, 1);
    let mut stem_out = sanitize(&out);
    if stem_out.is_empty() {
        stem_out = sanitize(stem);
    }
    if stem_out.is_empty() {
        stem_out = "photo".into();
    }
    if ext.is_empty() { stem_out } else { format!("{stem_out}.{ext}") }
}

/// The template's tokens replaced for `p` (no extension added, nothing sanitized). A bare `{seq}`
/// is zero-padded to `seq_width` digits.
pub fn expand_tokens(template: &str, p: &Photo, seq: usize, seq_width: usize) -> String {
    let (stem, ext) = split_ext(&p.file_name);
    let mut out = String::new();
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('}') else {
            out.push_str(&rest[i..]);
            rest = "";
            break;
        };
        let tok = &rest[i + 1..i + j];
        let (name, arg) = tok.split_once(':').map(|(a, b)| (a, Some(b))).unwrap_or((tok, None));
        let v = match name.trim().to_ascii_lowercase().as_str() {
            "name" | "filename" => stem.to_string(),
            "num" => {
                let digits = stem.chars().rev().take_while(char::is_ascii_digit).count();
                stem[stem.len() - digits..].to_string()
            }
            "seq" | "n" => {
                let w: usize = arg.and_then(|a| a.parse().ok()).unwrap_or(seq_width).min(9);
                format!("{seq:0w$}")
            }
            "date" => format_date(p.date(), arg.unwrap_or("%Y%m%d")),
            "folder" => match &p.source {
                Source::File { path } => {
                    Path::new(path).parent().and_then(Path::file_name).map(|f| f.to_string_lossy().to_string()).unwrap_or_default()
                }
                Source::Demo { .. } => String::new(),
            },
            "camera" => p.meta.camera.clone(),
            "lens" => p.meta.lens.clone(),
            "iso" => p.meta.iso.map(|v| v.to_string()).unwrap_or_default(),
            "rating" => p.rating.to_string(),
            "title" => p.meta.title.clone(),
            "creator" => p.meta.creator.clone(),
            "ext" => ext.to_string(),
            _ => format!("{{{tok}}}"),
        };
        out.push_str(&v);
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    out
}

/// Every sidecar that may belong to `path` (both naming conventions, `.xmp` and `.XMP`).
fn sidecars(path: &str) -> Vec<(PathBuf, SidecarNaming, bool)> {
    let mut v = Vec::new();
    for naming in [SidecarNaming::Stem, SidecarNaming::Full] {
        let p = crate::sidecar::sidecar_path(path, naming);
        v.push((p.with_extension("XMP"), naming, true));
        v.push((p, naming, false));
    }
    v
}

/// Move `from` to `to` (never over an existing file) together with its sidecars. On error nothing
/// is left moved.
pub fn move_file(from: &str, to: &str) -> std::result::Result<(), String> {
    if from == to {
        return Ok(());
    }
    let (f, t) = (Path::new(from), Path::new(to));
    // a case-only change on a case-insensitive file system "exists" already: that is this file
    let case_only = from.to_lowercase() == to.to_lowercase();
    if t.exists() && !case_only {
        return Err(format!("{to} already exists"));
    }
    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mv = |a: &Path, b: &Path| -> std::io::Result<()> {
        if case_only {
            // via a temporary name, so case-insensitive file systems see a real change
            let tmp = a.with_file_name(format!(".lc-rename-{}", std::process::id()));
            std::fs::rename(a, &tmp)?;
            std::fs::rename(&tmp, b).inspect_err(|_| {
                let _ = std::fs::rename(&tmp, a);
            })
        } else {
            std::fs::rename(a, b)
        }
    };
    mv(f, t).map_err(|e| format!("rename {from}: {e}"))?;
    done.push((f.to_path_buf(), t.to_path_buf()));
    for (sc, naming, upper) in sidecars(from) {
        if !sc.is_file() {
            continue;
        }
        let mut dst = crate::sidecar::sidecar_path(to, naming);
        if upper {
            dst = dst.with_extension("XMP");
        }
        if dst.exists() && !case_only {
            continue; // never overwrite: the old sidecar stays where it was
        }
        if let Err(e) = mv(&sc, &dst) {
            for (a, b) in done.iter().rev() {
                let _ = std::fs::rename(b, a);
            }
            return Err(format!("rename {}: {e}", sc.display()));
        }
        done.push((sc, dst));
    }
    Ok(())
}

impl Session {
    /// Plan renaming `ids` with `template` (sequence numbers from `start`), resolving collisions.
    pub fn plan_rename(&self, ids: &[PhotoId], template: &str, start: usize) -> Vec<RenamePlan> {
        // photos sharing one file (virtual copies) rename once
        let mut seen_paths: HashSet<String> = HashSet::new();
        let mut taken: HashSet<String> = HashSet::new(); // lower-case target paths/names claimed in this batch
        let mut out = Vec::new();
        let mut seq = start;
        for id in ids {
            let Some(p) = self.catalog.photo(*id) else { continue };
            if let Source::File { path } = &p.source
                && !seen_paths.insert(path.clone())
            {
                continue;
            }
            let want = expand(template, p, seq);
            seq += 1;
            let (stem, ext) = split_ext(&want);
            let (stem, ext) = (stem.to_string(), ext.to_string());
            let candidate = |k: usize| {
                if k == 0 {
                    want.clone()
                } else if ext.is_empty() {
                    format!("{stem}-{k}")
                } else {
                    format!("{stem}-{k}.{ext}")
                }
            };
            let (to, to_path) = match &p.source {
                Source::File { path } => {
                    let dir = Path::new(path).parent().map(Path::to_path_buf).unwrap_or_default();
                    let mut k = 0;
                    loop {
                        let name = candidate(k);
                        let tp = dir.join(&name).to_string_lossy().to_string();
                        let key = tp.to_lowercase();
                        let same = key == path.to_lowercase();
                        // free: not claimed in this batch and not on disk (unless it is this very file).
                        // A file this batch moves away still counts as taken: simple and safe.
                        let on_disk = Path::new(&tp).exists() && !same;
                        if !taken.contains(&key) && !on_disk {
                            taken.insert(key);
                            break (name, Some(tp));
                        }
                        k += 1;
                    }
                }
                Source::Demo { .. } => {
                    let mut k = 0;
                    loop {
                        let name = candidate(k);
                        if taken.insert(format!("demo:{}", name.to_lowercase())) {
                            break (name, None);
                        }
                        k += 1;
                    }
                }
            };
            let from_path = match &p.source {
                Source::File { path } => Some(path.clone()),
                _ => None,
            };
            out.push(RenamePlan { id: id.0, from: p.file_name.clone(), to, from_path, to_path });
        }
        out
    }

    /// Carry out `plans`: move the files (rolled back on failure), then commit one undoable op
    /// that updates the photos (and their virtual copies).
    pub fn apply_rename(&mut self, plans: &[RenamePlan]) -> Result<usize> {
        let mut moved: Vec<(String, String)> = Vec::new();
        for pl in plans {
            if let (Some(a), Some(b)) = (&pl.from_path, &pl.to_path)
                && a != b
            {
                if let Err(e) = move_file(a, b) {
                    for (a, b) in moved.iter().rev() {
                        let _ = move_file(b, a);
                    }
                    return Err(EngineError::Other(e));
                }
                moved.push((a.clone(), b.clone()));
            }
        }
        let mut ops = Vec::new();
        let by_path: HashMap<&str, &RenamePlan> = plans.iter().filter_map(|p| p.from_path.as_deref().map(|f| (f, p))).collect();
        for p in self.catalog.photos() {
            let plan = match &p.source {
                Source::File { path } => by_path.get(path.as_str()).copied(),
                Source::Demo { .. } => plans.iter().find(|x| x.id == p.id.0 && x.from_path.is_none()),
            };
            let Some(plan) = plan else { continue };
            let source = match &plan.to_path {
                Some(t) => Source::File { path: t.clone() },
                None => p.source.clone(),
            };
            if plan.to != p.file_name || source != p.source {
                ops.push(Op::SetFile { id: p.id, file_name: plan.to.clone(), source });
            }
        }
        let n = ops.len();
        if n > 0
            && let Err(e) = self.commit(&format!("Rename {n} Photo{}", if n == 1 { "" } else { "s" }), Op::Batch { ops })
        {
            for (a, b) in moved.iter().rev() {
                let _ = move_file(b, a);
            }
            return Err(e);
        }
        Ok(n)
    }

    /// File moves an undo/redo op implies: `SetFile` ops whose source path differs from the photo's
    /// current one.
    pub(crate) fn file_moves(&self, op: &Op) -> Vec<(String, String)> {
        fn rec(s: &Session, op: &Op, v: &mut Vec<(String, String)>) {
            match op {
                Op::Batch { ops } => ops.iter().for_each(|o| rec(s, o, v)),
                Op::SetFile { id, source: Source::File { path: to }, .. } => {
                    if let Some(Source::File { path: from }) = s.catalog.photo(*id).map(|p| &p.source)
                        && from != to
                        && !v.iter().any(|(f, _)| f == from)
                    {
                        v.push((from.clone(), to.clone()));
                    }
                }
                _ => {}
            }
        }
        let mut v = Vec::new();
        rec(self, op, &mut v);
        v
    }

    /// Move the files `moves` names; all or nothing.
    pub(crate) fn move_files(moves: &[(String, String)]) -> Result<()> {
        let mut done: Vec<&(String, String)> = Vec::new();
        for m in moves {
            if let Err(e) = move_file(&m.0, &m.1) {
                for (a, b) in done.iter().rev().map(|m| (&m.0, &m.1)) {
                    let _ = move_file(b, a);
                }
                return Err(EngineError::Other(format!("can't undo the rename: {e}")));
            }
            done.push(m);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates() {
        let mut p = Photo::new(PhotoId(1), Source::Demo { scene: 1 }, "IMG_0042.CR2", "CR2", 1, 1, "2026-01-01T00:00:00");
        p.captured = Some("2026-09-30T14:05:09".into());
        p.meta.camera = "Model X/2".into();
        p.meta.title = "Sunset: beach".into();
        assert_eq!(expand("{name}", &p, 1), "IMG_0042.CR2");
        assert_eq!(expand("Trip-{seq:3}", &p, 7), "Trip-007.CR2");
        assert_eq!(expand("{date}_{name}", &p, 1), "20260930_IMG_0042.CR2");
        assert_eq!(expand("{date:%Y-%m-%d %H.%M.%S}", &p, 1), "2026-09-30 14.05.09.CR2");
        assert_eq!(expand("{camera} {title}", &p, 1), "Model X-2 Sunset- beach.CR2");
        assert_eq!(expand("{unknown}-{seq}", &p, 12), "{unknown}-12.CR2");
        assert_eq!(expand("  ", &p, 1), "IMG_0042.CR2", "empty template keeps the name");
        assert_eq!(expand("../{name}", &p, 1), "-IMG_0042.CR2", "no path components");
    }

    #[test]
    fn number_folder_and_metadata_tokens() {
        let path = std::path::Path::new("shoots").join("2026-09 Coast").join("DSC_0815.NEF");
        let mut p = Photo::new(PhotoId(1), Source::File { path: path.to_string_lossy().into() }, "DSC_0815.NEF", "NEF", 1, 1, "2026-01-01T00:00:00");
        p.meta.lens = "24-70mm F2.8".into();
        p.meta.iso = Some(400);
        p.meta.creator = "A. Person".into();
        p.rating = 4;
        assert_eq!(expand("{folder}_{num}", &p, 1), "2026-09 Coast_0815.NEF");
        assert_eq!(expand("{rating}star-{iso}-{lens}-{creator}", &p, 1), "4star-400-24-70mm F2.8-A. Person.NEF");
        // no number at the end of the name / no folder: empty
        p.file_name = "beach.NEF".into();
        p.source = Source::Demo { scene: 0 };
        assert_eq!(expand("{folder}{name}{num}", &p, 1), "beach.NEF");
        // a bare {seq} is padded to the caller's width
        assert_eq!(expand_tokens("{seq}|{seq:2}", &p, 7, 3), "007|07");
    }
}
