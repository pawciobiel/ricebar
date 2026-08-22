//! Readouts taken from the kernel: CPU, memory, temperature, battery, backlight.
//!
//! They are one module rather than five, because they differ only in where the
//! number comes from: read a value, pick an icon for how high it is, show both.
//!
//! Everything is read from `/proc` and `/sys`, which are a few microseconds
//! each, so the reads happen in `update` rather than off on a task.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::futures::{SinkExt, Stream};
use iced::widget::{container, text};
use iced::{Element, Length, Subscription, Task};

use super::{BROKEN, Direction, Event, Module, icon_for, spawn};
use crate::config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Cpu,
    Memory,
    Temperature,
    Battery,
    Backlight,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Memory => "memory",
            Self::Temperature => "temperature",
            Self::Battery => "battery",
            Self::Backlight => "backlight",
        }
    }

    /// Nerd Font glyphs, lowest level first. Written as escapes because these
    /// live in Unicode private-use planes and do not survive copy-paste.
    fn icons(self) -> &'static [&'static str] {
        match self {
            Self::Cpu => &["\u{f2db}"],
            Self::Memory => &["\u{f0c9}"],
            Self::Temperature => &["\u{f76b}", "\u{f2c9}", "\u{f769}"],
            Self::Battery => &["\u{f244}", "\u{f243}", "\u{f242}", "\u{f241}", "\u{f240}"],
            Self::Backlight => &[
                "\u{e38d}", "\u{e39b}", "\u{e3c8}", "\u{e3ca}", "\u{e3cd}", "\u{e3ce}", "\u{e3cf}",
                "\u{e3d1}", "\u{e3d3}",
            ],
        }
    }
}

/// What a sensor found: how full it is, what to print, and what to say on hover.
#[derive(Default, Clone)]
struct Reading {
    /// 0.0 to 1.0, used to choose an icon.
    level: f32,
    value: String,
    tooltip: String,
    /// Overrides the level-chosen icon, for a battery that is charging.
    icon: Option<&'static str>,
    /// The sensor could not be read.
    failed: bool,
}

pub struct Sensor {
    kind: Kind,
    format: String,
    icons: Vec<String>,
    interval: Duration,
    on_scroll_up: Option<String>,
    on_scroll_down: Option<String>,
    background: Option<config::Rgba>,
    foreground: Option<config::Rgba>,
    reading: Reading,
    /// Previous `/proc/stat` totals, the whole machine first and then each
    /// core. CPU load is the change between reads, so there is nothing to
    /// report until the second one.
    previous: Vec<(u64, u64)>,
}

impl Sensor {
    pub fn new(kind: Kind, config: &config::Sensor) -> Self {
        let icons = if config.icons.is_empty() {
            kind.icons().iter().map(|icon| (*icon).to_owned()).collect()
        } else {
            config.icons.clone()
        };

        let mut sensor = Self {
            kind,
            format: config.format.clone(),
            icons,
            // A zero interval would read as fast as the loop can turn.
            interval: Duration::from_secs(config.interval.max(1)),
            on_scroll_up: config.on_scroll_up.clone(),
            on_scroll_down: config.on_scroll_down.clone(),
            background: config.background,
            foreground: config.foreground,
            reading: Reading::default(),
            previous: Vec::new(),
        };

        sensor.refresh();
        sensor
    }

    fn refresh(&mut self) {
        let reading = match self.kind {
            Kind::Cpu => cpu(&mut self.previous),
            Kind::Memory => memory(),
            Kind::Temperature => temperature(),
            Kind::Battery => battery(),
            Kind::Backlight => backlight(),
        };

        self.reading = reading.unwrap_or_else(|| Reading {
            level: 0.0,
            value: String::new(),
            tooltip: format!("{} could not be read", self.kind.name()),
            icon: Some(BROKEN),
            failed: true,
        });
    }

    fn icon(&self) -> &str {
        // A charging battery names its own glyph; the rest go by level.
        self.reading
            .icon
            .unwrap_or_else(|| icon_for(self.reading.level, &self.icons))
    }

    fn label(&self) -> String {
        self.format
            .replace("{icon}", self.icon())
            .replace("{value}", &self.reading.value)
    }
}

impl Module for Sensor {
    fn name(&self) -> &str {
        self.kind.name()
    }

    fn subscription(&self) -> Subscription<Event> {
        Subscription::run_with(self.interval, ticks)
    }

