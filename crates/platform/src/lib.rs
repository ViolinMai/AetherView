use aether_core::GpuBackendPreference;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCheck {
    pub name: String,
    pub available: bool,
    pub version_or_note: String,
    pub arch_install_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformDiagnostics {
    pub session_type: String,
    pub wayland_display: Option<String>,
    pub is_hyprland: bool,
    pub hyprland_instance: Option<String>,
    pub scale_factor_env: Option<String>,
    pub active_gpu_backend: String,
    pub xdg_portal_available: bool,
    pub tools: Vec<ToolCheck>,
    pub fonts_and_data: Vec<ToolCheck>,
}

impl PlatformDiagnostics {
    pub fn detect(pref: GpuBackendPreference) -> Self {
        let session_type = env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into());
        let wayland_display = env::var("WAYLAND_DISPLAY").ok();
        let hyprland_instance = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
        let is_hyprland = hyprland_instance.is_some()
            || env::var("XDG_CURRENT_DESKTOP")
                .map(|v| v.to_lowercase().contains("hyprland"))
                .unwrap_or(false);

        let active_gpu_backend = match pref {
            GpuBackendPreference::Vulkan => "Vulkan (Hardware Accelerated)".to_string(),
            GpuBackendPreference::AutoHardware => "Auto Hardware (Vulkan / GL)".to_string(),
            GpuBackendPreference::FallbackSoftware => "Software Fallback".to_string(),
        };

        let tools = vec![
            Self::probe_binary(
                "mutool",
                &["-v"],
                "Ultra-fast MuPDF C rendering engine (Primary PDF Rasterizer)",
                "sudo pacman -S mupdf-tools",
            ),
            Self::probe_binary(
                "pdftoppm",
                &["-v"],
                "Poppler PDF rendering engine (Fallback PDF Rasterizer)",
                "sudo pacman -S poppler",
            ),
            Self::probe_binary(
                "pdfinfo",
                &["-v"],
                "Instant PDF header & page count inspector (<2ms)",
                "sudo pacman -S poppler",
            ),
            Self::probe_binary(
                "qpdf",
                &["--version"],
                "High-speed C++ structural PDF page reordering engine",
                "sudo pacman -S qpdf",
            ),
            Self::probe_binary(
                "tesseract",
                &["--version"],
                "Real Arabic & English OCR Engine (Tesseract LSTM)",
                "sudo pacman -S tesseract tesseract-data-ara tesseract-data-eng",
            ),
        ];

        let fonts_and_data = Self::probe_fonts_and_ocr_data();

        Self {
            session_type,
            wayland_display,
            is_hyprland,
            hyprland_instance,
            scale_factor_env: env::var("GDK_SCALE")
                .ok()
                .or_else(|| env::var("QT_SCALE_FACTOR").ok()),
            active_gpu_backend,
            xdg_portal_available: true,
            tools,
            fonts_and_data,
        }
    }

    fn probe_binary(
        bin: &str,
        args: &[&str],
        purpose: &str,
        arch_hint: &str,
    ) -> ToolCheck {
        let output = std::process::Command::new(bin).args(args).output();
        match output {
            Ok(out) => {
                let raw = String::from_utf8_lossy(&out.stdout).to_string()
                    + &String::from_utf8_lossy(&out.stderr);
                let first_line = raw
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("Installed")
                    .trim()
                    .to_string();
                ToolCheck {
                    name: bin.to_string(),
                    available: true,
                    version_or_note: format!("{} — {}", first_line, purpose),
                    arch_install_hint: arch_hint.to_string(),
                }
            }
            Err(_) => ToolCheck {
                name: bin.to_string(),
                available: false,
                version_or_note: format!("Missing — {}", purpose),
                arch_install_hint: arch_hint.to_string(),
            },
        }
    }

    fn probe_fonts_and_ocr_data() -> Vec<ToolCheck> {
        let mut checks = Vec::new();

        // Check Tesseract language packs (`ara` and `eng`)
        let langs_out = std::process::Command::new("tesseract")
            .arg("--list-langs")
            .output()
            .ok()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout).to_string()
                    + &String::from_utf8_lossy(&o.stderr)
            })
            .unwrap_or_default();

        let has_ara = langs_out.lines().any(|l| l.trim() == "ara");
        let has_eng = langs_out.lines().any(|l| l.trim() == "eng");

        checks.push(ToolCheck {
            name: "tesseract-data-ara (Arabic OCR Pack)".into(),
            available: has_ara,
            version_or_note: if has_ara {
                "Installed — Arabic RTL script recognition ready".into()
            } else {
                "Missing — Required for Arabic manga/document OCR".into()
            },
            arch_install_hint: "sudo pacman -S tesseract-data-ara".into(),
        });

        checks.push(ToolCheck {
            name: "tesseract-data-eng (English OCR Pack)".into(),
            available: has_eng,
            version_or_note: if has_eng {
                "Installed — English LTR script recognition ready".into()
            } else {
                "Missing — Required for English OCR".into()
            },
            arch_install_hint: "sudo pacman -S tesseract-data-eng".into(),
        });

        // Probe installed system fonts via `fc-list`
        let fc_out = std::process::Command::new("fc-list")
            .arg(":family")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase())
            .unwrap_or_default();

        let has_noto = fc_out.contains("noto sans");
        let has_arabic_font =
            fc_out.contains("noto sans arabic") || fc_out.contains("amiri") || fc_out.contains("kacst") || fc_out.contains("dejavu sans");
        let has_dejavu = fc_out.contains("dejavu");

        checks.push(ToolCheck {
            name: "Noto Sans & Arabic UI Fonts".into(),
            available: has_noto && has_arabic_font,
            version_or_note: if has_noto && has_arabic_font {
                "Installed — Proper Arabic RTL & Latin shaping active".into()
            } else {
                "Missing recommended Noto Sans / Arabic font families".into()
            },
            arch_install_hint: "sudo pacman -S noto-fonts noto-fonts-extra ttf-dejavu".into(),
        });

        checks.push(ToolCheck {
            name: "DejaVu Sans Fallback Font".into(),
            available: has_dejavu,
            version_or_note: if has_dejavu {
                "Installed — Unicode symbol & glyph fallback ready".into()
            } else {
                "Missing DejaVu glyph fallback".into()
            },
            arch_install_hint: "sudo pacman -S ttf-dejavu".into(),
        });

        checks
    }

    pub fn summary_badge(&self) -> String {
        let wm = if self.is_hyprland {
            "Hyprland (Wayland Native)"
        } else if self.wayland_display.is_some() {
            "Wayland Native"
        } else {
            &self.session_type
        };
        format!("{} • {}", wm, self.active_gpu_backend)
    }
}

