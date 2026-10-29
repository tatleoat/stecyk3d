# SPDX-License-Identifier: GPL-3.0-or-later
# Variants
#       (5.0 settings shown only as 5.0)
# (Panels; needs Blender, not testable here)
"""Panels drawing the registered settings in their table order."""

import array
import os
import textwrap

import bpy

from . import create, defaults, gather, group_links, names

TEXT_SETTINGS = {"exact", "mix", "converted", "grown"}


# Settings whose label is not their identifier in words: `wash_grain` (the identifier old files carry) is shown as
# "Random grain" since it covers every shading that draws from the generator (soft shadows, radius lights, wash).
LABELS = {"wash_grain": "Random grain"}


def human(key):
    """A setting's name as a label: the identifier (which stays the earlier add-on's) in words."""
    if key in LABELS:
        return LABELS[key]
    return key.replace("sky5_", "").replace("_", " ").capitalize()


def is_five(context):
    return getattr(context.scene, names.SCENE_RENDER_GROUP).program == "5.0"


def light_group(light):
    """: the Bryce Light node's `settings`, else the lamp's `bryce` group (what the Light panel draws)."""
    node = gather.find_node(light.node_tree if light.use_nodes else None, "BryceLightNode")
    return node.settings if node is not None else getattr(light, names.DATA_GROUP)


def draw_list(layout, group, key, five):
    """(Light gradient keys): a list setting of a light ('s colour keys
    and value keys), each key's settings in a box with a remove button, and an add button."""
    items = getattr(group, key)
    box = layout.box()
    box.label(text="%s: %d" % (key, len(items)))
    for index, item in enumerate(items):
        row = box.box()
        draw_group(row, item, five)
        op = row.operator("stecyk3d.list_edit", text="Remove", icon="REMOVE")
        op.name, op.action, op.index = key, "REMOVE", index
    op = box.operator("stecyk3d.list_edit", text="Add", icon="ADD")
    op.name, op.action, op.index = key, "ADD", len(items)


def draw_gradient_keys(box, b):
    """The Gradient box's two key lists as the earlier add-on lays them out: a starter while both are empty, then
    Colours and Strengths, each key a row (value, place, midpoint, a remove icon), an add button, the one-key note."""
    if not len(b.color_keys) and not len(b.value_keys):
        box.label(text="No keys: the light keeps its own colour and strength", icon="INFO")
        box.operator("stecyk3d.light_lab", text="Start with the Light Lab's gradient", icon="ADD")
    for key, title, what in (("color_keys", "Colours", "color"), ("value_keys", "Strengths", "value")):
        items = getattr(b, key)
        part = box.column(align=True)
        part.use_property_split = False
        part.label(text=title + " (place 0 at the light, 1 at the range)")
        for index, item in enumerate(items):
            row = part.row(align=True)
            row.prop(item, what, text="")
            row.prop(item, "position", text="Place")
            row.prop(item, "midpoint", text="Mid")
            op = row.operator("stecyk3d.list_edit", text="", icon="X")
            op.name, op.action, op.index = key, "REMOVE", index
        op = part.operator("stecyk3d.list_edit", text="Add a key", icon="ADD")
        op.name, op.action, op.index = key, "ADD", len(items)
        if len(items) == 1:
            part.label(text="One key says nothing: a gradient needs two", icon="INFO")


def draw_group(layout, group, five, five_only=(), editable_lists=()):
    properties = group.bl_rna.properties
    for prop in properties:
        key = prop.identifier
        if key in ("rna_type", "name") or key in TEXT_SETTINGS or key.startswith(names.SWITCH_PREFIXES):
            continue  # (a named bit of a flag word is drawn by the sky panel, in its own place)
        if key + "_menu" in properties:
            continue  # the menu of the same number is drawn in its place (node_menus.py)
        if key in five_only and not five:
            continue
        if prop.type == "POINTER" and isinstance(getattr(group, key), bpy.types.PropertyGroup):
            box = layout.box()
            box.label(text=key)
            draw_group(box, getattr(group, key), five)
        elif prop.type == "COLLECTION" and key in editable_lists:
            draw_list(layout, group, key, five)
        elif prop.type == "COLLECTION":
            layout.label(text="%s: %d" % (key, len(getattr(group, key))))
        else:
            layout.prop(group, key, text=human(key))


LIGHT_LISTS = ("color_keys", "value_keys")


class STECYK3D_OT_list_edit(bpy.types.Operator):
    """Add a key to, or remove one from, a list setting of the active light ('s gradient keys)"""
    bl_idname = "stecyk3d.list_edit"
    bl_label = "Edit key list"
    bl_options = {"UNDO", "INTERNAL"}
    name: bpy.props.StringProperty()
    action: bpy.props.EnumProperty(items=[("ADD", "Add", ""), ("REMOVE", "Remove", "")])
    index: bpy.props.IntProperty()

    @classmethod
    def poll(cls, context):
        return context.light is not None

    def execute(self, context):
        if self.name not in LIGHT_LISTS:
            return {"CANCELLED"}
        items = getattr(light_group(context.light), self.name)
        if self.action == "ADD":
            items.add()
        elif 0 <= self.index < len(items):
            items.remove(self.index)
        return {"FINISHED"}


LIGHT_FLAG_SWITCHES = [(0x100, "On", "The light shines"),
                       (0x800, "No shadows", "Nothing stands in this light's way"),
                       (0x200, "Visible object", "Rays meet the light's own object, which then shows its material"),
                       (0x200000, "Gradient", "Bryce 5: the light's colour and strength follow the gradient's keys along the distance from the light")]
GRADIENT_FLAG = 0x200000
# The bits of a gradient key list's flag word the renderer reads (lights.rs: GRADIENT_WRAP 0x2, GRADIENT_OFFSET_WRAP 0x1).
GRADIENT_WRAPS = [(2, "Repeat", "A distance past the last key, or short of the first, comes round to the other end, so the gradient repeats. Off: it stays at the nearest key"),
                  (1, "Repeat after the offset", "The same for where the offset carries the distance past the keys")]
