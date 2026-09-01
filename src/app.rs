use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use iced::advanced::widget;
use iced::futures::{SinkExt, Stream};
use iced::widget::{column, container, mouse_area, row, space, text};
use iced::{Alignment, Border, Color, Element, Length, Rectangle, Subscription, Task, window};
use iced_layershell::reexport::{
    Anchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption,
};
use iced_layershell::to_layer_message;

use crate::compositor;
use crate::config;
use crate::modules::{self, Direction, Event, Module};

/// Everything the bar knows. `update` is the only thing allowed to change it.
pub struct Bar {
    config: config::Config,
    /// Every enabled module, in one flat list so a single index identifies one.
    /// Shared across bars: a module named by two of them is built once and
    /// drawn twice, so one script feeds both rather than running twice.
    modules: Vec<Box<dyn Module>>,
    /// One entry per `[[bar]]` in the config.
    layouts: Vec<Layout>,
    /// What each module asked to be drawn in, parallel to `modules`. Kept here
    /// rather than in the module because resolving it needs the bar as well.
    typography: Vec<(Option<iced::Font>, Option<f32>)>,
    /// Which layout each live surface is drawing, and where. An id that is not
    /// in here belongs to nothing and is drawn empty.
    surfaces: HashMap<window::Id, Surface>,
    /// The monitors the compositor last said it had. Empty means nobody could
    /// say, and each bar is then placed wherever the compositor likes.
    outputs: Vec<String>,
    /// Whether that list has ever arrived. An empty list is an answer -- it is
    /// the answer with no compositor backend -- and it looks exactly like the
    /// list we start with, so without this the surfaces are never built.
    told: bool,
    /// The hover popup, when one is open.
    popup: Option<Popup>,
    /// Which module was last located, and where. Kept because a script's popup
    /// arrives after the click that asked for it, and both need the same
    /// placement.
    placement: Option<(usize, Placement)>,
    /// The module the pointer is currently inside.
    hovered: Option<usize>,
    /// Scroll accumulated per module. A wheel reports whole notches, but a
    /// touchpad reports a stream of small ones, and acting on each would run
    /// the command dozens of times for a single flick.
    scrolled: HashMap<usize, f32>,
    /// A popup we have asked to remove. Its surface may still ask to be drawn
    /// once more, and it must not fall through to rendering the whole bar.
    retiring: Option<window::Id>,
    /// Where the warning about the config sits in `modules`, once there is one.
    notice: Option<usize>,
    /// When the pointer last entered a module. A leave arriving during a
    /// popup's opening grace is deferred rather than dropped, and this is how
    /// the deferred leave tells a real departure from the echo of the popup
    /// appearing: if the pointer came back, it did so after the leave.
    entered: Instant,
    /// How many times the config has been reloaded. Part of every
    /// subscription's identity, so a reload restarts the streams behind the
    /// new modules rather than leaving them attached to the old ones.
    generation: u64,
}

/// One `[[bar]]`: its own geometry, style and lines.
struct Layout {
    config: config::Bar,
    rows: Vec<Row>,
    /// The bar's own face, which its modules inherit unless they name one.
    font: Option<iced::Font>,
}

/// One line of a bar, holding indices into `modules`.
struct Row {
    height: u32,
    left: Vec<usize>,
    center: Vec<usize>,
    right: Vec<usize>,
}

/// A live layer surface: the layout it draws, and the monitor it is on.
struct Surface {
    layout: usize,
    /// `None` when the compositor chose the monitor rather than us.
    output: Option<String>,
}

struct Popup {
    /// Its own layer-shell surface, so it can extend past the bar's height.
    id: window::Id,
    /// The module it belongs to.
    module: usize,
    kind: PopupKind,
    opened: Instant,
}

/// Where a module sits on its bar, in that surface's own coordinates.
#[derive(Debug, Clone, Copy)]
pub struct Placement {
    /// The middle of the module, which is what the popup is centred under.
    centre: f32,
    /// The bar's own width, which is the room the popup has to stay inside.
    width: f32,
}

/// What a placement answer is for. One operation serves both, since a tooltip
/// and a menu want to sit in exactly the same place.
#[derive(Debug, Clone, Copy)]
pub enum Opening {
    /// Hover text.
    Tooltip,
    /// The module's own popup, opened by a click.
    Popup,
}

enum PopupKind {
    /// Hover text. Takes no pointer input, so the bar keeps receiving it. The
    /// text is not kept here: it is read from the module as it is drawn, so a
    /// tooltip showing a value follows that value while it is open.
    Tooltip,
    /// Whatever the module draws for itself: a menu, a calendar. Takes pointer
    /// input, since it exists to be clicked.
    Module,
}

/// A new surface briefly disturbs the pointer state of the one below it, which
/// would close the tooltip the instant it appears. Leaves this soon after
/// opening are that echo rather than the user moving away.
const POPUP_GRACE: Duration = Duration::from_millis(120);

impl Bar {
    pub fn new(config: config::Config) -> Self {
        let mut modules = Vec::new();
        let mut typography = Vec::new();
        // Names already built, so a module two bars both ask for is one module
        // drawn in two places rather than two scripts doing the same work.
        let mut built = HashMap::new();

        let trusted = config.trusted;

        let layouts: Vec<Layout> = config
            .bars
            .iter()
            .map(|bar| {
                let mut line = |names: &[String]| {
                    region(
                        &mut modules,
                        &mut typography,
                        &mut built,
                        names,
                        &config.module,
                        trusted,
                    )
                };

                Layout {
                    font: config::typeface(bar.font.as_deref()),
                    rows: bar
                        .rows()
                        .into_iter()
                        .map(|row| Row {
                            height: row.height.unwrap_or(bar.height),
                            left: line(&row.modules_left),
                            center: line(&row.modules_center),
                            right: line(&row.modules_right),
                        })
                        .collect(),
                    config: bar.clone(),
                }
            })
            .collect();

        if modules.is_empty() {
            eprintln!("ricebar: no modules enabled; check the modules-* lists in config");
        } else {
            let names: Vec<&str> = modules.iter().map(|module| module.name()).collect();
            eprintln!(
                "ricebar: {} bar(s), {} module(s): {}",
                layouts.len(),
                names.len(),
                names.join(", ")
            );
        }

        let mut bar = Self {
            config,
            modules,
            typography,
            layouts,
            surfaces: HashMap::new(),
            outputs: Vec::new(),
            told: false,
            popup: None,
            placement: None,
            hovered: None,
            scrolled: HashMap::new(),
            retiring: None,
            notice: None,
            entered: Instant::now(),
            generation: 0,
        };

        // A config that could not be read is why this bar looks like the
        // defaults, and that is worth saying where it will be seen.
        if let Some(problem) = bar.config.problem.clone() {
            bar.warn(problem);
        }

        bar
    }

