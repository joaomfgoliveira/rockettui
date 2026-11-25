#!/usr/bin/env python3
"""Put custom emojis and messages that use them on the local dev server, to look at them in the app
(change custom-emoji-images, tasks 5.1 and 5.2).

Usage (from the repository root):
    python3 -I dev/demo_emoji.py up      # create the emojis, post the messages, add the reactions
    python3 -I dev/demo_emoji.py down    # delete the emojis and those messages

Standard library only. Loopback servers only. Creates two emojis, `rtshipit` (alias `rtship`) and
`rtplain`, each a 32x32 png with a frame so that cropping or stretching shows, and posts messages
in `rt-public` as the app user that name them: in a short body, in a long body that wraps at a
narrow pane, twice in a row, in a code span, next to a unicode emoji, plus reactions. What it
posted is recorded in dev/state/demo_emoji.json so that `down` can delete it.

Output rule: statuses and counts only.
"""

import importlib.util
import json
import os
import struct
import sys
import time
import urllib.parse
import uuid
import zlib

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
STATE = os.path.join(REPO, "dev", "state", "demo_emoji.json")
EMOJIS = (
    ("rtshipit", "rtship", (220, 60, 40), "png"),
    ("rtplain", "", (40, 120, 220), "png"),
    ("rtspin", "", None, "gif"),
)

_spec = importlib.util.spec_from_file_location("capture_reactions", os.path.join(REPO, "dev", "capture_reactions.py"))
cap = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(cap)
pe = cap.pe


def png(color, size=32):
    """A size x size png: a gradient of `color` with a white frame and a diagonal."""

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    rows = b""
    for y in range(size):
        row = b"\x00"
        for x in range(size):
            if x in (0, size - 1) or y in (0, size - 1) or x == y:
                row += b"\xff\xff\xff\xff"
            else:
                shade = 0.4 + 0.6 * (x + y) / (2 * size)
                row += bytes([int(c * shade) for c in color]) + b"\xff"
        rows += row
    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")


def gif(size=16, frames=4, delay_cs=15):
    """An animated gif: a bright block that walks round a dark square, `frames` frames, looping.

    Written by hand (no library): four palette colours, LZW with a clear code before every second
    pixel so the code width never grows past 3 bits.
    """
    palette = [(20, 20, 40), (255, 200, 0), (255, 255, 255), (0, 0, 0)]
    out = b"GIF89a" + struct.pack("<HHBBB", size, size, 0xF1, 0, 0)
    for r, g, b in palette:
        out += bytes([r, g, b])
    out += b"\x21\xff\x0bNETSCAPE2.0\x03\x01\x00\x00\x00"
    half = size // 2
    spots = [(0, 0), (half, 0), (half, half), (0, half)]
    for n in range(frames):
        sx, sy = spots[n % len(spots)]
        pixels = []
        for y in range(size):
            for x in range(size):
                frame = x in (0, size - 1) or y in (0, size - 1)
                pixels.append(2 if frame else 1 if sx <= x < sx + half and sy <= y < sy + half else 0)
        bits, nbits, data = 0, 0, bytearray()

        def put(code):
            nonlocal bits, nbits
            bits |= code << nbits
            nbits += 3
            while nbits >= 8:
                data.append(bits & 0xFF)
                bits >>= 8
                nbits -= 8

        for i in range(0, len(pixels), 2):
            put(4)
            for p in pixels[i : i + 2]:
                put(p)
        put(4)
        put(5)
        if nbits:
            data.append(bits & 0xFF)
        out += b"\x21\xf9\x04\x04" + struct.pack("<H", delay_cs) + b"\x00\x00"
        out += b"\x2c" + struct.pack("<HHHHB", 0, 0, size, size, 0) + b"\x02"
        for i in range(0, len(data), 255):
            chunk = data[i : i + 255]
            out += bytes([len(chunk)]) + bytes(chunk)
        out += b"\x00"
    return out + b"\x3b"


