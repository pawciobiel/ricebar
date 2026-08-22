#!/bin/sh
# A popup listing open windows; choosing one focuses it.
#
# Prints a popup for ricebar to draw:
#
#   {"items": [{"label": "2  Alacritty", "exec": "hyprctl dispatch ..."}]}
#
#   [[module.custom]]
#   name = "windows"
#   label = ""
#   popup = "~/.config/ricebar/scripts/windows-popup.sh"
#
# ricebar knows nothing about windows: it draws what this prints. Which
# compositor is running is the script's business, not the bar's.

python3 - "$@" <<'PYTHON'
import json
import os
import shutil
import subprocess
import sys


def run(*command):
    try:
        return subprocess.run(command, capture_output=True, text=True, timeout=5).stdout
    except (OSError, subprocess.SubprocessError):
        return ""


def hyprland():
    windows = json.loads(run("hyprctl", "-j", "clients") or "[]")
    windows.sort(key=lambda w: (w["workspace"]["id"], w.get("class") or ""))
    return [
        {
            "label": f'{w["workspace"]["id"]}  {(w.get("class") or "?")[:14]}  {(w.get("title") or "")[:38]}',
            "exec": f'hyprctl dispatch focuswindow address:{w["address"]}',
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
            "label": f'{(w.get("app_id") or w.get("window_properties", {}).get("class") or "?")[:14]}  {(w.get("name") or "")[:38]}',
            "exec": f'swaymsg [con_id={w["id"]}] focus',
        }
        for w in walk(tree)
    ]


def niri():
    windows = json.loads(run("niri", "msg", "-j", "windows") or "[]")
    windows.sort(key=lambda w: (w.get("workspace_id") or 0, w.get("app_id") or ""))
    return [
        {
            "label": f'{(w.get("app_id") or "?")[:14]}  {(w.get("title") or "")[:38]}',
            "exec": f'niri msg action focus-window --id {w["id"]}',
        }
        for w in windows
    ]


if os.environ.get("NIRI_SOCKET") and shutil.which("niri"):
    items = niri()
elif os.environ.get("SWAYSOCK") and shutil.which("swaymsg"):
    items = sway()
elif shutil.which("hyprctl"):
    items = hyprland()
else:
    items = []

if not items:
    items = [{"label": "no windows", "exec": ""}]

json.dump({"items": items[:20]}, sys.stdout)
PYTHON
