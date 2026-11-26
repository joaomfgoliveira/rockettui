#!/usr/bin/env python3
"""Measure thread unread state on the local dev server (change add-thread-unread, task 1.1).

Usage: python3 -I dev/capture_thread_unread.py                          (from the repository root)
       RT_PROJECT=rockettui-dev9 python3 -I dev/capture_thread_unread.py   (the 9.0.0 stack)

Standard library only. Runs against the throwaway local server that dev/seed.py started, and
reuses the helpers of dev/probe_edit.py and dev/probe_ddp.py. The server must be on loopback and
credentials are read only from dev/state/<RT_PROJECT>/, as in the other probes.

What it does, as the seeded users `dev`, `alice` and `bob` (all message text is synthetic):
  * makes `dev` take part in threads (dev follows a thread it replies to), then has alice and bob
    reply, so dev's subscription of `rt-public` gets `tunread`, `tunreadUser` and `tunreadGroup`;
  * listens on DDP (`<uid>/subscriptions-changed`) while the replies are sent and records what the
    updates carry, key names and thread-field values (ids are replaced by placeholders);
  * tries four ways to mark a thread read (REST subscriptions.read with and without `tmid`, REST
    chat.getThreadMessages, DDP method `readThreads`) and, for each, reports yes/no whether the
    thread left `tunread`. Each attempt gets a fresh unread thread.

Output files, per stack (suffix _9 for 9.0.0), in src/api/fixtures/docker/:
  thread_unread_subscription, thread_unread_update

Output rule: stdout carries only labels, status codes, key names and counts. Fixtures are written
only after a check that no credential occurs in them. Thread ids are replaced by T1, T2, ...
"""

import importlib.util
import json
import os
import sys
import time
import urllib.parse

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIXTURES = os.path.join(REPO, "src", "api", "fixtures", "docker")


