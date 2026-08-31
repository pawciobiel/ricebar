//! Compositor backends.
//!
//! The bar core talks only to the [`Compositor`] trait, so adding sway or niri
//! means adding a module here and a branch in [`detect`].

pub mod hyprland;
pub mod niri;
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

/// The keyboard layouts configured, and which of them is in use.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layouts {
    /// Every layout, in the order the compositor holds them — which is the
    /// order they were configured in, and what an index means.
    ///
    /// sway and niri name them as xkb describes them, "Polish"; Hyprland
    /// reports the codes it was configured with, `pl`. The module accepts
    /// either, so neither has to be translated here.
    pub names: Vec<String>,
    /// Which of `names` is in use.
    pub current: usize,
}

impl Layouts {
    /// The layout in use, if the compositor has named any.
    pub fn active(&self) -> Option<&str> {
        self.names.get(self.current).map(String::as_str)
    }
}

pub trait Compositor {
    fn name(&self) -> &'static str;

    /// A stream of snapshots, pushed as the compositor changes.
    fn workspaces(&self) -> Subscription<Workspaces>;

    /// Ask the compositor to switch to a workspace.
    fn focus(&self, id: i32) -> Task<()>;

    /// A stream of the keyboard layouts configured, pushed as the one in use
    /// changes. The whole list every time, so the popup that lists them cannot
    /// drift out of sync with the compositor.
    fn layouts(&self) -> Subscription<Layouts>;

    /// Ask the compositor for the layout at this index of [`Layouts::names`].
    fn set_layout(&self, index: usize) -> Task<()>;

    /// The monitors that exist, pushed again whenever one is plugged in or
    /// taken away.
    ///
    /// The bar creates one layer surface per monitor itself, rather than
    /// letting the runtime do it, because that is the only way to know which
    /// surface is on which monitor — so this is where hotplug comes from.
    fn outputs(&self) -> Subscription<Vec<String>>;
}

/// The monitors to build bars on, whoever is able to say.
///
/// Without a backend there is no list to be had: layer-shell clients are told
/// about outputs only through the surfaces they create on them. Yielding an
/// empty list says exactly that, and the caller then asks the compositor to
/// place each bar wherever it likes.
pub fn outputs(preference: config::Backend) -> Subscription<Vec<String>> {
    match detect(preference) {
        Some(compositor) => compositor.outputs(),
        None => Subscription::run(|| iced::futures::stream::once(async { Vec::new() })),
    }
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
    let niri = || niri::available().then(|| Box::new(niri::Niri) as Box<dyn Compositor>);
    let sway = || sway::available().then(|| Box::new(sway::Sway) as Box<dyn Compositor>);
    let hyprland =
        || hyprland::available().then(|| Box::new(hyprland::Hyprland) as Box<dyn Compositor>);

    match preference {
        config::Backend::Auto => niri().or_else(sway).or_else(hyprland),
        config::Backend::Niri => niri(),
        config::Backend::Sway => sway(),
        config::Backend::Hyprland => hyprland(),
        config::Backend::None => None,
    }
}
