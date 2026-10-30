use aether_core::DecodedFrame;
use image::{ImageFormat, RgbaImage};
use lru::LruCache;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::fs;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

pub struct ImageCacheSystem {
    memory_full: Mutex<LruCache<String, DecodedFrame>>,
    memory_thumbs: Mutex<LruCache<String, Arc<Vec<u8>>>>,
    current_ram_bytes: Mutex<usize>,
    max_ram_bytes: usize,
    disk_cache_dir: PathBuf,
}

impl ImageCacheSystem {
    pub fn new(max_ram_mb: usize, disk_cache_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&disk_cache_dir);
        let full_cap = NonZeroUsize::new(64).expect("non-zero");
        let thumb_cap = NonZeroUsize::new(512).expect("non-zero");

        Self {
            memory_full: Mutex::new(LruCache::new(full_cap)),
            memory_thumbs: Mutex::new(LruCache::new(thumb_cap)),
            current_ram_bytes: Mutex::new(0),
            max_ram_bytes: max_ram_mb.saturating_mul(1024 * 1024),
            disk_cache_dir,
        }
    }

    /// Computes a collision-free cache key incorporating canonical path, byte length, and mtime.
    pub fn compute_file_key(path: &Path) -> String {
        let mut hasher = Sha256::new();
        hasher.update(path.to_string_lossy().as_bytes());
        if let Ok(meta) = fs::metadata(path) {
            hasher.update(meta.len().to_le_bytes());
            if let Ok(modified) = meta.modified() {
                if let Ok(dur) = modified.duration_since(UNIX_EPOCH) {
                    hasher.update(dur.as_nanos().to_le_bytes());
                }
            }
        }
        let digest = hasher.finalize();
        format!("{:x}", digest)
    }

    pub fn get_full(&self, key: &str) -> Option<DecodedFrame> {
        self.memory_full.lock().get(key).cloned()
    }

    pub fn insert_full(&self, key: String, frame: DecodedFrame) {
        let frame_bytes = frame.byte_len();
        let mut cache = self.memory_full.lock();
        let mut ram = self.current_ram_bytes.lock();

        while *ram + frame_bytes > self.max_ram_bytes && !cache.is_empty() {
            if let Some((_, evicted)) = cache.pop_lru() {
                *ram = ram.saturating_sub(evicted.byte_len());
            } else {
                break;
            }
        }

        if let Some((_, prev)) = cache.push(key, frame) {
            *ram = ram.saturating_sub(prev.byte_len());
        }
        *ram = ram.saturating_add(frame_bytes);
    }

    pub fn get_or_load_disk_thumbnail(
        &self,
        key: &str,
        thumb_size: u32,
    ) -> Option<(u32, u32, Arc<Vec<u8>>)> {
        let thumb_key = format!("{}_{}", key, thumb_size);
        let disk_path = self.disk_cache_dir.join(format!("{}.png", thumb_key));

        if disk_path.exists() {
            if let Ok(img) = image::open(&disk_path) {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                let raw = Arc::new(rgba.into_raw());
                self.memory_thumbs.lock().put(thumb_key, raw.clone());
                return Some((w, h, raw));
            }
        }
        None
    }

    pub fn store_disk_thumbnail(
        &self,
        key: &str,
        thumb_size: u32,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) {
        let thumb_key = format!("{}_{}", key, thumb_size);
        let disk_path = self.disk_cache_dir.join(format!("{}.png", thumb_key));
        if let Some(img_buf) = RgbaImage::from_raw(width, height, rgba.to_vec()) {
            let _ = img_buf.save_with_format(&disk_path, ImageFormat::Png);
        }
    }

    pub fn ram_usage_mb(&self) -> f64 {
        (*self.current_ram_bytes.lock()) as f64 / (1024.0 * 1024.0)
    }
}
