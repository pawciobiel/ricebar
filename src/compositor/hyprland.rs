//! Hyprland IPC.
//!
//! Hyprland exposes two Unix sockets: `.socket.sock` takes one command and
//! returns a reply, and `.socket2.sock` streams `event>>data` lines. That is the
//! whole protocol, so it is spoken directly here rather than through the
//! `hyprland` crate, which is a beta release well behind current Hyprland.

use std::env;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, Stream};
use iced::{Subscription, Task};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use super::{Compositor, Workspace, Workspaces};

pub struct Hyprland;

impl Compositor for Hyprland {
    fn name(&self) -> &'static str {
        "hyprland"
    }

    fn workspaces(&self) -> Subscription<Workspaces> {
        // `Subscription::run` takes a plain fn pointer, not a closure, so state
        // may not be captured here. Everything needed comes from the environment.
        Subscription::run(watch)
    }

    fn focus(&self, id: i32) -> Task<()> {
        Task::future(async move {
            if let Err(error) = request(&format!("dispatch workspace {id}")).await {
                eprintln!("ricebar: could not focus workspace {id}: {error}");
            }
        })
    }

    fn outputs(&self) -> Subscription<Vec<String>> {
        Subscription::run(watch_monitors)
    }
}

pub fn available() -> bool {
    env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
}

fn socket_dir() -> io::Result<PathBuf> {
    let runtime = env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| io::Error::other("XDG_RUNTIME_DIR is not set"))?;
    let signature = env::var_os("HYPRLAND_INSTANCE_SIGNATURE")
        .ok_or_else(|| io::Error::other("HYPRLAND_INSTANCE_SIGNATURE is not set"))?;

    let mut path = PathBuf::from(runtime);
    path.push("hypr");
    path.push(signature);
    Ok(path)
}

/// Send one command and read its reply to EOF.
async fn request(command: &str) -> io::Result<String> {
    let mut socket = UnixStream::connect(socket_dir()?.join(".socket.sock")).await?;
    socket.write_all(command.as_bytes()).await?;

    let mut reply = String::new();
    socket.read_to_string(&mut reply).await?;
    Ok(reply)
}

// Only the fields the bar uses; serde ignores the rest of Hyprland's JSON.
#[derive(Deserialize)]
struct RawWorkspace {
    id: i32,
    name: String,
    monitor: String,
    windows: u16,
}

#[derive(Deserialize)]
struct RawMonitor {
    name: String,
    focused: bool,
    #[serde(rename = "activeWorkspace")]
    active_workspace: RawActiveWorkspace,
}

#[derive(Deserialize)]
struct RawActiveWorkspace {
    id: i32,
}

async fn snapshot() -> io::Result<Workspaces> {
    let raw = request("j/workspaces").await?;
    let mut workspaces: Vec<RawWorkspace> = serde_json::from_str(&raw).map_err(io::Error::other)?;

    let raw = request("j/monitors").await?;
    let monitors: Vec<RawMonitor> = serde_json::from_str(&raw).map_err(io::Error::other)?;

    // Hyprland returns workspaces in creation order, which would make pills
    // jump around as workspaces come and go.
    workspaces.sort_by_key(|workspace| workspace.id);

    let focused = monitors
        .iter()
        .find(|monitor| monitor.focused)
        .map(|monitor| monitor.active_workspace.id);

    Ok(workspaces
        .into_iter()
        .map(|workspace| Workspace {
            visible: monitors
                .iter()
                .any(|monitor| monitor.active_workspace.id == workspace.id),
            focused: focused == Some(workspace.id),
            id: workspace.id,
            name: workspace.name,
            monitor: workspace.monitor,
            windows: workspace.windows,
        })
        .collect())
}

async fn monitors() -> io::Result<Vec<String>> {
    let raw = request("j/monitors").await?;
    let monitors: Vec<RawMonitor> = serde_json::from_str(&raw).map_err(io::Error::other)?;
    Ok(monitors.into_iter().map(|monitor| monitor.name).collect())
}

/// Events after which the monitor list is stale.
fn changes_monitors(line: &str) -> bool {
    let Some((event, _)) = line.split_once(">>") else {
        return false;
    };

    matches!(
        event,
        "monitoradded" | "monitoraddedv2" | "monitorremoved" | "monitorremovedv2"
    )
}

fn watch_monitors() -> impl Stream<Item = Vec<String>> {
    iced::stream::channel(4, async |mut output: mpsc::Sender<Vec<String>>| {
        loop {
            if let Err(error) = follow_monitors(&mut output).await {
                eprintln!("ricebar: hyprland ipc: {error}");
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}

async fn follow_monitors(output: &mut mpsc::Sender<Vec<String>>) -> io::Result<()> {
    let events = UnixStream::connect(socket_dir()?.join(".socket2.sock")).await?;
    let mut lines = BufReader::new(events).lines();

    if output.send(monitors().await?).await.is_err() {
        return Ok(());
    }

    while let Some(line) = lines.next_line().await? {
        if !changes_monitors(&line) {
            continue;
        }
        if output.send(monitors().await?).await.is_err() {
            return Ok(());
        }
    }

    Ok(())
}

/// Events after which the snapshot is stale. Hyprland emits many more.
fn changes_workspaces(line: &str) -> bool {
    let Some((event, _)) = line.split_once(">>") else {
        return false;
    };

    matches!(
        event,
        "workspace"
            | "workspacev2"
            | "createworkspace"
            | "createworkspacev2"
            | "destroyworkspace"
            | "destroyworkspacev2"
            | "moveworkspace"
            | "moveworkspacev2"
            | "renameworkspace"
            | "focusedmon"
            | "focusedmonv2"
            | "openwindow"
            | "closewindow"
            | "movewindow"
            | "movewindowv2"
            | "monitoradded"
            | "monitoraddedv2"
            | "monitorremoved"
            | "monitorremovedv2"
    )
}

fn watch() -> impl Stream<Item = Workspaces> {
    iced::stream::channel(16, async |mut output: mpsc::Sender<Workspaces>| {
        loop {
            if let Err(error) = follow(&mut output).await {
                eprintln!("ricebar: hyprland ipc: {error}");
            }
            // The socket is gone when Hyprland exits or is restarted. Keep
            // retrying so the bar recovers instead of silently freezing.
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}

async fn follow(output: &mut mpsc::Sender<Workspaces>) -> io::Result<()> {
    // Subscribe before the first snapshot, so no change slips through the gap.
    let events = UnixStream::connect(socket_dir()?.join(".socket2.sock")).await?;
    let mut lines = BufReader::new(events).lines();

    if output.send(snapshot().await?).await.is_err() {
        return Ok(());
    }

    while let Some(line) = lines.next_line().await? {
        if !changes_workspaces(&line) {
            continue;
        }
        // Re-query rather than patch state from the event payload: one socket
        // round trip is cheap, and the bar can never drift out of sync.
        if output.send(snapshot().await?).await.is_err() {
            return Ok(());
        }
    }

    Ok(())
}
