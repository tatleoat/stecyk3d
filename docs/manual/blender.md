# Rendering from Blender

## First render

1. [Make the substrate](substrate.md) once (preferences page, **Make substrate**).
2. In *Render properties* set **Render Engine** to **Stecyk3D**.
3. Press **F12**.

A scene with no sun lamp still renders: the add-on supplies a default sun unless the sky settings give a sun direction. A mesh with an ordinary Blender material renders with a default Bryce material (see [Materials](#materials)).

If the substrate is missing, the Render tab shows an error box that says so and has a button that opens the preferences, and F12 stops with the same message.

## F12, the live picture and the Rendered viewport

- **F12** draws the picture as it is made. The finished picture is always the exact render; what you see on the way is only how it arrives.
- **Progressive** (Render tab, on by default) turns the live picture on or off for F12. Material thumbnails always skip it.
- **Live order** chooses how the picture arrives:
  - **Rows** (default): a sweep from the top to the bottom, all cores working on a few rows at a time.
  - **Scattered**: pixels arrive at fixed random places over the whole frame, so the picture emerges from noise and sharpens. In the Rendered viewport this order also starts from a coarse picture and goes finer.
  - Both give the same final picture. With some anti-aliasing settings, and in scenes where objects met first change what later pixels show, the live picture is a one-ray preview and the exact frame arrives whole at the end.
- **Random grain** (Render tab; called Wash grain in earlier versions, and saved files keep working) matters only for scenes whose shading uses random numbers (soft shadows, the Bryce 5 sky wash, radius lights, jittered volumes). It sets how those random numbers are shared out between pixels for F12. The Rendered viewport is a live preview and always uses the fast way unless you choose EXACT. (Blender shows these choices in capitals, as written here.)
  - **Auto** (default): the renderer keeps the Bryce program's own order of random numbers (exact) while the amount of work is small and switches to the fast way when the work is large. The choice depends on the amount of work, never on the clock, so the same scene gives the same picture on every machine.
  - **Exact**: always the program's order. One core, and slow for big scenes.
  - **Fast**: every pixel draws from a place of its own in the random sequence, on every core. Quick (a soft-shadow scene that takes a minute in the exact order takes a few seconds), but the fine grain of the soft shadows and the wash differs from the Bryce program's.
- A new render stops the previous one. Cancel closes the live picture without merging it.
- In a 3D viewport switch to **Rendered** shading. The viewport re-renders when the scene or the view changes, starts from the previous picture so a change shows as sharpening, and draws at the **Viewport pixel size** and **Viewport quality** set on the Render tab (RENDER = like F12, NORMAL = one ray a pixel, DRAFT = also no pass over edges). It uses the viewport's own camera, not the scene's.
- **Colour.** The renderer's levels are shown unchanged only with the scene's view transform set to **Raw** (look None, dithering 0). Stecyk3D sets these when you import a Bryce document, on the first F12, and the first time the Rendered viewport starts on a scene that is not on them, and says so in the status bar. Blender's **Color Management** panel (Render tab) is shown with this engine, so you can change them; the viewport does not set them again after that.
- Orthographic viewport views work only when the **Program** setting is 3.0; with 3.1 or 5.0 the viewport shows a message and keeps the last picture.
- Blender gives the viewport its view as single-precision numbers, so grain-sensitive scenes (the wash) can differ slightly in the viewport from F12.

**Cores.** The renderer uses every core. The **Cores** setting on the Render tab is kept for compatibility and does not change that; use `--threads` on the [command line](command-line.md) to choose.

## Choosing the program version

The renderer imitates three programs: 3.0, 3.1 and 5.0. The Render tab's **Program** setting chooses which one the picture follows. They differ in arithmetic and in features, so the same scene gives slightly different pictures, and some settings (marked in the panels) exist only for 5.0. Changing the program switches the sky and light defaults that still hold the other program's defaults.

## The Render tab

| Setting | Meaning |
|---|---|
| Program | 3.0, 3.1 or 5.0 (above). |
| Cores | Kept for compatibility (above). |
| Antialias | 0, 1, 2 or 4, as in the Bryce render dialog; for 5.0, 2 shows *Super rays* and 4 *Premium rays*. |
| Bryce 5 box | Program 5.0 only: the switches and settings listed under [The Render tab's Bryce 5 box](#the-render-tabs-bryce-5-box). |
| Depth, Preview | Ray depth (reflections and transmissions); whether the program's quick preview pass runs before the frame (off by default). |
| Progressive, Live order, Random grain | The live picture (above). |
| Viewport quality, Viewport pixel size | The Rendered viewport (above). |
| Units | The scale between Blender units and Bryce units (1 = the same). |
| Exact png | If set to a file name, the renderer's 8-bit levels are also written there as a plain PNG, independent of Blender's colour settings. |
| Presets folder | A folder of `.mat` and `.brt` preset libraries to offer in the material tab. Empty: the libraries of your substrate. |
| Import Bryce document | Button; see [Importing](#importing-bryce-scenes). |

At the top of the tab, once a substrate exists, a box says *Substrate: N tables of Bryce's names* and lists each table of names it holds with its count (these feed the menus of the Noise, Filter and Blend nodes). Without a substrate the box is the red error described under [First render](#first-render).

### The Render tab's Bryce 5 box

With Program 5.0 the tab has a **Bryce 5** box:

- Three switches that Bryce 5 has in its render dialog: **Gamma correction**, **Dithering**, **Total internal reflection**. Hover a switch for what it does. With Total internal reflection on, a **Tir depth** setting appears.
- **Spatial kind** and **Spatial level**.
- **Premium effects**: **Soft shadows**, **Blurry reflections**, **Blurry transmissions**, **Depth of field**, **True ambience**. They act only with Antialias 4 (Premium rays); with another setting the heading reads *Premium effects (need the Premium quality)* and the switches are greyed. With Depth of field on, **Focus**, **Focal distance** and **Lens radius** appear.

These switches are kept with the world's sky settings, so they are shown only when the scene has a world.

## What is read from your scene

| Blender data | What is used |
|---|---|
| Camera | position, direction, lens/field of view, projection, render size. If you import a Bryce scene and do not touch its camera, the camera numbers are kept exactly as the document holds them. |
| Mesh objects | converted to triangle meshes, with transforms, smooth/flat shading and UV coordinates |
| Bryce shapes (3D viewport *Add > Bryce shape*) | sphere, cube, cylinder, cone, torus, pyramid, plane, slab, square, disc, and for program 5.0 metaball and tree: as exact shapes, not meshes |
| Terrains | made with *Add > Bryce terrain* (Terrain, Symmetrical lattice) or *Object > Terrain from Mesh*, or imported |
| Metaballs | balls and negative balls, grouped into sets |
| Trees | from imported documents and the add-on's tree settings |
| Groups | made with *Object > Bryce group from selection*: an empty with member objects as children, each with a role (normal, negative, ...) |
| Lights | sun lamp (the sky's sun), point/spot/area lamps with Bryce light settings, beams, visible lights |
| Materials | Bryce Material nodes (below) |
| Sky | the Bryce Sky node and the world's Bryce sky panel: sun, haze, fog, clouds, stars, moon, rainbow |

Objects hidden for render are skipped. Modifiers are evaluated at their render levels.

## Materials

A mesh whose material has no Bryce Material node renders as a *new Bryce material* (a plain default), **not** from its Principled BSDF. To use your Blender colours:

- *Material properties > Bryce material > Bryce material from the Blender shader* replaces the material's nodes with a Bryce Material built from its Principled BSDF's constants:

  | Principled BSDF | Bryce Material channel | How |
  |---|---|---|
  | Base Color | Diffuse colour | converted to the displayed (sRGB) colour |
  | Roughness | Specularity | 1 - roughness |
  | Metallic | Metallicity | as is |
  | Alpha and Transmission Weight | Transparency | 1 - alpha x (1 - transmission x (1 - metallic)): the light that passes straight through (alpha) plus the light the glass lets through |
  | Transmission Weight | Transparent colour | the base colour for glass, white for light that passes by alpha, mixed by their shares |
  | IOR | Refraction | as is (Bryce's refraction value is the index of refraction; the Materials Lab shows it times 100), only where Transmission Weight lets light through and Thin Wall is off |
  | Specular IOR Level | Reflection | 2 x level - 1, from 0 to 1: Blender's neutral 0.5 and below give no mirror, 1 a full mirror. Below 0.5 the highlight (Specularity) fades to none at 0 |
  | Transmission and Specular IOR Level | Diffusion | 1 x (1 - transmission x (1 - metallic)) x (1 - reflection): Blender shares the light between its layers, Bryce adds them, so a glass or mirror keeps only Blender's diffuse share |
  | Specular Tint | Specular colour | converted to the displayed colour |
  | Emission Color and Strength | Ambient colour and Ambience | colour as above; ambience = strength, at most 1 |
  | everything else | the new material's defaults | |

  Every input left at Blender's own default gives the same material as before these inputs were read. A glass is Transmission Weight 1 with the IOR you want (water 1.33); a mirror is Specular IOR Level 1. Coat, Sheen, Subsurface, Anisotropic, Thin Film and Diffuse Roughness have no Bryce counterpart: when one is set the add-on says it was not carried.

  A linked input (a texture or any node) is not carried: its constant is used, and the add-on lists which inputs were linked.
  The panel then lists, in a box headed *From a Blender material: what is the same thing in both was copied*, what was copied (check marks) and, under *Left for you to set in Bryce's terms:*, what was not. **Dismiss this list** clears the box; the material is unchanged.
- *Make it a new Bryce material* (*New Bryce material* once the node is there) gives the material the Bryce Material node with a new Bryce material's settings.
- *Bryce settings to nodes* moves the settings held in files made by older versions onto Bryce nodes.
- The material tab also offers preset libraries (`.mat`, `.brt` files from the Presets folder or from your substrate) and a *Texture components* panel to add, show, remove, reset and randomise a material's texture components.

### The Material tab, once the material has Bryce nodes

The **Bryce material** panel shows the nodes' own values, so editing there or in the Shader Editor is the same thing.

- **Faults and notes.** Lines at the top say what a render would refuse in this material's nodes (red, with an error icon) or only remark on (an info icon). Fix the red ones before rendering.
- **Channels.** The five colour channels and eight value channels are in two boxes. A channel with a constant shows its value; one fed by a Texture or Picture shows *from* and the node's name. A channel fed by a Mix Colour or Mix Value node shows a **mix row**: the channel's name, a choice of what the mix goes by (Altitude or Alpha), then rows **A** and **B** (each a value, and the name of what feeds it, if anything) and, for Alpha, a row **by**.
- **Two materials mixed.** If the material is a Mix Materials node, the panel shows the **Two materials mixed** slider, then the *First* and *Second* materials one after the other, each with its own channels and settings. A side with no Bryce Material linked is marked in red.
- **Texture components.** A subpanel of the Material tab. The buttons **A** to **D** show one texture of the material; *Add texture from a material* (with *From material* and *Component* choices), *Add picture texture* (with *Picture*) and *Remove texture* work on the letters. Below the buttons the texture is laid out the way Bryce's texture editor does, each part with its own settings: the *Bryce Mapping* (or a line saying it has none), the *Bryce Texture* with the *Phase amount* and the noise of the whole texture's phase, and then each *Component* with the Blend that joins it to the one before, its *Noise* (and the noise of its phase), its *Filter* and its colours. A Picture texture shows its image and mapping instead. After these come *Add a Bryce Mapping*, *Reset component*, *Randomize component* and the texture preset choice.

## The Bryce nodes

In the Shader Editor, with the Stecyk3D engine selected, the **Add** menu has a **Bryce** submenu:

| Group | Nodes |
|---|---|
| Material | **Material** (the channels of a Bryce material), **Mix Materials** |
| Texture | **Texture**, **Picture**, **Mapping** |
| Texture parts | **Noise**, **Filter**, **Component**, **Blend** |
| Mix | **Mix Colour**, **Mix Value** |
| World and light | **Sky** (put on the World Output; the button *Make it a Bryce sky* does it) and **Light** (*Make it a Bryce light*) |

and a **New texture (Noise, Component, Texture)** button that builds a Texture with a Noise and a Component already wired. The Material node's colour and value channels can be fed by Texture or Picture nodes; Mix Materials blends two materials. Wrap parts in Blender node groups if you like: the add-on follows links through groups (nested groups, reroutes, up to 16 deep), and names the node in the message when a link cannot be followed.

Nodes open at the width they have in Bryce's own editor. The Noise, Filter and Blend nodes are titled with the name of what they are set to, for example *Bryce Noise: <name>*, using the names from **your** Bryce install (read from the substrate); a Filter set to none keeps its plain title. Texture and Picture nodes are titled with their letter (*Texture A*).

The Material tab, the World tab (*Bryce sky*, with a closed *Every sky setting* subpanel) and the Data tab of a lamp (*Bryce light*) show the same values as the nodes, so you can edit either way. The sections below describe each place.

## The Data tab: Bryce light and Bryce camera

**Bryce light** (a lamp is selected, the Stecyk3D engine is on). A Sun lamp only points to *World properties > Bryce sky*, where the sun lives. For the other lamps, with no Bryce Light node the panel says *Renders as a new Bryce light* and offers *Make it a Bryce light*; with the node, it shows the node's values.

- **Light class.** The first line names what the Blender lamp becomes: *Radial light* (Point), *Spotlight* (Spot), *Square spotlight* (Spot with **Square beam**), *Parallel light* (Area, disc or ellipse) or *Square parallel light* (Area, square or rectangle). **Beam angle** and **Square beam** are the Spot lamp's; an Area lamp shows **Beam** (its shape), **Width** and, for a rectangle or ellipse, **Depth**.
- **Colour, Intensity**, and, for Program 5.0, **Range**, **Radius** and **Shadow ambience** (with **Shadow color** once ambience is not zero).
- **Gradient** (5.0 only). A box with **Gradient range** and **Gradient offset** and two lists of keys, *Colours* and *Strengths*, each key a row with its value, **Place** (0 at the light, 1 at the range) and **Mid** (midpoint), a remove button (an X) and **Add a key**. While both lists are empty the light keeps its own colour and strength, and **Start with the Light Lab's gradient** fills both with Bryce 5's starting keys. One key alone says nothing, and the panel says so: a gradient needs two. Four switches, *Colours: Repeat*, *Colours: Repeat after the offset*, *Strengths: Repeat* and *Strengths: Repeat after the offset*, make a distance past the last key come round to the first instead of staying at the nearest key.
- **Material, Tint**, then **Edge falloff** and **Reach**. A radial light has no edge and no reach, so these are hidden for it; a square spotlight or square parallel light has two edge falloffs (*across* and *along*).
- **Switches** in a grid: **On**, **No shadows**, **Visible object** and, for 5.0, **Gradient** (the light follows the gradient's keys), and the **Flags** number they set.

**Bryce camera** (a camera is selected). It tells how the Blender camera is read: a *Perspective* camera gives Bryce's field of view from **lens** and sensor, with **Pan, across** and **Pan, up** (the lens shift); an *Orthographic* camera is Bryce's parallel projection, with **Frame width** and the same pans; a *Panoramic* camera is Bryce's panorama (the lens does not count). Any other camera type has no counterpart and the render refuses it.

## The World tab: Bryce sky

With no Bryce Sky node the panel says *Renders as a new Bryce document's sky* and offers *Make it a Bryce sky*. The sections follow Bryce's sky dialog: **Sun** (a Sun object, or a direction when the scene has no sun lamp), **Sky**, **Clouds**, **Scene light**, and for Program 5.0 **The sun in Bryce 5** and **A sky of Bryce 5**.

- **Switch grids.** The sky's flag word is shown as grids of switches, one per named bit: *Atmosphere*, *Glow look 1*, *Glow look 2*, *Custom colours*, *Clouds*, *Stratus*, *Cumulus*, *Haze*, *Sky dome light*, *Haze takes the sun's colour*, *Fog takes the sun's colour*, *Clouds follow the view*, *Fixed cloud coordinates*. Ticking **A sky of Bryce 5** adds *Clouds on a dome* and *Haze shift*, and another grid with *Stars*, *Comets* and *Haze as an older document's*. Hover a switch for what it does.
- **New document's sky.** A button under the Sky settings. It gives the sky the settings a new document of the chosen Program starts with (colours, clouds, haze, fog, moon, stars and the switches). The sun, the scene light and the cloud textures stay as they are.
- **Cloud-layer textures.** When the sky is a node, the *Stratus texture* and *Cumulus texture* each have a box at the bottom of the panel. The box names the Bryce texture linked to that layer and draws its settings in place, or says *none* and, under it, *Link a Bryce texture to the Bryce Sky node's stratus input (Shader Editor, World)* (or *cumulus input*). See [Giving a cloud layer a texture](#giving-a-cloud-layer-a-texture).
- **Cloud-layer faults.** Above the settings, red lines name a layer that is switched on but has no texture, for example *the sky's Stratus layer is switched on and has no texture: link a Bryce texture to the Bryce Sky node's stratus input (Shader Editor, World), or switch Stratus off*. A red line also names a layer fed by something that is not a Bryce texture. A texture a render would refuse says so in its own box.

### Changing the sky

1. In *Render properties* set **Render Engine** to **Stecyk3D**.
2. Open the *World properties* tab. If the scene has no world, press **New** at the top of the tab.
3. Find the **Bryce sky** panel. If it says *Renders as a new Bryce document's sky*, press **Make it a Bryce sky**. This puts a Bryce Sky node on the world's World Output, in place of the Background node, and keeps the world's Bryce sky settings. The panel then says *The sky is its node (Shader Editor, World); this shows it*.
4. Change the settings in the panel, from the top:
   - **Sun**: the *Sun object* (or *Sun direction* when the scene has no sun lamp), *Sun color*, *Sun glow color* and *Sun intensity*.
   - **Sky**: the switch grid (*Atmosphere*, *Clouds*, *Stratus*, *Cumulus*, *Haze* and the rest), then *Zenith*, *Horizon*, *Plain*, *Ambient*, the haze and fog settings and the *Moon picture*. The **New document's sky** button under them gives the sky a new document's settings again.
   - **Clouds**: *Cover*, *Cover amplitude*, *Cloud height*, *Cloud frequency*, *Cloud offset*, *Detail*, *Cumulus color* and *Cumulus shade*.
   - **Scene light**: *Ambient color*, *Ambient amount* and *Shadow lift*.
   - With Program 5.0, **The sun in Bryce 5** and the **A sky of Bryce 5** switch follow.
5. The closed subpanel **Every sky setting** has every setting of the sky, the ones above included.

### Giving a cloud layer a texture

*Stratus* is the upper cloud layer and *Cumulus* the lower. Each takes its pattern from a Bryce texture linked to the Bryce Sky node. You link it in the Shader Editor.

1. Make the world a Bryce sky first (*Make it a Bryce sky*, above).
2. Open a Shader Editor. The **Shading** workspace has one, or set any area's editor type to **Shader Editor**.
3. In the Shader Editor's header, change the *Shader Type* menu from **Object** to **World**. The world's nodes appear: the **Bryce Sky** node linked to the **World Output**.
4. Choose **Add > Bryce > New texture (Noise, Component, Texture)** (the last entry of the Bryce menu). It adds a Bryce Texture node with a Noise and a Component already wired to it. **Add > Bryce > Texture** adds a Texture node alone instead: it needs a Component linked to its *component* input, and the node (and, once linked, its box on the World tab) says so in red until it has one. (The Bryce menu is there only with the Stecyk3D engine.)
5. Drag from the Texture node's **color** output to the Bryce Sky node's **stratus** input (the upper layer) or its **cumulus** input (the lower layer). The nodes spell the sockets this way.
6. In the switch grid under **Sky** on the World tab, make sure *Atmosphere*, *Clouds* and *Stratus* (or *Cumulus*) are ticked. A new sky of Program 3.0 or 3.1 has only *Atmosphere* of these on; one of Program 5.0 also has *Clouds* and *Stratus* on.
7. The *Stratus texture* (or *Cumulus texture*) box at the bottom of the World tab now names the texture and shows its settings: *Gives*, *Colour space*, and the *Filter* and *Colour map* boxes. The red line about the layer goes. Change the texture's noise and component on their nodes in the Shader Editor.

To have no clouds on a layer instead, untick its switch (*Stratus* or *Cumulus*) in the switch grid under **Sky**. The red line goes too.

## The Object tab: Bryce shape

Shown for mesh and empty objects.

- **Role** (in a group: *Role in its group*) of the object in a Bryce group, and **Cut shows positive** for a negative one; **Shape**, the Bryce shape the object stands for.
- A mesh has the **Terrain from Mesh** button (also in the *Object* menu). For a terrain, its settings are in a box and **Show the terrain** rebuilds the viewport mesh from the terrain's picture (the picture must be square). With the grid set to *Auto*, a picture whose side is not 16, 32, 64, 128, 256, 512 or 1024 points is resampled when you render to the next of those sizes up (at most 1024), over the same area, so the terrain keeps its shape and size; the render report says so and from what size.
- A **torus** has **Torus radii (tube, ring)**; *Add > Bryce shape > Torus* sets them (a torus made another way says *none set*).
- A **metaball** has **Negative**. A metaball or tree under a Program other than 5.0 renders as its stand-in mesh, and the panel says so.
- A **tree** has the *Tree Lab* boxes, with the tree's seed: **Trunk and branches**, **Shape (distance from the trunk, height)** and **Foliage (material slot 2: the leaves')**. The tree is 2 high, its foot 1 below the object's origin.
- **Show the shape** replaces the object's mesh with one that shows its Bryce shape. Press it after changing the shape or a torus's radii.

## The preferences page

*Edit > Preferences > Add-ons > Stecyk3D* (the first part is described in [Installing](installing.md)).

- **A mark under each Bryce folder** says whether the folder looks right: *looks like a Bryce 3D 3.0 install*, *no Bryce program file here*, *looks like Bryce 5.0, not 3.0* and so on. For 3.0 and 3.1 the program file is the same kind; the extractor tells those two apart when it runs. An empty field reads *(optional: leave empty if you have none)*.
- **After Make substrate**, a box *Last Make substrate: done in N s* (or *failed*, with the reasons) lists, per folder, what was recognised and how many files were kept, and then, for each Bryce program file, which tables were found in it. It stays until the next press or until Blender closes.
- The *Substrate*, *Renderer* and *Extractor* lines show what was found, as before.

## Refusals and the report

When a scene holds something the renderer cannot draw faithfully, it **refuses** rather than approximating. F12 stops, the status bar gets one short line, for example

> Not rendered: the first problem, cut to fit (3 problems); see Text Editor > Stecyk3D report

and every problem is written one per line to the Info editor and to a text block named **Stecyk3D report** (open it in the Text Editor; it is replaced on each refused render). Fix the listed items (or change the Program setting) and render again.

## Colour

The renderer's colours reach Blender unchanged. Before writing the result the add-on sets the scene's view transform to *Raw*, look to *None* and dither to 0, and says so with one INFO message. Set them back afterwards if you want Blender's colour management on the picture.

## Importing Bryce scenes

*File > Import > Bryce document (.br3, .br5)* (or the button on the Render tab) loads a saved scene. The dialog has three choices, all on by default:

- **Sky, sun and light**: the world and its Bryce sky, and the sun as one lamp.
- **Materials and textures**: the materials, with their textures and mixes, as Bryce nodes.
- **Objects, lights and camera**: the objects in the document's order, groups as empties with their members as children, and the camera in use as the scene's camera.

How it behaves:

- The document is **read as the Bryce program opens it**: the animation tracks are applied at the opening time and the result becomes your Blender scene. **No Blender keyframes are made.** If you want the scene at another time, use the command line's `--time` or `--frames` (see [Command-line rendering](command-line.md)).
- A `.br5` whole import sets the Program to 5.0.
- The whole document is read before anything is made; if it cannot be read nothing is changed. A second whole import into the same scene is refused (import into a new scene).
- Objects the importer had to simplify are reported as warnings with the object's name.
- An import is exact for most documents: the imported scene renders the same picture as the document does on the command line. Differences are listed under [Known differences and limits](troubleshooting.md).

## Animation

Blender renders animation frame by frame, and the add-on reads the scene as Blender evaluates it at each frame. Importing a Bryce document does not create keyframes (see above). Rendering Bryce animation ranges from a saved document is a command-line job (`--frames`).
