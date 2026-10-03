//! Corpus test over `corpus/raw/**` (git-ignored CC0 samples from raw.pixls.us, fetched with
//! `cargo xtask corpus --download`; `LIGHTCRAFT_CORPUS` overrides the corpus root). Skips cleanly when absent.
//!
//! Every file must be recognised, carry an embedded JPEG preview (DNG: optional), and either decode to a valid image or report
//! `Unsupported` for one of the variants we know we don't decode yet. Prints decode times
//! (`cargo test -p lightcraft-raw --release --test corpus -- --nocapture`).

use lightcraft_raw::{RawError, RawFormat, decode, embedded_preview, probe, probe_info};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn corpus_root() -> PathBuf {
    std::env::var_os("LIGHTCRAFT_CORPUS").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus"))
}

/// Variants known not to decode yet (see the crate docs): matched against the lower-case file name.
const KNOWN_UNSUPPORTED: &[&str] = &[
    "cr3-",                     // CR3 / CRX (M11.1)
    "raf-fuji-xt20-compressed", // Fujifilm compressed RAF
    "rw2-panasonic-gh5.",       // Panasonic raw format 4 (quantised)
    "rw2-panasonic-gx80",       // "
    "rw2-panasonic-g9-b",       // "
    "orf-olympus-em",           // Olympus compressed ORF
    "nef-nikon-d5100-lossless", // Nikon Huffman NEF
    "nef-nikon-d7000-lossy",    // "
    "sraw",                     // Canon sRAW / mRAW
];

#[test]
fn corpus_raw_decodes() {
    let dir = corpus_root().join("raw");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        eprintln!("skip: {} absent", dir.display());
        return;
    };
    let mut paths: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
    paths.sort();
    let (mut ok, mut unsupported) = (0, 0);
    for p in paths {
        let name = p.file_name().unwrap().to_string_lossy().to_lowercase();
        if name.ends_with(".part") || name.ends_with(".txt") || name.ends_with(".md") {
            continue;
        }
        let bytes = std::fs::read(&p).unwrap();
        let fmt = probe(&bytes).unwrap_or_else(|| panic!("{name}: not recognised"));
        let t0 = Instant::now();
        let preview = embedded_preview(&bytes);
        let tp = t0.elapsed().as_secs_f64() * 1e3;
        // DNG previews are optional (and some carry only an uncompressed RGB thumbnail); vendor raws always embed a JPEG
        if let Some(p) = &preview {
            assert!(p.starts_with(&[0xff, 0xd8]) && p.ends_with(&[0xff, 0xd9]), "{name}: preview is not a JPEG");
        } else {
            assert_eq!(fmt, RawFormat::Dng, "{name}: no embedded preview");
        }
        let preview_kb = preview.as_ref().map_or(0, |p| p.len() / 1024);
        let t1 = Instant::now();
        let decoded = decode(&bytes);
        let dt = t1.elapsed().as_secs_f64() * 1e3;
        // the header-only probe agrees with the full decode, faster
        let t2 = Instant::now();
        let info = probe_info(&bytes);
        let di = t2.elapsed().as_secs_f64() * 1e3;
        match (&decoded, &info) {
            (Ok(img), Ok(info)) => assert_eq!(&img.info(), info, "{name}: probe_info differs from decode"),
            (Err(RawError::Unsupported(_)), Err(RawError::Unsupported(_))) => {}
            (d, i) => panic!("{name}: decode {:?} but probe_info {:?}", d.as_ref().err(), i.as_ref().err()),
        }
        eprintln!("{name:44} probe_info {di:.1} ms");
        match decoded {
            Ok(img) => {
                img.validate().unwrap();
                assert!(img.white_at(0) > img.black.mean(), "{name}: white {} <= black {}", img.white_at(0), img.black.mean());
                let mp = (img.width * img.height) as f64 / 1e6;
                eprintln!(
                    "{name:44} {fmt:?} {}x{} {}-bit {:?}: decode {dt:.0} ms ({:.0} MP/s), preview {} KB in {tp:.1} ms",
                    img.width,
                    img.height,
                    img.bits,
                    img.cfa.as_ref().map(|c| c.name()),
                    mp / (dt / 1e3),
                    preview_kb
                );
                ok += 1;
            }
            Err(RawError::Unsupported(why)) => {
                assert!(KNOWN_UNSUPPORTED.iter().any(|k| name.contains(k)), "{name}: unexpectedly unsupported: {why}");
                eprintln!("{name:44} {fmt:?} unsupported ({why}); preview {preview_kb} KB in {tp:.1} ms");
                unsupported += 1;
            }
            Err(e) => panic!("{name}: {e}"),
        }
    }
    eprintln!("corpus/raw: {ok} decoded, {unsupported} known-unsupported (preview only)");
}
