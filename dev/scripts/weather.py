#!/usr/bin/env python3
"""Weather from wttr.in, as a streaming module.

    [[module.custom]]
    name = "weather"
    exec = "~/.config/ricebar/scripts/weather.py"
    stream = true

Streams rather than polls so ricebar re-runs nothing: this sleeps between
fetches and prints a line each time, which also means one process instead of
a fork every interval.

Everything is configured through the environment, so one script can serve
several modules watching different places:

    LOCATION=Lublin,Poland  a city, or empty to guess from the IP. Qualify it
                            with a country, or give coordinates, "51.25,22.57"
    EVERY=900               seconds between fetches; be kind to a free service
    ICONS=~/.config/ricebar/icons/weather
                            a directory of pictures, named the freedesktop way
                            (weather-clear, weather-showers, ...), so an icon
                            theme's status directory works as it is. With it
                            the condition picks a file rather than a glyph.
                            `.svg` is tried, then `.png`

A failed fetch is retried within the minute, since the bar starts with the
compositor and often beats the network to it.
"""

import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

LOCATION = os.environ.get("LOCATION", "")
EVERY = int(os.environ.get("EVERY", "900"))
ICONS = os.path.expanduser(os.environ.get("ICONS", ""))

TIMEOUT = 15
RETRY = 15  # seconds after a failed fetch, doubling up to EVERY

# %l is the place as wttr.in resolved it, which shows where an IP guess landed.
# %T carries a zone offset that sunrise and sunset do not, so only its clock is
# kept.
FORMAT = "%l|%t|%C|%h|%w|%S|%s|%T"
FIELDS = (
    "place",
    "temperature",
    "condition",
    "humidity",
    "wind",
    "sunrise",
    "sunset",
    "now",
)

# Substrings to match, then the freedesktop name and Nerd Font glyph to draw,
# and optionally a second pair for after dark. Order matters: "patchy light
# rain with thunder" is a storm, not rain. Only a clear or cloudy sky looks
# different at night, so only those two have a night form.
#
# The glyphs are escapes so the file stays plain ASCII: U+E30D sunny, U+E312
# partly cloudy, U+E313 cloudy, U+E318 rain, U+E31A snow, U+E31B fog, U+E31D
# storm, U+E32B crescent moon, U+E379 moon behind cloud.
CONDITIONS = (
    (("thunder", "storm"), ("weather-storm", "\ue31d")),
    (("snow", "sleet", "blizzard", "ice"), ("weather-snow", "\ue31a")),
    (("rain", "drizzle", "shower"), ("weather-showers", "\ue318")),
    (("fog", "mist", "haze"), ("weather-fog", "\ue31b")),
    (("overcast",), ("weather-overcast", "\ue313")),
    (
        ("cloud",),
        ("weather-few-clouds", "\ue312"),
        ("weather-few-clouds-night", "\ue379"),
    ),
    (
        ("clear", "sunny"),
        ("weather-clear", "\ue30d"),
        ("weather-clear-night", "\ue32b"),
    ),
)

# Anything unmatched: a nondescript grey sky.
UNKNOWN = ("weather-overcast", "\ue313")


def fetch():
    """One reading, as a dict of the fields above, or None if it failed."""
    place = urllib.parse.quote(LOCATION)
    url = f"https://wttr.in/{place}?format={urllib.parse.quote(FORMAT)}"

    try:
        with urllib.request.urlopen(url, timeout=TIMEOUT) as response:
            line = response.read().decode("utf-8", "replace").strip()
    except (urllib.error.URLError, OSError, ValueError):
        return None

    values = line.split("|")
    if len(values) != len(FIELDS):
        # A location wttr.in cannot place is answered with prose, not an error.
        return None

    reading = dict(zip(FIELDS, values))
    reading["temperature"] = reading["temperature"].replace("+", "").strip()
    return reading


def is_night(reading):
    """Whether it is dark out, from the sunrise and sunset in the same reading.

    wttr.in reports the sky, not the hour: a cloudless night comes back as
    "Sunny". All three times are HH:MM:SS, so string comparison is exact.
    """
    now, sunrise, sunset = reading["now"][:8], reading["sunrise"], reading["sunset"]
    if not (now and sunrise and sunset):
        return False
    return now < sunrise or now > sunset


def looks_like(condition, dark):
    """The freedesktop name and the glyph for a condition."""
    condition = condition.lower()
    for words, day, *night in CONDITIONS:
        if any(word in condition for word in words):
            return night[0] if dark and night else day
    return UNKNOWN


def picture(name):
    """The path to draw for an icon name, if ICONS holds one.

    A set with no night variants falls back to the daytime name.
    """
    if not ICONS:
        return None

    for candidate in (name, name.removesuffix("-night")):
        for extension in ("svg", "png"):
            path = os.path.join(ICONS, f"{candidate}.{extension}")
            if os.path.isfile(path):
                return path
    return None


def report(reading):
    name, glyph = looks_like(reading["condition"], is_night(reading))
    tooltip = (
        "{place}\n{condition}  {temperature}\nHumidity {humidity}   Wind {wind}"
    ).format(**reading)

    path = picture(name)
    if path:
        # A picture goes in its own field; ricebar joins it to the text.
        return {"text": reading["temperature"], "icon": path, "tooltip": tooltip}

    return {"text": f"{glyph} {reading['temperature']}", "tooltip": tooltip}


def main():
    # This runs forever, so `--help` must answer rather than hang.
    if {"-h", "--help"} & set(sys.argv[1:]):
        print(__doc__)
        return

    wait = RETRY

    while True:
        reading = fetch()

        if reading:
            # json.dumps escapes it, which is why the report is built as a dict.
            print(json.dumps(report(reading)), flush=True)
            wait = RETRY
            time.sleep(EVERY)
        else:
            print(
                json.dumps(
                    {"text": f"{UNKNOWN[1]} --", "tooltip": "Weather unavailable"}
                ),
                flush=True,
            )
            # Back off, but come back well before the next fetch is due.
            time.sleep(wait)
            wait = min(wait * 2, EVERY)


if __name__ == "__main__":
    main()