    /// Show a warning about the bar's own configuration, in the bar.
    ///
    /// The detail goes to stderr as well, but a bar started by the compositor
    /// has no terminal attached: without this, a mistake in the config is
    /// invisible and saving the file simply appears to do nothing.
    fn warn(&mut self, detail: String) {
        let notice: Box<dyn Module> = Box::new(modules::notice::Notice::new(detail));

        match self.notice.and_then(|index| self.modules.get_mut(index)) {
            Some(existing) => *existing = notice,
            None => {
                let index = self.modules.len();
                self.modules.push(notice);
                self.notice = Some(index);

                // On every bar, at the end of its first line: the config is
                // wrong for the process, not for one surface, and putting it
                // on only the first would hide it whenever that bar is the one
                // that could not be placed.
                for layout in &mut self.layouts {
                    if let Some(first) = layout.rows.first_mut() {
                        first.right.push(index);
                    }
                }
            }
        }
    }
}

/// Build one region's modules, returning their indices into the flat list.
///
/// A name already built is reused rather than built again, so naming `stocks`
/// on two bars runs one script and draws its output in both places.
fn region(
    modules: &mut Vec<Box<dyn Module>>,
    typography: &mut Vec<(Option<iced::Font>, Option<f32>)>,
    built: &mut HashMap<String, usize>,
    names: &[String],
    config: &config::Modules,
    trusted: bool,
) -> Vec<usize> {
    let mut indices = Vec::new();

    for name in names {
        if let Some(&index) = built.get(name) {
            indices.push(index);
            continue;
        }

        match modules::build(name, config, trusted) {
            Some(module) => {
                let index = modules.len();
                let (font, size) = modules::typography(name, config);
                modules.push(module);
                typography.push((config::typeface(font.as_deref()), size));
                built.insert(name.clone(), index);
                indices.push(index);
            }
            None => eprintln!("ricebar: unknown module `{name}`, ignoring"),
        }
    }

    indices
}

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
pub enum Message {
    /// An event for the module at this index.
    Module(usize, Event),
    /// The pointer entered the module at this index.
    Enter(usize),
    /// The pointer left the module at this index.
    Leave(usize),
    /// A leave that arrived while a popup was still settling, asked again once
    /// the grace had passed. Carries when it was first seen.
    Left(usize, Instant),
    /// The pointer scrolled over the module at this index, by however much it
    /// reports at a time.
    Scroll(usize, iced::mouse::ScrollDelta),
    /// The config file was written to.
    ConfigChanged,
    /// The monitors the compositor has, as it last reported them.
    Outputs(Vec<String>),
    /// Build and remove surfaces so they match the monitors and the config.
    Place,
    /// Open this module's popup, under `at` when the widget tree said where the
    /// module is. A timer sends `None` shortly after the click, so a popup still
    /// opens at the edge of the bar if that answer never comes.
    PopupUnder {
        module: usize,
        at: Option<Placement>,
        opening: Opening,
    },
}

/// How long to let a monitor settle before building a bar on it. Long enough
/// for a `wl_output` to arrive after the compositor announced the monitor,
/// short enough not to be seen.
const SETTLE: Duration = Duration::from_millis(400);

/// How far a touchpad must travel to count as one notch of a wheel.
const PIXELS_PER_NOTCH: f32 = 40.0;
/// The most notches a flick may bank, so letting go of a fast scroll does not
/// keep firing the command afterwards.
const NOTCH_LIMIT: f32 = 2.0;

/// A scroll in notches, whichever units the pointer reports in.
fn notches(delta: iced::mouse::ScrollDelta) -> f32 {
    match delta {
        iced::mouse::ScrollDelta::Lines { y, .. } => y,
        iced::mouse::ScrollDelta::Pixels { y, .. } => y / PIXELS_PER_NOTCH,
    }
}

pub fn namespace() -> String {
    String::from("ricebar")
}

pub fn subscription(bar: &Bar) -> Subscription<Message> {
    let generation = bar.generation;

    let modules = Subscription::batch(bar.modules.iter().enumerate().map(|(index, module)| {
        // `with` folds the index into the subscription's identity. Without it
        // iced would hash only the *type* of the closure below, so two modules
        // sharing an inner recipe would collapse into a single stream and one
        // of them would never receive an event.
        //
        // The generation is in there for the same reason, one level up. A
        // reload builds new modules, but iced recognises a subscription it is
        // already running and keeps the old stream -- which has already sent
        // its opening state, so the new module would sit empty waiting for a
        // change. Workspaces stayed blank; a weather script would have been
        // blank until its next print, a quarter of an hour later.
        module
            .subscription()
            .with((generation, index))
            .map(|((_, index), event)| Message::Module(index, event))
    }));

    // Bars are placed on monitors from this, so it runs whether or not a
    // workspaces module asked for a compositor.
    let outputs = compositor::outputs(bar.config.module.workspaces.compositor)
        .with(generation)
        .map(|(_, outputs)| Message::Outputs(outputs));

    let Some(path) = bar.config.path.clone() else {
        return Subscription::batch([modules, outputs]);
    };

    Subscription::batch([
        modules,
        outputs,
        Subscription::run_with(path, watch).map(|()| Message::ConfigChanged),
    ])
}

