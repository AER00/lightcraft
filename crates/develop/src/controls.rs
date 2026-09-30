//! Control specs: one table describes every numeric develop slider — id, label, section, range,
//! default, step, display precision and track style. The UI builds panels from it, MCP exposes it as
//! a schema, and `get`/`set` address settings by id (e.g. `light.exposure`, `mixer.red.hue`).

use serde::Serialize;

use crate::settings::DevelopSettings;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Section {
    Light,
    Curve,
    Color,
    Mixer,
    BwMix,
    Grading,
    Effects,
    Vignette,
    Grain,
    Detail,
    Optics,
    Geometry,
    Profile,
}

impl Section {
    pub fn label(self) -> &'static str {
        match self {
            Section::Light => "Light",
            Section::Curve => "Tone Curve",
            Section::Color => "Color",
            Section::Mixer => "Color Mixer",
            Section::BwMix => "B&W Mixer",
            Section::Grading => "Color Grading",
            Section::Effects => "Effects",
            Section::Vignette => "Vignette",
            Section::Grain => "Grain",
            Section::Detail => "Detail",
            Section::Optics => "Optics",
            Section::Geometry => "Geometry",
            Section::Profile => "Profile",
        }
    }
}

/// How the slider track is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Track {
    /// Plain grey track, fill from the minimum.
    Plain,
    /// Grey track, zero in the middle.
    Centered,
    /// Blue → yellow (white balance temperature).
    Temp,
    /// Green → magenta (tint).
    Tint,
    /// A two-colour gradient (sRGB hex strings).
    Gradient { from: &'static str, to: &'static str },
    /// Hue shift for a mixer band: gradient from the neighbouring hues through the band colour.
    Hue { band: u8 },
    /// Saturation for a band: grey → band colour.
    Sat { band: u8 },
    /// Luminance for a band: dark → light band colour.
    Lum { band: u8 },
    /// Full hue rainbow.
    Rainbow,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ControlSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub section: Section,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub step: f64,
    /// Decimal places shown.
    pub decimals: u8,
    pub track: Track,
}

impl ControlSpec {
    pub fn clamp(&self, v: f64) -> f64 {
        if v.is_finite() { v.clamp(self.min, self.max) } else { self.default }
    }
    pub fn format(&self, v: f64) -> String {
        let s = format!("{:.*}", self.decimals as usize, v);
        if v > 0.0 && self.min < 0.0 { format!("+{s}") } else { s }
    }
}

macro_rules! controls {
    ($( $id:literal => $($field:ident).+ , $label:literal, $section:ident, $min:expr, $max:expr, $def:expr, $step:expr, $dec:expr, $track:expr; )*) => {
        /// All numeric develop controls, in panel order.
        pub static CONTROLS: &[ControlSpec] = &[
            $( ControlSpec { id: $id, label: $label, section: Section::$section, min: $min as f64, max: $max as f64, default: $def as f64, step: $step as f64, decimals: $dec, track: $track }, )*
        ];

        /// Read a control value by id.
        pub fn get(s: &DevelopSettings, id: &str) -> Option<f64> {
            match id {
                $( $id => Some(s.$($field).+ as f64), )*
                _ => None,
            }
        }

        /// Write a control value by id (clamped to the spec range). Returns false for unknown ids.
        pub fn set(s: &mut DevelopSettings, id: &str, v: f64) -> bool {
            let Some(spec) = find(id) else { return false };
            let v = spec.clamp(v);
            match id {
                $( $id => { s.$($field).+ = v as _; true } )*
                _ => false,
            }
        }
    };
}

use Track::*;

