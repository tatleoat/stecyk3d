# Stecyk3D user manual

Stecyk3D is a render engine for Blender. It draws your Blender scene the way Bryce 3D 3.0, 3.1 or Bryce 5.0 would, with a renderer written in Rust. You work in Blender as usual and press F12.

> **Experimental.** This is a first release. It draws most scenes the way the Bryce programs do, and says so plainly when it cannot. See [Known differences and limits](troubleshooting.md) before you rely on a picture.

Start at the top and go down. Most people need pages 1 to 3.

1. [Installing](installing.md): build the renderer, install the Blender add-on, make your first render.
2. [Making the substrate](substrate.md): the one data file every render needs, made from your own Bryce install.
3. [Rendering from Blender](blender.md): F12, the Rendered viewport, the Bryce nodes, importing Bryce scenes.
4. [Scripting](scripting.md): build and render Bryce scenes from a Python script, and compare several looks side by side.
5. [Command-line rendering](command-line.md): the renderer without Blender (it is a separate program, so it can run on its own).
6. [File formats](file-formats.md): which files are read and written.
7. [Known differences, limits and troubleshooting](troubleshooting.md).

For programmers there is also [How it works](../how-it-works/README.md).

## What you need

- Blender 4.2 or newer (developed and tested with Blender 5.2).
- Your own copy of Bryce 3D 3.0, 3.1 and/or Bryce 5.0. Stecyk3D contains no Bryce data; it reads a few tables and data files from your install, once, on your machine.
- A Rust toolchain to build the renderer.
