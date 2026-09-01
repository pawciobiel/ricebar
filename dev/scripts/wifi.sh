#!/bin/sh
# Wi-Fi, as a streaming module: whether the radio is on, and how good the
# signal is. Clicking it opens the popup beside this script.
#
#   [[module.custom]]
#   name = "wifi"
#   exec = "~/.config/ricebar/scripts/wifi.sh"
#   stream = true
#   popup = "~/.config/ricebar/scripts/wifi-popup.py"
#
# Nothing here talks to iwd. The radio being on shows up as the interface being
# up, and `iw` reports the rest -- so this works the same under iwd,
# wpa_supplicant or NetworkManager. Only the popup needs iwctl.
#
# The `network` module shows which interface holds the default route and names
# the SSID. This one is about the radio itself, so it keeps to a glyph and puts
# the name on hover.

EVERY="${EVERY:-30}"

# Nerd Font Material Design glyphs, as UTF-8 bytes so this file stays plain
# ASCII. Same family as the Bluetooth module, which is what makes them match in
# weight and size.
#
#   U+F092E wifi-strength-off-outline   the radio is off
#   U+F092F wifi-strength-outline       on, but not associated
#   U+F091F .. U+F0928  strength 1 to 4
OFF="${OFF:-\363\260\244\256}"
IDLE="${IDLE:-\363\260\244\257}"
BARS="${BARS:-\363\260\244\237 \363\260\244\242 \363\260\244\245 \363\260\244\250}"

# An SSID is named by whoever runs the access point and reaches the bar through
# JSON, so a quote or a backslash in one would break the line ricebar parses.
clean() {
    tr -d '"\\' | tr -s ' '
}

# The first wireless interface. Named rather than assumed, since it is wlan0 on
# one machine and wlp4s0 on the next.
device() {
    for path in /sys/class/net/*/wireless; do
        [ -d "$path" ] || continue
        path=${path%/wireless}
        printf '%s\n' "${path##*/}"
        return
    done
}

# Whether the radio is on, from IFF_UP in the interface flags.
#
# Not operstate: that reports the *link*, and stays "down" for as long as the
# radio is on but not associated -- which is most of the time, and made the
# module say the radio was off whenever it was merely idle.
up() {
    flags=$(cat "/sys/class/net/$1/flags" 2>/dev/null) || return 1
    [ $((flags & 1)) -eq 1 ]
}

# Signal runs about -30 dBm (excellent) to -90 (unusable).
quality() {
    awk -v d="${1:--90}" 'BEGIN {
        q = (d + 90) / 60 * 100
        printf "%d", (q < 0 ? 0 : (q > 100 ? 100 : q))
    }'
}

# One of the strength glyphs, by quality. However many BARS holds.
bars() {
    # Read the argument before `set --` replaces it.
    level=$1
    set -- $BARS
    [ $# -gt 0 ] || return

    index=$(awk -v q="$level" -v n=$# 'BEGIN {
        i = int(q * n / 100) + 1
        print (i > n ? n : i)
    }')
    [ -n "$index" ] || index=1

    # %b, because BARS holds the glyphs as octal escapes.
    eval "printf '%b' \"\${$index}\""
}

state() {
    dev=$(device)

    if [ -z "$dev" ]; then
        printf '{"text":"","icon":"%b","tooltip":"No wireless interface","percentage":0}' "$OFF"
        return
    fi

    # iwd takes the interface down when the device is powered off, so IFF_UP
    # is the radio state.
    if ! up "$dev"; then
        printf '{"text":"","icon":"%b","tooltip":"Wi-Fi off (%s)","percentage":0}' "$OFF" "$dev"
        return
    fi

    link=$(iw dev "$dev" link 2>/dev/null)

    if [ -z "$link" ] || [ "$link" = "Not connected." ]; then
        printf '{"text":"","icon":"%b","tooltip":"Wi-Fi on, not connected","percentage":50}' "$IDLE"
        return
    fi

    essid=$(printf '%s\n' "$link" | awk '/SSID:/ {sub(/.*SSID: /, ""); print; exit}' | clean)
    dbm=$(printf '%s\n' "$link" | awk '/signal:/ {print $2; exit}')
    rate=$(printf '%s\n' "$link" | awk '/tx bitrate:/ {print $3, $4; exit}')
    level=$(quality "$dbm")

    printf '{"text":"","icon":"%s","tooltip":"%s\\n%s dBm on %s%s","percentage":%s}' \
        "$(bars "$level")" \
        "${essid:-connected}" "${dbm:--}" "$dev" \
        "${rate:+\\n$rate}" \
        "$level"
}

emit() {
    current=$(state)

    if [ "$current" != "$last" ]; then
        last="$current"
        printf '%s\n' "$current"
    fi
}

last=""
emit

# Wait for the link to change rather than polling: `ip monitor` needs no
# privileges and reports the interface going up or down the moment iwd does it,
# which is what a toggle from the popup looks like. The timeout is the ceiling,
# so a signal that drifts while associated is still followed.
while :; do
    timeout "$EVERY" ip monitor link 2>/dev/null | head -1 >/dev/null 2>&1
    emit
done
