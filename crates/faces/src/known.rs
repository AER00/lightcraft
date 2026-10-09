//! The models LightCraft recognises by their SHA-256, with what is honestly known about each.
//!
//! The licence and provenance texts here are what the user reads before enabling a model, so they say what
//! is *not* known too. Weights are never part of LightCraft: every model is an opt-in the user installs.

use crate::manifest::{Colour, Commercial, InputSpec, Licence, ModelManifest, OutputSpec, Resize, Role, Thresholds};

/// Detector output decoders built into LightCraft.
pub const DECODERS: &[&str] = &["yunet-v2"];

/// The id of the face detector LightCraft runs (see [`yunet`]).
pub const YUNET_ID: &str = "yunet-2023mar";
pub const YUNET_SHA256: &str = "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4";
pub const SFACE_SHA256: &str = "0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79";
pub const AURAFACE_SHA256: &str = "a7933ea5330113b01c9b60351d8f4c33003f145d8470ac5f0e52ee2effe25c60";

fn licence(name: &str, commercial: Commercial, url: &str, notice: &str) -> Licence {
    Licence { name: name.into(), commercial, url: Some(url.into()), notice: notice.into() }
}

/// YuNet 2023mar (OpenCV Zoo): a small face detector.
pub fn yunet() -> ModelManifest {
    ModelManifest {
        id: YUNET_ID.into(),
        name: "YuNet (face detector)".into(),
        version: "2023mar".into(),
        role: Role::Detector,
        licence: licence(
            "MIT",
            Commercial::Yes,
            "https://github.com/opencv/opencv_zoo/blob/main/models/face_detection_yunet/LICENSE",
            "MIT licence, copyright Shiqi Yu.",
        ),
        source: Some("https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet".into()),
        sha256: Some(YUNET_SHA256.into()),
        size_bytes: Some(232_589),
        provenance: "WIDER FACE, whose terms are not settled by the weights' MIT licence. A detector: it finds faces, it does not identify anyone."
            .into(),
        input: InputSpec { width: 640, height: 640, colour: Colour::Bgr, mean: [0.0; 3], std: [1.0; 3], resize: Resize::Letterbox },
        output: OutputSpec::Detector { decoder: "yunet-v2".into() },
        thresholds: Thresholds { match_cosine: None, score: Some(0.6), nms_iou: Some(0.3) },
    }
}

/// SFace 2021dec (OpenCV Zoo).
pub fn sface() -> ModelManifest {
    ModelManifest {
        id: "sface-2021dec".into(),
        name: "SFace (face recogniser)".into(),
        version: "2021dec".into(),
        role: Role::Embedder,
        licence: licence(
            "Apache-2.0 (as labelled by OpenCV Zoo)",
            Commercial::Unknown,
            "https://github.com/opencv/opencv_zoo/tree/main/models/face_recognition_sface",
            "Labelled Apache-2.0, but what it was trained on is not documented, and two questions about commercial use (opencv_zoo issues 313 and 318) are unanswered. Fine to try for yourself; do not redistribute.",
        ),
        source: Some("https://github.com/opencv/opencv_zoo/tree/main/models/face_recognition_sface".into()),
        sha256: Some(SFACE_SHA256.into()),
        size_bytes: Some(38_696_353),
        provenance: "Undocumented. The original SFace repository mentions CASIA-WebFace, VGGFace2 and MS1MV2.".into(),
        input: InputSpec { width: 112, height: 112, colour: Colour::Rgb, mean: [0.0; 3], std: [1.0; 3], resize: Resize::Stretch },
        output: OutputSpec::Embedding { dim: 128 },
        thresholds: Thresholds::default(),
    }
}

/// AuraFace v1 `glintr100` (fal.ai).
pub fn auraface() -> ModelManifest {
    ModelManifest {
        id: "auraface-v1".into(),
        name: "AuraFace v1 (face recogniser)".into(),
        version: "1".into(),
        role: Role::Embedder,
        licence: licence(
            "Apache-2.0",
            Commercial::Yes,
            "https://huggingface.co/fal/AuraFace-v1",
            "Apache-2.0. Its training data is described only as a commercial dataset, with no dataset named and no consent statement, and it covers some ethnicities less well. About 261 MB, and slow on a CPU.",
        ),
        source: Some("https://huggingface.co/fal/AuraFace-v1".into()),
        sha256: Some(AURAFACE_SHA256.into()),
        size_bytes: Some(260_694_151),
        provenance: "Undisclosed: \"a commercial dataset comprising face images from various sources\".".into(),
        input: InputSpec::default(),
        output: OutputSpec::Embedding { dim: 512 },
        thresholds: Thresholds::default(),
    }
}

/// Where LightCraft can fetch a model from when the user asks: an address pinned to a commit of the model's own
/// repository, and the exact size and SHA-256 of the file (checked after the download, so the address is never
/// trusted on its own). The file is the last segment of the address, which is what the downloader needs: it fetches
/// `<mirror>/<file name>`, and here the mirror is everything before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Download {
    pub id: &'static str,
    pub url: &'static str,
    pub file_name: &'static str,
    pub size_bytes: u64,
    pub sha256: &'static str,
}

