//! Bar modules.
//!
//! A module is enabled by naming it in one of the `modules-*` lists in config.
//! Adding one means implementing [`Module`] and adding an arm to [`build`].

pub mod clock;
pub mod custom;
pub mod menu;
pub mod workspaces;

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
}

/// The popup a module opens when clicked: a menu of commands, a calendar, or
/// whatever else it draws.
///
/// The module says how big it needs to be and what goes in it; creating and
/// destroying the surface is the bar's business.
#[derive(Debug, Clone, Copy)]
pub struct Popup {
    /// Roughly how many characters wide.
    pub columns: usize,
    /// Roughly how many lines tall.
    pub rows: usize,
}

/// What a module shows: a label, and optionally something longer to reveal on
/// hover.
#[derive(Debug, Clone, Default)]
pub struct Content {
    pub text: String,
    pub tooltip: Option<String>,
}

impl Content {
    /// Something went wrong. Keep the bar readable and put the detail in the
    /// tooltip rather than stretching the bar with a stack trace.
    pub fn error(detail: String) -> Self {
        Self {
            text: String::from("!"),
            tooltip: Some(detail),
        }
    }
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
pub fn build(name: &str, config: &config::Modules) -> Option<Box<dyn Module>> {
    match name {
        "clock" => Some(Box::new(clock::Clock::new(&config.clock))),
        "workspaces" => Some(Box::new(workspaces::Workspaces::new(&config.workspaces))),
        // Anything else is user-defined, by name.
        _ => config
            .custom
            .iter()
            .find(|custom| custom.name == name)
            .map(|custom| Box::new(custom::Custom::new(custom)) as Box<dyn Module>)
            .or_else(|| {
                config
                    .menu
                    .iter()
                    .find(|menu| menu.name == name)
                    .map(|menu| Box::new(menu::Menu::new(menu)) as Box<dyn Module>)
            }),
    }
}
