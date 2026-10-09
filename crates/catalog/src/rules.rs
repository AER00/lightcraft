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
    /// One of a fixed set: (id, label). Rules store the id; people see the label.
    Choice(&'static [(&'static str, &'static str)]),
    Bool,
}

/// The fields rules can test: (id, label, kind), in field-menu order ([`TOP_LEVEL_FIELDS`], then
/// each of [`FIELD_GROUPS`]).
pub const FIELDS: &[(&str, &str, Kind)] = &[
    ("rating", "Rating", Kind::Number),
    ("flag", "Pick Flag", Kind::Choice(&[("pick", "Picked"), ("reject", "Rejected"), ("none", "Unflagged")])),
    (
        "label",
        "Color Label",
        Kind::Choice(&[("red", "Red"), ("yellow", "Yellow"), ("green", "Green"), ("blue", "Blue"), ("purple", "Purple"), ("none", "No Label")]),
    ),
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
    ("kind", "File Type", Kind::Choice(&[("image", "Image"), ("raw", "Raw"), ("video", "Video")])),
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
    (
        "copyrightStatus",
        "Copyright Status",
        Kind::Choice(&[("copyrighted", "Copyrighted"), ("publicDomain", "Public Domain"), ("unknown", "Unknown")]),
    ),
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
    ("aspect", "Aspect Ratio", Kind::Choice(&[("landscape", "Landscape (wide)"), ("portrait", "Portrait (tall)"), ("square", "Square")])),
    ("megapixels", "Megapixels", Kind::Number),
    // Develop
    ("edited", "Has Edits", Kind::Bool),
    ("cropped", "Cropped", Kind::Bool),
    ("treatment", "Treatment", Kind::Choice(&[("color", "In Color"), ("monochrome", "Black & White")])),
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

/// A rule that can't mean anything ([`RuleSet::check`]): where it is and what is wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// Positions from the top rule list down: `[1, 0]` is the first rule of the second rule's group.
    pub path: Vec<usize>,
    pub message: String,
}

impl std::fmt::Display for Problem {
    /// `rule 2.1: rating is 0–5, not 9` (counting from 1, as people do).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let at: Vec<String> = self.path.iter().map(|i| (i.saturating_add(1)).to_string()).collect();
        write!(f, "rule {}: {}", at.join("."), self.message)
    }
}

/// What is wrong with one rule, if anything ([`RuleSet::check`]).
fn field_problem(field: &str, op: &str, value: &Value, cat: &Catalog) -> Option<String> {
    let Some(kind) = field_kind(field) else { return Some(format!("unknown field `{field}`")) };
    if !ops_for(kind).iter().any(|o| o.0 == op) {
        return Some(format!("`{field}` has no operator `{op}`"));
    }
    if matches!(op, "isEmpty" | "isNotEmpty") {
        return None;
    }
    match kind {
        Kind::Bool => bool_value(value).is_none().then(|| format!("`{field}` is yes or no, not {value}")),
        Kind::Number => {
            let each = match value {
                Value::Array(a) if op == "between" && a.len() == 2 => a.iter().find_map(|v| number_problem(field, v, cat)),
                _ if op == "between" => Some(format!("`{field}` between needs two values, not {value}")),
                v => number_problem(field, v, cat),
            };
            // a rating rule no rating could meet ("is 9", "≥ 7"); "< 6" is fine, if broad
            each.or_else(|| {
                let label = ops_for(kind).iter().find(|o| o.0 == op).map_or(op, |o| o.1);
                (field == "rating" && !(0..=5).any(|r| num_op(op, Some(f64::from(r)), value))).then(|| format!("no rating 0–5 {label} {value}"))
            })
        }
        Kind::Date => date_problem(field, op, value),
        Kind::Choice(choices) => choice_problem(field, value, choices, cat),
        Kind::Text | Kind::Keywords => match value {
            Value::String(s) if !s.trim().is_empty() => None,
            Value::String(_) | Value::Null => {
                let instead = if matches!(op, "isNot" | "notContains") { "isn't empty" } else { "is empty" };
                Some(format!("`{field}` needs something to look for (or use “{instead}”)"))
            }
            Value::Number(_) => None,
            other => Some(format!("`{field}` needs text, not {other}")),
        },
    }
}

