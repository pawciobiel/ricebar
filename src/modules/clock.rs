use std::time::Duration;

use iced::futures::{SinkExt, Stream};
use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Element, Length, Subscription, Task};
use jiff::civil::{Date, Weekday};
use jiff::fmt::strtime;
use jiff::{Span, Zoned};

use super::icon::faced;
use super::{Event, Module, Popup};
use crate::config;

pub struct Clock {
    format: String,
    tooltip_format: String,
    interval: Duration,
    label: String,
    calendar: Option<Calendar>,
}

struct Calendar {
    week_numbers: bool,
    weeks_pos: config::Side,
    start_monday: bool,
    on_click_day: Option<String>,
    /// Months away from the current one, moved by the popup's arrows.
    offset: i32,
}

impl Clock {
    pub fn new(config: &config::Clock, trusted: bool) -> Self {
        let mut clock = Self {
            format: config.format.clone(),
            tooltip_format: config.tooltip_format.clone(),
            // A zero interval would spin the timer as fast as it can.
            interval: Duration::from_secs(config.interval.max(1)),
            label: String::new(),
            calendar: config.calendar.then_some(Calendar {
                week_numbers: config.week_numbers,
                weeks_pos: config.weeks_pos,
                start_monday: config.start_monday,
                on_click_day: trusted.then(|| config.on_click_day.clone()).flatten(),
                offset: 0,
            }),
        };
        clock.refresh();
        clock
    }

    fn refresh(&mut self) {
        self.label = render(&self.format);
    }

    /// Run the configured command for whichever day was clicked.
    fn run_for_day(&self, cell: usize) -> Task<Event> {
        let Some(calendar) = &self.calendar else {
            return Task::none();
        };

        let Some(template) = &calendar.on_click_day else {
            return Task::none();
        };

        let Some(shown) = month_of(Zoned::now().date(), calendar.offset) else {
            return Task::none();
        };

        let Some(day) = calendar.weeks(shown).into_iter().flatten().nth(cell) else {
            return Task::none();
        };

        // ISO 8601, which is what a script can parse without guessing.
        let date = render_date("%Y-%m-%d", day);

        let date = config::shell_quote(&date);

        let command = if template.contains("{}") {
            template.replace("{}", &date)
        } else {
            format!("{template} {date}")
        };

        Task::future(async move {
            if let Err(error) = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(&command)
                .spawn()
            {
                eprintln!("ricebar: could not run `{command}`: {error}");
            }
        })
        .discard()
    }
}

impl Module for Clock {
    fn name(&self) -> &str {
        "clock"
    }

    fn subscription(&self) -> Subscription<Event> {
        Subscription::run_with(self.interval, ticks)
    }

    fn update(&mut self, event: Event) -> Task<Event> {
        match event {
            Event::Tick => self.refresh(),
            // Every opening starts on the current month.
            Event::TogglePopup => {
                if let Some(calendar) = &mut self.calendar {
                    calendar.offset = 0;
                }
            }
            Event::Step(months) => {
                if let Some(calendar) = &mut self.calendar {
                    calendar.offset = calendar.offset.saturating_add(months);
                }
            }
            Event::Activate(cell) => return self.run_for_day(cell),
            _ => {}
        }

        Task::none()
    }

    fn view(&self, style: config::Style) -> Element<'_, Event> {
        let label = faced(text(self.label.as_str()), style);

        // Without a calendar there is nothing to click, so stay a plain label.
        if self.calendar.is_none() {
            return label.into();
        }

