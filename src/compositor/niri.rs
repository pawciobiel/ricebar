//! niri IPC.
//!
//! niri takes the opposite approach to Hyprland and sway. Those announce that
//! something changed and leave you to ask what; niri sends the whole state
//! when the stream opens and deltas after that, so the state is kept here
//! rather than re-queried. Its own documentation is explicit that this is how
//! a client avoids drifting out of sync.
//!
//! The wire format is a line of JSON in, a line of JSON out.

use std::collections::HashMap;
use std::env;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, Stream};
use iced::{Subscription, Task};
use serde::Deserialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use super::{Compositor, Layouts, Workspace, Workspaces};

pub struct Niri;

impl Compositor for Niri {
    fn name(&self) -> &'static str {
        "niri"
    }

    fn workspaces(&self) -> Subscription<Workspaces> {
        Subscription::run(watch)
    }

    fn focus(&self, id: i32) -> Task<()> {
        Task::future(async move {
            // By id rather than index: an index only identifies a workspace
            // within one output.
            let request =
                format!(r#"{{"Action":{{"FocusWorkspace":{{"reference":{{"Id":{id}}}}}}}}}"#);

            if let Err(error) = send(&request).await {
                eprintln!("ricebar: could not focus workspace {id}: {error}");
            }
        })
    }

    fn outputs(&self) -> Subscription<Vec<String>> {
        Subscription::run(watch_outputs)
    }

    fn layouts(&self) -> Subscription<Layouts> {
        Subscription::run(watch_layouts)
    }

    fn set_layout(&self, index: usize) -> Task<()> {
        Task::future(async move {
            let request =
                format!(r#"{{"Action":{{"SwitchLayout":{{"layout":{{"Index":{index}}}}}}}}}"#);

            if let Err(error) = send(&request).await {
                eprintln!("ricebar: could not switch keyboard layout: {error}");
            }
        })
    }
}

pub fn available() -> bool {
    socket_path().is_some_and(|path| path.exists())
}

fn socket_path() -> Option<PathBuf> {
    env::var_os("NIRI_SOCKET").map(PathBuf::from)
}

async fn connect() -> io::Result<UnixStream> {
    let path = socket_path().ok_or_else(|| io::Error::other("NIRI_SOCKET is not set"))?;
    UnixStream::connect(path).await
}

/// Send one request and read its reply.
async fn send(request: &str) -> io::Result<String> {
    let stream = connect().await?;
    let mut stream = BufReader::new(stream);

    stream.get_mut().write_all(request.as_bytes()).await?;
    stream.get_mut().write_all(b"\n").await?;

    let mut reply = String::new();
    stream.read_line(&mut reply).await?;
    Ok(reply)
}

// Only the fields the bar uses; serde ignores the rest of niri's JSON.
#[derive(Deserialize, Clone)]
struct RawWorkspace {
    id: i64,
    /// Position within its own output, and what to show when unnamed.
    idx: u8,
    name: Option<String>,
    output: Option<String>,
    /// Active on its own output.
    is_active: bool,
    /// Focused across all outputs.
    is_focused: bool,
}

#[derive(Deserialize)]
struct RawWindow {
    id: u64,
    workspace_id: Option<i64>,
}

/// Everything the stream has told us so far.
#[derive(Default)]
struct State {
    workspaces: Vec<RawWorkspace>,
    /// Window id to the workspace holding it, so workspaces can be counted.
    windows: HashMap<u64, i64>,
}

impl State {
    /// Apply one event, saying whether it changed anything worth drawing.
    fn apply(&mut self, name: &str, body: &Value) -> bool {
        match name {
            "WorkspacesChanged" => {
                self.workspaces = parse(body.get("workspaces"));
                true
            }
            // Sent on its own by some niri versions rather than a full list.
            "WorkspaceActivated" => {
                let Some(id) = body.get("id").and_then(Value::as_i64) else {
                    return false;
                };
                let focused = body
                    .get("focused")
                    .and_then(Value::as_bool)
                    .unwrap_or_default();

                let output = self
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.id == id)
                    .and_then(|workspace| workspace.output.clone());

                for workspace in &mut self.workspaces {
                    if workspace.output == output {
                        workspace.is_active = workspace.id == id;
                    }
                    if focused {
                        workspace.is_focused = workspace.id == id;
                    }
                }

                true
            }
            "WindowsChanged" => {
                self.windows = parse::<RawWindow>(body.get("windows"))
                    .into_iter()
                    .filter_map(|window| Some((window.id, window.workspace_id?)))
                    .collect();
                true
            }
            "WindowOpenedOrChanged" => {
                let Some(window) = body.get("window") else {
                    return false;
                };
                let Ok(window) = serde_json::from_value::<RawWindow>(window.clone()) else {
                    return false;
                };

                match window.workspace_id {
                    Some(workspace) => self.windows.insert(window.id, workspace),
                    // A window with no workspace is not on the strip any more.
                    None => self.windows.remove(&window.id),
                };

                true
            }
            "WindowClosed" => {
                let Some(id) = body.get("id").and_then(Value::as_u64) else {
                    return false;
                };
                self.windows.remove(&id);
                true
            }
            _ => false,
        }
    }

    fn snapshot(&self) -> Workspaces {
        let mut workspaces = self.workspaces.clone();

        // niri stacks workspaces per output, so order by output then position.
        workspaces.sort_by(|a, b| a.output.cmp(&b.output).then(a.idx.cmp(&b.idx)));

        workspaces
            .into_iter()
            .map(|workspace| Workspace {
                windows: self
                    .windows
                    .values()
                    .filter(|held| **held == workspace.id)
                    .count()
                    .try_into()
                    .unwrap_or(u16::MAX),
                // niri workspaces are unnamed unless you name them.
                name: workspace.name.unwrap_or_else(|| workspace.idx.to_string()),
                id: workspace.id.try_into().unwrap_or_default(),
                monitor: workspace.output.unwrap_or_default(),
                visible: workspace.is_active,
                focused: workspace.is_focused,
            })
            .collect()
    }
}

