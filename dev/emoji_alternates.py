#!/usr/bin/env python3
"""Other names the server's own emoji data gives each emoji (helper for dev/probe_react_names.py).

Usage: python3 -I dev/emoji_alternates.py <emoji.tsv> <shortcodes dir> > alts.tsv

`emoji.tsv` is the dump of the `emojis` crate (`glyph<TAB>first<TAB>second..` per line, see the
docstring of `probe_react_names.py`). `<shortcodes dir>` is the `emojibase-data/en/shortcodes`
directory of the server's bundle, copied out of the throwaway dev container, for example:
    docker cp rockettui-dev-rocketchat-1:/app/bundle/programs/server/npm/node_modules/emojibase-data/en/shortcodes DIR

Each `*.json` there maps an emoji's hexcode (its code points joined by `-`, without a trailing
variation selector) to one shortcode or a list. For every emoji of the dump this prints
`first<TAB>name<TAB>name..`: every name the data gives it that the crate does not already list.
These are only candidates; the probe is what asks the server which ones it accepts. A name that the
crate already uses for a different emoji is never a candidate: the server would accept it (it is a
valid name) but it would mean the other emoji, so a reaction sent under it would be the wrong one.
Standard library only; reads two local paths and prints names, nothing else.
"""

import json
import os
import sys

SELECTOR = "️"


def hexcode(glyph):
    return "-".join("%04X" % ord(c) for c in glyph)


def main(argv):
    if len(argv) != 3:
        print("usage: emoji_alternates.py <emoji.tsv> <shortcodes dir>", file=sys.stderr)
        return 2
    by_hex = {}
    for name in sorted(os.listdir(argv[2])):
        if not name.endswith(".json") or name.endswith("-native.json"):
            continue
        with open(os.path.join(argv[2], name), encoding="utf-8") as handle:
            for key, value in json.load(handle).items():
                names = [value] if isinstance(value, str) else list(value)
                bucket = by_hex.setdefault(key, [])
                bucket.extend(n for n in names if n not in bucket)
    with open(argv[1], encoding="utf-8") as handle:
        rows = [line.rstrip("\n").split("\t") for line in handle]
    rows = [row for row in rows if len(row) >= 2]
    # Which emoji the crate gives each of its names to.
    owner = {}
    for row in rows:
        for name in row[1:]:
            owner.setdefault(name, row[1])
    if True:
        for parts in rows:
            glyph, codes = parts[0], parts[1:]
            names = by_hex.get(hexcode(glyph)) or by_hex.get(hexcode(glyph.replace(SELECTOR, ""))) or []
            extra = [n for n in names if n not in codes and owner.get(n, codes[0]) == codes[0]]
            if extra:
                print("\t".join([codes[0]] + extra))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
