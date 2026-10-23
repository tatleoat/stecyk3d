# Overview and architecture

## Goals

- Draw Bryce 3.0, 3.1 and 5.0 scenes and match the pictures bit for bit, where the scene is supported.
- No external Rust dependencies. One library crate (`stecyk3d`) and three small programs. A Blender add-on (Python) drives the renderer as a child process.
- Staged design: stages pass plain data in batches, so the same renderer can later run in a graphics-card form and give the same pictures.

## Data flow

```
scene file (.br3/.br5)        Blender scene
        |                           |
   scene reader                add-on: description (JSON)
        |                           |
        +----------> Scene <--------+      + substrate (tables)
                        |
                   render plan  (objects, lights, sky, camera)
                        |
   pixel order -> sample places -> camera rays
                        |
                  hits (geometry)
                        |
        colour (surface shading / sky)   <- further rays: reflection, transmission
                        |
           sum per pixel -> levels -> edge pass -> PNG / raw levels
```

The stages are listed in `src/render/stages.rs`:

1. **Order and places**: which pixels, in what order, and where inside each pixel the samples go.
2. **Camera rays**: sample places become rays.
3. **Hits**: each ray against the scene gives hit or miss, distance, point, normal and object.
4. **Colour**: a hit goes to the surface rules, a miss to the sky rules. Reflected and transmitted rays are walked as a tree on an explicit stack, not by recursion.
5. **Sum and levels**: samples are averaged per pixel and turned into 8-bit levels.
6. **Edge pass**: pixels on colour edges are redone with more rays.

Every request for a colour carries a tag saying which pass and sample it belongs to. Some textures keep state between calls, so colours are asked for in the order stage 1 produced them.

## Module map of `src/`

| Module | What it holds |
|---|---|
| `numerics/` | Arithmetic that must match the Bryce programs: rounding, power, logarithm, a 53-bit register emulation, the C-runtime random generator. |
| `colour.rs` | Colour spaces (RGB/HLS) and conversions. |
| `geom.rs` | `Rgb`, `Vec3` and small vector helpers. |
| `geometry/`, `geometry.rs` | Shapes and ray intersection: solids, quartic roots (torus), meshes, metaballs, terrain relief, trees, groups, the spatial structure that decides which object is hit first. |
| `shading/` | Surface rules: lighting, shadows, materials, further rays, media (glass), bump, fuzzy and soft effects, volume colour, visible lights. |
| `sky/` | Sky colours, air (haze and fog), clouds, stars, moon, rainbow, Bryce 5's sky. |
| `textures/` | Procedural textures: noise types, spaces and mapping, octaves, filters, colour maps, blending, picture textures; the lookup tables come from the substrate. |
| `trees/` | Tree growth from settings into geometry, leaves, wood. |
| `terrain*.rs` | Terrain heights, shapes, fractal tools, editor commands, pictures. |
| `animation/` | Time maps, tracks, links, paths, aiming, opening a scene at a time. |
| `scene*.rs` | The scene-file reader and its parts: records, materials, lights, sky, textures, trees, tracks. `scene_export` writes JSON for the add-on. |
| `description.rs`, `description/` | The JSON description that the Blender add-on writes, turned into a `Scene`. |
| `substrate*.rs`, `sha256.rs` | The substrate file, its tables and kept files; an own SHA-256. |
| `extract.rs` | Makes a substrate from install folders. |
| `sampling/` | Camera, lens, sample places, anti-aliasing pass, pixel order, blocks, output levels, PNG writer. |
| `render/` | Plan, stages, lit rules, modes (mask/distance/altitude), animation frames, the multi-core render (`parallel.rs`), the quick preview (`preview.rs`). |
| `cases/` | The case command (`stecyk3d-cases`): renders cases from a file of JSON lines, for checking the library without writing Rust. |
| `bin/` | `stecyk3d-render` (also `--stream` for the live picture and `--serve` for the viewport), `stecyk3d-extract`, `stecyk3d-cases`. |

## Conventions

- Arithmetic order is explicit and no fused multiply-add is used, so results do not depend on the compiler.
- Where the program refuses an unsupported combination, it returns an error that names it. It does not approximate.
- Shared types are small and `Copy`: batches of rays, hits and colours are plain structs of numbers with no references.