fn parse<T: for<'de> Deserialize<'de>>(value: Option<&Value>) -> Vec<T> {
    value
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

/// The outputs, taken from the same event stream the workspaces come from.
///
/// niri has an `Outputs` request, but every workspace names its output and
/// there is always at least one per output, so the event stream already
/// carries the answer -- and carries it again when a monitor is plugged in,
/// which a one-shot request would not.
fn watch_outputs() -> impl Stream<Item = Vec<String>> {
    iced::stream::channel(4, async |mut output: mpsc::Sender<Vec<String>>| {
        loop {
            if let Err(error) = follow_outputs(&mut output).await {
                eprintln!("ricebar: niri ipc: {error}");
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}

async fn follow_outputs(sender: &mut mpsc::Sender<Vec<String>>) -> io::Result<()> {
    let stream = connect().await?;
    let mut stream = BufReader::new(stream);

    stream.get_mut().write_all(b"\"EventStream\"\n").await?;

    let mut state = State::default();
    let mut sent: Vec<String> = Vec::new();
    let mut lines = stream.lines();

    while let Some(line) = lines.next_line().await? {
        let Ok(event) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        let Some((name, body)) = event.as_object().and_then(|event| event.iter().next()) else {
            continue;
        };

        if !state.apply(name, body) {
            continue;
        }

        let mut found: Vec<String> = state
            .workspaces
            .iter()
            .filter_map(|workspace| workspace.output.clone())
            .collect();
        found.sort();
        found.dedup();

        // Workspace events far outnumber monitor ones, and every one of them
        // would otherwise ask the bar to rebuild every surface it has.
        if found != sent && !found.is_empty() {
            sent = found.clone();

            if sender.send(found).await.is_err() {
                return Ok(());
            }
        }
    }

    Ok(())
}

fn watch_layouts() -> impl Stream<Item = Layouts> {
    iced::stream::channel(4, async |mut output: mpsc::Sender<Layouts>| {
        loop {
            if let Err(error) = follow_layouts(&mut output).await {
                eprintln!("ricebar: niri ipc: {error}");
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}

/// The layouts, kept from the two events niri sends about them: the whole list
/// when it is configured or reloaded, and an index alone on every switch.
async fn follow_layouts(sender: &mut mpsc::Sender<Layouts>) -> io::Result<()> {
    let stream = connect().await?;
    let mut stream = BufReader::new(stream);

    stream.get_mut().write_all(b"\"EventStream\"\n").await?;

    let mut layouts = Layouts::default();
    let mut lines = stream.lines();

    while let Some(line) = lines.next_line().await? {
        let Ok(event) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        let Some((name, body)) = event.as_object().and_then(|event| event.iter().next()) else {
            continue;
        };

        let index =
            |value: Option<&Value>| value.and_then(Value::as_u64).unwrap_or_default() as usize;

        match name.as_str() {
            "KeyboardLayoutsChanged" => {
                let changed = body.get("keyboard_layouts");
                layouts.names = parse(changed.and_then(|changed| changed.get("names")));
                layouts.current = index(changed.and_then(|changed| changed.get("current_idx")));
            }
            "KeyboardLayoutSwitched" => layouts.current = index(body.get("idx")),
            _ => continue,
        }

        if sender.send(layouts.clone()).await.is_err() {
            return Ok(());
        }
    }

    Ok(())
}

fn watch() -> impl Stream<Item = Workspaces> {
    iced::stream::channel(16, async |mut output: mpsc::Sender<Workspaces>| {
        loop {
            if let Err(error) = follow(&mut output).await {
                eprintln!("ricebar: niri ipc: {error}");
            }
            // The socket goes when niri exits. Keep retrying so the bar
            // recovers rather than silently freezing.
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}

async fn follow(output: &mut mpsc::Sender<Workspaces>) -> io::Result<()> {
    let stream = connect().await?;
    let mut stream = BufReader::new(stream);

    stream.get_mut().write_all(b"\"EventStream\"\n").await?;

    let mut state = State::default();
    let mut lines = stream.lines();

    while let Some(line) = lines.next_line().await? {
        let Ok(event) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        // Each event is a single-key object naming it, including the initial
        // {"Ok":"Handled"} acknowledgement, which no arm claims.
        let Some((name, body)) = event.as_object().and_then(|event| event.iter().next()) else {
            continue;
        };

        if state.apply(name, body) && output.send(state.snapshot()).await.is_err() {
            return Ok(());
        }
    }

    Ok(())
}
