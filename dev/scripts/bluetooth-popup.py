#!/usr/bin/env python3
"""A popup for Bluetooth: turn the adapter on or off, connect what is paired.

Prints a popup for ricebar to draw:

    {"items": [{"label": "Turn Bluetooth off", "exec": "bluetoothctl power off"}]}

    [[module.custom]]
    name = "bluetooth"
    exec = "~/.config/ricebar/scripts/bluetooth.sh"
    stream = true
    popup = "~/.config/ricebar/scripts/bluetooth-popup.py"

Pairing is deliberately not here. It needs an agent to answer the passkey
prompt, which is a dialogue rather than a line in a menu, so the last entry
opens whichever Bluetooth manager is installed and leaves that job to it.
"""

import json
import shutil
import subprocess
import sys

# Enough of a device name to tell one from another, not enough to stretch the
# popup off the screen.
NAME = 22
MOST = 20

# `bluetoothctl scan on` on its own asks for discovery and exits, and discovery
# stops with the client that asked -- so the timeout is what holds it open.
# Anything found this way shows up as a paired-or-not device next time this
# popup is opened.
SCAN = 12

# Managers that can pair a device, best first. All are windowed applications: a
# terminal one would need a terminal named here, which is a guess too far.
MANAGERS = ("blueman-manager", "overskride", "blueberry")


def run(*command):
    try:
        return subprocess.run(command, capture_output=True, text=True, timeout=5).stdout
    except (OSError, subprocess.SubprocessError):
        return ""


def powered():
    """Whether the adapter is on, or None when there is no adapter to ask."""
    for line in run("bluetoothctl", "show").splitlines():
        if "Powered:" in line:
            return line.split()[-1] == "yes"
    return None


def devices(kind):
    """The `Paired` or `Connected` devices, as (address, name) pairs.

    bluetoothctl prints one per line: "Device AA:BB:CC:DD:EE:FF The Name".
    A device with no name is listed under its address, which is still enough
    to connect it.
    """
    found = []

    for line in run("bluetoothctl", "devices", kind).splitlines():
        parts = line.split(maxsplit=2)

        if len(parts) < 2 or parts[0] != "Device":
            continue

        address = parts[1]
        name = parts[2] if len(parts) > 2 else address
        found.append((address, name[:NAME]))

    return found


def items():
    on = powered()

    if on is None:
        return [{"label": "no adapter", "exec": ""}]

    rows = [
        {
            "label": "Turn Bluetooth off" if on else "Turn Bluetooth on",
            "exec": "bluetoothctl power {}".format("off" if on else "on"),
        }
    ]

    # Nothing else is worth offering while the adapter is off: connecting would
    # fail and a scan would find nothing.
    if not on:
        return rows

    connected = dict(devices("Connected"))

    for address, name in devices("Paired"):
        linked = address in connected
        rows.append(
            {
                "label": "{} {}".format("Disconnect" if linked else "Connect", name),
                "exec": "bluetoothctl {} {}".format(
                    "disconnect" if linked else "connect", address
                ),
            }
        )

    rows.append(
        {
            "label": "Scan for {}s".format(SCAN),
            "exec": "bluetoothctl --timeout {} scan on".format(SCAN),
        }
    )

    manager = next((app for app in MANAGERS if shutil.which(app)), None)
    rows.append(
        {"label": "Pair a device...", "exec": manager}
        if manager
        else {"label": "Pair: install blueman", "exec": ""}
    )

    return rows


def main():
    json.dump({"items": items()[:MOST]}, sys.stdout)


if __name__ == "__main__":
    main()
