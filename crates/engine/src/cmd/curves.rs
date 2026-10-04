//! Tone-curve commands: reset a channel or the whole curve.

use lightcraft_develop::ToneCurve;
use serde_json::{Value, json};

use super::{CommandSpec, bad, cmd, has_active, str_param};
use crate::{Result, Session};

/// Channels `curve.reset` accepts.
pub const RESET_CHANNELS: [&str; 7] = ["all", "point", "parametric", "master", "red", "green", "blue"];

/// `c` with `channel` back at its default: `all` (the whole Tone Curve, incl. Refine Saturation),
/// `point` (the four point curves), `parametric` (region sliders and splits) or one point channel.
pub fn reset_channel(c: &mut ToneCurve, channel: &str) -> std::result::Result<(), String> {
    let d = ToneCurve::default();
    match channel {
        "all" => *c = d,
        "point" => {
            c.master.clear();
            c.red.clear();
            c.green.clear();
            c.blue.clear();
        }
        "parametric" => {
            (c.highlights, c.lights, c.darks, c.shadows) = (d.highlights, d.lights, d.darks, d.shadows);
            (c.split_shadows, c.split_mid, c.split_highlights) = (d.split_shadows, d.split_mid, d.split_highlights);
        }
        "master" | "rgb" | "luma" => c.master.clear(),
        "red" => c.red.clear(),
        "green" => c.green.clear(),
        "blue" => c.blue.clear(),
        other => return Err(format!("unknown channel `{other}` (one of {})", RESET_CHANNELS.join(", "))),
    }
    Ok(())
}

fn reset(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "curve.reset";
    let id = s.active().ok_or_else(|| bad(C, "no active photo"))?;
    let channel = str_param(p, "channel").unwrap_or("all");
    let mut d = (*s.develop_of(id).unwrap_or_default()).clone();
    reset_channel(&mut d.curve, channel).map_err(|e| bad(C, e))?;
    let label = if channel == "all" { "Reset Tone Curve".to_string() } else { format!("Reset Tone Curve ({channel})") };
    s.set_develop(id, d, &label)?;
    Ok(json!({"channel": channel}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "curve.reset",
        "Reset Tone Curve",
        [],
        None,
        "{channel?: all|point|parametric|master|red|green|blue (default all)} — the active photo's tone curve (or one channel) back to linear",
        has_active,
        reset
    )]
}