def load(name):
    spec = importlib.util.spec_from_file_location(name, os.path.join(REPO, "dev", name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


pe = load("probe_edit")
pd = load("probe_ddp")
SUFFIX = "_9" if pe.PROJECT.endswith("9") else ""
THREAD_FIELDS = ("tunread", "tunreadUser", "tunreadGroup")


def post(auth, room, text, tmid=None):
    message = {"rid": room, "msg": text}
    if tmid:
        message["tmid"] = tmid
    status, parsed = pe.api("POST", "/api/v1/chat.sendMessage", {"message": message}, auth)
    time.sleep(0.3)  # chat.sendMessage has its own five-per-second limiter
    sent = (parsed or {}).get("message") if isinstance(parsed, dict) else None
    mid = sent.get("_id") if isinstance(sent, dict) else None
    pe.out("post%s status: %d" % (" reply" if tmid else "", status))
    if status != 200 or not mid:
        pe.fail("setup could not post a message")
    return mid


def own_subscription(dev, room):
    status, listing = pe.api("GET", "/api/v1/subscriptions.get", auth=dev)
    if status != 200:
        pe.fail("subscriptions.get answered %d" % status)
    for sub in (listing or {}).get("update", []):
        if sub.get("rid") == room:
            return sub
    pe.fail("no subscription for rt-public")


def unread_threads(dev, room):
    return set(own_subscription(dev, room).get("tunread") or [])


def fresh_unread_thread(dev, other, room, label):
    """A thread dev follows (dev replied) with one later reply by `other`; returns the parent id."""
    parent = post(dev, room, "threadunread %s parent" % label)
    post(dev, room, "threadunread %s dev reply" % label, tmid=parent)
    post(other, room, "threadunread %s other reply" % label, tmid=parent)
    for _ in range(10):  # the subscription is updated asynchronously
        if parent in unread_threads(dev, room):
            return parent
        time.sleep(0.3)
    pe.out("  note: %s thread did not become unread" % label)
    return parent


class Placeholders:
    """Replace thread ids by T1, T2, ... in a document, consistently across fixtures."""

    def __init__(self):
        self.map = {}

    def name(self, value):
        if value not in self.map:
            self.map[value] = "T%d" % (len(self.map) + 1)
        return self.map[value]

    def apply(self, document):
        for field in THREAD_FIELDS:
            if isinstance(document.get(field), list):
                document[field] = [self.name(v) for v in document[field]]
        return document


def scrub(value):
    if isinstance(value, dict):
        return {k: scrub(v) for k, v in value.items() if k not in ("lastMessage", "md")}
    if isinstance(value, list):
        return [scrub(item) for item in value]
    return value


def write(name, document, secret_values):
    name = name + SUFFIX + ".json"
    text = json.dumps(scrub(document), indent=2) + "\n"
    for value in secret_values:
        if value and value in text:
            pe.out("CAPTURE FAIL: a credential occurs in %s; nothing written" % name)
            sys.exit(1)
    os.makedirs(FIXTURES, exist_ok=True)
    path = os.path.join(FIXTURES, name)
    with open(path, "w") as handle:
        handle.write(text)
    pe.out("wrote %s" % os.path.relpath(path, REPO))


def main():
    try:
        app = pe.read_env(pe.APP_ENV)
        env = pe.read_env(pe.ADMIN_ENV)
        pe.BASE = pd.BASE = app["ROCKETTUI_SERVER_URL"].rstrip("/")
        passwords = {"alice": env["ALICE_PASS"], "bob": env["BOB_PASS"]}
        dev = (app["ROCKETTUI_USER_ID"], app["ROCKETTUI_TOKEN"])
    except (OSError, KeyError):
        print("CAPTURE FAIL: no dev state files; run dev/seed.py up and seed first")
        return 1
    parsed_url = urllib.parse.urlparse(pe.BASE)
    if parsed_url.hostname not in pe.LOOPBACK:
        print("CAPTURE FAIL: not a loopback server")
        return 1
    pe.SECRETS.update(v for v in (*dev, *passwords.values()) if v)
    alice = pe.login("alice", passwords["alice"])
    bob = pe.login("bob", passwords["bob"])
    secret_values = [dev[1], alice[1], bob[1], *passwords.values()]
    pe.SECRETS.update(secret_values)

    status, info = pe.api("GET", "/api/info")
    pe.out("server version key present: %s" % ("yes" if (info or {}).get("version") else "no"))

    status, listing = pe.api("GET", "/api/v1/subscriptions.get", auth=dev)
    room = next((s["rid"] for s in (listing or {}).get("update", []) if s.get("name") == "rt-public"), None)
    if not room:
        pe.out("CAPTURE FAIL: rt-public not found")
        return 1
    pe.api("POST", "/api/v1/channels.join", {"roomId": room}, bob)

    placeholders = Placeholders()

    # --- Live updates: listen on DDP while replies are sent.
    ws = pd.WebSocket(parsed_url.hostname, parsed_url.port or 80, "/websocket")
    ws.recv_json(5)
    ws.send_json({"msg": "connect", "version": "1", "support": ["1", "pre2", "pre1"]})
    if (ws.recv_json(5) or {}).get("msg") != "connected":
        pe.out("CAPTURE FAIL: DDP connect did not answer connected")
        return 1
    result, error, _ = pd.ddp_call(ws, "login", [{"resume": dev[1]}], "1")
    if result is None or error is not None:
        pe.out("CAPTURE FAIL: DDP login with the PAT failed")
        return 1
    state, _ = pd.sub(ws, "stream-notify-user", ["%s/subscriptions-changed" % dev[0], False], "s-subs")
    pe.out("sub subscriptions-changed: %s" % state)
    ws.drain(1)

    # Thread A: dev follows it, bob replies (plain). Thread B: bob replies mentioning dev.
    # Thread C: bob replies mentioning @all (tunreadGroup).
    parent_a = post(dev, room, "threadunread A parent")
    post(dev, room, "threadunread A dev reply", tmid=parent_a)
    ws.drain(1)
    post(bob, room, "threadunread A plain reply from bob", tmid=parent_a)
    updates = [m for m in pd.events_of(ws.drain(2.5)) if is_sub_update(m)]
    pe.out("updates after a plain reply: %d" % len(updates))

    parent_b = post(dev, room, "threadunread B parent")
    post(dev, room, "threadunread B dev reply", tmid=parent_b)
    ws.drain(1)
    post(bob, room, "threadunread B reply mentioning @dev", tmid=parent_b)
    updates += [m for m in pd.events_of(ws.drain(2.5)) if is_sub_update(m)]

    parent_c = post(dev, room, "threadunread C parent")
    post(dev, room, "threadunread C dev reply", tmid=parent_c)
    ws.drain(1)
    post(bob, room, "threadunread C reply mentioning @all", tmid=parent_c)
    updates += [m for m in pd.events_of(ws.drain(2.5)) if is_sub_update(m)]

    summary = []
    for message in updates:
        args = message["fields"]["args"]
        doc = args[1]
        # Only updates about rt-public matter here; the others are counted and dropped.
        if doc.get("rid") != room:
            continue
        pe.out("  update action=%s thread fields present: %s; keys: %s" % (
            args[0] if isinstance(args[0], str) and args[0].isalpha() else "?",
            ",".join(f for f in THREAD_FIELDS if f in doc) or "none",
            ",".join(sorted(doc.keys()))))
        summary.append({"action": args[0], "subscription": placeholders.apply(dict(doc))})

    time.sleep(1)
    subscription = own_subscription(dev, room)
    pe.out("subscription thread fields present: %s" % (",".join(f for f in THREAD_FIELDS if f in subscription) or "none"))
    pe.out("counts: tunread=%d tunreadUser=%d tunreadGroup=%d" % tuple(
        len(subscription.get(f) or []) for f in THREAD_FIELDS))
    write("thread_unread_subscription", placeholders.apply(dict(subscription)), secret_values)
    write("thread_unread_update", {"updates": summary}, secret_values)

    # --- How can a thread be marked read? Each attempt gets a fresh unread thread.
    working = []

    def parent_ok(parent):
        return True

    def check(label, attempt):
        parent = fresh_unread_thread(dev, bob, room, label)
        status = attempt(parent)
        time.sleep(1)
        left = parent not in unread_threads(dev, room)
        pe.out("mark read via %s: status=%s left tunread=%s" % (label, status, "yes" if left else "no"))
        if left and parent_ok(parent) and not working:
            working.append((label, attempt))

    def rest_read(body_for):
        def run(parent):
            status, _ = pe.api("POST", "/api/v1/subscriptions.read", body_for(parent), dev)
            return status
        return run

    def thread_messages(parent):
        status, _ = pe.api("GET", "/api/v1/chat.getThreadMessages", auth=dev, query={"tmid": parent})
        return status

    ddp_seen = []

    def ddp_attempt(method, params_for):
        def run(parent):
            result, error, others = pd.ddp_call(ws, method, params_for(parent), "rt-%s-%s" % (method, parent[:8]))
            ddp_seen.extend(m for m in others if is_sub_update(m))
            if error:
                # Error codes and reasons are server vocabulary, not user text; print them short.
                pe.out("  ddp error: %s" % ",".join("%s=%s" % (k, str(error.get(k))[:60]) for k in ("error", "errorType", "reason") if k in error))
                return "error"
            return "ok"
        return run

    check("REST subscriptions.read {rid}", rest_read(lambda p: {"rid": room}))
    check("REST subscriptions.read {rid, tmid}", rest_read(lambda p: {"rid": room, "tmid": p}))
    check("REST chat.getThreadMessages", thread_messages)
    def rest_thread(body_for):
        def run(parent):
            status, parsed = pe.api("POST", "/api/v1/chat.readThread", body_for(parent), dev)
            if status != 200 and isinstance(parsed, dict):
                pe.out("  rest error: %s" % str(parsed.get("errorType") or parsed.get("error"))[:80])
            return status
        return run

    check("REST chat.readThread {tmid}", rest_thread(lambda p: {"tmid": p}))
    check("REST chat.readThread {tmid, rid}", rest_thread(lambda p: {"tmid": p, "rid": room}))
    check("DDP readThreads [tmid]", ddp_attempt("readThreads", lambda p: [p]))
    check("DDP readThreads [{tmid}]", ddp_attempt("readThreads", lambda p: [{"tmid": p}]))

    # A clean capture: one thread, quiet socket, one read by the method that worked, then listen.
    after = []
    if working:
        label, attempt = working[0]
        pe.out("clean read capture via %s" % label)
        parent = fresh_unread_thread(dev, bob, room, "clean")
        time.sleep(1)
        ws.drain(1.5)
        before = unread_threads(dev, room)
        ddp_seen.clear()
        attempt(parent)
        after = ddp_seen + [m for m in pd.events_of(ws.drain(2.5)) if is_sub_update(m) and m["fields"]["args"][1].get("rid") == room]
        pe.out("tunread before=%d after=%d" % (len(before), len(unread_threads(dev, room))))
    else:
        pe.out("no method marked a thread read on this server")
    pe.out("updates seen while threads were read: %d" % len(after))
    for m in after:
        doc = m["fields"]["args"][1]
        pe.out("  read update thread fields: %s" % (",".join("%s=%d" % (f, len(doc[f])) for f in THREAD_FIELDS if f in doc) or "none"))
    write(
        "thread_unread_read_update",
        {"updates": [{"action": m["fields"]["args"][0], "subscription": placeholders.apply(dict(m["fields"]["args"][1]))} for m in after]},
        secret_values,
    )
    pe.out("CAPTURE OK")
    return 0


def is_sub_update(message):
    fields = message.get("fields") or {}
    args = fields.get("args") or []
    return (
        message.get("collection") == "stream-notify-user"
        and len(args) >= 2
        and isinstance(args[1], dict)
        and "subscriptions-changed" in str(fields.get("eventName", ""))
    )


if __name__ == "__main__":
    sys.exit(main())
