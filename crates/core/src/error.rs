use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AetherError {
    #[error("I/O error on '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Unsupported file format or corrupted header: {0}")]
    UnsupportedFormat(String),

    #[error("Image decode failure: {0}")]
    DecodeError(String),

    #[error("Image dimensions ({width}x{height}) exceed safety limit ({max_dim}x{max_dim})")]
    DimensionsExceeded {
        width: u32,
        height: u32,
        max_dim: u32,
    },

    #[error("Archive security or extraction error: {0}")]
    ArchiveError(String),

    #[error("PDF engine error: {0}")]
    PdfError(String),

    #[error("OCR engine error: {0}")]
    OcrError(String),

    #[error("GPU resource error: {0}")]
    GpuError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Operation cancelled")]
    Cancelled,
}

pub type AetherResult<T> = Result<T, AetherError>;
