#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Idle requests: each request sent to an idle `--serve` must end finished (status 0), never cancelled by its own arrival.
usage: serve_idle_race.py RENDERER SUBSTRATE DESCRIPTION [COUNT]"""
import json, os, struct, subprocess, sys, tempfile
r, sub, desc = sys.argv[1:4]
count = int(sys.argv[4]) if len(sys.argv) > 4 else 300
srv = subprocess.Popen([r, "--substrate", sub, "--serve"], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
def rd(n):
    b = srv.stdout.read(n); assert len(b) == n; return b
assert rd(8)[:4] == b"VRDY"
tmp = tempfile.mkdtemp(); bad = 0
for i in range(1, count + 1):
    req = {"id": i, "description": desc, "levels": os.path.join(tmp, "l"), "order": "rows", "size": [16, 16], "threads": 0}
    srv.stdin.write((json.dumps(req) + "\n").encode()); srv.stdin.flush()
    while True:
        tag = rd(4)
        if tag == b"VBEG": rd(4)
        elif tag == b"VSTA":
            x, y, n = struct.unpack("<III", rd(12)); rd(3 * n)
        elif tag == b"VEND":
            _, st, n = struct.unpack("<III", rd(12)); rd(n)
            bad += st != 0; break
srv.stdin.close(); srv.wait()
print("%d requests, %d not finished" % (count, bad)); sys.exit(1 if bad else 0)
