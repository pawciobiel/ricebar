use jiff::Zoned;
use jiff::fmt::strtime;

/// A clock that renders the current local time through a strftime format.
pub struct Clock {
    format: String,
    label: String,
}

impl Clock {
    pub fn new(format: impl Into<String>) -> Self {
        let mut clock = Self {
            format: format.into(),
            label: String::new(),
        };
        clock.tick();
        clock
    }

    pub fn tick(&mut self) {
        // The format string will come from user config, so a bad one must not
        // bring the bar down. `Zoned::strftime` would panic when stringified.
        self.label = strtime::format(&self.format, &Zoned::now())
            .unwrap_or_else(|_| String::from("bad format"));
    }

    pub fn label(&self) -> &str {
        &self.label
    }
}
