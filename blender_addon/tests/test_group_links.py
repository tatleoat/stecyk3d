# SPDX-License-Identifier: GPL-3.0-or-later
# (Node groups: links read through them)
"""Bryce texture networks wrapped in node groups (the earlier add-on's files): reading through group_links gives the
description of the flat network, nested groups too; cycles and broken groups are refused with a message naming
them. Stand-in nodes (attribute access only); python3 standard library only."""

import types
import unittest

from stecyk3d import group_links, texture_nodes, texture_settings


class Sock:
    def __init__(self, name, value=0.0):
        self.name = self.identifier = name
        self.value = value
        self.links = []

    @property
    def is_linked(self):
        return bool(self.links)


class Socks(list):
    def __getitem__(self, key):
        if isinstance(key, str):
            return next(s for s in self if s.name == key)
        return list.__getitem__(self, key)


def node(idname, name, ins=(), outs=(), **attrs):
    return types.SimpleNamespace(bl_idname=idname, name=name, inputs=Socks(Sock(n) for n in ins),
                                 outputs=Socks(Sock(n) for n in outs), **attrs)


def link(src, out, dst, inp):
    s = src.outputs[out]
    dst.inputs[inp].links = [types.SimpleNamespace(from_node=src, from_socket=s)]


def noise():
    return node("BryceNoiseNode", "noise", ["phase"], ["value"], kind=6, method=0, count=4, dims=2, custom=False,
                rotation=[12.0, 90.0], frequency=[51.0, 51.0, -64.0], matrix=[0.0] * 9)
    

def component():
    return node("BryceComponentNode", "comp", ["value"], ["component"], flags=2, space=0, mode=4, first=1,
                second=2, third=3)


def mapping():
    return node("BryceMappingNode", "map", [], ["vector"], mode=1, flags=0, scale=[1.0] * 3, offset=[0.0] * 3,
                rotation=[0.0, 0.0, 0.0, 1.0])


def texture():
    t = node("BryceTextureNode", "tex", ["component", "phase", "vector"], ["colour", "alpha", "bump"], gives=2,
             color_space=0, filter=types.SimpleNamespace(kind=0, n=0, a=2.0, b=0.0),
             colors=types.SimpleNamespace(mode=0, first=0, second=0xFFFFFF, third=0))
    t.inputs["phase"].value = 0.0
    return t


def settings(tex):
    return texture_settings.texture_settings(texture_nodes.texture_input(group_links.view(tex)))


def flat():
    t, c, n, m = texture(), component(), noise(), mapping()
    link(n, "value", c, "value")
    link(c, "component", t, "component")
    link(m, "vector", t, "vector")
    return t


def group(name, ins, outs, inner_nodes):
    """A group node with an inner tree: Group Input, Group Output and `inner_nodes`."""
    gin = node("NodeGroupInput", "Group Input", [], ins)
    gout = node("NodeGroupOutput", "Group Output", outs, [], is_active_output=True)
    tree = types.SimpleNamespace(name=name + " tree", nodes=[gin, gout] + list(inner_nodes))
    return node("ShaderNodeGroup", name, ins, outs, node_tree=tree), gin, gout


def wrapped():
    """The flat network with the texture, its component and noise inside a group; the mapping outside feeds the
    group's `Vector`, and the texture's colour is the group's `Colour`. Returns (the node the colour comes out of)."""
    t, c, n, m = texture(), component(), noise(), mapping()
    grp, gin, gout = group("Group", ["Vector"], ["Colour"], [t, c, n])
    link(n, "value", c, "value")
    link(c, "component", t, "component")
    link(gin, "Vector", t, "vector")
    link(t, "colour", gout, "Colour")
    link(m, "vector", grp, "Vector")
    return grp, t


class GroupLinks(unittest.TestCase):
    def test_a_group_reads_as_the_flat_network(self):
        grp, _inner = wrapped()
        sink = node("BryceMaterialNode", "mat", ["diffuse_color"])
        link(grp, "Colour", sink, "diffuse_color")
        found = group_links.linked_from(sink.inputs["diffuse_color"])
        self.assertEqual(found.bl_idname, "BryceTextureNode")
        self.assertEqual(settings(found), settings(flat()))

    def test_nested_groups(self):
        grp, _inner = wrapped()
        # the whole of it once more inside an outer group, whose Vector reaches the inner group's Vector
        outer, oin, oout = group("Outer", ["Vector"], ["Colour"], [grp])
        link(oin, "Vector", grp, "Vector")
        link(grp, "Colour", oout, "Colour")
        m = mapping()
        link(m, "vector", outer, "Vector")
        sink = node("BryceMaterialNode", "mat", ["diffuse_color"])
        link(outer, "Colour", sink, "diffuse_color")
        found = group_links.linked_from(sink.inputs["diffuse_color"])
        self.assertEqual(found.key, "Outer/Group/tex")
        self.assertEqual(settings(found), settings(flat()))

    def test_an_unlinked_group_input_is_no_link(self):
        t = texture()
        grp, gin, gout = group("G", ["Vector"], ["Colour"], [t])
        link(gin, "Vector", t, "vector")
        link(t, "colour", gout, "Colour")
        sink = node("BryceMaterialNode", "mat", ["diffuse_color"])
        link(grp, "Colour", sink, "diffuse_color")
        found = group_links.linked_from(sink.inputs["diffuse_color"])
        self.assertEqual(found.inputs["vector"].links, [])

    def test_a_group_with_nothing_behind_its_output_is_no_link(self):
        grp, _gin, _gout = group("G", [], ["Colour"], [])
        sink = node("BryceMaterialNode", "mat", ["diffuse_color"])
        link(grp, "Colour", sink, "diffuse_color")
        self.assertIsNone(group_links.linked_from(sink.inputs["diffuse_color"]))

    def test_a_cycle_is_refused_by_name(self):
        grp, gin, gout = group("Loop", ["In"], ["Out"], [])
        link(gin, "In", gout, "Out")  # passes the value straight through ...
        link(grp, "Out", grp, "In")  # ... and the group feeds itself
        sink = node("BryceMaterialNode", "mat", ["diffuse_color"])
        link(grp, "Out", sink, "diffuse_color")
        with self.assertRaises(group_links.GroupLinkError) as caught:
            group_links.linked_from(sink.inputs["diffuse_color"])
        self.assertIn("deep", str(caught.exception))

    def test_a_group_without_a_tree_is_refused_by_name(self):
        grp = node("ShaderNodeGroup", "Empty", [], ["Colour"], node_tree=None)
        sink = node("BryceMaterialNode", "mat", ["diffuse_color"])
        link(grp, "Colour", sink, "diffuse_color")
        with self.assertRaisesRegex(group_links.GroupLinkError, "Empty"):
            group_links.linked_from(sink.inputs["diffuse_color"])

    def test_a_fault_inside_a_group_is_the_texture_fault_of_the_flat_network(self):
        grp, inner = wrapped()
        inner.inputs["component"].links = []
        bad = flat()
        bad.inputs["component"].links = []
        self.assertEqual(texture_nodes.faults_of(inner, "material 'M'"), texture_nodes.faults_of(bad, "material 'M'"))
        self.assertTrue(texture_nodes.faults_of(inner, "material 'M'"))


def _error(t):
    try:
        texture_nodes.texture_input(t)
    except texture_nodes.ERRORS as error:
        return error


if __name__ == "__main__":
    unittest.main()
