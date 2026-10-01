//! Export: encode a rendered image to JPEG / PNG / TIFF / WebP / AVIF with an embedded sRGB profile,
//! optional output sharpening and a JPEG file-size limit. Pure (bytes in, bytes out) so the desktop
//! app, CLI, MCP and the web build share it; writing the file is the caller's job.

use lightcraft_codecs::{ChromaSubsampling, EncodeImage, EncodeMeta, NamedSpace, TiffCompression, encode, icc};
use lightcraft_raster::Rgba8;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    #[default]
    Jpeg,
    Png,
    Tiff,
    Webp,
    Avif,
}

impl ExportFormat {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "jpeg" | "jpg" => Self::Jpeg,
            "png" => Self::Png,
            "tiff" | "tif" => Self::Tiff,
            "webp" => Self::Webp,
            "avif" => Self::Avif,
            _ => return None,
        })
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Tiff => "tif",
            Self::Webp => "webp",
            Self::Avif => "avif",
        }
    }
}

/// Output sharpening target (applied after resizing, in display-encoded values).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SharpenFor {
    #[default]
    None,
    Screen,
    Matte,
    Glossy,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SharpenAmount {
    Low,
    #[default]
    Standard,
    High,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExportOptions {
    pub format: ExportFormat,
    /// 1–100 (JPEG, AVIF).
    pub quality: u8,
    /// Resize so the long edge is at most this many pixels (`None` = full size).
    pub long_edge: Option<u32>,
    /// JPEG only: largest quality whose file fits in this many KB.
    pub limit_kb: Option<u32>,
    pub sharpen: SharpenFor,
    pub sharpen_amount: SharpenAmount,
    /// File name template: `{name}` (original stem), `{seq}` (1-based, zero-padded to 3), `{ext}` is appended.
    pub naming: String,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            format: ExportFormat::Jpeg,
            quality: 90,
            long_edge: None,
            limit_kb: None,
            sharpen: SharpenFor::None,
            sharpen_amount: SharpenAmount::Standard,
            naming: "{name}".into(),
        }
    }
}

impl ExportOptions {
    /// Read options from command params (`format`, `quality`, `longEdge`, `limitKb`, `sharpen`, `sharpenAmount`, `naming`).
    pub fn from_json(p: &serde_json::Value) -> Self {
        use serde_json::Value;
        let d = Self::default();
        let s = |k: &str| p.get(k).and_then(Value::as_str);
        let u = |k: &str| p.get(k).and_then(Value::as_u64);
        fn enm<T: serde::de::DeserializeOwned>(p: &Value, k: &str) -> Option<T> {
            p.get(k).cloned().and_then(|v| serde_json::from_value(v).ok())
        }
        Self {
            format: s("format").and_then(ExportFormat::parse).unwrap_or(d.format),
            quality: u("quality").map_or(d.quality, |q| q.clamp(1, 100) as u8),
            long_edge: u("longEdge").filter(|v| *v > 0).map(|v| v.min(65_535) as u32),
            limit_kb: u("limitKb").filter(|v| *v > 0).map(|v| v.min(u32::MAX as u64) as u32),
            sharpen: enm(p, "sharpen").unwrap_or(d.sharpen),
            sharpen_amount: enm(p, "sharpenAmount").unwrap_or(d.sharpen_amount),
            naming: s("naming").map_or(d.naming, str::to_string),
        }
    }

    /// Output file name for photo `stem` at 1-based position `seq` in a batch.
    pub fn file_name(&self, stem: &str, seq: usize) -> String {
        let base = if self.naming.trim().is_empty() { "{name}" } else { self.naming.as_str() };
        let name = base.replace("{name}", stem).replace("{seq}", &format!("{seq:03}"));
        let name: String = name.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '\0') { '_' } else { c }).collect();
        format!("{name}.{}", self.format.extension())
    }
}

