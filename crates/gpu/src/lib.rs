use aether_core::DecodedFrame;
use iced::widget::image::Handle as ImageHandle;
use lru::LruCache;
use parking_lot::Mutex;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GpuTextureKey {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub edit_revision: u64,
    pub is_preview: bool,
}

#[derive(Debug, Clone)]
pub struct CachedGpuSurface {
    pub handle: ImageHandle,
    pub width: u32,
    pub height: u32,
    pub vram_bytes: usize,
}

/// Manages GPU-backed image handles so the same decoded frame is never re-uploaded on every frame.
/// Implements strict VRAM budget tracking with LRU eviction.
pub struct GpuResourceManager {
    cache: Mutex<LruCache<GpuTextureKey, CachedGpuSurface>>,
    current_vram_bytes: Mutex<usize>,
    max_vram_bytes: usize,
}

impl GpuResourceManager {
    pub fn new(max_vram_mb: usize) -> Self {
        let cap = NonZeroUsize::new(256).expect("non-zero capacity");
        Self {
            cache: Mutex::new(LruCache::new(cap)),
            current_vram_bytes: Mutex::new(0),
            max_vram_bytes: max_vram_mb.saturating_mul(1024 * 1024),
        }
    }

    pub fn get_or_upload(
        &self,
        frame: &DecodedFrame,
        edit_revision: u64,
    ) -> CachedGpuSurface {
        let key = GpuTextureKey {
            path: frame.metadata.path.clone(),
            width: frame.width,
            height: frame.height,
            edit_revision,
            is_preview: frame.is_preview,
        };

        {
            let mut cache = self.cache.lock();
            if let Some(existing) = cache.get(&key) {
                return existing.clone();
            }
        }

        let vram_bytes = (frame.width as usize)
            .saturating_mul(frame.height as usize)
            .saturating_mul(4);

        self.evict_until_fits(vram_bytes);

        let handle = ImageHandle::from_rgba(
            frame.width,
            frame.height,
            frame.rgba_pixels.as_ref().clone(),
        );

        let surface = CachedGpuSurface {
            handle,
            width: frame.width,
            height: frame.height,
            vram_bytes,
        };

        let mut cache = self.cache.lock();
        let mut used = self.current_vram_bytes.lock();
        if let Some((_, evicted)) = cache.push(key, surface.clone()) {
            *used = used.saturating_sub(evicted.vram_bytes);
        }
        *used = used.saturating_add(vram_bytes);

        surface
    }

    pub fn invalidate_path(&self, path: &Path) {
        let mut cache = self.cache.lock();
        let mut used = self.current_vram_bytes.lock();
        let keys_to_remove: Vec<GpuTextureKey> = cache
            .iter()
            .filter(|(k, _)| k.path == path)
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys_to_remove {
            if let Some(removed) = cache.pop(&key) {
                *used = used.saturating_sub(removed.vram_bytes);
            }
        }
    }

    fn evict_until_fits(&self, needed_bytes: usize) {
        let mut cache = self.cache.lock();
        let mut used = self.current_vram_bytes.lock();

        while *used + needed_bytes > self.max_vram_bytes && !cache.is_empty() {
            if let Some((_, removed)) = cache.pop_lru() {
                *used = used.saturating_sub(removed.vram_bytes);
            } else {
                break;
            }
        }
    }

    pub fn vram_usage_mb(&self) -> f64 {
        let used = *self.current_vram_bytes.lock();
        used as f64 / (1024.0 * 1024.0)
    }

    pub fn cached_texture_count(&self) -> usize {
        self.cache.lock().len()
    }
}

pub type SharedGpuManager = Arc<GpuResourceManager>;
