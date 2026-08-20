//! Runs a shell command on an interval and shows its output.
//!
//! This is the escape hatch: anything that can be scripted becomes a module
//! without writing Rust.
//!
//! A command may print plain text, or a JSON object with `text` and an optional
//! `tooltip`, which is the convention waybar's custom modules use.

use std::time::Duration;

use iced::futures::{SinkExt, Stream};
use iced::widget::text;
use iced::{Element, Subscription, Task};
use serde::Deserialize;

use super::{Content, Event, Module};
use crate::config;

pub struct Custom {
    name: String,
    command: String,
    interval: Duration,
    content: Content,
}

impl Custom {
    pub fn new(config: &config::Custom) -> Self {
        Self {
            name: config.name.clone(),
            command: config.exec.clone(),
            // A zero interval would re-run the command as fast as it completes.
            interval: Duration::from_secs(config.interval.max(1)),
            content: Content {
                text: String::new(),
                tooltip: config.tooltip.clone(),
            },
        }
    }
}

impl Module for Custom {
    fn name(&self) -> &str {
        &self.name
    }

    fn subscription(&self) -> Subscription<Event> {
        // Keyed on the command and interval, so two custom modules never share
        // a stream. See the note in app::subscription.
        Subscription::run_with((self.command.clone(), self.interval), run)
    }

    fn update(&mut self, event: Event) -> Task<Event> {
        if let Event::Content(content) = event {
            self.content = content;
        }
        Task::none()
    }

    fn view(&self, _style: config::Style) -> Element<'_, Event> {
        text(self.content.text.as_str()).into()
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
}

fn run(key: &(String, Duration)) -> impl Stream<Item = Event> + use<> {
    let (command, interval) = key.clone();

    iced::stream::channel(1, async move |mut output| {
        let mut timer = tokio::time::interval(interval);

        loop {
            timer.tick().await;

            if output
                .send(Event::Content(execute(&command, interval).await))
                .await
                .is_err()
            {
                return;
            }
        }
    })
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

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = stdout.trim();

    // Plain text is by far the common case, so only try JSON when it looks
    // like an object.
    if stdout.starts_with('{')
        && let Ok(parsed) = serde_json::from_str::<Output>(stdout)
    {
        return Content {
            text: parsed.text,
            tooltip: parsed.tooltip,
        };
    }

    Content {
        text: stdout.to_owned(),
        tooltip: None,
    }
}