fn number_problem(field: &str, v: &Value, cat: &Catalog) -> Option<String> {
    if field == "shutterSpeed" {
        return shutter_value(v).is_null().then(|| format!("`shutterSpeed` needs a time like 1/250 or 2, not {v}"));
    }
    let Some(n) = number(v).filter(|n| n.is_finite()) else { return Some(format!("`{field}` needs a number, not {v}")) };
    match field {
        "album" => {
            let album = (n >= 0.0 && n.fract() == 0.0).then(|| cat.album(crate::AlbumId(n as u64))).flatten();
            match album {
                None => Some(format!("no album {v}")),
                Some(a) if a.is_smart() => Some(format!("album {v} is a smart album; only plain albums can be tested")),
                Some(_) => None,
            }
        }
        _ => None,
    }
}

/// A date as rules take it: `2026`, `2026-08`, `2026-08-14` or a full ISO time, and a real one
/// (no month 13, no 30 February).
fn is_rule_date(s: &str) -> bool {
    let s = s.trim();
    let full = match s.len() {
        4 => format!("{s}-01-01"),
        7 => format!("{s}-01"),
        _ => s.to_string(),
    };
    s.len() >= 4 && crate::dates::normalize_iso(&full).is_some()
}

fn date_problem(field: &str, op: &str, value: &Value) -> Option<String> {
    let date = |v: &Value| match v.as_str() {
        Some(s) if is_rule_date(s) => None,
        _ => Some(format!("`{field}` needs a date like 2026, 2026-08 or 2026-08-14, not {v}")),
    };
    match op {
        "inLast" | "notInLast" => {
            let (n, unit) = match value {
                Value::Object(o) => (o.get("n").and_then(number), o.get("unit").cloned().unwrap_or(Value::Null)),
                v => (number(v), Value::Null),
            };
            if !n.is_some_and(|n| n.is_finite() && n > 0.0) {
                return Some(format!("`{field}` needs a number of hours, days, weeks, months or years more than 0"));
            }
            unit_secs(&unit).is_none().then(|| format!("`{field}`: unknown unit {unit} (hours, days, weeks, months or years)"))
        }
        "between" => match value {
            Value::Array(a) if a.len() == 2 => a.iter().find_map(date).or_else(|| {
                let bound = |i: usize| a.get(i).and_then(Value::as_str).unwrap_or("").trim();
                let (from, to) = (bound(0), bound(1));
                (from > to && !from.starts_with(to)).then(|| format!("`{field}`: the first date is after the second"))
            }),
            _ => Some(format!("`{field}` between needs two dates, not {value}")),
        },
        _ => date(value),
    }
}

fn choice_problem(field: &str, value: &Value, choices: &[(&str, &str)], cat: &Catalog) -> Option<String> {
    let known = value.as_str().is_some_and(|s| {
        choices.iter().any(|c| c.0.eq_ignore_ascii_case(s.trim()))
            || match field {
                "flag" => matches!(s.trim().to_lowercase().as_str(), "picked" | "rejected"),
                // a colour's name or the custom name given to it
                "label" => cat.label_from_name(s).is_some(),
                "copyrightStatus" => crate::CopyrightStatus::parse(s).is_some(),
                _ => false,
            }
    });
    let ids: Vec<&str> = choices.iter().map(|c| c.0).collect();
    (!known).then(|| format!("`{field}` is one of {}, not {value}", ids.join(", ")))
}

