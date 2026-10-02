//! XMP sidecars end to end: save → delete library → re-import restores edits; auto-write; foreign
//! `crs:` sidecars; DNG-embedded XMP; read metadata from file (undoable).

use std::path::{Path, PathBuf};

use lightcraft_catalog::{ColorLabel, Flag, PhotoId};
use serde_json::json;

use crate::Session;

pub(crate) fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("lc-xmp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write_png(path: &Path, seed: u8) {
    let (w, h) = (40usize, 24usize);
    let data: Vec<[u8; 4]> = (0..w * h).map(|i| [(i % w * 6) as u8, (i / w * 9) as u8, seed, 255]).collect();
    let img = lightcraft_raster::Rgba8 { width: w, height: h, data };
    let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&img), &lightcraft_codecs::EncodeMeta::default()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn only(s: &Session) -> PhotoId {
    s.catalog.photos().next().unwrap().id
}

#[test]
fn save_delete_library_reimport_restores_edits() {
    let src = temp_dir("roundtrip-src");
    let lib = temp_dir("roundtrip-lib");
    write_png(&src.join("dune.png"), 1);
    write_png(&src.join("plain.png"), 2);
    let mut s = Session::new().with_fs();
    s.open_library(&lib, false).unwrap();
    s.execute("library.import", &json!({"paths": [src.join("dune.png").to_string_lossy()]})).unwrap();
    let id = only(&s);
    s.execute("library.select", &json!({"ids": [id.0]})).unwrap();
    s.execute("photo.rate", &json!({"rating": 4})).unwrap();
    s.execute("photo.pick", &json!({})).unwrap();
    s.execute("photo.label", &json!({"label": "blue"})).unwrap();
    s.execute("photo.setMeta", &json!({"title": "Dune", "caption": "Evening light", "copyright": "© me", "keywords": ["sand", "dusk"]})).unwrap();
    s.execute("develop.set", &json!({"control": "light.exposure", "value": 0.8})).unwrap();
    s.execute("develop.set", &json!({"control": "effects.clarity", "value": 25})).unwrap();
    s.execute("mask.add", &json!({"kind": "radial"})).unwrap();
    let edited = s.catalog.photo(id).unwrap().clone();
    let r = s.execute("photo.saveMetadataToFile", &json!({})).unwrap();
    assert_eq!(r["written"][0], src.join("dune.xmp").to_string_lossy().as_ref(), "{r}");
    assert!(src.join("dune.xmp").is_file());
    assert!(!src.join("plain.xmp").exists());
    drop(s);

    // delete the library; a fresh library re-imports the folder
    std::fs::remove_dir_all(&lib).unwrap();
    let mut s = Session::new().with_fs();
    s.open_library(&lib, false).unwrap();
    let r = s.execute("library.import", &json!({"paths": [src.to_string_lossy()]})).unwrap();
    assert_eq!(r["imported"].as_array().unwrap().len(), 2, "{r}");
    assert_eq!(r["sidecars"], 1, "{r}");
    let p = s.catalog.photos().find(|p| p.file_name == "dune.png").unwrap().clone();
    assert_eq!(p.develop, edited.develop, "develop settings restored exactly");
    assert_eq!(p.develop.masks.len(), 1);
    assert_eq!((p.rating, p.flag, p.label), (4, Flag::Pick, Some(ColorLabel::Blue)));
    assert_eq!(p.meta.title, "Dune");
    assert_eq!(p.meta.caption, "Evening light");
    assert_eq!(p.meta.copyright, "© me");
    assert_eq!(p.meta.keywords, vec!["sand".to_string(), "dusk".to_string()]);
    assert!(p.edited.is_some());
    let plain = s.catalog.photos().find(|p| p.file_name == "plain.png").unwrap();
    assert!(plain.develop.is_unedited() && plain.rating == 0);
    let _ = std::fs::remove_dir_all(&src);
    let _ = std::fs::remove_dir_all(&lib);
}

