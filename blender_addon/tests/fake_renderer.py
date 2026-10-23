# SPDX-License-Identifier: GPL-3.0-or-later
# (Tests: the viewport session's fake program). Not the renderer: it keeps the
# renderer's command line and stream, and draws a picture that depends on the description's camera.
"""A stand-in for stecyk3d-render: `--substrate S --serve` (the add-on's long-lived renderer: requests as lines of JSON
on standard input, the framed answer on standard output; a newer request, or {"cancel": true}, ends the render going on) and the one-shot
form `--substrate S --description D --levels L [--stream] [--scattered]`.
Pixel (x, y) is the level (7x + 13y + the camera's first position number) mod 256 in red, the next two one more each.
The stream sends the pixels in runs of 4. FAKE_DELAY seconds are slept after each row (a render that lasts);
FAKE_FAIL makes it refuse. FAKE_LOG names a file to which it appends "stopped <id> <time>" for each render stopped."""

import json
import os
import select
import struct
import sys
import time


def serve():
    out = sys.stdout.buffer
    out.write(b"VRDY" + struct.pack("<I", 1))
    out.flush()
    buffer = bytearray()
    delay = float(os.environ.get("FAKE_DELAY", "0"))

    def line(wait):
        """The next request line, or None when there is none within `wait` seconds (b"" at the end of input)."""
        while b"\n" not in buffer:
            if not select.select([0], [], [], wait)[0]:
                return None
            chunk = os.read(0, 1 << 16)
            if not chunk:
                return b""
            buffer.extend(chunk)
        at = buffer.index(b"\n")
        text = bytes(buffer[:at])
        del buffer[:at + 1]
        return text

    nxt = line(None)
    while nxt:
        request = json.loads(nxt)
        if request.get("cancel"):  # nothing going on: nothing to stop
            nxt = line(None)
            continue
        desc = json.load(open(request["description"], encoding="utf-8"))
        for key in ("size", "camera", "render"):
            if key in request:
                desc[key] = request[key]
        rid = request["id"]
        out.write(b"VBEG" + struct.pack("<I", rid))
        nxt = None
        if os.environ.get("FAKE_FAIL"):
            text = b"the scene is refused"
            out.write(b"VEND" + struct.pack("<III", rid, 1, len(text)) + text)
            out.flush()
            nxt = line(None)
            continue
        width, height = desc["size"]
        shift = int(desc["camera"]["position"][0])
        levels = bytearray()
        for y in range(height):
            row = bytearray()
            for x in range(width):
                v = (7 * x + 13 * y + shift) % 256
                row += bytes([v, (v + 1) % 256, (v + 2) % 256])
            levels += row
            for x in range(0, width, 4):
                n = min(4, width - x)
                out.write(b"VSTA" + struct.pack("<III", x, y, n) + bytes(row[x * 3:(x + n) * 3]))
            out.flush()
            nxt = line(delay)  # a newer request ends this render
            if nxt is not None:
                break
        if nxt:
            out.write(b"VEND" + struct.pack("<III", rid, 2, 0))
            out.flush()
            if os.environ.get("FAKE_LOG"):  # when each render was stopped, for the tests
                open(os.environ["FAKE_LOG"], "a").write("stopped %d %.3f\n" % (rid, time.monotonic()))
            if json.loads(nxt).get("cancel"):  # stops the render, starts nothing
                nxt = line(None)
            continue
        if nxt == b"":
            return
        open(request["levels"], "wb").write(levels)
        out.write(b"VEND" + struct.pack("<III", rid, 0, 4) + b"done")
        out.flush()
        nxt = line(None)


def main():
    args = sys.argv[1:]
    if "--serve" in args:
        return serve()
    desc = json.load(open(args[args.index("--description") + 1], encoding="utf-8"))
    out = args[args.index("--levels") + 1]
    if os.environ.get("FAKE_FAIL"):
        sys.stderr.write("the scene is refused\n")
        sys.exit(2)
    width, height = desc["size"]
    shift = int(desc["camera"]["position"][0])
    delay = float(os.environ.get("FAKE_DELAY", "0"))
    levels = bytearray()
    for y in range(height):
        row = bytearray()
        for x in range(width):
            v = (7 * x + 13 * y + shift) % 256
            row += bytes([v, (v + 1) % 256, (v + 2) % 256])
        levels += row
        if "--stream" in args:
            for x in range(0, width, 4):
                n = min(4, width - x)
                sys.stdout.buffer.write(b"VSTA" + struct.pack("<III", x, y, n) + bytes(row[x * 3:(x + n) * 3]))
            sys.stdout.buffer.flush()
        if delay:
            time.sleep(delay)
    open(out, "wb").write(levels)


main()