controls! {
    "profile.amount" => profile.amount, "Amount", Profile, 0, 200, 100, 1, 0, Plain;
    "wb.temp" => wb.temp, "Temp", Color, 2000, 50000, 6500, 50, 0, Temp;
    "wb.tint" => wb.tint, "Tint", Color, -150, 150, 0, 1, 0, Tint;
    "light.exposure" => light.exposure, "Exposure", Light, -5, 5, 0, 0.01, 2, Centered;
    "light.contrast" => light.contrast, "Contrast", Light, -100, 100, 0, 1, 0, Centered;
    "light.highlights" => light.highlights, "Highlights", Light, -100, 100, 0, 1, 0, Centered;
    "light.shadows" => light.shadows, "Shadows", Light, -100, 100, 0, 1, 0, Centered;
    "light.whites" => light.whites, "Whites", Light, -100, 100, 0, 1, 0, Centered;
    "light.blacks" => light.blacks, "Blacks", Light, -100, 100, 0, 1, 0, Centered;
    "curve.highlights" => curve.highlights, "Highlights", Curve, -100, 100, 0, 1, 0, Centered;
    "curve.lights" => curve.lights, "Lights", Curve, -100, 100, 0, 1, 0, Centered;
    "curve.darks" => curve.darks, "Darks", Curve, -100, 100, 0, 1, 0, Centered;
    "curve.shadows" => curve.shadows, "Shadows", Curve, -100, 100, 0, 1, 0, Centered;
    "curve.splitShadows" => curve.split_shadows, "Shadows split", Curve, 10, 70, 25, 1, 0, Plain;
    "curve.splitMid" => curve.split_mid, "Midtones split", Curve, 20, 80, 50, 1, 0, Plain;
    "curve.splitHighlights" => curve.split_highlights, "Highlights split", Curve, 30, 90, 75, 1, 0, Plain;
    "color.vibrance" => color.vibrance, "Vibrance", Color, -100, 100, 0, 1, 0, Gradient { from: "#7a7a7a", to: "#d8406a" };
    "color.saturation" => color.saturation, "Saturation", Color, -100, 100, 0, 1, 0, Gradient { from: "#7a7a7a", to: "#e04a3a" };
    "mixer.red.hue" => mixer.red.hue, "Red Hue", Mixer, -100, 100, 0, 1, 0, Hue { band: 0 };
    "mixer.red.sat" => mixer.red.sat, "Red Saturation", Mixer, -100, 100, 0, 1, 0, Sat { band: 0 };
    "mixer.red.lum" => mixer.red.lum, "Red Luminance", Mixer, -100, 100, 0, 1, 0, Lum { band: 0 };
    "mixer.orange.hue" => mixer.orange.hue, "Orange Hue", Mixer, -100, 100, 0, 1, 0, Hue { band: 1 };
    "mixer.orange.sat" => mixer.orange.sat, "Orange Saturation", Mixer, -100, 100, 0, 1, 0, Sat { band: 1 };
    "mixer.orange.lum" => mixer.orange.lum, "Orange Luminance", Mixer, -100, 100, 0, 1, 0, Lum { band: 1 };
    "mixer.yellow.hue" => mixer.yellow.hue, "Yellow Hue", Mixer, -100, 100, 0, 1, 0, Hue { band: 2 };
    "mixer.yellow.sat" => mixer.yellow.sat, "Yellow Saturation", Mixer, -100, 100, 0, 1, 0, Sat { band: 2 };
    "mixer.yellow.lum" => mixer.yellow.lum, "Yellow Luminance", Mixer, -100, 100, 0, 1, 0, Lum { band: 2 };
    "mixer.green.hue" => mixer.green.hue, "Green Hue", Mixer, -100, 100, 0, 1, 0, Hue { band: 3 };
    "mixer.green.sat" => mixer.green.sat, "Green Saturation", Mixer, -100, 100, 0, 1, 0, Sat { band: 3 };
    "mixer.green.lum" => mixer.green.lum, "Green Luminance", Mixer, -100, 100, 0, 1, 0, Lum { band: 3 };
    "mixer.aqua.hue" => mixer.aqua.hue, "Aqua Hue", Mixer, -100, 100, 0, 1, 0, Hue { band: 4 };
    "mixer.aqua.sat" => mixer.aqua.sat, "Aqua Saturation", Mixer, -100, 100, 0, 1, 0, Sat { band: 4 };
    "mixer.aqua.lum" => mixer.aqua.lum, "Aqua Luminance", Mixer, -100, 100, 0, 1, 0, Lum { band: 4 };
    "mixer.blue.hue" => mixer.blue.hue, "Blue Hue", Mixer, -100, 100, 0, 1, 0, Hue { band: 5 };
    "mixer.blue.sat" => mixer.blue.sat, "Blue Saturation", Mixer, -100, 100, 0, 1, 0, Sat { band: 5 };
    "mixer.blue.lum" => mixer.blue.lum, "Blue Luminance", Mixer, -100, 100, 0, 1, 0, Lum { band: 5 };
    "mixer.purple.hue" => mixer.purple.hue, "Purple Hue", Mixer, -100, 100, 0, 1, 0, Hue { band: 6 };
    "mixer.purple.sat" => mixer.purple.sat, "Purple Saturation", Mixer, -100, 100, 0, 1, 0, Sat { band: 6 };
    "mixer.purple.lum" => mixer.purple.lum, "Purple Luminance", Mixer, -100, 100, 0, 1, 0, Lum { band: 6 };
    "mixer.magenta.hue" => mixer.magenta.hue, "Magenta Hue", Mixer, -100, 100, 0, 1, 0, Hue { band: 7 };
    "mixer.magenta.sat" => mixer.magenta.sat, "Magenta Saturation", Mixer, -100, 100, 0, 1, 0, Sat { band: 7 };
    "mixer.magenta.lum" => mixer.magenta.lum, "Magenta Luminance", Mixer, -100, 100, 0, 1, 0, Lum { band: 7 };
    "bw.red" => bw_mix.red, "Red", BwMix, -100, 100, 0, 1, 0, Lum { band: 0 };
    "bw.orange" => bw_mix.orange, "Orange", BwMix, -100, 100, 0, 1, 0, Lum { band: 1 };
    "bw.yellow" => bw_mix.yellow, "Yellow", BwMix, -100, 100, 0, 1, 0, Lum { band: 2 };
    "bw.green" => bw_mix.green, "Green", BwMix, -100, 100, 0, 1, 0, Lum { band: 3 };
    "bw.aqua" => bw_mix.aqua, "Aqua", BwMix, -100, 100, 0, 1, 0, Lum { band: 4 };
    "bw.blue" => bw_mix.blue, "Blue", BwMix, -100, 100, 0, 1, 0, Lum { band: 5 };
    "bw.purple" => bw_mix.purple, "Purple", BwMix, -100, 100, 0, 1, 0, Lum { band: 6 };
    "bw.magenta" => bw_mix.magenta, "Magenta", BwMix, -100, 100, 0, 1, 0, Lum { band: 7 };
    "grading.shadows.hue" => grading.shadows.hue, "Shadows Hue", Grading, 0, 360, 0, 1, 0, Rainbow;
    "grading.shadows.sat" => grading.shadows.sat, "Shadows Saturation", Grading, 0, 100, 0, 1, 0, Plain;
    "grading.shadows.lum" => grading.shadows.lum, "Shadows Luminance", Grading, -100, 100, 0, 1, 0, Centered;
    "grading.midtones.hue" => grading.midtones.hue, "Midtones Hue", Grading, 0, 360, 0, 1, 0, Rainbow;
    "grading.midtones.sat" => grading.midtones.sat, "Midtones Saturation", Grading, 0, 100, 0, 1, 0, Plain;
    "grading.midtones.lum" => grading.midtones.lum, "Midtones Luminance", Grading, -100, 100, 0, 1, 0, Centered;
    "grading.highlights.hue" => grading.highlights.hue, "Highlights Hue", Grading, 0, 360, 0, 1, 0, Rainbow;
    "grading.highlights.sat" => grading.highlights.sat, "Highlights Saturation", Grading, 0, 100, 0, 1, 0, Plain;
    "grading.highlights.lum" => grading.highlights.lum, "Highlights Luminance", Grading, -100, 100, 0, 1, 0, Centered;
    "grading.global.hue" => grading.global.hue, "Global Hue", Grading, 0, 360, 0, 1, 0, Rainbow;
    "grading.global.sat" => grading.global.sat, "Global Saturation", Grading, 0, 100, 0, 1, 0, Plain;
    "grading.global.lum" => grading.global.lum, "Global Luminance", Grading, -100, 100, 0, 1, 0, Centered;
    "grading.blending" => grading.blending, "Blending", Grading, 0, 100, 50, 1, 0, Plain;
    "grading.balance" => grading.balance, "Balance", Grading, -100, 100, 0, 1, 0, Centered;
    "effects.texture" => effects.texture, "Texture", Effects, -100, 100, 0, 1, 0, Centered;
    "effects.clarity" => effects.clarity, "Clarity", Effects, -100, 100, 0, 1, 0, Centered;
    "effects.dehaze" => effects.dehaze, "Dehaze", Effects, -100, 100, 0, 1, 0, Centered;
    "vignette.amount" => vignette.amount, "Vignette", Vignette, -100, 100, 0, 1, 0, Gradient { from: "#101010", to: "#f0f0f0" };
    "vignette.midpoint" => vignette.midpoint, "Midpoint", Vignette, 0, 100, 50, 1, 0, Plain;
    "vignette.roundness" => vignette.roundness, "Roundness", Vignette, -100, 100, 0, 1, 0, Centered;
    "vignette.feather" => vignette.feather, "Feather", Vignette, 0, 100, 50, 1, 0, Plain;
    "vignette.highlights" => vignette.highlights, "Highlights", Vignette, 0, 100, 0, 1, 0, Plain;
    "grain.amount" => grain.amount, "Grain", Grain, 0, 100, 0, 1, 0, Plain;
    "grain.size" => grain.size, "Size", Grain, 0, 100, 25, 1, 0, Plain;
    "grain.roughness" => grain.roughness, "Roughness", Grain, 0, 100, 50, 1, 0, Plain;
    "detail.sharpenAmount" => detail.sharpen_amount, "Sharpening", Detail, 0, 150, 0, 1, 0, Plain;
    "detail.sharpenRadius" => detail.sharpen_radius, "Radius", Detail, 0.5, 3, 1, 0.1, 1, Plain;
    "detail.sharpenDetail" => detail.sharpen_detail, "Detail", Detail, 0, 100, 25, 1, 0, Plain;
    "detail.sharpenMasking" => detail.sharpen_masking, "Masking", Detail, 0, 100, 0, 1, 0, Plain;
    "detail.nrLuminance" => detail.nr_luminance, "Noise Reduction", Detail, 0, 100, 0, 1, 0, Plain;
    "detail.nrDetail" => detail.nr_detail, "Detail", Detail, 0, 100, 50, 1, 0, Plain;
    "detail.nrContrast" => detail.nr_contrast, "Contrast", Detail, 0, 100, 0, 1, 0, Plain;
    "detail.nrColor" => detail.nr_color, "Color Noise Reduction", Detail, 0, 100, 0, 1, 0, Plain;
    "detail.nrColorDetail" => detail.nr_color_detail, "Detail", Detail, 0, 100, 50, 1, 0, Plain;
    "detail.nrColorSmoothness" => detail.nr_color_smoothness, "Smoothness", Detail, 0, 100, 50, 1, 0, Plain;
    "optics.distortion" => optics.distortion, "Distortion", Optics, -100, 100, 0, 1, 0, Centered;
    "optics.vignetting" => optics.vignetting, "Vignetting", Optics, -100, 100, 0, 1, 0, Centered;
    "optics.vignettingMidpoint" => optics.vignetting_midpoint, "Midpoint", Optics, 0, 100, 50, 1, 0, Plain;
    "optics.profileDistortion" => optics.profile_distortion, "Profile Distortion", Optics, 0, 200, 100, 1, 0, Plain;
    "optics.profileVignetting" => optics.profile_vignetting, "Profile Vignetting", Optics, 0, 200, 100, 1, 0, Plain;
    "optics.defringePurple" => optics.defringe_purple_amount, "Purple Amount", Optics, 0, 20, 0, 1, 0, Gradient { from: "#7a7a7a", to: "#a040d0" };
    "optics.defringeGreen" => optics.defringe_green_amount, "Green Amount", Optics, 0, 20, 0, 1, 0, Gradient { from: "#7a7a7a", to: "#40c040" };
    "geometry.vertical" => geometry.vertical, "Vertical", Geometry, -100, 100, 0, 1, 0, Centered;
    "geometry.horizontal" => geometry.horizontal, "Horizontal", Geometry, -100, 100, 0, 1, 0, Centered;
    "geometry.rotate" => geometry.rotate, "Rotate", Geometry, -10, 10, 0, 0.1, 1, Centered;
    "geometry.aspect" => geometry.aspect, "Aspect", Geometry, -100, 100, 0, 1, 0, Centered;
    "geometry.scale" => geometry.scale, "Scale", Geometry, 50, 150, 100, 1, 0, Plain;
    "geometry.offsetX" => geometry.offset_x, "Offset X", Geometry, -100, 100, 0, 0.1, 1, Centered;
    "geometry.offsetY" => geometry.offset_y, "Offset Y", Geometry, -100, 100, 0, 0.1, 1, Centered;
    "crop.angle" => crop.geometry.angle, "Straighten", Geometry, -45, 45, 0, 0.01, 2, Centered;
}

