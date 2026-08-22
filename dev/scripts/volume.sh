#!/bin/sh
# A streaming module: prints a line whenever the volume changes, then waits.
#
# Use with `stream = true`, which keeps this running and reads a line per
# update. Nothing is polled -- pactl blocks until PipeWire says something
# happened, so an idle machine costs nothing.
#
#   [[module.custom]]
#   name = "volume"
#   exec = "~/.config/ricebar/scripts/volume.sh"
#   stream = true
#   icons = ["", "", ""]
#   format = "{icon} {value}"

report() {
    volume=$(pactl get-sink-volume @DEFAULT_SINK@ 2>/dev/null |
        awk 'NR==1 {for (i = 1; i <= NF; i++) if ($i ~ /%$/) {gsub(/%/, "", $i); print $i; exit}}')
    muted=$(pactl get-sink-mute @DEFAULT_SINK@ 2>/dev/null | awk '{print $2}')

    [ -n "$volume" ] || return

    if [ "$muted" = "yes" ]; then
        printf '{"text":"muted","tooltip":"Muted","percentage":0}\n'
    else
        printf '{"text":"%s%%","tooltip":"Volume %s%%","percentage":%s}\n' \
            "$volume" "$volume" "$volume"
    fi
}

# Say where things stand before waiting for the first change.
report

pactl subscribe 2>/dev/null | while read -r line; do
    case "$line" in
        *"'change'"*sink*) report ;;
    esac
done
