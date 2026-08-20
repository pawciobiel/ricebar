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
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Clock {
    /// strftime format.
    pub format: String,
    /// Seconds between redraws.
    pub interval: u64,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            format: String::from("%Y-%m-%d %H:%M:%S"),
            interval: 1,
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