# The words of the light's settings that have switches, and each word's switches.
LIGHT_SWITCH_WORDS = {"flags": LIGHT_FLAG_SWITCHES, "gradient_color_flags": GRADIENT_WRAPS, "gradient_value_flags": GRADIENT_WRAPS}
# The two lists the Light Lab starts a gradient with: colour keys (red, green, blue, place, midpoint) and value keys
# (value, place, midpoint).
LAB_GRADIENT = {"colors": [[1.0, 1.0, 1.0, 0.0, 0.5], [0.0, 0.0, 0.0, 1.0, 0.5]], "values": [[1.0, 0.0, 0.5], [1.0, 1.0, 0.5]]}


class STECYK3D_OT_light_flag(bpy.types.Operator):
    """Switch one bit of one of the active light's flag words (`flags` unless `word` names a gradient list's)"""
    bl_idname = "stecyk3d.light_flag"
    bl_label = "Light flag"
    bl_options = {"UNDO", "INTERNAL"}
    bit: bpy.props.IntProperty()
    word: bpy.props.StringProperty(default="flags")

    @classmethod
    def description(cls, context, properties):
        for bit, _label, text in LIGHT_SWITCH_WORDS.get(properties.word, ()):
            if bit == properties.bit:
                return text
        return cls.__doc__

    @classmethod
    def poll(cls, context):
        return context.light is not None

    def execute(self, context):
        if self.bit not in [bit for bit, _label, _text in LIGHT_SWITCH_WORDS.get(self.word, ())]:
            return {"CANCELLED"}
        group = light_group(context.light)
        setattr(group, self.word, getattr(group, self.word) ^ self.bit)
        return {"FINISHED"}


class STECYK3D_OT_light_lab(bpy.types.Operator):
    """Replace the active light's gradient keys with the two lists Bryce 5's Light Lab starts with"""
    bl_idname = "stecyk3d.light_lab"
    bl_label = "Start with the Light Lab's gradient"
    bl_options = {"UNDO", "INTERNAL"}

    @classmethod
    def poll(cls, context):
        return context.light is not None

    def execute(self, context):
        group = light_group(context.light)
        group.color_keys.clear()
        group.value_keys.clear()
        for red, green, blue, place, middle in LAB_GRADIENT["colors"]:
            key = group.color_keys.add()
            key.color, key.position, key.midpoint = (red, green, blue), place, middle
        for value, place, middle in LAB_GRADIENT["values"]:
            key = group.value_keys.add()
            key.value, key.position, key.midpoint = value, place, middle
        return {"FINISHED"}


class STECYK3D_OT_sky_new(bpy.types.Operator):
    """Give the sky the settings a new document of the chosen program starts with (colours, clouds, haze, fog, moon,
    stars, the flag words); the sun, the scene light and the cloud textures stay as they are"""
    bl_idname = "stecyk3d.sky_new"
    bl_label = "New document's sky"
    bl_options = {"REGISTER", "UNDO"}

    @classmethod
    def poll(cls, context):
        return context.world is not None

    def execute(self, context):
        sky = gather.sky_group(context.world)
        program = is_five(context) and "5.0" or "3.0"
        for key in SKY_RESET:
            value = defaults.program_default("sky", key, program)
            if value is None or key not in sky.bl_rna.properties:
                continue
            sky.property_unset(key)  # (what the setting holds when nothing was set; kept unset if it is the new value)
            current = getattr(sky, key)
            if hasattr(current, "__len__") and not isinstance(current, str):
                current = tuple(current)
            if current != value:
                setattr(sky, key, value)
        return {"FINISHED"}


class STECYK3D_OT_preferences_show(bpy.types.Operator):
    """Open the add-on's preferences (the Bryce folders and Make substrate)"""
    bl_idname = "stecyk3d.preferences_show"
    bl_label = "Your Bryce and Make substrate (the add-on's preferences)"
    bl_options = {"INTERNAL"}

    def execute(self, context):
        bpy.ops.preferences.addon_show(module=__package__)
        return {"FINISHED"}


class _BrycePanel:
    bl_space_type = "PROPERTIES"
    bl_region_type = "WINDOW"
    COMPAT_ENGINES = {names.ENGINE_ID}

    @classmethod
    def poll(cls, context):
        return context.engine == names.ENGINE_ID


def prop_list(layout, group, keys, five):
    """The listed settings, in that order, labelled in words; 5.0-only ones only when the scene renders as 5.0."""
    for key in keys:
        if key in group.bl_rna.properties and (five or key not in FIVE_SETTINGS):
            layout.prop(group, key, text=human(key))


FIVE_SETTINGS = names.FIVE_ONLY["render"] | names.FIVE_ONLY["sky"] | names.FIVE_ONLY["light"]

# The Render tab, in the order of the earlier add-on's panel: program, cores, quality; then the rest.
RENDER_TOP = ("program", "cores", "antialias")
# The settings the earlier add-on had for its helper programs and data folders (keep_description, renderer, reader,
# terrain_tool, data_folder, presets_tool) still exist as properties so old files load, but nothing reads them, so the
# panel does not offer them.
RENDER_REST = ("depth", "preview", "progressive", "live_order", "wash_grain", "viewport_quality", "viewport_pixel_size", "units", "exact_png",
               "presets_folder")


def draw_output_place(layout, context):
    """On Windows, where Blender puts pictures whose output path has no drive letter and no `//`: on the root of the
    Windows drive, not beside the blend file or in the working folder (Cycles and Eevee do the same)."""
    import sys
    from . import outputs
    path = context.scene.render.filepath
    if not sys.platform.startswith("win") or not path or path.startswith("//") \
            or outputs.blender_place(path, "", True, "") == path:
        return  # not Windows, or a path with a drive, `//` or a network path: it goes where it says
    try:
        folder = os.path.dirname(context.scene.render.frame_path(frame=context.scene.frame_current))
    except Exception:
        return
    wrapped_label(layout, "Output path has no drive and no //: Blender saves in %s. Start it with // for the "
                          "blend file's folder." % folder, "INFO")


