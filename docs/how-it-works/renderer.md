# The renderer

This page describes what the code does at the level a graphics programmer wants.

## Rays and cameras

`sampling/camera.rs` turns a pixel and a sample place into a ray. Projections: perspective, parallel (orthographic), spherical and panorama kinds, and a lens model for depth of field (`sampling/lens.rs`). The three program versions differ in the order of floating-point operations, so each has its own variant of the ray arithmetic. Start-of-ray and axis computations are done in the order that gives the matching bits.

## Intersection

`geometry/` has one routine per primitive: plane, sphere, cube, cylinder, cone, pyramid, disc, square, slab, torus and more. Quadrics are solved in closed form; the torus uses a quartic solver. Meshes are triangle lists. Each hit reports distance, point, normal and texture coordinates.

Scenes use a spatial structure (a tree or a grid, chosen by version) to decide which object a ray tests next. When two objects are hit at exactly the same distance, a fixed rule decides which one is reported, because it changes the picture.

## Shading

`shading/` and `render/lit.rs` compute the colour at a hit:

1. Material channels (diffuse, ambient, specular, transparency, reflection, bump and so on) can be constant or driven by textures.
2. For each light: shadow ray, then diffuse and specular terms. Light kinds: sun, radial, spot, parallel, beam lights and visible (volumetric) lights.
3. Further rays: a reflected ray and a transmitted ray, each with a weight. Refraction uses an angle law and tracks the medium the ray is inside; total internal reflection is handled. Depth is limited to 6.
4. Air (haze and fog) and volume colour are laid over the result by distance.

The ray tree is walked depth first on an explicit stack. Nothing recurses and there is no allocation per ray.

## Sky

`sky/` returns the colour for a ray that misses everything: gradient dome and horizon, sun and its glow, haze and fog, clouds (stratus and cumulus, with wind), stars, a moon image, rainbows. Bryce 5 has its own sky model (`sky/five.rs`) with a star catalogue read from the substrate.

## Terrains

A terrain is a height field. The renderer clips the ray to the terrain's bounding box, then walks the field cell by cell and solves for the crossing in each cell. The walk looks one cell ahead: each cell's exit is computed once, one step before it is needed, and reused. Heights come from the editor's operations in `terrain_*.rs` (fractal generation, shapes, stretches, tools) or from a picture.

## Metaballs

Each metaball adds a field value. A set of balls forms one object: the renderer finds the crossings of the summed field along the ray and reports hits on the single set object. Negative balls subtract. Each ball's material is mixed by its share of the field at the hit.

## Trees

Trees are grown from settings (trunk, branches, leaves) into geometry, then rendered like other solids and meshes. Leaves can come from picture files kept in the substrate.

## Textures

Textures are procedural. A texture picks a noise type, a space and mapping, octave method, filter, phase and a colour map, then blends into the channel it drives. Picture textures are also supported. Some noise functions use lookup tables from the substrate. Textures that keep their previous inputs make results depend on shading order, so that order is fixed.

## Anti-aliasing and output

- Each pixel takes several sample places (a grid, or random places from the C-runtime generator with a fixed seed). Samples are averaged.
- An edge pass finds pixels whose neighbours differ and re-renders them with extra rays at fixed offsets; the result replaces the pixel.
- A higher-quality mode adds cones of rays for soft effects.
- Averaged colours become 8-bit levels by version-specific rules (gamma, dithering in 5.0). The PNG writer is our own (zlib stored blocks).

## Pixel order

Pixels are visited in a fixed order that depends on the version and block settings. The order matters: random sample places are drawn in that sequence. The live picture can use other orders (rows from the top, or a fixed scattered permutation; see [the add-on](blender-addon.md)). Where an order would change the random draws, the shared-out render gives every pixel a place of its own in the generator's sequence (the Random grain setting), and the final levels still come from the render that was asked for.

## Threads and speed

The renderer uses every core (`--threads` sets another count). The first pass and the edge pass are cut into chunks of pixels; the random generator is stepped through the whole picture first, so each chunk starts from the state a serial render would have given it, and the picture equals the serial one byte for byte. Scenes with metaballs or trees are shared out the same way: the jobs start from a guess of the metaballs' state and are checked against the real one in order. A render stays on one core only where the result depends on pixel order and cannot be reproduced: a texture that keeps a result between calls, or shading that draws random numbers under the Exact grain (see [the add-on](blender-addon.md)).

Speedups that leave every picture byte-identical:

- The sine and cosine of the extended-precision arithmetic are memoised per thread (a pure function of the function and the argument), and the long division that reduces the argument works in wide steps. Texture materials ask for the same few angles over and over.
- A group's member boxes are cached per thread; a group asks for each member's box on every ray.
- Each texture's noise space matrices are cached per thread, keyed on the exact bits they are computed from.
- The terrain walk computes each cell's exit one cell ahead.
- A group walk puts member crossings straight into the role sequence instead of building a list per member per ray.
- The structure walk (the lookup of where a ray meets the Bryce objects) keeps one reusable mailbox per thread, stamped with a walk number, in place of a fresh vector for every ray.
- The box test stops as soon as the interval of the ray inside the box is empty, instead of finishing the other axes.
- Stars are looked up through a widened grid for every program: each star is entered in every cell its widened disc can reach, so a pixel looks only at the stars of its own cell. The candidates are visited in the order the full loop over all stars would have used, so the picture is unchanged.
