//! Sony ARW.
//!
//! Sources: H. Dietz, "Sony ARW2 Compression: Artifacts And Credible Repair" (IS&T Electronic Imaging 2016) for
//! the ARW2 ("cRAW") scheme, and the ExifTool Sony tag-name documentation for the meaning of the raw-IFD tags
//! (`0x7010` tone-curve thresholds, `0x7310` black levels, `0x7313` WB levels, `0x74c7/0x74c8` crop).
//!
//! ARW2, as described in the paper:
//! 1. Sensor values are tone-mapped to 11-bit codes by a five-segment piecewise-linear curve whose step doubles at
//!    each threshold; the thresholds are recorded in the file (`0x7010`). We invert it: code `c` indexes the curve at
//!    `2c` in a 12-bit domain whose breakpoints are the recorded thresholds / 4, with slopes 1, 2, 4, 8, 16; the
//!    result is in 14-bit sensor units (black 512, white 16383 per the file's own tags, which we verified against
//!    the decoded data).
//! 2. Each row is coded in 32-pixel groups split into two interleaved 16-pixel sets (even columns, then odd), each
//!    a 128-bit little-endian block: 11-bit max, 11-bit min, 4-bit index of the max, 4-bit index of the min and 14
//!    seven-bit deltas above the min, scaled by the smallest shift that fits `max − min` into 7 bits.
//!
//! Also: uncompressed 16-bit ARW, and lossless-JPEG tiled ARW through the generic TIFF path.

use crate::tiffraw::{Packing, read_image};
use crate::{BlackLevel, Cfa, ColorData, OpcodeLists, RawData, RawError, RawFormat, RawImage, Rect, Result};
use lightcraft_geom::Orientation;
use lightcraft_tiff::image::chunk_bytes;
use lightcraft_tiff::tags::{self as t, photometric};
use lightcraft_tiff::{Ifd, Tiff};
use rayon::prelude::*;

const TONE_CURVE: u16 = 0x7010;
const BLACK_LEVEL: u16 = 0x7310;
const WB_RGGB: u16 = 0x7313;
const CROP_TOP_LEFT: u16 = 0x74c7;
const CROP_SIZE: u16 = 0x74c8;

/// The inverse tone curve: 11-bit code → 14-bit sensor value.
pub(crate) fn code_curve(thresholds: &[u64]) -> Vec<u16> {
    let mut bp = [0usize, 4095, 4095, 4095, 4095, 4095];
    for (i, &v) in thresholds.iter().take(4).enumerate() {
        bp[i + 1] = ((v >> 2) as usize).min(4095);
    }
    // keep breakpoints monotonic
    for i in 1..6 {
        bp[i] = bp[i].max(bp[i - 1]);
    }
    let mut lut = vec![0u32; 4096];
    let mut seg = 0;
    for i in 1..4096 {
        while seg < 4 && i > bp[seg + 1] {
            seg += 1;
        }
        lut[i] = lut[i - 1] + (1 << seg);
    }
    (0..2048).map(|c| lut[(2 * c).min(4095)].min(16383) as u16).collect()
}

/// Decode one row of ARW2 data (`row.len() == width` bytes) into codes.
pub(crate) fn decode_row(row: &[u8], out: &mut [u16]) {
    let w = out.len();
    let mut x0 = 0;
    while x0 + 32 <= w && x0 + 32 <= row.len() {
        for half in 0..2 {
            let off = x0 + half * 16;
            let block = u128::from_le_bytes(row[off..off + 16].try_into().expect("16 bytes"));
            let max = (block & 0x7ff) as u16;
            let min = ((block >> 11) & 0x7ff) as u16;
            let imax = ((block >> 22) & 0xf) as usize;
            let imin = ((block >> 26) & 0xf) as usize;
            let range = max.saturating_sub(min);
            let mut sh = 0;
            while sh < 4 && (range >> sh) > 127 {
                sh += 1;
            }
            let mut bit = 30;
            for i in 0..16 {
                let v = if i == imax {
                    max
                } else if i == imin {
                    min
                } else {
                    // a corrupt block with imax == imin would read a 15th delta past bit 127
                    let d = if bit + 7 <= 128 { ((block >> bit) & 0x7f) as u16 } else { 0 };
                    bit += 7;
                    (min + (d << sh)).min(0x7ff)
                };
                out[x0 + half + 2 * i] = v;
            }
        }
        x0 += 32;
    }
}

