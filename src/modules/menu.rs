//! A label that opens a menu of commands, such as a power menu.
//!
//! The module owns the entries and runs whichever one is chosen. Presenting
//! them is the bar's job, since the menu needs a surface of its own.

use iced::widget::{button, text};
use iced::{Element, Task};

use super::{Event, Module};
use crate::config;

pub struct Menu {
    name: String,
    label: String,
    items: Vec<config::MenuItem>,
    tooltip: Option<String>,
}

impl Menu {
    pub fn new(config: &config::Menu) -> Self {
        Self {
            name: config.name.clone(),
            label: config.label.clone(),
            items: config.items.clone(),
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
            .on_press(Event::ToggleMenu)
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

    fn menu(&self) -> Option<&[config::MenuItem]> {
        Some(&self.items)
    }
}
