# Stecyk3D for Blender

**Experimental.** A Blender 4.2+ render engine that hands the scene to the Stecyk3D renderer (the Rust programs in this repository) and puts its picture back in Blender unchanged. GPL-3.0-or-later (see `stecyk3d/LICENSE`).

The add-on is the folder `stecyk3d/` (module `stecyk3d`; the engine shows as **Stecyk3D**). It contains no Bryce data: you make the substrate yourself from your own Bryce install with the preferences page's **Make substrate** button.

- Install, first render, nodes, import, settings, limits: the [user manual](../docs/manual/README.md), starting with [Installing](../docs/manual/installing.md).
- How the add-on is built: [How it works, the Blender add-on](../docs/how-it-works/blender-addon.md).

Quick start:

```
cargo install --path . --locked      # from the repository root
blender --command extension build --source-dir blender_addon/stecyk3d --output-dir /tmp
blender --command extension install-file -r user_default -e /tmp/stecyk3d-0.1.0.zip
```

## Tests

`tests/` holds pure Python tests and headless Blender scripts.

- Pure tests: `cd blender_addon && python3 -m unittest discover -s tests`. Tests that need the renderer use `target/release/stecyk3d-render` (or `STECYK3D_RENDERER`) and skip without it; tests that need a substrate use `BRYCE_SUBSTRATE` and skip without it.
- Blender scripts (`blender_*.py`) are driven by `tests/run_all.sh`; they need Blender, a built renderer and a substrate. Run them against a scratch `BLENDER_USER_RESOURCES`, never your own configuration.
