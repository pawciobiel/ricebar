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

- [ ] **niri backend.** `niri-ipc` 26.4.0. Its event stream is the odd one out:
      it sends complete state up front and then deltas, with
      `EventStreamStatePart::apply` maintaining the state for you. Our
      `Workspaces` type is already a whole-snapshot model, so it should map
      cleanly. Not installed here — needs installing to test.

## Modules

- [ ] **Built-in battery, volume, network and temperature modules.** All four
      are possible today as `[[module.custom]]` scripts; built-ins would give
      them icons, thresholds and click actions without a shell round trip.
      Battery and temperature can read sysfs directly.
- [ ] **Keyboard layout module.** Needs compositor support: Hyprland reports it
      over `.socket2.sock` (`activelayout>>`), so it likely belongs behind the
      `Compositor` trait rather than in a module reading the compositor itself.
- [ ] **Click and scroll actions for custom modules.** `on-click`, `on-scroll-up`
      and similar, as waybar has. The workspaces module already proves the
      click path end to end.
- [ ] **`format` templating** so a module's output can be wrapped without the
      script having to emit the final string, e.g. `format = "  {}"`.

## waybar parity

One item per module in the reference waybar config, with the icons it uses.
Codepoints rather than glyphs, since these live in Unicode private-use planes
and do not survive copy-paste reliably. All are Nerd Font glyphs unless noted.

Anything here can be approximated today with `[[module.custom]]` plus a script;
a built-in earns its place by removing the shell round trip, or by needing
state a script cannot see.

- [ ] **cpu** — `U+F2DB`. Icon in the bar, usage percent in the tooltip.
- [ ] **temperature** — `U+F76B` `U+F2C9` `U+F769` chosen by level, with a
      critical threshold that recolours. Reads sysfs/hwmon.
- [ ] **battery** — `U+F244`…`U+F240` by charge, `U+F1E6` plugged,
      `U+F5E7` charging. Needs warning and critical states.
- [ ] **backlight** — nine icons `U+E38D` `U+E39B` `U+E3C8` `U+E3CA` `U+E3CD`
      `U+E3CE` `U+E3CF` `U+E3D1` `U+E3D3` by brightness. Scroll to change.
- [ ] **pulseaudio** — `U+F026` `U+F027` `U+F028` by volume, `U+F131` muted,
      plus per-device icons (headphone `U+F025`, headset `U+F590`,
      phone `U+F095`, portable `U+F10B`, car `U+F1B9`). Scroll to change,
      click opens `pavucontrol`.
- [ ] **network** — `U+F1EB` wifi with essid and signal, `U+F796` ethernet,
      `U+26A0` disconnected. Tooltip shows interface, address and gateway.
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
- [ ] **`--config <path>` flag** for testing and for running two bars with
      different configs.

## Rendering

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

## Packaging

- [ ] **Publish to crates.io.** The name is free. Needs a real README badge set,
      a licence header check and a `cargo publish --dry-run`.
- [ ] **`CLAUDE.md`** with build, lint and test commands for future sessions.

## Known constraints (not bugs)

- Tooltips are layer surfaces, never xdg popups. `iced_layershell` builds
  popups *with a grab* (`multi_window.rs` takes a popup grab serial), which
  starves the bar of every pointer event for as long as the popup lives.
- `to_layer_message` appends its own variants to `Message`, so `update` must
  end with a catch-all. Upstream's example uses `unreachable!()`; that panics.
- `application()` asserts against `StartMode::AllScreens`. Multi-output
  requires `daemon()`.
