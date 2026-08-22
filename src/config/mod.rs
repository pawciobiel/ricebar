//! Configuration, read from `$XDG_CONFIG_HOME/ricebar/config.toml`.

mod color;

pub use color::Rgba;

use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub bar: Bar,
    pub module: Modules,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Bar {
    pub position: Position,
    pub height: u32,
    /// Top, right, bottom, left.
    pub margin: [i32; 4],
    /// Reserve the bar's space so windows cannot render underneath it.
    pub exclusive: bool,
    /// Gap between modules.
    pub spacing: f32,
    /// Gap between the modules and the edge of the bar.
    pub padding: f32,
    /// Where the modules sit within the bar's height.
    pub vertical_align: VerticalAlign,
    /// Font family, as fontconfig knows it. Naming one that ships icon glyphs,
    /// such as a Nerd Font, is what makes module icons render reliably rather
    /// than by whatever the system happens to fall back to.
    pub font: Option<String>,
    pub font_size: f32,
    pub modules_left: Vec<String>,
    pub modules_center: Vec<String>,
    pub modules_right: Vec<String>,
    pub style: Style,
}

impl Default for Bar {
    fn default() -> Self {
        Self {
            position: Position::default(),
            height: 32,
            margin: [0; 4],
            exclusive: true,
            spacing: 8.0,
            padding: 8.0,
            vertical_align: VerticalAlign::default(),
            font: None,
            font_size: 16.0,
            modules_left: vec![String::from("workspaces")],
            modules_center: vec![String::from("clock")],
            modules_right: Vec::new(),
            style: Style::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    #[default]
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerticalAlign {
    Top,
    #[default]
    Center,
    Bottom,
}

impl From<VerticalAlign> for iced::alignment::Vertical {
    fn from(value: VerticalAlign) -> Self {
        match value {
            VerticalAlign::Top => Self::Top,
            VerticalAlign::Center => Self::Center,
            VerticalAlign::Bottom => Self::Bottom,
        }
    }
}

/// The bar's palette. Modules render against it rather than carrying their own
/// colours, so one edit restyles everything.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Style {
    pub background: Rgba,
    pub foreground: Rgba,
    /// Fill behind the focused item.
    pub accent: Rgba,
    /// Fill behind an item that is active but not focused.
    pub muted: Rgba,
    /// Text of an item that is present but empty.
    pub dim: Rgba,
    /// Something that wants attention, such as today in the calendar.
    pub urgent: Rgba,
    pub border_color: Rgba,
    pub border_width: f32,
    pub border_radius: f32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            background: Rgba::new(0x1e, 0x1e, 0x2e),
            foreground: Rgba::new(0xcd, 0xd6, 0xf4),
            accent: Rgba::new(0x89, 0xb4, 0xfa),
            muted: Rgba::new(0x45, 0x47, 0x5a),
            dim: Rgba::new(0x6c, 0x70, 0x86),
            urgent: Rgba::new(0xf3, 0x8b, 0xa8),
            border_color: Rgba::new(0x89, 0xb4, 0xfa),
            border_width: 0.0,
            border_radius: 0.0,
        }
    }
}

/// Per-module settings. A module is *enabled* by naming it in one of the
/// `modules-*` lists; this only configures it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Modules {
    pub clock: Clock,
    pub workspaces: Workspaces,
    /// User-defined command modules, each enabled by its own `name`.
    pub custom: Vec<Custom>,
    /// User-defined menu modules, each enabled by its own `name`.
    pub menu: Vec<Menu>,
}

/// A module that shows a label and opens a menu of commands when clicked.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Menu {
    /// The name used in the `modules-*` lists.
    pub name: String,
    /// What the bar shows, usually a single icon glyph.
    pub label: String,
    pub items: Vec<MenuItem>,
    /// Shown on hover.
    #[serde(default)]
    pub tooltip: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct MenuItem {
    pub label: String,
    /// Passed to `sh -c` when the entry is chosen.
    pub exec: String,
}

/// A module that runs a shell command and shows its output.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Custom {
    /// The name used in the `modules-*` lists.
    pub name: String,
    /// Passed to `sh -c`.
    pub exec: String,
    /// Seconds between runs.
    #[serde(default = "one")]
    pub interval: u64,
    /// Shown on hover, unless the command prints its own.
    #[serde(default)]
    pub tooltip: Option<String>,
}

fn one() -> u64 {
    1
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Clock {
    /// strftime format.
    pub format: String,
    /// strftime format shown on hover. Independent of the calendar: a clock may
    /// have either, both or neither.
    pub tooltip_format: String,
    /// Seconds between redraws.
    pub interval: u64,
    /// Open a calendar when the clock is clicked.
    pub calendar: bool,
    /// Show ISO week numbers down the left of the calendar.
    pub week_numbers: bool,
    /// Start weeks on Monday rather than Sunday.
    pub start_monday: bool,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            format: String::from("%Y-%m-%d %H:%M:%S"),
            tooltip_format: String::from("%A, %-d %B %Y"),
            interval: 1,
            calendar: true,
            week_numbers: true,
            start_monday: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Workspaces {
    /// Show workspaces that hold no windows.
    pub show_empty: bool,
}

impl Default for Workspaces {
    fn default() -> Self {
        Self { show_empty: true }
    }
}

fn path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(dir).join("ricebar/config.toml"));
    }

    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/ricebar/config.toml"))
}

/// Read the config, falling back to defaults.
///
/// A broken config is reported and then ignored rather than fatal: the bar is
/// how the desktop is driven, and refusing to start can leave the user with no
/// visible way to fix the mistake.
pub fn load() -> Config {
    let Some(path) = path() else {
        eprintln!("ricebar: no HOME or XDG_CONFIG_HOME, using defaults");
        return Config::default();
    };

    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("ricebar: no config at {}, using defaults", path.display());
            return Config::default();
        }
        Err(error) => {
            eprintln!("ricebar: cannot read {}: {error}", path.display());
            return Config::default();
        }
    };

    match toml::from_str(&text) {
        Ok(config) => {
            eprintln!("ricebar: loaded {}", path.display());
            config
        }
        Err(error) => {
            eprintln!("ricebar: {} is invalid, using defaults:", path.display());
            eprintln!("{error}");
            Config::default()
        }
    }
}
