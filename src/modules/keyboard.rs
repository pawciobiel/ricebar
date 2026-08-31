//! The keyboard layout in use, and a popup to choose another.
//!
//! sway and niri name a layout the way xkb describes it — "Polish", "English
//! (UK)" — and Hyprland names it by the code it was configured with, `pl`.
//! Both forms are accepted and turned into whichever the bar wants, by reading
//! xkb's own table: no list of countries in here, and no key in config.

use std::collections::HashMap;

use iced::widget::{button, column, text};
use iced::{Element, Length, Subscription, Task};

use super::icon::faced;
use super::{BROKEN, Event, Icon, Module, Popup, labelled};
use crate::compositor::{self, Compositor, Layouts};
use crate::config;

/// Per-entry metrics, used to size the popup before anything is drawn.
const ENTRY_GLYPH: f32 = 9.5;
const ENTRY_HEIGHT: f32 = 24.0;
const ENTRY_PADDING: f32 = 10.0;

pub struct Keyboard {
    compositor: Option<Box<dyn Compositor>>,
    format: String,
    icon: Option<Icon>,
    icon_size: Option<f32>,
    short: bool,
    /// xkb's own table, read once: it is a 46K file, and nothing in it changes
    /// while the bar runs.
    xkb: Xkb,
    /// What the compositor last said. Empty until it says anything.
    layouts: Layouts,
    background: Option<config::Rgba>,
    foreground: Option<config::Rgba>,
}

impl Keyboard {
    pub fn new(config: &config::Keyboard) -> Self {
        let compositor = compositor::detect(config.compositor);

        if compositor.is_none() {
            eprintln!("ricebar: keyboard: no supported compositor detected");
        }

        Self {
            compositor,
            format: config.format.clone(),
            icon: (!config.icon.is_empty()).then(|| Icon::parse(&config.icon)),
            icon_size: config.icon_size,
            short: config.short,
            xkb: Xkb::read(),
            layouts: Layouts::default(),
            background: config.background,
            foreground: config.foreground,
        }
    }

    /// What to draw in the bar for the layout in use.
    fn value(&self) -> String {
        let Some(name) = self.layouts.active() else {
            return String::new();
        };

        if self.short {
            self.xkb.short(name)
        } else {
            self.xkb.long(name)
        }
    }
}

impl Module for Keyboard {
    fn name(&self) -> &str {
        "keyboard"
    }

    fn subscription(&self) -> Subscription<Event> {
        match &self.compositor {
            Some(compositor) => compositor.layouts().map(Event::Layouts),
            None => Subscription::none(),
        }
    }

    fn update(&mut self, event: Event) -> Task<Event> {
        match event {
            Event::Layouts(layouts) => self.layouts = layouts,
            // The popup is closed by the bar, which is also what tells this
            // module the click happened.
            Event::Activate(index) => {
                if let Some(compositor) = &self.compositor {
                    return compositor.set_layout(index).discard();
                }
            }
            // The shared enum carries events for every kind of module.
            _ => {}
        }

        Task::none()
    }

    fn view(&self, style: config::Style) -> Element<'_, Event> {
        // Nothing to talk to is a failure like any other: mark it, and put the
        // reason on hover rather than in the bar.
        let broken = self.compositor.is_none();
        let foreground = if broken {
            style.urgent
        } else {
            self.foreground.unwrap_or(style.foreground)
        };

        let icon = if broken {
            Some(Icon::Glyph(String::from(BROKEN)))
        } else {
            self.icon.clone()
        };

        let value = if broken { String::new() } else { self.value() };
        let label = labelled(
            &self.format,
            icon,
            &value,
            foreground.color(),
            self.icon_size.unwrap_or(style.icon_size),
            style,
        );

        if broken {
            return label;
        }

        let background = self.background;

