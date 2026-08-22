//! Compositor backends.
//!
//! The bar core talks only to the [`Compositor`] trait, so adding sway or niri
//! means adding a module here and a branch in [`detect`].

pub mod hyprland;
pub mod sway;

use iced::{Subscription, Task};

use crate::config;

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

/// Pick a backend, either the one asked for or whichever the environment
/// points at.
///
/// Detection reads environment variables, and those are inherited: a sway
/// session started inside Hyprland still has `HYPRLAND_INSTANCE_SIGNATURE`
/// set, so both look available and the bar would talk to the *host*
/// compositor. sway is tried first because that nesting direction is the
/// common one, and `compositor` in config settles it either way.
pub fn detect(preference: config::Backend) -> Option<Box<dyn Compositor>> {
    let sway = || sway::available().then(|| Box::new(sway::Sway) as Box<dyn Compositor>);
    let hyprland =
        || hyprland::available().then(|| Box::new(hyprland::Hyprland) as Box<dyn Compositor>);

    match preference {
        config::Backend::Auto => sway().or_else(hyprland),
        config::Backend::Sway => sway(),
        config::Backend::Hyprland => hyprland(),
        config::Backend::None => None,
    }
}
