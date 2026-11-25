#!/usr/bin/env python3
"""Probe how the server serves a custom emoji image (change custom-emoji-images, task 3.5).

Usage (from the repository root):
    python3 -I dev/probe_emoji_image.py
    RT_PROJECT=rockettui-dev9 python3 -I dev/probe_emoji_image.py   # the 9.0.0 stack

Standard library only. Runs against the throwaway local server that dev/seed.py started and
reuses the helpers of dev/capture_reactions.py. It creates one custom emoji (a real 1x1 png) as
admin, then asks for `/emoji-custom/<name>.<ext>`:
  * as the app user, with the auth headers the client sends;
  * with no headers at all (does the path need them?);
  * with the `?etag=` the list gave, and with another etag (does it matter?);
  * for a name that does not exist (what does the server answer?);
  * with a redirect check: the answer must not be a 3xx.
and deletes the emoji in a `finally`.

Output rule: statuses, content types, byte counts and a yes/no for the png magic bytes. Never a
credential, a body or a header value other than the content type. The server must be on loopback.
The last line is `PROBE OK` only when every expectation held and the emoji was deleted.
"""

import importlib.util
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NAME = "rtimgprobe"

_spec = importlib.util.spec_from_file_location("capture_reactions", os.path.join(REPO, "dev", "capture_reactions.py"))
cap = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(cap)
pe = cap.pe


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def get(path, headers=None):
    """(status, content type, byte count, starts with the png magic) of a GET, redirects not followed."""
    opener = urllib.request.build_opener(NoRedirect)
    request = urllib.request.Request(pe.BASE + path, headers=headers or {})
    try:
        with opener.open(request, timeout=15) as response:
            body = response.read()
            return response.status, response.headers.get("Content-Type", ""), len(body), body[:8] == b"\x89PNG\r\n\x1a\n"
    except urllib.error.HTTPError as error:
        body = error.read()
        return error.code, error.headers.get("Content-Type", ""), len(body), False
    except (urllib.error.URLError, OSError):
        return 0, "", 0, False


def main():
    try:
        app = pe.read_env(pe.APP_ENV)
        env = pe.read_env(pe.ADMIN_ENV)
        pe.BASE = app["ROCKETTUI_SERVER_URL"].rstrip("/")
        dev = (app["ROCKETTUI_USER_ID"], app["ROCKETTUI_TOKEN"])
        admin_pass = env["ADMIN_PASS"]
    except (OSError, KeyError):
        print("PROBE FAIL: no dev state files; run dev/seed.py up and seed first")
        return 1
    pe.SECRETS.update(v for v in (dev[0], dev[1], admin_pass) if v)
    if urllib.parse.urlparse(pe.BASE).hostname not in pe.LOOPBACK:
        pe.out("PROBE FAIL: not a loopback server")
        return 1
    admin = pe.login("admin", admin_pass)
    ok = True
    try:
        pe.out("create: %d" % cap.create_custom_emoji(admin, NAME, ""))
        time.sleep(0.5)
        status, listing = pe.api("GET", "/api/v1/emoji-custom.list", auth=dev)
        entries = [e for e in ((listing or {}).get("emojis") or {}).get("update", []) if e.get("name") == NAME]
        if status != 200 or not entries:
            pe.out("list: no entry (%d)" % status)
            return 1
        etag, ext = entries[0].get("etag", ""), entries[0].get("extension", "")
        pe.out("list: extension=%s etag_present=%s" % (ext, "yes" if etag else "no"))
        headers = {"X-User-Id": dev[0], "X-Auth-Token": dev[1]}
        path = "/emoji-custom/%s.%s" % (NAME, ext)
        for label, p, h in (
            ("with auth headers", path, headers),
            ("with auth headers and etag", path + "?etag=" + etag, headers),
            ("with auth headers, other etag", path + "?etag=zzzzzz", headers),
            ("no headers at all", path, {}),
            ("no headers, with etag", path + "?etag=" + etag, {}),
            ("missing name, auth headers", "/emoji-custom/rt_no_such_emoji_zz.png", headers),
        ):
            status, ctype, size, magic = get(p, h)
            pe.out("GET %s: %d %s bytes=%d png=%s" % (label, status, ctype, size, "yes" if magic else "no"))
            if label.startswith("with auth") or label.startswith("no headers"):
                ok = ok and status == 200 and magic
    finally:
        cap.delete_custom_emoji(admin, NAME)
        status, _ = pe.api("GET", "/api/v1/emoji-custom.list", auth=dev)
        _, listing = pe.api("GET", "/api/v1/emoji-custom.list", auth=dev)
        left = [e for e in ((listing or {}).get("emojis") or {}).get("update", []) if e.get("name") == NAME]
        pe.out("emoji deleted: %s" % ("yes" if not left else "NO"))
        ok = ok and not left
    pe.out("PROBE OK" if ok else "PROBE FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
