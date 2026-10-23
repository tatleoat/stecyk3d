# The Blender add-on

Location: `blender_addon/stecyk3d/`. Pure Python, no packages beyond Blender's own.

## Shape

Two layers:

| Layer | Modules | Notes |
|---|---|---|
| Pure | `convert.py`, `axes.py`, `camera_frame.py`, `scene_read.py`, `lamps.py`, `solids.py`, `channels.py`, `numbers.py`, `reader.py`, `presets.py`, `components.py`, `create.py`, `importing.py`, `stream.py` | Turn plain values into plain values. They do not import `bpy`, so they are tested with ordinary `python3 -m unittest`. |
| Blender | `engine.py`, `panels.py`, `props.py`, `nodes.py`, `gather.py`, `material_nodes.py`, `viewport*.py`, `import_op.py`, `preset_ops.py`, `create_ops.py`, `terrain_tool.py` | Thin: read Blender data into plain dicts, draw panels, register types. |

`__init__.py` imports `bpy` only inside `register()`.

## Rendering: a child program

1. `gather.py` reads the scene into plain dicts.
2. `convert.py` turns them into a **description**: JSON with the camera, objects, lights, materials, sky and settings. Axes and units are changed from Blender's to the renderer's; numbers are rounded to the single-precision values the renderer expects.
3. `engine.py` writes the description to a temp file and starts `stecyk3d-render --description D.json --levels L --stream --rows` (or `--scattered`, by the **Live order** setting) as a **child process**, on every core. Nothing links against the renderer, so a crash in the renderer cannot take Blender down.
4. When it finishes, the add-on reads the levels file, puts the picture into the render result, and removes the temp files.

## Streaming (the live picture)

With `--stream`, the renderer writes records to standard output as it draws: the bytes `VSTA`, then x, y, n as three little-endian 32-bit floats, then n pixels of RGB. The levels' hash goes to standard error. The add-on reads the pipe without blocking (`stream.py`): each turn it takes everything the pipe holds (up to 64 reads of 1 MB) with no pause while there is more, and only when the pipe is empty does it wait, for at most 10 ms and only until data arrives (`select`), never a fixed sleep. It keeps a levels buffer and a float rectangle, and refreshes the picture up to four times per second. Turning the levels into Blender's floats is a single bulk copy.

`--rows` hands the pixels over row by row from the top, in order, as the rows finish. `--scattered` visits pixels in a fixed pseudo-random permutation (seed 1), so the picture appears as noise that sharpens. The streamed picture is only a live view: the final picture always comes from the levels file, not from the stream.

### Random grain

Some shading draws random numbers (soft shadows, radius lights, jittered volumes, the wash, anti-aliasing with random places), and the order of the draws then depends on the order of the pixels. The **Random grain** setting (identifier `wash_grain`) chooses what to do:

- **Auto** (nothing is passed): the renderer keeps the program's serial order of draws until the frame has made a fixed number of draws (100 million; a count of work, never a clock, so every machine gets the same picture), then starts over giving every pixel a place of its own in the sequence, which runs on every core.
- **Exact** (`--exact-random`): always the serial order, one core.
- **Fast** (`--independent-random`): a place of its own for every pixel from the start, on every core.

Where the grain is not exact, the picture has the same kind of grain but not the same numbers. Scenes with metaballs or trees also run on every core, with the same picture as one core gives. `STECYK3D_DRAWS` (`exact`, `independent` or a number) sets the same thing for the command line.

## The Rendered viewport

`viewport_camera.py` converts the view matrices to the description's camera and builds a key that detects changes. `viewport_session.py` keeps one session per viewport. It talks to one long-lived `stecyk3d-render --serve` process: one JSON request per line on its input, framed answers on its output (the stream's records, then an end record with the request's status). A newer request stops the render going on.

How a change restarts the picture:

1. A change of the view key, the scene, **Live order** or **Random grain** tells the renderer at once to cancel the render going on (`{"cancel": true}`), and the old request's pixels are no longer taken.
2. After the view has been still for 40 ms, one new request is sent. The scene's description is written once per scene change, not per view or per step.
3. The new picture starts from the previous one, resampled if the size changed, so new pixels replace old ones.
4. With Live order Scattered the first request is coarse (at most about 30 000 pixels, one ray per pixel) and each following request is twice as fine until the asked size is reached; with Rows the full size is asked at once. **Viewport quality** and **Viewport pixel size** change the request.
5. When a request ends, the picture is replaced by the exact levels the renderer wrote.

In the viewport, Auto grain means a place of its own for every pixel from the start (no exact attempt first); choose **Exact** to get the program's own grain there too. F12 is not affected. The viewport never writes scene settings.

## Colour

The add-on sets the view transform to Raw, look to None and dither to 0, so Blender does not change the renderer's colours.

## Presets, import, create

`stecyk3d-render read ...` prints JSON for libraries, documents and terrain relief. `reader.py` calls it; `importing.py` plans an import, `create.py` makes new shapes and terrains.

## Files saved under the first name

The first release went by a working name (another extension id, engine id, program names and default substrate folder). `migrate.py` is the one place that still spells it. When a file is opened, its scenes are moved to the Stecyk3D engine (the old engine id is registered for the moment the file is read, because Blender resets a scene whose engine is unknown), the scene's plain custom properties are renamed, and the old extension's saved preference values (the Bryce folders, substrate file, program paths) are copied once into this one's. The old default substrate folder is still read when the new one has no file. Nothing needs to be done by the user.

## Tests

`blender_addon/tests/` has pure tests (`python3 -m unittest`, from `blender_addon/`) and Blender scripts; see [Testing](testing.md).
