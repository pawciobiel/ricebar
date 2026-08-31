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
- [x] **Keyboard layout module.** Done, as `keyboard`, behind two new methods
      on the `Compositor` trait: `layouts()` streams every layout configured
      and which is in use, and `set_layout(index)` picks one. Clicking the
      module opens a list of the layouts on a surface of its own, the way the
      clock's calendar and `[[module.menu]]` do; clicking a line switches to
      it, and the one in use is drawn in the `accent` colour.

      **The whole list every time**, rather than the active layout alone, for
      the same reason the workspaces backends publish whole snapshots: the
      popup lists what the compositor has, so it cannot be allowed to drift.

      **The two compositors do not agree on what to call a layout.** sway and
      niri use xkb's description, "Polish"; Hyprland reports the code it was
      configured with, `pl`. Rather than translate in the backends, both forms
      reach the module and `modules::keyboard::Xkb` maps between them out of
      xkb's own table, `/usr/share/X11/xkb/rules/evdev.lst`: the code upper
      cased for the bar, the description for the popup and for hover. No list
      of countries in the source, and nothing to write in config. A layout xkb
      has never heard of is shown as it came rather than dropped.

      **Per backend:**

      - Hyprland: `j/devices` for the `main` keyboard's `layout` (comma
        separated codes) and `active_layout_index`, and `activelayout>>` on the
        event socket. Switching is `switchxkblayout <device> <index>` — a
        command of its own, *not* a dispatcher: `dispatch switchxkblayout`
        answers "Invalid dispatcher". Its reply is checked, because Hyprland
        reports a refusal on the same socket as a success.
      - sway: `GET_INPUTS` (type 100) for `xkb_layout_names` and
        `xkb_active_layout_index`, subscribed to `["input"]`. It prefers a
        keyboard with more than one layout configured, because a power button
        and a row of hotkeys both report as keyboards and carry a layout each.
        Switching is `input type:keyboard xkb_switch_layout <index>`, which
        moves every keyboard rather than leaving the laptop one behind.
      - niri: `KeyboardLayoutsChanged` carries `names` and `current_idx`, and
        `KeyboardLayoutSwitched` carries an index alone, so both are kept.
        Switching is `{"Action":{"SwitchLayout":{"layout":{"Index":n}}}}`.

      **Verified against all three**, with the bar following a real switch:
      Hyprland `PL` to `US` on the live session, sway `US` to `PL` nested, niri
      `PL` to `US` nested. Every switch command was also run by hand and
      answered `ok`, `success` and `Ok:Handled`. The popup itself cannot be
      opened in a rig — a virtual pointer still cannot click, as the note
      further down says — so opening, closing and choosing are covered by a
      test in `app.rs` that sends the same messages a click would.

      **Hyprland sends `activelayout` for every device it has**, eight of them
      here for one change, and only one of those lines is the keyboard the bar
      shows. That is why the backend re-queries rather than reading the layout
      out of the event.
- [x] **Click actions for custom modules.** `on-click` runs a command and makes
      the module a button; with no `exec` and only a `label`, that is a
      launcher.
- [x] **Scroll actions.** `on-scroll-up`/`on-scroll-down` on custom modules and
      on the built-in sensors, which is what backlight and volume wanted.
- [x] **A font per module.** Done. `font` and `font-size` on the sensors and on
      custom modules, resolved once in `modules::typography` and applied in
      `app::view`, so a bar in JetBrainsMono can put one module in Material
      Symbols without moving everything. Every module draws its labels through
      `icon::faced`, which is what carries the face to the ones that do not go
      through `labelled`. Fallback is explicit rather than left to whatever
      fontconfig substitutes.