#[test]
fn auto_write_and_naming_preference() {
    let src = temp_dir("auto");
    let lib = temp_dir("auto-lib");
    write_png(&src.join("a.png"), 3);
    let mut s = Session::new().with_fs();
    s.open_library(&lib, false).unwrap();
    s.execute("library.import", &json!({"paths": [src.to_string_lossy()]})).unwrap();
    s.execute("photo.rate", &json!({"rating": 2})).unwrap();
    assert!(!src.join("a.xmp").exists(), "auto-write is off by default");

    let v = s.execute("library.xmpPreferences", &json!({"autoWrite": true, "naming": "full"})).unwrap();
    assert_eq!(v, json!({"autoWrite": true, "naming": "full"}));
    s.execute("photo.rate", &json!({"rating": 5})).unwrap();
    let sidecar = src.join("a.png.xmp");
    assert!(sidecar.is_file());
    assert!(std::fs::read_to_string(&sidecar).unwrap().contains("<xmp:Rating>5</xmp:Rating>"));
    // a slider drag writes once, at the end
    s.execute("develop.beginInteraction", &json!({"label": "Exposure"})).unwrap();
    s.execute("develop.set", &json!({"control": "light.exposure", "value": 1.5})).unwrap();
    let exposure = |p: &Path| match crate::sidecar::parse_sidecar(&std::fs::read_to_string(p).unwrap(), false).unwrap().develop {
        Some(crate::sidecar::DevelopPatch::Full(d)) => d.light.exposure,
        other => panic!("{other:?}"),
    };
    assert_eq!(exposure(&sidecar), 0.0, "not written mid-drag");
    s.execute("develop.endInteraction", &json!({})).unwrap();
    assert_eq!(exposure(&sidecar), 1.5);
    // undo rewrites too
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(exposure(&sidecar), 0.0);

    // the preference persists with the library
    drop(s);
    let mut s = Session::new().with_fs();
    s.open_library(&lib, false).unwrap();
    assert!(s.xmp.auto_write);
    assert_eq!(s.xmp.naming, crate::sidecar::SidecarNaming::Full);
    let v = s.execute("library.toggleAutoWriteXmp", &json!({})).unwrap();
    assert_eq!(v["autoWrite"], false);
    let _ = std::fs::remove_dir_all(&src);
    let _ = std::fs::remove_dir_all(&lib);
}

/// Hand-written sidecar in the style other raw developers use (attribute form, `crs:` fields).
const FOREIGN: &str = r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
    xmp:Rating="3" xmp:Label="Red"
    crs:ProcessVersion="11.0" crs:IncrementalTemperature="+20" crs:IncrementalTint="-5"
    crs:Exposure2012="+1.10" crs:Shadows2012="+35" crs:Vibrance="+15" crs:Clarity2012="+10"
    crs:SplitToningShadowHue="220" crs:SplitToningShadowSaturation="18"
    crs:HasCrop="True" crs:CropLeft="0.1" crs:CropTop="0" crs:CropRight="0.9" crs:CropBottom="1" crs:CropAngle="0">
   <dc:subject><rdf:Bag><rdf:li>harbour</rdf:li></rdf:Bag></dc:subject>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#;

