use std::time::{Duration, Instant};

use iced::widget::{container, mouse_area, row, space, text};
use iced::{Alignment, Border, Color, Element, Length, Subscription, Task, window};
use iced_layershell::reexport::{
    Anchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption,
};
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
    /// The hover popup, when one is open.
    popup: Option<Popup>,
    /// The module the pointer is currently inside.
    hovered: Option<usize>,
    /// A popup we have asked to remove. Its surface may still ask to be drawn
    /// once more, and it must not fall through to rendering the whole bar.
    retiring: Option<window::Id>,
}

struct Popup {
    /// Its own layer-shell surface, so it can extend past the bar's height.
    id: window::Id,
    /// The module it belongs to.
    module: usize,
    text: String,
    opened: Instant,
}

/// A new surface briefly disturbs the pointer state of the one below it, which
/// would close the tooltip the instant it appears. Leaves this soon after
/// opening are that echo rather than the user moving away.
const POPUP_GRACE: Duration = Duration::from_millis(120);

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
            popup: None,
            hovered: None,
            retiring: None,
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
    /// The pointer entered the module at this index.
    Enter(usize),
    /// The pointer left the module at this index.
    Leave(usize),
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
        Message::Enter(index) => {
            bar.hovered = Some(index);
            open_popup(bar)
        }
        Message::Leave(index) => {
            // A popup's own appearance makes iced report the pointer as having
            // left the bar. Treat a leave that arrives this soon as that echo,
            // and keep `hovered` intact so a later, real leave still closes it.
            if bar
                .popup
                .as_ref()
                .is_some_and(|popup| popup.opened.elapsed() < POPUP_GRACE)
            {
                return Task::none();
            }

            if bar.hovered == Some(index) {
                bar.hovered = None;
            }

            close_popup(bar)
        }
        // `to_layer_message` appends its own variants to this enum. Upstream's
        // example ends with `unreachable!()`, which is a panic waiting to happen.
        _ => Task::none(),
    }
}

/// Open a popup once both facts are known: which module is hovered, and where
/// the pointer is. The two arrive in separate messages, in no fixed order.
fn open_popup(bar: &mut Bar) -> Task<Message> {
    let Some(index) = bar.hovered else {
        return Task::none();
    };

    if bar
        .popup
        .as_ref()
        .is_some_and(|popup| popup.module == index)
    {
        return Task::none();
    }

    let Some(tooltip) = bar.modules.get(index).and_then(|module| module.tooltip()) else {
        return Task::none();
    };

    let closed = close_popup(bar);
    let id = window::Id::unique();
    let settings = tooltip_settings(bar, index, &tooltip);

    bar.popup = Some(Popup {
        id,
        module: index,
        text: tooltip,
        opened: Instant::now(),
    });

    Task::batch([closed, Task::done(Message::NewLayerShell { settings, id })])
}

fn close_popup(bar: &mut Bar) -> Task<Message> {
    match bar.popup.take() {
        Some(popup) => {
            bar.retiring = Some(popup.id);
            Task::done(Message::RemoveWindow(popup.id))
        }
        None => Task::none(),
    }
}

fn tooltip_settings(bar: &Bar, index: usize, tooltip: &str) -> NewLayerShellSettings {
    // A tooltip is its own layer surface rather than an xdg popup. The runtime
    // creates popups with a grab, which is right for a menu but takes every
    // pointer event away from the bar underneath for as long as it is open.
    let font_size = bar.config.bar.font_size;
    let width = (tooltip.chars().count() as f32)
        .mul_add(font_size * TOOLTIP_GLYPH_RATIO, 2.0 * TOOLTIP_PADDING)
        .clamp(48.0, 720.0) as u32;
    let height = font_size.mul_add(TOOLTIP_LINE_RATIO, 2.0 * TOOLTIP_PADDING) as u32;

    // Anchor under the module's own region. A layer surface anchored to
    // neither side is centred on that axis, and one anchored to a side cannot
    // run off it -- so the tooltip stays on screen without the bar's width,
    // which layer surfaces never report.
    let side = if bar.left.contains(&index) {
        Some(Anchor::Left)
    } else if bar.right.contains(&index) {
        Some(Anchor::Right)
    } else {
        None
    };

    let [top, _, bottom, _] = bar.config.bar.margin;

    // An exclusive bar already displaces the usable area, so the compositor
    // puts the tooltip directly below it. A non-exclusive one does not.
    let clearance = if bar.config.bar.exclusive {
        0
    } else {
        bar.config.bar.height as i32
    };

    let (edge, margin) = match bar.config.bar.position {
        config::Position::Top => (Anchor::Top, (top + clearance, 0, 0, 0)),
        config::Position::Bottom => (Anchor::Bottom, (0, 0, bottom + clearance, 0)),
    };

    NewLayerShellSettings {
        size: Some((width, height)),
        layer: Layer::Overlay,
        anchor: side.map_or(edge, |side| edge | side),
        exclusive_zone: Some(0),
        margin: Some(margin),
        keyboard_interactivity: KeyboardInteractivity::None,
        output_option: OutputOption::Active,
        // Without this the tooltip would take pointer events itself, and the
        // bar would never learn that the pointer had moved away.
        events_transparent: true,
        namespace: Some(String::from("ricebar-tooltip")),
    }
}

/// Text cannot be measured outside a renderer, so a tooltip's surface is sized
/// from the font size. Deliberately an over-estimate: too wide leaves harmless
/// empty space, while too narrow wraps the text and the surface clips it.
const TOOLTIP_GLYPH_RATIO: f32 = 0.6;
const TOOLTIP_LINE_RATIO: f32 = 1.4;
const TOOLTIP_PADDING: f32 = 10.0;

fn popup_view<'a>(bar: &'a Bar, popup: &'a Popup) -> Element<'a, Message> {
    let style = bar.config.bar.style;

    container(text(popup.text.as_str()).wrapping(text::Wrapping::None))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_y(Length::Fill)
        .padding([0.0, TOOLTIP_PADDING])
        .style(move |_theme| container::Style {
            background: Some(style.background.color().into()),
            text_color: Some(style.foreground.color()),
            border: Border {
                color: style.border_color.color(),
                width: 1.0,
                radius: 4.into(),
            },
            ..Default::default()
        })
        .into()
}

pub fn view(bar: &Bar, id: window::Id) -> Element<'_, Message> {
    if let Some(popup) = &bar.popup
        && popup.id == id
    {
        return popup_view(bar, popup);
    }

    if bar.retiring == Some(id) {
        return space::horizontal().into();
    }

    let style = bar.config.bar.style;
    let spacing = bar.config.bar.spacing;

    let region = |indices: &[usize]| {
        row(indices.iter().filter_map(|&index| {
            let module = bar.modules.get(index)?;

            let element = module
                .view(style)
                .map(move |event| Message::Module(index, event));

            Some(
                mouse_area(element)
                    .on_enter(Message::Enter(index))
                    .on_exit(Message::Leave(index))
                    .into(),
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
