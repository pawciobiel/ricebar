//! Compositor backends.
//!
//! The bar core talks only to the [`Compositor`] trait, so adding sway or niri
//! means adding a module here and a branch in [`detect`].

pub mod hyprland;

use iced::{Subscription, Task};

/// One workspace, as the bar needs to draw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub id: i32,
    pub name: String,
    pub monitor: String,
    pub windows: u16,
    /// Active on its own monitor. With several monitors, several are visible.
    pub visible: bool,
    /// Active on the focused monitor. Exactly one workspace is focused.
    pub focused: bool,
}

/// A complete snapshot of workspace state.
///
/// Backends always publish the whole set rather than deltas, so the bar cannot
/// drift out of sync with the compositor.
pub type Workspaces = Vec<Workspace>;

pub trait Compositor {
    fn name(&self) -> &'static str;

    /// A stream of snapshots, pushed as the compositor changes.
    fn workspaces(&self) -> Subscription<Workspaces>;

    /// Ask the compositor to switch to a workspace.
    fn focus(&self, id: i32) -> Task<()>;
}

/// Pick a backend from the environment.
pub fn detect() -> Option<Box<dyn Compositor>> {
    if hyprland::available() {
        return Some(Box::new(hyprland::Hyprland));
    }
    None
}
