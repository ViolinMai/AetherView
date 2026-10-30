use aether_core::{AetherError, AetherResult, DecodedFrame};
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageFormat, RgbaImage};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormatChoice {
    Png,
    Jpeg,
    WebP,
    Tiff,
    Qoi,
}

impl ExportFormatChoice {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
            Self::Tiff => "tiff",
            Self::Qoi => "qoi",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::WebP => "WebP",
            Self::Tiff => "TIFF",
            Self::Qoi => "QOI",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessingPipelineMode {
    GpuAccelerated,
    CpuSoftware,
}

impl ProcessingPipelineMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::GpuAccelerated => "GPU (Vulkan WGPU)",
            Self::CpuSoftware => "Software (CPU SIMD)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResampleFilter {
    Lanczos3,
    CatmullRom,
    Triangle,
    Nearest,
}

impl std::fmt::Display for ResampleFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lanczos3 => write!(f, "Lanczos3 (High Quality)"),
            Self::CatmullRom => write!(f, "Bicubic (Catmull-Rom)"),
            Self::Triangle => write!(f, "Bilinear (Fast)"),
            Self::Nearest => write!(f, "Nearest Neighbor (Pixel Art)"),
        }
    }
}

impl ResampleFilter {
    pub fn to_image_filter(self) -> image::imageops::FilterType {
        match self {
            Self::Lanczos3 => image::imageops::FilterType::Lanczos3,
            Self::CatmullRom => image::imageops::FilterType::CatmullRom,
            Self::Triangle => image::imageops::FilterType::Triangle,
            Self::Nearest => image::imageops::FilterType::Nearest,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AspectRatioPreset {
    Freeform,
    Square1x1,
    Standard4x3,
    Widescreen16x9,
    Photo3x2,
}

impl std::fmt::Display for AspectRatioPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Freeform => write!(f, "Freeform"),
            Self::Square1x1 => write!(f, "1:1 (Square)"),
            Self::Standard4x3 => write!(f, "4:3 (Standard)"),
            Self::Widescreen16x9 => write!(f, "16:9 (Widescreen)"),
            Self::Photo3x2 => write!(f, "3:2 (Photography)"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EditOperation {
    Rotate90Cw,
    Rotate90Ccw,
    Rotate180,
    FlipHorizontal,
    FlipVertical,
    Crop {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    Resize {
        width: u32,
        height: u32,
        filter: ResampleFilter,
    },
}

impl std::fmt::Display for EditOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rotate90Cw => write!(f, "Rotate 90° CW"),
            Self::Rotate90Ccw => write!(f, "Rotate 90° CCW"),
            Self::Rotate180 => write!(f, "Rotate 180°"),
            Self::FlipHorizontal => write!(f, "Flip Horizontal"),
            Self::FlipVertical => write!(f, "Flip Vertical"),
            Self::Crop {
                x,
                y,
                width,
                height,
            } => write!(f, "Crop ({}x{} at {},{})", width, height, x, y),
            Self::Resize {
                width,
                height,
                filter,
            } => write!(f, "Resize ({}x{} via {})", width, height, filter),
        }
    }
}

/// Non-destructive operation-based editing pipeline.
/// Stores lightweight operation records instead of duplicating full-resolution buffers.
#[derive(Debug, Clone, Default)]
pub struct NonDestructivePipeline {
    committed_ops: Vec<EditOperation>,
    redo_stack: Vec<EditOperation>,
    preview_op: Option<EditOperation>,
    revision: u64,
}

impl NonDestructivePipeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, op: EditOperation) {
        self.preview_op = None;
        self.committed_ops.push(op);
        self.redo_stack.clear();
        self.revision += 1;
    }

    pub fn set_preview(&mut self, op: Option<EditOperation>) {
        self.preview_op = op;
        self.revision += 1;
    }

    pub fn commit_preview(&mut self) {
        if let Some(op) = self.preview_op.take() {
            self.committed_ops.push(op);
            self.redo_stack.clear();
            self.revision += 1;
        }
    }