/// How often the config is checked for changes. A `stat` costs almost nothing,
/// and a second is soon enough to feel immediate after saving the file.
const POLL: Duration = Duration::from_secs(1);

/// Emit once each time the config file changes.
///
/// Polling rather than inotify: it needs no dependency, and it copes with the
/// editors that save by writing a new file and renaming it over the old one,
/// which a watch on the original inode would miss entirely.
// `&Path` would be the idiomatic argument, but the subscription's key type is
// what fixes this signature: `run_with` wants `Fn(&PathBuf)`, and a function
// taking `&Path` does not coerce to it.
#[allow(clippy::ptr_arg)]
fn watch(path: &PathBuf) -> impl Stream<Item = ()> + use<> {
    let path = path.clone();

    iced::stream::channel(1, async move |mut output| {
        let mut seen = stamp(&path);

        loop {
            tokio::time::sleep(POLL).await;

            let found = stamp(&path);
            if found == seen {
                continue;
            }
            seen = found;

            // A file that has just vanished is usually an editor partway
            // through replacing it, and the write that follows is the change
            // worth acting on.
            if found.is_none() {
                continue;
            }

            if output.send(()).await.is_err() {
                return;
            }
        }
    })
}

/// What identifies a version of the file: when it was written, and how long it
/// is. Cheaper than reading it, and enough to notice a save.
fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let found = std::fs::metadata(path).ok()?;
    Some((found.modified().ok()?, found.len()))
}

/// The pointer really has left the module at this index.
fn leave(bar: &mut Bar, index: usize) -> Task<Message> {
    // It may already be on the next one: modules sit side by side, and that
    // module's enter can arrive before this leave. Closing here would take
    // away the tooltip that has just opened for it.
    if bar.hovered.is_some_and(|hovered| hovered != index) {
        return Task::none();
    }

    bar.hovered = None;
    close_popup(bar)
}

/// Where every bar wants a surface, given the monitors that exist.
///
/// An empty monitor list means nobody could tell us what there is, which
/// happens without a compositor backend. Every bar then gets one surface and
/// the compositor decides where to put it.
fn wanted(bar: &Bar) -> Vec<(usize, Option<String>)> {
    let mut places = Vec::new();

    for (index, layout) in bar.layouts.iter().enumerate() {
        match &layout.config.output {
            Some(name) if bar.outputs.is_empty() => {
                // Named, but there is no list to check it against. Asking for
                // it by name is still right: layershellev looks the name up in
                // its own list of outputs, which it always has.
                places.push((index, Some(name.clone())));
            }
            Some(name) if bar.outputs.iter().any(|output| output == name) => {
                places.push((index, Some(name.clone())));
            }
            // A name that matches nothing resolves to "let the compositor
            // choose", which would silently stack this bar on top of another.
            // Draw nothing instead; `place` says why.
            Some(_) => {}
            None if bar.outputs.is_empty() => places.push((index, None)),
            None => places.extend(
                bar.outputs
                    .iter()
                    .map(|output| (index, Some(output.clone()))),
            ),
        }
    }

    // Every bar named a monitor that is not there, so refusing them all would
    // leave nothing on screen and no way to find out why -- a typo in `output`
    // would look exactly like a bar that failed to start. Put the first one
    // wherever the compositor likes; it carries the warning saying so.
    if places.is_empty() && !bar.layouts.is_empty() {
        places.push((0, None));
    }

    places
}

/// Create and destroy surfaces so the live ones match [`wanted`].
fn place(bar: &mut Bar) -> Task<Message> {
    let places = wanted(bar);

    let missing: Vec<&str> = bar
        .layouts
        .iter()
        .filter_map(|layout| layout.config.output.as_deref())
        .filter(|name| !bar.outputs.is_empty() && !bar.outputs.iter().any(|output| output == name))
        .collect();

    if !missing.is_empty() {
        let detail = format!(
            "No monitor named {}.\nThis machine has {}.",
            missing.join(", "),
            bar.outputs.join(", ")
        );
        eprintln!("ricebar: {}", detail.replace('\n', " "));
        bar.warn(detail);
    }

    let mut tasks = Vec::new();

    // Anything on a monitor that has gone, or belonging to a bar the config no
    // longer has. Dropping it from the map is what makes `view` draw nothing
    // into it while the surface is on its way out.
    let stale: Vec<window::Id> = bar
        .surfaces
        .iter()
        .filter(|(_, surface)| {
            !places
                .iter()
                .any(|(layout, output)| *layout == surface.layout && *output == surface.output)
        })
        .map(|(id, _)| *id)
        .collect();

    for id in stale {
        bar.surfaces.remove(&id);
        tasks.push(Task::done(Message::RemoveWindow(id)));
    }

    for (layout, output) in places {
        let held = bar
            .surfaces
            .values()
            .any(|surface| surface.layout == layout && surface.output == output);

        if held {
            continue;
        }

        let Some(settings) = bar_settings(bar, layout, output.clone()) else {
            continue;
        };

        let id = window::Id::unique();
        bar.surfaces.insert(id, Surface { layout, output });
        tasks.push(Task::done(Message::NewLayerShell { settings, id }));
    }

    Task::batch(tasks)
}

