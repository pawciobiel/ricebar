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

    [ -n "$volume" ] || return 1

    if [ "$muted" = "yes" ]; then
        printf '{"text":"muted","tooltip":"Muted","percentage":0}\n'
    else
        printf '{"text":"%s%%","tooltip":"Volume %s%%","percentage":%s}\n' \
            "$volume" "$volume" "$volume"
    fi
}

# Say where things stand before waiting for the first change.
#
# The bar starts with the session, so PipeWire may not have a sink yet. Waiting
# for one matters more than it looks: a module that has never printed draws
# nothing at all, so giving up here leaves a gap in the bar that only the next
# volume change fills -- which is why this looked like a missing icon rather
# than a broken script.
tries=5

until report; do
    tries=$((tries - 1))

    if [ "$tries" -le 0 ]; then
        printf '{"text":"no sink","tooltip":"No audio sink. Is PipeWire running?"}\n'
        break
    fi

    sleep 1
done

pactl subscribe 2>/dev/null | while read -r line; do
    case "$line" in
        # Every event on a sink, not only 'change': a sink that appears after
        # this started is the case above, and 'new' is how it says so.
        # `sink-input` events read "on sink-input #N", so they are not matched.
        # A change of *server* is how the default sink being switched arrives.
        *"on sink #"* | *"on server #"*) report ;;
    esac
done
