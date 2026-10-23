# SPDX-License-Identifier: GPL-3.0-or-later
# (Node groups: links read through them)
"""Follows a link through Blender node groups, so a Bryce texture or mix wrapped in a group reads as if it were
laid flat. A link into a group node's output goes to the inner tree's Group Output input behind it; a Group Input
output goes back out to the link on the group node's matching input; groups nest (MAX_DEPTH deep) and Reroutes
are passed. NodeView and SocketView wrap a node and socket with the group nodes it was reached through, so the
readers that walk `.inputs[...].links[0].from_node` (texture_nodes, gather) need no change. Attribute access only
(plain test objects work). A cycle, a group with no tree or no Group Output raise GroupLinkError, naming it."""

MAX_DEPTH = 16  # nesting + reroutes passed along one link; more is a cycle or a mistake


class GroupLinkError(ValueError):
    pass


def _index(sockets, socket):
    """Where `socket` sits among `sockets`: the same position in the group node and in its inner Group Input/Output."""
    for i, other in enumerate(sockets):
        if other == socket or (other.identifier == socket.identifier and other.name == socket.name):
            return i
    raise GroupLinkError("socket %r is not on its node" % socket.name)


def _inner_output(group):
    tree = group.node_tree
    if tree is None:
        raise GroupLinkError("group %s has no node tree" % group.name)
    outs = [n for n in tree.nodes if n.bl_idname == "NodeGroupOutput"]
    if not outs:
        raise GroupLinkError("group %s (%s) has no Group Output" % (group.name, tree.name))
    return next((n for n in outs if getattr(n, "is_active_output", False)), outs[0])


def resolve(socket, stack=()):
    """(from_node, from_socket, stack) for the node that really feeds the input `socket`, or None where nothing
    does (unlinked, or a group's output/input with nothing behind it). `stack` is the group nodes passed, outermost
    first. Raises GroupLinkError for a cycle or a broken group."""
    if not (socket.is_linked and socket.links):
        return None
    link = socket.links[0]
    node, out, stack = link.from_node, link.from_socket, tuple(stack)
    for _ in range(MAX_DEPTH):
        kind = node.bl_idname
        if kind == "ShaderNodeGroup":
            inner = _inner_output(node)
            slot = inner.inputs[_index(node.outputs, out)]
            if not (slot.is_linked and slot.links):
                return None
            stack += (node,)
        elif kind == "NodeGroupInput":
            if not stack:
                return None  # a group opened on its own: its inputs are not linked to anything
            slot = stack[-1].inputs[_index(node.outputs, out)]
            if not (slot.is_linked and slot.links):
                return None
            stack = stack[:-1]
        elif kind == "NodeReroute":
            slot = node.inputs[0]
            if not (slot.is_linked and slot.links):
                return None
        else:
            return node, out, stack
        link = slot.links[0]
        node, out = link.from_node, link.from_socket
    raise GroupLinkError("%s: links through node groups go more than %d deep (a group that contains itself?)"
                         % (node.name, MAX_DEPTH))


class LinkView:
    def __init__(self, node, socket):
        self.from_node, self.from_socket = node, socket


class SocketView:
    """An input socket whose links are resolved through groups."""

    def __init__(self, socket, stack):
        self._socket, self._stack = socket, stack

    def __getattr__(self, name):
        return getattr(self._socket, name)

    @property
    def real(self):
        """The Blender socket itself (what `layout.prop` takes)."""
        return self._socket

    @property
    def links(self):
        found = resolve(self._socket, self._stack)
        return [] if found is None else [LinkView(NodeView(found[0], found[2]), found[1])]


class _Inputs:
    def __init__(self, node, stack):
        self._node, self._stack = node, stack

    def __getitem__(self, key):
        return SocketView(self._node.inputs[key], self._stack)

    def __iter__(self):
        return (SocketView(s, self._stack) for s in self._node.inputs)

    def __len__(self):
        return len(self._node.inputs)


class NodeView:
    """A node with the groups it was reached through; `name` is the node's own name (the fault text of the render
    and of the node's own display agree), `key` tells two instances of one group's node apart."""

    def __init__(self, node, stack=()):
        self._node, self._stack = node, tuple(stack)

    def __getattr__(self, name):
        return getattr(self._node, name)

    @property
    def real(self):
        """The Blender node itself (what `layout.prop` and `template_ID` take)."""
        return self._node

    @property
    def inputs(self):
        return _Inputs(self._node, self._stack)

    @property
    def key(self):
        return "/".join([g.name for g in self._stack] + [self._node.name])

    def __eq__(self, other):
        return isinstance(other, NodeView) and self.key == other.key and self._node == other._node

    def __hash__(self):
        return hash(self.key)


def view(node):
    """`node` read through groups (a node of a material's own tree)."""
    return node if isinstance(node, NodeView) else NodeView(node)


def real(thing):
    """The Blender node or socket behind `thing` (a NodeView or SocketView), or `thing` itself if it is one already."""
    return thing.real if isinstance(thing, (NodeView, SocketView)) else thing


def linked_from(socket):
    """The node feeding input `socket` (a plain socket or a SocketView), through groups, or None."""
    if not isinstance(socket, SocketView):
        socket = SocketView(socket, ())
    links = socket.links
    return links[0].from_node if links else None
