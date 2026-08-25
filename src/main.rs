mod app;
mod compositor;
mod config;
mod modules;

use iced::{Font, Pixels};
use iced_layershell::build_pattern::daemon;
use iced_layershell::reexport::{KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};

/// What the command line asked for, if anything.
enum Arguments {
    Run(Option<std::path::PathBuf>),
    /// Printed and exited, rather than starting a bar.
    Handled,
}

fn arguments() -> Arguments {
    let mut arguments = std::env::args().skip(1);

    // Every flag either starts the bar or ends here, so only the first matters.
    let Some(argument) = arguments.next() else {
        return Arguments::Run(None);
    };

    match argument.as_str() {
        "-c" | "--config" => match arguments.next() {
            Some(path) => Arguments::Run(Some(path.into())),
            None => {
                eprintln!("ricebar: --config needs a path");
                Arguments::Handled
            }
        },
        "-h" | "--help" => {
            println!("{USAGE}");
            Arguments::Handled
        }
        "-V" | "--version" => {
            println!("ricebar {}", env!("CARGO_PKG_VERSION"));
            Arguments::Handled
        }
        other => {
            eprintln!("ricebar: unknown argument `{other}`");
            eprintln!("{USAGE}");
            Arguments::Handled
        }
    }
}

const USAGE: &str = "\
Usage: ricebar [options]

  -c, --config <path>  Read this config instead of the usual one
  -h, --help           Show this message
  -V, --version        Show the version

Without --config, ricebar reads $XDG_CONFIG_HOME/ricebar/config.toml,
or ~/.config/ricebar/config.toml.";

fn main() -> Result<(), iced_layershell::Error> {
    let Arguments::Run(path) = arguments() else {
        return Ok(());
    };

    let config = config::load(path);

    // The default font belongs to the runtime rather than to a surface, so
    // with several bars only the first one's choice can be honoured.
    //
    // `Font::with_name` holds a `&'static str`, and the family comes from a
    // config file read at runtime. Leaking it is the honest way to bridge that:
    // the font is chosen once and lives as long as the process.
    let first = config.first();
    let font = first.font.map_or_else(Font::default, |family| {
        Font::with_name(String::leak(family))
    });
    let font_size = Pixels(first.font_size);

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
            // Nothing here describes a bar. `Background` starts the runtime
            // with no surface at all, and every bar is then created from
            // `app::place` once the monitors are known -- which is the only
            // way to know which surface landed on which monitor, since
            // `AllScreens` creates them itself and reports neither.
            //
            // Requires `daemon`; `application` asserts against this mode.
            start_mode: StartMode::Background,
            layer: Layer::Top,
            // Defaults to OnDemand, which would let a bar steal focus.
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        },
        default_font: font,
        default_text_size: font_size,
        ..Default::default()
    })
    .run()
}
