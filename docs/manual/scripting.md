# Scripting: building scenes in Python

Stecyk3D has a small Python module, `api`, for scripts that build a Bryce scene and render it: materials from Bryce's presets, Bryce shapes and terrains, the sky and lights, renders, and several looks of one scene side by side. Scripts that use only `api` (and Blender's own `bpy`) keep working when the add-on's insides change; the add-on's other modules are not a promise.

Like the rest of the add-on, the API does not guess: a name that does not exist, or a setting the scene's program does not have, is refused with an error that names it.

## Importing it

Blender installs an extension under a long package name, `bl_ext.<repository>.stecyk3d` (`bl_ext.user_default.stecyk3d` for one installed from a file or the default repository). A script that should work wherever the add-on is installed finds it by the end of the name:

```python
import importlib
import bpy

api = importlib.import_module(next(k for k in bpy.context.preferences.addons.keys() if k.endswith("stecyk3d"))
                              + ".api")
```

Where you know the name, `from bl_ext.user_default.stecyk3d import api` does the same. `api.package_name()` gives the name it was imported under. The add-on must be enabled, the substrate made (see [Making the substrate](substrate.md)), and the renderer found, as for F12.

## A complete example

Save this as `ball.py` and run `blender -b --python ball.py -- out`. It writes `out/ball.png`.

```python
import importlib, math, os
import bpy

api = importlib.import_module(next(k for k in bpy.context.preferences.addons.keys() if k.endswith("stecyk3d"))
                              + ".api")
out = api.arguments().positional[0]

api.begin(program="5.0", clear=True)  # Stecyk3D, as Bryce 5.0, in an emptied scene

# Materials: a Bryce preset (the first of the names that loads) with two channels changed, and one from a colour.
water = api.preset_material("Waters&Liquids", "Shiny and Still", channels={"reflection": 0.7})
rock = api.preset_material("Rocks&Stones", "Wet Rock", "Pitted Granite")
red = api.colour_material("Red", (0.8, 0.05, 0.03), roughness=0.3)

# A hill from a height function in metres, and a ball set on it by numbers.
hill = api.terrain("Hill", 128, (0.0, 30.0), (60.0, 40.0), (-1.0, 12.0),
                   lambda x, y: 10.0 * math.exp(-(x * x + (y - 34.0) ** 2) / 150.0) - 0.5, rock)
api.shape("plane", "Lake", (0, 0, 0), (100, 100, 1), material=water)
api.shape("sphere", "Ball", (3.0, 14.0, hill.lowest_under(3.0, 14.0, 1.0) + 1.0), material=red)

# The sky, a sun low on the right, and a camera (plain Blender).
api.sky(sun_intensity=1.2, sun_color=(1.0, 0.95, 0.85), haze=0.05, ambient_color=(0.3, 0.35, 0.45))
api.light("Sun", "SUN", direction=(0.6, -0.4, 0.5))
cam = bpy.data.objects.new("Camera", bpy.data.cameras.new("Camera"))
bpy.context.scene.collection.objects.link(cam)
bpy.context.scene.camera = cam
cam.location, cam.rotation_euler = (0.0, -6.0, 2.5), (math.radians(84), 0.0, 0.0)

api.render(os.path.join(out, "ball.png"), resolution=(480, 320))
```

A longer one is `examples/lake/build_lake.py` in the repository: a moonlit lake with a dock, three terrains, reeds and pines, built only with `api` and plain Blender meshes.

## What it offers

