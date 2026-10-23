#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""How soon does `stecyk3d-render --serve` start a NEW view after a change that comes while an old one renders?

usage: serve_restart_latency.py RENDERER SUBSTRATE DESCRIPTION [--order rows|scattered|plain] [--size WxH]
                                [--after SECONDS] [--limit SECONDS] [--render-json '{"antialias": 4}']

Sends request 1 (the description's camera), waits `--after` seconds, sends request 2 (the camera turned a little),
and prints the seconds from sending request 2 to its VBEG and to its first pixels. Exit 1 when the first pixels of
request 2 take longer than `--limit` (default 1.0). The old request must still be running when 2 is sent (reported)."""
import json, os, struct, subprocess, sys, tempfile, threading, time, argparse

p = argparse.ArgumentParser()
p.add_argument("renderer"); p.add_argument("substrate"); p.add_argument("description")
p.add_argument("--order", default="rows"); p.add_argument("--size", default="320x180")
p.add_argument("--after", type=float, default=0.3); p.add_argument("--limit", type=float, default=1.0)
p.add_argument("--render-json", default="{}")
a = p.parse_args()
w, h = map(int, a.size.split("x"))
dirp = tempfile.mkdtemp(prefix="serve-latency-")
desc = json.load(open(a.description))
cam = desc["camera"]
cam2 = dict(cam, position=[cam["position"][0] + 0.4, cam["position"][1], cam["position"][2]])
render = json.loads(a.render_json)
srv = subprocess.Popen([a.renderer, "--substrate", a.substrate, "--serve"], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
events = []  # (time, kind, id)
def reader():
    f = srv.stdout
    def rd(n):
        b = f.read(n)
        if len(b) < n: raise EOFError
        return b
    try:
        while True:
            tag = rd(4)
            if tag == b"VRDY": rd(4)
            elif tag == b"VBEG": events.append((time.monotonic(), "beg", struct.unpack("<I", rd(4))[0]))
            elif tag == b"VSTA":
                x, y, n = struct.unpack("<III", rd(12)); rd(3 * n); events.append((time.monotonic(), "pix", None, y))
            elif tag == b"VEND":
                i, st, n = struct.unpack("<III", rd(12)); rd(n); events.append((time.monotonic(), "end", i, st))
            else: raise SystemExit("bad frame %r" % tag)
    except EOFError:
        pass
threading.Thread(target=reader, daemon=True).start()
def send(i, c):
    req = {"id": i, "description": a.description, "levels": os.path.join(dirp, "l%d.rgb" % i), "order": a.order,
           "size": [w, h], "camera": c, "threads": 0}
    if render: req["render"] = dict(desc["render"], **render)
    srv.stdin.write((json.dumps(req) + "\n").encode()); srv.stdin.flush()
t1 = time.monotonic(); send(1, cam)
time.sleep(a.after)
old_ended = any(e[1] == "end" and e[2] == 1 for e in events)
t2 = time.monotonic(); send(2, cam2)
deadline = t2 + 120
while time.monotonic() < deadline:
    if any(e[1] == "pix" and e[0] > t2 and any(b[1] == "beg" and b[2] == 2 for b in events) for e in events) and \
       any(e[1] == "beg" and e[2] == 2 for e in events):
        break
    time.sleep(0.005)
beg2 = next((e[0] for e in events if e[1] == "beg" and e[2] == 2), None)
begin_pix = next((e[0] for e in events if e[1] == "pix" and beg2 and e[0] > beg2), None)
end1 = next((e for e in events if e[1] == "end" and e[2] == 1), None)
print("order=%s size=%dx%d old request already finished before the change: %s" % (a.order, w, h, old_ended))
print("request 1 ended: %s" % ("status %d at +%.3fs after change" % (end1[3], end1[0] - t2) if end1 else "not within the wait"))
print("request 2 VBEG +%.3fs, first pixels +%.3fs after the change" % (
    (beg2 - t2) if beg2 else -1, (begin_pix - t2) if begin_pix else -1))
srv.stdin.close()
try: srv.wait(timeout=10)
except subprocess.TimeoutExpired: srv.kill()
ok = begin_pix is not None and begin_pix - t2 <= a.limit
print("PASS" if ok else "FAIL (limit %.2fs)" % a.limit)
sys.exit(0 if ok else 1)