/// The layer surface one bar wants on one monitor.
fn bar_settings(bar: &Bar, layout: usize, output: Option<String>) -> Option<NewLayerShellSettings> {
    let config = &bar.layouts.get(layout)?.config;

    let [top, right, bottom, left] = config.margin;
    let height = config.total_height();

    let edge = match config.position {
        config::Position::Top => Anchor::Top,
        config::Position::Bottom => Anchor::Bottom,
    };

    let exclusive_zone = if config.exclusive {
        // A margin pushes the bar inwards, so the space to reserve is the bar
        // plus whatever gap sits between it and the edge it is anchored to.
        let gap = match config.position {
            config::Position::Top => top,
            config::Position::Bottom => bottom,
        };
        i32::try_from(height)
            .unwrap_or(i32::MAX)
            .saturating_add(gap.max(0))
    } else {
        0
    };

    Some(NewLayerShellSettings {
        // Zero width fills the monitor, which is what anchoring to both sides
        // asks for anyway.
        size: Some((0, height)),
        layer: Layer::Top,
        anchor: edge | Anchor::Left | Anchor::Right,
        exclusive_zone: Some(exclusive_zone),
        margin: Some((top, right, bottom, left)),
        // Defaults to OnDemand, which would let the bar steal focus.
        keyboard_interactivity: KeyboardInteractivity::None,
        output_option: output.map_or(OutputOption::Active, OutputOption::OutputName),
        events_transparent: false,
        // One namespace for every bar, so compositor rules written against it
        // keep working whether the config describes one bar or four.
        namespace: Some(namespace()),
    })
}

/// Re-read the config and rebuild from it, or explain why it was not applied.
fn reload(bar: &mut Bar) -> Task<Message> {
    let Some(path) = bar.config.path.clone() else {
        return Task::none();
    };

    let next = match config::reread(&path) {
        Ok(next) => next,
        Err(problem) => {
            eprintln!(
                "ricebar: {} is invalid, keeping the running config:",
                path.display()
            );
            eprintln!("{problem}");
            bar.warn(format!(
                "{} is invalid.\nThe bar is still running the config it started with.\n\n{problem}",
                path.display()
            ));
            return Task::none();
        }
    };

    let fixed = config::fixed_differences(&bar.config, &next);
    if !fixed.is_empty() {
        let changed = fixed.join(", ");
        eprintln!("ricebar: {changed} changed; restart ricebar to apply");
        bar.warn(format!(
            "{changed} changed.\nThe bar's surface is sized and placed once, when it is created, so restart ricebar to apply this."
        ));
        return Task::none();
    }

    // Everything drawn comes from the config, so a new Bar is the cheapest
    // rebuild that cannot leave anything stale behind.
    let closed = close_popup(bar);
    let retiring = bar.retiring;
    let generation = bar.generation.wrapping_add(1);
    let outputs = std::mem::take(&mut bar.outputs);
    let surfaces = std::mem::take(&mut bar.surfaces);

    *bar = Bar::new(next);

    // The popup's surface is still on its way out, and its last draw must not
    // fall through to rendering the whole bar into it.
    bar.retiring = retiring;
    bar.generation = generation;
    bar.outputs = outputs;
    // The list is already known, so the next one to arrive says nothing new.
    bar.told = true;
    // Carried over so `place` can tell which surfaces already exist. Ones the
    // new config has no use for are removed there, and geometry that changed
    // is applied by tearing the surface down and building it again -- which is
    // why position, margin, height and exclusivity are no longer frozen.
    bar.surfaces = surfaces;

    let placed = replace(bar);

    eprintln!("ricebar: reloaded {}", path.display());
    Task::batch([closed, placed])
}

/// Rebuild every surface, for a reload that may have changed their geometry.
///
/// A layer surface is sized and placed once, when it is created, so the only
/// way to move one is to make a new one. Dropping them all first means `place`
/// sees nothing to keep.
fn replace(bar: &mut Bar) -> Task<Message> {
    let old: Vec<window::Id> = bar.surfaces.keys().copied().collect();
    bar.surfaces.clear();

    let closed = old
        .into_iter()
        .map(|id| Task::done(Message::RemoveWindow(id)));

    Task::batch(closed.chain(std::iter::once(place(bar))))
}

pub fn update(bar: &mut Bar, message: Message) -> Task<Message> {
    match message {
        // Opening and closing surfaces is the bar's job, so these are handled
        // here as well as being passed down.
        Message::Module(index, Event::TogglePopup) => {
            let told = match bar.modules.get_mut(index) {
                Some(module) => module
                    .update(Event::TogglePopup)
                    .map(move |event| Message::Module(index, event)),
                None => Task::none(),
            };

            Task::batch([told, toggle_popup(bar, index)])
        }
        Message::Module(index, Event::Entries(entries)) => {
            let told = match bar.modules.get_mut(index) {
                Some(module) => module
                    .update(Event::Entries(entries))
                    .map(move |event| Message::Module(index, event)),
                None => Task::none(),
            };

            // A script has answered, so the surface can be sized and shown now
            // rather than on the click that asked for it.
            Task::batch([told, show_popup(bar, index)])
        }
        Message::Module(index, Event::Activate(entry)) => {
            let closed = close_popup(bar);

            let ran = match bar.modules.get_mut(index) {
                Some(module) => module
                    .update(Event::Activate(entry))
                    .map(move |event| Message::Module(index, event)),
                None => Task::none(),
            };

            Task::batch([closed, ran])
        }
        Message::Module(index, event) => match bar.modules.get_mut(index) {
            Some(module) => module
                .update(event)
                .map(move |event| Message::Module(index, event)),
            None => Task::none(),
        },
        Message::Enter(index) => {
            bar.hovered = Some(index);
            bar.entered = Instant::now();
            locate(index, Opening::Tooltip)
        }
        Message::Scroll(index, delta) => {
            let banked = bar.scrolled.entry(index).or_default();
            *banked = (*banked + notches(delta)).clamp(-NOTCH_LIMIT, NOTCH_LIMIT);

            // Spend one whole notch, keeping the remainder for the next event.
            let direction = if *banked >= 1.0 {
                *banked -= 1.0;
                Direction::Up
            } else if *banked <= -1.0 {
                *banked += 1.0;
                Direction::Down
            } else {
                return Task::none();
            };

            match bar.modules.get_mut(index) {
                Some(module) => module
                    .update(Event::Scroll(direction))
                    .map(move |event| Message::Module(index, event)),
                None => Task::none(),
            }
        }
        Message::Leave(index) => {
            // A module's popup waits to be clicked, so it must survive the
            // pointer travelling from the module down to it.
            if matches!(
                bar.popup.as_ref().map(|popup| &popup.kind),
                Some(PopupKind::Module)
            ) {
                return Task::none();
            }

            // A popup's own appearance makes iced report the pointer as having
            // left the bar. A leave arriving this soon is usually that echo --
            // but it is also what a quick sweep across the bar looks like, and
            // dropping it would strand the tooltip on screen with no further
            // leave ever coming. Ask again once the grace has passed.
            if let Some(popup) = &bar.popup {
                let settling = POPUP_GRACE.saturating_sub(popup.opened.elapsed());

                if !settling.is_zero() {
                    let seen = Instant::now();

                    return Task::future(async move {
                        tokio::time::sleep(settling).await;
                        Message::Left(index, seen)
                    });
                }
            }

            leave(bar, index)
        }
        Message::Left(index, seen) => {
            // The pointer came back while this was waiting, so the leave that
            // scheduled it was the popup's echo after all.
            if bar.entered > seen {
                return Task::none();
            }

            leave(bar, index)
        }
        Message::ConfigChanged => reload(bar),
        Message::Outputs(outputs) => {
            // The first list always places the bars, even when it is empty --
            // with no compositor backend it is empty and stays that way, and
            // an early return here left such a bar with no surface at all.
            if outputs == bar.outputs && bar.told {
                return Task::none();
            }

            bar.told = true;
            bar.outputs = outputs;

            // Not placed straight away. A monitor reaches us over the
            // compositor's own socket before its `wl_output` reaches this
            // client's Wayland connection, and a name the layer-shell runtime
            // has not heard of yet resolves to "wherever you like" -- which
            // puts the new bar on top of an existing one instead. Waiting lets
            // the two agree on what exists.
            Task::future(async {
                tokio::time::sleep(SETTLE).await;
                Message::Place
            })
        }
        Message::Place => place(bar),
        Message::PopupUnder {
            module,
            at,
            opening,
        } => {
            if let Some(at) = at {
                bar.placement = Some((module, at));
            }

            match opening {
                // Whichever module the pointer is on now, which may not be the
                // one this answer was asked for.
                Opening::Tooltip => open_popup(bar),
                Opening::Popup => show_popup(bar, module),
            }
        }
        // `to_layer_message` appends its own variants to this enum. Upstream's
        // example ends with `unreachable!()`, which is a panic waiting to happen.
        _ => Task::none(),
    }
}

