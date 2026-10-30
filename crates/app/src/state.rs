use aether_archive::CbzDocument;
use aether_core::{
    natural_cmp, AppConfig, AppMode, DecodedFrame, OcrLanguageMode, ReadingDirection,
    SharedScheduler, TaskScheduler, ZoomMode,
};
use aether_editing::{
    AspectRatioPreset, EditOperation, ExportFormatChoice, NonDestructivePipeline,
    ProcessingPipelineMode, ResampleFilter,
};
use aether_gpu::{CachedGpuSurface, GpuResourceManager, SharedGpuManager};
use aether_image::{DetectedFormat, FormatRegistry, ImageCacheSystem, ImageEngine};
use aether_ocr::{OcrDocumentResult, OcrEngine};
use aether_pdf::PdfDocumentHandle;
use aether_platform::{
    open_directory_dialog, open_file_dialog, save_file_dialog, PlatformDiagnostics,
};
use iced::widget::image::Handle as ImageHandle;
use iced::{Subscription, Task, Theme};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveContentType {
    None,
    Image,
    Pdf,
    Cbz,
}

#[derive(Debug, Clone)]
pub enum Message {
    // File & Directory Operations
    RequestOpenFiles,
    RequestOpenDirectory,
    FilesSelected(Option<Vec<PathBuf>>),
    DirectorySelected(Option<PathBuf>),

    // Wayland Native Drag & Drop
    FileHovered(PathBuf),
    FileDropped(PathBuf),
    FilesHoveredLeft,

    // Async Image & PDF Decoding Lifecycle
    PreviewDecoded {
        generation: u64,
        frame: Result<DecodedFrame, String>,
    },
    FullImageDecoded {
        generation: u64,
        frame: Result<DecodedFrame, String>,
    },
    PdfPageRendered {
        generation: u64,
        page_index: usize,
        frame: Result<DecodedFrame, String>,
    },
    PdfPrefetchFinished,
    ThumbnailReady {
        path: PathBuf,
        width: u32,
        height: u32,
        rgba: Arc<Vec<u8>>,
    },
    ScheduleNextThumbnailBatch,

    // Navigation & View Controls
    NextItem,
    PrevItem,
    SelectIndex(usize),
    SetMode(AppMode),
    ToggleSidebar,
    ToggleInfoPanel,
    ToggleVerticalScrollMode,
    VerticalScrollChanged(f32),
    VerticalPageReady {
        page_index: usize,
        frame: DecodedFrame,
    },
    ToggleDiagnosticsModal,
    ToggleFullscreen,

    // Interactive Viewport Zoom & 1:1 Cursor Pan
    ZoomIn,
    ZoomOut,
    SetZoomMode(ZoomMode),
    ViewportZoomAt {
        scale: f32,
        pan_x: f32,
        pan_y: f32,
    },
    ViewportPan {
        pan_x: f32,
        pan_y: f32,
    },

    // Interactive Mouse Crop & Adobe-Style Scrubbable Numbers
    InteractiveCropChanged {
        x: u32,
        y: u32,
        w: u32,
        h: u32,
    },
    ScrubCropX(u32),
    ScrubCropY(u32),
    ScrubCropW(u32),
    ScrubCropH(u32),
    ScrubResizeW(u32),
    ScrubResizeH(u32),
    ApplyEdit(EditOperation),
    SetCropAspect(AspectRatioPreset),
    CommitCrop,
    ResetCropBoxToFull,
    SetResizeFilter(ResampleFilter),
    SetExportFormat(ExportFormatChoice),
    SetProcessingPipeline(ProcessingPipelineMode),
    ToggleKeepAspect(bool),
    ApplyResize,
    UndoEdit,
    RedoEdit,
    ResetEdits,
    RequestSaveAs,
    SaveImageTo(Option<PathBuf>),

    // PDF Mode Operations
    PdfMovePageUp(usize),
    PdfMovePageDown(usize),
    PdfJumpToPageInput(String),
    PdfRequestSave,
    PdfSaveTo(Option<PathBuf>),

    // CBZ Mode Operations
    ToggleReadingDirection,
    ToggleTwoPageSpread,

    // OCR Engine Operations
    RunOcrCurrent,
    CancelOcr,
    OcrFinished(Result<OcrDocumentResult, String>),
    SelectOcrBlock(usize),
    ToggleOcrOverlays(bool),
    CycleOcrLanguage,
    CopyAllOcrText,
    CopySelectedOcrBlock,
    RequestExportSearchablePdf,
    ExportSearchablePdfTo(Option<PathBuf>),
}

pub struct AetherApp {
    pub config: AppConfig,
    pub diagnostics: PlatformDiagnostics,
    pub show_diagnostics_modal: bool,
    pub content_type: ActiveContentType,
    pub mode: AppMode,
    pub is_fullscreen: bool,
    pub is_dragging_files: bool,
    pub hovered_drop_files: Vec<PathBuf>,

    // Core Subsystems
    pub scheduler: SharedScheduler,
    pub gpu_manager: SharedGpuManager,
    pub image_engine: Arc<ImageEngine>,
    pub ocr_engine: Arc<OcrEngine>,

    // Active Directory / File List & Lazy Sidebar Thumbnail Queue
    pub directory_files: Vec<PathBuf>,
    pub current_index: usize,
    pub thumbnails: std::collections::HashMap<PathBuf, ImageHandle>,
    pub pending_thumbnails: Vec<PathBuf>,

    // Active Image & GPU Surface
    pub base_frame: Option<DecodedFrame>,
    pub active_frame: Option<DecodedFrame>,
    pub active_surface: Option<CachedGpuSurface>,
    pub secondary_surface: Option<CachedGpuSurface>,
    pub status_message: String,
    pub error_banner: Option<String>,

    // Zoom & Clamped Pan State
    pub zoom_mode: ZoomMode,
    pub zoom_scale: f32,
    pub pan_x: f32,
    pub pan_y: f32,

    // Editing State
    pub edit_pipeline: NonDestructivePipeline,
    pub crop_aspect: AspectRatioPreset,
    pub interactive_crop_rect: Option<(u32, u32, u32, u32)>,
    pub resize_w_val: u32,
    pub resize_h_val: u32,
    pub resize_keep_aspect: bool,
    pub resize_filter: ResampleFilter,
    pub export_format: ExportFormatChoice,
    pub processing_pipeline: ProcessingPipelineMode,

