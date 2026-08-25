use iced::widget::{button, row, text};
use iced::{Element, Subscription, Task};

use super::icon::faced;
use super::{Event, Module};
use crate::compositor::{self, Compositor, Workspace};
use crate::config;

pub struct Workspaces {
    compositor: Option<Box<dyn Compositor>>,
    workspaces: compositor::Workspaces,
    show_empty: bool,
}

impl Workspaces {
    pub fn new(config: &config::Workspaces) -> Self {
        let compositor = compositor::detect(config.compositor);

        match &compositor {
            Some(compositor) => eprintln!("ricebar: compositor: {}", compositor.name()),
            None => eprintln!("ricebar: no supported compositor detected"),
        }

        Self {
            compositor,
            workspaces: compositor::Workspaces::new(),
            show_empty: config.show_empty,
        }
    }
}

impl Module for Workspaces {
    fn name(&self) -> &str {
        "workspaces"
    }

    fn subscription(&self) -> Subscription<Event> {
        match &self.compositor {
            Some(compositor) => compositor.workspaces().map(Event::Workspaces),
            None => Subscription::none(),
        }
    }

    fn update(&mut self, event: Event) -> Task<Event> {
        match event {
            Event::Workspaces(workspaces) => {
                self.workspaces = workspaces;
                Task::none()
            }
            Event::FocusWorkspace(id) => match &self.compositor {
                Some(compositor) => compositor.focus(id).discard(),
                None => Task::none(),
            },
            // The shared enum carries events for every kind of module.
            _ => Task::none(),
        }
    }

    fn view(&self, style: config::Style) -> Element<'_, Event> {
        let shown = self
            .workspaces
            .iter()
            // An empty workspace still has to be drawn while it is on screen,
            // or the one you just switched to would vanish under your cursor.
            .filter(|workspace| self.show_empty || workspace.windows > 0 || workspace.visible)
            .map(move |workspace| pill(workspace, style));

        row(shown).spacing(4).into()
    }
}

fn pill(workspace: &Workspace, style: config::Style) -> Element<'_, Event> {
    let focused = workspace.focused;
    let visible = workspace.visible;
    let occupied = workspace.windows > 0;

    button(faced(text(workspace.name.as_str()), style))
        .padding([2, 8])
        .on_press(Event::FocusWorkspace(workspace.id))
        .style(move |_theme, _status| button::Style {
            background: if focused {
                Some(style.accent.color().into())
            } else if visible {
                Some(style.muted.color().into())
            } else {
                None
            },
            text_color: if focused {
                style.background.color()
            } else if occupied {
                style.foreground.color()
            } else {
                style.dim.color()
            },
            border: iced::Border {
                radius: 4.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}
