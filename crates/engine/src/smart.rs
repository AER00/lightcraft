//! Smart previews: a compact proxy of each photo (its decoded preview-size source, about 1 MB)
//! kept in the library, so photos stay editable — and exportable at proxy size — while their
//! originals are offline (an unplugged drive). The proxy is the scene-linear source scaled into
//! 0..1 by a stored factor, sRGB-encoded and saved as a JPEG after a one-line header.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lightcraft_catalog::Photo;
use lightcraft_color::transfer::linear_to_srgb;
use lightcraft_raster::{Rgb32f, Rgba8};

const MAGIC: &[u8] = b"LCSP1\n";

/// The proxy's file name for a photo (by its content, so copies share one).
pub fn file_name(p: &Photo) -> String {
    let h = lightcraft_preview::Hasher128::new().str(&crate::media::content_key(p)).finish();
    format!("{:032x}.lcsp", h.0)
}

/// Encode a source image as a smart preview.
pub fn encode(img: &Rgb32f) -> Result<Vec<u8>, String> {
    // scale so all but the brightest 0.05 % fit into 0..1
    let mut lum: Vec<f32> = img.data.iter().map(|c| c[0].max(c[1]).max(c[2])).filter(|v| v.is_finite()).collect();
    let scale = if lum.is_empty() {
        1.0
    } else {
        let k = ((lum.len() as f64) * 0.9995) as usize;
        let k = k.min(lum.len() - 1);
        let (_, v, _) = lum.select_nth_unstable_by(k, f32::total_cmp);
        v.max(1.0)
    };
    let data: Vec<[u8; 4]> = img
        .data
        .iter()
        .map(|c| {
            let e = |v: f32| (linear_to_srgb((v / scale).clamp(0.0, 1.0)) * 255.0).round() as u8;
            [e(c[0]), e(c[1]), e(c[2]), 255]
        })
        .collect();
    let rgba = Rgba8 { width: img.width, height: img.height, data };
    let jpg = lightcraft_codecs::encode_jpeg(
        &lightcraft_codecs::EncodeImage::rgba8(&rgba),
        92,
        lightcraft_codecs::ChromaSubsampling::S444,
        &Default::default(),
    )
    .map_err(|e| e.to_string())?;
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(format!("{{\"w\":{},\"h\":{},\"scale\":{scale}}}\n", img.width, img.height).as_bytes());
    out.extend_from_slice(&jpg);
    Ok(out)
}

/// Decode a smart preview back into a source image.
pub fn decode(bytes: &[u8]) -> Result<Rgb32f, String> {
    let rest = bytes.strip_prefix(MAGIC).ok_or("not a smart preview")?;
    let nl = rest.iter().position(|b| *b == b'\n').ok_or("bad smart preview")?;
    let head: serde_json::Value = serde_json::from_slice(&rest[..nl]).map_err(|e| e.to_string())?;
    let scale = head["scale"].as_f64().unwrap_or(1.0) as f32;
    let d = lightcraft_codecs::decode(&rest[nl + 1..], Default::default()).map_err(|e| e.to_string())?;
    // the decoder undoes the sRGB encoding; the values are the source's own primaries
    let mut img = d.image;
    img.data.iter_mut().for_each(|c| *c = c.map(|v| v * scale));
    Ok(img)
}

/// Where a library keeps its smart previews.
pub fn dir(library: &Path) -> PathBuf {
    library.join("Smart Previews")
}

/// Load the proxy at `path`.
pub fn load(path: &Path) -> Result<Arc<Rgb32f>, String> {
    let b = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    decode(&b).map(Arc::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_keeps_the_picture() {
        let img = lightcraft_scenes::demo_library()[0].render(160, 100);
        let back = decode(&encode(&img).unwrap()).unwrap();
        assert_eq!((back.width, back.height), (img.width, img.height));
        let mut err = 0.0f64;
        for (a, b) in img.data.iter().zip(&back.data) {
            for k in 0..3 {
                let (x, y) = (linear_to_srgb(a[k].clamp(0.0, 1.0)), linear_to_srgb(b[k].clamp(0.0, 1.0)));
                err += ((x - y) as f64).abs();
            }
        }
        let mean = err / (img.data.len() * 3) as f64;
        assert!(mean < 0.02, "mean encoded error {mean}");
        assert!(decode(b"nope").is_err());
    }
}
