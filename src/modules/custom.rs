//! Modules built from scripts.
//!
//! This is the escape hatch: anything that can be scripted becomes a module
//! without writing Rust. A module is up to three scripts — one producing what
//! it shows, one for hover text, one for what a click does — and any of them
//! may be left out.
//!
//! A command may print plain text, or a JSON object with `text`, `tooltip` and
//! `percentage`, which is the convention waybar's custom modules use.
//!
//! With `stream = true` the command is kept running and each line it prints is
//! an update, rather than being re-run on a timer. That is how a module reacts
//! the moment something happens — blocking on `pactl subscribe` and printing
//! when the volume moves — instead of polling and mostly finding nothing.

use std::process::Stdio;
use std::time::Duration;

use iced::futures::{SinkExt, Stream};
use iced::widget::{button, column, container, text};
use iced::{Element, Length, Subscription, Task};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::{
    Content, Direction, Entry, Event, Icon, Module, Popup, color_for, icon_for, labelled, spawn,
};
use crate::config;

pub struct Custom {
    name: String,
    /// None when there is no `exec`, or the config is not trusted.
    source: Option<Source>,
    label: String,
    format: String,
    icons: Vec<Icon>,
    icon_size: Option<f32>,
    colors: Vec<config::Rgba>,
    on_click: Option<String>,
    on_scroll_up: Option<String>,
    on_scroll_down: Option<String>,
    /// A script printing a popup to show when this module is clicked.
    popup: Option<String>,
    /// What that script last printed. None until it has run.
    entries: Option<Vec<Entry>>,
    /// Whether its popup is showing, so a second click closes rather than
    /// closing and immediately fetching again.
    showing: bool,
    ticker: Option<Ticker>,
    background: Option<config::Rgba>,
    foreground: Option<config::Rgba>,
    content: Content,
}

/// A window onto content too long to sit in the bar, moved along a step at a
/// time. Character-stepped rather than pixel-smooth: text cannot be measured
/// outside a renderer, and a monospace font makes the difference hard to see.
struct Ticker {
    width: usize,
    step: Duration,
    /// How far the window has travelled, in characters.
    offset: usize,
}

/// What produces the module's content, and how it is run.
#[derive(Clone, Hash, PartialEq, Eq)]
struct Source {
    command: String,
    interval: Duration,
    stream: bool,
}

impl Custom {
    pub fn new(config: &config::Custom, trusted: bool) -> Self {
        let source = trusted
            .then(|| config.exec.clone())
            .flatten()
            .map(|command| Source {
                command,
                // A zero interval would re-run as fast as the command completes.
                interval: Duration::from_secs(config.interval.max(1)),
                stream: config.stream,
            });

        Self {
            name: config.name.clone(),
            source,
            label: config.label.clone(),
            format: config.format.clone(),
            icons: config.icons.iter().map(|icon| Icon::parse(icon)).collect(),
            icon_size: config.icon_size,
            colors: config.colors.clone(),
            on_click: trusted.then(|| config.on_click.clone()).flatten(),
            on_scroll_up: trusted.then(|| config.on_scroll_up.clone()).flatten(),
            on_scroll_down: trusted.then(|| config.on_scroll_down.clone()).flatten(),
            popup: trusted.then(|| config.popup.clone()).flatten(),
            entries: None,
            showing: false,
            ticker: (config.scroll_width > 0).then(|| Ticker {
                width: config.scroll_width,
                step: Duration::from_secs_f32(1.0 / config.scroll_speed.max(0.1)),
                offset: 0,
            }),
            background: config.background,
            foreground: config.foreground,
            content: if trusted {
                Content {
                    text: config.label.clone(),
                    tooltip: config.tooltip.clone(),
                    failed: false,
                    percentage: None,
                    icon: None,
                }
            } else {
                Content::error(format!(
                    "`{}` will not run: the config is writable by other users",
                    config.name
                ))
            },
        }
    }

