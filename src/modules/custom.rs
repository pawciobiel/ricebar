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
use iced::widget::{button, container, text};
use iced::{Element, Subscription, Task};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::{Content, Event, Module, icon_for};
use crate::config;

pub struct Custom {
    name: String,
    /// None when there is no `exec`, or the config is not trusted.
    source: Option<Source>,
    label: String,
    format: String,
    icons: Vec<String>,
    on_click: Option<String>,
    background: Option<config::Rgba>,
    foreground: Option<config::Rgba>,
    content: Content,
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
            icons: config.icons.clone(),
            on_click: trusted.then(|| config.on_click.clone()).flatten(),
            background: config.background,
            foreground: config.foreground,
            content: if trusted {
                Content {
                    text: config.label.clone(),
                    tooltip: config.tooltip.clone(),
                    percentage: None,
                }
            } else {
                Content::error(format!(
                    "`{}` will not run: the config is writable by other users",
                    config.name
                ))
            },
        }
    }

    fn label(&self) -> String {
        let icon = self
            .content
            .percentage
            .map(|percentage| icon_for(percentage / 100.0, &self.icons))
            .unwrap_or_default();

        let value = if self.content.text.is_empty() {
            self.label.as_str()
        } else {
            self.content.text.as_str()
        };

        self.format
            .replace("{icon}", icon)
            .replace("{value}", value)
    }
}

impl Module for Custom {
    fn name(&self) -> &str {
        &self.name
    }

    fn subscription(&self) -> Subscription<Event> {
        let Some(source) = self.source.clone() else {
            return Subscription::none();
        };

        // Keyed on the command, so two custom modules never share a stream.
        // See the note in app::subscription.
        Subscription::run_with(source, run)
    }

    fn update(&mut self, event: Event) -> Task<Event> {
        match event {
            Event::Content(content) => self.content = content,
            Event::Activate(_) => {
                let Some(command) = self.on_click.clone() else {
                    return Task::none();
                };

                return Task::future(async move {
                    // Detached: a launcher should not hold up the bar while the
                    // thing it launched is open.
                    if let Err(error) = tokio::process::Command::new("sh")
                        .arg("-c")
                        .arg(&command)
                        .spawn()
                    {
                        eprintln!("ricebar: could not run `{command}`: {error}");
                    }
                })
                .discard();
            }
            _ => {}
        }

        Task::none()
    }

    fn view(&self, style: config::Style) -> Element<'_, Event> {
        let foreground = self.foreground.unwrap_or(style.foreground).color();
        let label = text(self.label()).color(foreground);

        // Without an action there is nothing to click, so stay a plain label.
        let Some(_) = &self.on_click else {
            let Some(background) = self.background else {
                return label.into();
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
            .on_press(Event::Activate(0))
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
}

/// What a command may print instead of plain text.
#[derive(Deserialize)]
struct Output {
    text: String,
    #[serde(default)]
    tooltip: Option<String>,
    #[serde(default)]
    percentage: Option<f32>,
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

    while let Ok(Some(line)) = lines.next_line().await {
        if output.send(Event::Content(parse(&line))).await.is_err() {
            // Stop the command rather than leave it running unread.
            let _ = child.start_kill();
            return Err(());
        }
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
    if line.starts_with('{')
        && let Ok(parsed) = serde_json::from_str::<Output>(line)
    {
        return Content {
            text: parsed.text,
            tooltip: parsed.tooltip,
            percentage: parsed.percentage,
        };
    }

    Content {
        text: line.to_owned(),
        tooltip: None,
        percentage: None,
    }
}
