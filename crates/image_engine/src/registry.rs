use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedFormat {
    Jpeg,
    Png,
    WebP,
    Gif,
    Tiff,
    Bmp,
    Ico,
    Tga,
    Pnm,
    Qoi,
    Hdr,
    OpenExr,
    Dds,
    Svg,
    Avif,
    Heic,
    JpegXl,
    Pdf,
    Cbz,
    Raw,
}

#[derive(Debug, Clone)]
pub struct DecoderCapability {
    pub format: DetectedFormat,
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub supports_animation: bool,
    pub supports_alpha: bool,
    pub supports_color_profile: bool,
    pub supports_high_bit_depth: bool,
    pub supports_shrink_on_load: bool,
}

pub struct FormatRegistry {
    capabilities: Vec<DecoderCapability>,
}

impl Default for FormatRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatRegistry {
    pub fn new() -> Self {
        Self {
            capabilities: vec![
                DecoderCapability {
                    format: DetectedFormat::Jpeg,
                    name: "JPEG",
                    extensions: &["jpg", "jpeg", "jpe", "jfif"],
                    supports_animation: false,
                    supports_alpha: false,
                    supports_color_profile: true,
                    supports_high_bit_depth: false,
                    supports_shrink_on_load: true,
                },
                DecoderCapability {
                    format: DetectedFormat::Png,
                    name: "PNG / APNG",
                    extensions: &["png", "apng"],
                    supports_animation: true,
                    supports_alpha: true,
                    supports_color_profile: true,
                    supports_high_bit_depth: true,
                    supports_shrink_on_load: false,
                },
                DecoderCapability {
                    format: DetectedFormat::WebP,
                    name: "WebP",
                    extensions: &["webp"],
                    supports_animation: true,
                    supports_alpha: true,
                    supports_color_profile: true,
                    supports_high_bit_depth: false,
                    supports_shrink_on_load: true,
                },
                DecoderCapability {
                    format: DetectedFormat::Gif,
                    name: "GIF",
                    extensions: &["gif"],
                    supports_animation: true,
                    supports_alpha: true,
                    supports_color_profile: false,
                    supports_high_bit_depth: false,
                    supports_shrink_on_load: false,
                },
                DecoderCapability {
                    format: DetectedFormat::Tiff,
                    name: "TIFF",
                    extensions: &["tif", "tiff"],
                    supports_animation: false,
                    supports_alpha: true,
                    supports_color_profile: true,
                    supports_high_bit_depth: true,
                    supports_shrink_on_load: false,
                },
                DecoderCapability {
                    format: DetectedFormat::Bmp,
                    name: "BMP",
                    extensions: &["bmp"],
                    supports_animation: false,
                    supports_alpha: true,
                    supports_color_profile: false,
                    supports_high_bit_depth: false,
                    supports_shrink_on_load: false,
                },
                DecoderCapability {
                    format: DetectedFormat::Ico,
                    name: "ICO",
                    extensions: &["ico", "cur"],
                    supports_animation: false,
                    supports_alpha: true,
                    supports_color_profile: false,
                    supports_high_bit_depth: false,
                    supports_shrink_on_load: false,
                },
                DecoderCapability {
                    format: DetectedFormat::Tga,
                    name: "TGA",
                    extensions: &["tga"],
                    supports_animation: false,
                    supports_alpha: true,
                    supports_color_profile: false,
                    supports_high_bit_depth: false,
                    supports_shrink_on_load: false,
                },
                DecoderCapability {
                    format: DetectedFormat::Pnm,
                    name: "PNM / PPM / PGM / PBM",
                    extensions: &["pnm", "ppm", "pgm", "pbm", "pam"],
                    supports_animation: false,
                    supports_alpha: true,
                    supports_color_profile: false,
                    supports_high_bit_depth: true,
                    supports_shrink_on_load: false,
                },
                DecoderCapability {
                    format: DetectedFormat::Qoi,
                    name: "QOI (Quite OK Image)",
                    extensions: &["qoi"],
                    supports_animation: false,
                    supports_alpha: true,
                    supports_color_profile: true,
                    supports_high_bit_depth: false,
                    supports_shrink_on_load: false,
                },
                DecoderCapability {
                    format: DetectedFormat::Hdr,
                    name: "Radiance HDR",
                    extensions: &["hdr"],
                    supports_animation: false,
                    supports_alpha: false,
                    supports_color_profile: true,
                    supports_high_bit_depth: true,
                    supports_shrink_on_load: false,
                },
                DecoderCapability {
                    format: DetectedFormat::OpenExr,
                    name: "OpenEXR",
                    extensions: &["exr"],
                    supports_animation: false,
                    supports_alpha: true,
                    supports_color_profile: true,
                    supports_high_bit_depth: true,
                    supports_shrink_on_load: false,
                },
                DecoderCapability {
                    format: DetectedFormat::Dds,
                    name: "DirectDraw Surface (DDS)",
                    extensions: &["dds"],
                    supports_animation: false,
                    supports_alpha: true,
                    supports_color_profile: false,
                    supports_high_bit_depth: false,
                    supports_shrink_on_load: false,
                },
                DecoderCapability {
                    format: DetectedFormat::Svg,
                    name: "Scalable Vector Graphics (SVG)",
                    extensions: &["svg", "svgz"],
                    supports_animation: false,
                    supports_alpha: true,
                    supports_color_profile: true,
                    supports_high_bit_depth: false,
                    supports_shrink_on_load: true,
                },
                DecoderCapability {
                    format: DetectedFormat::Pdf,
                    name: "PDF Document",
                    extensions: &["pdf"],
                    supports_animation: false,
                    supports_alpha: true,
                    supports_color_profile: true,
                    supports_high_bit_depth: false,
                    supports_shrink_on_load: true,
                },
                DecoderCapability {
                    format: DetectedFormat::Cbz,
                    name: "CBZ Comic Archive",
                    extensions: &["cbz"],
                    supports_animation: false,
                    supports_alpha: true,
                    supports_color_profile: true,
                    supports_high_bit_depth: false,
                    supports_shrink_on_load: true,
                },
            ],
        }
    }

