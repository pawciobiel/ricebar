//! Icons, whether they are glyphs or pictures.
//!
//! An entry in an `icons` list is a font glyph unless it looks like a path, so
//! one ramp can mix them and a module needs to know nothing about which it got:
//!
//! ```toml
//! icons = ["", "~/.icons/candy/cpu-warm.svg", "/usr/share/pixmaps/hot.png"]
//! ```
//!
//! Colour follows the freedesktop convention rather than a config key: a
//! vector icon named `*-symbolic.svg` is a monochrome shape meant to take the
//! colour around it, so it follows the module's `colors`. Anything else keeps
//! the colours it was drawn with -- a weather icon with a yellow sun in it
//! stays yellow, and a raster icon could not be recoloured anyway.

use std::path::PathBuf;

use iced::widget::{image, row, svg, text};
use iced::{Alignment, Color, Element, Length};

/// One entry from an `icons` list, resolved.
///
/// Worked out once when the module is built rather than while drawing: what a
/// string means, where `~` points, and building a handle are not things to do
/// on every frame.
#[derive(Debug, Clone)]
pub enum Icon {
    /// Text: a Nerd Font glyph, an emoji, a word. The common case.
    Glyph(String),
    /// A vector image. `symbolic` follows the freedesktop convention: a file
    /// named `*-symbolic.svg` is a monochrome shape meant to take the colour
    /// around it, so it follows the module's `colors`. Any other keeps the
    /// colours it was drawn with, which is what a weather icon with a yellow
    /// sun in it needs.
    Vector { handle: svg::Handle, symbolic: bool },
    /// A raster image -- png, jpeg, whatever can be decoded. Keeps its own
    /// colours, since there is nothing sensible to recolour.
    Raster(image::Handle),
}

impl Icon {
    /// Read one entry from config.
    pub fn parse(value: &str) -> Self {
        let Some(path) = as_path(value) else {
            return Self::Glyph(String::from(value));
        };

        // A path that is not there would otherwise draw nothing at all, which
        // looks like the icon silently not working. Say so, and show the same
        // warning triangle everything else uses.
        if !path.is_file() {
            eprintln!("ricebar: icon {} does not exist", path.display());
            return Self::Glyph(String::from(super::BROKEN));
        }

        let vector = path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"));

        if !vector {
            return Self::Raster(image::Handle::from_path(path));
        }

        let symbolic = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| stem.ends_with("-symbolic"));

        Self::Vector {
            handle: svg::Handle::from_path(path),
            symbolic,
        }
    }

    fn view<'a, Message: 'a>(
        self,
        size: f32,
        color: Color,
        style: crate::config::Style,
    ) -> Element<'a, Message> {
        match self {
            // Through `faced`, so a module's own `font` and `font-size` reach a
            // glyph icon as they do its text. Without that a face named for the
            // module was silently ignored for exactly the thing it was most
            // likely named for -- and a Nerd Font glyph drawn from the wrong
            // variant overflows the fill behind it.
            Self::Glyph(glyph) => faced(text(glyph).color(color), style).into(),
            Self::Vector { handle, symbolic } => svg(handle)
                .width(Length::Fixed(size))
                .height(Length::Fixed(size))
                .style(move |_theme, _status| svg::Style {
                    color: symbolic.then_some(color),
                })
                .into(),
            Self::Raster(handle) => image(handle)
                .width(Length::Fixed(size))
                .height(Length::Fixed(size))
                .into(),
        }
    }
}

/// Whether this entry names a file rather than being a glyph to print.
///
/// A glyph is one or two characters and never contains a separator, so a `/`
/// or a leading `~` is enough to tell them apart without a prefix to remember.
fn as_path(value: &str) -> Option<PathBuf> {
    if !value.contains('/') && !value.starts_with('~') {
        return None;
    }

    // Nothing runs a shell for an icon, so `~` has to be expanded here.
    let Some(rest) = value.strip_prefix("~/") else {
        return Some(PathBuf::from(value));
    };

    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(rest))
}

