# TODO

Ordered roughly by value. Notes record what was already established, so the
work does not have to be rediscovered.

## Compositor backends

- [x] **sway backend.** Done, in `src/compositor/sway.rs`. Speaks i3-ipc
      directly over tokio rather than using `swayipc-async`, which is built on
      `async-io` and would have meant a second async runtime beside tokio.
      Window counts come from `GET_TREE`, since `GET_WORKSPACES` does not
      carry them.

      Develop and test it **nested**, without leaving your own session
      (verified on Hyprland 0.54.3 with sway 1.12):

      ```sh
      WLR_BACKENDS=wayland sway --config dev/sway-nested.conf &
      SOCK=$(ls /run/user/$UID/sway-ipc.$UID.*.sock | head -1)
      WAYLAND_DISPLAY=wayland-2 SWAYSOCK=$SOCK ./target/debug/ricebar
      ```

      sway opens as an ordinary window with its own `WAYLAND_DISPLAY`, and
      ricebar runs inside it as a layer-shell client. `$mod+Shift+e` quits it.
      The `Failed to start Xwayland` error is harmless.

      The host's environment leaks in, so inside that session both
      `HYPRLAND_INSTANCE_SIGNATURE` and `SWAYSOCK` are set and `auto` detection
      is genuinely ambiguous. `compositor` in `[module.workspaces]` settles it;
      `auto` tries sway first, that nesting direction being the common one.

- [x] **niri backend.** Done, in `src/compositor/niri.rs`. A line of JSON in,
      a line of JSON out. Unlike the other two it keeps state rather than
      re-querying: niri sends the whole state when the stream opens and deltas
      after, which its documentation calls the way to avoid drifting out of
      sync. Window counts come from `WindowsChanged` /
      `WindowOpenedOrChanged` / `WindowClosed`, since workspaces do not carry
      one. Focus is by workspace `id`, not `idx` — an index only identifies a
      workspace within one output.

      Nests the same way sway does:

      ```sh
      niri --config dev/niri-nested.kdl &
      SOCK=$(ls /run/user/$UID/niri.wayland-*.sock | head -1)
      WAYLAND_DISPLAY=wayland-2 NIRI_SOCKET=$SOCK ./target/debug/ricebar
      ```

      niri keeps an empty workspace at the end of each output, which shows up
      dimmed. Being a scrolling compositor changes nothing for the bar: layer
      surfaces are anchored to the output, and only windows scroll. Its
      Overview does matter — background and bottom layers zoom out with the
      workspaces while top and overlay stay above, so a bar wants `Layer::Top`,
      which is what ricebar already uses.

## Modules

- [x] **Built-in cpu, memory, temperature, battery and backlight.** One
      `sensor` module with five sources, since they differ only in where the
      number comes from. `format` takes `{icon}` and `{value}`, and each takes
      its own `background`/`foreground`. Still to do: **volume** and
      **network**, which need PulseAudio/D-Bus rather than a sysfs read.
- [ ] **Keyboard layout module.** Needs compositor support: Hyprland reports it
      over `.socket2.sock` (`activelayout>>`), so it likely belongs behind the
      `Compositor` trait rather than in a module reading the compositor itself.
- [x] **Click actions for custom modules.** `on-click` runs a command and makes
      the module a button; with no `exec` and only a `label`, that is a
      launcher.
- [x] **Scroll actions.** `on-scroll-up`/`on-scroll-down` on custom modules and
      on the built-in sensors, which is what backlight and volume wanted.
- [ ] **Graphical icons, and icon themes.** Today a module's icon is a glyph
      from a font, which limits it to what a Nerd Font carries and to one
      colour. Two steps:

      1. An image path: `icon = "/usr/share/icons/.../firefox.svg"`. iced can
         draw both, but its `image` and `svg` widgets are behind cargo
         features that are not enabled yet, and SVG brings `resvg` in.
      2. A named lookup against the freedesktop icon theme spec, so
         `icon-theme = "candy-icons"` plus `icon = "firefox"` resolves through
         `~/.local/share/icons`, `/usr/share/icons`, size directories and
         `index.theme` inheritance. Worth using an existing crate rather than
         implementing the spec.

      Scripts should be able to name one too, so a streaming script can return
      `{"icon": "audio-volume-high"}` alongside its text.
- [x] **`format` templating.** `{icon}` and `{value}`, on both custom modules
      and the built-in sensors.
- [x] **Streaming scripts.** `stream = true` keeps `exec` running and reads a
      line per update, so a module can block on `pactl subscribe` and print the
      moment something happens rather than polling. `percentage` in a script's
      JSON picks its glyph from `icons` in config, so the script reports a
      number and the look stays configurable. See `dev/scripts/volume.sh`.
- [ ] **Signal refresh.** `pkill -RTMIN+8 ricebar` to make a module re-read on
      demand, as waybar has, for scripts that cannot stream but should update
      after a keybind rather than on the next tick.

## waybar parity

One item per module in the reference waybar config, with the icons it uses.
Codepoints rather than glyphs, since these live in Unicode private-use planes
and do not survive copy-paste reliably. All are Nerd Font glyphs unless noted.

