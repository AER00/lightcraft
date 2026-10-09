//! Smart-album rules: a list of conditions matched all / any / none, with nested groups.
//!
//! Each [`Rule`] is `{field, op, value}`; [`FIELDS`] lists the fields with their kind, which
//! decides the operators ([`ops_for`]). Text compares case-insensitively; dates are ISO strings
//! compared by prefix; "in the last N days/weeks/months/years" is relative to [`now`].

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Catalog, Photo, Source};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Match {
    #[default]
    All,
    Any,
    None,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RuleSet {
    #[serde(default, rename = "match")]
    pub mode: Match,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Rule {
    /// A nested group with its own match mode.
    Group { group: RuleSet },
    Field {
        field: String,
        op: String,
        #[serde(default)]
        value: Value,
    },
}

/// How a field's value is compared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    /// A list of names (keywords, people): text ops over each name (any name matches; `isEmpty` =
    /// none).
    Keywords,
    Number,
    Date,
    /// One of a fixed set (`choices`).
    Choice(&'static [&'static str]),
    Bool,
}

/// The fields rules can test: (id, label, kind), in field-menu order ([`TOP_LEVEL_FIELDS`], then
/// each of [`FIELD_GROUPS`]).
pub const FIELDS: &[(&str, &str, Kind)] = &[
    ("rating", "Rating", Kind::Number),
    ("flag", "Pick Flag", Kind::Choice(&["pick", "reject", "none"])),
    ("label", "Color Label", Kind::Choice(&["red", "yellow", "green", "blue", "purple", "none"])),
    ("text", "Any Searchable Text", Kind::Text),
    // Source
    ("album", "Album", Kind::Number),
    ("virtualCopy", "Virtual Copy", Kind::Bool),
    ("copyName", "Copy Name", Kind::Text),
    ("stacked", "In a Stack", Kind::Bool),
    // File
    ("fileName", "Filename", Kind::Text),
    ("extension", "File Extension", Kind::Text),
    ("filePath", "File Path", Kind::Text),
    ("kind", "File Type", Kind::Choice(&["image", "raw", "video"])),
    ("format", "File Format", Kind::Text),
    ("duration", "Video Duration", Kind::Number),
    // Date
    ("captureDate", "Capture Date", Kind::Date),
    ("importDate", "Import Date", Kind::Date),
    ("editDate", "Edit Date", Kind::Date),
    // Keywords & People
    ("keywords", "Keywords", Kind::Keywords),
    ("keywordCount", "Keyword Count", Kind::Number),
    ("person", "People", Kind::Keywords),
    ("personCount", "People Count", Kind::Number),
    // Description
    ("title", "Title", Kind::Text),
    ("caption", "Caption", Kind::Text),
    ("altText", "Alt Text", Kind::Text),
    ("creator", "Creator", Kind::Text),
    ("copyright", "Copyright", Kind::Text),
    ("copyrightStatus", "Copyright Status", Kind::Choice(&["copyrighted", "publicDomain", "unknown"])),
    // Camera Info
    ("camera", "Camera", Kind::Text),
    ("lens", "Lens", Kind::Text),
    ("focalLength", "Focal Length", Kind::Number),
    ("aperture", "Aperture", Kind::Number),
    ("shutterSpeed", "Shutter Speed", Kind::Number),
    ("iso", "ISO Speed", Kind::Number),
    // Location
    ("location", "Location", Kind::Text),
    ("city", "City", Kind::Text),
    ("state", "State / Province", Kind::Text),
    ("country", "Country", Kind::Text),
    ("hasGps", "Has GPS", Kind::Bool),
    // Size
    ("longEdge", "Long Edge", Kind::Number),
    ("shortEdge", "Short Edge", Kind::Number),
    ("aspect", "Aspect Ratio", Kind::Choice(&["landscape", "portrait", "square"])),
    ("megapixels", "Megapixels", Kind::Number),
    // Develop
    ("edited", "Has Edits", Kind::Bool),
    ("cropped", "Cropped", Kind::Bool),
    ("treatment", "Treatment", Kind::Choice(&["color", "monochrome"])),
    // Assisted Culling
    ("sharpness", "Focus (assisted culling)", Kind::Number),
    ("bestOfGroup", "Best of Similar Shots", Kind::Bool),
];

/// The fields the field menu shows at its top level, before the groups.
pub const TOP_LEVEL_FIELDS: &[&str] = &["rating", "flag", "label", "text"];

/// The field menu's submenus: (label, fields), in order. Every field of [`FIELDS`] is either here
/// once or in [`TOP_LEVEL_FIELDS`].
pub const FIELD_GROUPS: &[(&str, &[&str])] = &[
    ("Source", &["album", "virtualCopy", "copyName", "stacked"]),
    ("File", &["fileName", "extension", "filePath", "kind", "format", "duration"]),
    ("Date", &["captureDate", "importDate", "editDate"]),
    ("Keywords & People", &["keywords", "keywordCount", "person", "personCount"]),
    ("Description", &["title", "caption", "altText", "creator", "copyright", "copyrightStatus"]),
    ("Camera Info", &["camera", "lens", "focalLength", "aperture", "shutterSpeed", "iso"]),
    ("Location", &["location", "city", "state", "country", "hasGps"]),
    ("Size", &["longEdge", "shortEdge", "aspect", "megapixels"]),
    ("Develop", &["edited", "cropped", "treatment"]),
    ("Assisted Culling", &["sharpness", "bestOfGroup"]),
];