        button(label)
            .padding([2, 6])
            .on_press(Event::TogglePopup)
            .style(move |_theme, status| button::Style {
                background: match status {
                    button::Status::Hovered | button::Status::Pressed => {
                        Some(style.muted.color().into())
                    }
                    _ => None,
                },
                text_color: style.foreground.color(),
                border: iced::Border {
                    radius: 4.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }

    fn tooltip(&self) -> Option<String> {
        Some(render(&self.tooltip_format))
    }

    fn popup(&self, _style: config::Style) -> Option<Popup> {
        let calendar = self.calendar.as_ref()?;
        let columns: f32 = if calendar.week_numbers { 8.0 } else { 7.0 };

        Some(Popup {
            width: columns.mul_add(CELL, 2.0 * CALENDAR_PADDING),
            // Always six week rows, even in a month that needs five: the
            // surface cannot be resized, and paging must not clip the grid.
            height: 6.0f32.mul_add(ROW, HEADER + WEEKDAYS + 2.0 * CALENDAR_PADDING),
        })
    }

    fn popup_view(&self, style: config::Style) -> Element<'_, Event> {
        match &self.calendar {
            Some(calendar) => calendar.view(style),
            None => iced::widget::space::horizontal().into(),
        }
    }
}

/// Fixed cell width, so the columns line up even in a proportional font.
const CELL: f32 = 26.0;
/// The calendar fits a surface sized in advance, so it draws a little smaller
/// than the bar's own text rather than following `font-size`.
const CALENDAR_TEXT: f32 = 13.0;
/// Row heights, used to size the surface before anything is drawn.
const ROW: f32 = 19.0;
const HEADER: f32 = 26.0;
const WEEKDAYS: f32 = 19.0;
const CALENDAR_PADDING: f32 = 8.0;

impl Calendar {
    fn view(&self, style: config::Style) -> Element<'_, Event> {
        let today = Zoned::now().date();

        let Some(shown) = month_of(today, self.offset) else {
            return text("out of range").into();
        };

        let header = row![
            arrow("\u{2039}", Event::Step(-1), style),
            container(
                text(render_date("%B %Y", shown))
                    .size(CALENDAR_TEXT)
                    .color(style.foreground.color())
            )
            .center_x(Length::Fill),
            arrow("\u{203a}", Event::Step(1), style),
        ]
        .align_y(Alignment::Center);

        let mut weeks = column![].spacing(2);
        weeks = weeks.push(self.weekday_names(style));

        for (index, week) in self.weeks(shown).into_iter().enumerate() {
            weeks = weeks.push(self.week_row(&week, index * 7, shown, today, style));
        }

        column![header, weeks].spacing(4).into()
    }

    /// The days to draw, as whole weeks covering the month.
    fn weeks(&self, shown: Date) -> Vec<Vec<Date>> {
        let first = shown.first_of_month();
        let lead = self.weekday_index(first.weekday());

        // Start on the weekday column the month begins under, so the grid keeps
        // its shape; days outside the month are drawn faintly.
        let mut day = first
            .checked_add(Span::new().days(-i64::from(lead)))
            .unwrap_or(first);

        let mut weeks = Vec::new();

        for _ in 0..6 {
            let mut week = Vec::with_capacity(7);

            for _ in 0..7 {
                week.push(day);
                day = day.tomorrow().unwrap_or(day);
            }

            let done = week[0].month() != shown.month() && week[0] > shown.last_of_month();
            weeks.push(week);

            if done {
                break;
            }
        }

        weeks
    }

    fn weekday_index(&self, weekday: Weekday) -> i8 {
        if self.start_monday {
            weekday.to_monday_zero_offset()
        } else {
            weekday.to_sunday_zero_offset()
        }
    }

    fn weekday_names(&self, style: config::Style) -> Element<'_, Event> {
        let names = if self.start_monday {
            ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]
        } else {
            ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
        };

        let days = names.into_iter().map(|name| cell(name, style.dim.color()));

        // "W" heads the week column, so it takes the week colour rather than
        // the weekday one.
        self.line(days, cell("W", style.accent.color()))
    }

    fn week_row<'a>(
        &self,
        week: &[Date],
        first_cell: usize,
        shown: Date,
        today: Date,
        style: config::Style,
    ) -> Element<'a, Event> {
        let days = week.iter().enumerate().map(|(offset, day)| {
            let colour = if *day == today {
                style.urgent.color()
            } else if day.month() == shown.month() {
                style.foreground.color()
            } else {
                style.dim.color()
            };

            match self.on_click_day {
                // Only clickable when something is configured to receive it,
                // so the grid stays plain text otherwise.
                Some(_) => day_button(day.day().to_string(), colour, first_cell + offset, style),
                None => cell(day.day().to_string(), colour),
            }
        });

        let number = week[0].iso_week_date().week();

        self.line(days, cell(number.to_string(), style.accent.color()))
    }

    /// Lay out a row, putting the week column on whichever side is configured.
    fn line<'a>(
        &self,
        days: impl Iterator<Item = Element<'a, Event>>,
        week: Element<'a, Event>,
    ) -> Element<'a, Event> {
        let mut line = row![].spacing(0);

        if self.week_numbers && self.weeks_pos == config::Side::Left {
            line = line.push(week);
            for day in days {
                line = line.push(day);
            }
        } else {
            for day in days {
                line = line.push(day);
            }
            if self.week_numbers {
                line = line.push(week);
            }
        }

        line.into()
    }
}

