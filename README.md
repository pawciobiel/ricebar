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
  compositor specifics. Hyprland, sway and niri are supported.
- **Modules.** In-tree modules implement a `Module` trait. Anything else can be a
  `[[module.custom]]` running a script — no Rust required. A script can be kept
  running and stream its updates, so a module reacts the moment something
  happens instead of polling.
- **More than one row.** `[[bar.row]]` stacks lines, each with its own left,
  centre and right, and its own height.

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
font = "JetBrainsMono Nerd Font"   # omit for the system default
font-size = 16
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

A module is made of scripts: `exec` for what it shows, `tooltip` for hover,
`on-click` for a click, `on-scroll-up`/`on-scroll-down` for the wheel, and
`popup` for a list to open. Any may be left out — a `label` and an
`on-click` with no `exec` is a launcher button.

Hovering a module opens a tooltip on its own layer-shell surface, so it is not
clipped by the bar's height. It is anchored under the module's region, which
keeps it on screen. The clock shows a full date; a custom module can print a
JSON object with `text` and `tooltip` to supply its own.

That pairing is how a module shows an icon with the detail on hover: print a
Nerd Font glyph as `text` and the numbers as `tooltip`, and set `font` to a
family that carries icon glyphs. See `memory` in
[`config.example.toml`](config.example.toml).

A module that breaks shows a warning triangle in the `urgent` colour, with the
reason on hover, and the rest of the bar carries on. A command that does not
exist, exits non-zero, never returns, or prints malformed JSON all end up
there rather than taking the bar down or leaving a blank space.

Missing config is normal and uses defaults. A *broken* config is reported on
stderr, naming the line and the valid field names, and then falls back to
defaults — a bar that refuses to start would leave you no way to fix it.

### Running commands

`[[module.custom]]`, `[[module.menu]]` and `on-click-day` all run shell
commands, so **the config file is the trust boundary**: anyone who can write it
can already run code as you. Validating the commands themselves would buy
nothing, since an attacker editing the file could name an allowed path just as
easily.

What ricebar does check is whether anyone *else* can write the file — the same
check `ssh` and `sudo` make of theirs. If the config, or the directory holding
it, is group- or other-writable, the bar still starts and still draws, but it
refuses to run any command and says so:

```
ricebar: ~/.config/ricebar/config.toml is writable by other users; refusing to run commands from it
ricebar: fix with `chmod go-w ~/.config/ricebar/config.toml`
```

Values substituted into a command — currently only `{}` in `on-click-day` — are
quoted for the shell before substitution.

## Build

```sh
cargo build --release
./target/release/ricebar                    # the usual config
./target/release/ricebar -c other.toml      # or a named one
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

What is planned next is in [TODO.md](TODO.md).

## License

MIT
