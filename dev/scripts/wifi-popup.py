#!/usr/bin/env python3
"""A popup for Wi-Fi: turn the radio on or off, scan, and join a network.

Prints a popup for ricebar to draw:

    {"items": [{"label": "Turn Wi-Fi off", "exec": "iwctl device wlan0 ..."}]}

    [[module.custom]]
    name = "wifi"
    exec = "~/.config/ricebar/scripts/wifi.sh"
    stream = true
    popup = "~/.config/ricebar/scripts/wifi-popup.py"

Talks to iwd through iwctl, and to nothing else -- on wpa_supplicant or
NetworkManager there is no iwctl to call, so write your own popup printing the
same JSON and name that in `popup` instead. The bar half, wifi.sh, reads `ip`
and `iw` and needs no such thing.

No root needed: iwd's D-Bus policy admits the `wheel` and `netdev` groups, so a
member of either can scan, connect and power the device.

A network iwd already has a passphrase for connects straight from the list. One
it does not cannot -- iwd would need to ask for the passphrase, and a menu has
nowhere to type it -- so that entry opens a terminal running the same command
interactively instead.
"""

import json
import os
import re
import shutil
import subprocess
import sys

# Enough of an SSID to tell one from another, not enough to stretch the popup
# off the screen.
NAME = 22
MOST = 20

# Terminals to run an interactive iwctl in, best first.
TERMINALS = ("alacritty", "foot", "kitty", "wezterm", "xterm")

# iwctl paints its output, and the marker for the connected network is a ">"
# in the first column.
PAINT = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")


def run(*command):
    try:
        return subprocess.run(
            command, capture_output=True, text=True, timeout=10
        ).stdout
    except (OSError, subprocess.SubprocessError):
        return ""


def plain(line):
    return PAINT.sub("", line).rstrip()


def device():
    """The first wireless interface, named as the kernel names it."""
    try:
        for name in sorted(os.listdir("/sys/class/net")):
            if os.path.isdir(f"/sys/class/net/{name}/wireless"):
                return name
    except OSError:
        pass
    return ""


def powered(dev):
    """Whether the radio is on. iwd takes the interface down when it is not.

    IFF_UP rather than operstate, which reports the link: an interface that is
    up but not associated still says "down", so reading that offered "Turn
    Wi-Fi on" for a radio already on, and the click did nothing.
    """
    try:
        with open(f"/sys/class/net/{dev}/flags") as flags:
            return int(flags.read().strip(), 16) & 1 == 1
    except (OSError, ValueError):
        return False


# The security column, which is what marks where the SSID ends. Both tables put
# it directly after the name, and an SSID may hold spaces while these never do.
SECURITY = ("psk", "open", "8021x", "wep")


def split(line):
    """An iwctl table row as (ssid, security, rest), or None if it is not one.

    Counting columns from either end goes wrong: an SSID may hold spaces, and
    `known-networks list` ends with a date that holds them too. The security
    keyword is the fixed point, so the name is whatever precedes it.
    """
    fields = line.split()

    for at, field in enumerate(fields):
        if field in SECURITY and at > 0:
            return " ".join(fields[:at]), field, fields[at + 1 :]

    return None


def networks(dev):
    """What `get-networks` lists, as (ssid, security, connected)."""
    found = []

    for raw in run("iwctl", "station", dev, "get-networks").splitlines():
        line = plain(raw)
        connected = line.lstrip().startswith(">")
        if connected:
            line = line.lstrip()[1:]

        row = split(line)
        if row:
            found.append((row[0], row[1], connected))

    return found


def known():
    """SSIDs iwd already holds a passphrase or profile for."""
    names = set()

    for raw in run("iwctl", "known-networks", "list").splitlines():
        row = split(plain(raw))
        if row:
            names.add(row[0])

    return names


def elide(ssid):
    """An SSID cut to NAME characters, from the middle rather than the end.

    A dual-band access point names its two radios alike but for a suffix, so
    cutting the tail drops the one part that tells them apart: "...B4" and
    "...B45G" both came out as the same string, and the menu read as an offer
    to join the network already connected.
    """
    if len(ssid) <= NAME:
        return ssid

    head = (NAME - 1) // 2
    tail = NAME - 1 - head

    return ssid[:head] + "…" + ssid[len(ssid) - tail :]


def terminal():
    return next((app for app in TERMINALS if shutil.which(app)), None)


def quote(value):
    return "'" + value.replace("'", "'\\''") + "'"


def items():
    dev = device()

    if not dev:
        return [{"label": "no wireless interface", "exec": ""}]

    if not shutil.which("iwctl"):
        return [{"label": "iwctl not installed", "exec": ""}]

    on = powered(dev)

    rows = [
        {
            "label": "Turn Wi-Fi off" if on else "Turn Wi-Fi on",
            "exec": "iwctl device {} set-property Powered {}".format(
                dev, "off" if on else "on"
            ),
        }
    ]

    # Nothing else is worth offering while the radio is off: a scan would find
    # nothing and connecting would fail.
    if not on:
        return rows

    remembered = known()
    term = terminal()

    for ssid, security, connected in networks(dev):
        short = elide(ssid)

        if connected:
            rows.append(
                {
                    "label": "Disconnect {}".format(short),
                    "exec": "iwctl station {} disconnect".format(dev),
                }
            )
            continue

        joining = "iwctl station {} connect {}".format(dev, quote(ssid))

        if ssid in remembered or security == "open":
            rows.append({"label": "Connect {}".format(short), "exec": joining})
        elif security == "8021x":
            # Not a passphrase prompt away: 802.1X wants a .8021x profile and
            # its certificates, which is a text editor's job, not a menu's.
            rows.append({"label": "{} needs 802.1X setup".format(short), "exec": ""})
        elif term:
            # iwd has to ask for the passphrase, and a menu has nowhere to
            # type it.
            rows.append(
                {
                    # Said in a word, not as a trailing "...", which reads as a
                    # name cut short -- next to a real elision it hid the very
                    # suffix that separates a 5 GHz network from its twin.
                    "label": "Join {} (passphrase)".format(short),
                    "exec": "{} -e sh -c {}".format(
                        term, quote(joining + "; echo; read -r _")
                    ),
                }
            )
        else:
            rows.append({"label": "{} needs a passphrase".format(short), "exec": ""})

    rows.append(
        {
            "label": "Scan again",
            "exec": "iwctl station {} scan".format(dev),
        }
    )

    return rows


def main():
    json.dump({"items": items()[:MOST]}, sys.stdout)


if __name__ == "__main__":
    main()