/// What a yes/no rule's value means: `true`, `"yes"`, `1` (or `1.0`) or no value is yes; `false`,
/// `"no"`, `0` is no (strings in any case); `None` for anything else, which matches nothing.
pub fn bool_value(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        Value::Null => Some(true),
        Value::Number(n) => match n.as_f64() {
            Some(1.0) => Some(true),
            Some(0.0) => Some(false),
            _ => None,
        },
        Value::String(s) => match s.trim().to_lowercase().as_str() {
            "true" | "yes" | "1" => Some(true),
            "false" | "no" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// The display label of a yes/no value.
pub fn bool_label(b: bool) -> &'static str {
    if b { "Yes" } else { "No" }
}

/// The display label of choice `id` of `field` ("Public Domain" for `publicDomain`); `None` when
/// `field` isn't a choice field or has no such choice.
pub fn choice_label(field: &str, id: &str) -> Option<&'static str> {
    let Some(Kind::Choice(choices)) = field_kind(field) else { return None };
    choices.iter().find(|c| c.0 == id).map(|c| c.1)
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

/// Seconds in one "in the last…" unit: hours, days, weeks, months or years, in any case, singular
/// or plural; none means days. `None` for anything else, which the check refuses and the matcher
/// matches nothing with.
fn unit_secs(unit: &Value) -> Option<f64> {
    let day = 86_400.0;
    let name = match unit {
        Value::Null => return Some(day),
        Value::String(s) => s.trim().to_lowercase(),
        _ => return None,
    };
    match name.strip_suffix('s').unwrap_or(&name) {
        "hour" => Some(3600.0),
        "day" => Some(day),
        "week" => Some(7.0 * day),
        "month" => Some(30.4375 * day),
        "year" => Some(365.25 * day),
        _ => None,
    }
}

/// The longest "in the last…" span: 10,000 years, beyond any capture date ("ever").
const MAX_LAST_SECS: f64 = 10_000.0 * 365.25 * 86_400.0;

/// `value` for in-the-last rules: `{n, unit: days|weeks|months|years}` or a number of days, in
/// seconds. `None` for a zero, negative or unreadable `n`; a longer span than [`MAX_LAST_SECS`]
/// is that (so the arithmetic on it can't overflow).
fn last_secs(value: &Value) -> Option<i64> {
    let (n, unit) = match value {
        Value::Object(o) => (o.get("n").and_then(number)?, o.get("unit").unwrap_or(&Value::Null)),
        v => (number(v)?, &Value::Null),
    };
    let per = unit_secs(unit)?;
    let secs = n * per;
    if secs.is_nan() || secs <= 0.0 {
        return None;
    }
    Some(secs.min(MAX_LAST_SECS) as i64)
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
            // a span reaching before year 0 is "ever": every dated photo is in it
            let Some(from) = crate::dates::shift_iso(&now(), -secs) else { return op == "inLast" };
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
        let yes = |have: bool| bool_value(value).is_some_and(|b| have == b);
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
            "edited" => yes(p.is_edited()),
            "hasGps" => yes(m.gps.is_some()),
            "virtualCopy" => yes(p.copy_of.is_some()),
            "copyName" => text(p.copy_name.as_deref().unwrap_or("")),
            "stacked" => yes(cat.stack_of(p.id).is_some()),
            "cropped" => yes(p.is_cropped()),
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
            "megapixels" => {
                let (w, h) = p.shown_size();
                num_op(op, Some(w * h / 1e6), value)
            }
            "sharpness" => num_op(op, p.analysis.map(|a| a.sharpness as f64), value),
            "bestOfGroup" => yes(p.analysis.is_some_and(|a| a.best || a.group.is_none())),
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

    /// The rules that can't mean anything, in order: an unknown field, an operator the field
    /// doesn't have, a value that isn't one of the field's (a number, a date, a choice, yes or
    /// no, a plain album…), text with nothing to look for, an empty group. Commands refuse a rule
    /// set with problems; the editor shows them. `cat` knows the albums and colour-label names.
    pub fn check(&self, cat: &Catalog) -> Vec<Problem> {
        let mut out = Vec::new();
        self.check_into(cat, &mut Vec::new(), &mut out);
        out
    }

    fn check_into(&self, cat: &Catalog, path: &mut Vec<usize>, out: &mut Vec<Problem>) {
        for (i, rule) in self.rules.iter().enumerate() {
            path.push(i);
            match rule {
                Rule::Group { group } if group.rules.is_empty() => out.push(Problem { path: path.clone(), message: "an empty group".into() }),
                Rule::Group { group } => group.check_into(cat, path, out),
                Rule::Field { field, op, value } => {
                    if let Some(message) = field_problem(field, op, value, cat) {
                        out.push(Problem { path: path.clone(), message });
                    }
                }
            }
            path.pop();
        }
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
                        _ if field_kind(field) == Some(Kind::Bool) => {
                            bool_value(value).map_or_else(|| value.to_string(), |b| bool_label(b).to_lowercase())
                        }
                        Value::String(s) => choice_label(field, s).map_or_else(|| s.clone(), str::to_lowercase),
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
        let messages: Vec<String> = r.check(&Catalog::new()).into_iter().map(|p| p.message).collect();
        assert_eq!(messages, vec!["`rating` has no operator `contains`".to_string(), "unknown field `nope`".to_string()]);
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
    /// a crop changes its edges and its megapixels. Long / Short Edge are in pixels.
    #[test]
    fn size_rules_follow_orientation_and_crop() {
        let cat = Catalog::new();
        let mut p = photo(1); // 6000 × 4000
        let m = |p: &Photo, field: &str, op: &str, value: serde_json::Value| {
            rs(json!({"rules": [{"field": field, "op": op, "value": value}]})).matches(p, &cat)
        };
        assert!(m(&p, "aspect", "is", json!("landscape")) && m(&p, "longEdge", "is", json!(6000)) && m(&p, "shortEdge", "is", json!(4000)));
        assert!(m(&p, "megapixels", "is", json!(24)));
        let mut s = (*p.develop).clone();
        s.orientation = lightcraft_geom::Orientation::Rotate90;
        p.develop = std::sync::Arc::new(s.clone());
        assert!(m(&p, "aspect", "is", json!("portrait")) && m(&p, "longEdge", "is", json!(6000)));
        // a square crop of the rotated frame: 4000 wide, 4000 of its 6000 tall
        s.crop.geometry.rect = lightcraft_geom::Rect::new(0.0, 1.0 / 6.0, 1.0, 5.0 / 6.0);
        p.develop = std::sync::Arc::new(s);
        assert!(m(&p, "aspect", "is", json!("square")) && m(&p, "longEdge", "is", json!(4000)));
        assert!(m(&p, "megapixels", "is", json!(16)), "a 4000 × 4000 crop is 16 MP, not the sensor's 24");
        assert!(m(&p, "megapixels", "lt", json!(20)) && !m(&p, "megapixels", "gte", json!(24)));
        assert!(m(&p, "aspect", "isNot", json!("portrait")));
        let empty = Photo::new(PhotoId(9), Source::Demo { scene: 0 }, "x.jpg", "JPEG", 0, 0, "2026-09-20T10:00:00");
        assert!(
            !m(&empty, "aspect", "is", json!("landscape")) && !m(&empty, "aspect", "is", json!("square")) && m(&empty, "longEdge", "is", json!(0))
        );
        assert!(m(&empty, "megapixels", "is", json!(0)));
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

    /// Choice values have readable labels: the editor and summaries show "Public Domain" and
    /// "Black & White", never ids like `publicDomain`; rules still store and accept the ids.
    #[test]
    fn choices_have_readable_labels() {
        assert_eq!(choice_label("copyrightStatus", "publicDomain"), Some("Public Domain"));
        assert_eq!(choice_label("treatment", "monochrome"), Some("Black & White"));
        // own labels where a shared word would translate wrongly: "Color" is the colour noun (the
        // Color panel) and "None" agrees with other nouns in gendered languages
        assert_eq!(choice_label("treatment", "color"), Some("In Color"));
        assert_eq!(choice_label("label", "none"), Some("No Label"));
        assert_eq!(choice_label("flag", "pick"), Some("Picked"));
        assert_eq!(choice_label("aspect", "landscape"), Some("Landscape (wide)"));
        assert_eq!(choice_label("flag", "nope"), None);
        assert_eq!(choice_label("rating", "pick"), None, "not a choice field");
        for (field, _, kind) in FIELDS {
            let Kind::Choice(choices) = kind else { continue };
            for (id, label) in *choices {
                assert!(!label.is_empty() && label != id, "{field}: {id} has no label of its own");
                assert!(label.starts_with(|c: char| c.is_uppercase()), "{field}: {label}");
            }
        }
        let r = rs(
            json!({"rules": [{"field": "copyrightStatus", "op": "is", "value": "publicDomain"}, {"field": "flag", "op": "isNot", "value": "reject"}]}),
        );
        assert_eq!(r.describe(), "copyright status is public domain and pick flag isn't rejected");
        let mut p = photo(1);
        p.meta.copyright_status = crate::CopyrightStatus::PublicDomain;
        assert!(r.matches(&p, &Catalog::new()), "ids still match");
    }

    /// Yes/no fields read "Yes" / "No", and a value means the same to matching and to every
    /// summary: `false`, `"false"`, `"no"` and `0` are no; `true`, `"yes"`, `1` and a missing value
    /// are yes. Anything else is reported, not silently taken as yes.
    #[test]
    fn yes_no_values() {
        for v in [json!(true), json!("true"), json!("Yes"), json!(1), json!(1.0), json!(null)] {
            assert_eq!(bool_value(&v), Some(true), "{v}");
        }
        for v in [json!(false), json!("false"), json!(" NO "), json!(0), json!(0.0)] {
            assert_eq!(bool_value(&v), Some(false), "{v}");
        }
        for v in [json!("maybe"), json!(2), json!(0.5), json!(-1), json!([true]), json!({"x": 1})] {
            assert_eq!(bool_value(&v), None, "{v}");
        }
        assert_eq!((bool_label(true), bool_label(false)), ("Yes", "No"));
        let cat = Catalog::new();
        let p = photo(1); // unedited
        assert!(rs(json!({"rules": [{"field": "edited", "op": "is", "value": "false"}]})).matches(&p, &cat), "\"false\" means no");
        assert!(
            !rs(json!({"rules": [{"field": "edited", "op": "is", "value": "false"}, {"field": "hasGps", "op": "is", "value": "yes"}]}))
                .matches(&p, &cat)
        );
        assert!(!rs(json!({"rules": [{"field": "edited", "op": "is", "value": "maybe"}]})).matches(&p, &cat), "an unreadable value matches nothing");
        let r = rs(json!({"rules": [{"field": "edited", "op": "is", "value": true}, {"field": "cropped", "op": "is", "value": "false"}]}));
        assert_eq!(r.describe(), "has edits is yes and cropped is no");
        let bad = rs(json!({"rules": [{"field": "edited", "op": "is", "value": "maybe"}]}));
        let messages: Vec<String> = bad.check(&cat).into_iter().map(|p| p.message).collect();
        assert_eq!(messages, vec!["`edited` is yes or no, not \"maybe\"".to_string()]);
    }

    /// "In the last N" never overflows, whatever N an agent or a saved file holds: a zero,
    /// negative or unreadable N matches nothing (either way round); an N beyond 10,000 years means
    /// "ever", so every dated photo is in it and none is outside it.
    #[test]
    fn in_the_last_survives_hostile_counts() {
        set_now(Some("2026-09-01T00:00:00".into()));
        let cat = Catalog::new();
        let p = photo(1); // captured 2026-08-14
        let m = |op: &str, n: serde_json::Value, unit: &str| {
            rs(json!({"rules": [{"field": "captureDate", "op": op, "value": {"n": n, "unit": unit}}]})).matches(&p, &cat)
        };
        for n in [json!(0), json!(-5), json!(-1e30), json!(-9.3e18), json!("soon")] {
            for op in ["inLast", "notInLast"] {
                assert!(!m(op, n.clone(), "years"), "{op} {n}");
            }
        }
        for n in [json!(1e30), json!(9.3e18), json!(u64::MAX), json!(20_000)] {
            assert!(m("inLast", n.clone(), "years"), "{n} years is ever");
            assert!(!m("notInLast", n.clone(), "years"), "{n}");
        }
        assert!(m("inLast", json!(1e30), "hours") && m("inLast", json!(30), "days"));
        set_now(None);
    }

    /// Every rule's value is checked against its field, so a rule that can't mean anything is
    /// reported instead of quietly matching nothing (or everything). Each problem says which rule
    /// it is about: "rule 2", or "rule 2.1" inside a group.
    #[test]
    fn values_are_checked_against_their_field() {
        use crate::{Album, AlbumId, Op};
        let mut cat = Catalog::new();
        cat.apply(Op::AddAlbum { album: Album::new(AlbumId(1), "Trip") }).unwrap();
        let smart = Album { smart: Some(Box::default()), ..Album::new(AlbumId(2), "Best") };
        cat.apply(Op::AddAlbum { album: smart }).unwrap();
        cat.apply(Op::SetLabelName { label: crate::ColorLabel::Red, name: Some("Client".into()) }).unwrap();
        let check = |rule: serde_json::Value| rs(json!({"rules": [rule]})).check(&cat);
        let ok = |rule: serde_json::Value| assert!(check(rule.clone()).is_empty(), "{rule}: {:?}", check(rule.clone()));
        let bad = |rule: serde_json::Value, want: &str| {
            let p = check(rule.clone());
            assert!(p.iter().any(|p| p.message.contains(want)), "{rule}: wanted {want:?}, got {p:?}");
        };
        let r = |field: &str, op: &str, value: serde_json::Value| json!({"field": field, "op": op, "value": value});
        // numbers
        ok(r("rating", "gte", json!(3)));
        ok(r("rating", "gte", json!("4")));
        ok(r("rating", "between", json!([1, 5])));
        ok(r("aperture", "lte", json!("f/4")));
        ok(r("focalLength", "is", json!("50mm")));
        bad(r("rating", "gte", json!("abc")), "needs a number");
        bad(r("rating", "gte", json!(7)), "no rating 0–5 is ≥ 7");
        bad(r("iso", "between", json!([100])), "two");
        bad(r("iso", "between", json!("100")), "two");
        ok(r("shutterSpeed", "lte", json!("1/60")));
        bad(r("shutterSpeed", "lte", json!("fast")), "1/250");
        // dates
        for d in ["2026", "2026-08", "2026-08-14", "2026-08-14T10:00:00"] {
            ok(r("captureDate", "is", json!(d)));
        }
        ok(r("captureDate", "between", json!(["2026-01", "2026-12"])));
        for d in [json!("banana"), json!("2026-13"), json!("2026-02-30"), json!(""), json!(2026)] {
            bad(r("captureDate", "after", d), "needs a date");
        }
        bad(r("captureDate", "between", json!(["2026-12", "2026-01"])), "after the second");
        bad(r("captureDate", "between", json!(["", "2026"])), "needs a date");
        ok(r("captureDate", "inLast", json!({"n": 2, "unit": "weeks"})));
        ok(r("captureDate", "inLast", json!(7)));
        bad(r("captureDate", "inLast", json!({"n": 0, "unit": "days"})), "more than 0");
        bad(r("captureDate", "notInLast", json!({"n": 3, "unit": "fortnights"})), "fortnights");
        ok(r("captureDate", "isEmpty", json!(null)));
        // choices: the ids, the flag's aliases and a colour label's custom name
        ok(r("kind", "is", json!("raw")));
        ok(r("flag", "is", json!("picked")));
        ok(r("label", "is", json!("client")));
        bad(r("label", "is", json!("pink")), "pink");
        bad(r("kind", "isNot", json!("photo")), "photo");
        // text needs something to look for
        ok(r("title", "contains", json!("kite")));
        ok(r("title", "isEmpty", json!(null)));
        bad(r("title", "contains", json!("  ")), "something to look for");
        bad(r("keywords", "startsWith", json!("")), "something to look for");
        // albums: a plain album that exists
        ok(r("album", "is", json!(1)));
        bad(r("album", "is", json!(999)), "no album 999");
        bad(r("album", "is", json!(2)), "smart album");
        // groups: an empty one is a mistake; problems inside one say where they are
        let p = rs(json!({"rules": [r("rating", "gte", json!(3)), {"group": {"match": "any", "rules": []}}]})).check(&cat);
        assert_eq!(p.iter().map(ToString::to_string).collect::<Vec<_>>(), vec!["rule 2: an empty group".to_string()]);
        let p = rs(json!({"rules": [r("rating", "gte", json!(3)), {"group": {"rules": [r("iso", "is", json!(100)), r("rating", "is", json!(9))]}}]}))
            .check(&cat);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].path, vec![1, 1]);
        assert_eq!(p[0].to_string(), "rule 2.2: no rating 0–5 is 9");
        // an empty rule list at the top is "all photos" (or none, for any), not a mistake
        assert!(rs(json!({"rules": []})).check(&cat).is_empty());
    }

    /// The check and the matcher agree. Units are any case, singular or plural, and missing means
    /// days; an unknown unit is refused and matches nothing (it used to count as days, so "week" was
    /// one day). A rating rule is refused only when no rating 0–5 could meet it. A colour label's
    /// custom name matches in any case, accents included. Empty text hints at the right operator.
    #[test]
    fn check_and_matcher_agree() {
        use crate::Op;
        set_now(Some("2026-09-01T00:00:00".into()));
        let mut cat = Catalog::new();
        cat.apply(Op::SetLabelName { label: crate::ColorLabel::Red, name: Some("Été".into()) }).unwrap();
        let p = photo(1); // captured 2026-08-14 (18 days before now), red label, rating 4
        let rule = |field: &str, op: &str, value: serde_json::Value| rs(json!({"rules": [{"field": field, "op": op, "value": value}]}));
        let valid = |r: &RuleSet| r.check(&cat).is_empty();
        for unit in [json!("Days"), json!("DAY"), json!("days"), json!(null)] {
            let r = rule("captureDate", "inLast", json!({"n": 30, "unit": unit}));
            assert!(valid(&r) && r.matches(&p, &cat), "{unit}");
        }
        let r = rule("captureDate", "inLast", json!({"n": 30}));
        assert!(valid(&r) && r.matches(&p, &cat), "no unit is days");
        let week = rule("captureDate", "inLast", json!({"n": 1, "unit": "week"}));
        assert!(valid(&week) && !week.matches(&p, &cat), "a week, not a day: 18 days ago is outside it");
        assert!(rule("captureDate", "inLast", json!({"n": 3, "unit": "Weeks"})).matches(&p, &cat));
        let odd = rule("captureDate", "inLast", json!({"n": 30, "unit": "fortnights"}));
        assert!(!valid(&odd) && !odd.matches(&p, &cat), "an unknown unit is refused and matches nothing");
        // rating: only a rule no rating could meet
        for (op, v) in [("lt", json!(6)), ("gt", json!(-1)), ("isNot", json!(9)), ("between", json!([3, 8]))] {
            assert!(valid(&rule("rating", op, v.clone())), "{op} {v}");
        }
        for (op, v) in [("is", json!(9)), ("gte", json!(7)), ("gt", json!(5)), ("lt", json!(0)), ("between", json!([6, 9]))] {
            assert!(!valid(&rule("rating", op, v.clone())), "{op} {v}");
        }
        // a custom label name, any case, accents too
        let r = rule("label", "is", json!("été"));
        assert!(valid(&r) && r.matches(&p, &cat));
        // "isn't" with nothing points at "isn't empty", the others at "is empty"
        let hint = |op: &str| rule("title", op, json!("")).check(&cat).first().map(|p| p.message.clone()).unwrap_or_default();
        assert!(hint("isNot").contains("isn't empty"), "{}", hint("isNot"));
        assert!(hint("contains").contains("“is empty”"), "{}", hint("contains"));
        set_now(None);
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
