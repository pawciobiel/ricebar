mod app;
mod clock;
mod compositor;

use iced_layershell::build_pattern::daemon;
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};

const HEIGHT: u32 = 32;

fn main() -> Result<(), iced_layershell::Error> {
    daemon(app::Bar::default, app::namespace, app::update, app::view)
        .style(app::style)
        .subscription(app::subscription)
        .settings(Settings {
            layer_settings: LayerShellSettings {
                anchor: Anchor::Top | Anchor::Left | Anchor::Right,
                size: Some((0, HEIGHT)),
                // Defaults to -1, which lets tiled windows render underneath us.
                exclusive_zone: HEIGHT as i32,
                layer: Layer::Top,
                // Defaults to OnDemand, which would let the bar steal focus.
                keyboard_interactivity: KeyboardInteractivity::None,
                // Requires `daemon`; `application` asserts against this mode.
                start_mode: StartMode::AllScreens,
                ..Default::default()
            },
            ..Default::default()
        })
        .run()
}