    /// The icon to draw, if there is one.
    ///
    /// A script naming one outranks the level: a weather condition or a
    /// keyboard layout is not a percentage of anything. Otherwise the
    /// `percentage` it reported picks from the configured list.
    fn icon(&self) -> Option<Icon> {
        if let Some(named) = &self.content.icon {
            return Some(Icon::parse(named));
        }

        let Some(percentage) = self.content.percentage else {
            // Nothing reported a level. A ramp needs one to choose from, but a
            // single icon does not -- that is a module with a fixed picture,
            // such as a launcher button, and it should simply show it.
            return match self.icons.as_slice() {
                [only] => Some(only.clone()),
                _ => None,
            };
        };

        icon_for(percentage / 100.0, &self.icons).cloned()
    }

    /// The `{value}` half of the format: whatever the command last printed, or
    /// the fixed label for a module that has no command.
    fn value(&self) -> &str {
        if self.content.text.is_empty() {
            &self.label
        } else {
            &self.content.text
        }
    }

    /// The whole module as one string.
    ///
    /// Only used when a ticker is scrolling, which windows a fixed number of
    /// characters and so cannot carry a picture. A ticker therefore renders
    /// its icon as text, and a path configured on one shows as the path.
    fn scrolled(&self, ticker: &Ticker) -> String {
        let icon = match self.icon() {
            Some(Icon::Glyph(glyph)) => glyph,
            _ => String::new(),
        };

        let label = self
            .format
            .replace("{icon}", &icon)
            .replace("{value}", self.value());

        ticker.window(&label)
    }
}

impl Module for Custom {
    fn name(&self) -> &str {
        &self.name
    }

    fn subscription(&self) -> Subscription<Event> {
        let content = match self.source.clone() {
            // Keyed on the command, so two custom modules never share a
            // stream. See the note in app::subscription.
            Some(source) => Subscription::run_with(source, run),
            None => Subscription::none(),
        };

        let Some(ticker) = &self.ticker else {
            return content;
        };

        // A second, faster subscription: content arrives when it arrives, and
        // the ticker moves on its own beat.
        Subscription::batch([content, Subscription::run_with(ticker.step, steps)])
    }

    fn update(&mut self, event: Event) -> Task<Event> {
        match event {
            Event::Content(content) => self.content = content,
            Event::Tick => {
                if let Some(ticker) = &mut self.ticker {
                    ticker.advance();
                }
            }
            Event::Entries(entries) => self.entries = Some(entries),
            Event::Scroll(direction) => {
                let command = match direction {
                    Direction::Up => self.on_scroll_up.clone(),
                    Direction::Down => self.on_scroll_down.clone(),
                };

                if let Some(command) = command {
                    return spawn(command);
                }
            }
            Event::TogglePopup => {
                let Some(script) = self.popup.clone() else {
                    return Task::none();
                };

                self.showing = !self.showing;

                if !self.showing {
                    // Closing. Drop the entries so the next click fetches
                    // again rather than showing what was true last time.
                    self.entries = None;
                    return Task::none();
                }

                // The surface cannot be sized until the script has answered,
                // so the bar opens it when these come back, not on the click.
                return Task::future(async move { Event::Entries(entries_from(&script).await) });
            }
            Event::Activate(entry) => {
                // Choosing an entry closes the popup, so forget it was open.
                self.showing = false;

                // With a popup open the click is a choice from it; otherwise
                // it is the module's own action.
                let command = match &self.entries {
                    Some(entries) => entries.get(entry).map(|entry| entry.exec.clone()),
                    None => self.on_click.clone(),
                };

                let Some(command) = command else {
                    return Task::none();
                };

                return spawn(command);
            }
            _ => {}
        }

        Task::none()
    }

