//! Base rendering profiles. A profile is a look applied *underneath* the user's sliders (the sliders
//! stay at their displayed values); `profile.amount` (0–200 %) scales it. All looks are our own.

use std::borrow::Cow;

use lightcraft_develop::{DevelopSettings, Treatment};

/// The settings the pipeline actually renders: user settings plus the profile's look.
pub fn effective(s: &DevelopSettings) -> Cow<'_, DevelopSettings> {
    let k = (s.profile.amount / 100.0).clamp(0.0, 2.0);
    let id = s.profile.id.as_str();
    if id == "lc.color" || id.is_empty() || k == 0.0 {
        return Cow::Borrowed(s);
    }
    let mut e = s.clone();
    let add = |v: &mut f64, d: f64, lo: f64, hi: f64| *v = (*v + d * k).clamp(lo, hi);
    match id {
        "lc.neutral" => {
            add(&mut e.light.contrast, -18.0, -100.0, 100.0);
            add(&mut e.color.saturation, -10.0, -100.0, 100.0);
            add(&mut e.light.highlights, -10.0, -100.0, 100.0);
            add(&mut e.light.shadows, 8.0, -100.0, 100.0);
        }
        "lc.vivid" => {
            add(&mut e.light.contrast, 16.0, -100.0, 100.0);
            add(&mut e.color.saturation, 14.0, -100.0, 100.0);
            add(&mut e.color.vibrance, 14.0, -100.0, 100.0);
            add(&mut e.light.blacks, -6.0, -100.0, 100.0);
        }
        "lc.landscape" => {
            add(&mut e.light.contrast, 10.0, -100.0, 100.0);
            add(&mut e.color.vibrance, 10.0, -100.0, 100.0);
            add(&mut e.mixer.green.sat, 14.0, -100.0, 100.0);
            add(&mut e.mixer.aqua.sat, 10.0, -100.0, 100.0);
            add(&mut e.mixer.blue.sat, 14.0, -100.0, 100.0);
            add(&mut e.mixer.blue.lum, -6.0, -100.0, 100.0);
            add(&mut e.effects.clarity, 6.0, -100.0, 100.0);
        }
        "lc.portrait" => {
            add(&mut e.light.contrast, -6.0, -100.0, 100.0);
            add(&mut e.mixer.orange.sat, -8.0, -100.0, 100.0);
            add(&mut e.mixer.orange.lum, 6.0, -100.0, 100.0);
            add(&mut e.mixer.red.sat, -5.0, -100.0, 100.0);
            add(&mut e.effects.texture, -6.0, -100.0, 100.0);
        }
        "lc.mono" => {
            e.treatment = Treatment::Bw;
            add(&mut e.light.contrast, 8.0, -100.0, 100.0);
        }
        _ => return Cow::Borrowed(s),
    }
    Cow::Owned(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_is_borrowed_and_others_change_things() {
        let s = DevelopSettings::default();
        assert!(matches!(effective(&s), Cow::Borrowed(_)));
        for id in ["lc.neutral", "lc.vivid", "lc.landscape", "lc.portrait", "lc.mono"] {
            let mut t = s.clone();
            t.profile.id = id.into();
            assert_ne!(*effective(&t), s, "{id}");
            t.profile.amount = 0.0;
            assert_eq!(*effective(&t), t, "{id} at 0%");
        }
    }
}