def draw_substrate_state(layout, context):
    """Where a render would be refused for want of the substrate: the reason, and a button to the preferences."""
    from . import engine
    path, message = engine.find_substrate(engine.preferences(context))
    if path:
        draw_tables_held(layout, context)
        return
    box = layout.box().column()
    box.label(text=message or "No substrate file yet", icon="ERROR")
    box.operator(STECYK3D_OT_preferences_show.bl_idname, icon="PREFERENCES")


# Bryce 5's own switches in the scene's flag word (`scene_flags` of the sky settings): (bit, label, what it does).
# The first three are Bryce 5's; the last five act on the rays of the Premium quality only.
FIVE_SWITCHES = (
    (0x80000, "Gamma correction", "Bryce 5: every pixel's colour is raised to the power 0.625 before it is stored"),
    (0x40000, "Dithering", "Bryce 5: colours are rounded to 8 bits through a pattern of thresholds"),
    (0x400000, "Total internal reflection", "Bryce 5: a ray that cannot leave a transparent object is mirrored back inside it"),
    (0x40, "Soft shadows", "Bryce 5, Premium quality: shadow rays go to a point of the light's disc"),
    (0x800000, "Blurry reflections", "Bryce 5, Premium quality: mirrored rays are spread inside a cone"),
    (0x1000000, "Blurry transmissions", "Bryce 5, Premium quality: rays through transparent surfaces are spread inside a cone"),
    (0x2000000, "Depth of field", "Bryce 5, Premium quality: the camera's rays leave from a lens and meet at the focal distance"),
    (0x4000000, "True ambience", "Bryce 5, Premium quality: a surface adds what one ray brings back from the scene above it"))
BASIC_SWITCHES, PREMIUM_SWITCHES = FIVE_SWITCHES[:3], FIVE_SWITCHES[3:]
GAMMA_BIT, DITHER_BIT, TIR_BIT, DEPTH_OF_FIELD_BIT = 0x80000, 0x40000, 0x400000, 0x2000000


def scene_flag_group(context):
    """The sky settings that hold the scene's flag word (the Bryce Sky node's, else the world's); None with no world."""
    world = context.scene.world
    return gather.sky_group(world) if world is not None else None


class STECYK3D_OT_scene_flag(bpy.types.Operator):
    """Switch one of Bryce 5's bits of the scene's flag word"""
    bl_idname = "stecyk3d.scene_flag"
    bl_label = "Bryce 5 switch"
    bl_options = {"INTERNAL", "UNDO"}
    bit: bpy.props.IntProperty(options={"SKIP_SAVE"})

    @classmethod
    def description(cls, context, properties):
        return next((d for b, _l, d in FIVE_SWITCHES if b == properties.bit), cls.__doc__)

    def execute(self, context):
        group = scene_flag_group(context)
        if group is None:
            return {"CANCELLED"}
        word = int(group.scene_flags) & 0xFFFFFFFF
        word ^= self.bit
        group.scene_flags = word - (1 << 32) if word >= 1 << 31 else word
        return {"FINISHED"}


def draw_switches(layout, group, switches, active=True):
    """A two-column grid of the listed switches, each a button that is pressed while its bit is set."""
    grid = layout.grid_flow(columns=2, align=True)
    grid.active = active
    word = int(group.scene_flags) & 0xFFFFFFFF
    for bit, label, _description in switches:
        grid.operator(STECYK3D_OT_scene_flag.bl_idname, text=label, depress=bool(word & bit)).bit = bit


def draw_tables_held(layout, context):
    """With a substrate, the tables of names it holds, each by its name and how many names it has."""
    from . import node_menus
    tables = node_menus.current_tables()
    box = layout.box().column()
    if not tables:
        box.label(text="Substrate found; it holds no tables of Bryce's names", icon="INFO")
        return
    box.label(text="Substrate: %d tables of Bryce's names" % len(tables), icon="CHECKMARK")
    for name, labels in tables.items():
        box.label(text="%s (%d)" % (name, len(labels)))


class STECYK3D_PT_render(_BrycePanel, bpy.types.Panel):
    bl_label = "Stecyk3D"
    bl_context = "render"

    def draw(self, context):
        group = getattr(context.scene, names.SCENE_RENDER_GROUP)
        five = is_five(context)
        col = self.layout.column()
        draw_substrate_state(col, context)
        col.use_property_split = True
        prop_list(col, group, RENDER_TOP, five)
        if five:
            antialias = group.antialias
            if antialias == "2":
                col.prop(group, "super_rays", text="Super rays")
            elif antialias == "4":
                col.prop(group, "premium_rays", text="Premium rays")
            box = col.box().column()
            box.label(text="Bryce 5")
            sky = scene_flag_group(context)
            if sky is not None:
                draw_switches(box, sky, BASIC_SWITCHES)
                if int(sky.scene_flags) & TIR_BIT:
                    prop_list(box, group, ("tir_depth",), five)
            prop_list(box, group, ("spatial_kind", "spatial_level"), five)
            if sky is not None:
                box.label(text="Premium effects" if antialias == "4" else "Premium effects (need the Premium quality)")
                draw_switches(box, sky, PREMIUM_SWITCHES, active=antialias == "4")
                if int(sky.scene_flags) & DEPTH_OF_FIELD_BIT:
                    prop_list(box, group, ("focus", "focal_distance", "lens_radius"), five)
        prop_list(col, group, RENDER_REST, five)
        draw_output_place(col, context)
        col.operator("stecyk3d.import_document", icon="IMPORT")