fn day_button<'a>(
    label: String,
    colour: iced::Color,
    cell_index: usize,
    style: config::Style,
) -> Element<'a, Event> {
    button(
        container(text(label).size(CALENDAR_TEXT).color(colour))
            .width(Length::Fill)
            .center_x(Length::Fill),
    )
    .width(Length::Fixed(CELL))
    .padding(0)
    .on_press(Event::Activate(cell_index))
    .style(move |_theme, status| button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => Some(style.muted.color().into()),
            _ => None,
        },
        text_color: colour,
        border: iced::Border {
            radius: 3.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

fn cell<'a>(label: impl text::IntoFragment<'a>, colour: iced::Color) -> Element<'a, Event> {
    container(text(label).size(CALENDAR_TEXT).color(colour))
        .width(Length::Fixed(CELL))
        .center_x(Length::Fixed(CELL))
        .into()
}

fn arrow<'a>(glyph: &'a str, event: Event, style: config::Style) -> Element<'a, Event> {
    button(text(glyph).size(CALENDAR_TEXT))
        .padding([0, 6])
        .on_press(event)
        .style(move |_theme, status| button::Style {
            background: match status {
                button::Status::Hovered | button::Status::Pressed => {
                    Some(style.accent.color().into())
                }
                _ => None,
            },
            text_color: match status {
                button::Status::Hovered | button::Status::Pressed => style.background.color(),
                _ => style.foreground.color(),
            },
            border: iced::Border {
                radius: 4.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

fn month_of(today: Date, offset: i32) -> Option<Date> {
    today
        .first_of_month()
        .checked_add(Span::new().months(i64::from(offset)))
        .ok()
}

fn render_date(format: &str, date: Date) -> String {
    strtime::format(format, date).unwrap_or_else(|_| String::from("?"))
}

/// The format comes from user config, so a bad one must not bring the bar down.
/// `Zoned::strftime` returns a Display that panics when stringified.
fn render(format: &str) -> String {
    strtime::format(format, &Zoned::now()).unwrap_or_else(|_| String::from("bad format"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn calendar(start_monday: bool) -> Calendar {
        Calendar {
            week_numbers: true,
            weeks_pos: config::Side::Right,
            start_monday,
            on_click_day: None,
            offset: 0,
        }
    }

    #[test]
    fn weeks_are_whole() {
        let weeks = calendar(true).weeks(date(2026, 8, 1));
        assert!(weeks.iter().all(|week| week.len() == 7));
    }

    #[test]
    fn first_week_holds_the_first_of_the_month() {
        let weeks = calendar(true).weeks(date(2026, 8, 1));
        assert!(weeks[0].contains(&date(2026, 8, 1)));
    }

    #[test]
    fn weeks_start_on_the_configured_day() {
        let monday = calendar(true).weeks(date(2026, 8, 1));
        assert_eq!(monday[0][0].weekday(), Weekday::Monday);

        let sunday = calendar(false).weeks(date(2026, 8, 1));
        assert_eq!(sunday[0][0].weekday(), Weekday::Sunday);
    }

    #[test]
    fn every_day_of_the_month_is_drawn() {
        // February 2024 is a leap February that begins on a Thursday, so it
        // spills over both edges of the grid.
        let shown = date(2024, 2, 1);
        let drawn: Vec<_> = calendar(true)
            .weeks(shown)
            .into_iter()
            .flatten()
            .filter(|day| day.month() == shown.month())
            .collect();

        assert_eq!(drawn.len(), 29);
        assert_eq!(drawn[0], date(2024, 2, 1));
        assert_eq!(drawn[28], date(2024, 2, 29));
    }

    #[test]
    fn stepping_moves_whole_months() {
        let january = date(2026, 1, 15);
        assert_eq!(month_of(january, 1), Some(date(2026, 2, 1)));
        assert_eq!(month_of(january, -1), Some(date(2025, 12, 1)));
        assert_eq!(month_of(january, 12), Some(date(2027, 1, 1)));
    }

    #[test]
    fn stepping_from_a_long_month_does_not_overflow() {
        // The 31st has no counterpart in February; anchoring on the first of
        // the month is what keeps this from failing.
        assert_eq!(month_of(date(2026, 1, 31), 1), Some(date(2026, 2, 1)));
    }
}