/// Open a popup once both facts are known: which module is hovered, and where
/// the pointer is. The two arrive in separate messages, in no fixed order.
fn open_popup(bar: &mut Bar) -> Task<Message> {
    let Some(index) = bar.hovered else {
        return Task::none();
    };

    // Something the user clicked open outranks hover text.
    if matches!(
        bar.popup.as_ref().map(|popup| &popup.kind),
        Some(PopupKind::Module)
    ) {
        return Task::none();
    }

    if bar
        .popup
        .as_ref()
        .is_some_and(|popup| popup.module == index)
    {
        return Task::none();
    }

    let Some(tooltip) = bar.modules.get(index).and_then(|module| module.tooltip()) else {
        return Task::none();
    };

    let closed = close_popup(bar);
    let id = window::Id::unique();
    // Text cannot be measured outside a renderer, so a tooltip's surface is
    // estimated from its longest line and how many lines there are.
    let font_size = bar.config.first().font_size;

    let widest = tooltip
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or_default() as f32;
    let lines = tooltip.lines().count().max(1) as f32;

    let width = widest.mul_add(font_size * TOOLTIP_GLYPH_RATIO, 2.0 * TOOLTIP_PADDING);
    let height = lines.mul_add(font_size * TOOLTIP_LINE_RATIO, 2.0 * TOOLTIP_PADDING);

    let settings = popup_settings(
        bar,
        index,
        width,
        height,
        true,
        bar.placement
            .filter(|(module, _)| *module == index)
            .map(|(_, placement)| placement),
    );

    bar.popup = Some(Popup {
        id,
        module: index,
        kind: PopupKind::Tooltip,
        opened: Instant::now(),
    });

    Task::batch([closed, Task::done(Message::NewLayerShell { settings, id })])
}

/// Show or hide the popup belonging to a module.
fn toggle_popup(bar: &mut Bar, index: usize) -> Task<Message> {
    // A second click on the same module closes what the first one opened.
    if bar
        .popup
        .as_ref()
        .is_some_and(|popup| popup.module == index && matches!(popup.kind, PopupKind::Module))
    {
        return close_popup(bar);
    }

    locate(index, Opening::Popup)
}

/// Ask where a module is, and open what was asked for under it.
///
/// Where the module sits has to be asked of the widget tree, so opening waits
/// one message for the answer. The timer beside it is the fallback: if that
/// answer never comes the popup still opens, at the edge of the bar as it used
/// to, rather than not at all.
fn locate(index: usize, opening: Opening) -> Task<Message> {
    Task::batch([
        placed(index, opening),
        Task::future(async move {
            tokio::time::sleep(PLACEMENT_GRACE).await;
            Message::PopupUnder {
                module: index,
                at: None,
                opening,
            }
        }),
    ])
}

/// How long to wait for the widget tree to say where a module is. Long enough
/// for a frame, short enough that a popup opening without an answer still feels
/// like a response to the click.
const PLACEMENT_GRACE: Duration = Duration::from_millis(150);

/// The id given to the container around one module, so a widget operation can
/// find where it ended up.
fn module_id(index: usize) -> widget::Id {
    widget::Id::from(format!("ricebar-module-{index}"))
}

/// The id on the container filling a whole bar, which is how wide that bar is.
const BAR_ID: &str = "ricebar-bar";

/// Where a module was drawn, and how wide its bar is.
///
/// iced reports no widget geometry to `update`, and a layer surface never
/// learns its own width, so both come from walking the widget tree after it has
/// been laid out. Without this a popup can only be anchored to an edge of the
/// bar, which puts a menu belonging to the third module from the right under
/// the last one.
struct Placed {
    wanted: widget::Id,
    module: Option<Rectangle>,
    bar: Option<f32>,
}

