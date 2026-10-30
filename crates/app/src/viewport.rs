use crate::state::Message;
use aether_core::ZoomMode;
use aether_editing::AspectRatioPreset;
use iced::advanced::image::Renderer as ImageRenderer;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::widget::{self, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::keyboard::{self, key::Named, Key, Modifiers};
use iced::mouse::{self, Button, Cursor, ScrollDelta};
use iced::widget::image::Handle as ImageHandle;
use iced::{Background, Border, Color, Element, Event, Length, Point, Rectangle, Size};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CropDragHandle {
    None,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    TopEdge,
    BottomEdge,
    LeftEdge,
    RightEdge,
    MoveInside,
    NewSelection,
}

impl Default for CropDragHandle {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Default)]
struct ViewportInteractionState {
    is_panning: bool,
    drag_start_cursor: Point,
    drag_start_pan: (f32, f32),
    active_crop_handle: CropDragHandle,
    crop_start_cursor_px: (i32, i32),
    crop_start_rect: (i32, i32, i32, i32),
    last_click_instant: Option<Instant>,
}

pub struct InteractiveViewport {
    pub primary_handle: Option<ImageHandle>,
    pub img_width: u32,
    pub img_height: u32,
    pub zoom_mode: ZoomMode,
    pub zoom_scale: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub crop_enabled: bool,
    pub crop_aspect: AspectRatioPreset,
    pub crop_rect_px: Option<(u32, u32, u32, u32)>,
}

impl InteractiveViewport {
    pub fn compute_placement(&self, bounds: Rectangle) -> (f32, f32, Rectangle) {
        let vw = bounds.width.max(1.0);
        let vh = bounds.height.max(1.0);
        let iw = self.img_width.max(1) as f32;
        let ih = self.img_height.max(1) as f32;

        // Leave breathing room around the image so the white crop border and handles are always clearly visible
        let margin = if self.crop_enabled { 48.0 } else { 20.0 };
        let fit_scale = ((vw - margin).max(32.0) / iw)
            .min((vh - margin).max(32.0) / ih)
            .clamp(0.02, 16.0);

        let effective_scale = match self.zoom_mode {
            ZoomMode::FitWindow => fit_scale,
            ZoomMode::FitWidth => ((vw - margin).max(32.0) / iw).clamp(0.02, 16.0),
            ZoomMode::FitHeight => ((vh - margin).max(32.0) / ih).clamp(0.02, 16.0),
            ZoomMode::ActualSize => 1.0,
            ZoomMode::Custom => self.zoom_scale.clamp(0.05, 32.0),
        };

        let drawn_w = iw * effective_scale;
        let drawn_h = ih * effective_scale;

        let max_pan_x = ((drawn_w + vw) * 0.5 - 64.0).max(0.0);
        let max_pan_y = ((drawn_h + vh) * 0.5 - 64.0).max(0.0);

        let clamped_pan_x = self.pan_x.clamp(-max_pan_x, max_pan_x);
        let clamped_pan_y = self.pan_y.clamp(-max_pan_y, max_pan_y);

        let img_x = bounds.x + (vw - drawn_w) * 0.5 + clamped_pan_x;
        let img_y = bounds.y + (vh - drawn_h) * 0.5 + clamped_pan_y;

        (
            effective_scale,
            fit_scale,
            Rectangle {
                x: img_x,
                y: img_y,
                width: drawn_w,
                height: drawn_h,
            },
        )
    }

    fn effective_crop_px(&self) -> (u32, u32, u32, u32) {
        self.crop_rect_px
            .unwrap_or((0, 0, self.img_width.max(1), self.img_height.max(1)))
    }

    fn crop_screen_rect(&self, img_rect: Rectangle) -> Rectangle {
        let (cx, cy, cw, ch) = self.effective_crop_px();
        let iw = self.img_width.max(1) as f32;
        let ih = self.img_height.max(1) as f32;

        Rectangle {
            x: img_rect.x + (cx as f32 / iw) * img_rect.width,
            y: img_rect.y + (cy as f32 / ih) * img_rect.height,
            width: ((cw as f32 / iw) * img_rect.width).max(4.0),
            height: ((ch as f32 / ih) * img_rect.height).max(4.0),
        }
    }

