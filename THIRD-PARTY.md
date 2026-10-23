# Third-party notices

Stecyk3D is made of two parts under two licences (see [README.md](README.md)): the Rust renderer and tools (MIT, `LICENSE`) and the Blender add-on (GPL-3.0-or-later, `blender_addon/stecyk3d/LICENSE`). This file lists everything in the package that someone else wrote.

## Code carried in the repository

| What | Where | Licence | Notice |
|---|---|---|---|
| The double-precision `pow` of the Arm Optimized Routines (Szabolcs Nagy, Arm Limited, 2018), translated into Rust, with its data tables, as musl v1.2.6 carries it | `src/numerics/published_pow.rs`, `src/numerics/pow_data.rs` | MIT | `LICENSES/musl-COPYRIGHT.md` (Copyright (c) 2018 Arm Limited; musl: Copyright (c) 2005-2020 Rich Felker, et al.). The notice must stay with these files. |

Nothing else is copied from another project. The SHA-256, PNG writer, JSON reader and all other code were written for this project.

## Rust dependencies

None. `Cargo.lock` lists only the `stecyk3d` crate itself. The programs link the Rust standard library (MIT OR Apache-2.0), which is part of the Rust toolchain and is not distributed here.

## Python dependencies of the add-on

None are bundled. The add-on runs inside Blender and uses Blender's own Python API (`bpy`, `gpu`, `mathutils`; GPL, which is why the add-on is GPL-3.0-or-later). It uses `numpy` (BSD-3-Clause) only if Blender provides it, and falls back without it.

## Licence compatibility

- MIT code in the renderer can be used inside a GPL-3.0-or-later work; the renderer is a separate program that the add-on starts as a child process, so the add-on's GPL does not extend to it.
- The musl/Arm `pow` is MIT and sits inside the MIT part.
- The add-on's zip contains only the add-on's own Python files and the GPL text.

## Bryce

"Bryce" is a trademark of its owners. Stecyk3D is an independent project and is not affiliated with or endorsed by them. The package contains **no file, table, picture or other data from any Bryce program**: the data a render needs is taken by the user, on their own machine, from their own Bryce install (see `docs/manual/substrate.md`).