impl Download {
    /// The address without the file name (the downloader's "mirror").
    pub fn base(&self) -> &'static str {
        self.url.strip_suffix(self.file_name).map_or(self.url, |b| b.trim_end_matches('/'))
    }

    /// "github.com": which site the file comes from, as the user is told.
    pub fn host(&self) -> &'static str {
        self.url.strip_prefix("https://").and_then(|r| r.split('/').next()).unwrap_or("")
    }
}

/// Models LightCraft offers to download. A model marked non-commercial is never here: the user brings that file.
pub const SOURCES: &[Download] = &[
    Download {
        id: YUNET_ID,
        url: "https://github.com/opencv/opencv_zoo/raw/25f423d0e04c31a17254620e58febd7386da523b/models/face_detection_yunet/face_detection_yunet_2023mar.onnx",
        file_name: "face_detection_yunet_2023mar.onnx",
        size_bytes: 232_589,
        sha256: YUNET_SHA256,
    },
    Download {
        id: "sface-2021dec",
        url: "https://github.com/opencv/opencv_zoo/raw/25f423d0e04c31a17254620e58febd7386da523b/models/face_recognition_sface/face_recognition_sface_2021dec.onnx",
        file_name: "face_recognition_sface_2021dec.onnx",
        size_bytes: 38_696_353,
        sha256: SFACE_SHA256,
    },
    Download {
        id: "auraface-v1",
        url: "https://huggingface.co/fal/AuraFace-v1/resolve/af6d057c9b0ec4071d4c49c80e3539258798b609/glintr100.onnx",
        file_name: "glintr100.onnx",
        size_bytes: 260_694_151,
        sha256: AURAFACE_SHA256,
    },
];

/// How to download the model with this id, if LightCraft offers to.
pub fn download(id: &str) -> Option<Download> {
    SOURCES.iter().find(|d| d.id == id).copied()
}

/// Every model we know by hash.
pub fn all() -> Vec<ModelManifest> {
    vec![yunet(), sface(), auraface()]
}

/// The known model whose file has this SHA-256 (lowercase hex).
pub fn lookup(sha256: &str) -> Option<ModelManifest> {
    all().into_iter().find(|m| m.sha256.as_deref() == Some(sha256))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::validate;

    #[test]
    fn known_models_are_valid_and_unique() {
        let all = all();
        for m in &all {
            assert_eq!(validate(m), Ok(()), "{}", m.id);
        }
        let mut ids: Vec<_> = all.iter().map(|m| &m.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "unique ids");
        let mut hashes: Vec<_> = all.iter().filter_map(|m| m.sha256.as_deref()).collect();
        hashes.sort();
        hashes.dedup();
        assert_eq!(hashes.len(), all.len(), "unique hashes");
    }

    #[test]
    fn lookup_finds_by_hash() {
        assert_eq!(lookup(AURAFACE_SHA256).map(|m| m.id), Some("auraface-v1".to_string()));
        assert_eq!(lookup(YUNET_SHA256).map(|m| m.role), Some(Role::Detector));
        assert!(lookup(&"0".repeat(64)).is_none());
        assert!(lookup("").is_none());
    }

    #[test]
    fn downloads_are_pinned_checked_and_only_for_models_we_may_point_at() {
        for d in SOURCES {
            let m = all().into_iter().find(|m| m.id == d.id).unwrap_or_else(|| panic!("{} is not a known model", d.id));
            assert!(d.url.starts_with("https://"), "{}: https only", d.id);
            assert!(!d.url.contains("/main/") && !d.url.contains("/master/"), "{}: pinned to a commit, not a branch", d.id);
            assert!(d.file_name.ends_with(".onnx") && !d.file_name.contains(['/', '\\']), "{}", d.file_name);
            assert!(d.url.ends_with(&format!("/{}", d.file_name)), "{}: the address ends in the file's name", d.id);
            assert!(d.base().starts_with("https://") && !d.base().ends_with('/') && !d.base().ends_with(".onnx"), "{}", d.base());
            assert_eq!((Some(d.size_bytes), Some(d.sha256)), (m.size_bytes, m.sha256.as_deref()), "{}", d.id);
            assert!(m.licence.commercial != Commercial::No, "{} is non-commercial and must not be offered", d.id);
        }
        assert_eq!(download("yunet-2023mar").map(|d| d.host()), Some("github.com"));
        assert_eq!(download("auraface-v1").map(|d| d.host()), Some("huggingface.co"));
        for id in ["nope", "", "../sface-2021dec", "SFACE-2021DEC"] {
            assert!(download(id).is_none(), "{id}");
        }
    }

    #[test]
    fn unresolved_models_say_so() {
        assert_eq!(sface().licence.commercial, Commercial::Unknown);
        assert!(sface().provenance.to_lowercase().contains("undocumented"));
        assert!(auraface().provenance.to_lowercase().contains("undisclosed"));
    }
}