pub fn find(id: &str) -> Option<&'static ControlSpec> {
    CONTROLS.iter().find(|c| c.id == id)
}

pub fn in_section(section: Section) -> impl Iterator<Item = &'static ControlSpec> {
    CONTROLS.iter().filter(move |c| c.section == section)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_control_reads_its_default() {
        let s = DevelopSettings::default();
        for c in CONTROLS {
            let v = get(&s, c.id).unwrap_or_else(|| panic!("no getter for {}", c.id));
            assert_eq!(v, c.default, "{}", c.id);
            assert!(c.min <= c.default && c.default <= c.max, "{}", c.id);
        }
    }

    #[test]
    fn ids_unique_and_settable() {
        let mut seen = std::collections::HashSet::new();
        let mut s = DevelopSettings::default();
        for c in CONTROLS {
            assert!(seen.insert(c.id), "duplicate {}", c.id);
            assert!(set(&mut s, c.id, c.max + 1000.0));
            assert_eq!(get(&s, c.id), Some(c.max), "{}", c.id);
            assert!(set(&mut s, c.id, f64::NAN));
            assert_eq!(get(&s, c.id), Some(c.default));
        }
        assert!(!set(&mut s, "nope", 1.0));
        assert_eq!(get(&s, "nope"), None);
    }

    #[test]
    fn formatting() {
        let e = find("light.exposure").unwrap();
        assert_eq!(e.format(0.5), "+0.50");
        assert_eq!(e.format(-1.0), "-1.00");
        assert_eq!(find("wb.temp").unwrap().format(5500.0), "5500");
    }
}
