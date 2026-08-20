//! Bar modules.
//!
//! A module is enabled by naming it in one of the `modules-*` lists in config.
//! Adding one means implementing [`Module`] and adding an arm to [`build`].

pub mod clock;
pub mod custom;
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

    /// Text to reveal in a popup while the pointer rests on this module.
    fn tooltip(&self) -> Option<String> {
        None
    }
}

/// Turn a name from config into a module.
pub fn build(name: &str, config: &config::Modules) -> Option<Box<dyn Module>> {
    match name {
        "clock" => Some(Box::new(clock::Clock::new(&config.clock))),
        "workspaces" => Some(Box::new(workspaces::Workspaces::new(&config.workspaces))),
        // Anything else may be a user-defined command module.
        _ => config
            .custom
            .iter()
            .find(|custom| custom.name == name)
            .map(|custom| Box::new(custom::Custom::new(custom)) as Box<dyn Module>),
    }
}