    fn update(&mut self, event: Event) -> Task<Event> {
        match event {
            Event::Tick => self.refresh(),
            Event::Scroll(direction) => {
                let command = match direction {
                    Direction::Up => self.on_scroll_up.clone(),
                    Direction::Down => self.on_scroll_down.clone(),
                };

                if let Some(command) = command {
                    // Read again straight after, so the bar catches up with
                    // what the command just changed.
                    self.refresh();
                    return spawn(command);
                }
            }
            _ => {}
        }

        Task::none()
    }

    fn view(&self, style: config::Style) -> Element<'_, Event> {
        // A failure is worth seeing whatever colour the module was given.
        let foreground = if self.reading.failed {
            style.urgent
        } else {
            self.foreground.unwrap_or(style.foreground)
        };
        let label = text(self.label()).color(foreground.color());

        let Some(background) = self.background else {
            return label.into();
        };

        container(label)
            .padding([2, 6])
            .style(move |_theme| container::Style {
                background: Some(background.color().into()),
                border: iced::Border {
                    radius: 4.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .height(Length::Shrink)
            .into()
    }

    fn tooltip(&self) -> Option<String> {
        Some(self.reading.tooltip.clone())
    }
}

/// Share of time not spent idle, between this read and the last.
///
/// The tooltip lists every core, which is the question you actually have when
/// one number says the machine is busy.
fn cpu(previous: &mut Vec<(u64, u64)>) -> Option<Reading> {
    let stat = fs::read_to_string("/proc/stat").ok()?;

    // The `cpu` line is the whole machine; `cpu0`, `cpu1` and so on are cores.
    let samples: Vec<(u64, u64)> = stat
        .lines()
        .filter(|line| line.starts_with("cpu"))
        .filter_map(sample)
        .collect();

    if samples.is_empty() {
        return None;
    }

    let before = std::mem::replace(previous, samples.clone());

    let busy = |index: usize| -> f32 {
        let (total, idle) = samples[index];
        let Some(&(was_total, was_idle)) = before.get(index) else {
            // The first read has nothing to compare against.
            return 0.0;
        };

        let spent = total.saturating_sub(was_total);
        let rested = idle.saturating_sub(was_idle);

        if spent == 0 {
            0.0
        } else {
            (1.0 - rested as f32 / spent as f32).clamp(0.0, 1.0)
        }
    };

    let level = busy(0);
    let cores: Vec<f32> = (1..samples.len()).map(busy).collect();

    Some(Reading {
        level,
        value: format!("{:.0}%", level * 100.0),
        tooltip: core_grid(&cores, level),
        icon: None,
        failed: false,
    })
}

/// How the core grid is shaped: cores are laid out in this many columns, but
/// never more than this many rows, so a 128-core machine widens rather than
/// growing a tooltip taller than the screen.
const GRID_COLUMNS: usize = 4;
const GRID_MAX_ROWS: usize = 8;

/// One core per cell, filling each column top to bottom.
fn core_grid(cores: &[f32], overall: f32) -> String {
    if cores.is_empty() {
        return format!("CPU {:.0}%", overall * 100.0);
    }

    let rows = cores.len().div_ceil(GRID_COLUMNS).clamp(1, GRID_MAX_ROWS);
    let columns = cores.len().div_ceil(rows);

    let mut grid = format!("CPU {:.0}%\n", overall * 100.0);

    for row in 0..rows {
        for column in 0..columns {
            let core = column * rows + row;

            if let Some(load) = cores.get(core) {
                grid.push_str(&format!("  {core:>2} {:>3.0}%", load * 100.0));
            }
        }
        grid.push('\n');
    }

    grid.trim_end().to_owned()
}

/// Where each figure sits on a `/proc/stat` cpu line once the label is
/// dropped: user, nice, system, idle, iowait, irq, softirq, steal, ...
const IDLE: usize = 3;
const IOWAIT: usize = 4;

/// Total and idle jiffies from one `/proc/stat` line.
fn sample(line: &str) -> Option<(u64, u64)> {
    let fields: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|field| field.parse().ok())
        .collect();

    // Waiting on IO is idle time too: the core had nothing else to run.
    Some((fields.iter().sum(), fields.get(IDLE)? + fields.get(IOWAIT)?))
}

fn memory() -> Option<Reading> {
    let meminfo = fs::read_to_string("/proc/meminfo").ok()?;

    let field = |name: &str| -> Option<u64> {
        meminfo
            .lines()
            .find(|line| line.starts_with(name))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    };

    let total = field("MemTotal:")?;
    let available = field("MemAvailable:")?;
    let used = total.saturating_sub(available);

    let level = used as f32 / total as f32;
    let gib = |kib: u64| kib as f32 / 1024.0 / 1024.0;

    Some(Reading {
        level,
        value: format!("{:.0}%", level * 100.0),
        tooltip: format!("{:.1} GiB of {:.1} GiB used", gib(used), gib(total)),
        icon: None,
        failed: false,
    })
}

fn temperature() -> Option<Reading> {
    let millidegrees: f32 = read_number(&thermal_source()?)?;
    let degrees = millidegrees / 1000.0;

    Some(Reading {
        // Roughly cool to hot for a laptop, which is what picks the icon.
        level: (degrees - 40.0) / 45.0,
        value: format!("{degrees:.0}\u{b0}C"),
        tooltip: format!("{degrees:.1}\u{b0}C"),
        icon: None,
        failed: false,
    })
}

/// Prefer a named hwmon sensor over the ACPI zone, which is often the case.
fn thermal_source() -> Option<PathBuf> {
    let hwmon = fs::read_dir("/sys/class/hwmon")
        .ok()?
        .flatten()
        .find(|entry| {
            let name = fs::read_to_string(entry.path().join("name")).unwrap_or_default();
            // The CPU package sensor on AMD and Intel respectively.
            matches!(name.trim(), "k10temp" | "coretemp")
        });

    if let Some(hwmon) = hwmon {
        let input = hwmon.path().join("temp1_input");
        if input.exists() {
            return Some(input);
        }
    }

    let zone = PathBuf::from("/sys/class/thermal/thermal_zone0/temp");
    zone.exists().then_some(zone)
}

fn battery() -> Option<Reading> {
    let battery = first_matching("/sys/class/power_supply", |name| name.starts_with("BAT"))?;

    let capacity: f32 = read_number(&battery.join("capacity"))?;
    let status = fs::read_to_string(battery.join("status")).unwrap_or_default();
    let status = status.trim().to_owned();

    // Whether the mains are connected is a better question than the battery's
    // own status, which reads "Not charging" both when unplugged and when
    // plugged in at a charge the firmware has decided is enough.
    let icon = match mains() {
        Some(true) => Some(PLUG),
        _ => None,
    };

    Some(Reading {
        level: capacity / 100.0,
        value: format!("{capacity:.0}%"),
        tooltip: if status.is_empty() {
            format!("Battery {capacity:.0}%")
        } else {
            format!("Battery {capacity:.0}% \u{2014} {status}")
        },
        icon,
        failed: false,
    })
}

/// The plug glyph, shown instead of a battery level when on mains.
const PLUG: &str = "\u{f1e6}";

/// Whether a mains supply is connected, or None if the machine has none.
fn mains() -> Option<bool> {
    let supplies = fs::read_dir("/sys/class/power_supply").ok()?;

    for supply in supplies.flatten() {
        let kind = fs::read_to_string(supply.path().join("type")).unwrap_or_default();

        if kind.trim() == "Mains" {
            let online = read_number(&supply.path().join("online"))?;
            return Some(online > 0.0);
        }
    }

    None
}

fn backlight() -> Option<Reading> {
    let device = first_matching("/sys/class/backlight", |_| true)?;

    let brightness: f32 = read_number(&device.join("brightness"))?;
    let max: f32 = read_number(&device.join("max_brightness"))?;

    if max <= 0.0 {
        return None;
    }

    let level = brightness / max;

    Some(Reading {
        level,
        value: format!("{:.0}%", level * 100.0),
        tooltip: format!("Backlight {:.0}%", level * 100.0),
        icon: None,
        failed: false,
    })
}

fn first_matching(directory: &str, wanted: impl Fn(&str) -> bool) -> Option<PathBuf> {
    let mut matches: Vec<PathBuf> = fs::read_dir(directory)
        .ok()?
        .flatten()
        .filter(|entry| wanted(&entry.file_name().to_string_lossy()))
        .map(|entry| entry.path())
        .collect();

    // Directory order is not stable, so pick the same one every time.
    matches.sort();
    matches.into_iter().next()
}

fn read_number(path: &Path) -> Option<f32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// `Subscription::run_with` takes a plain fn pointer and hashes the data it is
/// keyed on. See the note in `app::subscription` about the rest.
fn ticks(interval: &Duration) -> impl Stream<Item = Event> + use<> {
    let interval = *interval;

    iced::stream::channel(1, async move |mut output| {
        let mut timer = tokio::time::interval(interval);

        loop {
            timer.tick().await;

            if output.send(Event::Tick).await.is_err() {
                return;
            }
        }
    })
}
