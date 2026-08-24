use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use iced::futures::{SinkExt, Stream};
use iced::widget::{column, container, mouse_area, row, space, text};
use iced::{Alignment, Border, Color, Element, Length, Subscription, Task, window};
use iced_layershell::reexport::{
    Anchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption,
};
use iced_layershell::to_layer_message;

use crate::config;
use crate::modules::{self, Direction, Event, Module};

/// Everything the bar knows. `update` is the only thing allowed to change it.
pub struct Bar {
    config: config::Config,
    /// Every enabled module, in one flat list so a single index identifies one.
    modules: Vec<Box<dyn Module>>,
    /// One entry per line of the bar, stacked top to bottom.
    rows: Vec<Row>,
    /// The hover popup, when one is open.
    popup: Option<Popup>,
    /// The module the pointer is currently inside.
    hovered: Option<usize>,
    /// Scroll accumulated per module. A wheel reports whole notches, but a
    /// touchpad reports a stream of small ones, and acting on each would run
    /// the command dozens of times for a single flick.
    scrolled: HashMap<usize, f32>,
    /// A popup we have asked to remove. Its surface may still ask to be drawn
    /// once more, and it must not fall through to rendering the whole bar.
    retiring: Option<window::Id>,
    /// Where the warning about the config sits in `modules`, once there is one.
    notice: Option<usize>,
    /// How many times the config has been reloaded. Part of every
    /// subscription's identity, so a reload restarts the streams behind the
    /// new modules rather than leaving them attached to the old ones.
    generation: u64,
}

/// One line of the bar, holding indices into `modules`.
struct Row {
    height: u32,
    left: Vec<usize>,
    center: Vec<usize>,
    right: Vec<usize>,
}

struct Popup {
    /// Its own layer-shell surface, so it can extend past the bar's height.
    id: window::Id,
    /// The module it belongs to.
    module: usize,
    kind: PopupKind,
    opened: Instant,
}

enum PopupKind {
    /// Hover text. Takes no pointer input, so the bar keeps receiving it. The
    /// text is not kept here: it is read from the module as it is drawn, so a
    /// tooltip showing a value follows that value while it is open.
    Tooltip,
    /// Whatever the module draws for itself: a menu, a calendar. Takes pointer
    /// input, since it exists to be clicked.
    Module,
}

/// A new surface briefly disturbs the pointer state of the one below it, which
/// would close the tooltip the instant it appears. Leaves this soon after
/// opening are that echo rather than the user moving away.
const POPUP_GRACE: Duration = Duration::from_millis(120);

impl Bar {
    pub fn new(config: config::Config) -> Self {
        let mut modules = Vec::new();

        let trusted = config.trusted;

        let rows: Vec<Row> = config
            .bar
            .rows()
            .into_iter()
            .map(|line| Row {
                height: line.height.unwrap_or(config.bar.height),
                left: region(&mut modules, &line.modules_left, &config.module, trusted),
                center: region(&mut modules, &line.modules_center, &config.module, trusted),
                right: region(&mut modules, &line.modules_right, &config.module, trusted),
            })
            .collect();

        if modules.is_empty() {
            eprintln!("ricebar: no modules enabled; check the modules-* lists in config");
        } else {
            let names: Vec<&str> = modules.iter().map(|module| module.name()).collect();
            eprintln!("ricebar: {} module(s): {}", names.len(), names.join(", "));
        }

        let mut bar = Self {
            config,
            modules,
            rows,
            popup: None,
            hovered: None,
            scrolled: HashMap::new(),
            retiring: None,
            notice: None,
            generation: 0,
        };

        // A config that could not be read is why this bar looks like the
        // defaults, and that is worth saying where it will be seen.
        if let Some(problem) = bar.config.problem.clone() {
            bar.warn(problem);
        }

        bar
    }

    /// Show a warning about the bar's own configuration, in the bar.
    ///
    /// The detail goes to stderr as well, but a bar started by the compositor
    /// has no terminal attached: without this, a mistake in the config is
    /// invisible and saving the file simply appears to do nothing.
    fn warn(&mut self, detail: String) {
        let notice: Box<dyn Module> = Box::new(modules::notice::Notice::new(detail));

        match self.notice.and_then(|index| self.modules.get_mut(index)) {
            Some(existing) => *existing = notice,
            None => {
                let index = self.modules.len();
                self.modules.push(notice);
                self.notice = Some(index);

                // The end of the first line, which is where the eye goes and
                // where a bar of any shape has something visible.
                if let Some(first) = self.rows.first_mut() {
                    first.right.push(index);
                }
            }
        }
    }
}