# The Finishing subpanel (finishing.py): each switch, then its own settings, shown only while it is on.
FINISHING = (("glow", "Glow", (("glow_size", "Size"), ("glow_strength", "Strength"), ("glow_tint", "Tint"))),
             ("flare", "Lens flare", (("flare_strength", "Strength"),)),
             ("jpeg", "JPEG", (("jpeg_quality", "Quality"),)),
             ("palette", "Palette (GIF)", (("palette_colours", "Colours"),)))


class STECYK3D_PT_render_finishing(_BrycePanel, bpy.types.Panel):
    bl_label = "Finishing"
    bl_context = "render"
    bl_parent_id = "STECYK3D_PT_render"
    bl_options = {"DEFAULT_CLOSED"}

    def draw(self, context):
        group = getattr(context.scene, names.SCENE_RENDER_GROUP)
        col = self.layout.column()
        wrapped_label(col, "Not Bryce's: done to the finished picture afterwards (F12 and the saved file; not the "
                           "viewport, not exact_png), like a 90s paint program. All off gives Bryce's picture.", "INFO")
        col.use_property_split = True
        col.prop(group, "finish_threshold", text="Bright above")
        for switch, label, rows in FINISHING:
            box = col.box().column()
            box.prop(group, switch, text=label)
            if getattr(group, switch):
                for key, text in rows:
                    box.prop(group, key, text=text)


# The settings of the sky record itself (a new document's values): not the sun, the scene light or the textures.
SKY_RESET = ("flags", "cloud_height", "cumulus_color", "stratus_color", "cloud_offset", "cover", "cover_amplitude",
             "cloud_frequency", "detail", "detail_z", "zenith", "horizon", "ambient", "plain", "cumulus_shade",
             "haze_color", "haze", "fog_color", "fog_height", "fog", "moon", "discs", "stars", "sky5_flags2",
             "sky5_haze_offset", "sky5_haze_thickness", "sky5_fog_offset", "sky5_haze_shift", "sky5_cloud_detail",
             "sky5_sun_glow_size", "sky5_moon_glow_size", "sky5_sun_disc", "sky5_moon_disc", "sky5_air_steps",
             "sky5_star_intensity")


def sky_faults(world, program):
    """What Bryce cannot do with the sky's cloud layers, as sentences: a layer that is switched on with no texture,
    a layer fed by something that is not a Bryce texture. (A texture the render would refuse says so in its box.)"""
    node = gather.find_node(world.node_tree if world.use_nodes else None, "BryceSkyNode")
    if node is None:
        return []
    found = []
    flags = node.settings.flags & 0xFFFFFFFF
    for bit, layer in ((0x20, "stratus"), (0x40, "cumulus")):
        try:
            source = gather.linked_from(node.inputs[layer])
        except group_links.GroupLinkError as error:
            found.append("the sky's %s layer: %s" % (layer.capitalize(), error))
            continue
        if source is None:
            if flags & 1 and (program == "5.0" or flags & 0x10) and flags & bit:
                found.append("the sky's %s layer is switched on and has no texture: link a Bryce texture to the Bryce "
                             "Sky node's %s input (Shader Editor, World), or switch %s off"
                             % (layer.capitalize(), layer, layer.capitalize()))
        elif source.bl_idname not in gather.TEXTURE_NODES:
            found.append("the sky's %s layer is fed by %s, not a Bryce texture" % (layer.capitalize(), source.bl_idname))
    return found


LABEL_WRAP = 50  # characters a line of the Properties editor shows at its usual width (a longer label is cut)


def wrapped_label(layout, text, icon):
    """A long sentence as several labels, the icon on the first and a blank one before the rest."""
    for i, line in enumerate(textwrap.wrap(text, LABEL_WRAP) or [text]):
        layout.label(text=line, icon=icon if i == 0 else "BLANK1")


def draw_cloud_textures(layout, context, world):
    """The two cloud layers' textures in boxes, each drawn in place (the texture node's own settings and faults)."""
    node = gather.find_node(world.node_tree if world.use_nodes else None, "BryceSkyNode")
    for layer in ("stratus", "cumulus"):
        try:
            source = gather.linked_from(node.inputs[layer])
        except group_links.GroupLinkError:
            source = None
        box = layout.box()
        title = "%s texture" % layer.capitalize()
        if source is None:
            box.label(text=title + ": none", icon="TEXTURE")
            wrapped_label(box, "Link a Bryce texture to the Bryce Sky node's %s input (Shader Editor, World)" % layer,
                          "BLANK1")
        else:
            box.label(text="%s: %s" % (title, source.label or source.name), icon="TEXTURE")
            if source.bl_idname in gather.TEXTURE_NODES:
                group_links.real(source).draw_buttons(context, box)


class STECYK3D_PT_world(_BrycePanel, bpy.types.Panel):
    bl_label = "Bryce sky"
    bl_context = "world"

    @classmethod
    def poll(cls, context):
        return super().poll(context) and context.world is not None

    def draw(self, context):
        col = self.layout.column()
        col.use_property_split = True
        if gather.find_node(context.world.node_tree if context.world.use_nodes else None, "BryceSkyNode") is None:
            col.label(text="Renders as a new Bryce document's sky")
            col.operator("stecyk3d.make_sky", icon="WORLD")
        else:
            col.label(text="The sky is its node (Shader Editor, World); this shows it", icon="NODETREE")
            program = getattr(context.scene, names.SCENE_RENDER_GROUP).program
            for fault in sky_faults(context.world, program):
                wrapped_label(col, fault, "ERROR")
        draw_sky(col, gather.sky_group(context.world), is_five(context), context)
        if gather.find_node(context.world.node_tree if context.world.use_nodes else None, "BryceSkyNode") is not None:
            draw_cloud_textures(col, context, context.world)


def draw_sky_switches(layout, sky, prefix, rows):
    """The named bits of a flag word as a two-column grid of switches."""
    grid = layout.grid_flow(columns=2, align=True)
    for bit, _label, _description in rows:
        grid.prop(sky, "%s%x" % (prefix, bit))


