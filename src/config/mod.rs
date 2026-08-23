//! Configuration, read from `$XDG_CONFIG_HOME/ricebar/config.toml`.

mod color;
mod first_run;

pub use color::Rgba;

use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub bar: Bar,
    pub module: Modules,
    /// Whether the config is safe to take shell commands from. See
    /// [`trustworthy`]. Not a config key; decided when the file is read.
    #[serde(skip, default = "yes")]
    pub trusted: bool,
}

fn yes() -> bool {
    true
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
    /// The single row's modules. Ignored once `[[bar.row]]` is used.
    pub modules_left: Vec<String>,
    pub modules_center: Vec<String>,
    pub modules_right: Vec<String>,
    /// Extra rows, stacked. A bar with none is one row built from the
    /// `modules-*` lists above, which is the common case and stays simple.
    pub row: Vec<Row>,
    pub style: Style,
}

/// One line of the bar. Several stack into a taller bar, each with its own
/// left, centre and right.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Row {
    /// Defaults to the bar's `height`.
    pub height: Option<u32>,
    pub modules_left: Vec<String>,
    pub modules_center: Vec<String>,
    pub modules_right: Vec<String>,
}

impl Bar {
    /// The rows to draw, however they were written.
    pub fn rows(&self) -> Vec<Row> {
        if self.row.is_empty() {
            return vec![Row {
                height: Some(self.height),
                modules_left: self.modules_left.clone(),
                modules_center: self.modules_center.clone(),
                modules_right: self.modules_right.clone(),
            }];
        }

        self.row
            .iter()
            .map(|row| Row {
                height: Some(row.height.unwrap_or(self.height)),
                ..row.clone()
            })
            .collect()
    }

    /// How tall the whole bar is, which is what the surface and the space it
    /// reserves are sized from.
    pub fn total_height(&self) -> u32 {
        self.rows()
            .iter()
            .map(|row| row.height.unwrap_or(self.height))
            .sum()
    }
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
            row: Vec::new(),
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
    pub cpu: Sensor,
    pub memory: Sensor,
    pub temperature: Sensor,
    pub battery: Sensor,
    pub backlight: Sensor,
    /// User-defined command modules, each enabled by its own `name`.
    pub custom: Vec<Custom>,
    /// User-defined menu modules, each enabled by its own `name`.
    pub menu: Vec<Menu>,
}

/// A readout taken from the kernel: cpu, memory, temperature, battery or
/// backlight. They share a shape, so they share their settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Sensor {
    /// `{icon}` and `{value}` are replaced. Drop `{value}` for an icon alone.
    pub format: String,
    /// Lowest level first. Empty uses the built-in set for that sensor.
    pub icons: Vec<String>,
    /// Text colours, lowest level first, chosen the same way as `icons`:
    /// green, amber, red reads as fine, busy, struggling. Empty uses
    /// `foreground`.
    pub colors: Vec<Rgba>,
    /// Seconds between reads.
    pub interval: u64,
    /// Passed to `sh -c` when the wheel turns over the module. Backlight and
    /// volume are the obvious ones to wire up.
    pub on_scroll_up: Option<String>,
    pub on_scroll_down: Option<String>,
    /// Fill behind this module. Unset draws on the bar's own background.
    pub background: Option<Rgba>,
    /// Text colour for this module, overriding the bar's.
    pub foreground: Option<Rgba>,
}

impl Default for Sensor {
    fn default() -> Self {
        Self {
            format: String::from("{icon} {value}"),
            icons: Vec::new(),
            colors: Vec::new(),
            interval: 5,
            on_scroll_up: None,
            on_scroll_down: None,
            background: None,
            foreground: None,
        }
    }
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

/// A module built from scripts: one to produce what it shows, one for hover
/// text, one for what a click does. Any of the three may be left out.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Custom {
    /// The name used in the `modules-*` lists.
    pub name: String,
    /// Passed to `sh -c` to produce what the module shows. Leave it out for a
    /// module that only ever shows `label`, such as a launcher button.
    pub exec: Option<String>,
    /// Keep `exec` running and read a line each time it prints, rather than
    /// re-running it on a timer. Far cheaper, and updates the moment something
    /// happens instead of on the next tick.
    pub stream: bool,
    /// Seconds between runs. Ignored when streaming.
    pub interval: u64,
    /// Shown before any output arrives, and instead of it when there is no
    /// `exec` at all.
    pub label: String,
    /// `{icon}` and `{value}` are replaced.
    pub format: String,
    /// Lowest level first, chosen by the `percentage` a command reports.
    pub icons: Vec<String>,
    /// Text colours, chosen the same way as `icons`.
    pub colors: Vec<Rgba>,
    /// Shown on hover, unless the command prints its own.
    pub tooltip: Option<String>,
    /// Passed to `sh -c` when the module is clicked. Setting it is what makes
    /// the module a button.
    pub on_click: Option<String>,
    /// Passed to `sh -c` when the module is clicked, and expected to print a
    /// JSON popup: `{"items": [{"label": "...", "exec": "..."}]}`. Takes
    /// precedence over `on-click`, which then never runs.
    pub popup: Option<String>,
    /// Passed to `sh -c` when the wheel turns over the module.
    pub on_scroll_up: Option<String>,
    pub on_scroll_down: Option<String>,
    /// Show at most this many characters, scrolling the rest past like a
    /// ticker. Zero shows everything and never scrolls.
    pub scroll_width: usize,
    /// Characters per second the ticker moves.
    pub scroll_speed: f32,
    /// Fill behind this module. Unset draws on the bar's own background.
    pub background: Option<Rgba>,
    /// Text colour for this module, overriding the bar's.
    pub foreground: Option<Rgba>,
}

