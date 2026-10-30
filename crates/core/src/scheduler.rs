use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    BackgroundThumbnail = 1,
    VisibleThumbnail = 2,
    NearbyPreload = 3,
    UserOperation = 4,
    CurrentPdfPage = 5,
    CurrentImageRender = 6,
}

pub struct TaskScheduler {
    generation: AtomicU64,
    active_navigation_token: Mutex<CancellationToken>,
    active_ocr_token: Mutex<CancellationToken>,
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskScheduler {
    pub fn new() -> Self {
        Self {
            generation: AtomicU64::new(1),
            active_navigation_token: Mutex::new(CancellationToken::new()),
            active_ocr_token: Mutex::new(CancellationToken::new()),
        }
    }

    /// Cancels any obsolete image/page loading task and issues a fresh generation ID + token.
    pub fn next_navigation_epoch(&self) -> (u64, CancellationToken) {
        let mut guard = self.active_navigation_token.lock();
        guard.cancel();
        let next_token = CancellationToken::new();
        *guard = next_token.clone();
        let epoch_gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        (epoch_gen, next_token)
    }

    pub fn current_generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn start_ocr_job(&self) -> CancellationToken {
        let mut guard = self.active_ocr_token.lock();
        guard.cancel();
        let token = CancellationToken::new();
        *guard = token.clone();
        token
    }

    pub fn cancel_ocr_job(&self) {
        self.active_ocr_token.lock().cancel();
    }
}

pub type SharedScheduler = Arc<TaskScheduler>;