    // PDF & CBZ Continuous Vertical Scroll State
    pub vertical_scroll_mode: bool,
    pub vertical_page_handles: std::collections::BTreeMap<usize, CachedGpuSurface>,
    pub pdf_doc: Option<PdfDocumentHandle>,
    pub pdf_page_index: usize,
    pub pdf_page_input: String,

    // CBZ Comic Reader State
    pub cbz_doc: Option<CbzDocument>,
    pub cbz_page_index: usize,
    pub cbz_direction: ReadingDirection,
    pub cbz_two_page: bool,

    // OCR State
    pub ocr_running: bool,
    pub ocr_result: Option<OcrDocumentResult>,
    pub selected_ocr_block: Option<usize>,
    pub show_ocr_overlays: bool,
}

impl AetherApp {
    pub fn boot() -> (Self, Task<Message>) {
        let mut config = AppConfig::load();
        config.show_sidebar = false;
        config.show_info_panel = false;

        let diagnostics = PlatformDiagnostics::detect(config.gpu_backend);
        let scheduler = Arc::new(TaskScheduler::new());
        let gpu_manager = Arc::new(GpuResourceManager::new(config.vram_limit_mb));
        let cache = Arc::new(ImageCacheSystem::new(
            config.memory_cache_limit_mb,
            AppConfig::cache_dir_path(),
        ));
        let image_engine = Arc::new(ImageEngine::new(cache, config.max_image_dimension));
        let ocr_engine = Arc::new(OcrEngine::new(
            config.ocr_models_dir.clone(),
            config.ocr_use_cuda,
        ));

        let mut app = Self {
            zoom_mode: ZoomMode::FitWindow,
            cbz_direction: config.default_cbz_direction,
            cbz_two_page: config.two_page_spread,
            config,
            diagnostics,
            show_diagnostics_modal: false,
            content_type: ActiveContentType::None,
            mode: AppMode::ImageViewer,
            is_fullscreen: false,
            is_dragging_files: false,
            hovered_drop_files: Vec::new(),
            scheduler,
            gpu_manager,
            image_engine,
            ocr_engine,
            directory_files: Vec::new(),
            current_index: 0,
            thumbnails: std::collections::HashMap::new(),
            pending_thumbnails: Vec::new(),
            base_frame: None,
            active_frame: None,
            active_surface: None,
            secondary_surface: None,
            status_message: "Ready — Drop files or press Ctrl+O (F12: System Report)".to_string(),
            error_banner: None,
            zoom_scale: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            edit_pipeline: NonDestructivePipeline::new(),
            crop_aspect: AspectRatioPreset::Freeform,
            interactive_crop_rect: None,
            resize_w_val: 1920,
            resize_h_val: 1080,
            resize_keep_aspect: true,
            resize_filter: ResampleFilter::Lanczos3,
            export_format: ExportFormatChoice::Png,
            processing_pipeline: ProcessingPipelineMode::GpuAccelerated,
            vertical_scroll_mode: false,
            vertical_page_handles: std::collections::BTreeMap::new(),
            pdf_doc: None,
            pdf_page_index: 0,
            pdf_page_input: "1".into(),
            cbz_doc: None,
            cbz_page_index: 0,
            ocr_running: false,
            ocr_result: None,
            selected_ocr_block: None,
            show_ocr_overlays: true,
        };

        let cli_paths: Vec<PathBuf> = env::args()
            .skip(1)
            .map(PathBuf::from)
            .filter(|p| p.exists() && p.file_name().and_then(|s| s.to_str()) != Some("View"))
            .collect();

        if let Some(first) = cli_paths.first() {
            let task = if first.is_dir() {
                app.open_directory(first)
            } else {
                app.load_path_command(first.clone())
            };
            return (app, task);
        }

        (app, Task::none())
    }

    pub fn title(&self) -> String {
        match &self.active_frame {
            Some(frame) => format!(
                "{} ({}×{}) — AetherView",
                frame.metadata.file_name, frame.width, frame.height
            ),
            None => "AetherView".to_string(),
        }
    }

    pub fn theme(&self) -> Theme {
        crate::theme::resolve_theme(self.config.theme)
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::none()
    }

    /// Auto-fits the image whenever layout panels open/close or modes switch.
    fn trigger_auto_fit(&mut self) {
        self.zoom_mode = ZoomMode::FitWindow;
        self.pan_x = 0.0;
        self.pan_y = 0.0;
    }

