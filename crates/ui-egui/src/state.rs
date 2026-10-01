//! UI state (serde: saved as preferences, readable/settable through the control channel).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ViewMode {
    /// Justified rows ("Photo Grid").
    #[default]
    PhotoGrid,
    SquareGrid,
    Detail,
    /// Two photos side by side (select | candidate), synced zoom.
    Compare,
    /// The selected photos tiled.
    Survey,
}

/// The right-hand tool/panel shown next to the tool strip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RightPanel {
    #[default]
    None,
    Edit,
    Crop,
    Remove,
    Masking,
    RedEye,
    Versions,
    Activity,
    Keywords,
    Info,
}

impl RightPanel {
    pub fn is_edit_tool(self) -> bool {
        matches!(self, RightPanel::Edit | RightPanel::Crop | RightPanel::Remove | RightPanel::Masking | RightPanel::RedEye)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Zoom {
    #[default]
    Fit,
    Fill,
    /// 100 % = one image pixel per physical screen pixel.
    Percent(u32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BeforeAfter {
    #[default]
    Off,
    /// Hold `\`: show the original.
    Original,
    SideBySide,
    Split,
    /// Before above after.
    TopBottom,
    /// One image split horizontally: before above the line.
    SplitTopBottom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CropOverlay {
    #[default]
    Thirds,
    Grid,
    Golden,
    Diagonal,
    None,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UiState {
    pub view: ViewMode,
    pub left_panel: bool,
    pub right: RightPanel,
    /// Presets column open (opens to the left of the Edit panel).
    pub presets: bool,
    pub filmstrip: bool,
    pub zoom: Zoom,
    /// Pan offset of the loupe when zoomed (image-normalized centre).
    pub pan: (f32, f32),
    pub before_after: BeforeAfter,
    pub thumb_size: f32,
    /// Open Edit sections by id.
    pub open_sections: Vec<String>,
    /// Open flyouts (curve, mixer, grading…).
    pub open_flyouts: Vec<String>,
    /// Single-panel mode: opening one section closes the others.
    pub single_panel: bool,
    pub show_clipping: bool,
    pub histogram: bool,
    pub mask_overlay: bool,
    pub crop_overlay: CropOverlay,
    pub show_filenames: bool,
    pub search: String,
    /// Selected curve channel in the Curve flyout.
    pub curve_channel: String,
    /// Selected mixer mode: "hue" | "saturation" | "luminance" | "all".
    pub mixer_mode: String,
    /// Selected colour grading wheel: "3way" | "shadows" | "midtones" | "highlights" | "global".
    pub grading_mode: String,
    /// Active on-canvas tool: "", "brush", "linear", "radial", "wbPicker", "straighten", "remove".
    pub tool: String,
    pub brush_size: f32,
    pub brush_feather: f32,
    pub brush_flow: f32,
    pub brush_erase: bool,
    pub remove_size: f32,
    /// Culling: after a rating, flag or colour-label key, move to the next photo.
    pub auto_advance: bool,
    /// Compare view: (select, candidate) photo ids.
    #[serde(skip)]
    pub compare: Option<(u64, u64)>,
    /// Transient toast text and its expiry (seconds of app time).
    #[serde(skip)]
    pub toast: Option<(String, f64)>,
    #[serde(skip)]
    pub status: String,
    #[serde(skip)]
    pub dialog: Option<Dialog>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Dialog {
    NewAlbum {
        name: String,
        folder: bool,
    },
    RenameAlbum {
        id: u64,
        name: String,
    },
    /// Auto-stack by capture time: the largest gap between consecutive shots, in seconds.
    AutoStack {
        gap: f32,
    },
    /// Save the current view (source + filter) as a smart album.
    NewSmartAlbum {
        name: String,
    },
    CreatePreset {
        name: String,
        group: String,
    },
    CopySettings {
        groups: Vec<String>,
    },
    /// `long_edge` 0 = full size; `limit_kb` 0 = no limit; `dir` empty = default export folder.
    Export {
        opts: lightcraft_engine::export::ExportOptions,
        long_edge: u32,
        limit_kb: u32,
        dir: String,
    },
    About,
    Shortcuts,
}

impl Default for UiState {
    fn default() -> Self {
        UiState {
            view: ViewMode::Detail,
            left_panel: false,
            right: RightPanel::Edit,
            presets: false,
            filmstrip: true,
            zoom: Zoom::Fit,
            pan: (0.5, 0.5),
            before_after: BeforeAfter::Off,
            thumb_size: 220.0,
            open_sections: vec!["light".into()],
            open_flyouts: vec![],
            single_panel: false,
            show_clipping: false,
            histogram: true,
            mask_overlay: true,
            crop_overlay: CropOverlay::Thirds,
            show_filenames: true,
            search: String::new(),
            curve_channel: "parametric".into(),
            mixer_mode: "hue".into(),
            grading_mode: "3way".into(),
            tool: String::new(),
            brush_size: 0.04,
            brush_feather: 50.0,
            brush_flow: 60.0,
            brush_erase: false,
            remove_size: 0.02,
            auto_advance: false,
            compare: None,
            toast: None,
            status: String::new(),
            dialog: None,
        }
    }
}

impl UiState {
    pub fn section_open(&self, id: &str) -> bool {
        self.open_sections.iter().any(|s| s == id)
    }
    pub fn toggle_section(&mut self, id: &str) {
        if self.section_open(id) {
            self.open_sections.retain(|s| s != id);
        } else {
            if self.single_panel {
                self.open_sections.clear();
            }
            self.open_sections.push(id.to_string());
        }
    }
    pub fn flyout_open(&self, id: &str) -> bool {
        self.open_flyouts.iter().any(|s| s == id)
    }
    pub fn toggle_flyout(&mut self, id: &str) {
        if self.flyout_open(id) {
            self.open_flyouts.retain(|s| s != id);
        } else {
            self.open_flyouts.push(id.to_string());
        }
    }
    /// Clamp values restored from disk.
    pub fn sanitized(mut self) -> Self {
        self.thumb_size = self.thumb_size.clamp(90.0, 480.0);
        self.brush_size = self.brush_size.clamp(0.002, 0.5);
        self.dialog = None;
        self
    }
}