impl Default for Custom {
    fn default() -> Self {
        Self {
            name: String::new(),
            exec: None,
            stream: false,
            interval: 1,
            label: String::new(),
            format: String::from("{icon}{value}"),
            icons: Vec::new(),
            colors: Vec::new(),
            tooltip: None,
            on_click: None,
            popup: None,
            on_scroll_up: None,
            on_scroll_down: None,
            scroll_width: 0,
            scroll_speed: 4.0,
            background: None,
            foreground: None,
        }
    }
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
    /// Show ISO week numbers beside the calendar.
    pub week_numbers: bool,
    /// Which side those numbers sit on.
    pub weeks_pos: Side,
    /// Start weeks on Monday rather than Sunday.
    pub start_monday: bool,
    /// Run when a day is clicked. `{}` is replaced with the date in ISO form,
    /// and if it is absent the date is appended instead.
    #[serde(default)]
    pub on_click_day: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    #[default]
    Left,
    Right,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            format: String::from("%Y-%m-%d %H:%M:%S"),
            tooltip_format: String::from("%A, %-d %B %Y"),
            interval: 1,
            calendar: true,
            week_numbers: true,
            weeks_pos: Side::Right,
            start_monday: true,
            on_click_day: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Workspaces {
    /// Show workspaces that hold no windows.
    pub show_empty: bool,
    /// Which compositor to talk to. `auto` follows the environment, which is
    /// ambiguous inside a nested session — see `compositor::detect`.
    pub compositor: Backend,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    #[default]
    Auto,
    Hyprland,
    Sway,
    Niri,
    None,
}

impl Default for Workspaces {
    fn default() -> Self {
        Self {
            show_empty: true,
            compositor: Backend::default(),
        }
    }
}

/// Where the config lives when none was named on the command line.
fn default_path() -> Option<PathBuf> {
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
pub fn load(named: Option<PathBuf>) -> Config {
    // A path given on the command line wins over the usual location.
    let explicit = named.is_some();

    let Some(path) = named.or_else(default_path) else {
        eprintln!("ricebar: no HOME or XDG_CONFIG_HOME, using defaults");
        return Config::default();
    };

    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !explicit => {
            // First run. Coming up on built-in defaults would leave no trace of
            // what can be configured, so write a starter config and read that.
            match first_run::create(&path).and_then(|()| std::fs::read_to_string(&path)) {
                Ok(text) => text,
                Err(error) => {
                    eprintln!("ricebar: cannot write {}: {error}", path.display());
                    eprintln!("ricebar: using defaults");
                    return Config::default();
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Asked for by name and not there is a mistake, not a first run.
            eprintln!("ricebar: {} does not exist, using defaults", path.display());
            return Config::default();
        }
        Err(error) => {
            eprintln!("ricebar: cannot read {}: {error}", path.display());
            return Config::default();
        }
    };

    match toml::from_str::<Config>(&text) {
        Ok(mut config) => {
            eprintln!("ricebar: loaded {}", path.display());
            config.trusted = trustworthy(&path);
            config
        }
        Err(error) => {
            eprintln!("ricebar: {} is invalid, using defaults:", path.display());
            eprintln!("{error}");
            Config::default()
        }
    }
}

/// Whether shell commands in this config may be run.
///
/// The config file *is* the trust boundary: it names commands to execute, so
/// anyone who can write it can already run code as this user. Validating the
/// commands themselves would buy nothing — an attacker who can edit the file
/// can just as easily name an allowed path. What is worth checking is whether
/// anyone *else* can write it, which is the same check ssh and sudo make of
/// their own files.
fn trustworthy(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    let writable_by_others = |path: &std::path::Path| match std::fs::metadata(path) {
        // 0o022 is the group-write and other-write bits.
        Ok(metadata) => metadata.permissions().mode() & 0o022 != 0,
        Err(_) => false,
    };

    if writable_by_others(path) {
        eprintln!(
            "ricebar: {} is writable by other users; refusing to run commands from it",
            path.display()
        );
        eprintln!("ricebar: fix with `chmod go-w {}`", path.display());
        return false;
    }

    // A writable directory means the file can simply be replaced.
    if let Some(parent) = path.parent()
        && writable_by_others(parent)
    {
        eprintln!(
            "ricebar: {} is writable by other users; refusing to run commands from configs inside it",
            parent.display()
        );
        eprintln!("ricebar: fix with `chmod go-w {}`", parent.display());
        return false;
    }

    true
}

/// Wrap a value so a shell treats it as one literal argument.
///
/// Today the only substituted value is a date this program formats itself, but
/// quoting keeps that from becoming an injection the day something reads a
/// value from elsewhere.
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}