        button(label)
            .padding([2, 6])
            .on_press(Event::TogglePopup)
            .style(move |_theme, status| button::Style {
                background: match status {
                    button::Status::Hovered | button::Status::Pressed => {
                        Some(style.muted.color().into())
                    }
                    _ => background.map(|colour| colour.color().into()),
                },
                text_color: foreground.color(),
                border: iced::Border {
                    radius: 4.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }

    fn tooltip(&self) -> Option<String> {
        if self.compositor.is_none() {
            return Some(String::from(
                "no supported compositor, so no keyboard layout to report",
            ));
        }

        // The full name, which is the part `short` throws away. Nothing at all
        // until the compositor has said, rather than an empty hover box.
        self.layouts.active().map(|name| self.xkb.long(name))
    }

    fn popup(&self) -> Option<Popup> {
        if self.layouts.names.is_empty() {
            return None;
        }

        let widest = self
            .layouts
            .names
            .iter()
            .map(|name| self.xkb.long(name).chars().count())
            .max()
            .unwrap_or(0) as f32;

        Some(Popup {
            // Text cannot be measured outside a renderer, so this over-estimates
            // rather than risk clipping a label.
            width: widest.mul_add(ENTRY_GLYPH, 2.0 * ENTRY_PADDING),
            height: (self.layouts.names.len() as f32).mul_add(ENTRY_HEIGHT, 2.0 * ENTRY_PADDING),
        })
    }

    fn popup_view(&self, style: config::Style) -> Element<'_, Event> {
        let current = self.layouts.current;

        let entries = self.layouts.names.iter().enumerate().map(|(index, name)| {
            // The long name here whatever `short` says: the bar is where room
            // is short, and this is the list you opened to read.
            let label = self.xkb.long(name);
            let chosen = index == current;

            button(faced(text(label).wrapping(text::Wrapping::None), style))
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
                        // The one in use, marked by colour rather than by a glyph
                        // that may not be in the bar's font.
                        _ if chosen => style.accent.color(),
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

        column(entries).spacing(2).into()
    }
}

/// Where xkb keeps its own description of every layout.
///
/// `evdev.lst` is what a Wayland compositor resolves names through; `base.lst`
/// is the same table under the older rule set, and is there on the systems
/// that ship only that.
const XKB_RULES: [&str; 2] = [
    "/usr/share/X11/xkb/rules/evdev.lst",
    "/usr/share/X11/xkb/rules/base.lst",
];

/// xkb's table, both ways round.
///
/// Nothing here fails loudly. A machine with no xkb data still shows its
/// layouts, under whichever name the compositor gave.
#[derive(Default)]
struct Xkb {
    /// "Polish" to `pl`. A variant maps to the layout it belongs to, since
    /// that is the half a bar has room for.
    codes: HashMap<String, String>,
    /// `pl` to "Polish". Layouts only, so a code never resolves to the
    /// description of one of its variants.
    names: HashMap<String, String>,
}

impl Xkb {
    fn read() -> Self {
        let Some(text) = XKB_RULES
            .iter()
            .find_map(|path| std::fs::read_to_string(path).ok())
        else {
            return Self::default();
        };

        Self::parse(&text)
    }

    /// Read the `! layout` and `! variant` sections of an xkb rules list.
    ///
    /// ```text
    /// ! layout
    ///   pl              Polish
    /// ! variant
    ///   neo             de: German (Neo 2)
    /// ```
    fn parse(text: &str) -> Self {
        let mut found = Self::default();
        let mut section = "";

        for line in text.lines() {
            if let Some(name) = line.strip_prefix("! ") {
                section = name.trim();
                continue;
            }

            let Some((first, rest)) = line.trim().split_once(char::is_whitespace) else {
                continue;
            };
            let rest = rest.trim();

            match section {
                "layout" => {
                    found.codes.insert(rest.to_owned(), first.to_owned());
                    found.names.insert(first.to_owned(), rest.to_owned());
                }
                "variant" => {
                    if let Some((layout, description)) = rest.split_once(": ") {
                        found
                            .codes
                            .insert(description.trim().to_owned(), layout.to_owned());
                    }
                }
                _ => {}
            }
        }

        found
    }

    /// What the bar shows: the code, in upper case because that is how a bar
    /// shows a layout, even though xkb writes it in lower case.
    fn short(&self, name: &str) -> String {
        if let Some(code) = self.codes.get(name) {
            return code.to_uppercase();
        }

        // Hyprland reports the code to begin with.
        if self.names.contains_key(name) {
            return name.to_uppercase();
        }

        // Something xkb has never heard of. Show it as it came.
        name.to_owned()
    }

    /// The long name, for the popup and for hover.
    fn long(&self, name: &str) -> String {
        self.names
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES: &str = "\
! model
  pc105           Generic 105-key PC
! layout
  us              English (US)
  gb              English (UK)
  pl              Polish
! variant
  neo             de: German (Neo 2)
  dvorak          us: English (Dvorak)
! option
  grp:alt_shift_toggle  Alt+Shift
";

    #[test]
    fn a_layout_is_read_both_ways_round() {
        let xkb = Xkb::parse(RULES);

        assert_eq!(xkb.codes.get("Polish").map(String::as_str), Some("pl"));
        assert_eq!(
            xkb.names.get("gb").map(String::as_str),
            Some("English (UK)")
        );
    }

    /// A variant names its own layout, and that is the half worth showing.
    #[test]
    fn a_variant_gives_the_layout_it_belongs_to() {
        let xkb = Xkb::parse(RULES);

        assert_eq!(
            xkb.codes.get("German (Neo 2)").map(String::as_str),
            Some("de")
        );
        assert_eq!(
            xkb.codes.get("English (Dvorak)").map(String::as_str),
            Some("us")
        );
        // A variant must not become the description of its own layout, or
        // `us` would read back as "English (Dvorak)".
        assert_eq!(
            xkb.names.get("us").map(String::as_str),
            Some("English (US)")
        );
    }

    /// Models and options share the file and must not end up in the table --
    /// "Alt+Shift" is not a layout.
    #[test]
    fn only_layouts_and_variants_are_read() {
        let xkb = Xkb::parse(RULES);

        assert!(!xkb.codes.contains_key("Generic 105-key PC"));
        assert!(!xkb.codes.contains_key("Alt+Shift"));
    }

    /// The compositors disagree about what to call a layout, and the bar must
    /// not.
    #[test]
    fn either_name_a_compositor_gives_reads_the_same_way() {
        let xkb = Xkb::parse(RULES);

        // sway and niri hand over a description...
        assert_eq!(xkb.short("Polish"), "PL");
        assert_eq!(xkb.long("Polish"), "Polish");

        // ...and Hyprland hands over the code.
        assert_eq!(xkb.short("pl"), "PL");
        assert_eq!(xkb.long("pl"), "Polish");

        // Anything else is shown as it came rather than dropped.
        assert_eq!(xkb.short("Klingon"), "Klingon");
        assert_eq!(xkb.long("Klingon"), "Klingon");
    }

    #[test]
    fn the_bar_shows_the_layout_in_use() {
        let mut module = Keyboard::new(&config::Keyboard::default());
        module.xkb = Xkb::parse(RULES);
        module.layouts = Layouts {
            names: vec![String::from("pl"), String::from("gb")],
            current: 1,
        };

        assert_eq!(module.value(), "GB");

        module.short = false;
        assert_eq!(module.value(), "English (UK)");
    }

    /// The popup lists every layout, so it needs a row for each.
    #[test]
    fn the_popup_is_sized_for_every_layout() {
        let mut module = Keyboard::new(&config::Keyboard::default());
        module.xkb = Xkb::parse(RULES);

        assert!(
            module.popup().is_none(),
            "nothing to choose from before the compositor has said"
        );

        module.layouts = Layouts {
            names: vec![String::from("pl"), String::from("gb")],
            current: 0,
        };

        let popup = module.popup().expect("two layouts, two rows");
        assert!(popup.height >= 2.0 * ENTRY_HEIGHT);
        // Wide enough for "English (UK)" rather than for "GB".
        assert!(popup.width >= 12.0 * ENTRY_GLYPH);
    }
}
