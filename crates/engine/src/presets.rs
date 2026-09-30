//! Built-in presets and profiles. All values are our own (no third-party preset content).

use lightcraft_develop::Preset;
use serde::Serialize;
use serde_json::json;

fn p(group: &str, id: &str, name: &str, settings: serde_json::Value) -> Preset {
    Preset { id: format!("lc.{id}"), name: name.into(), group: group.into(), settings, favorite: false, builtin: true }
}

pub fn builtin() -> Vec<Preset> {
    vec![
        p(
            "Color",
            "warm-glow",
            "Warm Glow",
            json!({"wb": {"mode": "custom", "temp": 7200.0, "tint": 8.0}, "light": {"contrast": 8.0, "shadows": 12.0}, "color": {"vibrance": 18.0}}),
        ),
        p(
            "Color",
            "cool-morning",
            "Cool Morning",
            json!({"wb": {"mode": "custom", "temp": 5600.0, "tint": -4.0}, "light": {"highlights": -20.0}, "color": {"vibrance": 10.0}, "grading": {"shadows": {"hue": 215.0, "sat": 12.0, "lum": 0.0}}}),
        ),
        p(
            "Color",
            "teal-orange",
            "Teal & Orange",
            json!({"grading": {"shadows": {"hue": 195.0, "sat": 28.0, "lum": 0.0}, "highlights": {"hue": 35.0, "sat": 22.0, "lum": 0.0}, "balance": 10.0}, "color": {"vibrance": 12.0}, "light": {"contrast": 14.0}}),
        ),
        p(
            "Color",
            "vivid-pop",
            "Vivid Pop",
            json!({"color": {"vibrance": 40.0, "saturation": 8.0}, "light": {"contrast": 18.0, "whites": 10.0, "blacks": -10.0}, "effects": {"clarity": 12.0}}),
        ),
        p(
            "Color",
            "soft-pastel",
            "Soft Pastel",
            json!({"light": {"contrast": -25.0, "highlights": -30.0, "shadows": 35.0, "blacks": 20.0}, "color": {"saturation": -18.0, "vibrance": 10.0}, "curve": {"shadows": 25.0}}),
        ),
        p(
            "Film",
            "faded-matte",
            "Faded Matte",
            json!({"curve": {"shadows": 40.0, "highlights": -15.0}, "light": {"contrast": -10.0}, "color": {"saturation": -12.0}, "grain": {"amount": 18.0, "size": 25.0, "roughness": 50.0}}),
        ),
        p(
            "Film",
            "warm-film",
            "Warm Film",
            json!({"wb": {"mode": "custom", "temp": 6900.0, "tint": 5.0}, "curve": {"shadows": 22.0}, "grading": {"highlights": {"hue": 45.0, "sat": 15.0, "lum": 0.0}}, "grain": {"amount": 22.0, "size": 30.0, "roughness": 55.0}, "vignette": {"amount": -12.0}}),
        ),
        p(
            "Film",
            "muted-film",
            "Muted Film",
            json!({"color": {"saturation": -25.0, "vibrance": 5.0}, "light": {"contrast": 10.0}, "curve": {"shadows": 18.0}, "grain": {"amount": 15.0}}),
        ),
        p(
            "B&W",
            "bw-high-contrast",
            "High Contrast B&W",
            json!({"treatment": "bw", "light": {"contrast": 45.0, "whites": 20.0, "blacks": -25.0}, "effects": {"clarity": 20.0}, "bw_mix": {"blue": -30.0, "red": 15.0}}),
        ),
        p("B&W", "bw-soft", "Soft B&W", json!({"treatment": "bw", "light": {"contrast": -15.0, "shadows": 25.0}, "curve": {"shadows": 15.0}})),
        p(
            "B&W",
            "bw-selenium",
            "Selenium Tone",
            json!({"treatment": "bw", "light": {"contrast": 20.0}, "grading": {"shadows": {"hue": 285.0, "sat": 18.0, "lum": 0.0}, "highlights": {"hue": 40.0, "sat": 10.0, "lum": 0.0}}}),
        ),
        p(
            "Landscape",
            "crisp-landscape",
            "Crisp Landscape",
            json!({"light": {"highlights": -45.0, "shadows": 30.0, "contrast": 12.0}, "effects": {"clarity": 22.0, "dehaze": 12.0, "texture": 15.0}, "color": {"vibrance": 25.0}}),
        ),
        p(
            "Landscape",
            "golden-hour",
            "Golden Hour",
            json!({"wb": {"mode": "custom", "temp": 7600.0, "tint": 12.0}, "light": {"highlights": -35.0, "shadows": 20.0}, "grading": {"highlights": {"hue": 38.0, "sat": 25.0, "lum": 5.0}}, "vignette": {"amount": -15.0}}),
        ),
        p(
            "Landscape",
            "blue-hour",
            "Blue Hour Boost",
            json!({"wb": {"mode": "custom", "temp": 5200.0, "tint": 6.0}, "light": {"shadows": 25.0, "exposure": 0.2}, "mixer": {"blue": {"hue": 0.0, "sat": 25.0, "lum": 0.0}, "purple": {"hue": 0.0, "sat": 15.0, "lum": 0.0}}}),
        ),
        p(
            "Portrait",
            "soft-skin",
            "Soft Skin",
            json!({"effects": {"texture": -25.0, "clarity": -10.0}, "light": {"contrast": -5.0, "shadows": 15.0}, "mixer": {"orange": {"hue": 0.0, "sat": -8.0, "lum": 10.0}}}),
        ),
        p(
            "Portrait",
            "bright-airy",
            "Bright & Airy",
            json!({"light": {"exposure": 0.45, "contrast": -15.0, "highlights": -40.0, "shadows": 40.0, "whites": 15.0}, "color": {"vibrance": 8.0, "saturation": -8.0}}),
        ),
        p(
            "Style",
            "moody",
            "Moody",
            json!({"light": {"exposure": -0.3, "contrast": 20.0, "highlights": -30.0, "blacks": -15.0}, "color": {"saturation": -20.0}, "grading": {"shadows": {"hue": 200.0, "sat": 15.0, "lum": -5.0}}, "vignette": {"amount": -30.0}}),
        ),
        p(
            "Style",
            "cinematic",
            "Cinematic",
            json!({"curve": {"shadows": 20.0, "highlights": -10.0}, "grading": {"shadows": {"hue": 190.0, "sat": 25.0, "lum": 0.0}, "highlights": {"hue": 30.0, "sat": 18.0, "lum": 0.0}}, "light": {"contrast": 15.0}, "vignette": {"amount": -20.0}}),
        ),
    ]
}

#[derive(Clone, Debug, Serialize)]
pub struct ProfileInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub group: &'static str,
}

/// Our base profiles (rendering looks). Implemented in the pipeline.
pub const PROFILES: &[ProfileInfo] = &[
    ProfileInfo { id: "lc.color", name: "Color", group: "Basic" },
    ProfileInfo { id: "lc.neutral", name: "Neutral", group: "Basic" },
    ProfileInfo { id: "lc.vivid", name: "Vivid", group: "Basic" },
    ProfileInfo { id: "lc.landscape", name: "Landscape", group: "Basic" },
    ProfileInfo { id: "lc.portrait", name: "Portrait", group: "Basic" },
    ProfileInfo { id: "lc.mono", name: "Monochrome", group: "Basic" },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_presets_parse_and_apply() {
        let base = lightcraft_develop::DevelopSettings::default();
        let mut ids = std::collections::HashSet::new();
        for pr in builtin() {
            assert!(ids.insert(pr.id.clone()));
            let out = pr.apply(&base, 1.0);
            assert_ne!(out, base, "{} had no effect", pr.id);
        }
    }
}