/// Lay out a module's `format`, with the icon as a widget of its own.
///
/// The icon cannot always live inside the string: a picture is not text. The
/// format is split around `{icon}` instead, so a glyph and an image sit in the
/// same row and are written the same way in config.
pub fn labelled<'a, Message: 'a>(
    format: &str,
    icon: Option<Icon>,
    value: &str,
    color: Color,
    size: f32,
    style: crate::config::Style,
) -> Element<'a, Message> {
    let label = move |body: String| {
        let drawn = text(body).color(color).size(style.font_size);
        match style.font {
            Some(font) => drawn.font(font),
            None => drawn,
        }
    };

    let Some((before, after)) = format.split_once("{icon}") else {
        // No slot for one, so the format is all text however it was written.
        return label(format.replace("{value}", value)).into();
    };

    let before = before.replace("{value}", value);
    let after = after.replace("{value}", value);

    let mut parts: Vec<Element<'a, Message>> = Vec::with_capacity(3);

    if !before.is_empty() {
        parts.push(label(before).into());
    }

    if let Some(icon) = icon {
        parts.push(icon.view(size, color, style));
    }

    if !after.is_empty() {
        parts.push(label(after).into());
    }

    // No spacing: the gaps are whatever the format string puts there, which is
    // what makes `"{icon} {value}"` and `"{icon}{value}"` mean what they say.
    row(parts).align_y(Alignment::Center).into()
}

/// Apply the resolved face and size to a text widget.
///
/// Every module draws its labels through this, so `font` on a bar or a module
/// reaches all of them rather than only the ones that go through [`labelled`].
pub fn faced<'a>(
    drawn: iced::widget::Text<'a>,
    style: crate::config::Style,
) -> iced::widget::Text<'a> {
    let drawn = drawn.size(style.font_size);
    match style.font {
        Some(font) => drawn.font(font),
        None => drawn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glyph_is_anything_without_a_separator() {
        assert!(matches!(Icon::parse("\u{f2db}"), Icon::Glyph(_)));
        assert!(matches!(Icon::parse("cpu"), Icon::Glyph(_)));
        assert!(matches!(Icon::parse(""), Icon::Glyph(_)));
    }

    #[test]
    fn a_path_is_anything_with_one() {
        // Missing files fall back to a glyph, so this checks the decision
        // rather than the outcome.
        assert!(as_path("/usr/share/icons/x.svg").is_some());
        assert!(as_path("./icons/x.png").is_some());
        assert!(as_path("~/icons/x.svg").is_some());
        assert!(as_path("\u{f2db}").is_none());
    }

    /// The freedesktop convention decides whether an icon is recoloured, so
    /// getting the suffix wrong silently paints over a colourful icon.
    #[test]
    fn only_symbolic_vectors_are_recoloured() {
        let dir = std::env::temp_dir().join("ricebar-icon-test");
        std::fs::create_dir_all(&dir).unwrap();

        let symbolic = dir.join("weather-clear-symbolic.svg");
        let colourful = dir.join("weather-clear.svg");
        let raster = dir.join("weather-clear.png");

        for path in [&symbolic, &colourful, &raster] {
            std::fs::write(path, "").unwrap();
        }

        assert!(matches!(
            Icon::parse(symbolic.to_str().unwrap()),
            Icon::Vector { symbolic: true, .. }
        ));
        assert!(matches!(
            Icon::parse(colourful.to_str().unwrap()),
            Icon::Vector {
                symbolic: false,
                ..
            }
        ));
        assert!(matches!(
            Icon::parse(raster.to_str().unwrap()),
            Icon::Raster(_)
        ));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn home_is_expanded_because_no_shell_will_do_it() {
        // SAFETY: single-threaded test, and the value is restored below.
        let before = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", "/home/someone") };

        assert_eq!(
            as_path("~/.icons/x.svg"),
            Some(PathBuf::from("/home/someone/.icons/x.svg"))
        );

        match before {
            Some(value) => unsafe { std::env::set_var("HOME", value) },
            None => unsafe { std::env::remove_var("HOME") },
        }
    }
}
