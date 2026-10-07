//! Compile translated format strings so Rust checks every language's placeholders.
use std::{collections::BTreeMap, env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=locales/ja-formats.json");
    println!("cargo:rerun-if-changed=locales/zh-tw-formats.json");
    let japanese: BTreeMap<String, String> = serde_json::from_str(&fs::read_to_string("locales/ja-formats.json")?)?;
    let chinese: BTreeMap<String, String> = serde_json::from_str(&fs::read_to_string("locales/zh-tw-formats.json")?)?;
    if !japanese.keys().eq(chinese.keys()) {
        return Err("Format catalogs must contain the same English keys".into());
    }
    let mut source = String::from("macro_rules! tr_format {\n");
    for (english, japanese) in japanese {
        let en = serde_json::to_string(&english)?;
        let ja = serde_json::to_string(&japanese)?;
        let zh = serde_json::to_string(chinese.get(&english).ok_or("Missing Chinese format")?)?;
        source.push_str(&format!(
            "({en} $(, $($args:tt)*)?) => {{ match $crate::i18n::language() {{ $crate::i18n::Language::Ja => format!({ja} $(, $($args)*)?), $crate::i18n::Language::ZhTw => format!({zh} $(, $($args)*)?), $crate::i18n::Language::En => format!({en} $(, $($args)*)?) }} }};\n"
        ));
    }
    source.push_str("}\npub(crate) use tr_format;\n");
    fs::write(PathBuf::from(env::var("OUT_DIR")?).join("formats.rs"), source)?;
    Ok(())
}
