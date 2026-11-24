#!/usr/bin/env python3
"""Live probe of Rocket.Chat's chat.search for the rockettui search change.

Usage: python3 -I dev/probe_search.py        (from the repository root)

Standard library only and read-only: it only issues GET requests as the dev user against the
throwaway local server that dev/seed.py started. Same output rule as the other probes: a line
holds only a probe id, a status code, key names, counts and yes/no flags. Never a credential,
a message text or a search word (the word is picked from the room's own messages and only its
length is printed). The server must be on loopback.
"""

import collections
import json
import os
import re
import sys
import urllib.error
import urllib.parse
import urllib.request

PROJECT = os.environ.get("RT_PROJECT", "rockettui-dev")
REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
APP_ENV = os.path.join(REPO, "dev", "state", PROJECT, ".env.local")
SECRETS = set()
BASE = ""
AUTH = ("", "")


def out(line):
    for secret in SECRETS:
        if secret and secret in line:
            print("PROBE FAIL: refusing to print a secret value")
            sys.exit(1)
    print(line)


def read_env(path):
    values = {}
    with open(path) as handle:
        for line in handle:
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                key, _, value = line.partition("=")
                values[key] = value
    return values


def get(path, **query):
    url = BASE + path + "?" + urllib.parse.urlencode(query)
    headers = {"X-User-Id": AUTH[0], "X-Auth-Token": AUTH[1]}
    request = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            status, raw = response.status, response.read()
    except urllib.error.HTTPError as error:
        status, raw = error.code, error.read()
    except (urllib.error.URLError, OSError):
        return 0, None
    try:
        return status, json.loads(raw)
    except ValueError:
        return status, None


def keys(obj):
    return ",".join(sorted(obj)) if isinstance(obj, dict) else "-"


def search(room, text, **extra):
    return get("/api/v1/chat.search", roomId=room, searchText=text, **extra)


def msgs(reply):
    return (reply or {}).get("messages") or []


def ids(reply):
    return [m.get("_id") for m in msgs(reply)]


def main():
    global BASE, AUTH
    try:
        env = read_env(APP_ENV)
        BASE = env["ROCKETTUI_SERVER_URL"].rstrip("/")
        AUTH = (env["ROCKETTUI_USER_ID"], env["ROCKETTUI_TOKEN"])
    except (OSError, KeyError):
        print("PROBE FAIL: no dev state files; run dev/seed.py up and seed first")
        return 1
    SECRETS.update(v for v in AUTH if v)
    if urllib.parse.urlparse(BASE).hostname not in ("127.0.0.1", "localhost"):
        print("PROBE FAIL: not a loopback server")
        return 1

    status, info = get("/api/v1/rooms.info", roomName="rt-public")
    room = ((info or {}).get("room") or {}).get("_id")
    out("P0 rooms.info status=%s found=%s" % (status, bool(room)))
    if not room:
        return 1

    # A word taken from the room's own messages, never printed.
    status, hist = get("/api/v1/channels.history", roomId=room, count=100)
    words = collections.Counter(
        w.lower()
        for m in (hist or {}).get("messages", [])
        for w in re.findall(r"[A-Za-z]{4,}", m.get("msg", ""))
    )
    if not words:
        out("PROBE FAIL: no words in history")
        return 1
    word = words.most_common(1)[0][0]
    out("P0 history status=%s word_len=%d word_hits_in_100=%d" % (status, len(word), words[word]))

    # P1: shape.
    status, r = search(room, word)
    out("P1 basic status=%s top_keys=%s count=%d" % (status, keys(r), len(msgs(r))))
    first = msgs(r)[0] if msgs(r) else {}
    out("P1 message_keys=%s" % keys(first))
    stamps = [m.get("ts") for m in msgs(r)]
    out("P1 ts_type=%s newest_first=%s" % (type(stamps[0]).__name__ if stamps else "-",
                                          stamps == sorted(stamps, reverse=True)))

    # P2: default and maximum page size, paging by offset.
    status, big = search(room, word, count=500)
    out("P2 count=500 status=%s returned=%d" % (status, len(msgs(big))))
    status, a = search(room, word, count=5, offset=0)
    status2, b = search(room, word, count=5, offset=5)
    out("P2 pages status=%s/%s sizes=%d/%d disjoint=%s" % (
        status, status2, len(msgs(a)), len(msgs(b)), not set(ids(a)) & set(ids(b))))
    out("P2 has_total_or_offset_keys=%s" % keys({k: 1 for k in (a or {}) if k != "messages"}))

    # P3: case and empty query.
    status, up = search(room, word.upper())
    out("P3 uppercase_same_ids=%s status=%s" % (ids(up) == ids(r), status))
    status, empty = search(room, "")
    out("P3 empty_query status=%s returned=%d" % (status, len(msgs(empty))))
    status, none = search(room, "zzzqqqxxxnomatch")
    out("P3 no_match status=%s returned=%d" % (status, len(msgs(none))))

    # P4: the server's query syntax.
    for label, text in (("from", "from:alice"), ("has_star", "has:star"), ("is_pinned", "is:pinned"),
                        ("on_date", "on:2000-01-01"), ("from_plus_word", "from:alice " + word),
                        ("regex", "/" + word[:3] + "/")):
        status, q = search(room, text)
        out("P4 %s status=%s returned=%d" % (label, status, len(msgs(q))))

    # P5: other room kinds, and a room the user does not belong to.
    status, subs = get("/api/v1/subscriptions.get")
    for sub in (subs or {}).get("update", []):
        kind, name = sub.get("t"), sub.get("name", "")
        label = {"d": "dm", "p": "private", "c": "public"}.get(kind, kind)
        status, q = search(sub.get("rid"), word)
        status_s = status
        enc = (sub.get("encrypted") is True)
        out("P5 kind=%s status=%s returned=%d encrypted_flag=%s" % (label, status_s, len(msgs(q)), enc))
    status, q = search("nonexistentroomid", word)
    out("P5 unknown_room status=%s" % status)

    # P6: can a result be shown in context? Fetch a window around the oldest hit.
    hits = msgs(big)
    if hits:
        target = hits[-1]
        ts = target.get("ts")
        status, before = get("/api/v1/channels.history", roomId=room, latest=ts, count=3, inclusive="true")
        out("P6 history_latest_inclusive status=%s returned=%d has_target=%s" % (
            status, len((before or {}).get("messages", [])), target.get("_id") in
            [m.get("_id") for m in (before or {}).get("messages", [])]))
        status, after = get("/api/v1/channels.history", roomId=room, oldest=ts, count=3, inclusive="true")
        out("P6 history_oldest_inclusive status=%s returned=%d has_target=%s" % (
            status, len((after or {}).get("messages", [])), target.get("_id") in
            [m.get("_id") for m in (after or {}).get("messages", [])]))
        status, one = get("/api/v1/chat.getMessage", msgId=target.get("_id"))
        out("P6 getMessage status=%s keys=%s" % (status, keys((one or {}).get("message"))))
    out("PROBE DONE")
    return 0


if __name__ == "__main__":
    sys.exit(main())