impl widget::Operation<(Rectangle, f32)> for Placed {
    fn container(&mut self, id: Option<&widget::Id>, bounds: Rectangle) {
        let Some(id) = id else {
            return;
        };

        if *id == self.wanted {
            self.module = Some(bounds);
        } else if *id == widget::Id::new(BAR_ID) {
            // Every surface is walked, so the widest bar wins. With one bar per
            // output that is the one the module is on.
            self.bar = Some(self.bar.unwrap_or_default().max(bounds.width));
        }
    }

    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation<(Rectangle, f32)>)) {
        operate(self);
    }

    fn finish(&self) -> widget::operation::Outcome<(Rectangle, f32)> {
        match (self.module, self.bar) {
            (Some(module), Some(bar)) => widget::operation::Outcome::Some((module, bar)),
            _ => widget::operation::Outcome::None,
        }
    }
}

/// Ask where a module is before opening its popup under it.
fn placed(index: usize, opening: Opening) -> Task<Message> {
    widget::operate(Placed {
        wanted: module_id(index),
        module: None,
        bar: None,
    })
    .map(move |(module, bar)| Message::PopupUnder {
        module: index,
        at: Some(Placement {
            centre: module.center_x(),
            width: bar,
        }),
        opening,
    })
}

/// Open a module's popup, replacing whatever else was open.
///
/// A module whose popup comes from a script has nothing to show until that
/// script answers, which is why this can be reached twice: once from the click
/// and again when the entries arrive.
fn show_popup(bar: &mut Bar, index: usize) -> Task<Message> {
    if bar
        .popup
        .as_ref()
        .is_some_and(|popup| popup.module == index && matches!(popup.kind, PopupKind::Module))
    {
        return Task::none();
    }

    let style = style_for(bar, index);
    let Some(shape) = bar
        .modules
        .get(index)
        .and_then(|module| module.popup(style))
    else {
        return Task::none();
    };

    let closed = close_popup(bar);
    let id = window::Id::unique();
    // Unlike a tooltip, this one accepts pointer input, since it is clicked.
    let settings = popup_settings(
        bar,
        index,
        shape.width,
        shape.height,
        false,
        bar.placement
            .filter(|(module, _)| *module == index)
            .map(|(_, placement)| placement),
    );

    bar.popup = Some(Popup {
        id,
        module: index,
        kind: PopupKind::Module,
        opened: Instant::now(),
    });

    Task::batch([closed, Task::done(Message::NewLayerShell { settings, id })])
}

fn close_popup(bar: &mut Bar) -> Task<Message> {
    let Some(popup) = bar.popup.take() else {
        return Task::none();
    };

    bar.retiring = Some(popup.id);
    let removed = Task::done(Message::RemoveWindow(popup.id));

    match popup.kind {
        // A module keeps its own idea of whether its popup is open, and the
        // bar closes popups for reasons the module never hears about -- one
        // belonging to another module opening over the top of it. Left
        // untold, the module reads its next click as the close that already
        // happened, and swallows it.
        PopupKind::Module => Task::batch([
            removed,
            Task::done(Message::Module(popup.module, Event::ClosePopup)),
        ]),
        // Not a module's business, and a tooltip is closed to make room for
        // the popup of the very module it belongs to -- which must not be
        // told to forget what it is about to show.
        PopupKind::Tooltip => removed,
    }
}

/// The first bar that draws this module, of however many do.
fn holding(bar: &Bar, index: usize) -> Option<&Layout> {
    bar.layouts.iter().find(|layout| {
        layout.rows.iter().any(|row| {
            row.left.contains(&index) || row.center.contains(&index) || row.right.contains(&index)
        })
    })
}

/// Build the surface for a tooltip or a module's own popup, at the given size.
///
/// Both are layer surfaces rather than xdg popups. The runtime builds popups
/// with a grab, which suits a menu but takes every pointer event away from the
/// bar underneath for as long as one is open, leaving no way to dismiss it.
fn popup_settings(
    bar: &Bar,
    index: usize,
    width: f32,
    height: f32,
    transparent_to_events: bool,
    under: Option<Placement>,
) -> NewLayerShellSettings {
    let width = width.clamp(48.0, 720.0) as u32;
    let height = height.max(1.0) as u32;

    // Whichever bar holds this module is the one the popup hangs from, and
    // with several bars it is the one whose edge and margin apply.
    let layout = holding(bar, index);
    let config = layout.map_or_else(|| bar.config.first(), |layout| layout.config.clone());
    let rows = layout.map(|layout| layout.rows.as_slice()).unwrap_or(&[]);

    // Anchor under the module's own region. A layer surface anchored to
    // neither side is centred on that axis, and one anchored to a side cannot
    // run off it -- so the tooltip stays on screen without the bar's width,
    // which layer surfaces never report.
    let side = if rows.iter().any(|row| row.left.contains(&index)) {
        Some(Anchor::Left)
    } else if rows.iter().any(|row| row.right.contains(&index)) {
        Some(Anchor::Right)
    } else {
        None
    };

    // Where the module actually is, when that has been asked. Anchored left and
    // pushed across, so the popup opens under the module rather than at the end
    // of the bar the module happens to sit nearest.
    let offset = under.map(|placement| {
        let popup = width as f32;
        let room = placement.width - popup;
        (placement.centre - popup / 2.0).clamp(0.0, room.max(0.0)) as i32
    });

    let [top, _, bottom, _] = config.margin;

    // An exclusive bar already displaces the usable area, so the compositor
    // puts the tooltip directly below it. A non-exclusive one does not.
    let clearance = if config.exclusive {
        0
    } else {
        config.total_height() as i32
    };

    let left = offset.unwrap_or_default();

    let (edge, margin) = match config.position {
        config::Position::Top => (Anchor::Top, (top + clearance, 0, 0, left)),
        config::Position::Bottom => (Anchor::Bottom, (0, 0, bottom + clearance, left)),
    };

    // An offset is measured from the left, so it needs that anchor whatever
    // region the module is in.
    let side = if offset.is_some() {
        Some(Anchor::Left)
    } else {
        side
    };

    NewLayerShellSettings {
        size: Some((width, height)),
        layer: Layer::Overlay,
        anchor: side.map_or(edge, |side| edge | side),
        exclusive_zone: Some(0),
        margin: Some(margin),
        keyboard_interactivity: KeyboardInteractivity::None,
        output_option: OutputOption::Active,
        // A tooltip must not take pointer events, or the bar would never learn
        // that the pointer had moved away. A menu exists to be clicked.
        events_transparent: transparent_to_events,
        namespace: Some(String::from("ricebar-popup")),
    }
}

