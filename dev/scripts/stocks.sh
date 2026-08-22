#!/bin/sh
# Stock quotes from finnhub.io, as a ticker feed for a second bar row.
#
#   [[module.custom]]
#   name = "stocks"
#   exec = "~/.config/ricebar/scripts/stocks.sh"
#   stream = true
#   scroll-width = 46
#   scroll-speed = 6
#
# The API needs a key. Put it in a file of its own rather than in config.toml,
# which is read by ricebar itself and meant to be shareable:
#
#   mkdir -p ~/.config/ricebar
#   printf '%s\n' 'YOUR_KEY_HERE' > ~/.config/ricebar/finnhub.token
#   chmod 600 ~/.config/ricebar/finnhub.token
#
# $FINNHUB_TOKEN overrides the file if you would rather keep it elsewhere,
# such as a password manager exported into the session.
#
# The key is handed to curl on stdin, never as an argument, so it does not
# appear in `ps` output for every user on the machine to read.

SYMBOLS="${SYMBOLS:-AAPL MSFT NVDA}"
# Currencies quoted against the zloty, from the Polish central bank. No key,
# and it is the authoritative source for PLN -- finnhub's forex is paid.
# Leave empty to skip.
FX="${FX:-USD EUR}"
# Market indices, from Yahoo's chart endpoint -- no key, and finnhub answers
# "Market data subscription required" for these. Written as
# label=symbol so the bar shows a readable name.
INDICES="${INDICES:-SP500=^GSPC DOW=^DJI NASDAQ=^IXIC FTSE=^FTSE NIKKEI=^N225 DAX=^GDAXI WIG=WIG.WA}"
EVERY="${EVERY:-300}"          # finnhub's free tier allows 60 calls/minute
SEPARATOR="   \342\200\242   " # U+2022 bullet

token_file="${XDG_CONFIG_HOME:-$HOME/.config}/ricebar/finnhub.token"

read_token() {
    [ -n "$FINNHUB_TOKEN" ] && { printf '%s' "$FINNHUB_TOKEN"; return 0; }
    [ -r "$token_file" ] || return 1

    # Same reasoning as ricebar's own config check: a secret others can read
    # is not a secret.
    case "$(ls -l "$token_file" | cut -c5-10)" in
        *[rw]*) printf 'ricebar: %s is readable by other users; chmod 600 it\n' \
            "$token_file" >&2 ;;
    esac

    head -n1 "$token_file" | tr -d ' \t\r\n'
}

quote() {
    printf 'header = "X-Finnhub-Token: %s"\nurl = "https://finnhub.io/api/v1/quote?symbol=%s"\n' \
        "$2" "$1" |
        curl -sf --max-time 10 --config - 2>/dev/null |
        SYMBOL="$1" python3 -c '
import json, os, sys

try:
    q = json.load(sys.stdin)
except Exception:
    sys.exit(1)

price, change = q.get("c"), q.get("dp")
if not price:
    sys.exit(1)

symbol = os.environ["SYMBOL"]
change = change or 0
arrow = "▲" if change >= 0 else "▼"
print(f"{symbol} {price:g} {arrow}{abs(change):.1f}%")
'
}

# One index, with the change since the previous close.
index_quote() {
    label=${1%%=*}
    symbol=${1#*=}

    curl -sf --max-time 10 -A 'Mozilla/5.0' \
        "https://query1.finance.yahoo.com/v8/finance/chart/$symbol?range=2d&interval=1d" 2>/dev/null |
        LABEL="$label" python3 -c '
import json, os, sys

try:
    meta = json.load(sys.stdin)["chart"]["result"][0]["meta"]
except Exception:
    sys.exit(1)

now = meta.get("regularMarketPrice")
before = meta.get("chartPreviousClose") or meta.get("previousClose")
if not now:
    sys.exit(1)

change = (now - before) / before * 100 if before else 0.0
label = os.environ["LABEL"]
arrow = "▲" if change >= 0 else "▼"

# Indices run to five figures, so no decimals.
print(f"{label} {now:,.0f} {arrow}{abs(change):.1f}%")
'
}

# One currency against PLN, with the change since the previous fixing.
fx_rate() {
    curl -sf --max-time 10 \
        "https://api.nbp.pl/api/exchangerates/rates/A/$1/last/2/?format=json" 2>/dev/null |
        CODE="$1" python3 -c '
import json, os, sys

try:
    rates = json.load(sys.stdin)["rates"]
except Exception:
    sys.exit(1)

if not rates:
    sys.exit(1)

now = rates[-1]["mid"]
before = rates[0]["mid"] if len(rates) > 1 else now
change = (now - before) / before * 100 if before else 0.0

code = os.environ["CODE"]
arrow = "▲" if change >= 0 else "▼"
print(f"{code}/PLN {now:.4f} {arrow}{abs(change):.1f}%")
'
}

while :; do
    token=$(read_token)

    if [ -z "$token" ]; then
        printf '{"text":"stocks: no API key","tooltip":"Put one in %s"}\n' "$token_file"
        sleep "$EVERY"
        continue
    fi

    feed=""

    for symbol in $SYMBOLS; do
        entry=$(quote "$symbol" "$token") || continue
        feed="${feed:+$feed$(printf "$SEPARATOR")}$entry"
    done

    for pair in $INDICES; do
        entry=$(index_quote "$pair") || continue
        feed="${feed:+$feed$(printf "$SEPARATOR")}$entry"
    done

    for code in $FX; do
        entry=$(fx_rate "$code") || continue
        feed="${feed:+$feed$(printf "$SEPARATOR")}$entry"
    done

    if [ -n "$feed" ]; then
        printf '{"text":"%s","tooltip":"finnhub.io"}\n' \
            "$(printf '%s' "$feed" | sed 's/\\/\\\\/g; s/"/\\"/g')"
    else
        printf '{"text":"stocks unavailable","tooltip":"finnhub.io returned nothing"}\n'
    fi

    sleep "$EVERY"
done