/// The field-menu group `field` sits in; `None` for a top-level or unknown field.
pub fn field_group(field: &str) -> Option<&'static str> {
    FIELD_GROUPS.iter().find(|g| g.1.contains(&field)).map(|g| g.0)
}

/// The display label of `field`; `None` for an unknown field.
pub fn field_label(field: &str) -> Option<&'static str> {
    FIELDS.iter().find(|f| f.0 == field).map(|f| f.1)
}

/// The operators for a field kind: (id, label).
pub fn ops_for(kind: Kind) -> &'static [(&'static str, &'static str)] {
    match kind {
        Kind::Text => &[
            ("contains", "contains"),
            ("notContains", "doesn't contain"),
            ("is", "is"),
            ("isNot", "isn't"),
            ("startsWith", "starts with"),
            ("endsWith", "ends with"),
            ("isEmpty", "is empty"),
            ("isNotEmpty", "isn't empty"),
        ],
        Kind::Keywords => &[
            ("contains", "contains"),
            ("notContains", "doesn't contain"),
            ("is", "is"),
            ("startsWith", "starts with"),
            ("isEmpty", "are empty"),
            ("isNotEmpty", "aren't empty"),
        ],
        Kind::Number => {
            &[("is", "is"), ("isNot", "isn't"), ("gte", "is ≥"), ("lte", "is ≤"), ("gt", "is >"), ("lt", "is <"), ("between", "is between")]
        }
        Kind::Date => &[
            ("is", "is"),
            ("after", "is after"),
            ("before", "is before"),
            ("between", "is between"),
            ("inLast", "is in the last"),
            ("notInLast", "isn't in the last"),
            ("isEmpty", "is unknown"),
        ],
        Kind::Choice(_) => &[("is", "is"), ("isNot", "isn't")],
        Kind::Bool => &[("is", "is")],
    }
}

pub fn field_kind(field: &str) -> Option<Kind> {
    FIELDS.iter().find(|f| f.0 == field).map(|f| f.2)
}

thread_local! {
    static NOW: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Set the time "in the last…" rules count back from (the engine's clock; tests).
pub fn set_now(iso: Option<String>) {
    NOW.with(|n| *n.borrow_mut() = iso);
}

/// The current time as ISO 8601: [`set_now`]'s, else the system clock (UTC).
pub fn now() -> String {
    if let Some(n) = NOW.with(|n| n.borrow().clone()) {
        return n;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        crate::dates::civil(secs)
    }
    #[cfg(target_arch = "wasm32")]
    {
        "2026-01-01T00:00:00".to_string()
    }
}

fn lower(v: &Value) -> String {
    match v {
        Value::String(s) => s.trim().to_lowercase(),
        Value::Null => String::new(),
        other => other.to_string().to_lowercase(),
    }
}

fn text_op(op: &str, have: &str, want: &str) -> bool {
    let have = have.trim().to_lowercase();
    match op {
        "contains" => want.split_whitespace().all(|w| have.contains(w)),
        "notContains" => !want.split_whitespace().any(|w| have.contains(w)),
        "is" => have == want,
        "isNot" => have != want,
        "startsWith" => have.starts_with(want),
        "endsWith" => have.ends_with(want),
        "isEmpty" => have.is_empty(),
        "isNotEmpty" => !have.is_empty(),
        _ => false,
    }
}

/// A [`Kind::Keywords`] op over a list of names: any name matches (none for `notContains`); `is`
/// also matches one level of a hierarchical keyword (`italy` in `travel|italy|rome`).
fn names_op<S: AsRef<str>>(op: &str, names: &[S], want: &str) -> bool {
    let names = names.iter().map(AsRef::as_ref);
    match op {
        "isEmpty" => names.clone().all(|n| n.trim().is_empty()),
        "isNotEmpty" => names.clone().any(|n| !n.trim().is_empty()),
        "notContains" => !names.clone().any(|n| text_op("contains", n, want)),
        _ => names.clone().any(|n| text_op(op, n, want) || (op == "is" && n.to_lowercase().split('|').any(|part| part.trim() == want))),
    }
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().trim_start_matches("f/").trim_end_matches("mm").trim().parse().ok(),
        _ => None,
    }
}

fn num_op(op: &str, have: Option<f64>, value: &Value) -> bool {
    if op == "between" {
        let (a, b) = match value {
            Value::Array(v) if v.len() == 2 => (number(&v[0]), number(&v[1])),
            _ => (None, None),
        };
        return matches!((have, a, b), (Some(h), Some(a), Some(b)) if h >= a.min(b) && h <= a.max(b));
    }
    let (Some(h), Some(w)) = (have, number(value)) else { return op == "isNot" && have.is_none() };
    match op {
        "is" => (h - w).abs() < 1e-6,
        "isNot" => (h - w).abs() >= 1e-6,
        "gte" => h >= w - 1e-9,
        "lte" => h <= w + 1e-9,
        "gt" => h > w,
        "lt" => h < w,
        _ => false,
    }
}

/// `landscape`, `portrait` or `square` (edges within 1 %) for a `w × h` frame; `None` when it has no area.
fn aspect((w, h): (f64, f64)) -> Option<&'static str> {
    if !(w > 0.0 && h > 0.0) {
        return None;
    }
    Some(if (w - h).abs() <= 0.01 * w.max(h) {
        "square"
    } else if w > h {
        "landscape"
    } else {
        "portrait"
    })
}