/// Configures environment variables before initializing the GUI/WGPU stack
/// so Wayland native protocol and Vulkan backend are prioritized on Linux/NVIDIA.
pub fn configure_wayland_and_vulkan(pref: GpuBackendPreference) {
    if env::var("WAYLAND_DISPLAY").is_ok() && env::var("WINIT_UNIX_BACKEND").is_err() {
        unsafe {
            env::set_var("WINIT_UNIX_BACKEND", "wayland");
        }
    }

    match pref {
        GpuBackendPreference::Vulkan => {
            if env::var("WGPU_BACKEND").is_err() {
                unsafe {
                    env::set_var("WGPU_BACKEND", "vulkan");
                }
            }
            if env::var("WGPU_POWER_PREFERENCE").is_err() {
                unsafe {
                    env::set_var("WGPU_POWER_PREFERENCE", "high_performance");
                }
            }
        }
        GpuBackendPreference::AutoHardware => {
            if env::var("WGPU_POWER_PREFERENCE").is_err() {
                unsafe {
                    env::set_var("WGPU_POWER_PREFERENCE", "high_performance");
                }
            }
        }
        GpuBackendPreference::FallbackSoftware => {}
    }
}

pub async fn open_file_dialog() -> Option<Vec<PathBuf>> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Open Image, PDF, or CBZ Archive — AetherView")
        .add_filter(
            "All Supported Media",
            &[
                "jpg", "jpeg", "png", "webp", "gif", "tif", "tiff", "bmp", "ico", "tga", "pnm",
                "pbm", "pgm", "ppm", "qoi", "hdr", "exr", "dds", "svg", "avif", "heic", "heif",
                "jxl", "pdf", "cbz",
            ],
        )
        .add_filter(
            "Images",
            &[
                "jpg", "jpeg", "png", "webp", "gif", "tif", "tiff", "bmp", "ico", "tga", "qoi",
                "hdr", "exr", "dds", "svg", "avif", "heic", "jxl",
            ],
        )
        .add_filter("PDF Documents", &["pdf"])
        .add_filter("Comic Book Archives (CBZ)", &["cbz"])
        .pick_files()
        .await?;

    Some(handle.into_iter().map(|f| f.path().to_path_buf()).collect())
}

pub async fn open_directory_dialog() -> Option<PathBuf> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Open Directory — AetherView")
        .pick_folder()
        .await?;
    Some(handle.path().to_path_buf())
}

pub async fn save_file_dialog(default_name: String, extensions: Vec<&'static str>) -> Option<PathBuf> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Export / Save As — AetherView")
        .set_file_name(&default_name)
        .add_filter("Output Format", &extensions)
        .save_file()
        .await?;
    Some(handle.path().to_path_buf())
}