/// Text cannot be measured outside a renderer, so a tooltip's surface is sized
/// from the font size. Deliberately an over-estimate: too wide leaves harmless
/// empty space, while too narrow wraps the text and the surface clips it.
const TOOLTIP_GLYPH_RATIO: f32 = 0.6;
const TOOLTIP_LINE_RATIO: f32 = 1.4;
const TOOLTIP_PADDING: f32 = 10.0;

/// The palette and face one module is drawn in.
///
/// A module's own font wins, then its bar's, then whatever the process started
/// with -- and a popup takes the same, so a menu over a module in its own font
/// is not suddenly in a different one.
fn style_for(bar: &Bar, index: usize) -> config::Style {
    let (font, size) = bar.typography.get(index).copied().unwrap_or_default();

    holding(bar, index).map_or_else(
        || bar.config.first().style,
        |layout| config::Style {
            font: font.or(layout.font),
            font_size: size.unwrap_or(layout.config.font_size),
            ..layout.config.style
        },
    )
}

fn popup_view<'a>(bar: &'a Bar, popup: &'a Popup) -> Element<'a, Message> {
    let style = style_for(bar, popup.module);

    let body: Element<'a, Message> = match &popup.kind {
        PopupKind::Tooltip => {
            // Read at draw time rather than kept from when it opened, so a
            // tooltip showing a value follows that value while it is open.
            let tooltip = bar
                .modules
                .get(popup.module)
                .and_then(|module| module.tooltip())
                .unwrap_or_default();

            text(tooltip).wrapping(text::Wrapping::None).into()
        }
        PopupKind::Module => match bar.modules.get(popup.module) {
            Some(module) => {
                let index = popup.module;
                module
                    .popup_view(style)
                    .map(move |event| Message::Module(index, event))
            }
            None => space::horizontal().into(),
        },
    };

    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_y(Length::Fill)
        .padding([0.0, TOOLTIP_PADDING])
        .style(move |_theme| container::Style {
            background: Some(style.popup_fill().color().into()),
            text_color: Some(style.foreground.color()),
            border: Border {
                color: style.border_color.color(),
                width: 1.0,
                radius: 4.into(),
            },
            ..Default::default()
        })
        .into()
}

pub fn view(bar: &Bar, id: window::Id) -> Element<'_, Message> {
    if let Some(popup) = &bar.popup
        && popup.id == id
    {
        return popup_view(bar, popup);
    }

    // A surface on its way out, whether a popup or a bar whose monitor has
    // been unplugged, may still be asked to draw once more.
    let Some(layout) = bar
        .surfaces
        .get(&id)
        .and_then(|surface| bar.layouts.get(surface.layout))
    else {
        return space::horizontal().into();
    };

    if bar.retiring == Some(id) {
        return space::horizontal().into();
    }

    let style = layout.config.style;
    let spacing = layout.config.spacing;

    let region = |indices: &[usize]| {
        row(indices.iter().filter_map(|&index| {
            let module = bar.modules.get(index)?;

            // A module's own face wins, then the bar's, then whatever the
            // process was started with.
            let (font, size) = bar.typography.get(index).copied().unwrap_or_default();
            let style = config::Style {
                font: font.or(layout.font),
                font_size: size.unwrap_or(layout.config.font_size),
                ..style
            };

            let element = module
                .view(style)
                .map(move |event| Message::Module(index, event));

            Some(
                // The id is what lets `placed` find where this module ended up,
                // so its popup can open under it.
                container(
                    mouse_area(element)
                        .on_enter(Message::Enter(index))
                        .on_exit(Message::Leave(index))
                        .on_scroll(move |delta| Message::Scroll(index, delta)),
                )
                .id(module_id(index))
                .into(),
            )
        }))
        .spacing(spacing)
        .align_y(Alignment::Center)
    };

    let lines = layout.rows.iter().map(|line| {
        // The sides take the width they need and the centre takes the slack.
        // Giving each region a fixed third instead centres the middle exactly,
        // but a side that outgrows its third is then drawn off the edge of the
        // screen -- and a bar that silently hides a module is worse than one
        // whose clock sits a little off centre.
        let content = row![
            region(&line.left),
            container(region(&line.center)).center_x(Length::Fill),
            region(&line.right),
        ]
        .align_y(Alignment::Center);

        container(content)
            .width(Length::Fill)
            .height(Length::Fixed(line.height as f32))
            // The row's own `align_y` only lines the modules up against each
            // other. Without this each line sits at the top of its own height.
            .align_y(layout.config.vertical_align)
            .into()
    });

    container(column(lines))
        // The whole bar, which is how a popup learns the width it has to stay
        // inside. A layer surface is never told its own size.
        .id(widget::Id::new(BAR_ID))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([0.0, layout.config.padding])
        .style(move |_theme| container::Style {
            background: Some(style.background.color().into()),
            border: Border {
                color: style.border_color.color(),
                width: style.border_width,
                radius: style.border_radius.into(),
            },
            ..Default::default()
        })
        .into()
}