def create(admin, name, aliases, color, ext="png"):
    import urllib.error
    import urllib.request

    boundary = uuid.uuid4().hex
    parts = []
    for field, value in (("name", name), ("aliases", aliases)):
        parts.append(('--%s\r\nContent-Disposition: form-data; name="%s"\r\n\r\n%s\r\n' % (boundary, field, value)).encode())
    parts.append(
        (
            '--%s\r\nContent-Disposition: form-data; name="emoji"; filename="%s.%s"\r\n'
            "Content-Type: image/%s\r\n\r\n" % (boundary, name, ext, ext)
        ).encode()
        + (gif() if ext == "gif" else png(color))
        + b"\r\n"
    )
    parts.append(("--%s--\r\n" % boundary).encode())
    request = urllib.request.Request(
        pe.BASE + "/api/v1/emoji-custom.create",
        data=b"".join(parts),
        method="POST",
        headers={
            "Content-Type": "multipart/form-data; boundary=%s" % boundary,
            "X-User-Id": admin[0],
            "X-Auth-Token": admin[1],
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return response.status
    except urllib.error.HTTPError as error:
        return error.code
    except (urllib.error.URLError, OSError):
        return 0


def session():
    try:
        app = pe.read_env(pe.APP_ENV)
        env = pe.read_env(pe.ADMIN_ENV)
        pe.BASE = app["ROCKETTUI_SERVER_URL"].rstrip("/")
        dev = (app["ROCKETTUI_USER_ID"], app["ROCKETTUI_TOKEN"])
        admin_pass = env["ADMIN_PASS"]
    except (OSError, KeyError):
        print("DEMO FAIL: no dev state files; run dev/seed.py up and seed first")
        return None
    pe.SECRETS.update(v for v in (dev[0], dev[1], admin_pass) if v)
    if urllib.parse.urlparse(pe.BASE).hostname not in pe.LOOPBACK:
        pe.out("DEMO FAIL: not a loopback server")
        return None
    return dev, pe.login("admin", admin_pass)


def up():
    got = session()
    if not got:
        return 1
    dev, admin = got
    status, listing = pe.api("GET", "/api/v1/subscriptions.get", auth=dev)
    room = next((s["rid"] for s in (listing or {}).get("update", []) if s.get("name") == "rt-public"), None)
    if status != 200 or not room:
        pe.out("DEMO FAIL: rt-public not found")
        return 1
    for name, aliases, color, ext in EMOJIS:
        pe.out("create %s: %d" % (name, create(admin, name, aliases, color, ext)))
    time.sleep(0.5)
    texts = [
        "demo: one custom emoji :rtshipit: in a short line",
        "demo: the alias :rtship: and a second emoji :rtplain: and a unicode one :tada: together",
        "demo: a long line so it wraps in a narrow pane :rtshipit: with words around it :rtplain: and again "
        ":rtshipit: and once more :rtshipit: at the end of the line so that it wraps more than once :rtplain:",
        "demo: two in a row :rtshipit::rtshipit: and in code `:rtshipit:` and an unknown :rt_nothing_here:",
        "demo: an animated emoji :rtspin: and another :rtspin: next to a still one :rtshipit:",
        "demo: reactions below this one",
    ]
    ids = []
    for text in texts:
        ids.append(pe.post_message(dev, room, text))
    for emoji in (":rtshipit:", ":rtplain:", ":rtspin:", ":tada:"):
        pe.out("react %s: %d" % (emoji, cap.react(dev, ids[-1], emoji)))
    os.makedirs(os.path.dirname(STATE), exist_ok=True)
    with open(STATE, "w") as handle:
        json.dump({"room": room, "messages": ids}, handle)
    pe.out("posted %d messages" % len(ids))
    pe.out("DEMO UP")
    return 0


def down():
    got = session()
    if not got:
        return 1
    dev, admin = got
    try:
        with open(STATE) as handle:
            state = json.load(handle)
    except (OSError, ValueError):
        state = {"room": None, "messages": []}
    for mid in state["messages"]:
        pe.delete(dev, state["room"], mid)
    for name, _, _, _ in EMOJIS:
        cap.delete_custom_emoji(admin, name)
    try:
        os.remove(STATE)
    except OSError:
        pass
    pe.out("deleted %d messages and %d emojis" % (len(state["messages"]), len(EMOJIS)))
    pe.out("DEMO DOWN")
    return 0


if __name__ == "__main__":
    mode = sys.argv[1] if len(sys.argv) > 1 else ""
    sys.exit(up() if mode == "up" else down() if mode == "down" else (print(__doc__) or 1))