/// A shutter-speed rule's value in seconds: camera notation (`1/250`) or a number; an operand
/// that isn't a time becomes `null`, which matches nothing.
fn shutter_value(value: &Value) -> Value {
    let secs = |v: &Value| match v {
        Value::String(s) => crate::parse_shutter_seconds(s).map_or(Value::Null, Value::from),
        other => number(other).filter(|n| n.is_finite() && *n > 0.0).map_or(Value::Null, Value::from),
    };
    match value {
        Value::Array(a) => Value::Array(a.iter().map(secs).collect()),
        v => secs(v),
    }
}

/// `value` for in-the-last rules: `{n, unit: days|weeks|months|years}` or a number of days.
fn last_secs(value: &Value) -> Option<i64> {
    let (n, unit) = match value {
        Value::Object(o) => (o.get("n").and_then(number)?, o.get("unit").and_then(Value::as_str).unwrap_or("days")),
        v => (number(v)?, "days"),
    };
    let day = 86_400.0;
    let per = match unit {
        "hours" => 3600.0,
        "weeks" => 7.0 * day,
        "months" => 30.4375 * day,
        "years" => 365.25 * day,
        _ => day,
    };
    Some((n * per) as i64)
}

fn date_op(op: &str, have: Option<&str>, value: &Value) -> bool {
    if op == "isEmpty" {
        return have.is_none_or(str::is_empty);
    }
    let Some(h) = have.filter(|h| !h.is_empty()) else { return op == "notInLast" };
    let s = |v: &Value| v.as_str().map(str::trim).unwrap_or("").to_string();
    match op {
        // a prefix: 2026, 2026-04, 2026-04-12
        "is" => {
            let w = s(value);
            !w.is_empty() && h.starts_with(&w)
        }
        "after" => {
            let w = s(value);
            !w.is_empty() && h > w.as_str() && !h.starts_with(&w)
        }
        "before" => {
            let w = s(value);
            !w.is_empty() && h < w.as_str()
        }
        "between" => match value {
            Value::Array(v) if v.len() == 2 => {
                let (a, b) = (s(&v[0]), s(&v[1]));
                h >= a.as_str() && (h <= b.as_str() || h.starts_with(&b))
            }
            _ => false,
        },
        "inLast" | "notInLast" => {
            let Some(secs) = last_secs(value) else { return false };
            let from = crate::dates::shift_iso(&now(), -secs).unwrap_or_default();
            (h >= from.as_str()) == (op == "inLast")
        }
        _ => false,
    }
}