fn raw_ifd(tiff: &Tiff) -> Option<&Ifd> {
    tiff.all_ifds()
        .into_iter()
        .filter(|i| i.u16(t::PHOTOMETRIC) == Some(photometric::CFA) || i.contains(TONE_CURVE))
        .max_by_key(|i| i.u64(t::IMAGE_WIDTH).unwrap_or(0).saturating_mul(i.u64(t::IMAGE_LENGTH).unwrap_or(0)))
}

pub(crate) fn decode(bytes: &[u8]) -> Result<RawImage> {
    let tiff = Tiff::parse(bytes)?;
    let ifd0 = &tiff.ifds[0];
    let raw = raw_ifd(&tiff).ok_or_else(|| RawError::Unsupported("ARW without a CFA image IFD (old ARW or SR2)".into()))?;
    let info = raw.image()?;
    let (w, h) = (info.width as usize, info.height as usize);
    if w * h > crate::MAX_SAMPLES {
        return Err(RawError::Limit("image too large"));
    }
    let bits = info.bits() as u32;
    let chunks = info.chunks(bytes.len() as u64);
    let strip_len: u64 = chunks.iter().map(|c| c.len).sum();
    let (data, out_bits) = match info.compression {
        32767 if chunks.len() == 1 && strip_len >= (w * h) as u64 && strip_len < (w * h) as u64 * 5 / 4 => {
            let src = chunk_bytes(bytes, &chunks[0]).ok_or_else(|| RawError::Corrupt("raw strip outside file".into()))?;
            let curve = code_curve(&raw.u64s(TONE_CURVE).unwrap_or_else(|| vec![8000, 10400, 12900, 14100]));
            let mut data = vec![0u16; w * h];
            data.par_chunks_mut(w).enumerate().for_each(|(y, out)| {
                let row = src.get(y * w..((y + 1) * w).min(src.len())).unwrap_or(&[]);
                if row.len() == w {
                    decode_row(row, out);
                    out.iter_mut().for_each(|v| *v = curve[*v as usize]);
                }
            });
            (RawData::U16(data), 14)
        }
        32767 => return Err(RawError::Unsupported("Sony ARW version 1 / packed compressed variant".into())),
        1 => {
            let packing = if strip_len >= (w * h * 2) as u64 { Packing::Word16 } else { Packing::Msb };
            (read_image(bytes, &info, tiff.order, packing)?, bits)
        }
        _ => (read_image(bytes, &info, tiff.order, Packing::Msb)?, bits),
    };
    let RawData::U16(ref samples) = data else { return Err(RawError::Unsupported("float ARW".into())) };

    let cfa = match (raw.u64s(t::CFA_REPEAT_PATTERN_DIM).as_deref(), raw.bytes(t::CFA_PATTERN_EP)) {
        (Some([2, 2]), Some(p)) if p.len() == 4 && p.iter().all(|&c| c <= 2) => Cfa { width: 2, height: 2, pattern: p.to_vec() },
        _ => Cfa::bayer("RGGB").expect("static"),
    };
    let default_black = if out_bits >= 14 { 512.0 } else { 128.0 };
    let black = match raw.f64s(BLACK_LEVEL).as_deref() {
        Some([a, b, c, d]) => {
            BlackLevel { repeat_rows: 2, repeat_cols: 2, values: vec![*a as f32, *b as f32, *c as f32, *d as f32], ..Default::default() }
        }
        _ => BlackLevel::uniform(default_black),
    };
    let white = raw.f64(t::WHITE_LEVEL).map(|v| v as f32).filter(|v| *v > 0.0).unwrap_or_else(|| super::white_from_data(samples, out_bits));
    let wb = raw.f64s(WB_RGGB).filter(|v| v.len() == 4 && v[1] > 0.0 && v[0] > 0.0 && v[3] > 0.0).map(|v| {
        let g = (v[1] + v[2]) / 2.0;
        [(v[0] / g) as f32, 1.0, (v[3] / g) as f32]
    });
    let crop = match (raw.u64s(CROP_TOP_LEFT).as_deref(), raw.u64s(CROP_SIZE).as_deref()) {
        (Some([x, y]), Some([cw, ch])) if *cw > 0 && *ch > 0 => Rect::new(*x as usize, *y as usize, *cw as usize, *ch as usize).clipped(w, h),
        _ => Rect::new(0, 0, w, h),
    };
    let mut metadata = lightcraft_meta::from_tiff(&tiff);
    metadata.width = Some(crop.width as u32);
    metadata.height = Some(crop.height as u32);
    let img = RawImage {
        format: RawFormat::Arw,
        width: w,
        height: h,
        cpp: 1,
        data,
        cfa: Some(cfa),
        bits: out_bits,
        black,
        white: vec![white],
        active_area: Rect::new(0, 0, w, h),
        crop,
        orientation: Orientation::from_exif(ifd0.u16(t::ORIENTATION).unwrap_or(1)),
        color: ColorData::default(),
        wb_multipliers: wb,
        linearized: false,
        opcodes: OpcodeLists::default(),
        metadata,
    };
    img.validate()?;
    Ok(img)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode one 16-value set as an ARW2 block (the paper's scheme, used here to test the decoder).
    pub(crate) fn encode_block(v: &[u16; 16]) -> [u8; 16] {
        let (imax, &max) = v.iter().enumerate().max_by_key(|(i, x)| (**x, usize::MAX - *i)).unwrap();
        let (imin, &min) = v.iter().enumerate().filter(|(i, _)| *i != imax).min_by_key(|(_, x)| **x).unwrap();
        let range = max - min;
        let mut sh = 0;
        while sh < 4 && (range >> sh) > 127 {
            sh += 1;
        }
        let mut b: u128 = max as u128 | (min as u128) << 11 | (imax as u128) << 22 | (imin as u128) << 26;
        let mut bit = 30;
        for (i, &x) in v.iter().enumerate() {
            if i == imax || i == imin {
                continue;
            }
            b |= (((x - min) >> sh) as u128 & 0x7f) << bit;
            bit += 7;
        }
        b.to_le_bytes()
    }

    #[test]
    fn block_roundtrip_exact_when_range_small() {
        let mut row = vec![0u8; 64];
        let vals: Vec<u16> = (0..64).map(|i| 300 + (i * 37 % 100) as u16).collect();
        for g in 0..2 {
            for half in 0..2 {
                let set: [u16; 16] = std::array::from_fn(|i| vals[g * 32 + half + 2 * i]);
                row[g * 32 + half * 16..g * 32 + half * 16 + 16].copy_from_slice(&encode_block(&set));
            }
        }
        let mut out = vec![0u16; 64];
        decode_row(&row, &mut out);
        assert_eq!(out, vals);
    }

    #[test]
    fn block_quantises_large_ranges() {
        let set: [u16; 16] = std::array::from_fn(|i| (i as u16) * 130);
        let mut row = vec![0u8; 32];
        row[..16].copy_from_slice(&encode_block(&set));
        let mut out = vec![0u16; 32];
        decode_row(&row, &mut out);
        for i in 0..16 {
            let got = out[2 * i];
            assert!(got <= set[i] && set[i] - got < 16, "{i}: {got} vs {}", set[i]);
        }
        assert_eq!(out[0], 0);
        assert_eq!(out[30], 1950);
    }

    #[test]
    fn curve_is_monotonic_and_matches_tags() {
        let c = code_curve(&[8000, 10400, 12900, 14100]);
        assert!(c.windows(2).all(|w| w[1] >= w[0]));
        assert_eq!(c[256], 512); // black
        assert_eq!(c[1000], 2000);
        assert_eq!(*c.last().unwrap(), 16383);
        // degenerate thresholds do not panic
        let _ = code_curve(&[]);
        let _ = code_curve(&[60000, 1, 0, 0]);
    }
}
