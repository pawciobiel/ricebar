//! Bar modules.
//!
//! A module is enabled by naming it in one of the `modules-*` lists in config.
//! Adding one means implementing [`Module`] and adding an arm to [`build`].

pub mod clock;
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
}

pub trait Module {
    /// The name this module is enabled by in config.
    fn name(&self) -> &'static str;

    /// What should wake this module up.
    fn subscription(&self) -> Subscription<Event> {
        Subscription::none()
    }

    /// React to an event, optionally scheduling further work.
    fn update(&mut self, event: Event) -> Task<Event>;

    /// Render, using the bar's palette.
    fn view(&self, style: config::Style) -> Element<'_, Event>;
}

/// Turn a name from config into a module.
pub fn build(name: &str, config: &config::Modules) -> Option<Box<dyn Module>> {
    match name {
        "clock" => Some(Box::new(clock::Clock::new(&config.clock))),
        "workspaces" => Some(Box::new(workspaces::Workspaces::new(&config.workspaces))),
        _ => None,
    }
}