impl Rule {
    pub fn matches(&self, p: &Photo, cat: &Catalog) -> bool {
        let (field, op, value) = match self {
            Rule::Group { group } => return group.matches(p, cat),
            Rule::Field { field, op, value } => (field.as_str(), op.as_str(), value),
        };
        let m = &p.meta;
        let want = lower(value);
        let text = |have: &str| text_op(op, have, &want);
        match field {
            "rating" => num_op(op, Some(p.rating as f64), value),
            "flag" => {
                let f = format!("{:?}", p.flag).to_lowercase();
                (f == want || (want == "picked" && f == "pick") || (want == "rejected" && f == "reject")) == (op == "is")
            }
            "label" => {
                let l = p.label.map(|l| format!("{l:?}").to_lowercase()).unwrap_or_else(|| "none".into());
                // custom label names count too
                let named = p.label.is_some_and(|l| cat.label_name(l).to_lowercase() == want);
                (l == want || named) == (op == "is")
            }
            "kind" => (format!("{:?}", p.kind).to_lowercase() == want) == (op == "is"),
            "edited" => p.is_edited() == value.as_bool().unwrap_or(true),
            "hasGps" => m.gps.is_some() == value.as_bool().unwrap_or(true),
            "virtualCopy" => p.copy_of.is_some() == value.as_bool().unwrap_or(true),
            "copyName" => text(p.copy_name.as_deref().unwrap_or("")),
            "stacked" => cat.stack_of(p.id).is_some() == value.as_bool().unwrap_or(true),
            "cropped" => p.is_cropped() == value.as_bool().unwrap_or(true),
            "treatment" => {
                let t = if p.develop.treatment == lightcraft_develop::Treatment::Bw { "monochrome" } else { "color" };
                (t == want) == (op == "is")
            }
            "keywords" => names_op(op, &m.keywords, &want),
            "keywordCount" => num_op(op, Some(p.keyword_count() as f64), value),
            "person" => names_op(op, &p.people(), &want),
            "personCount" => num_op(op, Some(p.people().len() as f64), value),
            "text" => {
                let all = [
                    p.file_name.as_str(),
                    &m.title,
                    &m.caption,
                    &m.camera,
                    &m.lens,
                    &m.location,
                    &m.city,
                    &m.state,
                    &m.country,
                    &m.alt_text,
                    &p.format,
                    &m.keywords.join(" "),
                    &p.people().join(" "),
                    p.copy_name.as_deref().unwrap_or(""),
                ]
                .join(" ");
                text(&all)
            }
            "fileName" => text(&p.file_name),
            "extension" => text_op(op, p.extension(), want.trim_start_matches('.')),
            "duration" => num_op(op, p.duration.filter(|d| d.is_finite()), value),
            "filePath" => {
                // `/` and `\` both separate, whatever platform the catalog came from; demo photos have no path
                let path = match &p.source {
                    Source::File { path } => path.replace('\\', "/"),
                    Source::Demo { .. } => String::new(),
                };
                let want = want.replace('\\', "/");
                match op {
                    // the whole string, spaces included (not word by word like the other text fields)
                    "contains" => !want.trim().is_empty() && path.to_lowercase().contains(want.trim()),
                    "notContains" => want.trim().is_empty() || !path.to_lowercase().contains(want.trim()),
                    _ => text_op(op, &path, &want),
                }
            }
            "format" => text(&p.format),
            "title" => text(&m.title),
            "caption" => text(&m.caption),
            "altText" => text(&m.alt_text),
            "city" => text(&m.city),
            "state" => text(&m.state),
            "country" => text(&m.country),
            "camera" => text(&m.camera),
            "lens" => text(&m.lens),
            "location" => text(&[m.location.as_str(), &m.city, &m.state, &m.country].join(" ")),
            "creator" => text(&m.creator),
            "copyright" => text(&m.copyright),
            "copyrightStatus" => {
                let want = crate::CopyrightStatus::parse(&want);
                (want == Some(m.copyright_status)) == (op == "is")
            }
            "captureDate" => date_op(op, p.captured.as_deref(), value),
            "importDate" => date_op(op, Some(&p.imported), value),
            "editDate" => date_op(op, p.edited.as_deref(), value),
            "shutterSpeed" => num_op(op, crate::parse_shutter_seconds(&m.shutter), &shutter_value(value)),
            "iso" => num_op(op, m.iso.map(|v| v as f64), value),
            "aperture" => num_op(op, m.aperture.map(|v| v as f64), value),
            "focalLength" => num_op(op, m.focal_mm.map(|v| v as f64), value),
            "longEdge" | "shortEdge" => {
                let (w, h) = p.shown_size();
                num_op(op, Some(if field == "longEdge" { w.max(h) } else { w.min(h) }.round()), value)
            }
            "aspect" => (aspect(p.shown_size()) == Some(want.as_str())) == (op == "is"),
            "megapixels" => num_op(op, Some(p.width as f64 * p.height as f64 / 1e6), value),
            "sharpness" => num_op(op, p.analysis.map(|a| a.sharpness as f64), value),
            "bestOfGroup" => p.analysis.is_some_and(|a| a.best || a.group.is_none()) == value.as_bool().unwrap_or(true),
            "album" => {
                let id = number(value).map(|v| crate::AlbumId(v as u64));
                id.is_some_and(|a| cat.album(a).is_some_and(|al| !al.is_smart()) && cat.album_contains(a, p)) == (op == "is")
            }
            _ => false,
        }
    }
}

impl RuleSet {
    pub fn matches(&self, p: &Photo, cat: &Catalog) -> bool {
        if self.rules.is_empty() {
            return self.mode != Match::Any;
        }
        match self.mode {
            Match::All => self.rules.iter().all(|r| r.matches(p, cat)),
            Match::Any => self.rules.iter().any(|r| r.matches(p, cat)),
            Match::None => !self.rules.iter().any(|r| r.matches(p, cat)),
        }
    }

    /// Whether matches depend on the clock ("in the last…" rules, nested groups included): the
    /// same photos can enter or leave the set without any catalog change.
    pub fn depends_on_now(&self) -> bool {
        self.rules.iter().any(|r| match r {
            Rule::Group { group } => group.depends_on_now(),
            Rule::Field { op, .. } => op == "inLast" || op == "notInLast",
        })
    }

