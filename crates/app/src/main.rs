mod icons;
mod scrub_number;
mod state;
mod theme;
mod ui;
mod viewport;

use aether_core::AppConfig;
use aether_platform::configure_wayland_and_vulkan;
use iced::Size;
use state::AetherApp;

pub fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("warn,fontdb=error,wgpu_hal=error,aetherview=info")
            }),
        )
        .init();

    let config = AppConfig::load();
    configure_wayland_and_vulkan(config.gpu_backend);

    iced::application(AetherApp::title, AetherApp::update, AetherApp::view)
        .subscription(AetherApp::subscription)
        .theme(AetherApp::theme)
        .window(iced::window::Settings {
            size: Size::new(config.window_width as f32, config.window_height as f32),
            min_size: Some(Size::new(420.0, 320.0)),
            ..Default::default()
        })
        .antialiasing(false)
        .run_with(AetherApp::boot)
}
