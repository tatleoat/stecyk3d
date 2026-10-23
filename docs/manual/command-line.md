# Command-line rendering

The renderer is a separate program, so it does not need Blender at all. Use it to render saved Bryce scenes, to render a description, to script a series of frames or to check that two machines give identical pictures.

`stecyk3d-render` has three jobs.

## Render a saved scene

```
stecyk3d-render [--substrate S] --document FILE.br5 --as 3.0|3.1|5.0 --picture OUT.png [--levels L] [--threads N]
```

| Option | Meaning |
|---|---|
| `--substrate S` | The substrate file. Optional if `BRYCE_SUBSTRATE` is set or the default place has one. |
| `--document F` | A saved scene (`.br3` or `.br5`). |
| `--as V` | Required. Render the way program 3.0, 3.1 or 5.0 does. |
| `--picture OUT` | Where to write an 8-bit RGB PNG. |
| `--levels L` | Also write the raw levels: width x height x 3 bytes, top row first. |
| `--threads N` | Worker threads. `0` or absent: every core; `1`: the serial render. The picture is the same bytes either way. |
| `--mode M` | Draw one of the maps instead of the shaded picture: `mask`, `distance` or `altitude` (comma-separated choices are resolved by fixed rules). Only for a document render, without `--stream`, `--scattered` or `--rows`. |
| `--time T` | Render the scene as it stands at T seconds of its animation. Not with `--mode` or `--frames`. |
| `--frames FIRST-LAST` or `--frames all` | Render a range of frames, one PNG per frame, named `NAME-NNNN.png`. |
| `--series NAME` | Name and place of the frame files. Default: the document's name without extension, in its folder. |
| `--exact-random`, `--independent-random` | How scenes whose shading uses random numbers draw them: the program's own order (default for documents), or a place of its own for every pixel (faster, many cores). Same meaning as **Random grain** in Blender (Exact / Fast; Auto is neither flag). |

On success a single picture (not a `--frames` range) prints a line `levels <SHA-256 of the levels>`. Two renders that print the same line are identical to the last bit, on any machine.

## Render a Blender description

```
stecyk3d-render --substrate S --description D.json (--levels L | --picture OUT.png) [--stream] [--scattered | --rows] [--threads N]
```

This is the form the Blender add-on uses. `D.json` is the scene as the add-on describes it, with a `size` field. `--stream` sends finished pixels to standard output as they are drawn (the SHA-256 line goes to standard error); `--scattered` draws them in a fixed random order, `--rows` from the top to the bottom. The pixel stream is the add-on's internal protocol; see [the add-on page of How it works](../how-it-works/blender-addon.md).

## Stay running (the Rendered viewport)

```
stecyk3d-render --substrate S --serve
```

Reads one JSON request per line on standard input and answers on standard output, so the Rendered viewport pays the start-up cost once. A newer request stops the render going on. It ends when its standard input is closed. It takes no document, description or stream option.

## Read things as JSON

```
stecyk3d-render read library FILE.mat|FILE.brt
stecyk3d-render read library --substrate S --kept PATH
stecyk3d-render read kept-libraries --substrate S --as V
stecyk3d-render read document FILE.br3|FILE.br5 [--as V]
stecyk3d-render read relief SEED
```

These print JSON for the add-on: preset libraries, a scene's contents (used by *Import*; with `--as V` the scene as program V opens it, animation tracks applied) and the relief of a new terrain. You can use them to inspect a scene.

## Other programs

`stecyk3d-extract` makes the substrate ([Making the substrate](substrate.md)). `stecyk3d-cases CASE_FILE SUBSTRATE_FILE` is the checker the tests use; you do not need it to render.

## Notes

- None of the programs has a `--help`. `stecyk3d-render` run with a wrong option prints a line of usage and the reason, for example `unknown argument "--help"`, and exits with status 2. `stecyk3d-extract` takes anything that is not `--out` or `--bryce` as an install folder.
- Exit status is 0 on success, 1 when the scene or substrate is refused, 2 when the command line is wrong.