pub fn style(bar: &Bar, _theme: &iced::Theme) -> iced::theme::Style {
    iced::theme::Style {
        // The container in `view` paints the fill, so the surface itself stays
        // transparent. That is what lets border-radius round real corners
        // instead of cutting into an opaque rectangle.
        background_color: Color::TRANSPARENT,
        text_color: bar.config.first().style.foreground.color(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing here runs the tasks the bar hands back; the state it keeps is
    /// what these are about.
    fn tell(bar: &mut Bar, message: Message) {
        let _ = update(bar, message);
    }

    /// With no compositor backend the output list is empty and stays empty --
    /// which is the list the bar starts with. Reading that as "nothing
    /// changed" left the bar with no surface at all, on every compositor
    /// ricebar does not speak.
    #[test]
    fn an_empty_first_output_list_still_places_the_bar() {
        let mut bar = Bar::new(config::Config::default());
        assert!(bar.surfaces.is_empty());

        tell(&mut bar, Message::Outputs(Vec::new()));
        assert!(bar.told, "the first list is an answer, empty or not");

        let _ = place(&mut bar);
        assert_eq!(
            bar.surfaces.len(),
            1,
            "one bar, wherever the compositor likes"
        );
    }

    /// A click opens the layout list, a second click closes it, and choosing
    /// one closes it too. The module cannot be clicked in a test rig, so this
    /// is where that path is checked.
    #[test]
    fn the_keyboard_popup_opens_closes_and_chooses() {
        let config = config::Config {
            bars: vec![config::Bar {
                modules_left: vec![String::from("keyboard")],
                ..config::Bar::default()
            }],
            ..config::Config::default()
        };

        let mut bar = Bar::new(config);
        assert_eq!(bar.modules.len(), 1);

        // Stand-ins for what the widget operation reports, since nothing lays
        // out a widget tree in a test. Nothing anywhere reads a resolution: the
        // real width is the bar container's own bounds, whatever monitor it is
        // on, and these two numbers only have to be a module inside a bar.
        const MODULE_CENTRE: f32 = 100.0;
        const BAR_WIDTH: f32 = 640.0;

        // A click asks where the module is before opening anything, so that
        // answer has to be sent by hand here.
        let clicked = |bar: &mut Bar| {
            tell(bar, Message::Module(0, Event::TogglePopup));
            tell(
                bar,
                Message::PopupUnder {
                    module: 0,
                    at: Some(Placement {
                        centre: MODULE_CENTRE,
                        width: BAR_WIDTH,
                    }),
                    opening: Opening::Popup,
                },
            );
        };

        // Nothing to choose from until the compositor has said what there is.
        clicked(&mut bar);
        assert!(bar.popup.is_none());

        tell(
            &mut bar,
            Message::Module(
                0,
                Event::Layouts(compositor::Layouts {
                    names: vec![String::from("pl"), String::from("gb")],
                    current: 0,
                }),
            ),
        );

        clicked(&mut bar);
        assert!(
            matches!(
                bar.popup.as_ref().map(|popup| &popup.kind),
                Some(PopupKind::Module)
            ),
            "a click opens the list"
        );

        tell(&mut bar, Message::Module(0, Event::TogglePopup));
        assert!(bar.popup.is_none(), "a second click closes it");

        clicked(&mut bar);
        tell(&mut bar, Message::Module(0, Event::Activate(1)));
        assert!(bar.popup.is_none(), "choosing a layout closes it");
    }

    /// The widget tree is asked where a module is, and a popup that waited on
    /// that answer would never open if it did not come. It opens anyway.
    #[test]
    fn a_popup_opens_even_when_nothing_says_where_the_module_is() {
        let config = config::Config {
            bars: vec![config::Bar {
                modules_left: vec![String::from("keyboard")],
                ..config::Bar::default()
            }],
            ..config::Config::default()
        };

        let mut bar = Bar::new(config);

        tell(
            &mut bar,
            Message::Module(
                0,
                Event::Layouts(compositor::Layouts {
                    names: vec![String::from("pl")],
                    current: 0,
                }),
            ),
        );

        tell(&mut bar, Message::Module(0, Event::TogglePopup));
        tell(
            &mut bar,
            Message::PopupUnder {
                module: 0,
                at: None,
                opening: Opening::Popup,
            },
        );

        assert!(bar.popup.is_some(), "no placement is not a reason to hide");
        assert!(bar.placement.is_none(), "and nothing was learnt from it");
    }

    /// A tooltip, a menu and a popup all take their bar's `popup-background`,
    /// and its plain background where that is unset. Nothing renders in a test
    /// rig, so this checks the colour `popup_view` paints with -- and that the
    /// bar next to it keeps its own.
    #[test]
    fn a_popup_takes_the_fill_of_the_bar_it_belongs_to() {
        const FILL: config::Rgba = config::Rgba::new(0x11, 0x22, 0x33);

        let config = config::Config {
            bars: vec![
                config::Bar {
                    modules_left: vec![String::from("clock")],
                    style: config::Style {
                        popup_background: Some(FILL),
                        ..config::Style::default()
                    },
                    ..config::Bar::default()
                },
                config::Bar {
                    modules_left: vec![String::from("keyboard")],
                    ..config::Bar::default()
                },
            ],
            ..config::Config::default()
        };

        let bar = Bar::new(config);
        let index = |name: &str| {
            bar.modules
                .iter()
                .position(|module| module.name() == name)
                .expect("the module is named by a bar")
        };

        assert_eq!(style_for(&bar, index("clock")).popup_fill(), FILL);
        assert_eq!(
            style_for(&bar, index("keyboard")).popup_fill(),
            config::Style::default().background,
            "unset follows that bar's own background"
        );
    }

    /// The same list arriving again is nothing new, and rebuilding surfaces
    /// for it would tear down a bar the user is looking at.
    #[test]
    fn the_same_output_list_twice_changes_nothing() {
        let mut bar = Bar::new(config::Config::default());

        tell(&mut bar, Message::Outputs(vec![String::from("HEADLESS-1")]));
        let _ = place(&mut bar);
        let held: Vec<window::Id> = bar.surfaces.keys().copied().collect();

        tell(&mut bar, Message::Outputs(vec![String::from("HEADLESS-1")]));
        let _ = place(&mut bar);

        assert_eq!(bar.surfaces.keys().copied().collect::<Vec<_>>(), held);
    }
}