    pub fn undo(&mut self) -> bool {
        if let Some(op) = self.committed_ops.pop() {
            self.redo_stack.push(op);
            self.revision += 1;
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(op) = self.redo_stack.pop() {
            self.committed_ops.push(op);
            self.revision += 1;
            true
        } else {
            false
        }
    }

    pub fn reset(&mut self) {
        self.committed_ops.clear();
        self.redo_stack.clear();
        self.preview_op = None;
        self.revision += 1;
    }

    pub fn can_undo(&self) -> bool {
        !self.committed_ops.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn is_modified(&self) -> bool {
        !self.committed_ops.is_empty() || self.preview_op.is_some()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn operations(&self) -> &[EditOperation] {
        &self.committed_ops
    }

    /// Evaluates the non-destructive stack against the base frame to produce the transformed frame.
    pub fn evaluate(&self, base: &DecodedFrame) -> AetherResult<DecodedFrame> {
        if !self.is_modified() {
            return Ok(base.clone());
        }

        let rgba = RgbaImage::from_raw(
            base.width,
            base.height,
            base.rgba_pixels.as_ref().clone(),
        )
        .ok_or_else(|| AetherError::DecodeError("Invalid base RGBA buffer".into()))?;

        let mut img = DynamicImage::ImageRgba8(rgba);

        for op in &self.committed_ops {
            img = Self::apply_single(img, op);
        }

        if let Some(preview) = &self.preview_op {
            img = Self::apply_single(img, preview);
        }

        let out_rgba = img.to_rgba8();
        let (width, height) = out_rgba.dimensions();
        let mut metadata = base.metadata.clone();
        metadata.width = width;
        metadata.height = height;

        Ok(DecodedFrame {
            width,
            height,
            rgba_pixels: Arc::new(out_rgba.into_raw()),
            metadata,
            is_preview: base.is_preview,
        })
    }

    fn apply_single(img: DynamicImage, op: &EditOperation) -> DynamicImage {
        match op {
            EditOperation::Rotate90Cw => img.rotate90(),
            EditOperation::Rotate90Ccw => img.rotate270(),
            EditOperation::Rotate180 => img.rotate180(),
            EditOperation::FlipHorizontal => img.fliph(),
            EditOperation::FlipVertical => img.flipv(),
            EditOperation::Crop {
                x,
                y,
                width,
                height,
            } => {
                let max_w = img.width();
                let max_h = img.height();
                let cx = (*x).min(max_w.saturating_sub(1));
                let cy = (*y).min(max_h.saturating_sub(1));
                let cw = (*width).clamp(1, max_w.saturating_sub(cx));
                let ch = (*height).clamp(1, max_h.saturating_sub(cy));
                img.crop_imm(cx, cy, cw, ch)
            }
            EditOperation::Resize {
                width,
                height,
                filter,
            } => {
                let nw = (*width).clamp(1, 32768);
                let nh = (*height).clamp(1, 32768);
                img.resize_exact(nw, nh, filter.to_image_filter())
            }
        }
    }

    pub fn export_frame(
        frame: &DecodedFrame,
        target_path: &Path,
        jpeg_quality: u8,
    ) -> AetherResult<()> {
        let rgba = RgbaImage::from_raw(
            frame.width,
            frame.height,
            frame.rgba_pixels.as_ref().clone(),
        )
        .ok_or_else(|| AetherError::DecodeError("Invalid RGBA buffer for export".into()))?;

        let ext = target_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_else(|| "png".into());

        match ext.as_str() {
            "jpg" | "jpeg" => {
                let file = File::create(target_path).map_err(|source| AetherError::Io {
                    path: target_path.to_path_buf(),
                    source,
                })?;
                let writer = BufWriter::new(file);
                let rgb = DynamicImage::ImageRgba8(rgba).to_rgb8();
                let mut encoder = JpegEncoder::new_with_quality(writer, jpeg_quality.clamp(1, 100));
                encoder
                    .encode_image(&rgb)
                    .map_err(|e| AetherError::DecodeError(e.to_string()))?;
            }
            "webp" => {
                rgba.save_with_format(target_path, ImageFormat::WebP)
                    .map_err(|e| AetherError::DecodeError(e.to_string()))?;
            }
            "tif" | "tiff" => {
                rgba.save_with_format(target_path, ImageFormat::Tiff)
                    .map_err(|e| AetherError::DecodeError(e.to_string()))?;
            }
            "bmp" => {
                rgba.save_with_format(target_path, ImageFormat::Bmp)
                    .map_err(|e| AetherError::DecodeError(e.to_string()))?;
            }
            "qoi" => {
                rgba.save_with_format(target_path, ImageFormat::Qoi)
                    .map_err(|e| AetherError::DecodeError(e.to_string()))?;
            }
            _ => {
                rgba.save_with_format(target_path, ImageFormat::Png)
                    .map_err(|e| AetherError::DecodeError(e.to_string()))?;
            }
        }

        Ok(())
    }
}