Anything here can be approximated today with `[[module.custom]]` plus a script;
a built-in earns its place by removing the shell round trip, or by needing
state a script cannot see.

- [x] **cpu** — done, from `/proc/stat` deltas.
- [x] **temperature** — done, preferring hwmon `k10temp`/`coretemp` over the
      ACPI zone. Outstanding: a critical threshold that recolours.
- [x] **battery** — done. Shows a plug on mains and the level glyph on
      battery, with the percentage either way. Outstanding: warning and
      critical states.
- [x] **backlight** — done, nine icons by brightness, and scrolling over it
      changes the brightness.
- [x] **pulseaudio** — done as `dev/scripts/volume.sh`, streaming, with scroll
      to change and a click to `pavucontrol`.
- [ ] *(original note)* pulseaudio — `U+F026` `U+F027` `U+F028` by volume, `U+F131` muted,
      plus per-device icons (headphone `U+F025`, headset `U+F590`,
      phone `U+F095`, portable `U+F10B`, car `U+F1B9`). Scroll to change,
      click opens `pavucontrol`.
- [x] **network** — done as `dev/scripts/network.sh`, streaming. Follows
      whatever holds the default route, so docker and bridge interfaces are
      ignored without naming them, and uses `ip`/`iw` rather than `nmcli`,
      which is not installed everywhere NetworkManager is.
- [ ] **power-profiles-daemon** — `U+F0E7` performance, `U+F24E` balanced,
      `U+F06C` power-saver. D-Bus.
- [ ] **idle_inhibitor** — `U+F06E` active, `U+F070` inactive. Toggle on click.
- [ ] **keyboard-state** — `U+F023` locked, `U+F09C` unlocked, for numlock and
      capslock.
- [ ] **tray** — StatusNotifierItem host. The largest item here by far: it is a
      D-Bus service plus icon rendering, not a readout.
- [ ] **window title** — active window title, from the `Compositor` trait.
- [ ] **submap** — Hyprland submap name, already script-shaped
      (`hyprland-submap.sh`); needs `activesubmap>>` from the event socket.
- [ ] **media** — `U+F1BC` player-specific icons, scrolling title, playerctl or
      MPRIS over D-Bus.
- [x] **clock calendar** — done, and deliberately not as a tooltip: clicking
      opens a real surface with week numbers, today marked, month arrows and a
      configurable `on-click-day`. Hover keeps its own separate tooltip.
      Outstanding: the calendar draws at a fixed 13px rather than following
      `font-size`, because its surface is sized before anything is drawn.
- [ ] **separator** (`" | "`) and **static launcher icons** such as
      `U+2328` key bindings and `U+1F5A5` monitor toggle — both are a label
      plus `on-click`, so they fall out of click actions for custom modules.

## Configuration

- [ ] **Hot reload.** Watch the config file and rebuild the modules in place.
      The registry in `app::Bar::new` already builds everything from `Config`,
      so this is mostly a matter of swapping state and re-running `region()`.
- [ ] **Explicit font files.** `Settings.fonts` takes `Vec<Cow<'static, [u8]>>`,
      so a `font-files` key could load a font ricebar ships or the user points
      at, for machines where the family is not installed. Today an unknown
      family is silently substituted by fontconfig.
- [x] **`--config <path>` flag**, plus `--help` and `--version`.

## Rendering

- [x] **More than one row.** `[[bar.row]]` stacks lines, each with its own
      regions and height; the reserved space grows with the total. A second row
      is where anything too wide for a status bar goes — a ticker, now
      playing, a headline feed.
- [x] **Ticker scrolling.** `scroll-width` shows a window that many characters
      wide and moves the rest past it, wrapping through a separator. Stepped by
      character rather than pixel: text cannot be measured outside a renderer,
      and `iced_runtime`'s `scroll_to` is not re-exported through `iced`, so a
      pixel-smooth version means either a new dependency or a custom widget.
      Looks best in a monospace font.

- [ ] **Measure tooltip text properly.** The surface is sized from a glyph-count
      estimate scaled by font size, because text cannot be measured outside a
      renderer and `window::resize_events()` never fires for layer surfaces.
      Deliberately over-wide: too narrow wraps and clips. A real fix probably
      needs `iced_graphics`' text measurement.
- [ ] **Per-monitor bars.** *Nice to have* — every output currently shows the
      same content, which matches waybar. `StartMode::AllScreens` exposes no
      output identity: `WindowEvent` carries none and ids are `Id::unique()`.
      The way round it is `StartMode::Background` plus
      `Message::NewLayerShell { settings: NewLayerShellSettings {
      output_option: OutputOption::OutputName(name), .. }, id }` with an id we
      mint, keeping a `HashMap<window::Id, String>` of monitor names.
      `view(bar, id)` then filters by monitor. Also means handling
      `monitoradded`/`monitorremoved` ourselves.

## Untrusted input

Scripts, environment variables and file contents all reach the bar without
being checked for size or shape. None of it is a privilege boundary — the
config is already trusted to run commands — but a script that misbehaves, or
a filename that is not what anyone expected, should not be able to wedge the
bar or hand it an absurd surface to draw.

