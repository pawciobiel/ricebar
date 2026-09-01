#!/bin/sh
# Record the animated ricebar demo, start to finish, with nothing on screen.
#
#   dev/record/record.sh            # docs/ricebar.webp
#   dev/record/record.sh probe      # rig up, screenshot, stop -- for measuring
#
# Everything happens inside a headless sway: its own 1280x720 output, its own
# wallpaper, two ricebar processes, and a pointer driven from a script. Nothing
# appears on the screen you are sitting at, nothing personal is captured, and a
# take is the same every time.
#
# The pointer is the part that took a while. A headless seat has no pointer
# capability at all, so clients never bind `wl_pointer` and neither hover nor
# clicks arrive. `wlrctl` grants one for the length of one command, which is
# enough for a hover but loses the click that follows it. `dev/record/vpointer`
# holds a single virtual pointer open for the whole take instead, which is what
# makes clicking the calendar, the menu and the popups recordable.
set -eu

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
WORK=${WORK:-${TMPDIR:-/tmp}/ricebar-record}
WEBP=${WEBP:-$ROOT/docs/ricebar.webp}
WIDTH=1280
HEIGHT=720
FPS=${FPS:-14}
QUALITY=${QUALITY:-72}
MODE=${1:-record}

cd "$ROOT"
mkdir -p "$WORK" "$(dirname "$WEBP")"
rm -f "$WORK"/take.mkv

cargo build --release
(cd dev/record/vpointer && cargo build --release)
VPOINTER=$ROOT/dev/record/vpointer/target/release/vpointer

WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=1 \
    sway --config "$ROOT/dev/record/sway.conf" > "$WORK/sway.log" 2>&1 &
SWAY=$!

finish() {
    [ -n "${GRABBER:-}" ] && kill "$GRABBER" 2>/dev/null
    [ -n "${TOP:-}" ] && kill "$TOP" 2>/dev/null
    [ -n "${BOTTOM:-}" ] && kill "$BOTTOM" 2>/dev/null
    sleep 1
    kill "$SWAY" 2>/dev/null
    return 0
}
trap finish EXIT INT TERM

sleep 3

# The nested session's socket is the newest one: it was made a moment ago, and
# the session you are sitting in was not.
WAYLAND_DISPLAY=$(basename "$(ls -t /run/user/"$(id -u)"/wayland-[0-9]* | grep -v '\.lock' | head -1)")
SWAYSOCK=$(ls -t /run/user/"$(id -u)"/sway-ipc."$(id -u)".*.sock | head -1)
export WAYLAND_DISPLAY SWAYSOCK
echo "rig: $WAYLAND_DISPLAY on $SWAYSOCK"

# Two windows, so the workspace buttons have something to switch between and
# the window popup has something to list. What they show is the bar's own
# config: the whole pitch is that it is one file.
swaymsg -q "exec foot --title=config --font='JetBrains Mono:size=11' \
    -e sh -c 'sed -n 1,44p dev/record/top.toml; exec sleep 3600'" || true
sleep 2
swaymsg -q workspace 2 || true
swaymsg -q "exec foot --title=scripts --font='JetBrains Mono:size=11' \
    -e sh -c 'sed -n 1,40p dev/scripts/volume.sh; exec sleep 3600'" || true
sleep 2
swaymsg -q workspace 1 || true

"$ROOT/target/release/ricebar" -c dev/record/top.toml > "$WORK/top.log" 2>&1 &
TOP=$!
"$ROOT/target/release/ricebar" -c dev/record/bottom.toml > "$WORK/bottom.log" 2>&1 &
BOTTOM=$!
sleep 4

if [ "$MODE" = probe ]; then
    # A shot a second while the moves run, so a popup that only exists under
    # the pointer can be measured before a take is scripted around it.
    rm -f "$WORK"/probe-*.png
    (
        shot=0
        while [ $shot -lt 60 ]; do
            grim -c -o HEADLESS-1 "$(printf '%s/probe-%02d.png' "$WORK" "$shot")" 2>/dev/null || true
            shot=$((shot + 1))
            sleep 1
        done
    ) &
    GRABBER=$!

    if [ -n "${2:-}" ]; then
        "$VPOINTER" "$WIDTH" "$HEIGHT" < "$2"
    else
        printf 'move 640 400\nwait 2000\n' | "$VPOINTER" "$WIDTH" "$HEIGHT"
    fi

    kill "$GRABBER" 2>/dev/null
    GRABBER=
    echo "probe shots: $WORK/probe-*.png"
    exit 0
fi

# wf-recorder draws the cursor into the frame, which a recording of clicking
# needs, and gives a constant rate rather than whatever a screenshot loop
# manages. The clock ticks every second, so there is always damage to record.
wf-recorder -y -o HEADLESS-1 -r 20 -f "$WORK/take.mkv" > "$WORK/wf.log" 2>&1 &
GRABBER=$!
sleep 2

"$VPOINTER" "$WIDTH" "$HEIGHT" < "$ROOT/dev/record/moves.txt"

kill -INT "$GRABBER" 2>/dev/null
wait "$GRABBER" 2>/dev/null || true
GRABBER=

ffmpeg -y -loglevel error -i "$WORK/take.mkv" \
    -vf "fps=$FPS,scale=$WIDTH:-1:flags=lanczos" \
    -c:v libwebp_anim -lossless 0 -q:v "$QUALITY" -compression_level 6 -loop 0 \
    "$WEBP"

echo "take: $WORK/take.mkv"
ls -lh "$WEBP"
