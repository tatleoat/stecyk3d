# Installing

Stecyk3D has two parts: the **Blender add-on** (Python, "Stecyk3D for Blender") and the **renderer programs** (Rust). The add-on starts the renderer as a child program, so both must be on the same machine. Nothing is linked into Blender, so a problem in the renderer cannot take Blender down.

## Requirements

| Need | Notes |
|---|---|
| Rust toolchain | Edition 2024 (Rust 1.85 or newer; developed on 1.93). Install with rustup. |
| Blender | 4.2 or newer by the add-on manifest; developed and tested with Blender 5.2. Older versions than 5.2 have not been tested. |
| Your own Bryce 3D 3.0 / 3.1 or Bryce 5.0 install | Needed once, to [make the substrate](substrate.md). Any one of them is enough to start; each adds what only it holds. |
| Hardware | Nothing special. The renderer runs on the CPU and uses every core. |

The renderer has no third-party Rust dependencies.

## 1. Build and install the renderer programs

From the repository root:

```
cargo install --path . --locked
```

This builds and copies three programs into `~/.cargo/bin`, where the add-on looks for them:

| Program | What it does |
|---|---|
| `stecyk3d-render` | Renders. The add-on starts it for every picture. |
| `stecyk3d-extract` | Makes the substrate from your Bryce install. |
| `stecyk3d-cases` | A checker used by the tests. You do not need it to render. |

If you would rather not install, `cargo build --release` puts the same programs in `target/release/`. The add-on can be told where they are (step 3).

## 2. Install the add-on

Build the installable zip with Blender's own tool and install it:

```
blender --command extension build --source-dir blender_addon/stecyk3d --output-dir /tmp
blender --command extension install-file -r user_default -e /tmp/stecyk3d-0.1.0.zip
```

Or in the UI: *Edit > Preferences > Get Extensions* (the small arrow menu at the top right: *Install from Disk...*), choose the zip, and enable **Stecyk3D**.

The add-on adds a render engine called **Stecyk3D** to Blender's engine list, a **Bryce** menu in the Shader Editor's Add menu, Bryce entries in the 3D viewport's Add menu, and *File > Import > Bryce document*.

## 3. Tell the add-on about your Bryce install

Open *Edit > Preferences > Add-ons > Stecyk3D*. You will see:

| Item | What to do |
|---|---|
| **Bryce 3.0 folder**, **Bryce 3.1 folder**, **Bryce 5.0 folder** | Fill in the install folder of each version you own (all optional, any subset). The folder may be the install itself or hold it up to two levels below (for example a `Corel/Bryce 3D` wrapper). A line under each field says at once whether the folder looks right (see [The preferences page](blender.md#the-preferences-page)). A folder that holds another version than its field is refused when you make the substrate, with a message saying where it belongs. |
| **Make substrate** | Press it. The add-on runs the extractor on those folders; an INFO message and a box under the button say, for each, what it recognised and how many files it took, and how long it took. See [Making the substrate](substrate.md). |
| `Substrate`, `Renderer`, `Extractor` lines | Read-only. They show what the add-on found, or "not made yet" / "not found" with an error icon. |
| **Advanced** (a closed subpanel) | Leave empty unless something is not found: **Substrate file**, **Renderer program**, **Extractor program** override the automatic search. |

The renderer and the extractor are looked for in this order: the Advanced override; a `bin` folder beside the add-on; `PATH`; a cargo `target/release` or `target/debug` folder (`CARGO_TARGET_DIR`, or in any folder above the add-on); `~/.cargo/bin`. If one is missing, the add-on says which and where it looked.

## 4. Your first render

1. Press **Make substrate** (step 3). Until a substrate exists, the Render properties tab shows an error box: *"No substrate file yet. Set your Bryce folders in the Stecyk3D add-on preferences and press Make substrate."* with a button to the preferences, and F12 reports the same message in the status bar.
2. In the Render properties set **Render Engine** to **Stecyk3D**.
3. Press **F12**.

The default Blender scene renders: its cube gets a default Bryce material, and a scene without a sun lamp still gets a default sun.

Next: [make the substrate](substrate.md), then [render from Blender](blender.md).

## Uninstalling

Remove the extension in *Preferences > Add-ons*. The substrate is a plain file at the place named on the preferences page; delete it (and the `substrate.files` folder beside it) if you want it gone.
