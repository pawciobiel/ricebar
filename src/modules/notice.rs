//! A warning the bar shows about itself.

use iced::widget::text;
use iced::{Element, Task};

use super::{Content, Event, Module};
use crate::config;

/// Something wrong with the config, put in the bar because that is the only
/// place the user is looking.
///
/// A bar started by the compositor has no terminal attached, so a mistake in
/// the config would otherwise be silent: saving the file would simply appear
/// to do nothing. This is the same warning triangle a broken module shows,
/// with the detail on hover.
pub struct Notice {
    content: Content,
}

impl Notice {
    pub fn new(detail: String) -> Self {
        Self {
            content: Content::error(detail),
        }
    }
}

impl Module for Notice {
    fn name(&self) -> &str {
        "notice"
    }

    fn update(&mut self, event: Event) -> Task<Event> {
        if let Event::Content(content) = event {
            self.content = content;
        }

        Task::none()
    }

    fn view(&self, style: config::Style) -> Element<'_, Event> {
        text(&self.content.text).color(style.urgent.color()).into()
    }

    fn tooltip(&self) -> Option<String> {
        self.content.tooltip.clone()
    }
}