- [ ] **Bound the popup surface.** `popup_settings` clamps width to 48..720
      but only does `height.max(1.0)`, so height has no upper limit. A tooltip
      is sized from its number of lines, which means a script printing a few
      thousand lines asks the compositor for a surface taller than the screen.
      Clamp height the way width already is, and clamp the line count first.
- [ ] **Bound what a script may print.** `execute` reads all of stdout into
      memory with no limit, and `follow` accepts any line length. A runaway
      command can grow the bar's memory without bound. Take a fixed number of
      bytes per line and per run, and say the output was truncated.
- [ ] **Bound the number of popup entries.** A `popup` script returning ten
      thousand items becomes ten thousand buttons on one surface. Cap the list
      and note how many were dropped.
- [ ] **Strip control characters from anything drawn.** Script output, module
      names and config strings all reach `text()` unchecked. Newlines in a bar
      label break the row's layout, and ANSI escapes and other C0 characters
      have no business being rendered. Keep newlines only where they are
      meaningful, which is tooltips.
- [ ] **Sanity-check config and environment strings.** The font family is
      leaked deliberately, so a very long one leaks that much for the life of
      the process. Paths from `XDG_CONFIG_HOME`, `SWAYSOCK`, `NIRI_SOCKET` and
      `HYPRLAND_INSTANCE_SIGNATURE` are joined into paths without a length or
      content check.
- [ ] **Decide what a module's name may contain.** It is used as a key in
      config and shown in the bar; today it can be anything, including an
      empty string or something that looks like another module.

## Configuration, continued

- [ ] **Several bars in one config.** Today a bar is one process anchored to
      one edge, and a second edge means running ricebar twice with `--config`.
      A `[[bar]]` array would let one process own several, each with its own
      edge, height and rows. The runtime can already hold more than one
      surface — that is how `StartMode::AllScreens` works, and
      `Message::NewLayerShell` mints them on demand — so this is a question of
      config shape and of `view` knowing which bar a `window::Id` belongs to,
      the same lookup per-monitor bars need.
- [x] **Write a default config on first run.** Done, in
      `src/config/first_run.rs`. A missing config at the default location is
      taken as a first run: the directory is created, `config.default.toml` is
      written with the example scripts beside it, and both paths are reported.
      A path named with `--config` is still an error rather than a first run,
      an existing file is never overwritten, and a directory that cannot be
      created falls back to built-in defaults with the bar still coming up.

## Packaging

- [x] **Ship the example scripts, and start with them.** Done. The scripts are
      `include_str!`-ed into the binary and written next to the config on first
      run, which means `cargo install ricebar` carries them with no data
      directory for a package to own and no install step to get wrong. They are
      written only if absent, so editing one and deleting the config to start
      again keeps the edits.

      The tension was a fresh install that shows the bar's range without
      filling it with warning triangles, since each script needs something not
      everyone has:

      | script             | needs                                  |
      |--------------------|----------------------------------------|
      | `volume.sh`        | `pactl` (PipeWire or PulseAudio)       |
      | `network.sh`       | `ip`, `iw`                             |
      | `weather.sh`       | `curl`, and the network                |
      | `ticker.sh`        | `top`, `df`, `free`                    |
      | `stocks.sh`        | `curl`, `python3`                      |
      | `windows-popup.sh` | `python3`, and a compositor CLI        |

      Settled by writing the `modules-*` lists from what the machine has:
      `PATH` for programs, an actual sensor read for hardware, so a desktop
      gets no battery module. Anything unusable is written commented out with
      the reason beside it — `# "volume",  # needs pactl` — which answers the
      objection that first-run detection bakes in whatever was installed that
      day. Installing the missing piece leaves a line saying what to uncomment,
      rather than a module that silently never appeared.

      Not everything is enabled even where it works: the ticker and the markets
      want a second row, and the power menu really does power the machine off,
      so those ship commented with an explanation instead.

- [ ] **Publish to crates.io.** The name is free. Needs a real README badge set,
      a licence header check and a `cargo publish --dry-run`.
- [ ] **`CLAUDE.md`** with build, lint and test commands for future sessions.

## Failure behaviour

A module that breaks shows `U+F071`, a warning triangle, in the `urgent`
colour, with the reason on hover. Verified against a command that does not
exist, one exiting non-zero, one that never returns, one printing malformed
JSON, and a stream that dies without printing: all five mark themselves and
none of them stops the bar or its other modules.

- [ ] **A module that has gone stale.** A streaming script that stops printing
      looks identical to one with nothing to say. Worth marking a module whose
      last update is far older than its interval.

## Known constraints (not bugs)

- Tooltips are layer surfaces, never xdg popups. `iced_layershell` builds
  popups *with a grab* (`multi_window.rs` takes a popup grab serial), which
  starves the bar of every pointer event for as long as the popup lives.
- `to_layer_message` appends its own variants to `Message`, so `update` must
  end with a catch-all. Upstream's example uses `unreachable!()`; that panics.
- `application()` asserts against `StartMode::AllScreens`. Multi-output
  requires `daemon()`.
