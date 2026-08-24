#!/usr/bin/env python3
"""Market quotes, as a ticker feed for a second bar row.

    [[module.custom]]
    name = "stocks"
    exec = "~/.config/ricebar/scripts/stocks.py"
    stream = true
    scroll-width = 46
    scroll-speed = 6

Indices and currencies need no key at all, so this is useful as it stands.
Only individual company quotes do, from finnhub; without one it reports the
rest and says nothing about it.

For those, put the key in a file of its own rather than in config.toml, which
ricebar reads itself and which is meant to be shareable:

    mkdir -p ~/.config/ricebar
    printf '%s\\n' 'YOUR_KEY_HERE' > ~/.config/ricebar/finnhub.token
    chmod 600 ~/.config/ricebar/finnhub.token

$FINNHUB_TOKEN overrides the file, for a key kept somewhere else such as a
password manager exported into the session. Either way the key travels in a
request header and never on a command line, so it stays out of `ps`.

Everything is configured through the environment, so one script serves several
modules watching different things:

    SYMBOLS='AAPL MSFT'   company quotes; needs a finnhub key. Empty by default
    FX='USD EUR'          currencies against the zloty, from the Polish
                          central bank -- the authoritative source for PLN,
                          and finnhub's forex endpoint is paid
    INDICES='DAX=^GDAXI'  written label=symbol, from Yahoo's chart endpoint;
                          finnhub answers "Market data subscription required"
    EVERY=300             seconds between fetches
"""

import json
import os
import stat
import sys
import time
import urllib.error
import urllib.request

SYMBOLS = os.environ.get("SYMBOLS", "").split()
FX = os.environ.get("FX", "USD EUR").split()
INDICES = os.environ.get(
    "INDICES",
    "SP500=^GSPC DOW=^DJI NASDAQ=^IXIC FTSE=^FTSE NIKKEI=^N225 DAX=^GDAXI WIG=WIG.WA",
).split()
EVERY = int(os.environ.get("EVERY", "300"))

SEPARATOR = "   •   "
TIMEOUT = 10

CONFIG = os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config")
TOKEN_FILE = os.path.join(CONFIG, "ricebar", "finnhub.token")


def fetch(url, headers=None):
    """One request, returning parsed JSON, or None if anything at all failed.

    A ticker that drops a line is better than a ticker that stops, so every
    failure -- offline, rate limited, a body that is not JSON -- lands here.
    """
    request = urllib.request.Request(url, headers=headers or {})
    try:
        with urllib.request.urlopen(request, timeout=TIMEOUT) as response:
            return json.load(response)
    except (urllib.error.URLError, OSError, ValueError, json.JSONDecodeError):
        return None


def token():
    """The finnhub key, if there is one."""
    from_env = os.environ.get("FINNHUB_TOKEN")
    if from_env:
        return from_env.strip()

    try:
        mode = os.stat(TOKEN_FILE).st_mode
    except OSError:
        return None

    # The same reasoning as ricebar's own config check: a secret other users
    # can read is not a secret.
    if mode & (stat.S_IRWXG | stat.S_IRWXO):
        print(
            f"ricebar: {TOKEN_FILE} is readable by other users; chmod 600 it",
            file=sys.stderr,
        )

    try:
        with open(TOKEN_FILE, encoding="utf-8") as handle:
            return handle.readline().strip() or None
    except OSError:
        return None


def moved(now, before):
    """The change as a percentage, and the arrow that goes with it."""
    change = (now - before) / before * 100 if before else 0.0
    return change, "▲" if change >= 0 else "▼"


def company(symbol, key):
    quote = fetch(
        f"https://finnhub.io/api/v1/quote?symbol={symbol}",
        {"X-Finnhub-Token": key},
    )
    if not quote:
        return None

    price = quote.get("c")
    if not price:
        return None

    change = quote.get("dp") or 0.0
    arrow = "▲" if change >= 0 else "▼"
    return f"{symbol} {price:g} {arrow}{abs(change):.1f}%"


def index(pair):
    label, _, symbol = pair.partition("=")
    symbol = symbol or label

    chart = fetch(
        f"https://query1.finance.yahoo.com/v8/finance/chart/{symbol}"
        "?range=2d&interval=1d",
        # Yahoo's chart endpoint answers 403 to an unadorned client.
        {"User-Agent": "Mozilla/5.0"},
    )
    if not chart:
        return None

    try:
        meta = chart["chart"]["result"][0]["meta"]
    except (KeyError, IndexError, TypeError):
        return None

    now = meta.get("regularMarketPrice")
    if not now:
        return None

    before = meta.get("chartPreviousClose") or meta.get("previousClose")
    change, arrow = moved(now, before)

    # Indices run to five figures, so no decimals.
    return f"{label} {now:,.0f} {arrow}{abs(change):.1f}%"


def currency(code):
    answer = fetch(
        f"https://api.nbp.pl/api/exchangerates/rates/A/{code}/last/2/?format=json"
    )
    if not answer:
        return None

    try:
        rates = answer["rates"]
    except (KeyError, TypeError):
        return None

    if not rates:
        return None

    now = rates[-1]["mid"]
    before = rates[0]["mid"] if len(rates) > 1 else now
    change, arrow = moved(now, before)

    return f"{code}/PLN {now:.4f} {arrow}{abs(change):.1f}%"


def feed():
    key = token()
    entries = []

    if SYMBOLS and not key:
        # Company quotes were asked for without the key they need. Say so in
        # the feed rather than dropping them silently.
        entries.append(f"no API key in {TOKEN_FILE}")
    else:
        entries += [quote for quote in (company(s, key) for s in SYMBOLS) if quote]

    entries += [quote for quote in (index(p) for p in INDICES) if quote]
    entries += [quote for quote in (currency(c) for c in FX) if quote]

    return SEPARATOR.join(entries)


def main():
    # This runs forever by design, so anyone who tries `--help` first deserves
    # an answer rather than a hang.
    if {"-h", "--help"} & set(sys.argv[1:]):
        print(__doc__)
        return

    while True:
        line = feed()

        if line:
            report = {"text": line, "tooltip": "Yahoo Finance, NBP, finnhub"}
        else:
            report = {
                "text": "markets unavailable",
                "tooltip": "nothing answered; offline?",
            }

        # json.dumps does the escaping, which is the point of building the
        # report as a dict rather than printing the JSON by hand.
        print(json.dumps(report), flush=True)
        time.sleep(EVERY)


if __name__ == "__main__":
    main()