    fn hit_test_crop_handle(&self, pos: Point, crop_scr: Rectangle) -> CropDragHandle {
        let grab_radius = 12.0;
        let left = crop_scr.x;
        let right = crop_scr.x + crop_scr.width;
        let top = crop_scr.y;
        let bottom = crop_scr.y + crop_scr.height;

        let near_l = (pos.x - left).abs() <= grab_radius;
        let near_r = (pos.x - right).abs() <= grab_radius;
        let near_t = (pos.y - top).abs() <= grab_radius;
        let near_b = (pos.y - bottom).abs() <= grab_radius;

        let within_x = pos.x >= left - grab_radius && pos.x <= right + grab_radius;
        let within_y = pos.y >= top - grab_radius && pos.y <= bottom + grab_radius;

        if near_l && near_t {
            CropDragHandle::TopLeft
        } else if near_r && near_t {
            CropDragHandle::TopRight
        } else if near_l && near_b {
            CropDragHandle::BottomLeft
        } else if near_r && near_b {
            CropDragHandle::BottomRight
        } else if near_t && within_x {
            CropDragHandle::TopEdge
        } else if near_b && within_x {
            CropDragHandle::BottomEdge
        } else if near_l && within_y {
            CropDragHandle::LeftEdge
        } else if near_r && within_y {
            CropDragHandle::RightEdge
        } else if crop_scr.contains(pos) {
            CropDragHandle::MoveInside
        } else {
            CropDragHandle::NewSelection
        }
    }

    fn screen_to_image_px(&self, pt: Point, img_rect: Rectangle) -> (i32, i32) {
        let rel_x = ((pt.x - img_rect.x) / img_rect.width.max(1.0)).clamp(0.0, 1.0);
        let rel_y = ((pt.y - img_rect.y) / img_rect.height.max(1.0)).clamp(0.0, 1.0);
        let px = (rel_x * self.img_width as f32).round() as i32;
        let py = (rel_y * self.img_height as f32).round() as i32;
        (px, py)
    }

    fn map_key_to_message(key: Key, modifiers: Modifiers) -> Option<Message> {
        if modifiers.command() && modifiers.shift() {
            match key.as_ref() {
                Key::Character("o") | Key::Character("O") => return Some(Message::RunOcrCurrent),
                Key::Character("s") | Key::Character("S") => return Some(Message::RequestSaveAs),
                _ => {}
            }
        }

        if modifiers.command() {
            match key.as_ref() {
                Key::Character("o") | Key::Character("O") => {
                    return Some(Message::RequestOpenFiles)
                }
                Key::Character("s") | Key::Character("S") => return Some(Message::RequestSaveAs),
                Key::Character("z") | Key::Character("Z") => return Some(Message::UndoEdit),
                Key::Character("y") | Key::Character("Y") => return Some(Message::RedoEdit),
                Key::Character("r") | Key::Character("R") => {
                    return Some(Message::ApplyEdit(
                        aether_editing::EditOperation::Rotate90Cw,
                    ))
                }
                Key::Character("d") | Key::Character("D") => {
                    return Some(Message::ToggleDiagnosticsModal)
                }
                _ => {}
            }
        }

        match key.as_ref() {
            Key::Named(Named::ArrowRight) => Some(Message::NextItem),
            Key::Named(Named::ArrowLeft) => Some(Message::PrevItem),
            Key::Named(Named::Tab) => Some(Message::ToggleSidebar),
            Key::Named(Named::F11) => Some(Message::ToggleFullscreen),
            Key::Named(Named::F12) => Some(Message::ToggleDiagnosticsModal),
            Key::Named(Named::Enter) => Some(Message::CommitCrop),
            Key::Character("i") | Key::Character("I") => Some(Message::ToggleInfoPanel),
            Key::Character("v") | Key::Character("V") => Some(Message::ToggleVerticalScrollMode),
            Key::Character("e") | Key::Character("E") => {
                Some(Message::SetMode(aether_core::AppMode::ImageEditor))
            }
            Key::Character("+") | Key::Character("=") => Some(Message::ZoomIn),
            Key::Character("-") => Some(Message::ZoomOut),
            Key::Character("0") => Some(Message::SetZoomMode(ZoomMode::FitWindow)),
            _ => None,
        }
    }
}