/// Build one region's modules, returning their indices into the flat list.
fn region(
    modules: &mut Vec<Box<dyn Module>>,
    names: &[String],
    config: &config::Modules,
    trusted: bool,
) -> Vec<usize> {
    let mut indices = Vec::new();

    for name in names {
        match modules::build(name, config, trusted) {
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
    /// The pointer scrolled over the module at this index, by however much it
    /// reports at a time.
    Scroll(usize, iced::mouse::ScrollDelta),
    /// The config file was written to.
    ConfigChanged,
}

/// How far a touchpad must travel to count as one notch of a wheel.
const PIXELS_PER_NOTCH: f32 = 40.0;
/// The most notches a flick may bank, so letting go of a fast scroll does not
/// keep firing the command afterwards.
const NOTCH_LIMIT: f32 = 2.0;

/// A scroll in notches, whichever units the pointer reports in.
fn notches(delta: iced::mouse::ScrollDelta) -> f32 {
    match delta {
        iced::mouse::ScrollDelta::Lines { y, .. } => y,
        iced::mouse::ScrollDelta::Pixels { y, .. } => y / PIXELS_PER_NOTCH,
    }
}

pub fn namespace() -> String {
    String::from("ricebar")
}

pub fn subscription(bar: &Bar) -> Subscription<Message> {
    let generation = bar.generation;

    let modules = Subscription::batch(bar.modules.iter().enumerate().map(|(index, module)| {
        // `with` folds the index into the subscription's identity. Without it
        // iced would hash only the *type* of the closure below, so two modules
        // sharing an inner recipe would collapse into a single stream and one
        // of them would never receive an event.
        //
        // The generation is in there for the same reason, one level up. A
        // reload builds new modules, but iced recognises a subscription it is
        // already running and keeps the old stream -- which has already sent
        // its opening state, so the new module would sit empty waiting for a
        // change. Workspaces stayed blank; a weather script would have been
        // blank until its next print, a quarter of an hour later.
        module
            .subscription()
            .with((generation, index))
            .map(|((_, index), event)| Message::Module(index, event))
    }));

    let Some(path) = bar.config.path.clone() else {
        return modules;
    };

    Subscription::batch([
        modules,
        Subscription::run_with(path, watch).map(|()| Message::ConfigChanged),
    ])
}

/// How often the config is checked for changes. A `stat` costs almost nothing,
/// and a second is soon enough to feel immediate after saving the file.
const POLL: Duration = Duration::from_secs(1);

/// Emit once each time the config file changes.
///
/// Polling rather than inotify: it needs no dependency, and it copes with the
/// editors that save by writing a new file and renaming it over the old one,
/// which a watch on the original inode would miss entirely.
// `&Path` would be the idiomatic argument, but the subscription's key type is
// what fixes this signature: `run_with` wants `Fn(&PathBuf)`, and a function
// taking `&Path` does not coerce to it.
#[allow(clippy::ptr_arg)]
fn watch(path: &PathBuf) -> impl Stream<Item = ()> + use<> {
    let path = path.clone();

    iced::stream::channel(1, async move |mut output| {
        let mut seen = stamp(&path);

        loop {
            tokio::time::sleep(POLL).await;

            let found = stamp(&path);
            if found == seen {
                continue;
            }
            seen = found;

            // A file that has just vanished is usually an editor partway
            // through replacing it, and the write that follows is the change
            // worth acting on.
            if found.is_none() {
                continue;
            }

            if output.send(()).await.is_err() {
                return;
            }
        }
    })
}

/// What identifies a version of the file: when it was written, and how long it
/// is. Cheaper than reading it, and enough to notice a save.
fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let found = std::fs::metadata(path).ok()?;
    Some((found.modified().ok()?, found.len()))
}

/// Re-read the config and rebuild from it, or explain why it was not applied.
fn reload(bar: &mut Bar) -> Task<Message> {
    let Some(path) = bar.config.path.clone() else {
        return Task::none();
    };

    let next = match config::reread(&path) {
        Ok(next) => next,
        Err(problem) => {
            eprintln!(
                "ricebar: {} is invalid, keeping the running config:",
                path.display()
            );
            eprintln!("{problem}");
            bar.warn(format!(
                "{} is invalid.\nThe bar is still running the config it started with.\n\n{problem}",
                path.display()
            ));
            return Task::none();
        }
    };

    let fixed = config::fixed_differences(&bar.config, &next);
    if !fixed.is_empty() {
        let changed = fixed.join(", ");
        eprintln!("ricebar: {changed} changed; restart ricebar to apply");
        bar.warn(format!(
            "{changed} changed.\nThe bar's surface is sized and placed once, when it is created, so restart ricebar to apply this."
        ));
        return Task::none();
    }

    // Everything drawn comes from the config, so a new Bar is the cheapest
    // rebuild that cannot leave anything stale behind.
    let closed = close_popup(bar);
    let retiring = bar.retiring;
    let generation = bar.generation.wrapping_add(1);

    *bar = Bar::new(next);

    // The popup's surface is still on its way out, and its last draw must not
    // fall through to rendering the whole bar into it.
    bar.retiring = retiring;
    bar.generation = generation;

    eprintln!("ricebar: reloaded {}", path.display());
    closed
}

pub fn update(bar: &mut Bar, message: Message) -> Task<Message> {
    match message {
        // Opening and closing surfaces is the bar's job, so these are handled
        // here as well as being passed down.
        Message::Module(index, Event::TogglePopup) => {
            let told = match bar.modules.get_mut(index) {
                Some(module) => module
                    .update(Event::TogglePopup)
                    .map(move |event| Message::Module(index, event)),
                None => Task::none(),
            };

            Task::batch([told, toggle_popup(bar, index)])
        }
        Message::Module(index, Event::Entries(entries)) => {
            let told = match bar.modules.get_mut(index) {
                Some(module) => module
                    .update(Event::Entries(entries))
                    .map(move |event| Message::Module(index, event)),
                None => Task::none(),
            };

            // A script has answered, so the surface can be sized and shown now
            // rather than on the click that asked for it.
            Task::batch([told, show_popup(bar, index)])
        }
        Message::Module(index, Event::Activate(entry)) => {
            let closed = close_popup(bar);

            let ran = match bar.modules.get_mut(index) {
                Some(module) => module
                    .update(Event::Activate(entry))
                    .map(move |event| Message::Module(index, event)),
                None => Task::none(),
            };

            Task::batch([closed, ran])
        }
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
        Message::Scroll(index, delta) => {
            let banked = bar.scrolled.entry(index).or_default();
            *banked = (*banked + notches(delta)).clamp(-NOTCH_LIMIT, NOTCH_LIMIT);

            // Spend one whole notch, keeping the remainder for the next event.
            let direction = if *banked >= 1.0 {
                *banked -= 1.0;
                Direction::Up
            } else if *banked <= -1.0 {
                *banked += 1.0;
                Direction::Down
            } else {
                return Task::none();
            };

            match bar.modules.get_mut(index) {
                Some(module) => module
                    .update(Event::Scroll(direction))
                    .map(move |event| Message::Module(index, event)),
                None => Task::none(),
            }
        }
        Message::Leave(index) => {
            // A module's popup waits to be clicked, so it must survive the
            // pointer travelling from the module down to it.
            if matches!(
                bar.popup.as_ref().map(|popup| &popup.kind),
                Some(PopupKind::Module)
            ) {
                return Task::none();
            }

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
        Message::ConfigChanged => reload(bar),
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

    // Something the user clicked open outranks hover text.
    if matches!(
        bar.popup.as_ref().map(|popup| &popup.kind),
        Some(PopupKind::Module)
    ) {
        return Task::none();
    }

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
    // Text cannot be measured outside a renderer, so a tooltip's surface is
    // estimated from its longest line and how many lines there are.
    let font_size = bar.config.bar.font_size;

    let widest = tooltip
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or_default() as f32;
    let lines = tooltip.lines().count().max(1) as f32;

    let width = widest.mul_add(font_size * TOOLTIP_GLYPH_RATIO, 2.0 * TOOLTIP_PADDING);
    let height = lines.mul_add(font_size * TOOLTIP_LINE_RATIO, 2.0 * TOOLTIP_PADDING);

    let settings = popup_settings(bar, index, width, height, true);

    bar.popup = Some(Popup {
        id,
        module: index,
        kind: PopupKind::Tooltip,
        opened: Instant::now(),
    });

    Task::batch([closed, Task::done(Message::NewLayerShell { settings, id })])
}

/// Show or hide the popup belonging to a module.
fn toggle_popup(bar: &mut Bar, index: usize) -> Task<Message> {
    // A second click on the same module closes what the first one opened.
    if bar
        .popup
        .as_ref()
        .is_some_and(|popup| popup.module == index && matches!(popup.kind, PopupKind::Module))
    {
        return close_popup(bar);
    }

    show_popup(bar, index)
}

/// Open a module's popup, replacing whatever else was open.
///
/// A module whose popup comes from a script has nothing to show until that
/// script answers, which is why this can be reached twice: once from the click
/// and again when the entries arrive.
fn show_popup(bar: &mut Bar, index: usize) -> Task<Message> {
    if bar
        .popup
        .as_ref()
        .is_some_and(|popup| popup.module == index && matches!(popup.kind, PopupKind::Module))
    {
        return Task::none();
    }

    let Some(shape) = bar.modules.get(index).and_then(|module| module.popup()) else {
        return Task::none();
    };

    let closed = close_popup(bar);
    let id = window::Id::unique();
    // Unlike a tooltip, this one accepts pointer input, since it is clicked.
    let settings = popup_settings(bar, index, shape.width, shape.height, false);

    bar.popup = Some(Popup {
        id,
        module: index,
        kind: PopupKind::Module,
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

/// Build the surface for a tooltip or a module's own popup, at the given size.
///
/// Both are layer surfaces rather than xdg popups. The runtime builds popups
/// with a grab, which suits a menu but takes every pointer event away from the
/// bar underneath for as long as one is open, leaving no way to dismiss it.
fn popup_settings(
    bar: &Bar,
    index: usize,
    width: f32,
    height: f32,
    transparent_to_events: bool,
) -> NewLayerShellSettings {
    let width = width.clamp(48.0, 720.0) as u32;
    let height = height.max(1.0) as u32;

    // Anchor under the module's own region. A layer surface anchored to
    // neither side is centred on that axis, and one anchored to a side cannot
    // run off it -- so the tooltip stays on screen without the bar's width,
    // which layer surfaces never report.
    let side = if bar.rows.iter().any(|row| row.left.contains(&index)) {
        Some(Anchor::Left)
    } else if bar.rows.iter().any(|row| row.right.contains(&index)) {
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
        // A tooltip must not take pointer events, or the bar would never learn
        // that the pointer had moved away. A menu exists to be clicked.
        events_transparent: transparent_to_events,
        namespace: Some(String::from("ricebar-popup")),
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

    let body: Element<'a, Message> = match &popup.kind {
        PopupKind::Tooltip => {
            // Read at draw time rather than kept from when it opened, so a
            // tooltip showing a value follows that value while it is open.
            let tooltip = bar
                .modules
                .get(popup.module)
                .and_then(|module| module.tooltip())
                .unwrap_or_default();

            text(tooltip).wrapping(text::Wrapping::None).into()
        }
        PopupKind::Module => match bar.modules.get(popup.module) {
            Some(module) => {
                let index = popup.module;
                module
                    .popup_view(style)
                    .map(move |event| Message::Module(index, event))
            }
            None => space::horizontal().into(),
        },
    };

    container(body)
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
                    .on_scroll(move |delta| Message::Scroll(index, delta))
                    .into(),
            )
        }))
        .spacing(spacing)
        .align_y(Alignment::Center)
    };

    let lines = bar.rows.iter().map(|line| {
        let content = row![
            container(region(&line.left)).width(Length::FillPortion(1)),
            container(region(&line.center)).center_x(Length::FillPortion(1)),
            container(region(&line.right)).align_right(Length::FillPortion(1)),
        ]
        .align_y(Alignment::Center);

        container(content)
            .width(Length::Fill)
            .height(Length::Fixed(line.height as f32))
            // The row's own `align_y` only lines the modules up against each
            // other. Without this each line sits at the top of its own height.
            .align_y(bar.config.bar.vertical_align)
            .into()
    });

    container(column(lines))
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