- [x] **Graphical icons.** Done, in `src/modules/icon.rs`. An entry in an
      `icons` list that looks like a path — it contains `/`, or starts with
      `~` — is loaded as a picture; svg, png and jpeg all work, and one ramp
      can mix them with glyphs. `[bar.style] icon-size` sizes them, with a
      per-module `icon-size` to override it, because a picture has no font to
      take its size from.

      Colour follows the freedesktop convention rather than a config key: an
      svg named `*-symbolic.svg` is recoloured to follow the module's `colors`,
      anything else keeps the colours it was drawn with. That is what lets a
      symbolic memory icon turn amber under load while a weather icon keeps its
      yellow sun. A png can never be recoloured.

      Scripts can name one too: `{"icon": "/path/to/weather-storm.svg"}`
      alongside the text, which outranks whatever `percentage` would have
      chosen. That is for the cases where the icon is not a level at all — a
      weather condition, a keyboard layout, whether something is connected.
      `dev/scripts/weather.py` does exactly this when `ICONS` is set.

      A few weather icons ship in `dev/icons/weather/` and are written out on
      first run, because most distributions carry no weather icons at all and
      a config whose example paths do not exist teaches nothing.
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
- [x] **microphone** — done as `dev/scripts/microphone.sh`, streaming, with a
      click to toggle mute and scroll to change the recording level. The
      tooltip names the capture device.

      This one chooses its own glyph — `U+F130`, or `U+F131` crossed out when
      muted — rather than taking one from the module's `icons`. That list is
      indexed by percentage, and muted is not a level: a microphone turned
      down to 20% would otherwise be drawn as a muted one. Both glyphs stay
      overridable, through `ON` and `OFF` in the command.
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

- [x] **Hot reload.** Done. `app::watch` polls the file's timestamp once a
      second — not inotify, which misses the editors that save by writing a new
      file and renaming it over the old one — and `app::reload` builds a whole
      new `Bar` and replaces every surface. A config that does not parse is
      refused and reported on the bar itself, since a bar started by the
      compositor has no terminal to print to. The generation counter in
      `app::subscription` is what stops the new modules inheriting the old
      streams. Only one thing is still frozen: taking the *last* `font` key out
      of a config, because the fallback family is chosen once when iced starts.
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
- [x] **Per-monitor bars.** Done, as part of *Several bars in one config*
      below. Pinning a bar to an output and running two bars from one file
      turned out to be the same change, and doing them separately would have
      meant building the `window::Id` lookup twice.

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

