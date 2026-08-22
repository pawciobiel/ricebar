//! A label that opens a menu of commands, such as a power menu.
//!
//! The module owns the entries and runs whichever one is chosen. Presenting
//! them is the bar's job, since the menu needs a surface of its own.

use iced::widget::{button, column, text};
use iced::{Element, Length, Task};

use super::{Event, Module, Popup};
use crate::config;

/// Per-entry metrics, used to size the surface before anything is drawn.
const ENTRY_GLYPH: f32 = 9.5;
const ENTRY_HEIGHT: f32 = 24.0;
const ENTRY_PADDING: f32 = 10.0;

pub struct Menu {
    name: String,
    label: String,
    items: Vec<config::MenuItem>,
    trusted: bool,
    tooltip: Option<String>,
}

impl Menu {
    pub fn new(config: &config::Menu, trusted: bool) -> Self {
        Self {
            name: config.name.clone(),
            label: config.label.clone(),
            items: config.items.clone(),
            trusted,
            tooltip: config.tooltip.clone(),
        }
    }
}

impl Module for Menu {
    fn name(&self) -> &str {
        &self.name
    }

    fn update(&mut self, event: Event) -> Task<Event> {
        let Event::Activate(index) = event else {
            return Task::none();
        };

        if !self.trusted {
            eprintln!("ricebar: refusing to run menu commands from a config others can write");
            return Task::none();
        }

        let Some(item) = self.items.get(index) else {
            return Task::none();
        };

        let command = item.exec.clone();

        Task::future(async move {
            // Detached on purpose. These commands suspend or power off the
            // machine, so waiting on one would block the bar while it happens.
            match tokio::process::Command::new("sh")
                .arg("-c")
                .arg(&command)
                .spawn()
            {
                Ok(_) => {}
                Err(error) => eprintln!("ricebar: could not run `{command}`: {error}"),
            }
        })
        .discard()
    }

    fn view(&self, style: config::Style) -> Element<'_, Event> {
        button(text(self.label.as_str()))
            .padding([2, 6])
            .on_press(Event::TogglePopup)
            .style(move |_theme, status| button::Style {
                background: match status {
                    button::Status::Hovered | button::Status::Pressed => {
                        Some(style.muted.color().into())
                    }
                    _ => None,
                },
                text_color: style.foreground.color(),
                border: iced::Border {
                    radius: 4.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }

    fn tooltip(&self) -> Option<String> {
        self.tooltip.clone()
    }

    fn popup(&self) -> Option<Popup> {
        let widest = self
            .items
            .iter()
            .map(|item| item.label.chars().count())
            .max()
            .unwrap_or(0) as f32;

        Some(Popup {
            // Text cannot be measured outside a renderer, so this over-estimates
            // rather than risk clipping a label.
            width: widest.mul_add(ENTRY_GLYPH, 2.0 * ENTRY_PADDING),
            height: (self.items.len() as f32).mul_add(ENTRY_HEIGHT, 2.0 * ENTRY_PADDING),
        })
    }

    fn popup_view(&self, style: config::Style) -> Element<'_, Event> {
        let entries = self.items.iter().enumerate().map(|(entry, item)| {
            button(text(item.label.as_str()).wrapping(text::Wrapping::None))
                .width(Length::Fill)
                .padding([2, 6])
                .on_press(Event::Activate(entry))
                .style(move |_theme, status| button::Style {
                    background: match status {
                        button::Status::Hovered | button::Status::Pressed => {
                            Some(style.accent.color().into())
                        }
                        _ => None,
                    },
                    text_color: match status {
                        button::Status::Hovered | button::Status::Pressed => {
                            style.background.color()
                        }
                        _ => style.foreground.color(),
                    },
                    border: iced::Border {
                        radius: 4.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
        });

        column(entries).spacing(2).into()
    }
}
