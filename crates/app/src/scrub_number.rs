use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::Renderer as TextRenderer;
use iced::advanced::widget::{self, Widget};
use iced::advanced::{Clipboard, Shell, Text};
use iced::alignment::{Horizontal, Vertical};
use iced::mouse::{self, Button, Cursor};
use iced::widget::text::{LineHeight, Shaping, Wrapping};
use iced::{Background, Border, Color, Element, Event, Length, Point, Rectangle, Size};

#[derive(Default)]
struct ScrubState {
    is_dragging: bool,
    start_x: f32,
    start_value: u32,
}

/// Adobe After Effects / Photoshop style scrubbable numeric widget:
/// Hovering shows a `<->` resize cursor; dragging left/right smoothly scrubs the value in real time!
pub struct ScrubNumber<'a, Message> {
    label: &'a str,
    value: u32,
    min: u32,
    max: u32,
    step: f32,
    on_change: Box<dyn Fn(u32) -> Message + 'a>,
}

impl<'a, Message> ScrubNumber<'a, Message> {
    pub fn new(
        label: &'a str,
        value: u32,
        min: u32,
        max: u32,
        on_change: impl Fn(u32) -> Message + 'a,
    ) -> Self {
        Self {
            label,
            value,
            min,
            max: max.max(min),
            step: 2.0,
            on_change: Box::new(on_change),
        }
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer> for ScrubNumber<'a, Message>
where
    Renderer: renderer::Renderer + TextRenderer,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<ScrubState>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(ScrubState::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fixed(28.0),
        }
    }

    fn layout(
        &self,
        _tree: &mut widget::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(Size::new(limits.max().width, 28.0))
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
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<ScrubState>();

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(Button::Left)) => {
                if let Some(pos) = cursor.position_over(bounds) {
                    state.is_dragging = true;
                    state.start_x = pos.x;
                    state.start_value = self.value;
                    return iced::event::Status::Captured;
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if state.is_dragging {
                    let dx = (position.x - state.start_x) * self.step;
                    let next_val = (state.start_value as i64 + dx.round() as i64)
                        .clamp(self.min as i64, self.max as i64)
                        as u32;
                    if next_val != self.value {
                        shell.publish((self.on_change)(next_val));
                    }
                    return iced::event::Status::Captured;
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(Button::Left)) => {
                if state.is_dragging {
                    state.is_dragging = false;
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
        let state = tree.state.downcast_ref::<ScrubState>();
        if state.is_dragging || cursor.is_over(layout.bounds()) {
            mouse::Interaction::ResizingHorizontally
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<ScrubState>();
        let hovered = cursor.is_over(bounds) || state.is_dragging;

        renderer.fill_quad(
            Quad {
                bounds,
                border: Border {
                    color: if state.is_dragging {
                        Color::from_rgb8(59, 130, 246)
                    } else if hovered {
                        Color::from_rgb8(96, 165, 250)
                    } else {
                        Color::from_rgb8(55, 65, 88)
                    },
                    width: if state.is_dragging { 1.5 } else { 1.0 },
                    radius: 5.0.into(),
                },
                shadow: Default::default(),
            },
            Background::Color(if state.is_dragging {
                Color::from_rgb8(30, 48, 82)
            } else {
                Color::from_rgb8(20, 24, 34)
            }),
        );

        let display_str = format!("{}: {} px  <->", self.label, self.value);
        renderer.fill_text(
            Text {
                content: display_str,
                bounds: Size::new(bounds.width - 12.0, bounds.height),
                size: renderer.default_size() * 0.75,
                line_height: LineHeight::default(),
                font: renderer.default_font(),
                horizontal_alignment: Horizontal::Left,
                vertical_alignment: Vertical::Center,
                shaping: Shaping::Basic,
                wrapping: Wrapping::None,
            },
            Point::new(bounds.x + 8.0, bounds.center_y()),
            if state.is_dragging {
                Color::from_rgb8(147, 197, 253)
            } else {
                Color::from_rgb8(226, 232, 240)
            },
            *viewport,
        );
    }
}

impl<'a, Message: 'a> From<ScrubNumber<'a, Message>> for Element<'a, Message> {
    fn from(w: ScrubNumber<'a, Message>) -> Self {
        Element::new(w)
    }
}