- [x] **Several bars in one config.** Today a bar is
      one process anchored to one edge of every output, and a second edge means
      running ricebar twice with `--config`. One `[[bar]]` array replaces both
      that and per-monitor bars, because they need the same lookup.

      **Shape.** Module *definitions* stay global and each bar names the ones it
      wants, so a bar is a placement rather than a container:

      ```toml
      [module.cpu]
      format = "{icon} {value}"

      [[module.custom]]
      name = "stocks"
      exec = "~/.config/ricebar/scripts/stocks.py"

      [[bar]]
      output = "eDP-1"                 # omit for every output
      modules-left = ["workspaces"]
      modules-right = ["cpu", "memory", "battery"]

      [[bar]]
      output = "HDMI-A-1"
      height = 32
      modules-center = ["clock"]
      modules-right = ["stocks"]
      ```

      `[[bar.row]]` nests inside a `[[bar]]` unchanged.

      **One module instance, drawn wherever it is named.** `Bar.modules` stays a
      single `Vec` and each bar holds indices into it, which `region(&[usize])`
      already expects. So `stocks` named by one bar is one script, and `clock`
      named by both is one clock drawn twice — not two processes and two timers.
      Hover state is the one thing that is per-module today and would want to
      become per `(bar, module)`, or a tooltip opens on both bars at once.

      **Compatibility.** `[bar]` as a single table keeps meaning exactly what it
      means now, so every existing config — including the one first-run
      writes — is untouched. A `#[serde(untagged)]` enum takes either form.

      **Mechanism.** `StartMode::Background` so the runtime creates nothing by
      itself, then one `Message::NewLayerShell { settings, id }` per bar with an
      id we mint, and a `HashMap<window::Id, usize>` into the bar list. The
      mapping is known by construction, which is what `AllScreens` cannot give
      us: its ids are `Id::unique()` and no inbound event carries an output
      (`iced_layershell::event::WindowEvent` is input, refresh, closed and theme
      only). `NewLayerShellSettings` carries everything a bar needs — `size`,
      `anchor`, `exclusive_zone`, `margin`, `layer`, `keyboard_interactivity`,
      `output_option` and a per-bar `namespace`, which would also make
      `hyprctl layers` name them.

      **What it unlocks beyond the feature itself.** Four of the six settings a
      reload currently refuses stop being frozen: `position`, `margin`,
      `exclusive` and the total `height` are frozen only because
      `LayerShellSettings` is baked into `Settings` once at startup, and a
      surface built at runtime can be torn down and rebuilt. Only `font` and
      `font-size` stay, being genuinely process-wide. Popups also get more
      correct: they ask for `OutputOption::Active` today and let the compositor
      choose, and with the map they can name the parent bar's output, so a menu
      lands on the monitor it was clicked on whatever has focus.

      **The cost.** Hotplug is currently free and would stop being so.
      `StartMode::AllScreens` really does follow outputs — verified with
      `hyprctl output create headless`, which adds a third bar in the running
      process, and `hyprctl output remove`, which takes it away again. Under
      `Background` an unpinned bar has to enumerate outputs itself. That is fine
      for Hyprland, sway and niri, whose IPC we already speak and whose
      `Workspace` already carries `monitor`, but under `compositor = "none"`
      there is no output list and an unpinned bar would fall back to one surface
      on the active output. Narrow, but a real regression, and it belongs in the
      docs rather than in a bug report.

      That fallback did not work at all until it was tested: `Message::Outputs`
      returned early when the list matched what the bar already held, and with
      no backend the list is empty and the bar starts empty, so the two matched
      on the very first message and no surface was ever built. A bar with
      `compositor = "none"` — or on any compositor ricebar does not speak —
      simply never appeared. `Bar::told` now says whether the list has ever
      arrived, which is a different question from what it holds.

      **The trap to design around.** An `OutputOption::OutputName` that matches
      nothing resolves to `None`, and `None` means "compositor chooses"
      (`layershellev` lib.rs:2618). A typo, or an output that is unplugged at
      login, therefore puts that bar silently on top of another one rather than
      failing. Unlike `StartMode::TargetScreen`, which has the same fallback but
      resolves inside the runner, this happens at our own call site — so check
      the name against the compositor's output list first and raise the same
      in-bar notice a config parse error uses.

      **Rough size.** Config parsing with the compatibility shim ~80 lines, the
      surface map and per-bar `view` ~120, moving geometry out of `Settings`
      ~40, rebuilding surfaces on reload ~60, output enumeration ~30 per
      backend, plus `config.example.toml` and the README.

      **Written and verified**, on a headless sway rig with two outputs of
      different sizes — see the recipe in `CLAUDE.md`. Measured by decoding the
      screenshots rather than by eye:

      - `[bar]` and `[[bar]]` both parse; the single-table form is unchanged.
      - A 1280x720 monitor with a 30px top bar and a 1920x1080 one with a 48px
        bottom bar, from one config in one process.
      - Plugging a monitor in gives it a bar (`#1e1e2e` fill and `#cdd6f4` text
        in its top rows); unplugging takes it away.
      - Editing `position` and `height` while running moves the bar from top to
        bottom and resizes it: the top rows go back to wallpaper, the bottom 44
        become bar. Turning one `[bar]` into two `[[bar]]` works the same way.
      - A bar naming a monitor that is not there is refused, and the warning
        triangle appears on the bar that *could* be placed.

      **Do not test this against a live desktop session.** Presentation is
      gated on `wl_surface.frame` callbacks, and a compositor whose output is
      asleep or locked stops sending them: a surface created during that window
      never gets its first callback and never paints, while surfaces that
      already had content keep showing it. That looks exactly like a bug in this
      feature, and cost most of a session before the screen turning itself off
      was noticed as the cause.
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
      run, which means `cargo install --git` carries them with no data
      directory for a package to own and no install step to get wrong. They are
      written only if absent, so editing one and deleting the config to start
      again keeps the edits.

      The tension was a fresh install that shows the bar's range without
      filling it with warning triangles, since each script needs something not
      everyone has:

      | script             | needs                                  |
      |--------------------|----------------------------------------|
      | `volume.sh`        | `pactl` (PipeWire or PulseAudio)       |
      | `microphone.sh`    | `pactl` (PipeWire or PulseAudio)       |
      | `network.sh`       | `ip`, `iw`                             |
      | `ticker.sh`        | `top`, `df`, `free`                    |
      | `weather.py`       | `python3`, and the network             |
      | `stocks.py`        | `python3`, and the network             |
      | `windows-popup.py` | `python3`, and a compositor CLI        |

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

