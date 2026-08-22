#!/bin/sh
# Network status, as a streaming module.
#
#   [[module.custom]]
#   name = "network"
#   exec = "~/.config/ricebar/scripts/network.sh"
#   stream = true
#
# Reports whatever carries the default route, so docker and bridge interfaces
# are ignored without having to name them. Uses ip and iw rather than nmcli,
# which is not installed everywhere NetworkManager is.

EVERY="${EVERY:-5}"

ETHERNET="\356\236\266"   # U+E7B6 ethernet
WIFI="\357\207\253"       # U+F1EB wifi
OFFLINE="\357\207\232"    # U+F1DA disconnected

report() {
    # The interface with the default route is the one actually in use.
    interface=$(ip route show default 2>/dev/null | awk '{print $5; exit}')

    if [ -z "$interface" ]; then
        printf '{"text":"%b offline","tooltip":"No default route"}\n' "$OFFLINE"
        return
    fi

    address=$(ip -o -4 addr show dev "$interface" scope global 2>/dev/null |
        awk '{print $4; exit}')
    gateway=$(ip route show default 2>/dev/null | awk '{print $3; exit}')

    link=$(iw dev "$interface" link 2>/dev/null)

    if [ -n "$link" ] && [ "$link" != "Not connected." ]; then
        essid=$(printf '%s' "$link" | awk '/SSID:/ {sub(/.*SSID: /, ""); print; exit}')
        # Signal runs about -30 dBm (excellent) to -90 (unusable).
        dbm=$(printf '%s' "$link" | awk '/signal:/ {print $2; exit}')
        quality=$(awk -v d="${dbm:--90}" 'BEGIN {
            q = (d + 90) / 60 * 100
            printf "%d", (q < 0 ? 0 : (q > 100 ? 100 : q))
        }')

        printf '{"text":"%b %s","tooltip":"%s on %s\\n%s via %s\\nSignal %s dBm","percentage":%s}\n' \
            "$WIFI" "$essid" "$essid" "$interface" "$address" "$gateway" "$dbm" "$quality"
    else
        printf '{"text":"%b %s","tooltip":"Wired on %s\\n%s via %s","percentage":100}\n' \
            "$ETHERNET" "$interface" "$interface" "$address" "$gateway"
    fi
}

while :; do
    report
    sleep "$EVERY"
done
