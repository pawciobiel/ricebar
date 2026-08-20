use iced::widget::{container, row};
use iced::{Alignment, Border, Color, Element, Length, Subscription, Task, window};
use iced_layershell::to_layer_message;

use crate::config;
use crate::modules::{self, Event, Module};

/// Everything the bar knows. `update` is the only thing allowed to change it.
pub struct Bar {
    config: config::Config,
    /// Every enabled module, in one flat list so a single index identifies one.
    modules: Vec<Box<dyn Module>>,
    left: Vec<usize>,
    center: Vec<usize>,
    right: Vec<usize>,
}

impl Bar {
    pub fn new(config: config::Config) -> Self {
        let mut modules = Vec::new();

        let left = region(&mut modules, &config.bar.modules_left, &config.module);
        let center = region(&mut modules, &config.bar.modules_center, &config.module);
        let right = region(&mut modules, &config.bar.modules_right, &config.module);

        if modules.is_empty() {
            eprintln!("ricebar: no modules enabled; check the modules-* lists in config");
        } else {
            let names: Vec<&str> = modules.iter().map(|module| module.name()).collect();
            eprintln!("ricebar: {} module(s): {}", names.len(), names.join(", "));
        }

        Self {
            config,
            modules,
            left,
            center,
            right,
        }
    }
}

/// Build one region's modules, returning their indices into the flat list.
fn region(
    modules: &mut Vec<Box<dyn Module>>,
    names: &[String],
    config: &config::Modules,
) -> Vec<usize> {
    let mut indices = Vec::new();

    for name in names {
        match modules::build(name, config) {
            Some(module) => {
                indices.push(modules.len());
                modules.push(module);
            }
            None => eprintln!("ricebar: unknown module `{name}`, ignoring"),
        }
    }

    indices
}

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
pub enum Message {
    /// An event for the module at this index.
    Module(usize, Event),
}

pub fn namespace() -> String {
    String::from("ricebar")
}

pub fn subscription(bar: &Bar) -> Subscription<Message> {
    Subscription::batch(bar.modules.iter().enumerate().map(|(index, module)| {
        // `with` folds the index into the subscription's identity. Without it
        // iced would hash only the *type* of the closure below, so two modules
        // sharing an inner recipe would collapse into a single stream and one
        // of them would never receive an event.
        module
            .subscription()
            .with(index)
            .map(|(index, event)| Message::Module(index, event))
    }))
}

pub fn update(bar: &mut Bar, message: Message) -> Task<Message> {
    match message {
        Message::Module(index, event) => match bar.modules.get_mut(index) {
            Some(module) => module
                .update(event)
                .map(move |event| Message::Module(index, event)),
            None => Task::none(),
        },
        // `to_layer_message` appends its own variants to this enum. Upstream's
        // example ends with `unreachable!()`, which is a panic waiting to happen.
        _ => Task::none(),
    }
}

pub fn view(bar: &Bar, _id: window::Id) -> Element<'_, Message> {
    let style = bar.config.bar.style;
    let spacing = bar.config.bar.spacing;

    let region = |indices: &[usize]| {
        row(indices.iter().filter_map(|&index| {
            let module = bar.modules.get(index)?;

            Some(
                module
                    .view(style)
                    .map(move |event| Message::Module(index, event)),
            )
        }))
        .spacing(spacing)
        .align_y(Alignment::Center)
    };

    let content = row![
        container(region(&bar.left)).width(Length::FillPortion(1)),
        container(region(&bar.center)).center_x(Length::FillPortion(1)),
        container(region(&bar.right)).align_right(Length::FillPortion(1)),
    ]
    .align_y(Alignment::Center);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([0.0, bar.config.bar.padding])
        .style(move |_theme| container::Style {
            background: Some(style.background.color().into()),
            border: Border {
                color: style.border_color.color(),
                width: style.border_width,
                radius: style.border_radius.into(),
            },
            ..Default::default()
        })
        .into()
}

pub fn style(bar: &Bar, _theme: &iced::Theme) -> iced::theme::Style {
    iced::theme::Style {
        // The container in `view` paints the fill, so the surface itself stays
        // transparent. That is what lets border-radius round real corners
        // instead of cutting into an opaque rectangle.
        background_color: Color::TRANSPARENT,
        text_color: bar.config.bar.style.foreground.color(),
    }
}
