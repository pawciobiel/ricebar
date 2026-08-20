# ricebar

A configurable Wayland status bar built with [iced](https://iced.rs), for
compositors implementing `wlr-layer-shell`.

> Status: early. See milestones below.

## Why

Waybar is excellent but styled through GTK CSS. ricebar aims for a single TOML
config, a Rust module API, and a script escape hatch for everything else.

## Design

- **[iced](https://iced.rs) + [iced_layershell](https://crates.io/crates/iced_layershell)**
  on vanilla iced 0.14 — no forks.
- **The Elm loop.** A `Message` arrives, `update` mutates state, `view` redraws.
- **Compositor-agnostic.** A `Compositor` trait keeps the bar core free of
  compositor specifics. Hyprland first; sway and niri to follow.
- **Modules.** In-tree modules implement a `Module` trait. Anything else can be a
  `[[module.custom]]` running a script — no Rust required.

## Configuration

`$XDG_CONFIG_HOME/ricebar/config.toml`, or `~/.config/ricebar/config.toml`.
See [`config.example.toml`](config.example.toml) for every option.

A module runs only if it is named in one of the `modules-*` lists, so removing
`"clock"` from `modules-center` removes the clock.

```toml
[bar]
position = "top"          # top | bottom
height = 32
margin = [0, 0, 0, 0]     # top, right, bottom, left
modules-left = ["workspaces"]
modules-center = ["clock"]
modules-right = []

[bar.style]
background = "#1e1e2e"    # #rgb, #rgba, #rrggbb and #rrggbbaa all work
foreground = "#cdd6f4"
accent = "#89b4fa"        # fill behind the focused item
border-color = "#89b4fa"
border-width = 0
border-radius = 0

[module.clock]
format = "%Y-%m-%d %H:%M:%S"
interval = 1

[module.workspaces]
show-empty = true

# Any shell command becomes a module. No Rust required.
[[module.custom]]
name = "memory"
exec = "free -m | awk '/^Mem:/ {printf \"%.0f%%\", $3/$2*100}'"
interval = 5
```

Hovering a module opens a tooltip on its own layer-shell surface, so it is not
clipped by the bar's height. It is anchored under the module's region, which
keeps it on screen. The clock shows a full date; a custom module can print a
JSON object with `text` and `tooltip` to supply its own.

Missing config is normal and uses defaults. A *broken* config is reported on
stderr, naming the line and the valid field names, and then falls back to
defaults — a bar that refuses to start would leave you no way to fix it.

## Build

```sh
cargo build --release
./target/release/ricebar
```

Needs `wayland-client`, `libxkbcommon`, and a Vulkan driver (iced falls back to a
software renderer via `tiny-skia` if wgpu is unavailable).

## Milestones

- [x] **M0** Layer-shell surface, multi-monitor, exclusive zone
- [x] **M1** Elm loop + clock
- [x] **M2** TOML config + styling
- [x] **M3** Hyprland workspaces
- [x] **M4** `Module` trait + registry
- [x] **M5** `[[module.custom]]` script modules
- [ ] Per-monitor bars (currently every output shows the same content)

## License

MIT
