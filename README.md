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

`$XDG_CONFIG_HOME/ricebar/config.toml`:

```toml
[bar]
position = "top"
height = 32
modules-left = ["workspaces"]
modules-center = ["clock"]

[bar.style]
background = "#1e1e2ecc"
border-color = "#89b4fa"
border-width = 2
border-radius = 8

[module.clock]
format = "%Y-%m-%d %H:%M"
```

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
- [ ] **M2** TOML config + styling
- [ ] **M3** Hyprland workspaces
- [ ] **M4** `Module` trait + registry
- [ ] **M5** `[[module.custom]]` script modules

## License

MIT
