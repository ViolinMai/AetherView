use crate::cache::ImageCacheSystem;
use crate::registry::{DetectedFormat, FormatRegistry};
use aether_core::{
    AetherError, AetherResult, ColorSpaceInfo, DecodedFrame, ExifSummary, ImageMetadata,
};
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageReader};
use std::fs::{self, File};
use std::io::{BufReader, Cursor, Read};
use std::path::Path;
use std::sync::Arc;

pub struct ImageEngine {
    pub registry: FormatRegistry,
    pub cache: Arc<ImageCacheSystem>,
    pub max_dimension: u32,
}

impl ImageEngine {
    pub fn new(cache: Arc<ImageCacheSystem>, max_dimension: u32) -> Self {
        Self {
            registry: FormatRegistry::new(),
            cache,
            max_dimension,
        }
    }

    /// Reads EXIF tags and orientation from a file when present.
    pub fn extract_exif(path: &Path) -> ExifSummary {
        let mut summary = ExifSummary::default();
        let Ok(file) = File::open(path) else {
            return summary;
        };
        let mut bufreader = BufReader::new(file);
        let exifreader = exif::Reader::new();
        let Ok(exif) = exifreader.read_from_container(&mut bufreader) else {
            return summary;
        };

        if let Some(field) = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY) {
            if let Some(v) = field.value.get_uint(0) {
                summary.orientation = v as u16;
            }
        }
        if let Some(field) = exif.get_field(exif::Tag::Make, exif::In::PRIMARY) {
            summary.camera_make = Some(field.display_value().with_unit(&exif).to_string());
        }
        if let Some(field) = exif.get_field(exif::Tag::Model, exif::In::PRIMARY) {
            summary.camera_model = Some(field.display_value().with_unit(&exif).to_string());
        }
        if let Some(field) = exif.get_field(exif::Tag::ExposureTime, exif::In::PRIMARY) {
            summary.exposure_time = Some(field.display_value().with_unit(&exif).to_string());
        }
        if let Some(field) = exif.get_field(exif::Tag::FNumber, exif::In::PRIMARY) {
            summary.f_number = Some(field.display_value().with_unit(&exif).to_string());
        }
        if let Some(field) = exif.get_field(exif::Tag::PhotographicSensitivity, exif::In::PRIMARY)
        {
            summary.iso = field.value.get_uint(0);
        }
        if let Some(field) = exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY) {
            summary.date_taken = Some(field.display_value().to_string());
        }

        summary
    }

    /// Applies EXIF orientation transform so photos never appear sideways or upside down.
    pub fn apply_exif_orientation(img: DynamicImage, orientation: u16) -> DynamicImage {
        match orientation {
            2 => img.fliph(),
            3 => img.rotate180(),
            4 => img.flipv(),
            5 => img.rotate90().fliph(),
            6 => img.rotate90(),
            7 => img.rotate270().fliph(),
            8 => img.rotate270(),
            _ => img,
        }
    }

    /// Decodes a quick low-resolution preview for instant progressive rendering on massive files.
    pub fn decode_fast_preview(&self, path: &Path, target_max_edge: u32) -> AetherResult<DecodedFrame> {
        let key = ImageCacheSystem::compute_file_key(path);
        if let Some(cached) = self.cache.get_full(&key) {
            return Ok(cached);
        }

        let full = self.decode_file(path)?;
        let (w, h) = (full.width, full.height);
        if w <= target_max_edge && h <= target_max_edge {
            return Ok(full);
        }

        let scale = (target_max_edge as f32 / w.max(h) as f32).clamp(0.05, 1.0);
        let nw = ((w as f32 * scale) as u32).max(1);
        let nh = ((h as f32 * scale) as u32).max(1);

        let src = image::RgbaImage::from_raw(w, h, full.rgba_pixels.as_ref().clone())
            .ok_or_else(|| AetherError::DecodeError("Invalid RGBA buffer".into()))?;
        let resized = image::imageops::resize(&src, nw, nh, image::imageops::FilterType::Triangle);

        Ok(DecodedFrame {
            width: nw,
            height: nh,
            rgba_pixels: Arc::new(resized.into_raw()),
            metadata: full.metadata,
            is_preview: true,
        })
    }

    pub fn decode_file(&self, path: &Path) -> AetherResult<DecodedFrame> {
        let key = ImageCacheSystem::compute_file_key(path);
        if let Some(cached) = self.cache.get_full(&key) {
            return Ok(cached);
        }

        let meta_fs = fs::metadata(path).map_err(|source| AetherError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let file_size_bytes = meta_fs.len();

        let mut file = File::open(path).map_err(|source| AetherError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let mut header = [0u8; 256];
        let read_bytes = file.read(&mut header).unwrap_or(0);

        let detected = FormatRegistry::detect(path, &header[..read_bytes]).ok_or_else(|| {
            AetherError::UnsupportedFormat(format!(
                "Could not identify format of '{}'",
                path.display()
            ))
        })?;

        if detected == DetectedFormat::Svg {
            let frame = self.decode_svg(path, file_size_bytes)?;
            self.cache.insert_full(key, frame.clone());
            return Ok(frame);
        }

        let exif = Self::extract_exif(path);
        let reader = ImageReader::open(path)
            .map_err(|source| AetherError::Io {
                path: path.to_path_buf(),
                source,
            })?
            .with_guessed_format()
            .map_err(|source| AetherError::Io {
                path: path.to_path_buf(),
                source,
            })?;

        let format_str = reader
            .format()
            .map(|f| format!("{:?}", f).to_uppercase())
            .unwrap_or_else(|| format!("{:?}", detected).to_uppercase());

        let mut decoder = reader
            .into_decoder()
            .map_err(|e| AetherError::DecodeError(e.to_string()))?;

        let (raw_w, raw_h) = decoder.dimensions();
        if raw_w > self.max_dimension || raw_h > self.max_dimension {
            return Err(AetherError::DimensionsExceeded {
                width: raw_w,
                height: raw_h,
                max_dim: self.max_dimension,
            });
        }

        let icc_profile_present = decoder
            .icc_profile()
            .ok()
            .flatten()
            .map(|icc| !icc.is_empty())
            .unwrap_or(false);

        let color_type = decoder.color_type();
        let bit_depth = color_type.bits_per_pixel() / color_type.channel_count() as u16;
        let has_alpha = color_type.has_alpha();

        let dyn_img = DynamicImage::from_decoder(decoder)
            .map_err(|e| AetherError::DecodeError(e.to_string()))?;

        let oriented = Self::apply_exif_orientation(dyn_img, exif.orientation);
        let (width, height) = oriented.dimensions();
        let rgba = oriented.to_rgba8();

        let color_space = if detected == DetectedFormat::Hdr || detected == DetectedFormat::OpenExr
        {
            ColorSpaceInfo::LinearRgb
        } else if icc_profile_present {
            ColorSpaceInfo::DisplayP3
        } else {
            ColorSpaceInfo::Srgb
        };

        let metadata = ImageMetadata {
            path: path.to_path_buf(),
            file_name: path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".into()),
            file_size_bytes,
            width,
            height,
            format_name: format_str,
            bit_depth: bit_depth as u8,
            has_alpha,
            is_animated: matches!(detected, DetectedFormat::Gif | DetectedFormat::WebP),
            color_space,
            icc_profile_present,
            exif,
        };

        let frame = DecodedFrame {
            width,
            height,
            rgba_pixels: Arc::new(rgba.into_raw()),
            metadata,
            is_preview: false,
        };

        self.cache.insert_full(key, frame.clone());
        Ok(frame)
    }

    /// Decodes an in-memory buffer (e.g., from a CBZ archive entry).
    pub fn decode_bytes(
        &self,
        bytes: &[u8],
        virtual_path: &Path,
    ) -> AetherResult<DecodedFrame> {
        let reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| AetherError::DecodeError(e.to_string()))?;

        let format_str = reader
            .format()
            .map(|f| format!("{:?}", f).to_uppercase())
            .unwrap_or_else(|| "ARCHIVE_IMAGE".into());

        let dyn_img = reader
            .decode()
            .map_err(|e| AetherError::DecodeError(e.to_string()))?;

        let (width, height) = dyn_img.dimensions();
        if width > self.max_dimension || height > self.max_dimension {
            return Err(AetherError::DimensionsExceeded {
                width,
                height,
                max_dim: self.max_dimension,
            });
        }

        let has_alpha = dyn_img.color().has_alpha();
        let rgba = dyn_img.to_rgba8();

        let metadata = ImageMetadata {
            path: virtual_path.to_path_buf(),
            file_name: virtual_path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Page".into()),
            file_size_bytes: bytes.len() as u64,
            width,
            height,
            format_name: format_str,
            bit_depth: 8,
            has_alpha,
            is_animated: false,
            color_space: ColorSpaceInfo::Srgb,
            icc_profile_present: false,
            exif: ExifSummary::default(),
        };

        Ok(DecodedFrame {
            width,
            height,
            rgba_pixels: Arc::new(rgba.into_raw()),
            metadata,
            is_preview: false,
        })
    }

    fn decode_svg(&self, path: &Path, file_size_bytes: u64) -> AetherResult<DecodedFrame> {
        let data = fs::read(path).map_err(|source| AetherError::Io {
            path: path.to_path_buf(),
            source,
        })?;

        let opt = resvg::usvg::Options::default();
        let tree = resvg::usvg::Tree::from_data(&data, &opt)
            .map_err(|e| AetherError::DecodeError(format!("SVG parse error: {}", e)))?;

        let size = tree.size().to_int_size();
        let width = size.width().clamp(1, 8192);
        let height = size.height().clamp(1, 8192);

        let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
            .ok_or_else(|| AetherError::DecodeError("Failed to allocate SVG pixmap".into()))?;

        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::default(),
            &mut pixmap.as_mut(),
        );

        let metadata = ImageMetadata {
            path: path.to_path_buf(),
            file_name: path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "vector.svg".into()),
            file_size_bytes,
            width,
            height,
            format_name: "SVG".into(),
            bit_depth: 8,
            has_alpha: true,
            is_animated: false,
            color_space: ColorSpaceInfo::Srgb,
            icc_profile_present: false,
            exif: ExifSummary::default(),
        };

        Ok(DecodedFrame {
            width,
            height,
            rgba_pixels: Arc::new(pixmap.take()),
            metadata,
            is_preview: false,
        })
    }

    pub fn generate_thumbnail(
        &self,
        path: &Path,
        thumb_size: u32,
    ) -> AetherResult<(u32, u32, Arc<Vec<u8>>)> {
        let key = ImageCacheSystem::compute_file_key(path);
        if let Some(hit) = self.cache.get_or_load_disk_thumbnail(&key, thumb_size) {
            return Ok(hit);
        }

        let decoded = self.decode_file(path)?;
        let src = image::RgbaImage::from_raw(
            decoded.width,
            decoded.height,
            decoded.rgba_pixels.as_ref().clone(),
        )
        .ok_or_else(|| AetherError::DecodeError("Failed to build thumbnail buffer".into()))?;

        let dyn_img = DynamicImage::ImageRgba8(src);
        let thumb = dyn_img.thumbnail(thumb_size, thumb_size).to_rgba8();
        let (tw, th) = thumb.dimensions();
        let raw = Arc::new(thumb.into_raw());

        self.cache
            .store_disk_thumbnail(&key, thumb_size, tw, th, &raw);
        Ok((tw, th, raw))
    }
}
