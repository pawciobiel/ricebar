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
# Nerd Font U+F036C (microphone) and U+F036D (microphone crossed out), written
# as the UTF-8 bytes so this file stays plain ASCII.
#
# The Material Design pair rather than Font Awesome's U+F130 and U+F131, which
# these were: the crossed Font Awesome microphone draws 18px of ink at font-size
# 16, against 8 or 9 for the Bluetooth glyphs beside it, and its slash reaches
# 3px past the module's own background. Measured on the headless rig.
ON="${ON:-\363\260\215\254}"
OFF="${OFF:-\363\260\215\255}"

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

# Say where things stand before waiting for the first change. The bar starts
# with the session, so give PipeWire a few seconds before deciding there is no
# capture device: saying so wrongly at login would stand until the next change.
tries=5

until report; do
    tries=$((tries - 1))

    if [ "$tries" -le 0 ]; then
        printf '{"text":"%b","tooltip":"No microphone found"}\n' "$OFF"
        break
    fi

    sleep 1
done

pactl subscribe 2>/dev/null | while read -r line; do
    case "$line" in
        # Every event on a source, not only 'change': a microphone plugged in
        # after this started says so with 'new'. `source-output` events fire
        # for each application that starts listening and say nothing about the
        # microphone itself; they read "on source-output #N", so they are not
        # matched. A change of *server* is how the default source being
        # switched arrives.
        *"on source #"* | *"on server #"*) report ;;
    esac
done