- [x] **Not publishing to crates.io.** Decided against. Publishing there means
      signing in through GitHub — there is no other way in — and granting that
      login access the account does not need. `cargo install --git` installs
      ricebar without an account of any kind, which suits a bar whose whole
      dependency story is "one binary and some scripts".

      Nothing in the crate metadata is wasted if that changes: `Cargo.toml`
      keeps its description, keywords and `exclude`, and `publish = false`
      stops an accidental `cargo publish` rather than a deliberate one.
- [x] **`CLAUDE.md`** with build, lint and test commands, the traps already hit,
      and how to test against a live or nested session.

## Failure behaviour

A module that breaks shows `U+F071`, a warning triangle, in the `urgent`
colour, with the reason on hover. Verified against a command that does not
exist, one exiting non-zero, one that never returns, one printing malformed
JSON, and a stream that dies without printing: all five mark themselves and
none of them stops the bar or its other modules.

- [ ] **A module that has gone stale.** A streaming script that stops printing
      looks identical to one with nothing to say. Worth marking a module whose
      last update is far older than its interval.

- [ ] **A streaming script that has never printed draws nothing at all**, which
      is what made a blank volume module look like a missing icon rather than a
      broken script. The scripts now always print something, but the bar has no
      opinion of its own: a module named in a `modules-*` list and drawing an
      empty row is indistinguishable from one that is not there. Something as
      small as the module's name in `dim` until its first line would say which.

- [ ] **`kill_on_drop` reaches the script but not what the script started.**
      A reload drops the subscription and kills `sh -c <script>`, but a script
      whose last line is a pipeline — `pactl subscribe | while read ...`, which
      is both shipped streaming scripts — leaves the `while` subshell and the
      `pactl` behind, reparented to init. They are not permanent: the subshell
      dies of `SIGPIPE` on its next write, and `pactl` on the one after. But on
      a quiet machine that can be a long time, and every reload adds a pair.
      Measured: after one reload, `3542 volume.sh` and `3541 pactl subscribe`
      were still there with the new pair already running beside them.

      The fix is `process_group(0)` on the child — that much is in std — and
      signalling the whole group when the future is dropped, which needs
      `libc::kill(-pgid, ...)`. `libc` is already in the tree under tokio, so
      it costs a direct dependency and no compile time.

## Last, and least urgent

- [ ] **Icon themes by name.** `icon-theme = "candy-icons"` with
      `icons = ["cpu-symbolic"]`, resolved through the freedesktop icon theme
      spec instead of an absolute path: `~/.local/share/icons`,
      `/usr/share/icons`, `$XDG_DATA_DIRS`, size directories, and `index.theme`
      inheritance.

      Nicer to write than a path, and it follows the user's theme when they
      change it — but it is the whole spec, and worth pulling in
      `freedesktop-icons` rather than implementing. Deliberately last: paths
      already cover what people actually ask for, and a script that knows its
      own icons can name them itself.

## Known constraints (not bugs)

- Tooltips are layer surfaces, never xdg popups. `iced_layershell` builds
  popups *with a grab* (`multi_window.rs` takes a popup grab serial), which
  starves the bar of every pointer event for as long as the popup lives.
- `to_layer_message` appends its own variants to `Message`, so `update` must
  end with a catch-all. Upstream's example uses `unreachable!()`; that panics.
- `application()` asserts against `StartMode::AllScreens`. Multi-output
  requires `daemon()`.
