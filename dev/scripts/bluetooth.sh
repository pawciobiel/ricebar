#!/bin/sh
# Bluetooth, as a streaming module: whether the adapter is on, with whatever is
# connected named on hover. Clicking it opens the popup beside this script.
#
#   [[module.custom]]
#   name = "bluetooth"
#   exec = "~/.config/ricebar/scripts/bluetooth.sh"
#   stream = true
#   popup = "~/.config/ricebar/scripts/bluetooth-popup.py"
#
# Talks to BlueZ through bluetoothctl, since ricebar speaks no D-Bus. Nothing
# here needs root -- but the radio must not be rfkill-blocked, because BlueZ
# refuses to power the adapter on while it is and only root can lift that:
#
#   rfkill unblock bluetooth
#
# Once is enough where the rfkill service saves its state on stop, which is the
# default on Alpine and Gentoo.

# Nerd Font U+F00B2 (bluetooth crossed out), U+F00AF (bluetooth) and U+F00B1
# (bluetooth with dots), as UTF-8 bytes so this file stays plain ASCII. All
# three are Material Design glyphs, which matters: a Font Awesome one beside
# them is drawn at a different weight and size.
OFF="${OFF:-\363\260\202\262}"
ON="${ON:-\363\260\202\257}"
LINKED="${LINKED:-\363\260\202\261}"

# The state is reported as a percentage -- 0 off, 50 on or changing, 100
# connected -- which is what lets `colors` in config tell them apart at a
# glance. At bar size the slash through the "off" glyph is easy to miss.

# A device is named by whoever made it, and that name reaches the bar through
# JSON. A quote or a backslash in one would break the line ricebar parses.
clean() {
    tr -d '"\\' | tr -s ' '
}

# One line of JSON for however things stand.
state() {
    status=$(bluetoothctl show 2>/dev/null)

    # No controller at all, or bluetoothd is not running to answer for one.
    if [ -z "$status" ]; then
        printf '{"text":"%b","tooltip":"No Bluetooth adapter","percentage":0}' "$OFF"
        return
    fi

    # PowerState carries the two transitional values as well as on and off.
    # Older BlueZ has only Powered, so fall back to that.
    power=$(printf '%s\n' "$status" | awk '/PowerState:/ {print $2; exit}')

    if [ -z "$power" ]; then
        if [ "$(printf '%s\n' "$status" | awk '/Powered:/ {print $2; exit}')" = "yes" ]; then
            power=on
        else
            power=off
        fi
    fi

    case "$power" in
        # Turning on or off. BlueZ says which itself, which is a better answer
        # than animating the icon and hoping.
        *-enabling)
            printf '{"text":"%b","tooltip":"Bluetooth turning on","percentage":50}' "$ON"
            return
            ;;
        *-disabling)
            printf '{"text":"%b","tooltip":"Bluetooth turning off","percentage":50}' "$ON"
            return
            ;;
        on) ;;
        *)
            printf '{"text":"%b","tooltip":"Bluetooth off","percentage":0}' "$OFF"
            return
            ;;
    esac

    # One device per line, as "Device AA:BB:CC:DD:EE:FF The Name".
    names=$(bluetoothctl devices Connected 2>/dev/null | cut -d' ' -f3- | clean)

    if [ -z "$names" ]; then
        printf '{"text":"%b","tooltip":"Bluetooth on\\nNothing connected","percentage":50}' "$ON"
        return
    fi

    # The bar keeps to the glyph. Which device it is belongs on hover and in the
    # popup, where there is room for the whole name.
    detail=$(printf '%s\n' "$names" | awk 'NR > 1 {printf "\\n"} {printf "%s", $0}')

    printf '{"text":"%b","tooltip":"Bluetooth on\\n%s","percentage":100}' "$LINKED" "$detail"
}

state
echo

# A stdin that never reaches EOF, and no helper process to leave behind.
#
# bluetoothctl reads EOF as "quit", so given the stdin a bar hands it the
# monitor stops before printing anything -- /dev/null is read once and that is
# that. This shell holds the write end of a fifo instead, and hands the monitor
# a read-only handle on it, so it blocks there rather than quitting. The
# read-write open comes first because opening a fifo read-only blocks until a
# writer arrives.
#
# It does not follow that the monitor stops when this script does: bluetoothctl
# stops reading stdin once its own event loop is up, so closing the write end
# is not noticed. It goes on the next thing it prints, when its stdout pipe is
# found closed -- the same transient orphan the other streaming scripts leave,
# and the same cure: see the process-group note in TODO.md.
FIFO=$(mktemp -u) || exit 1
mkfifo "$FIFO" || exit 1
exec 3<>"$FIFO"
exec 4<"$FIFO"
rm -f "$FIFO"

# Events rather than a timer: powering the adapter takes about a third of a
# second, and a poll slow enough to be cheap is slow enough to look broken.
last=""

# Both halves of the pipeline drop the write handle: either one holding it
# would keep the monitor alive after this shell is gone.
bluetoothctl --monitor <&4 3>&- 2>/dev/null | while read -r line; do
    case "$line" in
        # The controller powering up or down, and devices coming and going.
        # Nothing else: during a scan every device reports its signal strength
        # over and over, and none of that changes what the bar shows.
        *PowerState:* | *Powered:* | *Connected:*)
            current=$(state)

            if [ "$current" != "$last" ]; then
                last="$current"
                printf '%s\n' "$current"
            fi
            ;;
    esac
done 3>&- 4>&-