def draw_sky(col, sky, five, context):
    """A sky's settings in the earlier add-on's sections: Sun, Sky, Clouds, Scene light, then Bryce 5's."""
    col.label(text="Sun (the sky, the moon and the scene's light follow it)")
    col.prop(sky, "sun_object", text="Sun object")
    if sky.sun_object is None and not any(o.type == "LIGHT" and o.data.type == "SUN" for o in context.scene.objects):
        col.prop(sky, "sun_direction", text="Sun direction")
    prop_list(col, sky, ("sun_color", "sun_glow_color", "sun_intensity"), five)
    col.separator()
    col.label(text="Sky")
    draw_sky_switches(col, sky, "flag_", names.SKY_SWITCHES)
    prop_list(col, sky, ("zenith", "horizon", "plain", "ambient", "haze", "haze_color", "fog", "fog_height",
                         "fog_color", "moon_picture"), five)
    col.operator("stecyk3d.sky_new", icon="LOOP_BACK")
    col.separator()
    col.label(text="Clouds")
    prop_list(col, sky, ("cover", "cover_amplitude", "cloud_height", "cloud_frequency", "cloud_offset", "detail",
                         "cumulus_color", "cumulus_shade"), five)
    col.separator()
    col.label(text="Scene light")
    prop_list(col, sky, ("ambient_color", "ambient_amount", "shadow_lift"), five)
    if five:
        col.separator()
        col.label(text="The sun in Bryce 5")
        prop_list(col, sky, ("sun_radius", "sun_shadow_ambience"), five)
        if sky.sun_shadow_ambience != 0.0:
            col.prop(sky, "sun_shadow_color", text="Sun shadow color")
        col.separator()
        col.prop(sky, "sky5", text="A sky of Bryce 5")
        if sky.sky5:
            draw_sky_switches(col, sky, "sky5_flag_", names.SKY5_SWITCHES)
            prop_list(col, sky, ("sky5_haze_offset", "sky5_haze_thickness", "sky5_haze_shift", "sky5_fog_offset",
                                 "sky5_sun_glow_size", "sky5_sun_disc", "sky5_moon_glow_size", "sky5_moon_disc",
                                 "sky5_star_intensity", "sky5_comet_intensity", "sky5_star_orientation"), five)
            draw_sky_switches(col, sky, "sky5_flag2_", names.SKY5_SWITCHES2)


class STECYK3D_PT_world_all(_BrycePanel, bpy.types.Panel):
    bl_label = "Every sky setting"
    bl_context = "world"
    bl_parent_id = "STECYK3D_PT_world"
    bl_options = {"DEFAULT_CLOSED"}

    @classmethod
    def poll(cls, context):
        return super().poll(context) and context.world is not None

    def draw(self, context):
        col = self.layout.column()
        col.use_property_split = True
        draw_group(col, gather.sky_group(context.world), is_five(context), names.FIVE_ONLY["sky"])


class STECYK3D_PT_light(_BrycePanel, bpy.types.Panel):
    bl_label = "Bryce light"
    bl_context = "data"

    @classmethod
    def poll(cls, context):
        return super().poll(context) and context.light is not None

    def draw(self, context):
        light = context.light
        col = self.layout.column()
        col.use_property_split = True
        col.row().prop(light, "type", expand=True)
        if light.type == "SUN":
            col.label(text="The sky's sun: its settings are in World properties > Bryce sky")
            return
        if gather.find_node(light.node_tree if light.use_nodes else None, "BryceLightNode") is None:
            col.label(text="Renders as a new Bryce light")
            col.operator("stecyk3d.make_light", icon="LIGHT")
        else:
            col.label(text="The light is its node (Shader Editor, the lamp's nodes); this shows it", icon="NODETREE")
        draw_light(col, light_group(light), is_five(context), light)


LIGHT_CLASS_LABELS = {"sphere": "Radial light", "cone": "Spotlight", "pyramid": "Square spotlight",
                      "cylinder": "Parallel light", "cube": "Square parallel light"}


def light_class(lamp):
    """Which of Bryce's five light classes a Blender lamp becomes (the earlier add-on's schema.light_class)."""
    if lamp.type == "POINT":
        return "sphere"
    if lamp.type == "SPOT":
        return "pyramid" if lamp.use_square else "cone"
    if lamp.type == "AREA":
        return "cylinder" if lamp.shape in ("DISK", "ELLIPSE") else "cube"
    return None


def draw_light(col, b, five, lamp=None):
    """A light's settings in the earlier add-on's order: the light class and beam (from the lamp), colour, strength,
    Bryce 5's, material, edges, flags. `lamp`: the Blender lamp (None: no class line, no beam)."""
    if lamp is not None:
        col.label(text=LIGHT_CLASS_LABELS.get(light_class(lamp), "The sky's sun"))
        if lamp.type == "SPOT":
            col.prop(lamp, "spot_size", text="Beam angle")
            col.prop(lamp, "use_square", text="Square beam")
        elif lamp.type == "AREA":
            col.prop(lamp, "shape", text="Beam")
            col.prop(lamp, "size", text="Width")
            if lamp.shape in ("RECTANGLE", "ELLIPSE"):
                col.prop(lamp, "size_y", text="Depth")
        col.separator()
    prop_list(col, b, ("color", "intensity"), five)
    if five:
        prop_list(col, b, ("range", "radius", "shadow_ambience"), five)
        if b.shadow_ambience != 0.0:
            col.prop(b, "shadow_color", text="Shadow color")
        box = col.box()
        box.label(text="Gradient")
        prop_list(box, b, ("gradient_range", "gradient_offset"), five)
        draw_gradient_keys(box, b)
        for word_name, title in (("gradient_color_flags", "Colours"), ("gradient_value_flags", "Strengths")):
            grid = box.grid_flow(columns=2, align=True)
            for bit, label, _text in GRADIENT_WRAPS:
                op = grid.operator("stecyk3d.light_flag", text=title + ": " + label, depress=bool(getattr(b, word_name) & bit))
                op.bit, op.word = bit, word_name
        prop_list(box, b, ("gradient_color_flags", "gradient_value_flags"), five)
    prop_list(col, b, ("material", "tint"), five)
    cls = light_class(lamp) if lamp is not None else None
    if cls != "sphere":  # a radial light has no edge and no reach
        col.prop(b, "edge", index=0, text="Edge falloff" if cls in ("cone", "cylinder") else "Edge falloff, across")
        if cls in ("pyramid", "cube", None):
            col.prop(b, "edge", index=1, text="Edge falloff, along")
        col.prop(b, "reach", text="Reach")
    grid = col.grid_flow(columns=2, align=True)
    for bit, label, _text in LIGHT_FLAG_SWITCHES:
        if bit == GRADIENT_FLAG and not five:  # the gradient is Bryce 5's alone
            continue
        grid.operator("stecyk3d.light_flag", text=label, depress=bool(b.flags & bit)).bit = bit
    col.prop(b, "flags", text="Flags")


