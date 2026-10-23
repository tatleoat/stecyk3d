# Known differences, limits and troubleshooting

## What "experimental" means here

The renderer follows three Bryce program versions and aims for the same picture, to the last bit, wherever it draws. For most scenes it does. Where a scene uses something it cannot draw faithfully it **refuses** and says what, instead of approximating. This first release has been compared against pictures from the real programs on a large set of test scenes; most match exactly and a minority differ in the ways listed below. Treat a picture as a very close imitation, not a guarantee.

## Known differences from the Bryce programs

- **Animation is not carried into Blender.** Importing a document gives the scene at its opening time. No keyframes are made. Animation ranges of saved documents are rendered with the command line (`--time`, `--frames`).
- **Principled BSDF materials are not read automatically.** Use *Bryce material from the Blender shader* (a one-time conversion of constants, linked inputs are not carried), or build the material from Bryce nodes.
- **Orthographic views** in the Rendered viewport work only with Program 3.0.
- **Single-precision camera.** Blender stores cameras and view matrices as single-precision numbers. An imported camera keeps the document's exact numbers until you move it. The Rendered viewport always uses Blender's view numbers, so scenes with very fine random grain (the Bryce 5 sky wash) can differ slightly in the viewport from F12.
- **Program 3.0 sky edge colours.** Under Program 3.0 a ray that misses everything in a few scene types draws a colour that can be one or two levels off in a small number of pixels.
- **A few Program 3.0 volume scenes** (ranges of light in a medium) differ from the real program in a handful of pixels.
- **Very large wash scenes.** A big Bryce 5 sky wash frame with Random grain Auto or Exact (the setting was called Wash grain) can take many minutes on one core; use **Fast** (or `--independent-random`) for speed, at the price of a different fine grain.
- **Importing:** a document's *region* render setting is not read; trees shared between objects stay shared only in documents the add-on imported itself; a few scene files with older path parts are refused with a message naming them; a rainbow of extreme size, or a sun direction that is not a unit vector, can differ.
- **Object-space textures** on a rotated or scaled mesh object use the mesh origin without the rotation and scale.
- **Cores.** The renderer uses every core; the Cores setting does not change that.
- **A graphics card is not used.**
- **Blender and the renderer must run on the same machine.**
- **Blender versions.** Developed and tested with Blender 5.2; the manifest allows 4.2 and newer.

## Troubleshooting

| Symptom | Likely cause and fix |
|---|---|
| The Render tab shows "No substrate file yet..." | No substrate. Fill in the Bryce folders in the add-on preferences and press **Make substrate**. |
| "the substrate cannot be read" or a missing-table message | The substrate is damaged or old, or lacks something. [Make it again](substrate.md), naming every install you have. |
| Message names a star catalogue or a kept file | A 5.0 render needs data only a Bryce 5 install holds. Make the substrate again with that install. |
| "Substrate not made: ... no Bryce program file found" | The folder is not a Bryce install (or is one level too deep or too shallow). Pick the folder that holds the program and its `Data` folder. |
| "this is Bryce 3D 3.1, not Bryce 3.0..." | The folder is in the wrong field. Put it in the field the message names. |
| "The renderer program (`stecyk3d-render`) was not found" | Install the programs (`cargo install --path .`) or name the renderer under *Advanced* in the preferences. The message lists where the add-on looked. |
| "Not rendered: ..." and a short line about a report | The scene has something the renderer will not approximate. Open the text **Stecyk3D report** in the Text Editor for the full list, one problem per line. |
| "unknown argument" (command line) | An option the program does not know. Check [Command-line rendering](command-line.md). |
| Colours look different from a normal Blender render | The add-on sets the view transform to Raw. Change it back after rendering if you want Blender's colour management. |
| A scene file is refused | The message names the part and why. Parts from an unknown file version are not guessed. |
| Pictures differ between 3.0, 3.1 and 5.0 | Expected. Choose the version on the Render tab. |
| The Rendered viewport keeps an old picture | An error or an unsupported view (for example orthographic with Program 5.0). Look at the viewport's status text. |
| The first live picture is slow to start | The renderer starts once per Blender session; big scenes take a moment to read. The viewport starts coarse and sharpens. |

When you report a bug, include the program's message, the Program setting, the report text if there is one, and if possible the printed SHA-256 line.
