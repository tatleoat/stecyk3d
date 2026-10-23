# File formats

## Read

| File | Used for |
|---|---|
| `.br3` | Bryce 3D 3.0 / 3.1 scene (binary). Rendered with `--document`, or imported in Blender. |
| `.br5` | Bryce 5.0 scene. Same use. |
| `.mat` | Material preset library. |
| `.brt` | Texture preset library. |
| Substrate (`substrate.bin` and `substrate.files/`) | Made by `stecyk3d-extract` from your own Bryce install, [see here](substrate.md). |
| Description (`.json`) | What the Blender add-on writes for each render. |

What a scene file holds that is read: the camera and view, render settings, objects (solids, meshes, terrains, metaballs, trees, groups, lights), materials and their textures, the sky, and animation tracks. Parts of a file whose version the program does not know are refused with a message instead of being guessed.

## Written

| File | Content |
|---|---|
| PNG | 8-bit RGB picture. |
| Levels | Raw bytes, width x height x 3, top row first. |
| `NAME-NNNN.png` | Frames of an animation range. |
| Stream | Records on standard output (internal protocol between add-on and renderer; see [How it works](../how-it-works/blender-addon.md)). |
| JSON | Output of `stecyk3d-render read ...`. |
| `substrate.bin` and `substrate.files/` | Written by `stecyk3d-extract`. |
