//! Bar modules.
//!
//! A module is enabled by naming it in one of the `modules-*` lists in config.
//! Adding one means implementing [`Module`] and adding an arm to [`build`].

pub mod clock;
pub mod custom;
pub mod icon;
pub mod menu;
pub mod notice;
pub mod sensor;
pub mod workspaces;

pub use icon::{Icon, labelled};

use iced::{Element, Subscription, Task};

use crate::compositor;
use crate::config;

/// Everything that can happen to a module.
///
/// One shared enum rather than an associated type per module: a trait object
/// cannot carry a generic message type, and the alternative — passing
/// `Box<dyn Any>` around and downcasting — is worse. Modules ignore the
/// variants they do not care about.
#[derive(Debug, Clone)]
pub enum Event {
    /// A timer fired.
    Tick,
    /// A fresh workspace snapshot arrived.
    Workspaces(compositor::Workspaces),
    /// The user asked to switch workspace.
    FocusWorkspace(i32),
    /// A module produced new content to display.
    Content(Content),
    /// The user clicked a module that owns a popup.
    TogglePopup,
    /// The user chose the entry at this index, which closes the popup.
    Activate(usize),
    /// The user paged the popup's contents, which leaves it open.
    Step(i32),
    /// The wheel turned over the module, away from the user or towards them.
    Scroll(Direction),
    /// A script produced the entries for a popup, which is what the bar waits
    /// for before it can size the surface.
    Entries(Vec<Entry>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

/// One line of a popup a script described.
#[derive(Debug, Clone)]
pub struct Entry {
    pub label: String,
    /// Passed to `sh -c` when the entry is chosen.
    pub exec: String,
}

/// The popup a module opens when clicked: a menu of commands, a calendar, or
/// whatever else it draws.
///
/// The module says how big it needs to be and what goes in it; creating and
/// destroying the surface is the bar's business.
/// Sized in pixels rather than characters: a grid knows its own geometry
/// exactly, and the surface cannot be resized once created.
#[derive(Debug, Clone, Copy)]
pub struct Popup {
    pub width: f32,
    pub height: f32,
}

/// What a module shows: a label, and optionally something longer to reveal on
/// hover.
#[derive(Debug, Clone, Default)]
pub struct Content {
    pub text: String,
    pub tooltip: Option<String>,
    /// Whether this is a failure rather than a reading, so the bar can mark it
    /// without having to guess from the text.
    pub failed: bool,
    /// 0 to 100, if the module reported one. Chooses an icon from a config's
    /// list, so a script reports a number and the look stays configurable.
    pub percentage: Option<f32>,
    /// An icon the script named for itself, overriding the one `percentage`
    /// would have chosen. For the cases where the icon is not a level at all:
    /// a weather condition, a keyboard layout, whether something is connected.
    pub icon: Option<String>,
}

impl Content {
    /// Something went wrong. Keep the bar readable and put the detail in the
    /// tooltip rather than stretching the bar with a stack trace.
    pub fn error(detail: String) -> Self {
        Self {
            text: String::from(BROKEN),
            tooltip: Some(detail),
            failed: true,
            percentage: None,
            icon: None,
        }
    }
}

/// Shown in place of a reading when something is wrong: U+F071, a warning
/// triangle. One marker everywhere, so a broken module looks broken rather
/// than looking like a module reporting a question mark.
pub const BROKEN: &str = "\u{f071}";

/// Run a command and leave it to get on with it.
///
/// Detached on purpose: these launch things, change the volume, power the
/// machine off. Waiting on one would hold up the bar while it happened.
pub fn spawn(command: String) -> Task<Event> {
    Task::future(async move {
        if let Err(error) = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&command)
            .spawn()
        {
            eprintln!("ricebar: could not run `{command}`: {error}");
        }
    })
    .discard()
}

/// Pick an icon for how full something is, from a list running low to high.
///
/// Shared so a script reporting a percentage and a built-in sensor reading one
/// choose their icon the same way.
pub fn icon_for(level: f32, icons: &[Icon]) -> Option<&Icon> {
    let last = icons.len().checked_sub(1)?;
    let index = (level.clamp(0.0, 1.0) * last as f32).round() as usize;

    icons.get(index.min(last))
}

/// Pick a colour for how full something is, from a list running low to high.
/// `None` when no colours were configured, so the caller keeps its own.
pub fn color_for(level: f32, colors: &[config::Rgba]) -> Option<config::Rgba> {
    let last = colors.len().checked_sub(1)?;
    let index = (level.clamp(0.0, 1.0) * last as f32).round() as usize;

    colors.get(index.min(last)).copied()
}

pub trait Module {
    /// The name this module is enabled by in config.
    fn name(&self) -> &str;

    /// What should wake this module up.
    fn subscription(&self) -> Subscription<Event> {
        Subscription::none()
    }

    /// React to an event, optionally scheduling further work.
    fn update(&mut self, event: Event) -> Task<Event>;

    /// Render, using the bar's palette.
    fn view(&self, style: config::Style) -> Element<'_, Event>;

    /// Text to reveal while the pointer rests on this module.
    ///
    /// Independent of [`Module::popup`]: a module may have either, both or
    /// neither. Hovering shows this; clicking opens that.
    fn tooltip(&self) -> Option<String> {
        None
    }

    /// The popup this module opens when clicked, if it has one.
    fn popup(&self) -> Option<Popup> {
        None
    }

    /// Draw that popup. Only called for modules that return a [`Popup`].
    fn popup_view(&self, style: config::Style) -> Element<'_, Event> {
        let _ = style;
        iced::widget::space::horizontal().into()
    }
}

/// Turn a name from config into a module.
///
/// `trusted` says whether the config may be taken as a source of shell
/// commands; modules that run any refuse when it is false.
pub fn build(name: &str, config: &config::Modules, trusted: bool) -> Option<Box<dyn Module>> {
    match name {
        "clock" => Some(Box::new(clock::Clock::new(&config.clock, trusted))),
        "workspaces" => Some(Box::new(workspaces::Workspaces::new(&config.workspaces))),
        "cpu" => Some(Box::new(sensor::Sensor::new(
            sensor::Kind::Cpu,
            &config.cpu,
        ))),
        "memory" => Some(Box::new(sensor::Sensor::new(
            sensor::Kind::Memory,
            &config.memory,
        ))),
        "temperature" => Some(Box::new(sensor::Sensor::new(
            sensor::Kind::Temperature,
            &config.temperature,
        ))),
        "battery" => Some(Box::new(sensor::Sensor::new(
            sensor::Kind::Battery,
            &config.battery,
        ))),
        "backlight" => Some(Box::new(sensor::Sensor::new(
            sensor::Kind::Backlight,
            &config.backlight,
        ))),
        // Anything else is user-defined, by name.
        _ => config
            .custom
            .iter()
            .find(|custom| custom.name == name)
            .map(|custom| Box::new(custom::Custom::new(custom, trusted)) as Box<dyn Module>)
            .or_else(|| {
                config
                    .menu
                    .iter()
                    .find(|menu| menu.name == name)
                    .map(|menu| Box::new(menu::Menu::new(menu, trusted)) as Box<dyn Module>)
            }),
    }
}
