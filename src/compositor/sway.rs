//! sway IPC.
//!
//! sway speaks i3's protocol: a `i3-ipc` magic, a length and a type, then a
//! JSON payload, all in native byte order. That is little enough to speak
//! directly, and doing so keeps the bar on one async runtime -- `swayipc-async`
//! is built on `async-io`, which would mean a second reactor beside tokio.

use std::env;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, Stream};
use iced::{Subscription, Task};
use serde::Deserialize;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use super::{Compositor, Layouts, Workspace, Workspaces};

pub struct Sway;

impl Compositor for Sway {
    fn name(&self) -> &'static str {
        "sway"
    }

    fn workspaces(&self) -> Subscription<Workspaces> {
        Subscription::run(watch)
    }

    fn focus(&self, id: i32) -> Task<()> {
        Task::future(async move {
            if let Err(error) = command(&format!("workspace number {id}")).await {
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
            // `type:keyboard` rather than one device: sway keeps a layout per
            // keyboard, and switching only the one you named leaves the laptop
            // keyboard on the old layout while the external one moves.
            let payload = format!("input type:keyboard xkb_switch_layout {index}");

            if let Err(error) = command(&payload).await {
                eprintln!("ricebar: could not switch keyboard layout: {error}");
            }
        })
    }
}

pub fn available() -> bool {
    socket_path().is_some_and(|path| path.exists())
}

fn socket_path() -> Option<PathBuf> {
    // I3SOCK covers i3 and the sway builds that set it instead.
    env::var_os("SWAYSOCK")
        .or_else(|| env::var_os("I3SOCK"))
        .map(PathBuf::from)
}

const MAGIC: &[u8; 6] = b"i3-ipc";
const RUN_COMMAND: u32 = 0;
const GET_WORKSPACES: u32 = 1;
const SUBSCRIBE: u32 = 2;
const GET_OUTPUTS: u32 = 3;
const GET_TREE: u32 = 4;
const GET_INPUTS: u32 = 100;

async fn connect() -> io::Result<UnixStream> {
    let path = socket_path().ok_or_else(|| io::Error::other("SWAYSOCK is not set"))?;
    UnixStream::connect(path).await
}

async fn send(stream: &mut UnixStream, kind: u32, payload: &str) -> io::Result<()> {
    let mut message = Vec::with_capacity(MAGIC.len() + 8 + payload.len());

    message.extend_from_slice(MAGIC);
    // Native byte order, which is what i3 and sway both write.
    message.extend_from_slice(&(payload.len() as u32).to_ne_bytes());
    message.extend_from_slice(&kind.to_ne_bytes());
    message.extend_from_slice(payload.as_bytes());

    stream.write_all(&message).await
}

async fn receive(stream: &mut UnixStream) -> io::Result<(u32, Vec<u8>)> {
    let mut header = [0u8; 14];
    stream.read_exact(&mut header).await?;

    if &header[..6] != MAGIC {
        return Err(io::Error::other(
            "reply did not start with the i3-ipc magic",
        ));
    }

    let length = u32::from_ne_bytes([header[6], header[7], header[8], header[9]]) as usize;
    let kind = u32::from_ne_bytes([header[10], header[11], header[12], header[13]]);

    let mut payload = vec![0u8; length];
    stream.read_exact(&mut payload).await?;

    Ok((kind, payload))
}

async fn command(payload: &str) -> io::Result<()> {
    let mut stream = connect().await?;
    send(&mut stream, RUN_COMMAND, payload).await?;
    receive(&mut stream).await?;
    Ok(())
}

// Only the fields the bar uses; serde ignores the rest of sway's JSON.
#[derive(Deserialize)]
struct RawWorkspace {
    num: i32,
    name: String,
    output: String,
    visible: bool,
    focused: bool,
}

async fn snapshot() -> io::Result<Workspaces> {
    let mut stream = connect().await?;

    send(&mut stream, GET_WORKSPACES, "").await?;
    let (_, payload) = receive(&mut stream).await?;
    let mut workspaces: Vec<RawWorkspace> =
        serde_json::from_slice(&payload).map_err(io::Error::other)?;

    // `get_workspaces` does not carry a window count, so it comes from the tree.
    send(&mut stream, GET_TREE, "").await?;
    let (_, payload) = receive(&mut stream).await?;
    let tree: Value = serde_json::from_slice(&payload).map_err(io::Error::other)?;

    // sway lists workspaces per output; sort so the pills keep a stable order.
    workspaces.sort_by_key(|workspace| workspace.num);

    Ok(workspaces
        .into_iter()
        .map(|workspace| Workspace {
            windows: count_windows(&tree, &workspace.name),
            id: workspace.num,
            name: workspace.name,
            monitor: workspace.output,
            visible: workspace.visible,
            focused: workspace.focused,
        })
        .collect())
}

/// Count the views on one workspace by walking the tree to its node.
fn count_windows(node: &Value, workspace: &str) -> u16 {
    if node.get("type").and_then(Value::as_str) == Some("workspace")
        && node.get("name").and_then(Value::as_str) == Some(workspace)
    {
        return count_views(node);
    }

    children(node)
        .map(|child| count_windows(child, workspace))
        .sum()
}

