use aether_core::{natural_cmp, AetherError, AetherResult, DecodedFrame};
use aether_image::{FormatRegistry, ImageEngine};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

/// Maximum uncompressed page size allowed from a CBZ entry (128 MB) to guard against ZIP bombs.
const MAX_ENTRY_UNCOMPRESSED_BYTES: u64 = 128 * 1024 * 1024;
/// Maximum compression ratio allowed before flagging a suspicious archive entry.
const MAX_COMPRESSION_RATIO: u64 = 150;

#[derive(Debug, Clone)]
pub struct CbzPageEntry {
    pub archive_index: usize,
    pub display_name: String,
    pub uncompressed_size: u64,
}

#[derive(Debug, Clone)]
pub struct CbzDocument {
    pub archive_path: PathBuf,
    pub pages: Vec<CbzPageEntry>,
}

impl CbzDocument {
    /// Opens a CBZ archive without extracting to disk, validates paths against traversal,
    /// filters supported images, and sorts pages naturally (1.jpg, 2.jpg, 10.jpg).
    pub fn open(path: &Path) -> AetherResult<Self> {
        let file = File::open(path).map_err(|source| AetherError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let reader = BufReader::new(file);
        let mut archive = ZipArchive::new(reader)
            .map_err(|e| AetherError::ArchiveError(format!("Invalid CBZ/ZIP archive: {}", e)))?;

        let mut pages = Vec::new();

        for idx in 0..archive.len() {
            let entry = archive
                .by_index(idx)
                .map_err(|e| AetherError::ArchiveError(e.to_string()))?;

            if entry.is_dir() {
                continue;
            }

            // Security: Reject path traversal attempts (`..` or leading `/`)
            let Some(enclosed_path) = entry.enclosed_name() else {
                continue;
            };

            if !FormatRegistry::is_supported_image_extension(&enclosed_path) {
                continue;
            }

            let uncompressed = entry.size();
            let compressed = entry.compressed_size().max(1);

            if uncompressed > MAX_ENTRY_UNCOMPRESSED_BYTES {
                return Err(AetherError::ArchiveError(format!(
                    "Archive entry '{}' exceeds safety limit ({} bytes)",
                    enclosed_path.display(),
                    uncompressed
                )));
            }

            if uncompressed / compressed > MAX_COMPRESSION_RATIO {
                return Err(AetherError::ArchiveError(format!(
                    "Suspicious compression ratio on entry '{}' (potential ZIP bomb)",
                    enclosed_path.display()
                )));
            }

            pages.push(CbzPageEntry {
                archive_index: idx,
                display_name: enclosed_path.to_string_lossy().to_string(),
                uncompressed_size: uncompressed,
            });
        }

        pages.sort_by(|a, b| natural_cmp(&a.display_name, &b.display_name));

        if pages.is_empty() {
            return Err(AetherError::ArchiveError(
                "CBZ archive contains no supported comic page images".into(),
            ));
        }

        Ok(Self {
            archive_path: path.to_path_buf(),
            pages,
        })
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Reads and decodes a single page on demand directly into memory without temp files.
    pub fn decode_page(&self, page_number: usize, engine: &ImageEngine) -> AetherResult<DecodedFrame> {
        let entry_meta = self.pages.get(page_number).ok_or_else(|| {
            AetherError::ArchiveError(format!("Page index {} out of bounds", page_number))
        })?;

        let file = File::open(&self.archive_path).map_err(|source| AetherError::Io {
            path: self.archive_path.clone(),
            source,
        })?;
        let reader = BufReader::new(file);
        let mut archive = ZipArchive::new(reader)
            .map_err(|e| AetherError::ArchiveError(e.to_string()))?;

        let zip_file = archive
            .by_index(entry_meta.archive_index)
            .map_err(|e| AetherError::ArchiveError(e.to_string()))?;

        let mut buf = Vec::with_capacity(entry_meta.uncompressed_size as usize);
        zip_file
            .take(MAX_ENTRY_UNCOMPRESSED_BYTES)
            .read_to_end(&mut buf)
            .map_err(|e| AetherError::ArchiveError(e.to_string()))?;

        let virtual_path = self.archive_path.join(&entry_meta.display_name);
        engine.decode_bytes(&buf, &virtual_path)
    }
}
