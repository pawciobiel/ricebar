# TODO

Ordered roughly by value. Notes record what was already established, so the
work does not have to be rediscovered.

## Compositor backends

- [ ] **sway backend.** `swayipc` 4.0.0 is stable and mature (4.7M downloads),
      unlike Hyprland's wrapper, so use the crate rather than hand-rolling.
      Implement `Compositor` in `src/compositor/sway.rs` and add a branch to
      `compositor::detect` — the bar core needs no changes. sway is already
      installed here, so it can be tested in a nested session.
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
