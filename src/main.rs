mod app;
mod compositor;
mod config;
mod modules;

use iced::{Font, Pixels};
use iced_layershell::build_pattern::daemon;
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};

fn main() -> Result<(), iced_layershell::Error> {
    let config = config::load();

    let edge = match config.bar.position {
        config::Position::Top => Anchor::Top,
        config::Position::Bottom => Anchor::Bottom,
    };

    let [top, right, bottom, left] = config.bar.margin;
    let height = config.bar.total_height();

    let exclusive_zone = if config.bar.exclusive {
        // A margin pushes the bar inwards, so the space to reserve is the bar
        // plus whatever gap sits between it and the edge it is anchored to.
        let gap = match config.bar.position {
            config::Position::Top => top,
            config::Position::Bottom => bottom,
        };
        i32::try_from(height)
            .unwrap_or(i32::MAX)
            .saturating_add(gap.max(0))
    } else {
        0
    };

    // `Font::with_name` holds a `&'static str`, and the family comes from a
    // config file read at runtime. Leaking it is the honest way to bridge that:
    // the font is chosen once and lives as long as the process.
    let font = config
        .bar
        .font
        .clone()
        .map_or_else(Font::default, |family| {
            Font::with_name(String::leak(family))
        });
    let font_size = Pixels(config.bar.font_size);

    daemon(
        move || app::Bar::new(config.clone()),
        app::namespace,
        app::update,
        app::view,
    )
    .style(app::style)
    .subscription(app::subscription)
    .settings(Settings {
        layer_settings: LayerShellSettings {
            anchor: edge | Anchor::Left | Anchor::Right,
            size: Some((0, height)),
            // Defaults to -1, which lets tiled windows render underneath us.
            exclusive_zone,
            margin: (top, right, bottom, left),
            layer: Layer::Top,
            // Defaults to OnDemand, which would let the bar steal focus.
            keyboard_interactivity: KeyboardInteractivity::None,
            // Requires `daemon`; `application` asserts against this mode.
            start_mode: StartMode::AllScreens,
            ..Default::default()
        },
        default_font: font,
        default_text_size: font_size,
        ..Default::default()
    })
    .run()
}