def draw_material(layout, context, material, node):
    """T11: the Bryce Material node's channels and settings in the Material tab (the same values as the node's own
    sockets, edited in place), in the earlier add-on's order: channels first, then the node's switches and numbers;
    without the node, the older `bryce` group and the buttons that make one."""
    if node is None:
        layout.label(text="Renders as a new Bryce material")
        layout.operator("stecyk3d.new_material", text="Make it a new Bryce material", icon="MATERIAL")
        layout.operator("stecyk3d.material_from_shader", icon="NODE_MATERIAL")
        col = layout.column()
        col.use_property_split = True
        draw_group(col, getattr(material, names.DATA_GROUP), is_five(context))
        layout.operator("stecyk3d.settings_to_nodes", icon="NODETREE")
        return
    layout.label(text="The material is its nodes (Shader Editor); this shows them", icon="NODETREE")
    draw_channels(layout, node)
    col = layout.column()
    col.use_property_split = True
    group_links.real(node).draw_buttons(context, col)
    layout.operator("stecyk3d.new_material", icon="LOOP_BACK")


def source_name(node):
    return node.label or node.name


def draw_channels(layout, node):
    """A Bryce Material node's thirteen channels as the earlier add-on's Material tab shows them: each constant (with
    what is linked to it), or, where a Bryce Mix Colour or Value feeds it, the channel with the mix's choice and its
    A, B and (mixing by a third) "by" rows."""
    for group in (names.COLOUR_CHANNELS, names.VALUE_CHANNELS):
        box = layout.box()
        for name in group:
            socket = node.inputs[name]
            source = gather.linked_from(socket) if socket.is_linked else None
            if source is None:
                box.prop(group_links.real(socket), "value", text=human(name))
            elif source.bl_idname in gather.MIX_NODES:
                row = box.row(align=True)
                row.label(text=human(name))
                row.prop(group_links.real(source), "by", text="")
                keys = (("a", "   A"), ("b", "   B")) + ((("alpha", "   by"),) if source.by == "ALPHA" else ())
                for key, title in keys:
                    place = source.inputs[key]
                    row = box.row(align=True)
                    row.prop(group_links.real(place), "value", text=title)
                    fed = gather.linked_from(place) if place.is_linked else None
                    if fed is not None:
                        row.label(text=source_name(fed), icon="TEXTURE")
            else:
                box.label(text="%s: from %s" % (human(name), source_name(source)), icon="TEXTURE")


def graph_messages(material):
    """What the render would say about the material's graph: (refusals, notes), as gather reads it."""
    problems, notices = [], []
    try:
        gather.material_plain(material, problems, notices, [])
    except Exception as error:  # a graph the walk cannot read is itself the fault
        problems.append(str(error))
    return problems, notices


def converted_lists(node):
    """The "copied" and "left" lines a converted material kept in its `converted` text (JSON), or None."""
    import json
    if node is None or not node.converted:
        return None
    try:
        told = json.loads(node.converted)
    except ValueError:
        return None
    if not isinstance(told, dict):
        return None
    return [str(x) for x in told.get("copied", [])], [str(x) for x in told.get("left", [])]


def _fed(socket):
    return gather.linked_from(socket) if socket.is_linked else None


def texture_parts(node):
    """The nodes a Bryce Texture is made of, as the earlier add-on's texture editor lists them: its mapping, the noise
    that is its phase, and its components in Bryce's order, each (component, blend before it or None, filter or None,
    noise or None) from the Bryce Component, Bryce Blend, Bryce Filter and Bryce Noise nodes linked to it."""
    mapping = _fed(node.inputs["vector"])
    mapping = mapping if mapping is not None and mapping.bl_idname == "BryceMappingNode" else None
    phase = _fed(node.inputs["phase"]) if "phase" in node.inputs else None
    phase = phase if phase is not None and phase.bl_idname == "BryceNoiseNode" else None
    found = []

    def walk(socket, blend):
        end = _fed(socket)
        if end is None:
            return
        if end.bl_idname == "BryceBlendNode":
            walk(end.inputs["first"], blend)
            walk(end.inputs["second"], end)
        elif end.bl_idname == "BryceComponentNode":
            filt = _fed(end.inputs["value"])
            noise = filt
            if filt is not None and filt.bl_idname == "BryceFilterNode":
                noise = _fed(filt.inputs["value"])
            else:
                filt = None
            noise = noise if noise is not None and noise.bl_idname == "BryceNoiseNode" else None
            found.append((end, blend, filt, noise))

    if "component" in node.inputs:  # (a Bryce Picture has none)
        walk(node.inputs["component"], None)
    return mapping, phase, found