    fn view(&self, style: config::Style) -> Element<'_, Event> {
        // A failure is worth seeing whatever colour the module was given.
        let foreground = if self.content.failed {
            style.urgent.color()
        } else {
            self.content
                .percentage
                .and_then(|percentage| color_for(percentage / 100.0, &self.colors))
                .or(self.foreground)
                .unwrap_or(style.foreground)
                .color()
        };
        // A scrolling module is one windowed string, so it stays text. Any
        // other renders its icon as a widget, which is what lets it be a
        // picture rather than a glyph.
        let label: Element<'_, Event> = match &self.ticker {
            Some(ticker) => text(self.scrolled(ticker)).color(foreground).into(),
            None => labelled(
                &self.format,
                self.icon(),
                self.value(),
                foreground,
                self.icon_size.unwrap_or(style.icon_size),
            ),
        };

        let press = if self.popup.is_some() {
            Some(Event::TogglePopup)
        } else {
            self.on_click.as_ref().map(|_| Event::Activate(0))
        };

        // Without an action there is nothing to click, so stay a plain label.
        let Some(press) = press else {
            let Some(background) = self.background else {
                return label;
            };

            return container(label)
                .padding([2, 6])
                .style(move |_theme| container::Style {
                    background: Some(background.color().into()),
                    border: iced::Border {
                        radius: 4.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into();
        };

        let background = self.background;

        button(label)
            .padding([2, 6])
            .on_press(press)
            .style(move |_theme, status| button::Style {
                background: match status {
                    button::Status::Hovered | button::Status::Pressed => {
                        Some(style.muted.color().into())
                    }
                    _ => background.map(|colour| colour.color().into()),
                },
                text_color: foreground,
                border: iced::Border {
                    radius: 4.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }

    fn tooltip(&self) -> Option<String> {
        self.content.tooltip.clone()
    }

    fn popup(&self) -> Option<Popup> {
        let entries = self.entries.as_ref()?;

        if entries.is_empty() {
            return None;
        }

        let widest = entries
            .iter()
            .map(|entry| entry.label.chars().count())
            .max()
            .unwrap_or(0) as f32;

        Some(Popup {
            // Text cannot be measured outside a renderer, so this over-estimates
            // rather than risk clipping a label.
            width: widest.mul_add(ENTRY_GLYPH, 2.0 * ENTRY_PADDING),
            height: (entries.len() as f32).mul_add(ENTRY_HEIGHT, 2.0 * ENTRY_PADDING),
        })
    }

    fn popup_view(&self, style: config::Style) -> Element<'_, Event> {
        let Some(entries) = &self.entries else {
            return iced::widget::space::horizontal().into();
        };

        let rows = entries.iter().enumerate().map(|(index, entry)| {
            button(text(entry.label.as_str()).wrapping(text::Wrapping::None))
                .width(Length::Fill)
                .padding([2, 6])
                .on_press(Event::Activate(index))
                .style(move |_theme, status| button::Style {
                    background: match status {
                        button::Status::Hovered | button::Status::Pressed => {
                            Some(style.accent.color().into())
                        }
                        _ => None,
                    },
                    text_color: match status {
                        button::Status::Hovered | button::Status::Pressed => {
                            style.background.color()
                        }
                        _ => style.foreground.color(),
                    },
                    border: iced::Border {
                        radius: 4.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
        });

        column(rows).spacing(2).into()
    }
}

/// Per-entry metrics, used to size the surface before anything is drawn.
const ENTRY_GLYPH: f32 = 9.5;
const ENTRY_HEIGHT: f32 = 24.0;
const ENTRY_PADDING: f32 = 10.0;

/// What a popup script prints.
#[derive(Deserialize)]
struct PopupOutput {
    items: Vec<PopupItem>,
}

#[derive(Deserialize)]
struct PopupItem {
    label: String,
    #[serde(default)]
    exec: String,
}

/// Run a popup script and read the entries it describes.
async fn entries_from(script: &str) -> Vec<Entry> {
    let run = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(script)
        .output();

    let output = match tokio::time::timeout(Duration::from_secs(5), run).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => return vec![failed(format!("could not run: {error}"))],
        Err(_) => return vec![failed(String::from("timed out"))],
    };

    let printed = String::from_utf8_lossy(&output.stdout);

    match serde_json::from_str::<PopupOutput>(printed.trim()) {
        Ok(popup) => popup
            .items
            .into_iter()
            .map(|item| Entry {
                label: item.label,
                exec: item.exec,
            })
            .collect(),
        Err(error) => vec![failed(format!("bad popup JSON: {error}"))],
    }
}

/// Shown in the popup itself, so a broken script says so where it is looked
/// for rather than only on stderr.
fn failed(detail: String) -> Entry {
    Entry {
        label: detail,
        exec: String::new(),
    }
}

impl Ticker {
    fn advance(&mut self) {
        self.offset = self.offset.wrapping_add(1);
    }

    /// The slice of `text` currently in view, wrapping round through a gap so
    /// the end runs into the beginning rather than snapping back.
    fn window(&self, text: &str) -> String {
        let characters: Vec<char> = text.chars().collect();

        if characters.len() <= self.width {
            return text.to_owned();
        }

        let loop_: Vec<char> = characters.into_iter().chain(GAP.chars()).collect();

        (0..self.width)
            .map(|column| loop_[(self.offset + column) % loop_.len()])
            .collect()
    }
}

/// Separates the end of the text from its beginning as it comes round again.
const GAP: &str = "   \u{2022}   ";

/// What a command may print instead of plain text.
#[derive(Deserialize)]
struct Output {
    text: String,
    #[serde(default)]
    tooltip: Option<String>,
    #[serde(default)]
    percentage: Option<f32>,
    /// A glyph or a path, chosen by the script itself. Overrides whatever
    /// `percentage` would have picked from the module's `icons`.
    #[serde(default)]
    icon: Option<String>,
}

fn steps(step: &Duration) -> impl Stream<Item = Event> + use<> {
    let step = *step;

    iced::stream::channel(1, async move |mut output| {
        let mut timer = tokio::time::interval(step);

        loop {
            timer.tick().await;

            if output.send(Event::Tick).await.is_err() {
                return;
            }
        }
    })
}

fn run(source: &Source) -> impl Stream<Item = Event> + use<> {
    let source = source.clone();

    iced::stream::channel(4, async move |mut output| {
        if source.stream {
            loop {
                if follow(&source.command, &mut output).await.is_err() {
                    return;
                }
                // The command exited. Wait before restarting, so a script that
                // fails immediately cannot spin.
                tokio::time::sleep(source.interval.max(Duration::from_secs(1))).await;
            }
        }

        let mut timer = tokio::time::interval(source.interval);

        loop {
            timer.tick().await;

            let content = execute(&source.command, source.interval).await;

            if output.send(Event::Content(content)).await.is_err() {
                return;
            }
        }
    })
}

/// Keep the command running, treating each line it prints as an update.
///
/// Returns `Err` only when the receiver has gone, meaning the bar is done with
/// this module; a command that simply exits is reported and retried.
async fn follow(
    command: &str,
    output: &mut iced::futures::channel::mpsc::Sender<Event>,
) -> Result<(), ()> {
    let child = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .spawn();

    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            let content = Content::error(format!("could not run: {error}"));
            return output.send(Event::Content(content)).await.map_err(|_| ());
        }
    };

    let Some(stdout) = child.stdout.take() else {
        return Ok(());
    };

    let mut lines = BufReader::new(stdout).lines();
    let mut said_anything = false;

    while let Ok(Some(line)) = lines.next_line().await {
        said_anything = true;

        if output.send(Event::Content(parse(&line))).await.is_err() {
            // Stop the command rather than leave it running unread.
            let _ = child.start_kill();
            return Err(());
        }
    }

    // A stream that keeps ending without printing is broken, not quiet, and
    // would otherwise leave the module blank forever while it retried.
    if !said_anything {
        let content = Content::error(String::from("the command ended without printing anything"));
        return output.send(Event::Content(content)).await.map_err(|_| ());
    }

    Ok(())
}

async fn execute(command: &str, interval: Duration) -> Content {
    // A command that never returns would otherwise wedge this module forever.
    let deadline = interval.min(Duration::from_secs(10));

    let run = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .output();

    let output = match tokio::time::timeout(deadline, run).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => return Content::error(format!("could not run: {error}")),
        Err(_) => return Content::error(format!("timed out after {deadline:?}")),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Content::error(format!("{}: {}", output.status, stderr.trim()));
    }

    parse(&String::from_utf8_lossy(&output.stdout))
}

fn parse(line: &str) -> Content {
    let line = line.trim();

    // Plain text is by far the common case, so only try JSON when it looks
    // like an object.
    if line.starts_with('{') {
        return match serde_json::from_str::<Output>(line) {
            Ok(parsed) => Content {
                text: parsed.text,
                tooltip: parsed.tooltip,
                failed: false,
                percentage: parsed.percentage,
                icon: parsed.icon,
            },
            // It meant to be JSON and is not, so say so rather than printing
            // the broken JSON into the bar as though it were a reading.
            Err(error) => Content::error(format!("{error}\n\n{line}")),
        };
    }

    Content {
        text: line.to_owned(),
        tooltip: None,
        failed: false,
        percentage: None,
        icon: None,
    }
}
