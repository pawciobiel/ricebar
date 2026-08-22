#!/bin/sh
# A ticker feed, for a second bar row.
#
# Prints one long line; ricebar scrolls it past a fixed window when the module
# sets `scroll-width`. Streams rather than polls, so this is one process that
# sleeps between refreshes.
#
#   [[module.custom]]
#   name = "ticker"
#   exec = "~/.config/ricebar/scripts/ticker.sh"
#   stream = true
#   scroll-width = 60
#   scroll-speed = 6
#
# The feed below is local so it always works. Replace `collect` with whatever
# you want scrolling past -- quotes, headlines, build status, train times. A
# quotes API generally wants a key, so it belongs in your config rather than
# hard-coded here:
#
#   collect() {
#       curl -sf "https://api.example.com/quote?symbols=AAPL,MSFT&key=$KEY" |
#           jq -r '.[] | "\(.symbol) \(.price)"' | paste -sd '   '
#   }

EVERY="${EVERY:-30}"
SEPARATOR="   \342\200\242   "   # U+2022 bullet

collect() {
    uptime_part=$(uptime | sed 's/.*up //; s/,  *[0-9]* user.*//; s/^ *//')
    load_part=$(cut -d' ' -f1-3 /proc/loadavg)

    disk_part=$(df -h / 2>/dev/null | awk 'NR==2 {printf "root %s of %s", $3, $2}')

    # busybox ps takes no sort flags, so the busiest processes come from top.
    top_part=$(top -bn1 2>/dev/null |
        awk 'NR>4 && NR<8 {name=$9; sub(".*/", "", name); printf "%s%s %s", (NR>5 ? " " : ""), name, $8}')

    mem_part=$(free -m 2>/dev/null |
        awk '/^Mem:/ {printf "mem %d of %d MiB", $3, $2}')

    printf 'up %s%bload %s%b%s%b%s%b%s' \
        "$uptime_part" "$SEPARATOR" \
        "$load_part" "$SEPARATOR" \
        "$disk_part" "$SEPARATOR" \
        "$mem_part" "$SEPARATOR" \
        "$top_part"
}

while :; do
    feed=$(collect)
    [ -n "$feed" ] || feed="nothing to report"

    printf '{"text":"%s","tooltip":"System ticker"}\n' \
        "$(printf '%s' "$feed" | sed 's/\\/\\\\/g; s/"/\\"/g')"

    sleep "$EVERY"
done