/// Unsharp mask on an 8-bit image (separable box-blur approximation of a small Gaussian).
pub fn output_sharpen(img: &mut Rgba8, target: SharpenFor, amount: SharpenAmount) {
    let (radius, base) = match target {
        SharpenFor::None => return,
        SharpenFor::Screen => (1usize, 0.35f32),
        SharpenFor::Matte => (2, 0.6),
        SharpenFor::Glossy => (1, 0.5),
    };
    let k = base
        * match amount {
            SharpenAmount::Low => 0.6,
            SharpenAmount::Standard => 1.0,
            SharpenAmount::High => 1.5,
        };
    let (w, h) = (img.width, img.height);
    if w < 3 || h < 3 {
        return;
    }
    let src: Vec<[f32; 3]> = img.data.iter().map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
    let r = radius as isize;
    let box_pass = |inp: &[[f32; 3]], horizontal: bool| -> Vec<[f32; 3]> {
        let mut out = vec![[0.0; 3]; inp.len()];
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0.0f32; 3];
                for d in -r..=r {
                    let (sx, sy) = if horizontal {
                        ((x as isize + d).clamp(0, w as isize - 1) as usize, y)
                    } else {
                        (x, (y as isize + d).clamp(0, h as isize - 1) as usize)
                    };
                    let p = inp[sy * w + sx];
                    acc = [acc[0] + p[0], acc[1] + p[1], acc[2] + p[2]];
                }
                let n = (2 * r + 1) as f32;
                out[y * w + x] = acc.map(|v| v / n);
            }
        }
        out
    };
    let blur = box_pass(&box_pass(&src, true), false);
    for (i, p) in img.data.iter_mut().enumerate() {
        for c in 0..3 {
            let v = src[i][c] + (src[i][c] - blur[i][c]) * k;
            p[c] = v.round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Encode a rendered (display-referred sRGB) image according to `o`. Resizing to `long_edge` is the
/// caller's job (render at that size); sharpening is applied here.
pub fn encode_image(img: &Rgba8, o: &ExportOptions) -> Result<Vec<u8>, String> {
    let mut img = img.clone();
    output_sharpen(&mut img, o.sharpen, o.sharpen_amount);
    let profile = icc::write_named(NamedSpace::Srgb);
    let meta = EncodeMeta { icc: Some(&profile), ..Default::default() };
    let e = EncodeImage::rgba8(&img);
    let r = match o.format {
        ExportFormat::Jpeg => {
            // 4:4:4 normally; 4:2:0 when targeting a file size (much smaller at equal visual quality).
            let sub = if o.limit_kb.is_some() { ChromaSubsampling::S420 } else { ChromaSubsampling::S444 };
            let jpeg = |q: u8| encode::encode_jpeg(&e, q, sub, &meta);
            match o.limit_kb {
                Some(kb) => {
                    let limit = kb as usize * 1024;
                    // Binary search for the highest quality that fits.
                    let (mut lo, mut hi) = (1u8, o.quality);
                    let mut best = None;
                    while lo <= hi {
                        let mid = lo + (hi - lo) / 2;
                        let b = jpeg(mid).map_err(|e| e.to_string())?;
                        if b.len() <= limit {
                            best = Some(b);
                            lo = mid + 1;
                        } else if mid == 1 {
                            break;
                        } else {
                            hi = mid - 1;
                        }
                    }
                    return best.ok_or_else(|| format!("cannot fit within {kb} KB"));
                }
                None => jpeg(o.quality),
            }
        }
        ExportFormat::Png => encode::encode_png(&e, &meta),
        ExportFormat::Tiff => encode::encode_tiff(&e, TiffCompression::Deflate, &meta),
        ExportFormat::Webp => encode::encode_webp_lossless(&e, &meta),
        ExportFormat::Avif => encode::encode_avif(&e, o.quality, 6, &meta),
    };
    r.map_err(|e| e.to_string())
}

/// One exported file.
pub struct Exported {
    pub file_name: String,
    pub bytes: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

/// Render photo `id` at the requested size (full size when `long_edge` is `None`) and encode it.
pub fn export_photo(session: &mut crate::Session, id: lightcraft_catalog::PhotoId, o: &ExportOptions, seq: usize) -> Result<Exported, String> {
    let p = session.catalog.photo(id).ok_or("no such photo")?;
    let stem = p.file_name.rsplit_once('.').map_or(p.file_name.as_str(), |(a, _)| a).to_string();
    let full = p.width.max(p.height).max(1) as usize;
    let size = o.long_edge.map_or(full, |l| l as usize);
    let r = session.render_now(id, size, size)?;
    let bytes = encode_image(&r.image, o)?;
    Ok(Exported { file_name: o.file_name(&stem, seq), bytes, width: r.image.width, height: r.image.height })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image() -> Rgba8 {
        let mut img = Rgba8::new(96, 64);
        for (i, p) in img.data.iter_mut().enumerate() {
            let (x, y) = (i % 96, i / 96);
            *p = [(x * 2) as u8, (y * 3) as u8, ((x * y) % 255) as u8, 255];
        }
        img
    }

    #[test]
    fn formats_have_magic() {
        let img = test_image();
        for (f, magic) in
            [(ExportFormat::Jpeg, &b"\xFF\xD8"[..]), (ExportFormat::Png, b"\x89PNG"), (ExportFormat::Tiff, b"II"), (ExportFormat::Webp, b"RIFF")]
        {
            let b = encode_image(&img, &ExportOptions { format: f, ..Default::default() }).unwrap();
            assert!(b.starts_with(magic), "{f:?}");
        }
    }

    #[test]
    fn size_limit_respected() {
        let img = test_image();
        let full = encode_image(&img, &ExportOptions { quality: 100, ..Default::default() }).unwrap();
        let kb = (full.len() / 1024 / 2).max(2) as u32;
        let b = encode_image(&img, &ExportOptions { quality: 100, limit_kb: Some(kb), ..Default::default() }).unwrap();
        assert!(b.len() <= kb as usize * 1024);
    }

    #[test]
    fn sharpen_increases_edge_contrast() {
        let mut img = Rgba8::new(8, 8);
        for (i, p) in img.data.iter_mut().enumerate() {
            let v = if i % 8 < 4 { 80 } else { 170 };
            *p = [v, v, v, 255];
        }
        output_sharpen(&mut img, SharpenFor::Matte, SharpenAmount::High);
        assert!(img.data[3][0] < 80 && img.data[4][0] > 170);
    }

    #[test]
    fn naming_and_params() {
        let o = ExportOptions::from_json(&serde_json::json!({"format": "jpg", "quality": 150, "longEdge": 2048, "naming": "{name}-{seq}"}));
        assert_eq!(o.format, ExportFormat::Jpeg);
        assert_eq!(o.quality, 100);
        assert_eq!(o.long_edge, Some(2048));
        assert_eq!(o.file_name("IMG/1", 7), "IMG_1-007.jpg");
    }
}
