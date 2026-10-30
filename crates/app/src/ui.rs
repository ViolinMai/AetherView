use crate::icons::{icon, IconKind};
use crate::scrub_number::ScrubNumber;
use crate::state::{ActiveContentType, AetherApp, Message};
use crate::theme::{
    accent_button_style, active_card_style, active_icon_button_style, canvas_backdrop_style,
    card_style, drop_zone_overlay_style, floating_hud_pill_style, icon_button_style,
    pill_button_style, sidebar_style, top_bar_style,
};
use crate::viewport::InteractiveViewport;
use aether_core::{AppMode, ReadingDirection, ZoomMode};
use aether_editing::{
    AspectRatioPreset, EditOperation, ExportFormatChoice, ProcessingPipelineMode,
};
use iced::widget::text::Wrapping;
use iced::widget::{
    button, checkbox, column, container, horizontal_space, image as iced_image, row, scrollable,
    text, text_input, vertical_space, Column, Row, Stack,
};
use iced::{Alignment, Color, ContentFit, Element, Length};

impl AetherApp {
    pub fn view(&self) -> Element<'_, Message> {
        if self.is_dragging_files {
            return self.view_wayland_drop_overlay();
        }

        let mut main_row = Row::new().width(Length::Fill).height(Length::Fill);

        if self.config.show_sidebar && self.content_type != ActiveContentType::None {
            main_row = main_row.push(self.view_left_sidebar());
        }

        main_row = main_row.push(self.view_central_stage());

        if let Some(right_panel) = self.view_right_contextual_panel() {
            main_row = main_row.push(right_panel);
        }

        let mut root = Column::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(self.view_loupe_header_bar());

