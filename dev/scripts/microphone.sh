#!/bin/sh
# The microphone, as a streaming module: prints a line whenever it changes,
# then waits. Nothing is polled -- pactl blocks until PipeWire says something
# happened, so an idle machine costs nothing.
#
#   [[module.custom]]
#   name = "microphone"
#   exec = "~/.config/ricebar/scripts/microphone.sh"
#   stream = true
#   on-click = "pactl set-source-mute @DEFAULT_SOURCE@ toggle"
#   on-scroll-up = "pactl set-source-volume @DEFAULT_SOURCE@ +5%"
#   on-scroll-down = "pactl set-source-volume @DEFAULT_SOURCE@ -5%"
#
# The icon is chosen here rather than by the module's `icons` list, because
# muted is not a level: a list is indexed by percentage, so a quiet microphone
# would be drawn as a muted one. Override either glyph from the config:
#
#   exec = "ON='(o)' OFF='(x)' ~/.config/ricebar/scripts/microphone.sh"
#
# Nerd Font U+F130 (microphone) and U+F131 (microphone with a slash), written
# as the UTF-8 bytes so this file stays plain ASCII.
ON="${ON:-\357\204\260}"
OFF="${OFF:-\357\204\261}"

# Which microphone, for the tooltip. `pactl list` is only read when something
# actually changes, so this costs nothing while nothing is happening.
description() {
    pactl list sources 2>/dev/null | awk -v want="$1" '
        $1 == "Name:" { name = $2 }
        $1 == "Description:" {
            sub(/^[^:]*: /, "")
            if (name == want) { print; exit }
        }'
}

report() {
    volume=$(pactl get-source-volume @DEFAULT_SOURCE@ 2>/dev/null |
        awk 'NR==1 {for (i = 1; i <= NF; i++) if ($i ~ /%$/) {gsub(/%/, "", $i); print $i; exit}}')
    muted=$(pactl get-source-mute @DEFAULT_SOURCE@ 2>/dev/null | awk '{print $2}')

    [ -n "$volume" ] || return 1

    name=$(pactl get-default-source 2>/dev/null)
    detail=$(description "$name")
    [ -n "$detail" ] || detail="$name"

    if [ "$muted" = "yes" ]; then
        printf '{"text":"%b","tooltip":"Microphone muted\\n%s","percentage":0}\n' \
            "$OFF" "$detail"
    else
        printf '{"text":"%b %s%%","tooltip":"Microphone %s%%\\n%s","percentage":%s}\n' \
            "$ON" "$volume" "$volume" "$detail" "$volume"
    fi
}

# Say where things stand before waiting for the first change. A machine with no
# capture device at all says so once, rather than leaving a silent gap in the
# bar that looks like a broken script.
report || printf '{"text":"%b","tooltip":"No microphone found"}\n' "$OFF"

pactl subscribe 2>/dev/null | while read -r line; do
    case "$line" in
        # `source-output` events fire for every application that starts
        # listening, and are not what this shows -- but a stray refresh is
        # cheaper than missing a real change, so they are let through.
        *"'change'"*source*) report ;;
    esac
done