| Function | What it does |
|---|---|
| `begin(program="5.0", clear=False)` | Sets the render engine to Stecyk3D and the program (`"3.0"`, `"3.1"`, `"5.0"`). With `clear=True` (off by default, since it also empties a scene open in Blender), removes the scene's objects, gives it a new empty world and removes every datablock nothing uses any more. Returns the scene. |
| `preset_material(library, *names, channels=None)` | A new material from the first of the named presets of a material library that loads. The library is named as the Material panel shows it, without the folder in brackets (`"Waters&Liquids"`); a preset by its whole name or the start of it. Each preset that does not load is printed (`STECYK3D PRESET Rocks&Stones / Wet Rock won't load (...)`) and the next tried; the one used is printed too. `channels` puts Materials Lab values over the preset's: numbers, and colours as three display values (0 to 1, as the Bryce Material node shows them). |
| `colour_material(name, rgb, roughness=0.8, metallic=0.0, glow=None)` | A Bryce material made from a Principled BSDF's colour (linear, as Blender's colour picker gives it), roughness and metallic, by the add-on's **Bryce material from the Blender shader**. `glow=(rgb, strength)` is the emission: Bryce's ambient colour and ambience, so the object lights itself. |
| `shape(kind, name, location, scale=(1, 1, 1), rotation=(0, 0, 0), material=None)` | A Bryce shape as the Add menu makes it: plane, slab, cube, sphere, cylinder, cone, pyramid, torus, square, disc; metaball and tree with the program 5.0. Rotation in radians. Made directly, with no 3D cursor, selection or operator, so it works headless. |
| `terrain(name, n, centre, size, z_range, height, material=None)` | A Bryce terrain of n by n points (16, 32, ... 1024) spread over `size` (x, y) about `centre` (x, y), its heights `height(x, y)` in world units. `z_range` (low, high) is the height of the terrain's lowest and highest level; heights outside it are held to it and the number held is printed. Returns an object with `.object`, `.z_at(x, y)` and `.lowest_under(x, y, radius)`. |
| `sky(world=None, **settings)` | Gives the world a Bryce Sky node if it has none, then sets the settings by name (the World tab's: `sun_intensity`, `haze`, `zenith`, `stars`, and its switches such as `flag_8`, Custom colours). Returns the node's settings, for reading or setting more. |
| `light(name, kind="POINT", location=(0, 0, 0), direction=None, **settings)` | A lamp: POINT, SPOT or AREA with a Bryce Light node and its settings by name (`intensity`, `color`, `edge`, `reach`, ...), or a SUN. `direction` is the way from the scene to the light (the lamp's +Z); for a sun, the way to the sun. A sun's light is the sky's, so it takes no settings: use `sky(sun_intensity=..., sun_color=...)`. |
| `render(path, resolution=None)` | Renders the scene (as F12 does) to a PNG at `path` and returns the path. A relative path is taken from the folder the script runs in (Blender itself would take it from the .blend), and the folder is made. A refused render raises Blender's error, which names the problems. |
| `render_variants(build, variants, out_dir, resolution=None, sheet=None, columns=3)` | Several looks rendered one after the other in this Blender (below). |
| `render_parallel(script, variants, out_dir, blender=None, jobs=None, args=())` | Several looks rendered at once, each by a headless Blender of its own (below). |
| `contact_sheet(paths, out_path, labels=None, columns=3)` | One PNG of the pictures in a grid, each labelled below (the file name by default). The labels are drawn in capitals with a small built-in font. |
| `arguments()` | The script's arguments after Blender's `--`: `.positional` (those before the first flag), `.has("--quick")`, `.get("--moon", 9.0)` (of the default's type). |
| `variant_flags(overrides)` | A variant's overrides as the flags `render_parallel` passes (`{"moon": 12, "no_halo": True}` gives `--moon 12 --no-halo`). |

What is refused, with an error naming it:

- a library, preset or channel name that does not exist (a misspelt name is not a fallback), and a list of presets none of which loads;
- a channel that is a mix of two values (Materials Lab's altitude or alpha blend): its constant is not what shows;
- a terrain side that is not 16, 32, ... 1024, an empty `z_range` or size, and a height function that gives no number;
- a sky or light setting that does not exist, a value of the wrong kind, and a setting only Bryce 5.0 has (`sky5_...`, a light's `radius`, ...) while the program is 3.0 or 3.1;
- a metaball or tree while the program is not 5.0, and a shape kind the Add menu does not make;
- settings given to a SUN.

### Setting things on a terrain

`z_at(x, y)` is the height of the surface the renderer draws at a point, or None off the terrain. The renderer draws each cell of the grid as two flat triangles (split along one diagonal), so this is exact, not a smooth blend of the four corners. `lowest_under(x, y, radius)` is the lowest point of that surface over a disc, so a stone or a stump put at that height plus a little has no edge in the air. Both work from the heights as the terrain holds them (65,536 levels between the ends of `z_range`), and the parts of the disc off the terrain are left out.

## Comparing looks

### In one Blender

`render_variants` calls your `build` function once per variant with the variant's values as keyword arguments, renders, and writes `OUT_DIR/NAME.png`:

```python
def build(sun=1.2, haze=0.05):
    api.begin("5.0", clear=True)         # each variant starts from an empty scene
    ...                                  # the scene, as above
    api.sky(sun_intensity=sun, haze=haze)

api.render_variants(build, {"plain": {}, "hazy": {"haze": 0.3}, "bright": {"sun": 2.0}},
                    "out/looks", resolution=(320, 240), sheet="out/looks/sheet.png")
```

`variants` is `{name: values}` or a list of value dicts (named `v1`, `v2`, ...). Names become file names (letters, digits, `_`, `-`, `.`; anything else becomes `_`).

### In parallel

One Blender builds and renders one scene at a time, and building a big scene in Python can take longer than its render. `render_parallel` starts one headless Blender per variant and waits for all of them:

```
blender -b --python-exit-code 1 --python SCRIPT -- OUT_DIR --name NAME [args] [the variant's flags]
```

The script reads its folder, name and flags with `api.arguments()` and must write `OUT_DIR/NAME.png`. Each run's output goes to `OUT_DIR/NAME.log`. A variant whose Blender ends with an error, or that writes no picture, is reported by name with its log, after all have finished. `jobs` limits how many run at once (all by default); the renderer itself already uses every core, so the gain is in the building. The Blenders are the program running the call (`blender=` to choose another) and inherit its environment (such as `BLENDER_USER_RESOURCES`), so they load the same add-on and preferences.

`examples/lake/variants.py` renders three looks of the lake this way and puts them on one sheet:

```python
LOOKS = {"moon_9": {"moon": 9}, "moon_12_haze": {"moon": 12, "haze": 0.04},
         "glassy": {"reflect": 0.85, "bump": 0.004, "no_halo": True}}
paths = api.render_parallel("build_lake.py", LOOKS, out, args=["--quick"])
api.contact_sheet(paths, os.path.join(out, "sheet.png"),
                  labels=["%s  %s" % (name, " ".join(api.variant_flags(v))) for name, v in LOOKS.items()])
```

Run it with `blender -b --python examples/lake/variants.py -- out/lake --res 320x226`.

## Tips

- Run headless with `blender -b --python script.py -- ...`; add `--python-exit-code 1` so an error ends Blender with a failure.
- Each API call prints a line beginning `STECYK3D` (the preset used, terrain heights held, the picture written), so a headless log says what was made.
- Everything else in the scene is ordinary Blender: meshes made with `bpy.data.meshes.new` and `from_pydata` render as meshes, cameras are Blender cameras, and anything `api` does can still be changed afterwards in the Shader Editor and the panels.
