#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""The SHA-256 of the levels `stecyk3d-render --serve` gives for each order, uninterrupted (to compare two renderer builds).
usage: serve_sha.py RENDERER SUBSTRATE DESCRIPTION WxH ['{"antialias": 4}']"""
import json, os, struct, subprocess, sys, tempfile
r, sub, desc, size = sys.argv[1:5]
render = json.loads(sys.argv[5]) if len(sys.argv) > 5 else {}
w, h = map(int, size.split("x"))
d = json.load(open(desc))
srv = subprocess.Popen([r, "--substrate", sub, "--serve"], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
def rd(n):
    b = srv.stdout.read(n)
    assert len(b) == n
    return b
assert rd(8)[:4] == b"VRDY"
tmp = tempfile.mkdtemp()
for i, order in enumerate(["plain", "rows", "scattered"], 1):
    req = {"id": i, "description": desc, "levels": os.path.join(tmp, "l%d" % i), "order": order, "size": [w, h], "threads": 0}
    if render: req["render"] = dict(d["render"], **render)
    srv.stdin.write((json.dumps(req) + "\n").encode()); srv.stdin.flush()
    while True:
        tag = rd(4)
        if tag == b"VBEG": rd(4)
        elif tag == b"VSTA":
            x, y, n = struct.unpack("<III", rd(12)); rd(3 * n)
        elif tag == b"VEND":
            _, st, n = struct.unpack("<III", rd(12)); print(order, st, rd(n).decode()); break
srv.stdin.close(); srv.wait()
