# ricebar

**A Wayland status bar you configure in one file.**
No stylesheet, no second language, no D-Bus. Built with [iced](https://iced.rs)
for Hyprland, sway, niri and anything else speaking `wlr-layer-shell`.

![ricebar running on sway](docs/screenshot.png)

```toml
[bar]
modules-left = ["workspaces"]
modules-center = ["clock"]
modules-right = ["cpu", "battery"]

[bar.style]
background = "#1e1e2e"
accent = "#89b4fa"

[module.cpu]
format = "{icon} {value}"
colors = ["#a6e3a1", "#f9e2af", "#f38ba8"]   # green, amber, red by load
```

That is the whole idea. Colours, layout, modules and the commands behind them
live together in `~/.config/ricebar/config.toml`, and editing it updates the
running bar.

## Why another bar?

There are good bars for Wayland already. This one was written around a
particular set of wishes:

**One file, one language.** No CSS, no SCSS, no stylesheet to keep in step with
a config. Colours, layout, modules and the commands behind them sit together in
one TOML file.

**Edit it and see it.** Saving that file updates the running bar, so a colour
or a module is a change you make and look at, not a change you make and restart
for. A mistake in it leaves the running config alone and puts a warning in the
bar, rather than the bar disappearing on you.

Beyond those two:

- **Scripts are first-class, not an afterthought.** A module can be up to three
  scripts — one for what it shows, one for hover, one for a click — and a script
  can build a popup, stream its updates, or choose its own icon.
- **No D-Bus, no tray.** ricebar talks to a compositor socket and reads `/proc`.
  That is a deliberate limit, not a gap: see [what it does not do](#what-it-does-not-do).
- **Icons can be pictures.** svg, png or a font glyph, mixed freely, sized and
  coloured from config.
- **Vanilla iced 0.14**, not a fork of it.

## Features

**The bar**

- Top or bottom, any height, margins, border width, radius and per-module fills
- **More than one row.** `[[bar.row]]` stacks lines, each with its own left,
  centre and right, and its own height
- Multiple monitors, and hotplug
- Reserves its own space, so nothing renders underneath it
- Runs a second instance for a second edge

**Built in**

| module | what it does |
|---|---|
| `clock` | time in any `strftime` format, with a **click-through calendar** — month navigation, ISO week numbers, and a command per day |
| `workspaces` | clickable, live, on Hyprland, sway and niri |
| `cpu` `memory` | usage, with colour by level |
| `temperature` | from hwmon, preferring a named sensor over the ACPI zone |
| `battery` | charge level, and a plug when it is on mains |
| `backlight` | brightness, with the wheel to change it |
| `[[module.menu]]` | a label that opens a list of commands — a power menu in six lines |

**From scripts, with no Rust**

| feature | how |
|---|---|
| Any command as a module | `exec`, on an interval or `stream = true` |
| React the instant something happens | a streaming script blocks on `pactl subscribe` and prints; nothing polls |
| Hover text | `tooltip`, or `"tooltip"` in the script's JSON |
| Click actions | `on-click`; with a `label` and no `exec` that is a launcher button |
| Wheel actions | `on-scroll-up` / `on-scroll-down` |
| **Script-built popups** | `popup` prints a JSON list of labels and commands, drawn on its own layer surface |
| **A ticker** | `scroll-width` scrolls anything too wide for a bar past a fixed window |
| Icon by value | the script reports `percentage`, config maps it to an icon and a colour |
| Icon by name | the script returns `"icon"`, a glyph or a path, for icons that are not a level |

Examples ship with it and are written out on first run: volume, microphone,
network, weather, a market ticker, and a window switcher that works out the
compositor for itself.

**Icons**

Anything in an `icons` list that looks like a path — it has a `/`, or starts
with `~` — is a picture. **svg**, **png** and **jpeg** all work, and one ramp
can mix them with glyphs:

```toml
[module.cpu]
icons = ["\uf2db", "~/.icons/candy/cpu-warm.svg", "/usr/share/pixmaps/hot.png"]
```

`[bar.style] icon-size` sizes them, and a module can override it. Colour
follows the freedesktop convention rather than another config key: an svg named
`*-symbolic.svg` is recoloured to follow the module's `colors`, so a memory
icon turns amber under load; anything else keeps the colours it was drawn with,
so a weather icon keeps its yellow sun.

## Install

```sh
cargo install --git https://github.com/pawciobiel/ricebar
```

That puts `ricebar` in `~/.cargo/bin`. `cargo install --git … --force` updates
it later.

Or build it yourself:

```sh
git clone https://github.com/pawciobiel/ricebar
cd ricebar
cargo build --release
./target/release/ricebar
```

Not on crates.io: publishing there requires signing in through GitHub and
granting the account access it does not need. Installing from git needs no
account at all.

Needs `wayland-client`, `libxkbcommon`, and a Vulkan driver — iced falls back to
software rendering via `tiny-skia` if wgpu is unavailable.

Nothing else. The binary links against libc, libgcc and libxkbcommon, and opens
Wayland and Vulkan at runtime. There is no D-Bus, PulseAudio or PipeWire library
in it, because audio is not a built-in module: it is a script calling `pactl`,
which costs nothing on a machine that does not use it, and the first run leaves
it commented out where `pactl` is missing.

**The first run writes a config for you**, along with the example scripts and
icons it refers to, and says where they went. What it enables depends on the
machine: `PATH` decides whether the launcher and the volume module are on, and
an actual sensor read decides whether the battery is, so a desktop does not get
a battery module that can only ever fail. Anything left out is written commented
out with the reason beside it, which is also the instruction for turning it on:

```toml
modules-right = [
    "weather",
    # "volume",  # needs pactl
    "cpu",
]
```

Start it from your compositor's config:

```
# Hyprland
exec-once = ricebar

# sway
exec ricebar

# niri
spawn-at-startup "ricebar"
```

## Configuration

`$XDG_CONFIG_HOME/ricebar/config.toml`, or `~/.config/ricebar/config.toml`.
`-c <path>` reads another one, which is also how you run a second bar.

Every option is documented in [`config.example.toml`](config.example.toml).
A module runs only if it is named in a `modules-*` list, so deleting `"clock"`
from `modules-center` removes the clock.

A custom module is up to three scripts, any of which may be left out:

```toml
[[module.custom]]
name = "volume"
exec = "~/.config/ricebar/scripts/volume.sh"   # what it shows
stream = true                                   # kept running, one line per update
icons = ["\uf026", "\uf027", "\uf028"]        # chosen by the percentage it reports
format = "{icon} {value}"
tooltip = "Volume"                              # hover
on-click = "pavucontrol"                        # click
on-scroll-up = "pactl set-sink-volume @DEFAULT_SINK@ +5%"
```

### Reloading

Saving the config reloads the bar — modules, colours, formats, scripts and
icons all change in place.

Six settings cannot: `position`, `margin`, `exclusive`, the total `height`,
`font` and `font-size`. A layer surface is sized and placed once when it is
created, and the font is chosen once for the process. Changing one of those is
**refused whole** rather than half-applied, and the bar says so.

A config that does not parse never reaches the bar. The running one keeps going,
the parser error goes to stderr naming the line, and a warning triangle appears
in the bar with the detail on hover — because a bar started by your compositor
has no terminal to print to, and saving a broken file would otherwise look like
nothing happening.

### When something breaks

A module that fails shows a warning triangle in the `urgent` colour with the
reason on hover, and the rest of the bar carries on. A command that does not
exist, exits non-zero, never returns, or prints malformed JSON all end up there
rather than taking the bar down or leaving a hole.

### Running commands

`[[module.custom]]`, `[[module.menu]]` and `on-click-day` all run shell
commands, so **the config file is the trust boundary**: anyone who can write it
can already run code as you. Validating the commands themselves would buy
nothing, since an attacker editing the file could name an allowed path just as
easily.

What ricebar does check is whether anyone *else* can write it — the same check
`ssh` and `sudo` make of theirs. If the config, or the directory holding it, is
group- or other-writable, the bar still starts and still draws, but runs no
commands and says so:

```
ricebar: ~/.config/ricebar/config.toml is writable by other users; refusing to run commands from it
ricebar: fix with `chmod go-w ~/.config/ricebar/config.toml`
```

Values substituted into a command — currently only `{}` in `on-click-day` — are
quoted for the shell first.

## Fonts and icons

ricebar runs with no particular font installed, but most icons it draws by
default are **Nerd Font** glyphs, and without one they render as empty boxes.
That is the first thing to check if the bar comes up looking broken.

Nerd Fonts patch icon sets into a font's Private Use Area. The built-in modules
draw from three of them:

| what | set |
|---|---|
| cpu, memory, battery, mains, the broken-module triangle | Font Awesome |
| temperature, the power menu's entries | Material Design Icons |
| the backlight ramp | Weather |

These are not emoji, so an emoji font supplies none of them.

Install any Nerd Font — JetBrainsMono, FiraCode, Hack and Iosevka all have
patched builds — and name it in config:

```toml
[bar]
font = "JetBrainsMono Nerd Font"
```

The name must be the family as **fontconfig** knows it, which is not always what
the download was called:

```sh
fc-list : family | grep -i nerd            # what is installed
fc-match "JetBrainsMono Nerd Font" family  # what that name resolves to
```

A name fontconfig does not recognise is substituted silently rather than
reported, so text in the wrong face is worth checking with `fc-match` before
suspecting ricebar.

**None of it is mandatory.** Every icon is config, so a plain-text bar needs no
special font at all — and pictures need none either:

```toml
[module.cpu]
format = "cpu {value}"                # no icon
icons = ["*", "**", "***"]            # or your own, lowest level first
```

Config files here write glyphs as TOML escapes — `"\uf2db"` rather than the
character itself — so they stay plain ASCII and survive an editor with no font
for them. Either form works, and it is why the examples above are readable
without one.

## What it does not do

Worth knowing before you switch:

- **No system tray**, and none planned. It is D-Bus (StatusNotifierItem plus
  DBusMenu), and ricebar deliberately does not speak D-Bus. If you rely on
  apps that close to a tray icon, waybar or ironbar will serve you better.
- **No media controls** or notification module, for the same reason.
- **Every monitor shows the same thing.** Per-output content is planned.
- **No icon themes by name yet** — icons are paths. `icon-theme = "candy-icons"`
  is on the list.
- **Young.** Built over a few days and used daily by its author, which is not
  the same as being tested by anyone else.

[TODO.md](TODO.md) has the full list, with the reasoning behind each.

## Contributing

Issues and pull requests welcome — especially compositor backends, example
scripts, and reports from setups unlike the author's.

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

`dev/` holds nested sway and niri sessions for working on backends without
leaving your own compositor, and `dev/record/` the configs behind the
screenshot above.

## License

MIT
