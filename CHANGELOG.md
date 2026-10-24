# Changelog

Newest first.

## Unreleased

### Fixed

- The Rendered viewport no longer looks washed out until the first F12: the scene is set to the Raw view transform (look None, dithering 0) when the viewport first starts on it, and when a Bryce document is imported. Blender's Color Management panel is now shown with the Stecyk3D engine.
- The Material tab's Texture components panel and the per-channel mix rows no longer stop with an error when a texture or mix comes through a node group.

### Name

- The project is now called Stecyk3D (add-on "Stecyk3D for Blender", render engine "Stecyk3D", programs `stecyk3d-render`, `stecyk3d-extract`, `stecyk3d-cases`). Files and settings saved under the earlier working name move over by themselves: opening a scene switches it to the Stecyk3D engine, its custom properties are renamed, and the Bryce folders, substrate file and program paths from the old preferences are copied once. The old default substrate file is still found.

### Blender add-on

- F12 is faster: the add-on no longer pauses while it reads the renderer's picture, and it hands the picture to Blender much faster (a small scene at 640x480: about 1 s -> 0.1 s; at 1920x1440: about 13 s -> 0.5 s). Pictures are unchanged.
- The Rendered viewport restarts at once when you move the view: the render going on is cancelled, and the new view's first pixels appear in a fraction of a second.
- The Rendered viewport draws every pixel from its own random place from the start (set Random grain to Exact to get the program's own grain there too). F12 is unchanged.
- The Render tab's "Wash grain" setting is now called "Random grain", since it covers soft shadows, radius lights and jittered volumes as well as the wash. Saved files keep their setting.
- Panels now offer the settings of the old Bryce interface: the full Render, World (sky), Object, Material and Texture settings, light key buttons and the Light Lab gradient starter, texture mapping, noise, filter, colours, phase and blend, and Mix Materials limits.
- The World tab says where a cloud layer's texture goes (the Bryce Sky node's stratus or cumulus input, in the Shader Editor set to World), and its red lines wrap instead of being cut off. The manual has step-by-step instructions for changing the sky and giving a cloud layer a texture.

### Renderer

- Scenes with metaballs or trees render on every core (glass-vine F12 35.5 s -> 9.7 s; tree-dense 5.0 s -> 0.63 s), with the same picture as before.
- Scenes whose shading uses random numbers no longer spend their exact-order budget on the quick preview before the frame (wash-vol 122-129 s -> 89 s; a soft-shadow scene 63.5 s -> 36 s), same picture.
- The exact mode is faster on scenes heavy in sine, cosine and arctangent, textures, groups and terrains (on one core: smoker 9.5 s -> 1.8 s, clouds 18.4 s -> 7.7 s; vlx and terrain1 also faster). Pictures are unchanged.
- Renders of 3.0 and 3.1 scenes with stars are faster, and rendering is faster again overall, with the same pictures (the CD's space station on one core 1.65 s -> 1.07 s).

## 0.1.0 - first release (experimental)

- Renderer (Rust, MIT): draws Bryce 3.0, 3.1 and 5.0 scenes (`.br3`, `.br5`) and scenes described by the Blender add-on, bit for bit wherever it draws, and refuses with a named reason where it does not. Uses every core.
- Blender add-on (GPL-3.0-or-later): render engine for F12 and the Rendered viewport; live picture with Rows or Scattered order; the Bryce node set in the Shader Editor (Add > Bryce) with node groups followed; Bryce shapes, terrains and groups in the Add menu; Principled BSDF to Bryce material conversion; presets; import of `.br3` / `.br5` documents; a text report of everything a refused render found.
- The substrate (tables and data files the renderer needs) is made by you from your own Bryce install; the package contains none.
- Documentation: a user manual and a "how it works" explanation in `docs/`. Known differences and limits: `docs/manual/troubleshooting.md`.