impl<Theme, Renderer> Widget<Message, Theme, Renderer> for InteractiveViewport
where
    Renderer: renderer::Renderer + ImageRenderer<Handle = ImageHandle>,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<ViewportInteractionState>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(ViewportInteractionState::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    fn layout(
        &self,
        _tree: &mut widget::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn on_event(
        &mut self,
        tree: &mut widget::Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> iced::event::Status {
        match &event {
            Event::Window(iced::window::Event::FileHovered(path)) => {
                shell.publish(Message::FileHovered(path.clone()));
                return iced::event::Status::Captured;
            }
            Event::Window(iced::window::Event::FileDropped(path)) => {
                shell.publish(Message::FileDropped(path.clone()));
                return iced::event::Status::Captured;
            }
            Event::Window(iced::window::Event::FilesHoveredLeft) => {
                shell.publish(Message::FilesHoveredLeft);
                return iced::event::Status::Captured;
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                if let Some(msg) = Self::map_key_to_message(key.clone(), *modifiers) {
                    shell.publish(msg);
                    return iced::event::Status::Captured;
                }
            }
            _ => {}
        }

        if self.primary_handle.is_none() {
            return iced::event::Status::Ignored;
        }

        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<ViewportInteractionState>();
        let (effective_scale, fit_scale, img_rect) = self.compute_placement(bounds);

        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if let Some(cursor_pos) = cursor.position_over(bounds) {
                    let lines = match delta {
                        ScrollDelta::Lines { y, .. } => y,
                        ScrollDelta::Pixels { y, .. } => y / 32.0,
                    };
                    if lines.abs() > 0.001 {
                        let factor = if lines > 0.0 { 1.16 } else { 1.0 / 1.16 };
                        let next_scale = (effective_scale * factor).clamp(fit_scale, 32.0);

                        if lines < 0.0 && next_scale <= fit_scale * 1.005 {
                            shell.publish(Message::SetZoomMode(ZoomMode::FitWindow));
                            return iced::event::Status::Captured;
                        }

                        let center_x = bounds.x + bounds.width * 0.5;
                        let center_y = bounds.y + bounds.height * 0.5;
                        let rel_cx = cursor_pos.x - center_x;
                        let rel_cy = cursor_pos.y - center_y;

                        let scale_ratio = next_scale / effective_scale.max(0.0001);
                        let mut next_pan_x = (self.pan_x - rel_cx) * scale_ratio + rel_cx;
                        let mut next_pan_y = (self.pan_y - rel_cy) * scale_ratio + rel_cy;

                        // Smoothly interpolate (lerp) pan towards (0,0) Auto-Fit center during the final 10% of zoom-out!
                        let smooth_zone_top = fit_scale * 1.12;
                        if lines < 0.0 && next_scale < smooth_zone_top {
                            let t = ((next_scale - fit_scale)
                                / (smooth_zone_top - fit_scale).max(0.0001))
                            .clamp(0.0, 1.0);
                            next_pan_x *= t;
                            next_pan_y *= t;
                        }

                        shell.publish(Message::ViewportZoomAt {
                            scale: next_scale,
                            pan_x: next_pan_x,
                            pan_y: next_pan_y,
                        });
                        return iced::event::Status::Captured;
                    }
                }
            }

            Event::Mouse(mouse::Event::ButtonPressed(Button::Left)) => {
                if let Some(pos) = cursor.position_over(bounds) {
                    let now = Instant::now();
                    if let Some(prev) = state.last_click_instant {
                        if now.duration_since(prev).as_millis() < 280 && !self.crop_enabled {
                            state.last_click_instant = None;
                            shell.publish(Message::SetZoomMode(ZoomMode::FitWindow));
                            return iced::event::Status::Captured;
                        }
                    }
                    state.last_click_instant = Some(now);

                    if self.crop_enabled {
                        let crop_scr = self.crop_screen_rect(img_rect);
                        let handle = self.hit_test_crop_handle(pos, crop_scr);
                        state.active_crop_handle = handle;
                        let (px, py) = self.screen_to_image_px(pos, img_rect);
                        state.crop_start_cursor_px = (px, py);
                        let (cx, cy, cw, ch) = self.effective_crop_px();
                        state.crop_start_rect = (cx as i32, cy as i32, cw as i32, ch as i32);
                        return iced::event::Status::Captured;
                    } else {
                        state.is_panning = true;
                        state.drag_start_cursor = pos;
                        state.drag_start_pan = (self.pan_x, self.pan_y);
                        return iced::event::Status::Captured;
                    }
                }
            }

            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if self.crop_enabled && state.active_crop_handle != CropDragHandle::None {
                    let (cur_x, cur_y) = self.screen_to_image_px(position, img_rect);
                    let (start_px, start_py) = state.crop_start_cursor_px;
                    let dx = cur_x - start_px;
                    let dy = cur_y - start_py;

                    let (sx, sy, sw, sh) = state.crop_start_rect;
                    let max_w = self.img_width as i32;
                    let max_h = self.img_height as i32;
                    let min_dim = 16;

                    let (mut nx, mut ny, mut nw, mut nh) = match state.active_crop_handle {
                        CropDragHandle::MoveInside => {
                            let clamped_x = (sx + dx).clamp(0, (max_w - sw).max(0));
                            let clamped_y = (sy + dy).clamp(0, (max_h - sh).max(0));
                            (clamped_x, clamped_y, sw, sh)
                        }
                        CropDragHandle::LeftEdge => {
                            let new_l = (sx + dx).clamp(0, sx + sw - min_dim);
                            (new_l, sy, (sx + sw) - new_l, sh)
                        }
                        CropDragHandle::RightEdge => {
                            let new_w = (sw + dx).clamp(min_dim, max_w - sx);
                            (sx, sy, new_w, sh)
                        }
                        CropDragHandle::TopEdge => {
                            let new_t = (sy + dy).clamp(0, sy + sh - min_dim);
                            (sx, new_t, sw, (sy + sh) - new_t)
                        }
                        CropDragHandle::BottomEdge => {
                            let new_h = (sh + dy).clamp(min_dim, max_h - sy);
                            (sx, sy, sw, new_h)
                        }
                        CropDragHandle::TopLeft => {
                            let new_l = (sx + dx).clamp(0, sx + sw - min_dim);
                            let new_t = (sy + dy).clamp(0, sy + sh - min_dim);
                            (new_l, new_t, (sx + sw) - new_l, (sy + sh) - new_t)
                        }
                        CropDragHandle::TopRight => {
                            let new_w = (sw + dx).clamp(min_dim, max_w - sx);
                            let new_t = (sy + dy).clamp(0, sy + sh - min_dim);
                            (sx, new_t, new_w, (sy + sh) - new_t)
                        }
                        CropDragHandle::BottomLeft => {
                            let new_l = (sx + dx).clamp(0, sx + sw - min_dim);
                            let new_h = (sh + dy).clamp(min_dim, max_h - sy);
                            (new_l, sy, (sx + sw) - new_l, new_h)
                        }
                        CropDragHandle::BottomRight => {
                            let new_w = (sw + dx).clamp(min_dim, max_w - sx);
                            let new_h = (sh + dy).clamp(min_dim, max_h - sy);
                            (sx, sy, new_w, new_h)
                        }
                        CropDragHandle::NewSelection => {
                            let x0 = start_px.min(cur_x).clamp(0, max_w - 1);
                            let y0 = start_py.min(cur_y).clamp(0, max_h - 1);
                            let w0 = (start_px - cur_x).abs().max(min_dim).min(max_w - x0);
                            let h0 = (start_py - cur_y).abs().max(min_dim).min(max_h - y0);
                            (x0, y0, w0, h0)
                        }
                        CropDragHandle::None => (sx, sy, sw, sh),
                    };

                    // Enforce aspect ratio when locked
                    match self.crop_aspect {
                        AspectRatioPreset::Freeform => {}
                        AspectRatioPreset::Square1x1 => {
                            let side = nw.min(nh).max(min_dim);
                            nw = side;
                            nh = side;
                        }
                        AspectRatioPreset::Standard4x3 => {
                            nh = ((nw * 3) / 4).clamp(min_dim, max_h - ny);
                        }
                        AspectRatioPreset::Widescreen16x9 => {
                            nh = ((nw * 9) / 16).clamp(min_dim, max_h - ny);
                        }
                        AspectRatioPreset::Photo3x2 => {
                            nh = ((nw * 2) / 3).clamp(min_dim, max_h - ny);
                        }
                    }

                    nx = nx.clamp(0, (max_w - 1).max(0));
                    ny = ny.clamp(0, (max_h - 1).max(0));
                    nw = nw.clamp(1, (max_w - nx).max(1));
                    nh = nh.clamp(1, (max_h - ny).max(1));

                    shell.publish(Message::InteractiveCropChanged {
                        x: nx as u32,
                        y: ny as u32,
                        w: nw as u32,
                        h: nh as u32,
                    });
                    return iced::event::Status::Captured;
                }

                if state.is_panning {
                    let total_dx = position.x - state.drag_start_cursor.x;
                    let total_dy = position.y - state.drag_start_cursor.y;

                    let max_pan_x = ((img_rect.width + bounds.width) * 0.5 - 64.0).max(0.0);
                    let max_pan_y = ((img_rect.height + bounds.height) * 0.5 - 64.0).max(0.0);

                    let next_pan_x =
                        (state.drag_start_pan.0 + total_dx).clamp(-max_pan_x, max_pan_x);
                    let next_pan_y =
                        (state.drag_start_pan.1 + total_dy).clamp(-max_pan_y, max_pan_y);

                    shell.publish(Message::ViewportPan {
                        pan_x: next_pan_x,
                        pan_y: next_pan_y,
                    });
                    return iced::event::Status::Captured;
                }
            }

            Event::Mouse(mouse::Event::ButtonReleased(Button::Left)) => {
                if state.is_panning || state.active_crop_handle != CropDragHandle::None {
                    state.is_panning = false;
                    state.active_crop_handle = CropDragHandle::None;
                    return iced::event::Status::Captured;
                }
            }

            _ => {}
        }

        iced::event::Status::Ignored
    }

    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<ViewportInteractionState>();
        if state.is_panning {
            return mouse::Interaction::Grabbing;
        }
        if self.primary_handle.is_some() {
            if let Some(pos) = cursor.position_over(layout.bounds()) {
                if self.crop_enabled {
                    let (_, _, img_rect) = self.compute_placement(layout.bounds());
                    let crop_scr = self.crop_screen_rect(img_rect);
                    let handle = if state.active_crop_handle != CropDragHandle::None {
                        state.active_crop_handle
                    } else {
                        self.hit_test_crop_handle(pos, crop_scr)
                    };
                    return match handle {
                        CropDragHandle::LeftEdge | CropDragHandle::RightEdge => {
                            mouse::Interaction::ResizingHorizontally
                        }
                        CropDragHandle::TopEdge | CropDragHandle::BottomEdge => {
                            mouse::Interaction::ResizingVertically
                        }
                        CropDragHandle::TopLeft
                        | CropDragHandle::TopRight
                        | CropDragHandle::BottomLeft
                        | CropDragHandle::BottomRight => mouse::Interaction::Crosshair,
                        CropDragHandle::MoveInside => mouse::Interaction::Grab,
                        _ => mouse::Interaction::Crosshair,
                    };
                }
                return mouse::Interaction::Grab;
            }
        }
        mouse::Interaction::default()
    }

    fn draw(
        &self,
        _tree: &widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();

        renderer.fill_quad(
            Quad {
                bounds,
                border: Border::default(),
                shadow: Default::default(),
            },
            Background::Color(Color::from_rgb8(17, 17, 20)),
        );

        let Some(handle) = &self.primary_handle else {
            return;
        };

        let (effective_scale, _, img_rect) = self.compute_placement(bounds);

        // Layer 1: Draw the GPU image texture
        renderer.with_layer(bounds, |renderer| {
            let filter = if effective_scale >= 3.0 {
                iced::widget::image::FilterMethod::Nearest
            } else {
                iced::widget::image::FilterMethod::Linear
            };

            let image_primitive = iced::advanced::image::Image {
                handle: handle.clone(),
                filter_method: filter,
                rotation: iced::Radians(0.0),
                opacity: 1.0,
                snap: true,
            };

            renderer.draw_image(image_primitive, img_rect);
        });

        // Layer 2 (Strictly ON TOP of the image): Draw Interactive White Crop Border, Dimmed Mask, Rule-of-Thirds & 8 Grab Handles
        if self.crop_enabled {
            renderer.with_layer(bounds, |renderer| {
                let crop_screen = self.crop_screen_rect(img_rect);
                let dim_color = Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.68));

                // Top darkened region outside crop
                if crop_screen.y > img_rect.y {
                    renderer.fill_quad(
                        Quad {
                            bounds: Rectangle {
                                x: img_rect.x,
                                y: img_rect.y,
                                width: img_rect.width,
                                height: crop_screen.y - img_rect.y,
                            },
                            ..Default::default()
                        },
                        dim_color,
                    );
                }
                // Bottom darkened region outside crop
                let crop_bottom = crop_screen.y + crop_screen.height;
                let img_bottom = img_rect.y + img_rect.height;
                if img_bottom > crop_bottom {
                    renderer.fill_quad(
                        Quad {
                            bounds: Rectangle {
                                x: img_rect.x,
                                y: crop_bottom,
                                width: img_rect.width,
                                height: img_bottom - crop_bottom,
                            },
                            ..Default::default()
                        },
                        dim_color,
                    );
                }
                // Left darkened region outside crop
                if crop_screen.x > img_rect.x {
                    renderer.fill_quad(
                        Quad {
                            bounds: Rectangle {
                                x: img_rect.x,
                                y: crop_screen.y,
                                width: crop_screen.x - img_rect.x,
                                height: crop_screen.height,
                            },
                            ..Default::default()
                        },
                        dim_color,
                    );
                }
                // Right darkened region outside crop
                let crop_right = crop_screen.x + crop_screen.width;
                let img_right = img_rect.x + img_rect.width;
                if img_right > crop_right {
                    renderer.fill_quad(
                        Quad {
                            bounds: Rectangle {
                                x: crop_right,
                                y: crop_screen.y,
                                width: img_right - crop_right,
                                height: crop_screen.height,
                            },
                            ..Default::default()
                        },
                        dim_color,
                    );
                }

                // Rule of Thirds subtle interior grid lines
                let grid_color = Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.28));
                for i in 1..=2 {
                    let gx = crop_screen.x + (crop_screen.width * (i as f32 / 3.0));
                    renderer.fill_quad(
                        Quad {
                            bounds: Rectangle {
                                x: gx,
                                y: crop_screen.y,
                                width: 1.0,
                                height: crop_screen.height,
                            },
                            ..Default::default()
                        },
                        grid_color,
                    );
                    let gy = crop_screen.y + (crop_screen.height * (i as f32 / 3.0));
                    renderer.fill_quad(
                        Quad {
                            bounds: Rectangle {
                                x: crop_screen.x,
                                y: gy,
                                width: crop_screen.width,
                                height: 1.0,
                            },
                            ..Default::default()
                        },
                        grid_color,
                    );
                }

                // High-contrast crisp White Crop Border
                renderer.fill_quad(
                    Quad {
                        bounds: crop_screen,
                        border: Border {
                            color: Color::WHITE,
                            width: 2.5,
                            radius: 0.0.into(),
                        },
                        shadow: Default::default(),
                    },
                    Background::Color(Color::TRANSPARENT),
                );

                // Draw 8 Interactive White Grab Handles (4 corners + 4 mid-edge bars)
                let handle_points = [
                    (crop_screen.x, crop_screen.y),
                    (crop_screen.x + crop_screen.width, crop_screen.y),
                    (crop_screen.x, crop_screen.y + crop_screen.height),
                    (
                        crop_screen.x + crop_screen.width,
                        crop_screen.y + crop_screen.height,
                    ),
                    (crop_screen.x + crop_screen.width * 0.5, crop_screen.y),
                    (
                        crop_screen.x + crop_screen.width * 0.5,
                        crop_screen.y + crop_screen.height,
                    ),
                    (crop_screen.x, crop_screen.y + crop_screen.height * 0.5),
                    (
                        crop_screen.x + crop_screen.width,
                        crop_screen.y + crop_screen.height * 0.5,
                    ),
                ];

                for (hx, hy) in handle_points {
                    renderer.fill_quad(
                        Quad {
                            bounds: Rectangle {
                                x: hx - 6.0,
                                y: hy - 6.0,
                                width: 12.0,
                                height: 12.0,
                            },
                            border: Border {
                                color: Color::from_rgb8(15, 23, 42),
                                width: 1.5,
                                radius: 2.0.into(),
                            },
                            shadow: Default::default(),
                        },
                        Background::Color(Color::WHITE),
                    );
                }
            });
        }
    }
}

impl<'a> From<InteractiveViewport> for Element<'a, Message> {
    fn from(vp: InteractiveViewport) -> Self {
        Element::new(vp)
    }
}
