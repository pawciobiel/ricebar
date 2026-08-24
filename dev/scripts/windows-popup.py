#!/usr/bin/env python3
"""A popup listing open windows; choosing one focuses it.

Prints a popup for ricebar to draw:

    {"items": [{"label": "2  Alacritty", "exec": "hyprctl dispatch ..."}]}

    [[module.custom]]
    name = "windows"
    label = ""
    popup = "~/.config/ricebar/scripts/windows-popup.py"

ricebar knows nothing about windows: it draws what this prints. Which
compositor is running is the script's business, not the bar's -- which is
also why the popup scripts need not be shell at all. `popup` is handed to a
shell, so anything executable will do.
"""

import json
import os
import shutil
import subprocess
import sys

# Enough to tell windows apart, not enough to stretch the popup off screen.
APP = 14
TITLE = 38
MOST = 20


def run(*command):
    try:
        return subprocess.run(
            command, capture_output=True, text=True, timeout=5
        ).stdout
    except (OSError, subprocess.SubprocessError):
        return ""


def hyprland():
    windows = json.loads(run("hyprctl", "-j", "clients") or "[]")
    windows.sort(key=lambda w: (w["workspace"]["id"], w.get("class") or ""))

    return [
        {
            "label": "{}  {}  {}".format(
                w["workspace"]["id"],
                (w.get("class") or "?")[:APP],
                (w.get("title") or "")[:TITLE],
            ),
            "exec": "hyprctl dispatch focuswindow address:{}".format(w["address"]),
        }
        for w in windows
    ]


def sway():
    def walk(node):
        if node.get("type") in ("con", "floating_con") and node.get("name"):
            yield node
        for key in ("nodes", "floating_nodes"):
            for child in node.get(key, []):
                yield from walk(child)

    tree = json.loads(run("swaymsg", "-t", "get_tree") or "{}")

    return [
        {
            "label": "{}  {}".format(
                (
                    w.get("app_id")
                    or w.get("window_properties", {}).get("class")
                    or "?"
                )[:APP],
                (w.get("name") or "")[:TITLE],
            ),
            "exec": "swaymsg [con_id={}] focus".format(w["id"]),
        }
        for w in walk(tree)
    ]


def niri():
    windows = json.loads(run("niri", "msg", "-j", "windows") or "[]")
    windows.sort(key=lambda w: (w.get("workspace_id") or 0, w.get("app_id") or ""))

    return [
        {
            "label": "{}  {}".format(
                (w.get("app_id") or "?")[:APP],
                (w.get("title") or "")[:TITLE],
            ),
            "exec": "niri msg action focus-window --id {}".format(w["id"]),
        }
        for w in windows
    ]


def windows():
    """Whichever compositor is actually running.

    A nested session inherits the host's environment, so more than one of
    these can look present at once. Ask the most specific first.
    """
    if os.environ.get("NIRI_SOCKET") and shutil.which("niri"):
        return niri()
    if os.environ.get("SWAYSOCK") and shutil.which("swaymsg"):
        return sway()
    if shutil.which("hyprctl"):
        return hyprland()
    return []


def main():
    items = windows() or [{"label": "no windows", "exec": ""}]
    json.dump({"items": items[:MOST]}, sys.stdout)


if __name__ == "__main__":
    main()
