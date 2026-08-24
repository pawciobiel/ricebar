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
# Qualify it with a country -- "Lublin,Poland", not "Lublin" -- since plenty of
# names are shared, and coordinates such as "51.25,22.57" are exact.

LOCATION="${LOCATION:-}"
EVERY="${EVERY:-900}"   # seconds between fetches; be kind to a free service

# Set ICONS to a directory of pictures and the condition names a file in it
# instead of a glyph -- ricebar draws whichever it is given:
#
#   exec = "ICONS=~/.config/ricebar/icons/weather ~/.config/ricebar/scripts/weather.sh"
#
# The names are the freedesktop ones (weather-clear, weather-showers, ...), so
# an icon theme that has them can be pointed at directly. `.svg` is tried
# first, then `.png`.
ICONS="${ICONS:-}"

# Nerd Font weather glyphs, written as octal UTF-8 so this file stays plain
# ASCII. U+E30D sunny, U+E312 partly cloudy, U+E313 cloudy, U+E318 rain,
# U+E31A snow, U+E31B fog, U+E31D storm.
glyph_for() {
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

name_for() {
    case "$1" in
        *hunder*|*Storm*|*storm*)         printf 'weather-storm' ;;
        *now*|*leet*|*lizzard*|*Ice*)     printf 'weather-snow' ;;
        *ain*|*rizzle*|*hower*)           printf 'weather-showers' ;;
        *og*|*ist*|*aze*)                 printf 'weather-fog' ;;
        *vercast*)                        printf 'weather-overcast' ;;
        *loud*)                           printf 'weather-clouds' ;;
        *lear*|*unny*)                    printf 'weather-clear' ;;
        *)                                printf 'weather-clouds' ;;
    esac
}

# What to put in the JSON's "icon" field: a path when ICONS is set and holds
# one, and nothing at all otherwise, which leaves the glyph in the text.
icon_for() {
    [ -n "$ICONS" ] || return

    name=$(name_for "$1")

    for extension in svg png; do
        if [ -f "$ICONS/$name.$extension" ]; then
            printf '%s/%s.%s' "$ICONS" "$name" "$extension"
            return
        fi
    done
}

while :; do
    # %l comes back as whatever the place resolved to, which is worth
    # showing: it names the city when several modules watch different ones,
    # and reveals where the IP guess landed when LOCATION is empty.
    reading=$(curl -sf --max-time 15 "https://wttr.in/${LOCATION}?format=%l|%t|%C|%h|%w" 2>/dev/null)

    if [ -n "$reading" ]; then
        place=$(printf '%s' "$reading" | cut -d'|' -f1)
        temperature=$(printf '%s' "$reading" | cut -d'|' -f2 | tr -d ' +')
        condition=$(printf '%s' "$reading" | cut -d'|' -f3)
        humidity=$(printf '%s' "$reading" | cut -d'|' -f4)
        wind=$(printf '%s' "$reading" | cut -d'|' -f5)

        picture=$(icon_for "$condition")

        if [ -n "$picture" ]; then
            # A picture goes in its own field, and the text is just the
            # temperature: ricebar puts the two together itself.
            printf '{"text":"%s","icon":"%s","tooltip":"%s\\n%s  %s\\nHumidity %s   Wind %s"}\n' \
                "$temperature" "$picture" \
                "$place" "$condition" "$temperature" "$humidity" "$wind"
        else
            printf '{"text":"%s %s","tooltip":"%s\\n%s  %s\\nHumidity %s   Wind %s"}\n' \
                "$(glyph_for "$condition")" "$temperature" \
                "$place" "$condition" "$temperature" "$humidity" "$wind"
        fi
    else
        printf '{"text":"\356\214\223 --","tooltip":"Weather unavailable"}\n'
    fi

    sleep "$EVERY"
done
