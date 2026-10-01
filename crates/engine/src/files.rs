//! Real files: probing (dimensions + metadata for import) and loading (decode → linear Rec.2020,
//! oriented, downscaled to the requested level). Standard formats go through `lightcraft-codecs`,
//! camera raws through `lightcraft-raw` (demosaic + DNG colour model).

use std::sync::Arc;

use lightcraft_catalog::{MediaKind, Meta};
use lightcraft_color::cct::xy_to_temp_tint;
use lightcraft_geom::Orientation;
use lightcraft_pipeline::SourceInfo;
use lightcraft_raster::Rgb32f;
use lightcraft_raster::resample::{Filter, fit};

use crate::media::{FileLoader, FileProbe, ProbeInfo};

fn meta_of(m: &lightcraft_meta::Metadata) -> (Meta, Option<String>) {
    let shutter = m.exposure_time.map(|t| if t >= 1.0 { format!("{t:.0}") } else { format!("1/{:.0}", 1.0 / t) }).unwrap_or_default();
    let camera = [m.make.clone().unwrap_or_default(), m.model.clone().unwrap_or_default()].join(" ").trim().to_string();
    let meta = Meta {
        camera,
        lens: m.lens_model.clone().unwrap_or_default(),
        focal_mm: m.focal_length.map(|f| f as f32),
        aperture: m.f_number.map(|f| f as f32),
        shutter,
        iso: m.iso,
        location: String::new(),
        gps: m.gps.as_ref().map(|g| (g.latitude, g.longitude)),
        title: m.title.clone().unwrap_or_default(),
        caption: m.caption.clone().unwrap_or_default(),
        copyright: m.copyright.clone().unwrap_or_default(),
        creator: m.artist.clone().unwrap_or_default(),
        keywords: m.keywords.clone(),
    };
    (meta, m.capture_time.as_ref().map(|d| d.to_iso()))
}

fn ext_upper(name: &str) -> String {
    std::path::Path::new(name).extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default()
}

/// Probe a file's bytes: kind, dimensions (oriented), metadata.
pub fn probe_bytes(name: &str, bytes: &[u8]) -> Result<ProbeInfo, String> {
    let content_hash = Some(lightcraft_preview::hash_bytes(bytes).to_string());
    let m = lightcraft_meta::extract(bytes);
    let (meta, captured) = meta_of(&m);
    if lightcraft_raw::probe(bytes).is_some() {
        let raw = lightcraft_raw::decode(bytes).map_err(|e| e.to_string())?;
        let (mut w, mut h) = (raw.crop.width.max(1) as u32, raw.crop.height.max(1) as u32);
        if w <= 1 || h <= 1 {
            (w, h) = (raw.active_area.width as u32, raw.active_area.height as u32);
        }
        if raw.orientation.swaps_axes() {
            std::mem::swap(&mut w, &mut h);
        }
        let (t, tint) = xy_to_temp_tint(lightcraft_raw::color::as_shot_white_xy(&raw));
        let as_shot_wb = Some((t.round(), tint.round()));
        return Ok(ProbeInfo {
            width: w,
            height: h,
            format: ext_upper(name),
            kind: MediaKind::Raw,
            file_size: bytes.len() as u64,
            captured,
            meta,
            as_shot_wb,
            content_hash,
            xmp: lightcraft_meta::embedded(bytes).xmp,
        });
    }
    let fmt = lightcraft_codecs::sniff(bytes).ok_or("unrecognized file format")?;
    if !fmt.can_decode() {
        return Err(format!("{fmt:?} files are not supported yet"));
    }
    let d = lightcraft_codecs::decode(bytes, lightcraft_codecs::DecodeOptions::fit(64, 64)).map_err(|e| e.to_string())?;
    let o = Orientation::from_exif(d.orientation);
    let (mut w, mut h) = (d.source_width, d.source_height);
    if o.swaps_axes() {
        std::mem::swap(&mut w, &mut h);
    }
    let format = match ext_upper(name).as_str() {
        "JPG" | "JPEG" => "JPEG".to_string(),
        "" => format!("{fmt:?}").to_uppercase(),
        e => e.to_string(),
    };
    Ok(ProbeInfo {
        width: w,
        height: h,
        format,
        kind: MediaKind::Image,
        file_size: bytes.len() as u64,
        captured,
        meta,
        as_shot_wb: None,
        content_hash,
        xmp: None,
    })
}

/// Decode a file into a linear Rec.2020 image no larger than `max_edge`, oriented.
pub fn load_bytes(bytes: &[u8], max_edge: usize) -> Result<(Rgb32f, SourceInfo), String> {
    if lightcraft_raw::probe(bytes).is_some() {
        let raw = lightcraft_raw::decode(bytes).map_err(|e| e.to_string())?;
        let method = if max_edge <= 600 { lightcraft_raw::Method::Bilinear } else { lightcraft_raw::Method::Ahd };
        let mut img = raw.develop(method).map_err(|e| e.to_string())?;
        let xy = lightcraft_raw::color::as_shot_white_xy(&raw);
        let t = lightcraft_raw::color::camera_transform(&raw, xy);
        lightcraft_raw::highlight::reconstruct(&mut img, t.wb, 0.99);
        let m = t.matrix.to_f32();
        let gain = 2f32.powf(t.baseline_exposure as f32);
        let wb = t.wb;
        let img = img.map(|p| {
            let c = [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
            [
                ((m[0][0] * c[0] + m[0][1] * c[1] + m[0][2] * c[2]) * gain).max(0.0),
                ((m[1][0] * c[0] + m[1][1] * c[1] + m[1][2] * c[2]) * gain).max(0.0),
                ((m[2][0] * c[0] + m[2][1] * c[1] + m[2][2] * c[2]) * gain).max(0.0),
            ]
        });
        let img = fit(&img, max_edge, max_edge, Filter::Box).oriented(raw.orientation);
        let (temp, tint) = xy_to_temp_tint(xy);
        return Ok((img, SourceInfo { raw: true, as_shot_temp: temp.round(), as_shot_tint: tint.round() }));
    }
    let d = lightcraft_codecs::decode(bytes, lightcraft_codecs::DecodeOptions::fit(max_edge as u32, max_edge as u32)).map_err(|e| e.to_string())?;
    let img = d.to_working();
    let img = if img.width.max(img.height) > max_edge { fit(&img, max_edge, max_edge, Filter::Mitchell) } else { img };
    Ok((img.oriented(Orientation::from_exif(d.orientation)), SourceInfo::default()))
}

/// Filesystem-backed hooks (native). On the web the host installs bytes-based hooks instead.
pub fn fs_hooks() -> (FileLoader, FileProbe) {
    let loader: FileLoader = Arc::new(|path: &str, max_edge: usize| {
        let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        load_bytes(&bytes, max_edge)
    });
    let probe: FileProbe = Arc::new(|path: &str| {
        let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        probe_bytes(path, &bytes)
    });
    (loader, probe)
}

impl crate::Session {
    /// Install the filesystem file hooks (desktop, CLI, MCP headless).
    pub fn with_fs(mut self) -> Self {
        let (l, p) = fs_hooks();
        self.media.file_loader = Some(l);
        self.media.file_probe = Some(p);
        self
    }
}