    fn init_full_image_crop_border(&mut self) {
        if let Some(frame) = &self.active_frame {
            self.interactive_crop_rect = Some((0, 0, frame.width.max(1), frame.height.max(1)));
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::RequestOpenFiles => Task::perform(open_file_dialog(), Message::FilesSelected),
            Message::RequestOpenDirectory => {
                Task::perform(open_directory_dialog(), Message::DirectorySelected)
            }
            Message::FilesSelected(Some(paths)) => {
                if let Some(first) = paths.first().cloned() {
                    if paths.len() > 1 {
                        self.directory_files = paths;
                        self.current_index = 0;
                    } else {
                        self.directory_files.clear();
                    }
                    return self.load_path_command(first);
                }
                Task::none()
            }
            Message::FilesSelected(None) | Message::DirectorySelected(None) => Task::none(),
            Message::DirectorySelected(Some(dir)) => self.open_directory(&dir),

            // Wayland Drag & Drop
            Message::FileHovered(path) => {
                self.is_dragging_files = true;
                if !self.hovered_drop_files.contains(&path) {
                    self.hovered_drop_files.push(path);
                }
                Task::none()
            }
            Message::FilesHoveredLeft => {
                self.is_dragging_files = false;
                self.hovered_drop_files.clear();
                Task::none()
            }
            Message::FileDropped(path) => {
                self.is_dragging_files = false;
                self.hovered_drop_files.clear();
                if path.is_dir() {
                    self.open_directory(&path)
                } else {
                    self.directory_files.clear();
                    self.load_path_command(path)
                }
            }

            // Progressive Image Loading
            Message::PreviewDecoded {
                generation,
                frame: Ok(preview_frame),
            } => {
                if generation != self.scheduler.current_generation() {
                    return Task::none();
                }
                if self.base_frame.is_none() {
                    let surface = self.gpu_manager.get_or_upload(&preview_frame, 0);
                    self.active_surface = Some(surface);
                    self.active_frame = Some(preview_frame);
                    self.trigger_auto_fit();
                    self.status_message = "Preview ready — Decoding full resolution...".into();
                }
                Task::none()
            }
            Message::PreviewDecoded { .. } => Task::none(),

            Message::FullImageDecoded {
                generation,
                frame: Ok(full_frame),
            } => {
                if generation != self.scheduler.current_generation() {
                    return Task::none();
                }
                self.error_banner = None;
                self.resize_w_val = full_frame.width;
                self.resize_h_val = full_frame.height;
                self.interactive_crop_rect =
                    Some((0, 0, full_frame.width.max(1), full_frame.height.max(1)));

                let surface = self.gpu_manager.get_or_upload(&full_frame, 0);
                self.status_message = format!(
                    "{} ({}×{} • {})",
                    full_frame.metadata.file_name,
                    full_frame.width,
                    full_frame.height,
                    full_frame.metadata.format_name
                );
                self.base_frame = Some(full_frame.clone());
                self.active_frame = Some(full_frame);
                self.active_surface = Some(surface);
                self.trigger_auto_fit();
                self.edit_pipeline.reset();

                if self.config.show_sidebar {
                    self.enqueue_missing_thumbnails();
                    return self.spawn_next_thumbnail_batch();
                }
                Task::none()
            }
            Message::FullImageDecoded {
                generation,
                frame: Err(err),
            } => {
                if generation == self.scheduler.current_generation() {
                    self.error_banner = Some(err.clone());
                    self.status_message = format!("Decode error: {}", err);
                }
                Task::none()
            }

            Message::PdfPageRendered {
                generation,
                page_index,
                frame: Ok(frame),
            } => {
                if generation != self.scheduler.current_generation() {
                    return Task::none();
                }
                let surface = self.gpu_manager.get_or_upload(&frame, page_index as u64);
                self.active_surface = Some(surface);
                self.base_frame = Some(frame.clone());
                self.active_frame = Some(frame);
                self.secondary_surface = None;
                self.trigger_auto_fit();
                if let Some(pdf) = &self.pdf_doc {
                    self.status_message = format!(
                        "PDF Page {} of {} • {}",
                        page_index + 1,
                        pdf.page_count(),
                        pdf.active_engine_name
                    );
                }
                // Prefetch adjacent pages in background so next/prev page turns are 0ms
                self.spawn_pdf_prefetch(page_index)
            }
            Message::PdfPageRendered {
                generation,
                frame: Err(err),
                ..
            } => {
                if generation == self.scheduler.current_generation() {
                    self.error_banner = Some(err);
                }
                Task::none()
            }
            Message::PdfPrefetchFinished => Task::none(),

            Message::ThumbnailReady {
                path,
                width,
                height,
                rgba,
            } => {
                let handle = ImageHandle::from_rgba(width, height, rgba.as_ref().clone());
                self.thumbnails.insert(path, handle);
                if self.config.show_sidebar {
                    return self.spawn_next_thumbnail_batch();
                }
                Task::none()
            }
            Message::ScheduleNextThumbnailBatch => {
                if self.config.show_sidebar {
                    return self.spawn_next_thumbnail_batch();
                }
                Task::none()
            }

            // Navigation & Layout Panels (Auto-Fit on every layout change!)
            Message::NextItem => self.navigate_step(1),
            Message::PrevItem => self.navigate_step(-1),
            Message::SelectIndex(idx) => self.select_index(idx),
            Message::SetMode(mode) => {
                if self.content_type == ActiveContentType::None {
                    return Task::none();
                }
                self.mode = mode;
                self.trigger_auto_fit();
                if mode == AppMode::ImageEditor {
                    self.init_full_image_crop_border();
                }
                Task::none()
            }
            Message::ToggleSidebar => {
                self.config.show_sidebar = !self.config.show_sidebar;
                self.trigger_auto_fit();
                if self.config.show_sidebar && self.content_type == ActiveContentType::Image {
                    self.enqueue_missing_thumbnails();
                    return self.spawn_next_thumbnail_batch();
                } else {
                    self.pending_thumbnails.clear();
                }
                Task::none()
            }
            Message::ToggleInfoPanel => {
                self.config.show_info_panel = !self.config.show_info_panel;
                self.trigger_auto_fit();
                Task::none()
            }
            Message::ToggleDiagnosticsModal => {
                self.show_diagnostics_modal = !self.show_diagnostics_modal;
                if self.show_diagnostics_modal {
                    self.diagnostics = PlatformDiagnostics::detect(self.config.gpu_backend);
                }
                Task::none()
            }
            Message::ToggleVerticalScrollMode => {
                if matches!(self.content_type, ActiveContentType::Pdf | ActiveContentType::Cbz) {
                    self.vertical_scroll_mode = !self.vertical_scroll_mode;
                    if self.vertical_scroll_mode {
                        return self.spawn_vertical_window_loads(self.current_doc_page_index());
                    }
                }
                Task::none()
            }
            Message::VerticalScrollChanged(relative_y) => {
                let total = self.current_doc_page_count();
                if total > 0 {
                    let approx_page = ((relative_y.clamp(0.0, 1.0) * (total.saturating_sub(1) as f32))
                        .round() as usize)
                        .min(total.saturating_sub(1));
                    if self.content_type == ActiveContentType::Pdf {
                        self.pdf_page_index = approx_page;
                    } else if self.content_type == ActiveContentType::Cbz {
                        self.cbz_page_index = approx_page;
                    }
                    return self.spawn_vertical_window_loads(approx_page);
                }
                Task::none()
            }
            Message::VerticalPageReady { page_index, frame } => {
                let surface = self
                    .gpu_manager
                    .get_or_upload(&frame, (page_index as u64) + 10_000);
                self.vertical_page_handles.insert(page_index, surface);
                Task::none()
            }
            Message::ToggleFullscreen => {
                self.is_fullscreen = !self.is_fullscreen;
                self.trigger_auto_fit();
                let mode = if self.is_fullscreen {
                    iced::window::Mode::Fullscreen
                } else {
                    iced::window::Mode::Windowed
                };
                iced::window::get_latest().and_then(move |id| iced::window::change_mode(id, mode))
            }

            // Interactive Viewport Zoom & Pan (Unzooming snaps back to Auto-Fit!)
            Message::ZoomIn => {
                let base = if self.zoom_mode == ZoomMode::FitWindow {
                    0.85
                } else {
                    self.zoom_scale
                };
                self.zoom_mode = ZoomMode::Custom;
                self.zoom_scale = (base * 1.22).clamp(0.1, 32.0);
                Task::none()
            }
            Message::ZoomOut => {
                let next = self.zoom_scale / 1.22;
                if self.zoom_mode == ZoomMode::FitWindow || next <= 0.88 {
                    self.trigger_auto_fit();
                } else {
                    self.zoom_mode = ZoomMode::Custom;
                    self.zoom_scale = next;
                }
                Task::none()
            }
            Message::SetZoomMode(mode) => {
                self.zoom_mode = mode;
                self.pan_x = 0.0;
                self.pan_y = 0.0;
                if mode == ZoomMode::ActualSize {
                    self.zoom_scale = 1.0;
                }
                Task::none()
            }
            Message::ViewportZoomAt {
                scale,
                pan_x,
                pan_y,
            } => {
                self.zoom_mode = ZoomMode::Custom;
                self.zoom_scale = scale;
                self.pan_x = pan_x;
                self.pan_y = pan_y;
                Task::none()
            }
            Message::ViewportPan { pan_x, pan_y } => {
                self.pan_x = pan_x;
                self.pan_y = pan_y;
                Task::none()
            }

            // Interactive Mouse Crop & Adobe-Style Scrubbable Numbers
            Message::InteractiveCropChanged { x, y, w, h } => {
                self.interactive_crop_rect = Some((x, y, w, h));
                Task::none()
            }
            Message::ScrubCropX(nx) => {
                if let Some(frame) = &self.active_frame {
                    let (_, cy, cw, ch) = self
                        .interactive_crop_rect
                        .unwrap_or((0, 0, frame.width, frame.height));
                    let clamped_x = nx.min(frame.width.saturating_sub(1));
                    let clamped_w = cw.min(frame.width.saturating_sub(clamped_x)).max(1);
                    self.interactive_crop_rect = Some((clamped_x, cy, clamped_w, ch));
                }
                Task::none()
            }
            Message::ScrubCropY(ny) => {
                if let Some(frame) = &self.active_frame {
                    let (cx, _, cw, ch) = self
                        .interactive_crop_rect
                        .unwrap_or((0, 0, frame.width, frame.height));
                    let clamped_y = ny.min(frame.height.saturating_sub(1));
                    let clamped_h = ch.min(frame.height.saturating_sub(clamped_y)).max(1);
                    self.interactive_crop_rect = Some((cx, clamped_y, cw, clamped_h));
                }
                Task::none()
            }
            Message::ScrubCropW(nw) => {
                if let Some(frame) = &self.active_frame {
                    let (cx, cy, _, ch) = self
                        .interactive_crop_rect
                        .unwrap_or((0, 0, frame.width, frame.height));
                    let clamped_w = nw.clamp(1, frame.width.saturating_sub(cx));
                    self.interactive_crop_rect = Some((cx, cy, clamped_w, ch));
                }
                Task::none()
            }
            Message::ScrubCropH(nh) => {
                if let Some(frame) = &self.active_frame {
                    let (cx, cy, cw, _) = self
                        .interactive_crop_rect
                        .unwrap_or((0, 0, frame.width, frame.height));
                    let clamped_h = nh.clamp(1, frame.height.saturating_sub(cy));
                    self.interactive_crop_rect = Some((cx, cy, cw, clamped_h));
                }
                Task::none()
            }
            Message::ScrubResizeW(nw) => {
                let nw = nw.clamp(1, 16384);
                if self.resize_keep_aspect {
                    if let Some(frame) = &self.active_frame {
                        let ratio = frame.height as f32 / frame.width.max(1) as f32;
                        self.resize_h_val = ((nw as f32 * ratio).round() as u32).max(1);
                    }
                }
                self.resize_w_val = nw;
                Task::none()
            }
            Message::ScrubResizeH(nh) => {
                let nh = nh.clamp(1, 16384);
                if self.resize_keep_aspect {
                    if let Some(frame) = &self.active_frame {
                        let ratio = frame.width as f32 / frame.height.max(1) as f32;
                        self.resize_w_val = ((nh as f32 * ratio).round() as u32).max(1);
                    }
                }
                self.resize_h_val = nh;
                Task::none()
            }
            Message::ResetCropBoxToFull => {
                self.init_full_image_crop_border();
                Task::none()
            }
            Message::ApplyEdit(op) => {
                self.edit_pipeline.push(op);
                self.refresh_edited_surface();
                self.init_full_image_crop_border();
                self.trigger_auto_fit();
                Task::none()
            }
            Message::SetCropAspect(preset) => {
                self.crop_aspect = preset;
                if let Some(frame) = &self.active_frame {
                    let fw = frame.width.max(1);
                    let fh = frame.height.max(1);
                    let (w, h) = match preset {
                        AspectRatioPreset::Freeform => (fw, fh),
                        AspectRatioPreset::Square1x1 => {
                            let s = fw.min(fh);
                            (s, s)
                        }
                        AspectRatioPreset::Standard4x3 => {
                            let h_try = (fw * 3) / 4;
                            if h_try <= fh {
                                (fw, h_try)
                            } else {
                                ((fh * 4) / 3, fh)
                            }
                        }
                        AspectRatioPreset::Widescreen16x9 => {
                            let h_try = (fw * 9) / 16;
                            if h_try <= fh {
                                (fw, h_try)
                            } else {
                                ((fh * 16) / 9, fh)
                            }
                        }
                        AspectRatioPreset::Photo3x2 => {
                            let h_try = (fw * 2) / 3;
                            if h_try <= fh {
                                (fw, h_try)
                            } else {
                                ((fh * 3) / 2, fh)
                            }
                        }
                    };
                    let cx = (fw.saturating_sub(w)) / 2;
                    let cy = (fh.saturating_sub(h)) / 2;
                    self.interactive_crop_rect = Some((cx, cy, w.max(1), h.max(1)));
                }
                Task::none()
            }
            Message::CommitCrop => {
                if let Some((x, y, width, height)) = self.interactive_crop_rect {
                    self.edit_pipeline.push(EditOperation::Crop {
                        x,
                        y,
                        width,
                        height,
                    });
                    self.refresh_edited_surface();
                    self.init_full_image_crop_border();
                    self.trigger_auto_fit();
                }
                Task::none()
            }
            Message::SetResizeFilter(filter) => {
                self.resize_filter = filter;
                Task::none()
            }
            Message::SetExportFormat(fmt) => {
                self.export_format = fmt;
                Task::none()
            }
            Message::SetProcessingPipeline(pipe) => {
                self.processing_pipeline = pipe;
                Task::none()
            }
            Message::ToggleKeepAspect(keep) => {
                self.resize_keep_aspect = keep;
                Task::none()
            }
            Message::ApplyResize => {
                let width = self.resize_w_val.max(1);
                let height = self.resize_h_val.max(1);
                self.edit_pipeline.push(EditOperation::Resize {
                    width,
                    height,
                    filter: self.resize_filter,
                });
                self.refresh_edited_surface();
                self.init_full_image_crop_border();
                self.trigger_auto_fit();
                Task::none()
            }
            Message::UndoEdit => {
                if self.edit_pipeline.undo() {
                    self.refresh_edited_surface();
                    self.init_full_image_crop_border();
                    self.trigger_auto_fit();
                }
                Task::none()
            }
            Message::RedoEdit => {
                if self.edit_pipeline.redo() {
                    self.refresh_edited_surface();
                    self.init_full_image_crop_border();
                    self.trigger_auto_fit();
                }
                Task::none()
            }
            Message::ResetEdits => {
                self.edit_pipeline.reset();
                self.refresh_edited_surface();
                self.init_full_image_crop_border();
                self.trigger_auto_fit();
                Task::none()
            }
            Message::RequestSaveAs => {
                if self.active_frame.is_none() {
                    return Task::none();
                }
                let ext = self.export_format.extension();
                let stem = self
                    .active_frame
                    .as_ref()
                    .and_then(|f| {
                        Path::new(&f.metadata.file_name)
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                    })
                    .unwrap_or_else(|| "image".into());
                let default_name = format!("edited_{}.{}", stem, ext);
                Task::perform(
                    save_file_dialog(
                        default_name,
                        vec![ext, "png", "jpg", "webp", "tiff", "qoi"],
                    ),
                    Message::SaveImageTo,
                )
            }
            Message::SaveImageTo(Some(target)) => {
                if let Some(frame) = &self.active_frame {
                    match NonDestructivePipeline::export_frame(frame, &target, 92) {
                        Ok(()) => {
                            self.status_message =
                                format!("Exported image to '{}'", target.display());
                        }
                        Err(e) => {
                            self.error_banner = Some(e.to_string());
                        }
                    }
                }
                Task::none()
            }
            Message::SaveImageTo(None) => Task::none(),

            // PDF Reordering & Async Rendering
            Message::PdfMovePageUp(idx) => {
                if idx > 0 {
                    if let Some(pdf) = &mut self.pdf_doc {
                        if pdf.move_page(idx, idx - 1) {
                            self.pdf_page_index = idx - 1;
                            return self.spawn_pdf_page_render();
                        }
                    }
                }
                Task::none()
            }
            Message::PdfMovePageDown(idx) => {
                if let Some(pdf) = &mut self.pdf_doc {
                    if idx + 1 < pdf.page_count() && pdf.move_page(idx, idx + 1) {
                        self.pdf_page_index = idx + 1;
                        return self.spawn_pdf_page_render();
                    }
                }
                Task::none()
            }
            Message::PdfJumpToPageInput(val) => {
                self.pdf_page_input = val.clone();
                if let (Ok(page_num), Some(pdf)) = (val.parse::<usize>(), &self.pdf_doc) {
                    if page_num >= 1 && page_num <= pdf.page_count() {
                        self.pdf_page_index = page_num - 1;
                        return self.spawn_pdf_page_render();
                    }
                }
                Task::none()
            }
            Message::PdfRequestSave => {
                let default_name = self
                    .pdf_doc
                    .as_ref()
                    .map(|d| format!("reordered_{}", d.title))
                    .unwrap_or_else(|| "modified.pdf".into());
                Task::perform(
                    save_file_dialog(default_name, vec!["pdf"]),
                    Message::PdfSaveTo,
                )
            }
            Message::PdfSaveTo(Some(target)) => {
                if let Some(pdf) = &mut self.pdf_doc {
                    match pdf.save_reordered(&target) {
                        Ok(()) => {
                            self.status_message =
                                format!("Saved modified PDF structure to '{}'", target.display());
                        }
                        Err(e) => {
                            self.error_banner = Some(e.to_string());
                        }
                    }
                }
                Task::none()
            }
            Message::PdfSaveTo(None) => Task::none(),

            // CBZ Comic Controls
            Message::ToggleReadingDirection => {
                self.cbz_direction = match self.cbz_direction {
                    ReadingDirection::LeftToRight => ReadingDirection::RightToLeft,
                    ReadingDirection::RightToLeft => ReadingDirection::LeftToRight,
                };
                self.config.default_cbz_direction = self.cbz_direction;
                let _ = self.config.save();
                self.render_active_cbz_page();
                Task::none()
            }
            Message::ToggleTwoPageSpread => {
                self.cbz_two_page = !self.cbz_two_page;
                self.config.two_page_spread = self.cbz_two_page;
                let _ = self.config.save();
                self.render_active_cbz_page();
                Task::none()
            }

            // OCR Operations
            Message::RunOcrCurrent => {
                let Some(frame) = self.active_frame.clone() else {
                    return Task::none();
                };
                self.mode = AppMode::OcrInspector;
                self.trigger_auto_fit();
                self.ocr_running = true;
                self.status_message = "Running OCR (Arabic + English) on active surface...".into();

                let token = self.scheduler.start_ocr_job();
                let engine = self.ocr_engine.clone();
                let lang = self.config.ocr_language;

                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            engine
                                .recognize_frame(&frame, lang, &token)
                                .map_err(|e| e.to_string())
                        })
                        .await
                        .map_err(|e| e.to_string())?
                    },
                    Message::OcrFinished,
                )
            }
            Message::CancelOcr => {
                self.scheduler.cancel_ocr_job();
                self.ocr_running = false;
                self.status_message = "OCR job cancelled".into();
                Task::none()
            }
            Message::OcrFinished(Ok(result)) => {
                self.ocr_running = false;
                self.error_banner = None;
                self.status_message = format!(
                    "OCR finished in {} ms ({} text lines)",
                    result.inference_ms,
                    result.blocks.len()
                );
                self.selected_ocr_block = result.blocks.first().map(|b| b.id);
                self.ocr_result = Some(result);
                Task::none()
            }
            Message::OcrFinished(Err(err)) => {
                self.ocr_running = false;
                self.error_banner = Some(err);
                Task::none()
            }
            Message::SelectOcrBlock(id) => {
                self.selected_ocr_block = Some(id);
                Task::none()
            }
            Message::ToggleOcrOverlays(show) => {
                self.show_ocr_overlays = show;
                Task::none()
            }
            Message::CycleOcrLanguage => {
                self.config.ocr_language = match self.config.ocr_language {
                    OcrLanguageMode::ArabicAndEnglish => OcrLanguageMode::ArabicOnly,
                    OcrLanguageMode::ArabicOnly => OcrLanguageMode::EnglishOnly,
                    OcrLanguageMode::EnglishOnly => OcrLanguageMode::Auto,
                    OcrLanguageMode::Auto => OcrLanguageMode::ArabicAndEnglish,
                };
                let _ = self.config.save();
                Task::none()
            }
            Message::CopyAllOcrText => {
                if let Some(res) = &self.ocr_result {
                    self.status_message = "Copied full OCR text to Wayland clipboard".into();
                    return iced::clipboard::write(res.full_text.clone());
                }
                Task::none()
            }
            Message::CopySelectedOcrBlock => {
                if let (Some(res), Some(sel_id)) = (&self.ocr_result, self.selected_ocr_block) {
                    if let Some(block) = res.blocks.iter().find(|b| b.id == sel_id) {
                        self.status_message =
                            format!("Copied OCR Block #{} to Wayland clipboard", sel_id);
                        return iced::clipboard::write(block.text_logical.clone());
                    }
                }
                Task::none()
            }
            Message::RequestExportSearchablePdf => Task::perform(
                save_file_dialog("searchable_ocr_output.pdf".to_string(), vec!["pdf"]),
                Message::ExportSearchablePdfTo,
            ),
            Message::ExportSearchablePdfTo(Some(target)) => {
                if let Some(ocr) = &self.ocr_result {
                    match OcrEngine::export_searchable_pdf(ocr, &target) {
                        Ok(()) => {
                            self.status_message =
                                format!("Exported searchable PDF to '{}'", target.display());
                        }
                        Err(e) => {
                            self.error_banner = Some(e.to_string());
                        }
                    }
                }
                Task::none()
            }
            Message::ExportSearchablePdfTo(None) => Task::none(),
        }
    }

    fn enqueue_missing_thumbnails(&mut self) {
        if !self.config.show_sidebar {
            self.pending_thumbnails.clear();
            return;
        }
        let mut ordered: Vec<PathBuf> = Vec::with_capacity(self.directory_files.len());
        let total = self.directory_files.len();
        for offset in 0..total {
            if let Some(p) = self.directory_files.get(self.current_index + offset) {
                if !self.thumbnails.contains_key(p) {
                    ordered.push(p.clone());
                }
            }
            if offset > 0 && self.current_index >= offset {
                if let Some(p) = self.directory_files.get(self.current_index - offset) {
                    if !self.thumbnails.contains_key(p) {
                        ordered.push(p.clone());
                    }
                }
            }
        }
        ordered.reverse();
        self.pending_thumbnails = ordered;
    }

    fn spawn_next_thumbnail_batch(&mut self) -> Task<Message> {
        if !self.config.show_sidebar {
            return Task::none();
        }
        let Some(next_path) = self.pending_thumbnails.pop() else {
            return Task::none();
        };
        if self.thumbnails.contains_key(&next_path) {
            return self.spawn_next_thumbnail_batch();
        }

        let thumb_size = self.config.thumbnail_size;
        let eng = self.image_engine.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    eng.generate_thumbnail(&next_path, thumb_size)
                        .map(|(w, h, rgba)| (next_path, w, h, rgba))
                })
                .await
            },
            |res| match res {
                Ok(Ok((path, width, height, rgba))) => Message::ThumbnailReady {
                    path,
                    width,
                    height,
                    rgba,
                },
                _ => Message::ScheduleNextThumbnailBatch,
            },
        )
    }

    fn open_directory(&mut self, dir: &Path) -> Task<Message> {
        let Ok(entries) = fs::read_dir(dir) else {
            return Task::none();
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|d| d.path()))
            .filter(|p| p.is_file() && FormatRegistry::is_supported_image_extension(p))
            .collect();

        files.sort_by(|a, b| {
            natural_cmp(
                &a.file_name().unwrap_or_default().to_string_lossy(),
                &b.file_name().unwrap_or_default().to_string_lossy(),
            )
        });

        if let Some(first) = files.first().cloned() {
            self.directory_files = files;
            self.current_index = 0;
            return self.load_path_command(first);
        }
        Task::none()
    }

    fn load_path_command(&mut self, path: PathBuf) -> Task<Message> {
        self.error_banner = None;
        self.config.record_recent_file(&path);
        let _ = self.config.save();

        if self.directory_files.is_empty() || !self.directory_files.contains(&path) {
            if let Some(parent) = path.parent() {
                if let Ok(entries) = fs::read_dir(parent) {
                    let mut siblings: Vec<PathBuf> = entries
                        .filter_map(|e| e.ok().map(|d| d.path()))
                        .filter(|p| p.is_file() && FormatRegistry::is_supported_image_extension(p))
                        .collect();
                    siblings.sort_by(|a, b| {
                        natural_cmp(
                            &a.file_name().unwrap_or_default().to_string_lossy(),
                            &b.file_name().unwrap_or_default().to_string_lossy(),
                        )
                    });
                    if let Some(pos) = siblings.iter().position(|p| p == &path) {
                        self.current_index = pos;
                    }
                    self.directory_files = siblings;
                }
            }
        } else if let Some(pos) = self.directory_files.iter().position(|p| p == &path) {
            self.current_index = pos;
        }

        let mut header = [0u8; 64];
        if let Ok(mut f) = fs::File::open(&path) {
            let _ = std::io::Read::read(&mut f, &mut header);
        }

        self.vertical_page_handles.clear();

        match FormatRegistry::detect(&path, &header) {
            Some(DetectedFormat::Pdf) => {
                match PdfDocumentHandle::open(&path) {
                    Ok(doc) => {
                        let saved_page = self.config.get_bookmark(&path).unwrap_or(0);
                        self.pdf_page_index = saved_page.min(doc.page_count().saturating_sub(1));
                        self.pdf_page_input = (self.pdf_page_index + 1).to_string();
                        self.pdf_doc = Some(doc);
                        self.cbz_doc = None;
                        self.content_type = ActiveContentType::Pdf;
                        self.mode = AppMode::PdfViewer;
                        if self.vertical_scroll_mode {
                            let idx = self.pdf_page_index;
                            return Task::batch(vec![
                                self.spawn_pdf_page_render(),
                                self.spawn_vertical_window_loads(idx),
                            ]);
                        }
                        return self.spawn_pdf_page_render();
                    }
                    Err(e) => {
                        self.error_banner = Some(e.to_string());
                    }
                }
                Task::none()
            }
            Some(DetectedFormat::Cbz) => {
                match CbzDocument::open(&path) {
                    Ok(doc) => {
                        let saved_page = self.config.get_bookmark(&path).unwrap_or(0);
                        self.cbz_page_index = saved_page.min(doc.page_count().saturating_sub(1));
                        self.cbz_doc = Some(doc);
                        self.pdf_doc = None;
                        self.content_type = ActiveContentType::Cbz;
                        self.mode = AppMode::ComicReader;
                        self.render_active_cbz_page();
                        if self.vertical_scroll_mode {
                            let idx = self.cbz_page_index;
                            return self.spawn_vertical_window_loads(idx);
                        }
                    }
                    Err(e) => {
                        self.error_banner = Some(e.to_string());
                    }
                }
                Task::none()
            }
            _ => {
                self.pdf_doc = None;
                self.cbz_doc = None;
                self.content_type = ActiveContentType::Image;
                if matches!(self.mode, AppMode::PdfViewer | AppMode::ComicReader) {
                    self.mode = AppMode::ImageViewer;
                }

                let (generation, _token) = self.scheduler.next_navigation_epoch();
                self.base_frame = None;
                self.secondary_surface = None;

                let engine_full = self.image_engine.clone();
                let path_full = path;
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            engine_full
                                .decode_file(&path_full)
                                .map_err(|e| e.to_string())
                        })
                        .await
                        .map_err(|e| e.to_string())?
                    },
                    move |frame| Message::FullImageDecoded { generation, frame },
                )
            }
        }
    }

    fn spawn_pdf_page_render(&mut self) -> Task<Message> {
        let Some(pdf) = self.pdf_doc.clone() else {
            return Task::none();
        };
        let page_index = self.pdf_page_index;
        let dpi = 130u32;

        // If already prefetched in cache, present in 0.0ms without spawning an async task!
        if pdf.has_cached_page(page_index, dpi) {
            if let Ok(frame) = pdf.render_page_surface(page_index, dpi) {
                let surface = self.gpu_manager.get_or_upload(&frame, page_index as u64);
                self.active_surface = Some(surface);
                self.base_frame = Some(frame.clone());
                self.active_frame = Some(frame);
                self.trigger_auto_fit();
                self.status_message = format!(
                    "PDF Page {} of {} (Instant Cache • {})",
                    page_index + 1,
                    pdf.page_count(),
                    pdf.active_engine_name
                );
                return self.spawn_pdf_prefetch(page_index);
            }
        }

        let (generation, _token) = self.scheduler.next_navigation_epoch();
        self.status_message = format!("Rendering PDF Page {}...", page_index + 1);

        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    pdf.render_page_surface(page_index, dpi)
                        .map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| e.to_string())?
            },
            move |frame| Message::PdfPageRendered {
                generation,
                page_index,
                frame,
            },
        )
    }

    /// Pre-renders next and previous PDF pages in the background so page flips feel instantaneous.
    fn spawn_pdf_prefetch(&self, current_page: usize) -> Task<Message> {
        let Some(pdf) = self.pdf_doc.clone() else {
            return Task::none();
        };
        let total = pdf.page_count();
        let dpi = 130u32;
        let mut neighbors = Vec::new();
        if current_page + 1 < total && !pdf.has_cached_page(current_page + 1, dpi) {
            neighbors.push(current_page + 1);
        }
        if current_page > 0 && !pdf.has_cached_page(current_page - 1, dpi) {
            neighbors.push(current_page - 1);
        }
        if neighbors.is_empty() {
            return Task::none();
        }

        Task::perform(
            async move {
                let _ = tokio::task::spawn_blocking(move || {
                    for idx in neighbors {
                        let _ = pdf.render_page_surface(idx, dpi);
                    }
                })
                .await;
            },
            |_| Message::PdfPrefetchFinished,
        )
    }

    fn navigate_step(&mut self, delta: isize) -> Task<Message> {
        match self.content_type {
            ActiveContentType::Pdf => {
                if let Some(pdf) = &self.pdf_doc {
                    let count = pdf.page_count() as isize;
                    let next = (self.pdf_page_index as isize + delta).clamp(0, count - 1) as usize;
                    if next != self.pdf_page_index {
                        self.pdf_page_index = next;
                        self.pdf_page_input = (next + 1).to_string();
                        let pdf_path = pdf.path.clone();
                        self.config.save_bookmark(&pdf_path, next);
                        let _ = self.config.save();
                        return self.spawn_pdf_page_render();
                    }
                }
                Task::none()
            }
            ActiveContentType::Cbz => {
                if let Some(cbz) = &self.cbz_doc {
                    let count = cbz.page_count() as isize;
                    let step = if self.cbz_two_page { delta * 2 } else { delta };
                    let effective_delta = match self.cbz_direction {
                        ReadingDirection::LeftToRight => step,
                        ReadingDirection::RightToLeft => -step,
                    };
                    let next = (self.cbz_page_index as isize + effective_delta)
                        .clamp(0, count - 1) as usize;
                    if next != self.cbz_page_index {
                        self.cbz_page_index = next;
                        let cbz_path = cbz.archive_path.clone();
                        self.config.save_bookmark(&cbz_path, next);
                        let _ = self.config.save();
                        self.render_active_cbz_page();
                    }
                }
                Task::none()
            }
            ActiveContentType::Image => {
                if self.directory_files.is_empty() {
                    return Task::none();
                }
                let len = self.directory_files.len() as isize;
                let next = (self.current_index as isize + delta).rem_euclid(len) as usize;
                self.current_index = next;
                let path = self.directory_files[next].clone();
                self.load_path_command(path)
            }
            ActiveContentType::None => Task::none(),
        }
    }

    fn select_index(&mut self, idx: usize) -> Task<Message> {
        match self.content_type {
            ActiveContentType::Pdf => {
                if let Some(pdf) = &self.pdf_doc {
                    if idx < pdf.page_count() {
                        self.pdf_page_index = idx;
                        self.pdf_page_input = (idx + 1).to_string();
                        return self.spawn_pdf_page_render();
                    }
                }
                Task::none()
            }
            ActiveContentType::Cbz => {
                if let Some(cbz) = &self.cbz_doc {
                    if idx < cbz.page_count() {
                        self.cbz_page_index = idx;
                        self.render_active_cbz_page();
                    }
                }
                Task::none()
            }
            ActiveContentType::Image => {
                if idx < self.directory_files.len() {
                    self.current_index = idx;
                    let path = self.directory_files[idx].clone();
                    return self.load_path_command(path);
                }
                Task::none()
            }
            ActiveContentType::None => Task::none(),
        }
    }

    pub fn current_doc_page_count(&self) -> usize {
        match self.content_type {
            ActiveContentType::Pdf => self.pdf_doc.as_ref().map(|d| d.page_count()).unwrap_or(0),
            ActiveContentType::Cbz => self.cbz_doc.as_ref().map(|d| d.page_count()).unwrap_or(0),
            _ => 0,
        }
    }

    pub fn current_doc_page_index(&self) -> usize {
        match self.content_type {
            ActiveContentType::Pdf => self.pdf_page_index,
            ActiveContentType::Cbz => self.cbz_page_index,
            _ => 0,
        }
    }

    /// Lazily loads a window of pages around `center_page` for smooth continuous vertical scrolling
    /// in PDF and CBZ documents without exhausting RAM or VRAM.
    fn spawn_vertical_window_loads(&self, center_page: usize) -> Task<Message> {
        let total = self.current_doc_page_count();
        if total == 0 {
            return Task::none();
        }
        let start = center_page.saturating_sub(2);
        let end = (center_page + 5).min(total);
        let mut tasks = Vec::new();

        for page_idx in start..end {
            if self.vertical_page_handles.contains_key(&page_idx) {
                continue;
            }
            if let Some(pdf) = self.pdf_doc.clone() {
                tasks.push(Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            pdf.render_page_surface(page_idx, 125).ok()
                        })
                        .await
                        .ok()
                        .flatten()
                    },
                    move |opt| match opt {
                        Some(frame) => Message::VerticalPageReady {
                            page_index: page_idx,
                            frame,
                        },
                        None => Message::PdfPrefetchFinished,
                    },
                ));
            } else if let Some(cbz) = self.cbz_doc.clone() {
                let eng = self.image_engine.clone();
                tasks.push(Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || cbz.decode_page(page_idx, &eng).ok())
                            .await
                            .ok()
                            .flatten()
                    },
                    move |opt| match opt {
                        Some(frame) => Message::VerticalPageReady {
                            page_index: page_idx,
                            frame,
                        },
                        None => Message::PdfPrefetchFinished,
                    },
                ));
            }
        }

        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    fn refresh_edited_surface(&mut self) {
        if let Some(base) = &self.base_frame {
            if let Ok(transformed) = self.edit_pipeline.evaluate(base) {
                let surface = self
                    .gpu_manager
                    .get_or_upload(&transformed, self.edit_pipeline.revision());
                self.resize_w_val = transformed.width;
                self.resize_h_val = transformed.height;
                self.active_surface = Some(surface);
                self.active_frame = Some(transformed);
            }
        }
    }

    fn render_active_cbz_page(&mut self) {
        let Some(cbz) = self.cbz_doc.clone() else {
            return;
        };

        match cbz.decode_page(self.cbz_page_index, &self.image_engine) {
            Ok(frame) => {
                let surface = self
                    .gpu_manager
                    .get_or_upload(&frame, self.cbz_page_index as u64);
                self.active_surface = Some(surface);
                self.active_frame = Some(frame);
                self.trigger_auto_fit();

                if self.cbz_two_page && self.cbz_page_index + 1 < cbz.page_count() {
                    if let Ok(second_frame) =
                        cbz.decode_page(self.cbz_page_index + 1, &self.image_engine)
                    {
                        self.secondary_surface = Some(
                            self.gpu_manager
                                .get_or_upload(&second_frame, (self.cbz_page_index + 1) as u64),
                        );
                    } else {
                        self.secondary_surface = None;
                    }
                } else {
                    self.secondary_surface = None;
                }

                self.status_message = format!(
                    "CBZ Page {} / {} ({:?})",
                    self.cbz_page_index + 1,
                    cbz.page_count(),
                    self.cbz_direction
                );
            }
            Err(e) => {
                self.error_banner = Some(e.to_string());
            }
        }
    }
}
