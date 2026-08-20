use std::time::Duration;

use iced::futures::{SinkExt, Stream};
use iced::widget::text;
use iced::{Element, Subscription, Task};
use jiff::Zoned;
use jiff::fmt::strtime;

use super::{Event, Module};
use crate::config;

pub struct Clock {
    format: String,
    interval: Duration,
    label: String,
}

impl Clock {
    pub fn new(config: &config::Clock) -> Self {
        let mut clock = Self {
            format: config.format.clone(),
            // A zero interval would spin the timer as fast as it can.
            interval: Duration::from_secs(config.interval.max(1)),
            label: String::new(),
        };
        clock.refresh();
        clock
    }

    fn refresh(&mut self) {
        // The format comes from user config, so a bad one must not bring the bar
        // down. `Zoned::strftime` returns a Display that panics when stringified.
        self.label = strtime::format(&self.format, &Zoned::now())
            .unwrap_or_else(|_| String::from("bad format"));
    }
}

impl Module for Clock {
    fn name(&self) -> &'static str {
        "clock"
    }

    fn subscription(&self) -> Subscription<Event> {
        Subscription::run_with(self.interval, ticks)
    }

    fn update(&mut self, event: Event) -> Task<Event> {
        if matches!(event, Event::Tick) {
            self.refresh();
        }
        Task::none()
    }

    fn view(&self, _style: config::Style) -> Element<'_, Event> {
        text(self.label.as_str()).into()
    }
}

/// `Subscription::run_with` takes a plain fn pointer and hashes the data it is
/// keyed on, so two clocks configured at different intervals stay distinct.
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