    pub fn capabilities(&self) -> &[DecoderCapability] {
        &self.capabilities
    }

    /// Detects file format primarily via Magic Bytes, falling back to extension when needed.
    pub fn detect(path: &Path, header: &[u8]) -> Option<DetectedFormat> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();

        // Check CBZ explicitly before generic ZIP
        if ext == "cbz" && header.len() >= 4 && &header[0..4] == b"PK\x03\x04" {
            return Some(DetectedFormat::Cbz);
        }

        if header.len() >= 5 && &header[0..5] == b"%PDF-" {
            return Some(DetectedFormat::Pdf);
        }

        if header.len() >= 3 && header[0..3] == [0xFF, 0xD8, 0xFF] {
            return Some(DetectedFormat::Jpeg);
        }

        if header.len() >= 8 && header[0..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
            return Some(DetectedFormat::Png);
        }

        if header.len() >= 12 && &header[0..4] == b"RIFF" && &header[8..12] == b"WEBP" {
            return Some(DetectedFormat::WebP);
        }

        if header.len() >= 6 && (&header[0..6] == b"GIF87a" || &header[0..6] == b"GIF89a") {
            return Some(DetectedFormat::Gif);
        }

        if header.len() >= 4
            && (&header[0..4] == b"II*\x00" || &header[0..4] == b"MM\x00*")
        {
            return Some(DetectedFormat::Tiff);
        }

        if header.len() >= 2 && &header[0..2] == b"BM" {
            return Some(DetectedFormat::Bmp);
        }

        if header.len() >= 4 && &header[0..4] == b"qoif" {
            return Some(DetectedFormat::Qoi);
        }

        if header.len() >= 4 && header[0..4] == [0x76, 0x2F, 0x31, 0x01] {
            return Some(DetectedFormat::OpenExr);
        }

        if header.len() >= 4 && &header[0..4] == b"DDS " {
            return Some(DetectedFormat::Dds);
        }

        if header.len() >= 10 && &header[0..10] == b"#?RADIANCE" {
            return Some(DetectedFormat::Hdr);
        }

        // Check SVG text header
        if ext == "svg" || ext == "svgz" {
            return Some(DetectedFormat::Svg);
        }
        if let Ok(prefix) = std::str::from_utf8(&header[..header.len().min(256)]) {
            if prefix.contains("<svg") {
                return Some(DetectedFormat::Svg);
            }
        }

        // Fallback to extension for formats without simple fixed 4-byte magic (e.g. TGA, ICO)
        match ext.as_str() {
            "jpg" | "jpeg" => Some(DetectedFormat::Jpeg),
            "png" | "apng" => Some(DetectedFormat::Png),
            "webp" => Some(DetectedFormat::WebP),
            "gif" => Some(DetectedFormat::Gif),
            "tif" | "tiff" => Some(DetectedFormat::Tiff),
            "bmp" => Some(DetectedFormat::Bmp),
            "ico" | "cur" => Some(DetectedFormat::Ico),
            "tga" => Some(DetectedFormat::Tga),
            "pnm" | "ppm" | "pgm" | "pbm" | "pam" => Some(DetectedFormat::Pnm),
            "qoi" => Some(DetectedFormat::Qoi),
            "hdr" => Some(DetectedFormat::Hdr),
            "exr" => Some(DetectedFormat::OpenExr),
            "dds" => Some(DetectedFormat::Dds),
            "avif" => Some(DetectedFormat::Avif),
            "heic" | "heif" => Some(DetectedFormat::Heic),
            "jxl" => Some(DetectedFormat::JpegXl),
            "pdf" => Some(DetectedFormat::Pdf),
            "cbz" => Some(DetectedFormat::Cbz),
            "dng" | "cr2" | "nef" | "arw" => Some(DetectedFormat::Raw),
            _ => None,
        }
    }

    pub fn is_supported_image_extension(path: &Path) -> bool {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return false;
        };
        let lower = ext.to_ascii_lowercase();
        matches!(
            lower.as_str(),
            "jpg"
                | "jpeg"
                | "png"
                | "webp"
                | "gif"
                | "tif"
                | "tiff"
                | "bmp"
                | "ico"
                | "tga"
                | "pnm"
                | "ppm"
                | "pgm"
                | "pbm"
                | "qoi"
                | "hdr"
                | "exr"
                | "dds"
                | "svg"
        )
    }
}