def draw_node_body(layout, context, node):
    col = layout.column()
    col.use_property_split = True
    group_links.real(node).draw_buttons(context, col)


def draw_noise_with_phase(layout, context, noise, title):
    """A Bryce Noise's own settings and the amount (and noise) of its phase."""
    box = layout.box()
    box.label(text=title)
    draw_node_body(box, context, noise)
    box.prop(group_links.real(noise.inputs["phase"]), "value", text="Phase amount")
    phase = _fed(noise.inputs["phase"])
    if phase is not None and phase.bl_idname == "BryceNoiseNode":
        pb = box.box()
        pb.label(text="Phase: " + source_name(phase))
        draw_node_body(pb, context, phase)


def draw_texture_parts(layout, context, node):
    """T11: one texture as the earlier add-on's Material tab shows it: its Bryce Mapping, the Bryce Texture's settings
    (its filter, colours, what it gives, the phase of the whole texture), then each component with its blend, noise,
    filter, colours and phase, all the nodes' own properties drawn by the nodes' own draw code."""
    mapping, phase, found = texture_parts(node)
    box = layout.box()
    if mapping is not None:
        box.label(text="Bryce Mapping", icon="ORIENTATION_GLOBAL")
        draw_node_body(box, context, mapping)
    else:
        box.label(text="No Bryce Mapping: " + ("by the surface's own coordinates, shown once"
                                               if node.bl_idname == "BrycePictureNode" else "in object space"),
                  icon="ORIENTATION_GLOBAL")
    if node.bl_idname == "BrycePictureNode":
        layout.template_ID(group_links.real(node), "image", open="image.open")
        return
    box = layout.box()
    box.label(text="Bryce Texture", icon="TEXTURE")
    draw_node_body(box, context, node)
    box.prop(group_links.real(node.inputs["phase"]), "value", text="Phase amount")
    if phase is not None:
        draw_noise_with_phase(box, context, phase, "Phase of the texture: " + source_name(phase))
    for k, (comp, blend, filt, noise) in enumerate(found):
        box = layout.box()
        box.label(text="Component %d: %s%s" % (k + 1, source_name(noise) if noise is not None else "Nothing",
                                              ", blended by " + source_name(blend) if blend is not None else ""))
        if blend is not None:
            draw_node_body(box, context, blend)
        if noise is not None:
            draw_noise_with_phase(box, context, noise, "Noise")
        if filt is not None:
            fb = box.box()
            fb.label(text="Filter")
            draw_node_body(fb, context, filt)
        draw_node_body(box, context, comp)


class STECYK3D_OT_dismiss_converted(bpy.types.Operator):
    """Clear the list of what was copied from a Blender material and what was left (the material is unchanged)"""
    bl_idname = "stecyk3d.dismiss_converted"
    bl_label = "Dismiss this list"
    bl_options = {"INTERNAL", "UNDO"}

    @classmethod
    def poll(cls, context):
        material = context.material
        node = gather.find_node(material.node_tree if material and material.use_nodes else None, "BryceMaterialNode")
        return node is not None and bool(node.converted)

    def execute(self, context):
        material = context.material
        for node in material.node_tree.nodes:
            if node.bl_idname == "BryceMaterialNode":
                node.converted = ""
        return {"FINISHED"}


class STECYK3D_PT_material(_BrycePanel, bpy.types.Panel):
    bl_label = "Bryce material"
    bl_context = "material"

    @classmethod
    def poll(cls, context):
        return super().poll(context) and context.material is not None

    def draw(self, context):
        from . import preset_ops
        material = context.material
        layout = self.layout
        tree = material.node_tree if material.use_nodes else None
        node = gather.find_node(tree, "BryceMaterialNode")
        mix = gather.mix_root(tree)
        preset_ops.draw_preset_box(layout, context, "materials")
        ends = []
        if mix is not None:
            for key in ("first", "second"):
                try:
                    end = gather.linked_from(mix.inputs[key])
                except Exception:
                    end = None
                ends.append(end if end is not None and end.bl_idname == "BryceMaterialNode" else None)
            node = ends[0] or ends[1] or node
        for end in (ends or [node]):
            told = converted_lists(end)
            if told is None:
                continue
            copied, left = told
            box = layout.box()
            box.label(text="From a Blender material: what is the same thing in both was copied", icon="INFO")
            for line in copied:
                box.label(text=line, icon="CHECKMARK")
            if left:
                box.label(text="Left for you to set in Bryce's terms:")
            for line in left:
                box.label(text=line, icon="DOT")
            box.operator(STECYK3D_OT_dismiss_converted.bl_idname, icon="X")
            break
        if node is not None:
            problems, notices = graph_messages(material)
            for line in problems:
                layout.label(text=line, icon="ERROR")
            for line in notices:
                layout.label(text=line, icon="INFO")
        if mix is not None:
            layout.prop(group_links.real(mix), "amount", text="Two materials mixed", slider=True)
            for title, end in zip(("First: ", "Second: "), ends):
                if end is None:
                    layout.label(text=title + "no Bryce Material linked", icon="ERROR")
                    continue
                layout.label(text=title + source_name(end))
                draw_channels(layout, end)
                col = layout.column()
                col.use_property_split = True
                group_links.real(end).draw_buttons(context, col)
            return
        draw_material(layout, context, material, node)


class STECYK3D_PT_camera(_BrycePanel, bpy.types.Panel):
    bl_label = "Bryce camera"
    bl_context = "data"

    @classmethod
    def poll(cls, context):
        return super().poll(context) and context.camera is not None

    def draw(self, context):
        cam = context.camera
        col = self.layout.column()
        col.use_property_split = True
        if cam.type == "PERSP":
            col.label(text="Perspective: the lens and the sensor give Bryce's field of view")
            col.prop(cam, "lens")
            col.prop(cam, "shift_x", text="Pan, across")
            col.prop(cam, "shift_y", text="Pan, up")
        elif cam.type == "ORTHO":
            col.label(text="Bryce's parallel projection: every ray runs straight ahead")
            col.prop(cam, "ortho_scale", text="Frame width")
            col.prop(cam, "shift_x", text="Pan, across")
            col.prop(cam, "shift_y", text="Pan, up")
        elif cam.type == "PANO":
            col.label(text="Bryce's panorama: the whole turn on an upright cylinder")
            col.label(text="(its height spans a quarter turn up and down by tangent; the lens does not count)")
        else:
            col.label(text="No counterpart in Bryce: the render refuses this camera", icon="ERROR")


