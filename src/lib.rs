// SPDX-License-Identifier: MIT
//! stecyk3d: a renderer for Bryce 3.0, 3.1 and 5.0 scenes (the Stecyk3D renderer).

// (bit-exact arithmetic). `(x - x) / (x - x)` makes the NaN a published
// routine makes, and `!(a < b)` keeps a NaN on the side the entry puts it; both are deliberate.
#![allow(clippy::eq_op, clippy::neg_cmp_op_on_partial_ord)]

pub mod animation;
pub mod cases;
pub mod colour;
pub mod description;
pub mod extract;
pub mod geom;
pub mod geometry;
pub mod numerics;
pub mod render;
pub mod sampling;
pub mod scene;
pub mod scene_beams;
pub mod scene_export;
pub mod scene_file;
pub mod scene_library;
pub mod scene_meshes;
pub mod scene_parts;
pub mod scene_paths;
pub mod scene_pictures;
pub mod scene_height_fields;
pub mod scene_records;
pub mod scene_sky;
pub mod scene_textures;
pub mod scene_opening;
pub mod scene_tracks;
pub mod scene_track_list;
pub mod scene_tracking;
pub mod scene_trees;
pub mod scene_view;
pub mod shading;
pub mod sky;
pub mod version;
pub mod sha256;
pub mod substrate;
pub mod substrate_kept;
pub mod terrain;
pub mod terrain_editor;
pub mod terrain_tools;
pub mod terrain_clip; // , rule 8 (edges by an amount)
pub mod terrain_shapes; // C9 to C11
pub mod terrain_pictures;
pub mod terrain_picture_tab;
pub mod terrain_fractal;
pub mod textures;
pub mod transform_parts;
pub mod trees;
