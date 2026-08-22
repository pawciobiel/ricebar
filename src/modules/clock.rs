use std::time::Duration;

use iced::futures::{SinkExt, Stream};
use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Element, Length, Subscription, Task};
use jiff::civil::{Date, Weekday};
use jiff::fmt::strtime;
use jiff::{Span, Zoned};

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
    start_monday: bool,
    /// Months away from the current one, moved by the popup's arrows.
    offset: i32,
}

impl Clock {
    pub fn new(config: &config::Clock) -> Self {
        let mut clock = Self {
            format: config.format.clone(),
            tooltip_format: config.tooltip_format.clone(),
            // A zero interval would spin the timer as fast as it can.
            interval: Duration::from_secs(config.interval.max(1)),
            label: String::new(),
            calendar: config.calendar.then_some(Calendar {
                week_numbers: config.week_numbers,
                start_monday: config.start_monday,
                offset: 0,
            }),
        };
        clock.refresh();
        clock
    }

    fn refresh(&mut self) {
        self.label = render(&self.format);
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
            _ => {}
        }

        Task::none()
    }

    fn view(&self, style: config::Style) -> Element<'_, Event> {
        let label = text(self.label.as_str());

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

    fn popup(&self) -> Option<Popup> {
        let calendar = self.calendar.as_ref()?;

        Some(Popup {
            // Seven day columns of three characters, plus the week column.
            columns: if calendar.week_numbers { 30 } else { 26 },
            // A title, the weekday names, and up to six weeks.
            rows: 8,
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

        for week in self.weeks(shown) {
            weeks = weeks.push(self.week_row(&week, shown, today, style));
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

        let mut line = row![].spacing(0);

        if self.week_numbers {
            line = line.push(cell("W", style.dim.color()));
        }

        for name in names {
            line = line.push(cell(name, style.dim.color()));
        }

        line.into()
    }

    fn week_row<'a>(
        &self,
        week: &[Date],
        shown: Date,
        today: Date,
        style: config::Style,
    ) -> Element<'a, Event> {
        let mut line = row![].spacing(0);

        if self.week_numbers {
            let number = week[0].iso_week_date().week();
            line = line.push(cell(number.to_string(), style.dim.color()));
        }

        for day in week {
            let colour = if *day == today {
                style.urgent.color()
            } else if day.month() == shown.month() {
                style.foreground.color()
            } else {
                style.dim.color()
            };

            line = line.push(cell(day.day().to_string(), colour));
        }

        line.into()
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn calendar(start_monday: bool) -> Calendar {
        Calendar {
            week_numbers: true,
            start_monday,
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