/// A view is a node with nothing inside it; containers only hold other nodes.
fn count_views(node: &Value) -> u16 {
    let mut leaves = children(node).peekable();

    if leaves.peek().is_none() {
        let kind = node.get("type").and_then(Value::as_str);
        return u16::from(matches!(kind, Some("con" | "floating_con")));
    }

    leaves.map(count_views).sum()
}

fn children(node: &Value) -> impl Iterator<Item = &Value> {
    ["nodes", "floating_nodes"]
        .into_iter()
        .filter_map(|key| node.get(key))
        .filter_map(Value::as_array)
        .flatten()
}

#[derive(Deserialize)]
struct RawOutput {
    name: String,
    active: bool,
}

async fn outputs() -> io::Result<Vec<String>> {
    let mut stream = connect().await?;
    send(&mut stream, GET_OUTPUTS, "").await?;
    let (_, payload) = receive(&mut stream).await?;
    let outputs: Vec<RawOutput> = serde_json::from_slice(&payload).map_err(io::Error::other)?;

    // sway keeps disabled and disconnected outputs in the list; a surface on
    // one of those would never be seen.
    Ok(outputs
        .into_iter()
        .filter(|output| output.active)
        .map(|output| output.name)
        .collect())
}

fn watch_outputs() -> impl Stream<Item = Vec<String>> {
    iced::stream::channel(4, async |mut output: mpsc::Sender<Vec<String>>| {
        loop {
            if let Err(error) = follow_outputs(&mut output).await {
                eprintln!("ricebar: sway ipc: {error}");
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}

async fn follow_outputs(sender: &mut mpsc::Sender<Vec<String>>) -> io::Result<()> {
    let mut events = connect().await?;
    send(&mut events, SUBSCRIBE, r#"["output"]"#).await?;
    receive(&mut events).await?;

    if sender.send(outputs().await?).await.is_err() {
        return Ok(());
    }

    loop {
        receive(&mut events).await?;

        if sender.send(outputs().await?).await.is_err() {
            return Ok(());
        }
    }
}

// Only the fields the bar uses; serde ignores the rest of sway's JSON.
#[derive(Deserialize)]
struct RawInput {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    xkb_active_layout_index: usize,
    #[serde(default)]
    xkb_layout_names: Vec<String>,
}

/// The layouts configured, from the keyboard most likely to be the one being
/// typed on.
async fn layouts() -> io::Result<Layouts> {
    let mut stream = connect().await?;
    send(&mut stream, GET_INPUTS, "").await?;
    let (_, payload) = receive(&mut stream).await?;
    let inputs: Vec<RawInput> = serde_json::from_slice(&payload).map_err(io::Error::other)?;

    let keyboards: Vec<&RawInput> = inputs
        .iter()
        .filter(|input| input.kind == "keyboard")
        .collect();

    // A power button and a set of hotkeys both report as keyboards, and both
    // carry a layout of their own. Prefer one with something to switch
    // between; a machine with several real keyboards configures them alike, so
    // any of those answers the question.
    keyboards
        .iter()
        .find(|input| input.xkb_layout_names.len() > 1)
        .or(keyboards.first())
        .map(|input| Layouts {
            names: input.xkb_layout_names.clone(),
            current: input.xkb_active_layout_index,
        })
        .ok_or_else(|| io::Error::other("no keyboard reports a layout"))
}

fn watch_layouts() -> impl Stream<Item = Layouts> {
    iced::stream::channel(4, async |mut output: mpsc::Sender<Layouts>| {
        loop {
            if let Err(error) = follow_layouts(&mut output).await {
                eprintln!("ricebar: sway ipc: {error}");
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}

async fn follow_layouts(sender: &mut mpsc::Sender<Layouts>) -> io::Result<()> {
    let mut events = connect().await?;
    send(&mut events, SUBSCRIBE, r#"["input"]"#).await?;
    receive(&mut events).await?;

    // An input event fires for a device being added, a libinput setting
    // changing and much else besides. Most leave the layout alone, and a bar
    // that redrew for each of them would be doing it for nothing.
    let mut sent: Option<Layouts> = None;

    loop {
        let found = layouts().await?;

        if sent.as_ref() != Some(&found) {
            sent = Some(found.clone());

            if sender.send(found).await.is_err() {
                return Ok(());
            }
        }

        receive(&mut events).await?;
    }
}

fn watch() -> impl Stream<Item = Workspaces> {
    iced::stream::channel(16, async |mut output: mpsc::Sender<Workspaces>| {
        loop {
            if let Err(error) = follow(&mut output).await {
                eprintln!("ricebar: sway ipc: {error}");
            }
            // The socket goes when sway exits. Keep retrying so the bar
            // recovers rather than silently freezing.
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}

async fn follow(output: &mut mpsc::Sender<Workspaces>) -> io::Result<()> {
    // A subscribed connection only carries events, so requests use their own.
    let mut events = connect().await?;
    send(&mut events, SUBSCRIBE, r#"["workspace","window"]"#).await?;

    // Subscribe before the first snapshot, so no change slips through the gap.
    receive(&mut events).await?;

    if output.send(snapshot().await?).await.is_err() {
        return Ok(());
    }

    loop {
        receive(&mut events).await?;

        // Re-query rather than patch state from the event payload: one round
        // trip is cheap, and the bar can never drift out of sync.
        if output.send(snapshot().await?).await.is_err() {
            return Ok(());
        }
    }
}
