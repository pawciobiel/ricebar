#!/bin/sh
# Weather from wttr.in, as a streaming module.
#
# Streams rather than polls so ricebar re-runs nothing: this sleeps between
# fetches and prints a line each time, which also means one process instead of
# a fork every interval.
#
#   [[module.custom]]
#   name = "weather"
#   exec = "~/.config/ricebar/scripts/weather.sh"
#   stream = true
#
# Set LOCATION to a city, or leave it empty to let wttr.in guess from the IP.

LOCATION="${LOCATION:-}"
EVERY="${EVERY:-900}"   # seconds between fetches; be kind to a free service

# Nerd Font weather glyphs, written as octal UTF-8 so this file stays plain
# ASCII. U+E30D sunny, U+E312 partly cloudy, U+E313 cloudy, U+E318 rain,
# U+E31A snow, U+E31B fog, U+E31D storm.
icon_for() {
    case "$1" in
        *hunder*|*Storm*|*storm*)         printf '\356\214\235' ;;
        *now*|*leet*|*lizzard*|*Ice*)     printf '\356\214\232' ;;
        *ain*|*rizzle*|*hower*)           printf '\356\214\230' ;;
        *og*|*ist*|*aze*)                 printf '\356\214\233' ;;
        *vercast*)                        printf '\356\214\223' ;;
        *loud*)                           printf '\356\214\222' ;;
        *lear*|*unny*)                    printf '\356\214\215' ;;
        *)                                printf '\356\214\223' ;;
    esac
}

while :; do
    reading=$(curl -sf --max-time 15 "https://wttr.in/${LOCATION}?format=%t|%C|%h|%w" 2>/dev/null)

    if [ -n "$reading" ]; then
        temperature=$(printf '%s' "$reading" | cut -d'|' -f1 | tr -d ' +')
        condition=$(printf '%s' "$reading" | cut -d'|' -f2)
        humidity=$(printf '%s' "$reading" | cut -d'|' -f3)
        wind=$(printf '%s' "$reading" | cut -d'|' -f4)

        printf '{"text":"%s %s","tooltip":"%s  %s\\nHumidity %s   Wind %s"}\n' \
            "$(icon_for "$condition")" "$temperature" \
            "$condition" "$temperature" "$humidity" "$wind"
    else
        printf '{"text":"\356\214\223 --","tooltip":"Weather unavailable"}\n'
    fi

    sleep "$EVERY"
done
