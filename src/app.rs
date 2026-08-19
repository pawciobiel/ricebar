use std::time::Duration;

use iced::widget::{button, container, row, space, text};
use iced::{Alignment, Color, Element, Length, Subscription, Task, window};
use iced_layershell::to_layer_message;

use crate::clock::Clock;
use crate::compositor::{self, Compositor, Workspace, Workspaces};

/// Everything the bar knows. `update` is the only thing allowed to change it.
pub struct Bar {
    clock: Clock,
    compositor: Option<Box<dyn Compositor>>,
    workspaces: Workspaces,
}

impl Default for Bar {
    fn default() -> Self {
        let compositor = compositor::detect();

        match &compositor {
            Some(compositor) => eprintln!("ricebar: compositor: {}", compositor.name()),
            None => eprintln!("ricebar: no supported compositor detected"),
        }

        Self {
            clock: Clock::new("%Y-%m-%d %H:%M:%S"),
            compositor,
            workspaces: Workspaces::new(),
        }
    }
}

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    Workspaces(Workspaces),
    FocusWorkspace(i32),
}

pub fn namespace() -> String {
    String::from("ricebar")
}

pub fn subscription(bar: &Bar) -> Subscription<Message> {
    let clock = iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick);

    match &bar.compositor {
        Some(compositor) => {
            Subscription::batch([clock, compositor.workspaces().map(Message::Workspaces)])
        }
        None => clock,
    }
}

pub fn update(bar: &mut Bar, message: Message) -> Task<Message> {
    match message {
        Message::Tick => {
            bar.clock.tick();
            Task::none()
        }
        Message::Workspaces(workspaces) => {
            bar.workspaces = workspaces;
            Task::none()
        }
        Message::FocusWorkspace(id) => match &bar.compositor {
            Some(compositor) => compositor.focus(id).discard(),
            None => Task::none(),
        },
        // `to_layer_message` appends its own variants to this enum. Upstream's
        // example ends with `unreachable!()`, which is a panic waiting to happen.
        _ => Task::none(),
    }
}

pub fn view(bar: &Bar, _id: window::Id) -> Element<'_, Message> {
    let workspaces = row(bar.workspaces.iter().map(workspace)).spacing(4);

    let bar = row![
        container(workspaces).width(Length::FillPortion(1)),
        container(text(bar.clock.label())).center_x(Length::FillPortion(1)),
        container(space::horizontal()).width(Length::FillPortion(1)),
    ]
    .align_y(Alignment::Center);

    container(bar)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([0, 8])
        .into()
}

fn workspace(workspace: &Workspace) -> Element<'_, Message> {
    let occupied = workspace.windows > 0;
    let focused = workspace.focused;
    let visible = workspace.visible;

    button(text(workspace.name.as_str()))
        .padding([2, 8])
        .on_press(Message::FocusWorkspace(workspace.id))
        .style(move |_theme, _status| {
            let background = if focused {
                Some(Color::from_rgb8(0x89, 0xb4, 0xfa).into())
            } else if visible {
                Some(Color::from_rgb8(0x45, 0x47, 0x5a).into())
            } else {
                None
            };

            button::Style {
                background,
                text_color: if focused {
                    Color::from_rgb8(0x1e, 0x1e, 0x2e)
                } else if occupied {
                    Color::from_rgb8(0xcd, 0xd6, 0xf4)
                } else {
                    Color::from_rgb8(0x6c, 0x70, 0x86)
                },
                border: iced::Border {
                    radius: 4.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
}

pub fn style(_bar: &Bar, _theme: &iced::Theme) -> iced::theme::Style {
    iced::theme::Style {
        background_color: Color::from_rgb8(0x1e, 0x1e, 0x2e),
        text_color: Color::from_rgb8(0xcd, 0xd6, 0xf4),
    }
}