#[test]
fn foreign_crs_sidecar_on_import_and_read_from_file() {
    let src = temp_dir("foreign");
    write_png(&src.join("harbour.png"), 4);
    std::fs::write(src.join("harbour.xmp"), FOREIGN).unwrap();
    let mut s = Session::new().with_fs();
    let r = s.execute("library.import", &json!({"paths": [src.to_string_lossy()]})).unwrap();
    assert_eq!(r["sidecars"], 1);
    let p = s.catalog.photos().next().unwrap().clone();
    assert_eq!((p.rating, p.label), (3, Some(ColorLabel::Red)));
    assert_eq!(p.meta.keywords, vec!["harbour".to_string()]);
    let d = &p.develop;
    assert_eq!((d.light.exposure, d.light.shadows, d.color.vibrance, d.effects.clarity), (1.1, 35.0, 15.0, 10.0));
    assert!(d.wb.temp > 6500.0 && d.wb.tint == -5.0, "relative WB for a rendered file: {:?}", d.wb);
    assert_eq!((d.grading.shadows.hue, d.grading.shadows.sat), (220.0, 18.0));
    assert_eq!((d.crop.geometry.rect.x0, d.crop.geometry.rect.x1), (0.1, 0.9));
    assert!(s.render_now(p.id, 32, 32).is_ok());

    // change things, then read the sidecar again: one undo step restores the edits
    s.execute("library.select", &json!({"ids": [p.id.0]})).unwrap();
    s.execute("develop.reset", &json!({})).unwrap();
    s.execute("photo.rate", &json!({"rating": 1})).unwrap();
    let r = s.execute("photo.readMetadataFromFile", &json!({})).unwrap();
    assert_eq!(r["read"].as_array().unwrap().len(), 1, "{r}");
    let q = s.catalog.photo(p.id).unwrap();
    assert_eq!(q.rating, 3);
    assert_eq!(q.develop.light.exposure, 1.1);
    s.execute("edit.undo", &json!({})).unwrap();
    let q = s.catalog.photo(p.id).unwrap();
    assert_eq!((q.rating, q.develop.light.exposure), (1, 0.0));
    // no sidecar → reported, not an error
    std::fs::remove_file(src.join("harbour.xmp")).unwrap();
    let r = s.execute("photo.readMetadataFromFile", &json!({})).unwrap();
    assert_eq!(r["failed"].as_array().unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(&src);
}

fn synthetic_dng(xmp: &str) -> Vec<u8> {
    synthetic_dng_with(Some(xmp), lightcraft_meta::Metadata::default())
}

/// A tiny Bayer DNG written by our own DNG writer (optional embedded XMP, camera metadata).
pub(crate) fn synthetic_dng_with(xmp: Option<&str>, metadata: lightcraft_meta::Metadata) -> Vec<u8> {
    use lightcraft_raw::*;
    let (w, h) = (32usize, 24usize);
    let cfa = Cfa::bayer("RGGB").unwrap();
    let data: Vec<u16> = (0..w * h).map(|i| 256 + ((i % w) * 300 + (i / w) * 200) as u16).collect();
    let raw = RawImage {
        format: RawFormat::Dng,
        width: w,
        height: h,
        cpp: 1,
        data: RawData::U16(data),
        cfa: Some(cfa),
        bits: 14,
        black: BlackLevel::uniform(256.0),
        white: vec![16000.0],
        active_area: Rect::new(0, 0, w, h),
        crop: Rect::new(0, 0, w, h),
        orientation: Orientation::Normal,
        color: ColorData {
            illuminant: [17, 21],
            color_matrix: [
                Some(lightcraft_color::Mat3([[0.9, 0.2, -0.15], [-0.3, 1.25, 0.08], [0.02, -0.12, 0.85]])),
                Some(lightcraft_color::Mat3([[0.7, 0.3, -0.1], [-0.35, 1.3, 0.1], [0.05, -0.2, 1.0]])),
            ],
            as_shot_neutral: Some([0.5, 1.0, 0.7]),
            ..Default::default()
        },
        wb_multipliers: None,
        linearized: false,
        opcodes: OpcodeLists::default(),
        metadata,
    };
    write_dng(&raw, &DngWriteOptions { xmp: xmp.map(str::to_string), ..Default::default() }).unwrap()
}

#[test]
fn dng_embedded_crs_settings_are_read_on_import() {
    let src = temp_dir("dng");
    let x = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
        xmp:Rating="2" crs:WhiteBalance="Custom" crs:Temperature="4300" crs:Tint="+7" crs:Contrast2012="+30" crs:Dehaze="+12"/>
      </rdf:RDF></x:xmpmeta>"#;
    std::fs::write(src.join("synth.dng"), synthetic_dng(x)).unwrap();
    let mut s = Session::new().with_fs();
    let r = s.execute("library.import", &json!({"paths": [src.to_string_lossy()]})).unwrap();
    assert_eq!((r["imported"].as_array().unwrap().len(), r["sidecars"].as_u64()), (1, Some(1)), "{r}");
    let p = s.catalog.photos().next().unwrap();
    assert_eq!(p.kind, lightcraft_catalog::MediaKind::Raw);
    assert_eq!((p.develop.wb.temp, p.develop.wb.tint), (4300.0, 7.0));
    assert_eq!((p.develop.light.contrast, p.develop.effects.dehaze), (30.0, 12.0));
    assert_eq!(p.develop.detail.sharpen_amount, 40.0, "raw defaults kept for fields the XMP doesn't set");
    assert_eq!(p.rating, 2);

    // a sidecar next to the DNG takes precedence over the embedded XMP
    let id = p.id;
    s.execute("library.select", &json!({"ids": [id.0]})).unwrap();
    s.execute("develop.set", &json!({"control": "light.contrast", "value": -10})).unwrap();
    s.execute("photo.saveMetadataToFile", &json!({})).unwrap();
    let mut s2 = Session::new().with_fs();
    s2.execute("library.import", &json!({"paths": [src.to_string_lossy()]})).unwrap();
    assert_eq!(s2.catalog.photos().next().unwrap().develop.light.contrast, -10.0);
    let _ = std::fs::remove_dir_all(&src);
}

#[test]
fn demo_photos_have_no_sidecar() {
    let mut s = Session::with_demo();
    let r = s.execute("photo.saveMetadataToFile", &json!({})).unwrap();
    assert_eq!(r["written"].as_array().unwrap().len(), 0);
    assert_eq!(r["failed"].as_array().unwrap().len(), 1);
}
