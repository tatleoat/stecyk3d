# Testing

## Principle

The aim is bit-exact pictures, so tests compare **exact bytes**, not "close enough" images. A picture is summarised by the SHA-256 of its levels (width x height x 3 bytes): a single wrong bit in a single pixel changes the hash.

## What ships

| Layer | Where | What is checked |
|---|---|---|
| Unit tests | `#[cfg(test)]` modules in `src/` | Small functions: arithmetic helpers, colour conversion, the SHA-256, quartic roots, the substrate reader. Floats are compared by their bit patterns. |
| Integration tests | `tests/*.rs` that build their scenes in code | Scene-file reading, geometry, shading and sky rules, the description reader for the add-on's kinds, and `tests/serve.rs`: the long-lived renderer (`--serve`) gives exactly what a program started for the request gives, in every order, after any history of requests; a newer request ends the render going on; a bad request fails alone. |
| Add-on unit tests | `blender_addon/tests/test_*.py` | Pure Python: conversion to the description, the stream decoder, the viewport session and its restart and ladder, presets, import, migration of the first release's names. They need no Blender. |
| Blender scripts | `blender_addon/tests/blender_*.py` | Headless Blender: F12 on the factory scene, F12 on a scene with a sun and one object of each kind with a pixel-for-pixel colour check, the live picture against the exact one, a refused render's report, a terrain picture that is not a grid size against one that is, the Rendered viewport with a fake region, the Bryce nodes, the panels, preferences and Make substrate, and a file saved under the first release's names. |

Unsupported combinations must fail with an error that names them, not draw something; the refusal report test and the render refusals in the unit tests check that.

## Running them

```
cargo test --release
cd blender_addon && python3 -m unittest discover -s tests
bash blender_addon/tests/run_all.sh        # needs Blender; builds the programs first
```

`run_all.sh` builds the release programs, makes a small test substrate with `make_test_substrate.py` (it holds only made-up values, enough for the checks, not Bryce's data), builds and validates the extension zip, installs it into a scratch Blender profile and runs the Blender scripts. Set `BLENDER` to the Blender program if it is not called `blender`. Output goes to `out/tests`. The last line is `RUN-ALL-OK` when everything passed.

Tests that need a substrate (such as `tests/serve.rs`) take its path from `BRYCE_SUBSTRATE` and skip themselves when it is not set. Set `STECYK3D_TRACE_THREADS=1` to see on standard error whether a render ran on all cores or stayed serial, and why.

## Checking your own render

`stecyk3d-render` prints a line `levels <sha256>` for a document render, and a streamed render prints the same line on standard error. Two renders of the same scene give the same hash on any machine, on any number of cores (for scenes whose shading draws random numbers, with Random grain set to Exact).
