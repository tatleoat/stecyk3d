# Stecyk3D

> **Experimental.** First release. It draws most scenes the way the Bryce programs do and refuses, with a reason, what it cannot draw. Read [the known differences](docs/manual/troubleshooting.md) before relying on a picture.

Stecyk3D is a renderer, written in Rust with no third-party dependencies, that draws Bryce 3.0, 3.1 and 5.0 scenes the way those programs do, and a Blender add-on ("Stecyk3D for Blender") that lets you render your Blender scenes with it: F12, the Rendered viewport, Bryce material, sky and light nodes in the Shader Editor, Bryce shapes and terrains in the Add menu, and import of saved `.br3` / `.br5` scenes. The renderer is a separate program the add-on starts, so it also runs on its own from the command line.

## Build and install

You need a Rust toolchain (edition 2024, Rust 1.85 or newer) and, for the add-on, Blender 4.2 or newer (tested with 5.2).

```
cargo install --path . --locked        # stecyk3d-render, stecyk3d-extract, stecyk3d-cases into ~/.cargo/bin
blender --command extension build --source-dir blender_addon/stecyk3d --output-dir /tmp
blender --command extension install-file -r user_default -e /tmp/stecyk3d-0.1.0.zip
```

Then, in *Edit > Preferences > Add-ons > Stecyk3D*, fill in the folder of each Bryce version you own and press **Make substrate** (once). Set the render engine to **Stecyk3D** and press F12. The whole walk-through is in [docs/manual](docs/manual/README.md).

### Programs and where the add-on looks for them

The add-on does not contain the programs; build them with `cargo build --release` (they land in `target/release/`) or `cargo install`. For each program (`stecyk3d-render`, `stecyk3d-extract`) it looks, in order: the path set under Advanced in the add-on preferences; a `bin` folder beside the add-on; a folder on `PATH`; `$CARGO_TARGET_DIR/release`, a `target/release` in a folder above the add-on, and `~/.cargo/bin`. A prebuilt Linux x86_64 download may be offered later as a GitHub release; it is not part of this repository or of the extension zip.

## You need your own Bryce

The renderer needs some fixed data tables and a few data files (a moon picture, a star catalogue, tree leaves, preset libraries). **None of it is in this package.** `stecyk3d-extract` (the add-on's *Make substrate* button) reads them once, on your machine, from your own Bryce 3D 3.0, 3.1 or Bryce 5.0 install and writes a private "substrate" file. Without it, a render stops and says what is missing.

## Documentation

- [User manual](docs/manual/README.md): installing, the substrate, rendering from Blender, command line, known limits.
- [How it works](docs/how-it-works/README.md): architecture, renderer, add-on, testing, extending.
- [CHANGELOG.md](CHANGELOG.md)

## Licence

- The Rust renderer and tools (everything outside `blender_addon/`): **MIT**, see [LICENSE](LICENSE).
- The Blender add-on (`blender_addon/`): **GPL-3.0-or-later**, see [blender_addon/stecyk3d/LICENSE](blender_addon/stecyk3d/LICENSE). Blender requires this for add-ons that use its Python API.
- Third-party code and notices: [THIRD-PARTY.md](THIRD-PARTY.md).

"Bryce" is a trademark of its owners; Stecyk3D is an independent project, not affiliated with or endorsed by them.
