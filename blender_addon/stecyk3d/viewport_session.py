# SPDX-License-Identifier: GPL-3.0-or-later
# (The Rendered viewport: the change key, the picture as it arrives, timer)
"""One Rendered viewport's work: the long-lived renderer (`stecyk3d-render --serve`), its stream, the picture. Pure Python (`subprocess`,
`tempfile`); no bpy, no thread. `tick` is the only thing that moves, and it never waits."""

import json
import os
import sys
import time as _time
import shutil
import subprocess
import tempfile

from .stream import ServeDecoder, StreamDecoder, grain_name, live_order_name, resample
from .viewport_camera import picture_size, view_key, viewport_members

DEBOUNCE = 0.04  # seconds the view must be still before a new request is sent: under one timer step, so it only joins
# the changes of one tick (the renderer is told to stop at once, so waiting longer buys nothing; design note)
READ_BUDGET = 0.02  # seconds of one tick spent reading and decoding the pipe
READ_CHUNK = 1 << 18
KILL_AFTER = 5.0
LADDER_PIXELS = 30000  # the first picture of a change has at most this many pixels; each later one has 4 times more
MAX_DIVISOR = 16
DEBUG = bool(os.environ.get("STECYK_VIEWPORT_DEBUG"))  # opt-in timing lines on standard error


def debug(text):
    if DEBUG:
        print("[stecyk3d-viewport %.2f] %s" % (_time.monotonic() % 10000, text), file=sys.stderr, flush=True)


def first_divisor(width, height, pixel_size):
    """The power of two (1, 2, 4, ... MAX_DIVISOR) by which the first picture of a change is coarser than the
    asked one: the least that brings it to LADDER_PIXELS or fewer pixels. A picture renders at about two thousand
    pixels a second on a scene like the glass vine, so the first thing seen must be small."""
    divisor = 1
    while divisor < MAX_DIVISOR:
        w, h = picture_size(width, height, pixel_size * divisor)
        if w * h <= LADDER_PIXELS:
            break
        divisor *= 2
    return divisor


