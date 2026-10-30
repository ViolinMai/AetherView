use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppMode {
    ImageViewer,
    ImageEditor,
    PdfViewer,
    ComicReader,
    OcrInspector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorSpaceInfo {
    Srgb,
    LinearRgb,
    DisplayP3,
    AdobeRgb,
    Cmyk,
    Grayscale,
    Hdr10,
    Unknown,
}

impl std::fmt::Display for ColorSpaceInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Srgb => write!(f, "sRGB"),
            Self::LinearRgb => write!(f, "Linear RGB"),
            Self::DisplayP3 => write!(f, "Display P3 (Wide Gamut)"),
            Self::AdobeRgb => write!(f, "Adobe RGB (1998)"),
            Self::Cmyk => write!(f, "CMYK"),
            Self::Grayscale => write!(f, "Grayscale"),
            Self::Hdr10 => write!(f, "HDR10 / Rec.2020"),
            Self::Unknown => write!(f, "Untagged (Assumed sRGB)"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExifSummary {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub orientation: u16,
    pub exposure_time: Option<String>,
    pub f_number: Option<String>,
    pub iso: Option<u32>,
    pub date_taken: Option<String>,
}

impl Default for ExifSummary {
    fn default() -> Self {
        Self {
            camera_make: None,
            camera_model: None,
            orientation: 1,
            exposure_time: None,
            f_number: None,
            iso: None,
            date_taken: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMetadata {
    pub path: PathBuf,
    pub file_name: String,
    pub file_size_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub format_name: String,
    pub bit_depth: u8,
    pub has_alpha: bool,
    pub is_animated: bool,
    pub color_space: ColorSpaceInfo,
    pub icc_profile_present: bool,
    pub exif: ExifSummary,
}

#[derive(Debug, Clone)]
pub struct DecodedFrame {
    pub width: u32,
    pub height: u32,
    pub rgba_pixels: Arc<Vec<u8>>,
    pub metadata: ImageMetadata,
    pub is_preview: bool,
}

impl DecodedFrame {
    pub fn byte_len(&self) -> usize {
        self.rgba_pixels.len()
    }
}