TREE_BOXES = (  # , grouped as the earlier add-on's Tree Lab panel groups them
    ("Trunk and branches", ("thickness", "least", "whorls", "trunk_whorl", "whorl", "share", "angle_thick",
                            "angle_thin", "twist", "gravity", "spread", "random")),
    ("Shape (distance from the trunk, height)", ("bottom", "low", "high", "top")),
    ("Foliage (material slot 2: the leaves')", ("leaf_kind", "leaves", "leaf_size", "leaf")),
)


class STECYK3D_OT_shape_standin(bpy.types.Operator):
    """Replace this object's mesh by one that shows its Bryce shape (after changing the shape, a torus's radii, or a terrain's picture)"""
    bl_idname = "stecyk3d.shape_standin"
    bl_label = "Show the shape"
    bl_options = {"REGISTER", "UNDO"}

    @classmethod
    def poll(cls, context):
        obj = context.object
        return obj is not None and obj.type == "MESH" and (obj.bryce_shape in create.SHAPES_ADDED or obj.bryce_shape == "terrain")

    def execute(self, context):
        from . import terrain_tool
        obj = context.object
        old = obj.data
        if obj.bryce_shape == "terrain":
            group = getattr(obj, names.OBJECT_TERRAIN_GROUP)
            image = group.image
            if image is None or image.size[0] * image.size[1] == 0:
                self.report({"ERROR"}, "the terrain has no picture")
                return {"CANCELLED"}
            n = image.size[0]
            if image.size[1] != n:
                self.report({"ERROR"}, "the terrain's picture %r is not square" % image.name)
                return {"CANCELLED"}
            buffer = array.array("f", bytes(4 * n * n * image.channels))
            image.pixels.foreach_get(buffer)
            heights = [round(v * terrain_tool.HEIGHT_TOP) for v in buffer[0::image.channels]]
            mesh = terrain_tool.stand_in_mesh(obj.name, n, heights, group.mirrored)
        else:
            torus = obj.get("bryce_torus")
            verts, faces = create.stand_in(obj.bryce_shape, tuple(torus) if torus is not None else None)
            mesh = bpy.data.meshes.new(obj.name)
            mesh.from_pydata(verts, [], faces)
            mesh.update()
        obj.data = mesh
        if old.users == 0:
            bpy.data.meshes.remove(old)
        return {"FINISHED"}


class STECYK3D_PT_object(_BrycePanel, bpy.types.Panel):
    bl_label = "Bryce shape"
    bl_context = "object"

    @classmethod
    def poll(cls, context):
        return super().poll(context) and context.object is not None and context.object.type in ("MESH", "EMPTY")

    def draw(self, context):
        obj = context.object
        col = self.layout.column()
        col.use_property_split = True
        five = is_five(context)
        member = obj.parent is not None and obj.parent.get("bryce_shape") == "group"
        col.prop(obj, "bryce_role", text="Role in its group" if member else "Role (counts in a Bryce group)")
        if obj.bryce_role == "NEGATIVE":
            col.prop(obj, "bryce_cut_shows_positive", text="Cut shows positive")
        col.separator()
        col.prop(obj, "bryce_shape", text="Shape")
        shape = obj.bryce_shape
        if obj.type == "MESH" and shape != "terrain":
            col.operator("stecyk3d.terrain_from_mesh", icon="RNDCURVE")
        if shape == "terrain":
            draw_group(col.box(), getattr(obj, names.OBJECT_TERRAIN_GROUP), five)
            col.operator("stecyk3d.shape_standin", icon="MESH_DATA", text="Show the terrain")
            return
        if shape == "torus":
            if "bryce_torus" in obj:
                col.prop(obj, '["bryce_torus"]', text="Torus radii (tube, ring)")
            else:
                col.label(text="Torus radii: none set; Add > Bryce shape > Torus gives them")
        if shape in ("metaball", "tree") and not five:
            col.label(text="Bryce 5's: this program renders the stand-in mesh", icon="INFO")
        if shape == "metaball":
            col.prop(obj, "bryce_negative", text="Negative")
        if shape == "tree":
            t = getattr(obj, names.OBJECT_TREE_GROUP)
            col.label(text="Tree Lab: the tree is 2 high, its foot 1 below the object's origin")
            col.prop(t, "seed", text=human("seed"))
            for title, keys in TREE_BOXES:
                box = col.box()
                box.label(text=title)
                prop_list(box, t, keys, five)
        if shape in create.SHAPES_ADDED:
            col.operator("stecyk3d.shape_standin", icon="MESH_DATA")


CLASSES = [STECYK3D_OT_list_edit, STECYK3D_OT_light_flag, STECYK3D_OT_light_lab, STECYK3D_OT_preferences_show, STECYK3D_OT_scene_flag, STECYK3D_OT_shape_standin,
           STECYK3D_OT_dismiss_converted, STECYK3D_PT_render, STECYK3D_PT_render_finishing, STECYK3D_PT_world, STECYK3D_PT_world_all, STECYK3D_PT_light, STECYK3D_PT_camera, STECYK3D_PT_material,
           STECYK3D_PT_object]

CLASSES.append(STECYK3D_OT_sky_new)


def register():
    for cls in CLASSES:
        bpy.utils.register_class(cls)


def unregister():
    for cls in reversed(CLASSES):
        bpy.utils.unregister_class(cls)