    /// Unknown fields or operators (for command validation), as readable messages.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        for r in &self.rules {
            match r {
                Rule::Group { group } => out.extend(group.problems()),
                Rule::Field { field, op, .. } => match field_kind(field) {
                    None => out.push(format!("unknown field `{field}`")),
                    Some(k) if !ops_for(k).iter().any(|o| o.0 == op) => out.push(format!("`{field}` has no operator `{op}`")),
                    _ => {}
                },
            }
        }
        out
    }

    /// A short readable summary ("rating is ≥ 3 and keywords contains travel").
    pub fn describe(&self) -> String {
        let join = match self.mode {
            Match::All => " and ",
            Match::Any => " or ",
            Match::None => " nor ",
        };
        let parts: Vec<String> = self
            .rules
            .iter()
            .map(|r| match r {
                Rule::Group { group } => format!("({})", group.describe()),
                Rule::Field { field, op, value } => {
                    let label = field_label(field).unwrap_or(field).to_lowercase();
                    let op = field_kind(field).and_then(|k| ops_for(k).iter().find(|o| o.0 == op)).map_or(op.as_str(), |o| o.1);
                    let v = match value {
                        Value::String(s) => s.clone(),
                        Value::Null => String::new(),
                        Value::Object(o) => format!(
                            "{} {}",
                            o.get("n").map(|n| n.to_string()).unwrap_or_default(),
                            o.get("unit").and_then(Value::as_str).unwrap_or("days")
                        ),
                        Value::Array(a) => a.iter().map(|x| x.as_str().map_or_else(|| x.to_string(), str::to_string)).collect::<Vec<_>>().join(" – "),
                        other => other.to_string(),
                    };
                    format!("{label} {op} {v}").trim().to_string()
                }
            })
            .collect();
        let s = parts.join(join);
        if self.mode == Match::None { format!("none of: {s}") } else { s }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ColorLabel, Flag, PhotoId, Source};
    use serde_json::json;

    fn photo(id: u64) -> Photo {
        let mut p = Photo::new(PhotoId(id), Source::Demo { scene: 0 }, "IMG_0042.CR2", "CR2", 6000, 4000, "2026-09-20T10:00:00");
        p.rating = 4;
        p.flag = Flag::Pick;
        p.label = Some(ColorLabel::Red);
        p.captured = Some("2026-08-14T18:30:00".into());
        p.meta.keywords = vec!["travel|italy|rome".into(), "food".into()];
        p.meta.camera = "Model X2".into();
        p.meta.iso = Some(1600);
        p.meta.aperture = Some(2.8);
        p
    }

    fn rs(v: serde_json::Value) -> RuleSet {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn fields_ops_and_groups() {
        let cat = Catalog::new();
        let p = photo(1);
        let yes = |v: serde_json::Value| assert!(rs(v.clone()).matches(&p, &cat), "{v}");
        let no = |v: serde_json::Value| assert!(!rs(v.clone()).matches(&p, &cat), "{v}");
        yes(json!({"rules": [{"field": "rating", "op": "gte", "value": 3}, {"field": "flag", "op": "is", "value": "pick"}]}));
        no(json!({"rules": [{"field": "rating", "op": "gt", "value": 4}]}));
        yes(json!({"match": "any", "rules": [{"field": "rating", "op": "is", "value": 1}, {"field": "label", "op": "is", "value": "red"}]}));
        no(json!({"match": "none", "rules": [{"field": "label", "op": "is", "value": "red"}]}));
        yes(json!({"rules": [{"field": "keywords", "op": "is", "value": "italy"}]}));
        yes(
            json!({"rules": [{"field": "keywords", "op": "contains", "value": "ROME"}, {"field": "keywords", "op": "notContains", "value": "beach"}]}),
        );
        no(json!({"rules": [{"field": "keywords", "op": "isEmpty"}]}));
        yes(json!({"rules": [{"field": "fileName", "op": "startsWith", "value": "img_"}, {"field": "fileName", "op": "endsWith", "value": ".cr2"}]}));
        // file path: the whole string (spaces included), either separator, case-insensitive
        let mut fp = photo(3);
        fp.source = Source::File { path: "D:\\Photos\\Aliah Ira Polanco-Grylls\\2026\\IMG_0042.CR2".into() };
        let fpm = |v: serde_json::Value| rs(v).matches(&fp, &cat);
        assert!(fpm(json!({"rules": [{"field": "filePath", "op": "contains", "value": "/Aliah Ira Polanco-Grylls/"}]})));
        assert!(!fpm(json!({"rules": [{"field": "filePath", "op": "contains", "value": "/Ira Aliah/"}]})));
        assert!(!fpm(json!({"rules": [{"field": "filePath", "op": "contains", "value": "Polanco Grylls"}]})));
        assert!(fpm(json!({"rules": [{"field": "filePath", "op": "notContains", "value": "/Other/"}]})));
        assert!(fpm(json!({"rules": [{"field": "filePath", "op": "startsWith", "value": "d:/photos/"}]})));
        no(json!({"rules": [{"field": "filePath", "op": "contains", "value": "/Aliah/"}]})); // demo photo has no path
        yes(json!({"rules": [{"field": "filePath", "op": "isEmpty"}]}));
        yes(json!({"rules": [{"field": "camera", "op": "contains", "value": "x2"}, {"field": "title", "op": "isEmpty"}]}));
        yes(json!({"rules": [{"field": "iso", "op": "between", "value": [800, 3200]}, {"field": "aperture", "op": "lte", "value": "f/4"}]}));
        yes(json!({"rules": [{"field": "megapixels", "op": "gte", "value": 24}]}));
        yes(
            json!({"rules": [{"field": "captureDate", "op": "is", "value": "2026-08"}, {"field": "captureDate", "op": "before", "value": "2026-09-01"}]}),
        );
        yes(json!({"rules": [{"field": "captureDate", "op": "between", "value": ["2026-08-01", "2026-08"]}]}));
        no(json!({"rules": [{"field": "captureDate", "op": "after", "value": "2026-08"}]}));
        yes(json!({"rules": [{"field": "kind", "op": "isNot", "value": "video"}, {"field": "edited", "op": "is", "value": false}]}));
        // copyright status (unknown until set)
        yes(json!({"rules": [{"field": "copyrightStatus", "op": "is", "value": "unknown"}]}));
        no(json!({"rules": [{"field": "copyrightStatus", "op": "is", "value": "copyrighted"}]}));
        let mut pd = photo(2);
        pd.meta.copyright_status = crate::CopyrightStatus::PublicDomain;
        assert!(rs(json!({"rules": [{"field": "copyrightStatus", "op": "is", "value": "publicDomain"}]})).matches(&pd, &cat));
        assert!(rs(json!({"rules": [{"field": "copyrightStatus", "op": "isNot", "value": "copyrighted"}]})).matches(&pd, &cat));
        // nested: rating ≥ 4 and (label is blue or keywords contain food)
        yes(json!({"rules": [{"field": "rating", "op": "gte", "value": 4}, {"group": {"match": "any", "rules": [
            {"field": "label", "op": "is", "value": "blue"}, {"field": "keywords", "op": "contains", "value": "food"}]}}]}));
        // in the last N days, relative to now
        set_now(Some("2026-09-01T00:00:00".into()));
        yes(json!({"rules": [{"field": "captureDate", "op": "inLast", "value": {"n": 30, "unit": "days"}}]}));
        no(json!({"rules": [{"field": "captureDate", "op": "inLast", "value": {"n": 1, "unit": "weeks"}}]}));
        yes(json!({"rules": [{"field": "captureDate", "op": "notInLast", "value": 7}]}));
        set_now(None);
        // an empty rule list: all → everything, any → nothing
        yes(json!({"rules": []}));
        no(json!({"match": "any", "rules": []}));
    }

    #[test]
    fn problems_and_description() {
        let r = rs(json!({"rules": [{"field": "rating", "op": "contains", "value": 1}, {"group": {"rules": [{"field": "nope", "op": "is"}]}}]}));
        assert_eq!(r.problems(), vec!["`rating` has no operator `contains`".to_string(), "unknown field `nope`".to_string()]);
        let r = rs(
            json!({"match": "any", "rules": [{"field": "rating", "op": "gte", "value": 3}, {"field": "captureDate", "op": "inLast", "value": {"n": 2, "unit": "weeks"}}]}),
        );
        assert_eq!(r.describe(), "rating is ≥ 3 or capture date is in the last 2 weeks");
        // every field has a kind with operators
        for (f, _, k) in FIELDS {
            assert!(!ops_for(*k).is_empty(), "{f}");
        }
    }

    /// Keyword Count: a photo with 2 keywords is in "≥ 2", "≤ 2" and "is 2"; a hierarchical path
    /// (travel|italy|rome) is one keyword, and the same keyword twice in different case is one.
    #[test]
    fn keyword_count_rules() {
        let cat = Catalog::new();
        let mut p = photo(1); // travel|italy|rome, food
        let m = |p: &Photo, op: &str, value: serde_json::Value| {
            rs(json!({"rules": [{"field": "keywordCount", "op": op, "value": value}]})).matches(p, &cat)
        };
        assert!(m(&p, "gte", json!(2)) && m(&p, "lte", json!(2)) && m(&p, "is", json!(2)));
        assert!(!m(&p, "gte", json!(3)) && !m(&p, "lt", json!(2)) && m(&p, "between", json!([1, 2])));
        p.meta.keywords.push("Food".into());
        assert!(m(&p, "is", json!(2)), "Food and food are one keyword");
        p.meta.keywords.clear();
        assert!(m(&p, "is", json!(0)) && m(&p, "lte", json!(2)));
        assert_eq!(field_group("keywordCount"), Some("Keywords & People"));
    }

    /// People: the names on face regions. "People contains ana" finds Ana Lima; pets and unnamed
    /// faces are not people; People Count counts each person once.
    #[test]
    fn people_rules() {
        use lightcraft_meta::{Rect, Region, RegionKind};
        let cat = Catalog::new();
        let region = |name: Option<&str>, kind: RegionKind| Region {
            rect: Rect { x0: 0.4, y0: 0.4, x1: 0.6, y1: 0.6 },
            kind,
            name: name.map(str::to_string),
            description: None,
        };
        let mut p = photo(1);
        let m = |p: &Photo, field: &str, op: &str, value: serde_json::Value| {
            rs(json!({"rules": [{"field": field, "op": op, "value": value}]})).matches(p, &cat)
        };
        assert!(m(&p, "person", "isEmpty", json!(null)) && m(&p, "personCount", "is", json!(0)));
        p.meta.regions = vec![
            region(Some("Ana Lima"), RegionKind::Face),
            region(Some(" ana lima "), RegionKind::Face),
            region(Some("Rex"), RegionKind::Pet),
            region(None, RegionKind::Face),
            region(Some("Bo"), RegionKind::Face),
        ];
        assert!(m(&p, "person", "contains", json!("ana")));
        assert!(m(&p, "person", "is", json!("ANA LIMA")));
        assert!(!m(&p, "person", "is", json!("rex")), "a pet isn't a person");
        assert!(m(&p, "person", "notContains", json!("rex")));
        assert!(m(&p, "person", "isNotEmpty", json!(null)));
        assert!(m(&p, "personCount", "is", json!(2)), "Ana Lima once, Bo; not the pet or the unnamed face");
        assert_eq!(field_group("person"), Some("Keywords & People"));
    }

    /// Shutter Speed compares exposure times in seconds, written as the camera shows them: a
    /// photo at 1/250 is in "≤ 1/60" (faster) and not in "≥ 1/60"; "between 1/1000 and 1/125".
    #[test]
    fn shutter_speed_rules() {
        let cat = Catalog::new();
        let mut p = photo(1);
        let m = |p: &Photo, op: &str, value: serde_json::Value| {
            rs(json!({"rules": [{"field": "shutterSpeed", "op": op, "value": value}]})).matches(p, &cat)
        };
        assert!(!m(&p, "gte", json!(0)), "no shutter speed recorded");
        p.meta.shutter = "1/250".into();
        assert!(m(&p, "is", json!("1/250")) && m(&p, "lte", json!("1/60")) && !m(&p, "gte", json!("1/60")));
        assert!(m(&p, "between", json!(["1/1000", "1/125"])) && m(&p, "lt", json!(0.01)));
        assert!(!m(&p, "is", json!("1/0")) && !m(&p, "is", json!("fast")), "a value that isn't a time matches nothing");
        p.meta.shutter = "2\"".into();
        assert!(m(&p, "gte", json!("1")) && m(&p, "is", json!(2)));
        assert_eq!(field_group("shutterSpeed"), Some("Camera Info"));
    }

    #[test]
    fn shutter_seconds_parses_camera_notation() {
        for (s, want) in [("1/250", Some(0.004)), (" 1/250 s", Some(0.004)), ("0.5", Some(0.5)), ("2\"", Some(2.0)), ("30s", Some(30.0))] {
            assert_eq!(crate::parse_shutter_seconds(s), want, "{s}");
        }
        for s in ["", "1/0", "0", "-1/250", "fast", "inf", "NaN", "1/x"] {
            assert_eq!(crate::parse_shutter_seconds(s), None, "{s}");
        }
    }

    /// File Extension ignores case and a leading dot; Copy Name, Alt Text, City, State /
    /// Province and Country are text fields of their own (Location still matches all of them).
    #[test]
    fn file_source_description_and_location_rules() {
        let cat = Catalog::new();
        let mut p = photo(1); // IMG_0042.CR2
        let m = |p: &Photo, field: &str, op: &str, value: serde_json::Value| {
            rs(json!({"rules": [{"field": field, "op": op, "value": value}]})).matches(p, &cat)
        };
        assert!(m(&p, "extension", "is", json!("cr2")) && m(&p, "extension", "is", json!(".CR2")));
        assert!(!m(&p, "extension", "is", json!("jpg")));
        p.file_name = "README".into();
        assert!(m(&p, "extension", "isEmpty", json!(null)), "no dot, no extension");
        p.file_name = "archive.tar.gz".into();
        assert!(m(&p, "extension", "is", json!("gz")));
        p.file_name = ".hidden".into();
        assert!(m(&p, "extension", "isEmpty", json!(null)), "a dot file has no extension");
        assert!(m(&p, "copyName", "isEmpty", json!(null)));
        p.copy_name = Some("Black and white".into());
        assert!(m(&p, "copyName", "contains", json!("black")));
        p.meta.alt_text = "A red kite over a hill".into();
        assert!(m(&p, "altText", "contains", json!("kite")));
        p.meta.city = "Porto".into();
        p.meta.state = "Porto District".into();
        p.meta.country = "Portugal".into();
        assert!(m(&p, "city", "is", json!("porto")) && m(&p, "state", "startsWith", json!("porto")) && m(&p, "country", "is", json!("portugal")));
        assert!(!m(&p, "city", "is", json!("portugal")), "City tests only the city");
        assert!(m(&p, "location", "contains", json!("portugal")), "Location still covers the country");
    }

    /// In a Stack, Video Duration (seconds; photos have none), Cropped and Treatment.
    #[test]
    fn source_file_and_develop_rules() {
        use crate::{Op, Stack, StackId};
        let mut cat = Catalog::new();
        let a = photo(1);
        let b = photo(2);
        let c = photo(3);
        for p in [&a, &b, &c] {
            cat.apply(Op::AddPhoto { photo: Box::new(p.clone()) }).unwrap();
        }
        cat.apply(Op::AddStack { stack: Stack { id: StackId(1), photos: vec![a.id, b.id], collapsed: false } }).unwrap();
        let m = |p: &Photo, field: &str, op: &str, value: serde_json::Value| {
            rs(json!({"rules": [{"field": field, "op": op, "value": value}]})).matches(p, &cat)
        };
        assert!(m(&a, "stacked", "is", json!(true)) && m(&b, "stacked", "is", json!(true)));
        assert!(m(&c, "stacked", "is", json!(false)));
        let mut v = photo(4);
        assert!(!m(&v, "duration", "gte", json!(0)), "a photo has no duration");
        v.duration = Some(95.0);
        assert!(m(&v, "duration", "gt", json!(60)) && m(&v, "duration", "between", json!([90, 120])));
        let mut d = photo(5);
        assert!(m(&d, "cropped", "is", json!(false)) && m(&d, "treatment", "is", json!("color")));
        let mut s = (*d.develop).clone();
        s.crop.geometry.rect = lightcraft_geom::Rect::new(0.1, 0.0, 0.9, 1.0);
        s.treatment = lightcraft_develop::Treatment::Bw;
        d.develop = std::sync::Arc::new(s);
        assert!(m(&d, "cropped", "is", json!(true)) && m(&d, "treatment", "is", json!("monochrome")));
        assert!(m(&d, "treatment", "isNot", json!("color")));
        let mut s = (*d.develop).clone();
        s.crop = Default::default();
        s.crop.geometry.angle = 2.0;
        d.develop = std::sync::Arc::new(s);
        assert!(m(&d, "cropped", "is", json!(true)), "straightening counts as a crop");
    }

    /// Size follows the photo as shown: rotated a quarter turn a landscape frame is a portrait, and
    /// a crop changes its edges. Long / Short Edge are in pixels.
    #[test]
    fn size_rules_follow_orientation_and_crop() {
        let cat = Catalog::new();
        let mut p = photo(1); // 6000 × 4000
        let m = |p: &Photo, field: &str, op: &str, value: serde_json::Value| {
            rs(json!({"rules": [{"field": field, "op": op, "value": value}]})).matches(p, &cat)
        };
        assert!(m(&p, "aspect", "is", json!("landscape")) && m(&p, "longEdge", "is", json!(6000)) && m(&p, "shortEdge", "is", json!(4000)));
        let mut s = (*p.develop).clone();
        s.orientation = lightcraft_geom::Orientation::Rotate90;
        p.develop = std::sync::Arc::new(s.clone());
        assert!(m(&p, "aspect", "is", json!("portrait")) && m(&p, "longEdge", "is", json!(6000)));
        // a square crop of the rotated frame: 4000 wide, 4000 of its 6000 tall
        s.crop.geometry.rect = lightcraft_geom::Rect::new(0.0, 1.0 / 6.0, 1.0, 5.0 / 6.0);
        p.develop = std::sync::Arc::new(s);
        assert!(m(&p, "aspect", "is", json!("square")) && m(&p, "longEdge", "is", json!(4000)));
        assert!(m(&p, "aspect", "isNot", json!("portrait")));
        let empty = Photo::new(PhotoId(9), Source::Demo { scene: 0 }, "x.jpg", "JPEG", 0, 0, "2026-09-20T10:00:00");
        assert!(
            !m(&empty, "aspect", "is", json!("landscape")) && !m(&empty, "aspect", "is", json!("square")) && m(&empty, "longEdge", "is", json!(0))
        );
    }

    /// Any Searchable Text also finds the state / province, the alt text, a person on a face and
    /// a virtual copy's name, as it already finds the city and country.
    #[test]
    fn any_searchable_text_covers_new_fields() {
        let cat = Catalog::new();
        let mut p = photo(1);
        let m = |p: &Photo, value: &str| rs(json!({"rules": [{"field": "text", "op": "contains", "value": value}]})).matches(p, &cat);
        for word in ["oregon", "kite", "ana", "bluish"] {
            assert!(!m(&p, word), "{word}");
        }
        p.meta.state = "Oregon".into();
        p.meta.alt_text = "A kite".into();
        p.meta.regions = vec![lightcraft_meta::Region {
            rect: lightcraft_meta::Rect { x0: 0.4, y0: 0.4, x1: 0.6, y1: 0.6 },
            kind: lightcraft_meta::RegionKind::Face,
            name: Some("Ana".into()),
            description: None,
        }];
        p.copy_name = Some("Bluish".into());
        for word in ["oregon", "kite", "ana", "bluish"] {
            assert!(m(&p, word), "{word}");
        }
    }

    /// The field menu shows every rule field once: at the top level or in exactly one group,
    /// and [`FIELDS`] lists them in menu order.
    #[test]
    fn every_field_is_in_the_menu_exactly_once() {
        let menu: Vec<&str> = TOP_LEVEL_FIELDS.iter().chain(FIELD_GROUPS.iter().flat_map(|g| g.1.iter())).copied().collect();
        let fields: Vec<&str> = FIELDS.iter().map(|f| f.0).collect();
        assert_eq!(menu, fields, "the menu and FIELDS list the same fields in the same order");
        for (label, group) in FIELD_GROUPS {
            assert!(!group.is_empty(), "group {label} is empty");
        }
        let mut labels: Vec<&str> = FIELD_GROUPS.iter().map(|g| g.0).collect();
        labels.dedup();
        assert_eq!(labels.len(), FIELD_GROUPS.len(), "group labels are unique");
    }

    /// Related fields sit together: all the file fields, all the camera (EXIF) fields, all the
    /// dates, all the keyword fields.
    #[test]
    fn related_fields_share_a_group() {
        let same = |fields: &[&str], group: &str| {
            for f in fields {
                assert_eq!(field_group(f), Some(group), "{f}");
            }
        };
        same(&["fileName", "extension", "filePath", "kind", "format", "duration"], "File");
        same(&["album", "virtualCopy", "copyName", "stacked"], "Source");
        same(&["longEdge", "shortEdge", "aspect", "megapixels"], "Size");
        same(&["edited", "cropped", "treatment"], "Develop");
        same(&["camera", "lens", "focalLength", "aperture", "shutterSpeed", "iso"], "Camera Info");
        same(&["captureDate", "importDate", "editDate"], "Date");
        same(&["keywords", "keywordCount", "person", "personCount"], "Keywords & People");
        same(&["title", "caption", "altText", "creator", "copyright", "copyrightStatus"], "Description");
        same(&["location", "city", "state", "country", "hasGps"], "Location");
        for f in ["rating", "flag", "label", "text"] {
            assert_eq!(field_group(f), None, "{f} stays at the top level");
        }
        assert_eq!(field_group("nope"), None);
        assert_eq!(field_label("filePath"), Some("File Path"));
        assert_eq!(field_label("nope"), None);
    }
}
