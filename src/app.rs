use std::time::Duration;

use iced::widget::{container, text};
use iced::{Color, Element, Length, Subscription, Task, window};
use iced_layershell::to_layer_message;

use crate::clock::Clock;

/// Everything the bar knows. `update` is the only thing allowed to change it.
pub struct Bar {
    clock: Clock,
}

impl Default for Bar {
    fn default() -> Self {
        Self {
            clock: Clock::new("%Y-%m-%d %H:%M:%S"),
        }
    }
}

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
pub enum Message {
    Tick,
}

pub fn namespace() -> String {
    String::from("ricebar")
}

pub fn subscription(_bar: &Bar) -> Subscription<Message> {
    iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick)
}

pub fn update(bar: &mut Bar, message: Message) -> Task<Message> {
    match message {
        Message::Tick => {
            bar.clock.tick();
            Task::none()
        }
        // `to_layer_message` appends its own variants to this enum. Upstream's
        // example ends with `unreachable!()`, which is a panic waiting to happen.
        _ => Task::none(),
    }
}

pub fn view(bar: &Bar, _id: window::Id) -> Element<'_, Message> {
    container(text(bar.clock.label()))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}

pub fn style(_bar: &Bar, _theme: &iced::Theme) -> iced::theme::Style {
    iced::theme::Style {
        background_color: Color::from_rgb8(0x1e, 0x1e, 0x2e),
        text_color: Color::from_rgb8(0xcd, 0xd6, 0xf4),
    }
}
