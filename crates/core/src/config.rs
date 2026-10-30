use crate::error::{AetherError, AetherResult};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    Dark,
    Light,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZoomMode {
    FitWindow,
    FitWidth,
    FitHeight,
    ActualSize,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadingDirection {
    LeftToRight,
    RightToLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuBackendPreference {
    Vulkan,
    AutoHardware,
    FallbackSoftware,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OcrLanguageMode {
    ArabicAndEnglish,
    ArabicOnly,
    EnglishOnly,
    Auto,
}

impl std::fmt::Display for OcrLanguageMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ArabicAndEnglish => write!(f, "Arabic + English (Bilingual)"),
            Self::ArabicOnly => write!(f, "Arabic Only (RTL)"),
            Self::EnglishOnly => write!(f, "English Only (LTR)"),
            Self::Auto => write!(f, "Auto-Detect Script"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub theme: ThemeMode,
    pub default_zoom_mode: ZoomMode,
    pub default_cbz_direction: ReadingDirection,
    pub two_page_spread: bool,
    pub show_sidebar: bool,
    pub show_info_panel: bool,
    pub thumbnail_size: u32,
    pub memory_cache_limit_mb: usize,
    pub disk_cache_limit_mb: usize,
    pub vram_limit_mb: usize,
    pub max_image_dimension: u32,
    pub gpu_backend: GpuBackendPreference,
    pub ocr_language: OcrLanguageMode,
    pub ocr_use_cuda: bool,
    pub ocr_models_dir: PathBuf,
    pub recent_files: Vec<PathBuf>,
    pub document_bookmarks: HashMap<String, usize>,
    pub window_width: u32,
    pub window_height: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        let dirs = Self::project_dirs();
        let models_dir = dirs
            .as_ref()
            .map(|d| d.data_local_dir().join("models"))
            .unwrap_or_else(|| PathBuf::from("assets/models"));

        Self {
            theme: ThemeMode::Dark,
            default_zoom_mode: ZoomMode::FitWindow,
            default_cbz_direction: ReadingDirection::RightToLeft,
            two_page_spread: false,
            show_sidebar: false,
            show_info_panel: false,
            thumbnail_size: 160,
            memory_cache_limit_mb: 1024,
            disk_cache_limit_mb: 2048,
            vram_limit_mb: 2048,
            max_image_dimension: 32768,
            gpu_backend: GpuBackendPreference::Vulkan,
            ocr_language: OcrLanguageMode::ArabicAndEnglish,
            ocr_use_cuda: true,
            ocr_models_dir: models_dir,
            recent_files: Vec::new(),
            document_bookmarks: HashMap::new(),
            window_width: 1600,
            window_height: 960,
        }
    }
}

impl AppConfig {
    pub fn project_dirs() -> Option<ProjectDirs> {
        ProjectDirs::from("org", "aetherview", "AetherView")
    }

    pub fn config_file_path() -> PathBuf {
        Self::project_dirs()
            .map(|d| d.config_dir().join("config.toml"))
            .unwrap_or_else(|| PathBuf::from("aetherview_config.toml"))
    }

    pub fn cache_dir_path() -> PathBuf {
        Self::project_dirs()
            .map(|d| d.cache_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".aetherview_cache"))
    }

    pub fn load() -> Self {
        let path = Self::config_file_path();
        if let Ok(contents) = fs::read_to_string(&path) {
            if let Ok(cfg) = toml::from_str::<AppConfig>(&contents) {
                return cfg;
            }
        }
        let default_cfg = Self::default();
        let _ = default_cfg.save();
        default_cfg
    }

    pub fn save(&self) -> AetherResult<()> {
        let path = Self::config_file_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| AetherError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let serialized = toml::to_string_pretty(self)
            .map_err(|e| AetherError::ConfigError(e.to_string()))?;
        fs::write(&path, serialized).map_err(|source| AetherError::Io { path, source })?;
        Ok(())
    }

    pub fn record_recent_file(&mut self, path: &Path) {
        let canonical = path.to_path_buf();
        self.recent_files.retain(|p| p != &canonical);
        self.recent_files.insert(0, canonical);
        if self.recent_files.len() > 20 {
            self.recent_files.truncate(20);
        }
    }

    pub fn save_bookmark(&mut self, path: &Path, page_index: usize) {
        self.document_bookmarks
            .insert(path.to_string_lossy().to_string(), page_index);
    }

    pub fn get_bookmark(&self, path: &Path) -> Option<usize> {
        self.document_bookmarks
            .get(&path.to_string_lossy().to_string())
            .copied()
    }
}