        if let Some(err) = &self.error_banner {
            root = root.push(
                container(
                    row![
                        text(format!("Error: {}", err))
                            .size(12)
                            .wrapping(Wrapping::None)
                            .color(Color::from_rgb8(254, 202, 202)),
                        horizontal_space(),
                    ]
                    .padding([4, 12]),
                )
                .width(Length::Fill)
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(Color::from_rgb8(127, 29, 29))),
                    ..Default::default()
                }),
            );
        }

        let base_layout: Element<'_, Message> = root.push(main_row).into();

        if self.show_diagnostics_modal {
            Stack::new()
                .push(base_layout)
                .push(self.view_compatibility_modal())
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            base_layout
        }
    }

    /// Clean, minimalist Loupe-inspired dark header bar with custom drawn SVG icons and centered title.
    fn view_loupe_header_bar(&self) -> Element<'_, Message> {
        let mut left_group = row![
            button(icon(IconKind::OpenFolder, 17.0))
                .padding([6, 8])
                .style(icon_button_style)
                .on_press(Message::RequestOpenFiles),
            button(icon(IconKind::FitExpand, 17.0))
                .padding([6, 8])
                .style(icon_button_style)
                .on_press(Message::SetZoomMode(ZoomMode::FitWindow)),
        ]
        .spacing(4)
        .align_y(Alignment::Center);

        if self.content_type != ActiveContentType::None {
            left_group = left_group.push(
                button(icon(IconKind::Save, 17.0))
                    .padding([6, 8])
                    .style(icon_button_style)
                    .on_press(Message::RequestSaveAs),
            );
        }

        let raw_title = match &self.active_frame {
            Some(f) => f.metadata.file_name.clone(),
            None => "AetherView".to_string(),
        };
        let center_title = Self::truncate_title(&raw_title, 34);

        let mut right_group = Row::new().spacing(4).align_y(Alignment::Center);

        if self.content_type != ActiveContentType::None {
            if matches!(
                self.content_type,
                ActiveContentType::Pdf | ActiveContentType::Cbz
            ) {
                let scroll_icon = if self.vertical_scroll_mode {
                    IconKind::VerticalScroll
                } else {
                    IconKind::SinglePage
                };
                right_group = right_group.push(
                    button(icon(scroll_icon, 17.0))
                        .padding([6, 8])
                        .style(if self.vertical_scroll_mode {
                            active_icon_button_style
                        } else {
                            icon_button_style
                        })
                        .on_press(Message::ToggleVerticalScrollMode),
                );
            }

            if self.content_type == ActiveContentType::Image {
                let is_edit = self.mode == AppMode::ImageEditor;
                let next_edit_mode = if is_edit {
                    AppMode::ImageViewer
                } else {
                    AppMode::ImageEditor
                };
                right_group = right_group.push(
                    button(icon(IconKind::Crop, 17.0))
                        .padding([6, 8])
                        .style(if is_edit {
                            active_icon_button_style
                        } else {
                            icon_button_style
                        })
                        .on_press(Message::SetMode(next_edit_mode)),
                );
            }

            let is_ocr = self.mode == AppMode::OcrInspector;
            let next_ocr_mode = if is_ocr {
                match self.content_type {
                    ActiveContentType::Pdf => AppMode::PdfViewer,
                    ActiveContentType::Cbz => AppMode::ComicReader,
                    _ => AppMode::ImageViewer,
                }
            } else {
                AppMode::OcrInspector
            };

            right_group = right_group
                .push(
                    button(icon(IconKind::OcrScan, 17.0))
                        .padding([6, 8])
                        .style(if is_ocr {
                            active_icon_button_style
                        } else {
                            icon_button_style
                        })
                        .on_press(if is_ocr {
                            Message::SetMode(next_ocr_mode)
                        } else {
                            Message::RunOcrCurrent
                        }),
                )
                .push(
                    button(icon(IconKind::InfoCircle, 17.0))
                        .padding([6, 8])
                        .style(if self.config.show_info_panel {
                            active_icon_button_style
                        } else {
                            icon_button_style
                        })
                        .on_press(Message::ToggleInfoPanel),
                )
                .push(
                    button(icon(IconKind::SidebarMenu, 17.0))
                        .padding([6, 8])
                        .style(if self.config.show_sidebar {
                            active_icon_button_style
                        } else {
                            icon_button_style
                        })
                        .on_press(Message::ToggleSidebar),
                );
        }

        // Use a two-layer Stack so the centered title can NEVER collide with or push the right-side icons!
        let controls_row = row![left_group, horizontal_space(), right_group]
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .padding([5, 12]);

        let title_overlay = container(
            text(center_title)
                .size(13)
                .wrapping(Wrapping::None)
                .color(Color::from_rgb8(225, 229, 238)),
        )
        .width(Length::Fill)
        .center_x(Length::Fill)
        .padding([8, 180]);

        container(Stack::new().push(title_overlay).push(controls_row))
            .width(Length::Fill)
            .style(top_bar_style)
            .into()
    }

    fn truncate_title(s: &str, max_chars: usize) -> String {
        let count = s.chars().count();
        if count <= max_chars {
            return s.to_string();
        }
        let head: String = s.chars().take(max_chars / 2 - 1).collect();
        let tail: String = s.chars().skip(count - (max_chars / 2 - 2)).collect();
        format!("{}…{}", head, tail)
    }

    /// Central stage combining the GPU Canvas (or Vertical Document Scroller) + floating bottom-left `< >` and bottom-right zoom pills.
    fn view_central_stage(&self) -> Element<'_, Message> {
        let base_canvas: Element<'_, Message> = if self.vertical_scroll_mode
            && matches!(
                self.content_type,
                ActiveContentType::Pdf | ActiveContentType::Cbz
            ) {
            self.view_vertical_document_scroller()
        } else {
            self.view_interactive_gpu_canvas()
        };

        if self.active_surface.is_none() {
            return base_canvas;
        }

        let nav_pill = container(
            row![
                button(icon(IconKind::ChevronLeft, 16.0))
                    .padding([6, 10])
                    .style(icon_button_style)
                    .on_press(Message::PrevItem),
                button(icon(IconKind::ChevronRight, 16.0))
                    .padding([6, 10])
                    .style(icon_button_style)
                    .on_press(Message::NextItem),
            ]
            .spacing(2)
            .align_y(Alignment::Center),
        )
        .padding([2, 4])
        .style(floating_hud_pill_style);

        let zoom_text = match self.zoom_mode {
            ZoomMode::FitWindow | ZoomMode::FitWidth | ZoomMode::FitHeight => "Fit".to_string(),
            _ => format!("{:.0}%", self.zoom_scale * 100.0),
        };

        let zoom_pill = container(
            row![
                button(
                    row![
                        icon(IconKind::ZoomSearch, 14.0),
                        text(zoom_text).size(12).wrapping(Wrapping::None),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center)
                )
                .padding([5, 9])
                .style(icon_button_style)
                .on_press(Message::SetZoomMode(ZoomMode::FitWindow)),
                button(icon(IconKind::Minus, 14.0))
                    .padding([5, 7])
                    .style(icon_button_style)
                    .on_press(Message::ZoomOut),
                button(icon(IconKind::Plus, 14.0))
                    .padding([5, 7])
                    .style(icon_button_style)
                    .on_press(Message::ZoomIn),
            ]
            .spacing(2)
            .align_y(Alignment::Center),
        )
        .padding([2, 4])
        .style(floating_hud_pill_style);

        let hud_overlay = column![
            vertical_space(),
            row![nav_pill, horizontal_space(), zoom_pill]
                .align_y(Alignment::Center)
                .padding([14, 18]),
        ]
        .width(Length::Fill)
        .height(Length::Fill);

        Stack::new()
            .push(base_canvas)
            .push(hud_overlay)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_interactive_gpu_canvas(&self) -> Element<'_, Message> {
        let vp = InteractiveViewport {
            primary_handle: self.active_surface.as_ref().map(|s| s.handle.clone()),
            img_width: self.active_surface.as_ref().map(|s| s.width).unwrap_or(1),
            img_height: self.active_surface.as_ref().map(|s| s.height).unwrap_or(1),
            zoom_mode: self.zoom_mode,
            zoom_scale: self.zoom_scale,
            pan_x: self.pan_x,
            pan_y: self.pan_y,
            crop_enabled: self.mode == AppMode::ImageEditor
                && self.content_type == ActiveContentType::Image,
            crop_aspect: self.crop_aspect,
            crop_rect_px: self.interactive_crop_rect,
        };

        if self.active_surface.is_some() {
            return Element::from(vp);
        }

        Stack::new()
            .push(Element::from(vp))
            .push(
                container(
                    column![
                        text("AetherView").size(22),
                        text("Drop images, PDFs, or CBZ archives anywhere")
                            .size(13)
                            .color(Color::from_rgb8(140, 145, 160)),
                        row![
                            button(
                                row![
                                    icon(IconKind::OpenFolder, 16.0),
                                    text("Open File").size(13)
                                ]
                                .spacing(8)
                                .align_y(Alignment::Center)
                            )
                            .padding([8, 16])
                            .style(accent_button_style)
                            .on_press(Message::RequestOpenFiles),
                            button(text("Open Folder").size(13))
                                .padding([8, 16])
                                .style(pill_button_style)
                                .on_press(Message::RequestOpenDirectory),
                        ]
                        .spacing(10),
                    ]
                    .spacing(14)
                    .align_x(Alignment::Center),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// Continuous vertical scrolling view for PDF and CBZ documents where each page is auto-fitted
    /// to a comfortable card height (`680px` scaled by zoom) with clean spacing so a single page never hogs the whole vertical stream.
    fn view_vertical_document_scroller(&self) -> Element<'_, Message> {
        let total = self.current_doc_page_count();
        let page_card_height = (680.0 * self.zoom_scale).clamp(360.0, 2200.0);

        let mut pages_col = Column::new()
            .spacing(18)
            .align_x(Alignment::Center)
            .width(Length::Fill)
            .padding([20, 24]);

        for idx in 0..total {
            let page_element: Element<'_, Message> =
                if let Some(surface) = self.vertical_page_handles.get(&idx) {
                    let aspect = surface.width.max(1) as f32 / surface.height.max(1) as f32;
                    let card_w = (page_card_height * aspect).clamp(240.0, 1800.0);

                    container(
                        column![
                            iced_image(surface.handle.clone())
                                .width(Length::Fixed(card_w))
                                .height(Length::Fixed(page_card_height))
                                .content_fit(ContentFit::Contain),
                            text(format!("Page {} / {}", idx + 1, total))
                                .size(11)
                                .color(Color::from_rgb8(148, 163, 184)),
                        ]
                        .spacing(6)
                        .align_x(Alignment::Center),
                    )
                    .padding(10)
                    .style(card_style)
                    .into()
                } else {
                    container(
                        button(
                            text(format!("Page {} — Click or scroll to render", idx + 1))
                                .size(12)
                                .color(Color::from_rgb8(148, 163, 184)),
                        )
                        .style(pill_button_style)
                        .on_press(Message::VerticalScrollChanged(
                            idx as f32 / total.max(1) as f32,
                        )),
                    )
                    .width(Length::Fixed(480.0))
                    .height(Length::Fixed(page_card_height))
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .style(card_style)
                    .into()
                };

            pages_col = pages_col.push(page_element);
        }

        container(
            scrollable(pages_col)
                .height(Length::Fill)
                .width(Length::Fill)
                .on_scroll(|vp| Message::VerticalScrollChanged(vp.relative_offset().y)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(canvas_backdrop_style)
        .into()
    }

    fn view_left_sidebar(&self) -> Element<'_, Message> {
        let content: Element<'_, Message> = match self.content_type {
            ActiveContentType::Pdf => self.view_pdf_sidebar(),
            ActiveContentType::Cbz => self.view_cbz_sidebar(),
            _ => self.view_image_directory_sidebar(),
        };

        container(content)
            .width(Length::Fixed(220.0))
            .height(Length::Fill)
            .style(sidebar_style)
            .padding(8)
            .into()
    }

    fn view_image_directory_sidebar(&self) -> Element<'_, Message> {
        let mut list = Column::new().spacing(6);
        list = list.push(
            text(format!(
                "Files ({} / {} thumbs)",
                self.thumbnails.len(),
                self.directory_files.len()
            ))
            .size(12)
            .color(Color::from_rgb8(148, 163, 184)),
        );

        for (idx, path) in self.directory_files.iter().enumerate() {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Item".into());
            let is_active = idx == self.current_index;

            let mut item_col = Column::new().spacing(4);
            if let Some(thumb_handle) = self.thumbnails.get(path) {
                item_col = item_col.push(
                    iced_image(thumb_handle.clone())
                        .width(Length::Fill)
                        .height(Length::Fixed(84.0))
                        .content_fit(ContentFit::Contain),
                );
            }
            item_col = item_col.push(
                text(format!("{}. {}", idx + 1, name))
                    .size(11)
                    .wrapping(Wrapping::None),
            );

            let card = container(
                button(item_col)
                    .width(Length::Fill)
                    .style(pill_button_style)
                    .on_press(Message::SelectIndex(idx)),
            )
            .style(if is_active {
                active_card_style
            } else {
                card_style
            })
            .padding(4);

            list = list.push(card);
        }

        scrollable(list).height(Length::Fill).into()
    }

    fn view_pdf_sidebar(&self) -> Element<'_, Message> {
        let mut col = Column::new().spacing(6);

        let Some(pdf) = &self.pdf_doc else {
            return col.push(text("No PDF loaded.").size(12)).into();
        };

        col = col.push(
            row![
                text(format!("Pages ({})", pdf.page_count())).size(12),
                horizontal_space(),
                button(text("Save PDF").size(11))
                    .padding([3, 8])
                    .style(accent_button_style)
                    .on_press(Message::PdfRequestSave),
            ]
            .align_y(Alignment::Center),
        );

        col = col.push(
            row![
                text("Go to:").size(11),
                text_input("1", &self.pdf_page_input)
                    .on_input(Message::PdfJumpToPageInput)
                    .width(Length::Fixed(56.0))
                    .size(11),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );

        let mut list = Column::new().spacing(4);
        for (idx, page) in pdf.pages.iter().enumerate() {
            let active = idx == self.pdf_page_index;

            let item = row![
                button(
                    text(format!(
                        "P.{} (#{})",
                        page.current_number, page.original_page_num
                    ))
                    .size(11)
                    .wrapping(Wrapping::None)
                )
                .width(Length::Fill)
                .style(if active {
                    accent_button_style
                } else {
                    pill_button_style
                })
                .on_press(Message::SelectIndex(idx)),
                button(text("Up").size(10))
                    .padding([2, 5])
                    .style(pill_button_style)
                    .on_press(Message::PdfMovePageUp(idx)),
                button(text("Dn").size(10))
                    .padding([2, 5])
                    .style(pill_button_style)
                    .on_press(Message::PdfMovePageDown(idx)),
            ]
            .spacing(3)
            .align_y(Alignment::Center);

            list = list.push(item);
        }

        col.push(scrollable(list).height(Length::Fill)).into()
    }

    fn view_cbz_sidebar(&self) -> Element<'_, Message> {
        let mut col = Column::new().spacing(6);

        let Some(cbz) = &self.cbz_doc else {
            return col.push(text("No CBZ open.").size(12)).into();
        };

        let dir_label = match self.cbz_direction {
            ReadingDirection::RightToLeft => "RTL (Manga/Arabic)",
            ReadingDirection::LeftToRight => "LTR (Western)",
        };

        col = col
            .push(text(format!("Comic Pages ({})", cbz.page_count())).size(12))
            .push(
                button(text(dir_label).size(11))
                    .width(Length::Fill)
                    .style(accent_button_style)
                    .on_press(Message::ToggleReadingDirection),
            )
            .push(
                checkbox("Two-Page Spread", self.cbz_two_page)
                    .on_toggle(|_| Message::ToggleTwoPageSpread)
                    .text_size(11),
            );

        let mut list = Column::new().spacing(4);
        for (idx, entry) in cbz.pages.iter().enumerate() {
            let active = idx == self.cbz_page_index;
            list = list.push(
                button(
                    text(format!("{}. {}", idx + 1, entry.display_name))
                        .size(11)
                        .wrapping(Wrapping::None),
                )
                .width(Length::Fill)
                .style(if active {
                    accent_button_style
                } else {
                    pill_button_style
                })
                .on_press(Message::SelectIndex(idx)),
            );
        }

        col.push(scrollable(list).height(Length::Fill)).into()
    }

    fn view_right_contextual_panel(&self) -> Option<Element<'_, Message>> {
        if self.content_type == ActiveContentType::None {
            return None;
        }

        let panel_content: Element<'_, Message> = match self.mode {
            AppMode::ImageEditor => self.view_editor_panel(),
            AppMode::OcrInspector => self.view_ocr_panel(),
            _ => {
                if !self.config.show_info_panel {
                    return None;
                }
                self.view_metadata_panel()
            }
        };

        Some(
            container(panel_content)
                .width(Length::Fixed(280.0))
                .height(Length::Fill)
                .style(sidebar_style)
                .padding(10)
                .into(),
        )
    }

    fn view_metadata_panel(&self) -> Element<'_, Message> {
        let mut col = Column::new().spacing(8);
        col = col.push(text("Properties").size(14));

        if let Some(frame) = &self.active_frame {
            let m = &frame.metadata;
            col = col.push(
                container(
                    column![
                        text(format!("File: {}", m.file_name))
                            .size(11)
                            .wrapping(Wrapping::WordOrGlyph),
                        text(format!("Resolution: {} × {} px", frame.width, frame.height)).size(11),
                        text(format!("Format: {}", m.format_name)).size(11),
                        text(format!("Size: {:.2} KB", m.file_size_bytes as f64 / 1024.0)).size(11),
                        text(format!("Bit Depth: {}-bit", m.bit_depth)).size(11),
                        text(format!("Color Space: {}", m.color_space)).size(11),
                        text(format!(
                            "ICC Profile: {}",
                            if m.icc_profile_present {
                                "Embedded"
                            } else {
                                "sRGB"
                            }
                        ))
                        .size(11),
                    ]
                    .spacing(4),
                )
                .padding(8)
                .width(Length::Fill)
                .style(card_style),
            );
        }

        col = col.push(text("GPU Telemetry (F12: Full Report)").size(13));
        col = col.push(
            container(
                column![
                    text(format!(
                        "Cached GPU Surfaces: {}",
                        self.gpu_manager.cached_texture_count()
                    ))
                    .size(11),
                    text(format!(
                        "VRAM: {:.1} / {} MB",
                        self.gpu_manager.vram_usage_mb(),
                        self.config.vram_limit_mb
                    ))
                    .size(11),
                ]
                .spacing(4),
            )
            .padding(8)
            .width(Length::Fill)
            .style(card_style),
        );

        scrollable(col).height(Length::Fill).into()
    }

    fn view_editor_panel(&self) -> Element<'_, Message> {
        let mut col = Column::new().spacing(8);
        col = col.push(text("Edit & Crop").size(14));

        col = col.push(
            row![
                button(
                    row![icon(IconKind::Undo, 13.0), text("Undo").size(11)]
                        .spacing(5)
                        .align_y(Alignment::Center)
                )
                .style(pill_button_style)
                .on_press(Message::UndoEdit),
                button(text("Redo").size(11))
                    .style(pill_button_style)
                    .on_press(Message::RedoEdit),
                button(text("Reset").size(11))
                    .style(pill_button_style)
                    .on_press(Message::ResetEdits),
            ]
            .spacing(4),
        );

        let img_w = self.active_frame.as_ref().map(|f| f.width).unwrap_or(4096);
        let img_h = self.active_frame.as_ref().map(|f| f.height).unwrap_or(4096);
        let (cx, cy, cw, ch) = self.interactive_crop_rect.unwrap_or((0, 0, img_w, img_h));

        col = col.push(
            container(
                column![
                    text(format!("Crop ({})", self.crop_aspect)).size(12),
                    row![
                        button(text("Free").size(10))
                            .style(if self.crop_aspect == AspectRatioPreset::Freeform {
                                accent_button_style
                            } else {
                                pill_button_style
                            })
                            .on_press(Message::SetCropAspect(AspectRatioPreset::Freeform)),
                        button(text("1:1").size(10))
                            .style(if self.crop_aspect == AspectRatioPreset::Square1x1 {
                                accent_button_style
                            } else {
                                pill_button_style
                            })
                            .on_press(Message::SetCropAspect(AspectRatioPreset::Square1x1)),
                        button(text("16:9").size(10))
                            .style(if self.crop_aspect == AspectRatioPreset::Widescreen16x9 {
                                accent_button_style
                            } else {
                                pill_button_style
                            })
                            .on_press(Message::SetCropAspect(AspectRatioPreset::Widescreen16x9)),
                        button(text("4:3").size(10))
                            .style(if self.crop_aspect == AspectRatioPreset::Standard4x3 {
                                accent_button_style
                            } else {
                                pill_button_style
                            })
                            .on_press(Message::SetCropAspect(AspectRatioPreset::Standard4x3)),
                    ]
                    .spacing(4),
                    ScrubNumber::new("X", cx, 0, img_w.saturating_sub(1), Message::ScrubCropX),
                    ScrubNumber::new("Y", cy, 0, img_h.saturating_sub(1), Message::ScrubCropY),
                    ScrubNumber::new("W", cw, 16, img_w, Message::ScrubCropW),
                    ScrubNumber::new("H", ch, 16, img_h, Message::ScrubCropH),
                    row![
                        button(text("Full Frame").size(11))
                            .width(Length::Fill)
                            .style(pill_button_style)
                            .on_press(Message::ResetCropBoxToFull),
                        button(text("Apply Crop").size(11))
                            .width(Length::Fill)
                            .style(accent_button_style)
                            .on_press(Message::CommitCrop),
                    ]
                    .spacing(4),
                ]
                .spacing(6),
            )
            .padding(8)
            .width(Length::Fill)
            .style(card_style),
        );

        col = col.push(
            container(
                column![
                    text("Transform").size(12),
                    row![
                        button(icon(IconKind::RotateCcw, 15.0))
                            .padding([6, 12])
                            .style(pill_button_style)
                            .on_press(Message::ApplyEdit(EditOperation::Rotate90Ccw)),
                        button(icon(IconKind::RotateCw, 15.0))
                            .padding([6, 12])
                            .style(pill_button_style)
                            .on_press(Message::ApplyEdit(EditOperation::Rotate90Cw)),
                        button(icon(IconKind::FlipH, 15.0))
                            .padding([6, 12])
                            .style(pill_button_style)
                            .on_press(Message::ApplyEdit(EditOperation::FlipHorizontal)),
                        button(icon(IconKind::FlipV, 15.0))
                            .padding([6, 12])
                            .style(pill_button_style)
                            .on_press(Message::ApplyEdit(EditOperation::FlipVertical)),
                    ]
                    .spacing(6),
                ]
                .spacing(6),
            )
            .padding(8)
            .width(Length::Fill)
            .style(card_style),
        );

        col = col.push(
            container(
                column![
                    text(format!("Resize ({})", self.resize_filter)).size(12),
                    ScrubNumber::new("Width", self.resize_w_val, 16, 16384, Message::ScrubResizeW),
                    ScrubNumber::new(
                        "Height",
                        self.resize_h_val,
                        16,
                        16384,
                        Message::ScrubResizeH
                    ),
                    checkbox("Keep Aspect Ratio", self.resize_keep_aspect)
                        .on_toggle(Message::ToggleKeepAspect)
                        .text_size(11),
                    button(text("Apply Resize").size(11))
                        .width(Length::Fill)
                        .style(accent_button_style)
                        .on_press(Message::ApplyResize),
                ]
                .spacing(6),
            )
            .padding(8)
            .width(Length::Fill)
            .style(card_style),
        );

        col = col.push(
            container(
                column![
                    text("Export Format & Pipeline").size(12),
                    row![
                        button(text("PNG").size(10))
                            .style(if self.export_format == ExportFormatChoice::Png {
                                accent_button_style
                            } else {
                                pill_button_style
                            })
                            .on_press(Message::SetExportFormat(ExportFormatChoice::Png)),
                        button(text("JPEG").size(10))
                            .style(if self.export_format == ExportFormatChoice::Jpeg {
                                accent_button_style
                            } else {
                                pill_button_style
                            })
                            .on_press(Message::SetExportFormat(ExportFormatChoice::Jpeg)),
                        button(text("WebP").size(10))
                            .style(if self.export_format == ExportFormatChoice::WebP {
                                accent_button_style
                            } else {
                                pill_button_style
                            })
                            .on_press(Message::SetExportFormat(ExportFormatChoice::WebP)),
                        button(text("TIFF").size(10))
                            .style(if self.export_format == ExportFormatChoice::Tiff {
                                accent_button_style
                            } else {
                                pill_button_style
                            })
                            .on_press(Message::SetExportFormat(ExportFormatChoice::Tiff)),
                        button(text("QOI").size(10))
                            .style(if self.export_format == ExportFormatChoice::Qoi {
                                accent_button_style
                            } else {
                                pill_button_style
                            })
                            .on_press(Message::SetExportFormat(ExportFormatChoice::Qoi)),
                    ]
                    .spacing(4),
                    row![
                        button(text("GPU (Vulkan)").size(10))
                            .width(Length::Fill)
                            .style(
                                if self.processing_pipeline
                                    == ProcessingPipelineMode::GpuAccelerated
                                {
                                    accent_button_style
                                } else {
                                    pill_button_style
                                }
                            )
                            .on_press(Message::SetProcessingPipeline(
                                ProcessingPipelineMode::GpuAccelerated
                            )),
                        button(text("Software (CPU)").size(10))
                            .width(Length::Fill)
                            .style(
                                if self.processing_pipeline == ProcessingPipelineMode::CpuSoftware {
                                    accent_button_style
                                } else {
                                    pill_button_style
                                }
                            )
                            .on_press(Message::SetProcessingPipeline(
                                ProcessingPipelineMode::CpuSoftware
                            )),
                    ]
                    .spacing(4),
                    text(format!(
                        "Active: {} • Output: .{}",
                        self.processing_pipeline.label(),
                        self.export_format.extension()
                    ))
                    .size(10)
                    .color(Color::from_rgb8(148, 163, 184)),
                    button(
                        text(format!(
                            "Export as {} (Ctrl+S)",
                            self.export_format.label()
                        ))
                        .size(12)
                    )
                    .width(Length::Fill)
                    .padding(7)
                    .style(accent_button_style)
                    .on_press(Message::RequestSaveAs),
                ]
                .spacing(6),
            )
            .padding(8)
            .width(Length::Fill)
            .style(card_style),
        );

        scrollable(col).height(Length::Fill).into()
    }

    fn view_ocr_panel(&self) -> Element<'_, Message> {
        let mut col = Column::new().spacing(8);
        col = col.push(text("Tesseract OCR (AR/EN)").size(14));

        col = col.push(
            button(text(format!("Lang: {}", self.config.ocr_language)).size(11))
                .width(Length::Fill)
                .style(pill_button_style)
                .on_press(Message::CycleOcrLanguage),
        );

        col = col.push(
            row![
                button(text("Run Tesseract OCR").size(11))
                    .width(Length::Fill)
                    .style(accent_button_style)
                    .on_press(Message::RunOcrCurrent),
                button(text("Cancel").size(11))
                    .style(pill_button_style)
                    .on_press(Message::CancelOcr),
            ]
            .spacing(4),
        );

        if self.ocr_running {
            col = col.push(
                text("Running Tesseract LSTM OCR...")
                    .size(11)
                    .color(Color::from_rgb8(96, 165, 250)),
            );
        }

        if let Some(res) = &self.ocr_result {
            col = col.push(
                text(format!(
                    "{} lines extracted in {} ms via {}",
                    res.blocks.len(),
                    res.inference_ms,
                    res.execution_provider
                ))
                .size(11)
                .color(Color::from_rgb8(148, 163, 184)),
            );

            if res.blocks.is_empty() {
                col = col.push(
                    text("No high-confidence text lines detected on this page. Ensure `tesseract-data-ara` is installed for Arabic manga/documents.")
                        .size(11)
                        .color(Color::from_rgb8(251, 191, 36)),
                );
            }

            col = col.push(
                row![
                    button(text("Copy All").size(11))
                        .width(Length::Fill)
                        .style(pill_button_style)
                        .on_press(Message::CopyAllOcrText),
                    button(text("Searchable PDF").size(11))
                        .width(Length::Fill)
                        .style(accent_button_style)
                        .on_press(Message::RequestExportSearchablePdf),
                ]
                .spacing(4),
            );

            for block in &res.blocks {
                let active = self.selected_ocr_block == Some(block.id);
                let dir_badge = if block.is_rtl { "RTL • عربي" } else { "LTR" };

                let card = container(
                    column![
                        row![
                            text(format!("#{} ({})", block.id, dir_badge)).size(10),
                            horizontal_space(),
                            text(format!("{:.0}%", block.confidence * 100.0)).size(10),
                        ],
                        button(text(&block.text_visual_display).size(12))
                            .width(Length::Fill)
                            .style(pill_button_style)
                            .on_press(Message::SelectOcrBlock(block.id)),
                    ]
                    .spacing(3),
                )
                .padding(6)
                .width(Length::Fill)
                .style(if active {
                    active_card_style
                } else {
                    card_style
                });

                col = col.push(card);
            }
        }

        scrollable(col).height(Length::Fill).into()
    }

    fn view_compatibility_modal(&self) -> Element<'_, Message> {
        let d = &self.diagnostics;
        let mut tool_rows = Column::new().spacing(6);

        for tool in d.tools.iter().chain(d.fonts_and_data.iter()) {
            let status_text = if tool.available { "[OK]" } else { "[MISSING]" };
            let status_color = if tool.available {
                Color::from_rgb8(74, 222, 128)
            } else {
                Color::from_rgb8(248, 113, 113)
            };

            let mut item = column![
                row![
                    text(status_text).size(12).color(status_color),
                    text(&tool.name).size(13),
                    horizontal_space(),
                ]
                .spacing(8),
                text(&tool.version_or_note)
                    .size(11)
                    .color(Color::from_rgb8(148, 163, 184)),
            ]
            .spacing(2);

            if !tool.available {
                item = item.push(
                    text(format!("Install on Arch: {}", tool.arch_install_hint))
                        .size(11)
                        .color(Color::from_rgb8(251, 191, 36)),
                );
            }

            tool_rows = tool_rows.push(
                container(item)
                    .padding(8)
                    .width(Length::Fill)
                    .style(card_style),
            );
        }

        let modal_card = container(
            column![
                row![
                    text("System Compatibility Report (F12)").size(15),
                    horizontal_space(),
                    button(text("Close").size(12))
                        .style(accent_button_style)
                        .on_press(Message::ToggleDiagnosticsModal),
                ]
                .align_y(Alignment::Center),
                scrollable(tool_rows).height(Length::Fixed(310.0)),
            ]
            .spacing(10),
        )
        .width(Length::Fixed(560.0))
        .padding(16)
        .style(sidebar_style);

        container(modal_card)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(
                    0.0, 0.0, 0.0, 0.72,
                ))),
                ..Default::default()
            })
            .into()
    }

    fn view_wayland_drop_overlay(&self) -> Element<'_, Message> {
        let count = self.hovered_drop_files.len().max(1);
        container(
            container(
                column![
                    text("Drop Files on Wayland Surface").size(22),
                    text(format!("Ready to open {} file(s)", count)).size(13),
                ]
                .spacing(10)
                .align_x(Alignment::Center),
            )
            .padding(36)
            .style(drop_zone_overlay_style),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(canvas_backdrop_style)
        .into()
    }
}
