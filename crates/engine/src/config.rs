//! Shared, opt-in per-user denoise model storage for native hosts.

use crate::Session;
use std::path::PathBuf;

pub fn default_denoise_models_dir() -> Option<PathBuf> {
    std::env::var_os("LIGHTCRAFT_DENOISE_MODELS")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| crate::camera_profiles::config_dir().map(|d| d.join("denoise-models")))
}

impl Session {
    pub fn with_default_denoise_models(mut self) -> Self {
        self.set_denoise_models_dir(default_denoise_models_dir());
        self
    }

    pub fn set_denoise_models_dir(&mut self, dir: Option<PathBuf>) {
        self.denoise.models_dir = dir;
        self.denoise.touch();
    }
}