class ViewportSession:
    """The state of one viewport engine instance.

    `command()` returns the argument list of the long-lived renderer (the engine builds it from the preferences).
    `clock` is a function giving seconds (tests give their own). One renderer process serves every request of the
    session: the scene's description is written once for each change of the scene, and a request names it with the
    view's camera, quality and size; a newer request ends the render going on (The
    long-lived renderer)."""

    def __init__(self, command, clock):
        self.command = command
        self.clock = clock
        self.base = None  # the scene's description (gather.describe), or None
        self.units = 1.0
        self.program = "5.0"
        self.state = None  # the last ViewState
        self.key = None
        self.message = ""  # why there is nothing new to show (a refusal, a failed renderer), or ""
        self.levels = None  # the picture: RGB, top row first
        self.size = (0, 0)
        self.version = 0  # grows whenever `levels` changes
        self.final = False  # the picture is the finished exact one
        self.progress = 0.0
        self.wash_grain = "AUTO"  # the 'Wash grain' setting (stream.grain_name)
        self.live_order = "ROWS"  # the 'Live order' setting (stream.live_order_name)
        self.divisor = None  # the rung of the ladder now rendering (1 = the asked size); None before the first
        self.starts = 0  # requests sent (the debug aid and the tests)
        self.server_starts = 0  # renderer processes started
        self._server = None
        self._server_command = None
        self._folder = None  # the session's folder: the description files, the levels files, the error text
        self._stderr = None
        self._serving = ServeDecoder()
        self._active = None  # the id of the request whose picture is wanted, or None
        self._sent_at = 0.0
        self._next_id = 1
        self._base_path = None
        self._base_dirty = True  # the description in the file is not the scene's
        self._written = 0
        self._decoder = None
        self._dirty = None  # the first and last row of the picture that changed since `take_dirty`
        self._pending_since = None  # the time of the last change, while a request is wanted

    # ------------------------------------------------------------ what the engine tells it

    def set_scene(self, base, units, program):
        """The scene's description changed (or is first known): a new request is wanted. The same description again
        (an update that changed nothing the renderer reads) changes nothing."""
        if self.base is not None and units == self.units and program == self.program and base == self.base:
            return
        self.base = base
        self.units = units
        self.program = program
        self._base_dirty = True
        self._changed()

    def set_view(self, state):
        """The view as it is now. Returns True when it differs from the last one (a new request is wanted)."""
        key = view_key(state)
        self.state = state
        if key == self.key:
            return False
        self.key = key
        self._changed()
        return True

    def _changed(self):
        self._cancel_render()
        self.stop_run()
        self.final = False
        self.divisor = None  # a change starts the ladder again from its coarse first rung
        self._pending_since = self.clock()

    def set_live_order(self, order):
        """The 'Live order' setting as it is now; a change restarts the live render."""
        if order != self.live_order:
            self.live_order = order
            if self.base is not None:
                self._changed()

    def set_wash_grain(self, grain):
        """The 'Wash grain' setting as it is now; a change restarts the live render."""
        if grain != self.wash_grain:
            self.wash_grain = grain
            if self.base is not None:
                self._changed()

    def _mark(self, first, last):
        d = self._dirty
        self._dirty = (first, last) if d is None else (min(d[0], first), max(d[1], last))

    def take_dirty(self):
        """The first and last row of the picture that changed since the last call (the whole of it after a new size or
        the exact levels), or None. The texture is brought up to date from these rows alone."""
        dirty, self._dirty = self._dirty, None
        return dirty

    def busy(self):
        """True while a request is out or one is waiting to be sent: the timer keeps ticking."""
        return self._active is not None or self._pending_since is not None

    # ------------------------------------------------------------ the renderer

    def _cancel_render(self):
        """Tell the renderer at once that the render going on is not wanted. The next request comes only when the view
        has been still (DEBOUNCE); without this a drag leaves the old view rendering all the while, and the renderer's
        cores are not free for the new view when it comes."""
        if self._active is None or self._server is None:
            return
        try:
            self._server.stdin.write(b'{"cancel": true}\n')
            self._server.stdin.flush()
        except (OSError, ValueError):
            pass  # the renderer is gone (or its pipe closed): tick says so
        debug("cancel sent for request %s" % self._active)

    def stop_run(self):
        """Forget the request in flight: its pixels are no longer taken. The renderer is told to stop by `_changed`
        (before this) and, failing that, by the next request."""
        self._active = None
        self._decoder = None
        self._serving.drop()

    def _levels_path(self, request_id):
        return os.path.join(self._folder, "levels-%d.rgb" % request_id)

    def _stop_server(self):
        server = self._server
        self._server = None
        self._active = None
        self._serving = ServeDecoder()
        if server is not None:
            try:
                server.stdin.close()  # the renderer ends when its input does
            except OSError:
                pass
            try:
                server.wait(timeout=1.0)
            except subprocess.TimeoutExpired:
                server.terminate()
                try:
                    server.wait(timeout=KILL_AFTER)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait()
            server.stdout.close()
        if self._stderr is not None:
            self._stderr.close()
            self._stderr = None
        if self._folder is not None:
            shutil.rmtree(self._folder, ignore_errors=True)
            self._folder = None
        self._base_path = None
        self._base_dirty = True

    def close(self):
        self.stop_run()
        self._stop_server()
        self._pending_since = None

    def _ensure_server(self):
        """The renderer process, started when there is none (or the program asked for has changed)."""
        command = list(self.command())
        if self._server is not None and (command != self._server_command or self._server.poll() is not None):
            self._stop_server()
        if self._server is not None:
            return True
        self._folder = tempfile.mkdtemp(prefix="stecyk3d_vp_")
        self._stderr = open(os.path.join(self._folder, "stderr.txt"), "wb")
        try:
            self._server = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                            stderr=self._stderr)
        except OSError as error:
            self.message = "the renderer program cannot be started: %s" % error
            self._stop_server()
            return False
        os.set_blocking(self._server.stdout.fileno(), False)  # blender-integration.md 5.1
        self._server_command = command
        self.server_starts += 1
        debug("renderer %d started" % self.server_starts)
        return True

    def _write_base(self):
        """The scene's description, written once for each change of the scene (not for each view or rung)."""
        self._written += 1
        old = self._base_path
        self._base_path = os.path.join(self._folder, "scene-%d.json" % self._written)
        began = _time.monotonic()
        text = json.dumps({k: v for k, v in self.base.items() if k != "viewport_scene_camera"})  # dumps: the C encoder is several times faster than dump's
        with open(self._base_path, "w", encoding="utf-8") as f:
            f.write(text)
        self._base_dirty = False
        if old is not None:
            try:
                os.remove(old)
            except OSError:
                pass
        debug("description written, %d bytes in %.2fs" % (len(text), _time.monotonic() - began))

    def _start(self):
        self._pending_since = None
        if self.base is None or self.state is None:
            return
        state = self.state
        if self.divisor is None:
            # ROWS is the old recomp's live render: the asked size at once, one whole row after another from the top,
            # no coarse rungs. Only SCATTERED climbs the ladder. (Live order)
            self.divisor = 1 if self.live_order == "ROWS" else first_divisor(state.width, state.height, state.pixel_size)
        last = self.divisor == 1
        # A coarse rung is a look at the scene, not the picture: one ray a pixel. The last rung is the asked one.
        rung = state if last or state.quality != "RENDER" else state._replace(quality="NORMAL")
        size = picture_size(state.width, state.height, state.pixel_size * self.divisor)
        try:
            members = viewport_members(self.base, rung, self.units, self.program, size)
        except ValueError as error:
            self.message = str(error)
            return
        self.message = ""
        if size != self.size or self.levels is None:  # another size stands in with the old picture, resampled
            self.levels = (resample(self.levels, self.size, size) if self.levels is not None and self.size[0]
                           else bytearray(b"\xff" * (size[0] * size[1] * 3)))
            self.size = size
            self.version += 1
            self._mark(0, size[1] - 1)
        if not self._ensure_server():
            return
        if self._base_dirty or self._base_path is None:
            self._write_base()
        request_id = self._next_id
        self._next_id += 1
        request = dict(members, id=request_id, description=self._base_path, levels=self._levels_path(request_id),
                       order=live_order_name(self.live_order), threads=0, draws=grain_name(self.wash_grain))
        try:
            self._server.stdin.write((json.dumps(request) + "\n").encode("utf-8"))
            self._server.stdin.flush()
        except OSError:
            self._lost_server()
            return
        self._active = request_id
        self._decoder = StreamDecoder(size[0], size[1], self.levels)
        self._serving.begin(request_id, self._decoder)
        self.progress = 0.0
        self.starts += 1
        self._sent_at = _time.monotonic()
        debug("request %d: rung 1/%d, %dx%d, %s, quality %s" % (request_id, self.divisor, size[0], size[1],
                                                                 self.live_order, rung.quality))

    def _lost_server(self):
        """The renderer process has ended on its own: say why."""
        error = ""
        if self._stderr is not None:
            self._stderr.flush()
            try:
                with open(os.path.join(self._folder, "stderr.txt"), "rb") as f:
                    error = f.read().decode("utf-8", "replace").strip()
            except OSError:
                pass
        self.message = "The renderer stopped: " + (error or "no reason given")
        self.version += 1
        self.stop_run()
        self._stop_server()

    def tick(self):
        """Send a waiting request when the view has been still, read what the pipe holds, finish an ended request.
        Returns True when the picture changed (the engine then asks for a redraw)."""
        now = self.clock()
        if self._pending_since is not None and now - self._pending_since >= DEBOUNCE:
            self._start()
        server = self._server
        if server is None:
            return False
        changed = False
        deadline = now + READ_BUDGET
        ends = []
        while True:
            try:
                chunk = os.read(server.stdout.fileno(), READ_CHUNK)
            except OSError:  # BlockingIOError: nothing at the moment
                chunk = b""
            if not chunk:
                break
            ends += self._serving.feed(chunk)
            if self.clock() >= deadline:
                break
        decoder = self._decoder
        if decoder is not None and decoder.dirty:
            decoder.dirty = False
            if decoder.rows is not None:
                self._mark(*decoder.rows)
                decoder.rows = None
            self.progress = decoder.fraction()
            self.version += 1
            changed = True
        for request_id, status, text in ends:
            changed |= self._ended(request_id, status, text)
        if not ends and server.poll() is not None:
            self._lost_server()
            changed = True
        return changed

    def _ended(self, request_id, status, text):
        """A request is over. Only the one wanted matters; the others (passed by a newer request) are cancelled."""
        path = self._levels_path(request_id)
        wanted = request_id == self._active
        exact = None
        if wanted and status == 0:
            try:
                with open(path, "rb") as f:
                    exact = f.read()
            except OSError as error:
                self.message = "The renderer's picture cannot be read: %s" % error
        try:
            os.remove(path)
        except OSError:
            pass
        if not wanted:
            return False
        self._active = None
        self._decoder = None
        self._serving.drop()
        if status != 0:
            self.message = "The renderer refused: " + text
            self.version += 1
            return True
        if exact is None:
            self.version += 1
            return True
        if len(exact) != len(self.levels):
            self.message = "The renderer gave %d bytes, not %d" % (len(exact), len(self.levels))
            return True
        self.levels[:] = exact  # the picture is the exact render's, not the stream's
        self._mark(0, self.size[1] - 1)
        self.version += 1
        debug("request %d done in %.1fs, rung 1/%d" % (request_id, _time.monotonic() - self._sent_at, self.divisor))
        if self.divisor > 1:  # the next rung, twice as fine, at once
            self.divisor //= 2
            self.progress = 0.0
            self._start()
            return True
        self.final = True
        self.progress = 1.0
        return True
