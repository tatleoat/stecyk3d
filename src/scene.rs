// SPDX-License-Identifier: MIT
//! The scene reader: a document's bytes to a description of the scene.
//!
//! (through `scene_file`; the mesh part,, in `scene_meshes`; decoders in `scene_records`;
//! the view's transform,, in `scene_view`; the texture
//! resources' parts,, in `scene_textures`; the tree part,, in `scene_trees`;
//! the terrain's own part and heights,, in `scene_height_fields`)
//! (the linked lists' objects and their transforms per version; the chain's
//! arithmetic in `animation::LinkArithmetic`)
//! , (the light part's
//! versions, the gradient reference;: the gradient resource it names), (binding the
//! tracks; their layout in `scene_tracks`, their application on opening in `scene_opening`),
//! , as revised at (a camera
//! linked to an object or held to a path: `linked_view`, `chain_as`),
//! (the object track list, read in `scene_track_list`, its rows resolved here
//! to objects; what they do,, in `scene_tracking`)
//! (The scene reader), (
//! linked objects)

use std::fmt;

use crate::geom::Vec3;
use crate::geometry::Mesh;
use crate::scene_file::{Container, ContainerError, Flavour, IndexError, Resource, ResourceIndex};
use crate::scene_parts::{
    CAMERA_PART_LEN, CameraPart, FIRST_PART_LEN, GradientPart, LightPart, MaterialPart, Rgb, SKY_AXIS_VIEW,
    SUN_PART_LEN, Sky, SunPart, first_part_name, order_of,
};
use crate::scene_records::{
    ByteOrder, DocSettings, ObjectKind, ObjectList, ObjectRow, PLACEMENT_LEN, PLACEMENT_V2_VERSION,
    PlacementPart, RecordError, TORUS_PART_LEN, TorusPart, Transform,
};
use crate::scene_parts::TextureRef;
use crate::scene_pictures::{HeldPicture, PICTURE_RESOURCE_CLASS, PictureRefusal, decode_held};
use crate::scene_textures::{
    MAPPING_PART_LEN, Mapping, PICTURE_DATA_LEN, PICTURE_READ_LEN, PROCEDURAL_DATA_LEN, PictureTexture,
    ProceduralTexture, RecordForm, Texture, TextureBody, TextureClass, TextureRefusal, picture_part,
};
use crate::scene_trees::{StoredTree, TreePart, TreeRefusal};
use crate::scene_meshes::{MeshRefusal, MeshResource};
use crate::scene_height_fields::{TerrainPart, TerrainRefusal};
use crate::animation::{OpeningInputs, PartsForm};
use crate::scene_tracks::{DocTrack, ObjectTrackSlots, SceneTracks, TrackRefusal, TrackValues, slots as track_slots};
use crate::scene_view::view_transforms;
use crate::scene_beams::{BeamClass, BeamRecord};
pub use crate::scene_view::{OpenView, ViewTransform, ViewTransforms};
use crate::scene_track_list::{ListedObject, TRACK_LIST, TrackList, TrackListRefusal};
pub use crate::scene_tracking::TrackRow;
use crate::version::RenderAs;

/// Class tags of that this reader uses beyond's table. [format]
const DOCSET: &str = "brycedocset";
const ROOT_LIST: &str = "C3dRootObjList";
const LINKED_LIST: &str = "C3dObjList";
const SKY: &str = "C3dBaseSky";
const SUN: &str = "C3dSunLight";
pub(crate) const MATERIAL: &str = "C3dBaseMaterial";
/// A light's gradient.
const GRADIENT: &str = "C3dBaseGradient";

/// The view cameras' names. [format]
const VIEW_NAMES: [&[u8]; 8] = [b"perspective", b"top", b"right", b"front", b"left", b"back", b"bottom", b"director"];
/// The views along the axes, for which the sky's bit 0x400 is set.
const AXIS_VIEWS: [&[u8]; 6] = [b"top", b"right", b"front", b"left", b"back", b"bottom"];

/// Why a document is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SceneError {
    Container(ContainerError),
    Index(IndexError),
    /// A part's length or version; an object list's form.
    Record { class: String, name: String, error: RecordError },
    RequiredCount { class: &'static str, count: usize },
    /// A resource's data entry is not in the directory.
    NoDataEntry { class: String, identity: u32, entry: u32 },
    ShortData { class: String, name: String, len: usize, needed: usize },
    NoSuchObject { row: usize, class: &'static str, identity: u32 },
    /// (a linked list's row names a resource the document lacks; or a row names a
    /// linked list the document lacks)
    NoSuchLinked { list: u32, row: usize, class: &'static str, identity: u32 },
    NoSuchMaterial { object: String, identity: u32 },
    NoView,
    /// A tree resource refused.
    Tree { object: String, refusal: TreeRefusal },
    /// A texture reference or the texture it names refused; `referrer` says whose
    /// reference it is (a material's channel, or the sky's layer).
    Texture { referrer: String, refusal: TextureRefusal },
    /// A picture a picture texture names refused.
    Picture { name: String, identity: u32, refusal: PictureRefusal },
    /// A mesh resource refused.
    Mesh { object: String, refusal: MeshRefusal },
    /// A height field refused.
    Terrain { object: String, refusal: TerrainRefusal },
    /// A path resource refused.
    Path { object: String, refusal: crate::scene_paths::PathRefusal },
    /// A light's gradient reference names no C3dBaseGradient.
    NoSuchGradient { light: String, identity: u32 },
    /// An owner's track slot names no resource of its class.
    NoSuchTrack { owner: String, class: &'static str, identity: u32 },
    /// A track resource refused.
    Track { class: &'static str, identity: u32, refusal: TrackRefusal },
    /// The object track list refused.
    TrackList(TrackListRefusal),
    /// More than one object track list.
    TrackLists { count: usize },
}

impl fmt::Display for SceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use SceneError::*;
        match self {
            Container(e) => write!(f, "{e}"),
            Index(e) => write!(f, "{e}"),
            Record { class, name, error } => write!(f, "{error} (resource \"{name}\" of class {class})"),
            RequiredCount { class, count } => {
                write!(f, "damaged document: it holds {count} resources of class {class}, not exactly one")
            }
            NoDataEntry { class, identity, entry } => {
                write!(f, "damaged document: resource {identity} of class {class} names entry {entry}, which is not in the directory")
            }
            ShortData { class, name, len, needed } => {
                write!(f, "damaged document: resource \"{name}\" of class {class} has {len} bytes, fewer than its {needed}")
            }
            NoSuchObject { row, class, identity } => {
                write!(f, "damaged document: object list row {row} names {class} {identity}, which the document lacks")
            }
            NoSuchLinked { list, row, class, identity } => {
                write!(f, "damaged document: row {row} of linked list {list} names {class} {identity}, which the document lacks")
            }
            NoSuchMaterial { object, identity } => {
                write!(f, "damaged document: object \"{object}\" refers to material {identity}, which the document lacks")
            }
            NoView => write!(f, "damaged document: no camera can be the view (none of the view names, no \"director\")"),
            Tree { object, refusal } => write!(f, "{refusal} (tree \"{object}\")"),
            Texture { referrer, refusal } => write!(f, "{refusal} ({referrer})"),
            Picture { name, identity, refusal } => write!(f, "{refusal} (picture \"{name}\", C3dCImage {identity})"),
            Mesh { object, refusal } => write!(f, "{refusal} (mesh \"{object}\")"),
            Terrain { object, refusal } => write!(f, "{refusal} (terrain \"{object}\")"),
            Path { object, refusal } => write!(f, "{refusal} (path \"{object}\")"),
            NoSuchGradient { light, identity } => write!(
                f,
                "damaged document: light \"{light}\" names gradient {identity}, which the document lacks"
            ),
            NoSuchTrack { owner, class, identity } => write!(
                f,
                "damaged document: \"{owner}\" names track {identity} of class {class}, which the document lacks"
            ),
            Track { class, identity, refusal } => write!(f, "{refusal} (track {identity} of class {class})"),
            TrackList(refusal) => write!(f, "{refusal}"),
            TrackLists { count } => write!(
                f,
                "not supported: {count} object track lists (SCN-22: a document holds none or one)"
            ),
        }
    }
}

impl std::error::Error for SceneError {}

impl From<ContainerError> for SceneError {
    fn from(e: ContainerError) -> Self {
        SceneError::Container(e)
    }
}

impl From<IndexError> for SceneError {
    fn from(e: IndexError) -> Self {
        SceneError::Index(e)
    }
}

/// A primitive shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Sphere,
    Cube,
    Cylinder,
    Cone,
    Plane,
    Torus { tube_radius: f64, ring_radius: f64 },
    Slab,
    Square,
    Pyramid,
    /// The Blender description's disc (the add-on's `bryce_shape` "disc"). No document
    /// class names it (has no disc), so the reader never builds one.
    Disc,
}

/// What an object is, beyond its placement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Body {
    Shape(Shape),
    /// Index into `Scene::lights`.
    RadialLight(usize),
    /// Index into `Scene::cameras`. A camera of a linked list (its object's `linked_to` set)
    /// is one too: its view takes its parent's chain (A camera linked to an object;
    /// A camera held to a path).
    Camera(usize),
    /// Index into `Scene::trees`.
    Tree(usize),
    /// Not an object of the document: the object a hit on a leaf of tree `Scene::trees[k]` is
    /// reported for, holding the tree's leaves' material. Added to the document a render
    /// opens, after its objects (`plan::opened`); never read from a file.
    /// . (The skins)
    TreeLeaves(usize),
    /// A height field: index into `Scene::terrains`.
    Terrain(usize),
    /// A triangle soup carried in `Scene::meshes` (the Blender description's mesh). No
    /// document builds one (a mesh's own part is Open).
    Mesh(usize),
    /// A group: its placement and its group part are read ; its members are the
    /// objects of its linked list , linked to it . It has no shape of its
    /// own that gives.
    Group,
    /// A path: its placement and its path part , index into `Scene::paths`.
    Path(usize),
    /// A document's mesh: its placement is read  and its mesh part is in
    /// `Scene::mesh_resources` at this index.. (Meshes)
    MeshResource(usize),
    /// A metaball: its placement is read  and its sign (`negative`: the byte at offset 4
    /// of the metaball part is 1).
    Metaball { negative: bool },
    /// A kind whose parts leaves Open: only its name is read.
    Unread,
}

/// One object of the root list, or of a linked list (after the root list's objects, each
/// root list object's lists depth first).
#[derive(Clone, Debug, PartialEq)]
pub struct SceneObject {
    pub name: Vec<u8>,
    pub kind: ObjectKind,
    pub identity: u32,
    /// The identity of its C3dObjList; 0 for none.
    pub linked_list: u32,
    /// For an object of a linked list, its parent: the object (index into
    /// `Scene::objects`, always an earlier one) whose row names the list that holds it;
    /// `None` in the root list.
    pub linked_to: Option<usize>,
    pub placement: Option<PlacementPart>,
    /// The object-to-world transform where every version gives the same one:'s for
    /// an object of the root list,'s for a linked one. `None` for a linked object
    /// whose versions differ (see `linked_worlds`, `world_as`) or whose world is not given
    /// (`world_open`).
    pub world: Option<Transform>,
    /// A linked object's own transform as 3.0, 3.1 and 5.0 make it.
    pub linked_worlds: Option<[Transform; 3]>,
    /// Why a linked object with a placement has no world: what it is linked to is Open.
    pub world_open: Option<crate::animation::AnimError>,
    /// The worlds (as 3.0, 3.1 and 5.0) the structure lists the object by where they are not its
    /// own: as 5.0 a held object none of whose tracks has a key is placed, but its box stays the
    /// box of the place the document stored (A held object with no key, as 5.0); so
    /// do the objects linked below it. (Held objects)
    pub listing_worlds: Option<[Transform; 3]>,
    /// Index into `Scene::materials`.
    pub material: Option<usize>,
    pub body: Body,
}

/// A material.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub identity: u32,
    pub name: Vec<u8>,
    pub part: MaterialPart,
    /// The textures its channels' references name.
    pub textures: ChannelTextures,
    /// The second material a channel track makes between two keys that name different textures
    /// (set only by opening a document at a time).,
    pub second: SecondChannels,
}

/// A channel's second channel: key i + 1's whole channel as it is, the textures its references
/// name (indices into `Scene::textures`) and the mix amount m.
#[derive(Clone, Debug, PartialEq)]
pub struct SecondChannel<T> {
    pub channel: crate::scene_parts::Channel<T>,
    pub textures: [Option<usize>; 3],
    pub amount: f64,
}

/// A material's second channels, in the part's channel order: none for a channel without one
/// (m = 0).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SecondChannels {
    pub colours: [Option<SecondChannel<crate::scene_parts::Rgb>>; 5],
    pub values: [Option<SecondChannel<f32>>; 8],
}

/// The textures a material's channels name, in the part's channel order: for each channel
/// its references A, B and alpha, each an index into `Scene::textures`, `None` for none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChannelTextures {
    /// Diffuse to Transparent colour.
    pub colours: [[Option<usize>; 3]; 5],
    /// Diffusion to Volume density.
    pub values: [[Option<usize>; 3]; 8],
}

impl ChannelTextures {
    pub fn any(&self) -> bool {
        self.colours.iter().chain(&self.values).flatten().any(Option::is_some)
    }
}

/// The textures of the sky's two cloud layers, indices into `Scene::textures`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CloudTextures {
    pub stratus: Option<usize>,
    pub cumulus: Option<usize>,
}

/// The kinds of light of the root list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightKind {
    Radial,
    /// C3dConeLight:'s cone.
    Spot,
    /// C3dCubeLight:'s box.
    Parallel,
    /// C3dCylLight:'s cylinder.
    Cylinder,
    /// C3dPyrLightType:'s pyramid.
    Square,
}

/// A light of the root list: a radial light, or a spot or parallel light (`beam`), whose
/// other fields reads as a radial light's.
/// (What is read)
#[derive(Clone, Debug, PartialEq)]
pub struct RadialLight {
    /// Index into `Scene::objects`.
    pub object: usize,
    /// The light part's own colour.
    pub own_colour: Rgb,
    /// The colour as the light shines. (own colour times the material's
    /// Diffuse A, binary32). With no material, the own colour (point 4).
    /// With a gel, the own colour as a stand-in only: see `gel`.
    pub colour: Rgb,
    /// The material's index when it has texture references (a gel). When set, `colour`
    /// is not the light's colour as it shines, which is Open (point 4,
    /// component shading).
    pub gel: Option<usize>,
    pub intensity: f64,
    pub position: Vec3,
    pub switches: u32,
    pub part: LightPart,
    /// The beam record of a spot or parallel light; `None` for a radial light.
    pub beam: Option<BeamRecord>,
    /// A spot light's falloff track's value at the opening time, where the opening applies
    /// tracks ; `None` otherwise. A render takes it in place of the stored exponent
    /// . (Opening)
    pub falloff_opened: Option<f64>,
    /// The gradient the light part names (C3dBaseGradient), read; `None` where it
    /// names none. What a render does with it is's (as 5.0 with switch bit 0x200000).
    pub gradient: Option<GradientPart>,
}

/// The material a beam light (spot, parallel and the rest) is given when its object names none:
/// the material of the latest earlier object of the list that has one. and settle
/// the radial light only: one whose object names no material shines its own colour unchanged,
/// in every version and with every switch word, so for it this gives none. A beam light's case
/// is Open (the implementer's choice) and keeps the older rule.
/// . (A light with no material)
pub(crate) fn unmaterialled_light_material(beam: bool, earlier: &[SceneObject]) -> Option<usize> {
    if !beam {
        return None;
    }
    earlier.iter().rev().find_map(|o| o.material)
}

impl RadialLight {
    /// The light's kind.
    pub fn kind(&self) -> LightKind {
        match self.beam {
            None => LightKind::Radial,
            Some(BeamRecord::Spot { .. }) => LightKind::Spot,
            Some(BeamRecord::Parallel { .. }) => LightKind::Parallel,
            Some(BeamRecord::Cylinder { .. }) => LightKind::Cylinder,
            Some(BeamRecord::Square { .. }) => LightKind::Square,
        }
    }
}

/// The sun.
#[derive(Clone, Debug, PartialEq)]
pub struct Sun {
    pub name: Vec<u8>,
    pub colour: Rgb,
    pub glow_colour: Rgb,
    pub direction: Vec3,
    pub intensity: f64,
    pub switches: u32,
    /// The light part's own colour (the sun's colour is the sun part's).
    pub light_colour: Rgb,
    pub placement: PlacementPart,
    pub light: LightPart,
    pub sun: SunPart,
}

/// A light, in's order.
#[derive(Clone, Debug, PartialEq)]
pub enum Light {
    Radial(RadialLight),
    Sun(Sun),
}

/// A camera of the root list or of a linked list.
#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    pub name: Vec<u8>,
    /// Index into `Scene::objects`.
    pub object: usize,
    pub placement: PlacementPart,
    pub part: CameraPart,
    /// The placement's second flag word, as stored.
    pub projection_word: u32,
}

/// The projection word's bits the saved views' change touches.,
const PROJECTION_PERSPECTIVE: u32 = 0x4000;
const PROJECTION_PARALLEL: u32 = 0x8000;
const PROJECTION_PANORAMA: u32 = 0x1_0000;
/// The bit a "dot" camera's projection word needs to be read as a saved view.
const PROJECTION_SAVED_DOT: u32 = 0x8_0000;
/// The scale a saved view takes as 3.1 and 5.0.
const SAVED_VIEW_SCALE: f64 = 5_200_000.0;

impl Camera {
    /// Whether `version`'s reader treats this camera as a saved view: as 3.1 and 5.0 a camera
    /// named "top", "right", "front", "left", "back" or "bottom", or one whose name begins with
    /// "dot", whose projection word has bit 0x80000 and whose word at offset 36 is 1 to 6; as
    /// 3.0 none.
    pub fn saved_view_as(&self, version: RenderAs) -> bool {
        version != RenderAs::Bryce30
            && (AXIS_VIEWS.contains(&self.name.as_slice())
                || (self.name.starts_with(b"dot")
                    && self.projection_word & PROJECTION_SAVED_DOT != 0
                    && (1..=6).contains(&self.part.saved_view_word)))
    }

    /// The projection word and scale as `version` reads the camera: a saved view's word with
    /// bits 0x4000 and 0x10000 cleared and 0x8000 set (parallel) and its scale 5,200,000; any
    /// other camera's as stored.
    pub fn projection_as(&self, version: RenderAs) -> (u32, f64) {
        if self.saved_view_as(version) {
            ((self.projection_word & !(PROJECTION_PERSPECTIVE | PROJECTION_PANORAMA)) | PROJECTION_PARALLEL, SAVED_VIEW_SCALE)
        } else {
            (self.projection_word, self.part.scale)
        }
    }
}

/// A tree object, of the root list or of a linked list.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeObject {
    /// Index into `Scene::objects`.
    pub object: usize,
    pub name: Vec<u8>,
    /// Bryce 5's placement part (version 2), its first 376 bytes read as's.
    pub placement: PlacementPart,
    /// The object-to-world transform: carries the tree's own space into the world.
    pub world: Transform,
    /// The wood's material, the placement's (index into `Scene::materials`).
    pub wood_material: Option<usize>,
    /// The leaves' material (index into `Scene::materials`); `None` where the reference's type
    /// code is 0.
    pub leaves_material: Option<usize>,
    /// The stored tree it shows (index into `Scene::stored_trees`).
    pub stored_tree: usize,
}

/// A terrain object, of the root list or of a linked list. Its placement, world and
/// material are its `Scene::objects` entry's.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainObject {
    /// Index into `Scene::objects`.
    pub object: usize,
    pub name: Vec<u8>,
    /// The own part and the grid .
    pub part: TerrainPart,
}

/// The view a picture is seen through.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// Index into `Scene::cameras`.
    pub camera: usize,
    /// The camera's axes as rows and the image plane centre as translation, as each
    /// version opens the document, or why it is not given.
    pub transforms: ViewTransforms,
}

impl View {
    /// The view's transform as `version` opens the document.
    pub fn transform(&self, version: RenderAs) -> ViewTransform {
        self.transforms.get(version)
    }
}

/// A scene document, read.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub settings: DocSettings,
    /// The root list's objects in order, cameras and lights included.
    pub objects: Vec<SceneObject>,
    /// Every C3dBaseMaterial, in class order.
    pub materials: Vec<Material>,
    /// Radial lights in list order, then the sun.
    pub lights: Vec<Light>,
    /// The cameras in object order: the root list's in list order, then those of the linked
    /// lists.
    pub cameras: Vec<Camera>,
    /// The path objects of every list, in object order.
    pub paths: Vec<crate::scene_paths::PathObject>,
    pub view: View,
    pub sky: Sky,
    /// The root list's trees in list order.
    pub trees: Vec<TreeObject>,
    /// The document's stored trees, in the order held (all in its first tree).
    pub stored_trees: Vec<StoredTree>,
    /// The terrains of every list, in object order.
    pub terrains: Vec<TerrainObject>,
    /// Every texture resource a reference names, once each, in the order first named:
    /// the materials' channels in class and channel order, then the sky's layers.
    pub textures: Vec<Texture>,
    /// Every picture a picture texture names, once each, in the order first named.
    pub pictures: Vec<HeldPicture>,
    /// The sky's cloud layers' textures.
    pub clouds: CloudTextures,
    /// The meshes `Body::Mesh` indexes: the Blender description's triangle soups. Empty
    /// for every document.
    pub meshes: Vec<Mesh>,
    /// The document's mesh objects' mesh parts, in the objects' order (`Body::MeshResource`
    /// indexes it).
    pub mesh_resources: Vec<MeshResource>,
    /// The form of the document's parts: Bryce 5's when any placement part is of version 2.
    pub form: PartsForm,
    /// The animation tracks the owners' slots name, read and bound..
    /// (Bryce 5's documents: the tracks)
    pub animation: SceneTracks,
    /// The object track list as read, `None` where the document holds none.
    pub track_list: Option<TrackList>,
    /// Its rows resolved to `objects` indices, in the list's order.
    pub tracking: Vec<TrackRow>,
    /// The time a frame asks for (`Scene::at_time`), `None` for the document as it opens at its
    /// saved time.
    pub frame_time: Option<f64>,
    /// The readings 3.0 and 3.1 make where they differ from this one; `None` where they are
    /// this one.. (Readings as
    /// 3.0 and 3.1)
    pub older: Option<Box<Older>>,
}

/// The document as 3.0 and 3.1 read it where that is not as 5.0 does.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Older {
    /// As 3.1, a document with a placement part of version 2 or a light part of version 2 or 3
    /// (`Err`: the reading refuses, with the reason).
    pub three_one: Option<Result<Box<Scene>, String>>,
    /// As 3.0, a Bryce 3D document whose texture records are of version 2.
    /// (`Err`: the reading refuses, with the reason).
    pub three_zero: Option<Result<Box<Scene>, String>>,
}

impl Scene {
    /// What opening reads of the document , with the time a frame asks where this copy
    /// is one .,
    pub fn opening_inputs(&self) -> crate::animation::OpeningInputs {
        let inputs = self.settings.opening_inputs(self.form);
        match self.frame_time {
            Some(t) => inputs.at_time(t),
            None => inputs,
        }
    }

    /// The document as a frame asks for it at the time `t` (seconds, binary64): a copy whose
    /// opening (`opened`) applies the tracks at `t` as says, on top of the values the
    /// records hold.
    pub fn at_time(&self, t: f64) -> Scene {
        Scene { frame_time: Some(t), ..self.clone() }
    }

    /// The view's camera.
    pub fn view_camera(&self) -> &Camera {
        &self.cameras[self.view.camera]
    }

    /// The sun (always the last light).
    pub fn sun(&self) -> &Sun {
        match self.lights.last() {
            Some(Light::Sun(s)) => s,
            _ => unreachable!("the reader always ends the lights with the sun"),
        }
    }

    /// A radial light's place as `version` opens the document. As 3.0 and 3.1 it is the
    /// translation of the light's second transform carried through the objects it is linked to
    /// (its chain), so that a translation in its first transform does not move it; as
    /// 5.0 the translation of its object-to-world transform. The two are the same for a light
    /// whose first transform has no translation (all 329 radial, spot and parallel lights of the
    /// documents observed); a linked light's differs between versions where bits of 0xf80000
    /// are on its chain. The beam is carried by the whole transform.
    pub fn light_position(&self, light: &RadialLight, version: RenderAs) -> Vec3 {
        let object = &self.objects[light.object];
        if version != RenderAs::Bryce50
            && let Some(p) = &object.placement
            && p.first.translation != [0.0; 3]
        {
            return chain_as(&self.objects, light.object, version).map_or(p.second.translation, |t| t.translation);
        }
        object.world_as(version).map_or(light.position, |w| w.translation)
    }
}

impl Scene {
    /// The scene's order of the objects as `version` forms it: every list of the scene's objects
    /// follows it (the walk of component geometry,, takes ties in it). As 3.0 each object
    /// comes before the objects linked to it (a root row, then the first row of its linked list
    /// with what is linked to that row, then the second, and so on, then the next root row); as
    /// 3.1 and 5.0 each comes after them (the rows of its linked list, each with what is linked
    /// to it, then the object). Indices into `objects`.
    pub fn order_as(&self, version: RenderAs) -> Vec<usize> {
        let n = self.objects.len();
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut roots = Vec::new();
        for (i, o) in self.objects.iter().enumerate() {
            match o.linked_to {
                Some(p) => children[p].push(i),
                None => roots.push(i),
            }
        }
        let before = version == RenderAs::Bryce30;
        let mut out = Vec::with_capacity(n);
        // Each frame: the object and how many of its children are done.
        let mut stack: Vec<(usize, usize)> = Vec::new();
        for r in roots {
            if before {
                out.push(r);
            }
            stack.push((r, 0));
            while let Some(top) = stack.last_mut() {
                let (i, k) = *top;
                if let Some(&c) = children[i].get(k) {
                    top.1 += 1;
                    if before {
                        out.push(c);
                    }
                    stack.push((c, 0));
                } else {
                    if !before {
                        out.push(i);
                    }
                    stack.pop();
                }
            }
        }
        out
    }

    /// Whether the row of object `i` is one the reader skips as 3.1 and 5.0: its identity is 1
    /// (the root list's own), or is the identity of the list it stands in or of a list that
    /// encloses that one; nothing is made for it and nothing is linked to it, so the objects
    /// below a skipped row are skipped too. As 3.0 such a row is read like any other.
    pub fn row_skipped_as_new(&self, i: usize) -> bool {
        let mut at = Some(i);
        while let Some(j) = at {
            let o = &self.objects[j];
            let mut up = o.linked_to;
            if o.identity == 1 {
                return true;
            }
            while let Some(a) = up {
                if self.objects[a].linked_list == o.identity {
                    return true;
                }
                up = self.objects[a].linked_to;
            }
            at = o.linked_to;
        }
        false
    }

    /// Whether a list holds, at any depth, a row whose linked list is that list again (a group
    /// listed among its own members): as 3.0 the original opens nothing; as 3.1 and 5.0 the row
    /// is skipped (`row_skipped_as_new`).
    pub fn lists_itself(&self) -> bool {
        self.objects.iter().any(|o| {
            let mut up = o.linked_to;
            o.linked_list != 0
                && std::iter::from_fn(|| {
                    let a = up?;
                    up = self.objects[a].linked_to;
                    Some(self.objects[a].linked_list)
                })
                .any(|l| l == o.linked_list)
        })
    }
}

impl SceneObject {
    /// The world the structure lists the object by, where it is not `world_as`'s.
    pub fn listing_world_as(&self, version: RenderAs) -> Option<Transform> {
        self.listing_worlds.map(|w| w[version_slot(version)])
    }

    /// The object's own transform as `version` makes it.
    pub fn world_as(&self, version: RenderAs) -> Option<Transform> {
        match &self.linked_worlds {
            Some(w) => Some(w[version_slot(version)]),
            None => self.world,
        }
    }
}

/// The index of a version in `SceneObject::linked_worlds`.
fn version_slot(version: RenderAs) -> usize {
    match version {
        RenderAs::Bryce30 => 0,
        RenderAs::Bryce31 => 1,
        RenderAs::Bryce50 => 2,
    }
}

/// The versions in `SceneObject::linked_worlds`'s order.
const VERSIONS: [RenderAs; 3] = [RenderAs::Bryce30, RenderAs::Bryce31, RenderAs::Bryce50];

/// A linked list being walked: its identity, its rows, the next row, the parent of its
/// objects. (the order of linked objects)
type ListWalk = (u32, Vec<ObjectRow>, usize, usize);

/// The objects, lights, cameras and trees, as the lists are read. scene-files
struct Built {
    objects: Vec<SceneObject>,
    lights: Vec<Light>,
    cameras: Vec<Camera>,
    trees: Vec<TreeObject>,
    mesh_resources: Vec<MeshResource>,
    terrains: Vec<TerrainObject>,
    paths: Vec<crate::scene_paths::PathObject>,
    /// A torus's object index and its part's two track identities .
    torus_tracks: Vec<(usize, [u32; 2])>,
}

/// Every linked object's own transform, per version, from its parents' chains: an
/// object of the root list's chain is its second transform; a linked object's chain is its
/// second followed by (what it takes of) its parent's. Parents come before the objects
/// linked to them. The rule is the same whatever the parent's class: a shape, a group (a
/// group's members are linked to it), a mesh, and a height field (no document holds
/// one; the implementer's choice, the same rule), and a path (a held object's second
/// transform is made on opening, `scene_opening`). A linked object gets no world where it
/// is linked to a path that lies below another object or has bits of 0xf80000 itself
/// (Open), it is itself a camera (its view: `linked_view`), or some parent's
/// placement is not read (a blob, a cone or cube light: Open); a linked tree there
/// keeps its own product as a stand-in (trees are not drawn).
pub(crate) fn linked_worlds(objects: &mut [SceneObject], lights: &mut [Light], trees: &mut [TreeObject]) {
    use crate::animation::{AnimError, LinkArithmetic};
    let arithmetic = VERSIONS.map(LinkArithmetic::of);
    let mut chains: Vec<Result<[Transform; 3], AnimError>> = Vec::with_capacity(objects.len());
    struct Links<'a> {
        objects: &'a mut [SceneObject],
        lights: &'a mut [Light],
        trees: &'a mut [TreeObject],
    }
    let b = Links { objects, lights, trees };
    for i in 0..b.objects.len() {
        let o = &b.objects[i];
        let chain = match (o.linked_to, &o.placement) {
            (None, Some(p)) => Ok([p.second; 3]),
            (_, None) => Err(AnimError::Unsupported(
                "an object linked to an object whose parts are Open (SCN-5; AN-1.9 needs its second transform)",
            )),
            (Some(parent), Some(p)) => match (&chains[parent], b.objects[parent].kind) {
                (Err(e), _) => Err(e.clone()),
                (Ok(pc), _) => {
                    let mut own = [p.world(); 3];
                    let mut chain = [p.second; 3];
                    for k in 0..3 {
                        let (w, c) = arithmetic[k].linked(&p.first, &p.second, p.first_flags, &pc[k], VERSIONS[k]);
                        own[k] = w;
                        chain[k] = c;
                    }
                    let o = &mut b.objects[i];
                    {
                        // (a camera linked below an object or a camera: its chain
                        // is formed as for any object; its view is `linked_view`)
                        let agree = own.iter().all(|w| w == &own[0]);
                        o.world = if agree { Some(own[0]) } else { None };
                        o.linked_worlds = Some(own);
                        match o.body {
                            Body::RadialLight(l) => {
                                if let Light::Radial(r) = &mut b.lights[l] {
                                    r.position = own[0].translation;
                                }
                            }
                            Body::Tree(t) if agree => b.trees[t].world = own[0],
                            _ => {}
                        }
                    }
                    Ok(chain)
                }
            },
        };
        let o = &mut b.objects[i];
        if let Err(e) = &chain
            && o.linked_to.is_some()
            && o.placement.is_some()
            && o.world_open.is_none()
        {
            o.world_open = Some(e.clone());
        }
        chains.push(chain);
    }
}

/// An object's chain  as `version` forms it: its second transform followed by (what
/// it takes of) its parent's chain; an object of the root list's is its second transform.
/// An error where a placement is not read or what it takes is Open.
/// . (Cameras linked to objects)
pub(crate) fn chain_as(objects: &[SceneObject], i: usize, version: RenderAs) -> Result<Transform, crate::animation::AnimError> {
    use crate::animation::{AnimError, LinkArithmetic};
    let o = &objects[i];
    let p = o.placement.as_ref().ok_or(AnimError::Unsupported(
        "an object linked to an object whose parts are Open (SCN-5; AN-1.9 needs its second transform)",
    ))?;
    if let Some(e) = &o.world_open {
        return Err(e.clone());
    }
    match o.linked_to {
        None => Ok(p.second),
        Some(parent) => {
            let pc = chain_as(objects, parent, version)?;
            Ok(LinkArithmetic::of(version).chain(&p.second, p.first_flags, &pc, version))
        }
    }
}

/// The view's transforms where its camera lies in a linked list: the camera's own transform
/// as makes it (the view as `View::choose` or the opening made it) followed by its
/// parent's chain , per version. Open: a camera of another name than "perspective",
/// a camera with bits of 0xf80000, a camera linked below a camera, a parent whose chain is
/// Open, and a document with a "perspective" camera in the root list and another in a linked
/// list. A camera of the root list: the transforms as they are.
/// (A camera linked to an object; A camera held to a path: the view is the
/// camera's own transform followed by the path's second transform),
pub(crate) fn linked_view(objects: &[SceneObject], cameras: &[Camera], view: &View, opening: OpeningInputs) -> ViewTransforms {
    use crate::animation::LinkArithmetic;
    let cam = &cameras[view.camera];
    let o = &objects[cam.object];
    let Some(parent) = o.linked_to else { return view.transforms };
    let open_of = |t: ViewTransform| match t {
        ViewTransform::Open(v) => v,
        t => OpenView { stand_in: t.given().expect("a given transform") },
    };
    // The camera's own transform in its two pieces (the first transform as makes it with
    // the roll, the second with the rows of the angles where the version makes them), for the
    // version whose view `view` holds; the pieces whose product is that view.
    let pieces = |version: RenderAs, t: &Transform| {
        [true, false].into_iter().find_map(|perspective| {
            let (f, s) = crate::scene_view::view_pieces(
                &cam.placement.first,
                &cam.placement.second,
                cam.part.angles,
                cam.part.roll,
                perspective,
                opening,
            )[version_slot(version)];
            (crate::transform_parts::product_h(&f, &s) == *t).then_some((f, s))
        })
    };
    ViewTransforms(VERSIONS.map(|version| {
        let t = view.transforms.get(version);
        let chain = chain_as(objects, parent, version);
        // : as 3.1 and 5.0 every camera has bit
        // 0x100000 set in its first flag word as it is read; its own bits act as for any object.
        let flags = cam.placement.first_flags | if version == RenderAs::Bryce30 { 0 } else { 0x0010_0000 };
        match (t.given(), chain) {
            (Some(own), Ok(c)) => {
                let arithmetic = LinkArithmetic::of(version);
                let made = match pieces(version, &own) {
                    // the first transform, then the second followed by what it takes of the chain
                    Some((f, s)) => f.followed_by(&arithmetic.chain(&s, flags, &c, version)),
                    // the product only (a view that tracks made): the whole chain
                    None => own.followed_by(&c),
                };
                match t {
                    ViewTransform::Chosen(_) => ViewTransform::Chosen(made),
                    _ => ViewTransform::Stated(made),
                }
            }
            (_, c) => {
                let mut v = open_of(t);
                if let Ok(c) = c {
                    v.stand_in = v.stand_in.followed_by(&c);
                }
                ViewTransform::Open(v)
            }
        }
    }))
}

/// The resources of the classes with one tag, in class order.
pub(crate) struct Doc<'a> {
    pub(crate) container: &'a Container,
    pub(crate) index: &'a ResourceIndex,
    pub(crate) reading: Reading,
}

/// Which program's reading of the document is made.,
/// (Readings as 3.0 and 3.1)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    /// The reading every render as 5.0 takes, each part by its own version.
    Normal,
    /// As 3.1: a placement part of version 2 is read as 376 bytes of version 1's layout (the
    /// parts after it from the bytes after those 376), and a light part of version 2 or 3 as
    /// 192 bytes of version 1's (Errors).
    ThreeOne,
    /// As 3.0: a texture's record part is read as version 1's whatever its version word
    /// (Variants).
    ThreeZero,
}

/// A resource with its data.
pub(crate) struct Data<'a> {
    pub(crate) reading: Reading,
    pub(crate) class: &'static str,
    pub(crate) res: &'a Resource,
    pub(crate) bytes: &'a [u8],
}

impl<'a> Doc<'a> {
    /// Whether the document holds a track (a resource of a class whose tag holds an
    /// underscore, as every track class's does) with `identity`.
    /// (a sky part of version 3 to 7 read as 3.0)
    fn has_track(&self, identity: u32) -> bool {
        self.index.classes.iter().any(|c| c.tag.contains(&b'_') && c.resources.iter().any(|r| r.identity == identity))
    }

    /// Whether the document holds the texture a reference names (its code a texture's, a
    /// resource of that class with the identity). as `has_track`
    fn has_texture(&self, r: Option<TextureRef>) -> bool {
        let Some(r) = r else { return false };
        let Some(class) = TextureClass::of_code(r.code) else { return false };
        self.resources(class.tag()).any(|res| res.identity == r.identity)
    }

    /// Every class with the tag, taken together in class order, for lookup and for's
    /// count. point 3 (the implementer's choice; consistent with)
    pub(crate) fn resources(&self, tag: &'static str) -> impl Iterator<Item = &'a Resource> + 'a {
        let tag = tag.as_bytes();
        self.index.classes.iter().filter(move |c| c.tag == tag).flat_map(|c| c.resources.iter())
    }

    pub(crate) fn data(&self, class: &'static str, res: &'a Resource) -> Result<Data<'a>, SceneError> {
        let n = if class == DOCSET { res.entries[1] } else { res.entries[0] };
        let bytes = self.container.entry(n as usize).ok_or_else(|| SceneError::NoDataEntry {
            class: class.to_string(),
            identity: res.identity,
            entry: n,
        })?;
        Ok(Data { reading: self.reading, class, res, bytes })
    }

    /// The resource of a "one, required" class the document is read from: the first in the class
    /// table's row order that `take` accepts; a document with none such is refused, saying why.
    /// ("Missing and doubled classes": the first row of identity 0 for the
    /// settings, of identity 1 for the root list)
    fn the_first(&self, class: &'static str, take: impl Fn(&Resource) -> bool) -> Result<Data<'a>, SceneError> {
        match self.resources(class).find(|r| take(r)) {
            Some(res) => self.data(class, res),
            None => Err(SceneError::RequiredCount { class, count: self.resources(class).count() }),
        }
    }

    /// The sky or the sun: the first resource of the class in row order whose name, in the first
    /// part of its data, is `name`; with none so named, the first of the class (the reader's
    /// freedom,"Freedom (the names)"); with none at all, refused.
    fn the_named(&self, class: &'static str, name: &[u8]) -> Result<Data<'a>, SceneError> {
        let mut first = None;
        for res in self.resources(class) {
            let d = self.data(class, res)?;
            if first_part_name(d.bytes).is_ok_and(|n| n == name) {
                return Ok(d);
            }
            first.get_or_insert(d);
        }
        first.ok_or(SceneError::RequiredCount { class, count: 0 })
    }

    fn find(&self, row: usize, class: &'static str, identity: u32) -> Result<Data<'a>, SceneError> {
        let res = self
            .resources(class)
            .find(|r| r.identity == identity)
            .ok_or(SceneError::NoSuchObject { row, class, identity })?;
        self.data(class, res)
    }
}

impl Data<'_> {
    fn name(&self) -> String {
        String::from_utf8_lossy(&self.res.name).into_owned()
    }

    fn record(&self, error: RecordError) -> SceneError {
        SceneError::Record { class: self.class.to_string(), name: self.name(), error }
    }

    /// The data after the first part, which must hold `needed` bytes in all..
    /// Bytes beyond the kind's parts are ignored (point 1, the implementer's
    /// choice).
    pub(crate) fn need(&self, needed: usize) -> Result<(), SceneError> {
        if self.bytes.len() < needed {
            return Err(SceneError::ShortData {
                class: self.class.to_string(),
                name: self.name(),
                len: self.bytes.len(),
                needed,
            });
        }
        Ok(())
    }
}

/// The first part and the placement part of an object, light or camera.
struct Placed<'a> {
    name: Vec<u8>,
    order: crate::scene_records::ByteOrder,
    placement: PlacementPart,
    /// The data after the placement.
    rest: &'a [u8],
    /// The placement's length as the reading takes it (376 or 1,024).
    len: usize,
}

/// The first part and the placement part of either version, told by its first word: Bryce
/// 3D's version 1 (376 bytes) or Bryce 5's version 2 (1,024 bytes, its first 376 laid out as
/// version 1's). Another version is refused , before the length is checked
/// . Every class with a placement reads both (Interface; a group's data
/// 644 or 1,292 bytes).
/// . (Bryce 5's documents: the placement part)
fn placed<'a>(d: &Data<'a>) -> Result<Placed<'a>, SceneError> {
    d.need(FIRST_PART_LEN + 4)?;
    let order = order_of(d.bytes);
    let name = first_part_name(d.bytes).map_err(|e| d.record(e))?;
    let after = &d.bytes[FIRST_PART_LEN..];
    let version = crate::scene_records::word(after, 0, order);
    let len = PlacementPart::len_of_version(version)
        .ok_or_else(|| d.record(RecordError::UnknownVersion { record: "placement", version }))?;
    d.need(FIRST_PART_LEN + len)?;
    let placement = PlacementPart::parse_either(&after[..len], order).map_err(|e| d.record(e))?;
    // : as 3.1 neither program tests the version word of a placement part;
    // one of version 2 is read as 376 bytes of version 1's layout and the parts after it are
    // read from the bytes after those, inside the larger part.
    let len = if d.reading == Reading::ThreeOne && version == PLACEMENT_V2_VERSION { PLACEMENT_LEN } else { len };
    Ok(Placed { name, order, placement, rest: &after[len..], len })
}

/// The same as `placed`: groups, height fields, meshes and paths read either version too.
fn placed_either<'a>(d: &Data<'a>) -> Result<Placed<'a>, SceneError> {
    placed(d)
}

/// The metaball part's length, after the placement part.
const METABALL_PART_LEN: usize = 5;

/// The shape a kind names, for the kinds gives parts to.
fn plain_shape(kind: ObjectKind) -> Option<Shape> {
    Some(match kind {
        ObjectKind::Sphere => Shape::Sphere,
        ObjectKind::Cube => Shape::Cube,
        ObjectKind::Cylinder => Shape::Cylinder,
        ObjectKind::Cone => Shape::Cone,
        ObjectKind::Plane => Shape::Plane,
        ObjectKind::Slab => Shape::Slab,
        ObjectKind::Square => Shape::Square,
        ObjectKind::Pyramid => Shape::Pyramid,
        _ => return None,
    })
}

/// Each object's place in the walk of the root list in order with every object's linked
/// objects right after it (pre-order, the linked in list order). (A camera linked
/// to an object: of two cameras named perspective the view is the one met first),
/// (first in list order). (Cameras linked)
pub(crate) fn walk_ranks(objects: &[SceneObject]) -> Vec<usize> {
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); objects.len()];
    let mut roots = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        match o.linked_to {
            Some(p) => children[p].push(i),
            None => roots.push(i),
        }
    }
    let mut rank = vec![usize::MAX; objects.len()];
    let mut next = 0;
    let mut stack: Vec<usize> = roots.into_iter().rev().collect();
    while let Some(i) = stack.pop() {
        rank[i] = next;
        next += 1;
        stack.extend(children[i].iter().rev());
    }
    rank
}

impl View {
    /// Which camera is the view, and its transform., (points 5 and 6)
    ///
    /// `opening` is what opening reads of the document ('s saved time and switch byte,
    /// the form of its parts). Never refuses a document for its view's
    /// transform (an Open view is `ViewTransform::Open`); the only refusal
    /// is's, no camera that can be the view.
    pub fn choose(cameras: &[Camera], opening: OpeningInputs) -> Result<View, SceneError> {
        let ranks: Vec<usize> = (0..cameras.len()).collect();
        View::choose_ranked(cameras, &ranks, opening)
    }

    /// `choose` with each camera's place in the list given (`ranks`, per camera): "in list order"
    /// is the walk of the root list with each object's linked objects at its own place
    /// (`walk_ranks`)..
    /// (Cameras linked)
    pub fn choose_ranked(cameras: &[Camera], ranks: &[usize], opening: OpeningInputs) -> Result<View, SceneError> {
        let at = |i: usize| ranks[i];
        // For each of the eight names, the first camera (in the walk) that has it.
        let first_named = |n: &[u8]| (0..cameras.len()).filter(|&i| cameras[i].name.as_slice() == n).min_by_key(|&i| at(i));
        let firsts: Vec<usize> = VIEW_NAMES.iter().filter_map(|n| first_named(n)).collect();
        // More than one marked: the first in list order (point 5).
        let marked = firsts.iter().copied().filter(|&i| cameras[i].placement.is_camera_in_use()).min_by_key(|&i| at(i));
        let camera = match marked {
            Some(i) => i,
            None => first_named(b"director").ok_or(SceneError::NoView)?,
        };
        let c = &cameras[camera];
        // Only the first camera named "perspective" has its rows made from its angles; the
        // view is always the first camera of its name (above).
        let perspective = c.name.as_slice() == b"perspective";
        let transforms = view_transforms(
            &c.placement.first,
            &c.placement.second,
            c.part.angles,
            c.part.roll,
            perspective,
            opening,
        );
        Ok(View { camera, transforms })
    }
}

impl Scene {
    /// Read a document.. The reading is the one a render as 5.0 takes;
    /// where 3.0 or 3.1 reads a part of the document differently (Errors, Variants)
    /// that reading is kept in `older`, made here too (Readings as 3.0
    /// and 3.1).
    pub fn read(file: &[u8], flavour: Flavour) -> Result<Scene, SceneError> {
        let mut scene = Self::read_as(file, flavour, Reading::Normal)?;
        let bryce5_parts = scene.holds_bryce5_parts();
        let texture_v2 = scene.textures.iter().any(|t| t.procedural().is_some_and(|p| p.form == RecordForm::Single));
        let mut older = Older::default();
        if bryce5_parts {
            // as 3.0 the document is not opened (Errors)
            older.three_one = Some(Self::read_as(file, flavour, Reading::ThreeOne).map(Box::new).map_err(|e| e.to_string()));
        } else if texture_v2 {
            older.three_zero = Some(Self::read_as(file, flavour, Reading::ThreeZero).map(Box::new).map_err(|e| e.to_string()));
        }
        if older.three_one.is_some() || older.three_zero.is_some() {
            scene.older = Some(Box::new(older));
        }
        Ok(scene)
    }

    /// Whether a placement part is of Bryce 5's version 2 or a light part of its version 2 or 3:
    /// a document 3.0 and 3.1 read in their own way (Errors).
    pub fn holds_bryce5_parts(&self) -> bool {
        let sun = self.lights.iter().filter_map(|l| match l {
            Light::Sun(s) => Some((&s.placement, s.light.version)),
            _ => None,
        });
        let radial = self.lights.iter().filter_map(|l| match l {
            Light::Radial(r) => Some((self.objects[r.object].placement.as_ref()?, r.part.version)),
            _ => None,
        });
        self.objects.iter().filter_map(|o| o.placement.as_ref()).any(|p| p.version == PLACEMENT_V2_VERSION)
            || sun.chain(radial).any(|(p, v)| p.version == PLACEMENT_V2_VERSION || v != 1)
    }

    fn read_as(file: &[u8], flavour: Flavour, reading: Reading) -> Result<Scene, SceneError> {
        let container = Container::parse(file, flavour)?;
        let index = ResourceIndex::read(&container)?;
        let doc = Doc { container: &container, index: &index, reading };

        let set = doc.the_first(DOCSET, |r| r.identity == 0)?;
        let settings = DocSettings::parse(set.bytes).map_err(|e| set.record(e))?;

        // : every material, in class order.
        let mut materials = Vec::new();
        for res in doc.resources(MATERIAL) {
            // The textures are resolved once the whole document is read (below).
            materials.push(read_material(&doc, res)?);
        }
        let material_index = |name: &[u8], p: &PlacementPart| -> Result<Option<usize>, SceneError> {
            match p.material {
                None => Ok(None),
                Some(id) => materials.iter().position(|m| m.identity == id).map(Some).ok_or_else(|| {
                    SceneError::NoSuchMaterial { object: String::from_utf8_lossy(name).into_owned(), identity: id }
                }),
            }
        };

        let root = doc.the_first(ROOT_LIST, |r| r.identity == 1)?;
        let list = ObjectList::parse(root.bytes).map_err(|e| root.record(e))?;

        let mut stored_trees = Vec::new();
        // : the tree that holds the stored trees is the
        // first the program reads, in the order of the object lists' rows (a row's object, then
        // its linked list, before the next row); a tree in a linked list may be it (
        //). A tree resource no row names is not read.
        let tree_class = ObjectKind::Tree.class_tag();
        let holder: Option<u32> = first_tree_in_list_order(&doc, &list)?;
        if let Some(identity) = holder {
            let res = doc.resources(tree_class).find(|r| r.identity == identity).ok_or(SceneError::NoSuchLinked {
                list: 0,
                row: 0,
                class: tree_class,
                identity,
            })?;
            let d = doc.data(tree_class, res)?;
            let (_, _, part) = tree_parts(&d, true)?;
            stored_trees = part.stored_trees.unwrap_or_default();
        }
        let tree_of = |d: &Data<'_>, row: &ObjectRow, index: usize, linked_to: Option<usize>, number: usize| {
            let holds = holder == Some(row.identity);
            let (name, placement, part) = tree_parts(d, holds)?;
            let refuse = |refusal| SceneError::Tree { object: String::from_utf8_lossy(&name).into_owned(), refusal };
            // : a type code of 0 gives the leaves no material, any other
            // code is taken as `base`; the identity must name a material of the document
            let leaves_material = match part.leaves_material {
                None => None,
                Some(id) => Some(
                    materials
                        .iter()
                        .position(|m| m.identity == id)
                        .ok_or_else(|| refuse(TreeRefusal::NoSuchLeavesMaterial(id)))?,
                ),
            };
            let stored_tree = part.stored_tree as usize;
            if stored_tree >= stored_trees.len() {
                return Err(refuse(TreeRefusal::StoredTreeNumber { number: part.stored_tree, held: stored_trees.len() }));
            }
            let world = placement.world();
            let wood_material = material_index(&name, &placement)?;
            let object = SceneObject {
                name: name.clone(),
                kind: row.kind,
                identity: row.identity,
                linked_list: row.linked_list,
                linked_to,
                placement: Some(placement),
                world: Some(world),
                linked_worlds: None,
                world_open: None,
                listing_worlds: None,
                material: wood_material,
                body: Body::Tree(number),
            };
            let tree = TreeObject { object: index, name, placement, world, wood_material, leaves_material, stored_tree };
            Ok((object, tree))
        };
        // One row of a list, of the root list or of a linked list: its object, and the
        // light, camera or tree it is.,
        let read_row = |b: &mut Built, d: &Data<'_>, row: &ObjectRow, linked_to: Option<usize>| -> Result<(), SceneError> {
            let index = b.objects.len();
            if row.kind == ObjectKind::Tree {
                let (object, tree) = tree_of(d, row, index, linked_to, b.trees.len())?;
                b.trees.push(tree);
                b.objects.push(object);
                return Ok(());
            }
            let mut object = SceneObject {
                name: Vec::new(),
                kind: row.kind,
                identity: row.identity,
                linked_list: row.linked_list,
                linked_to,
                placement: None,
                world: None,
                linked_worlds: None,
                world_open: None,
                listing_worlds: None,
                material: None,
                body: Body::Unread,
            };
            // : a mesh's placement (either version), then its mesh part.
            if row.kind == ObjectKind::TriMesh {
                let pl = placed_either(d)?;
                let part = MeshResource::parse(pl.rest, pl.order, index).map_err(|r| match r {
                    MeshRefusal::Short { needed, .. } => SceneError::ShortData {
                        class: d.class.to_string(),
                        name: d.name(),
                        len: d.bytes.len(),
                        needed: usize::try_from(needed).unwrap_or(usize::MAX).saturating_add(d.bytes.len() - pl.rest.len()),
                    },
                    r => SceneError::Mesh { object: String::from_utf8_lossy(&pl.name).into_owned(), refusal: r },
                })?;
                object.body = Body::MeshResource(b.mesh_resources.len());
                b.mesh_resources.push(part);
                object.name = pl.name.clone();
                object.material = material_index(&pl.name, &pl.placement)?;
                object.placement = Some(pl.placement);
                object.world = if linked_to.is_none() { Some(pl.placement.world()) } else { None };
                b.objects.push(object);
                return Ok(());
            }
            // : a height field's placement, of either version , then its own
            // part and its heights. (Height fields)
            if row.kind == ObjectKind::HeightField {
                let pl = placed_either(d)?;
                let at = d.bytes.len() - pl.rest.len();
                d.need(at + crate::scene_height_fields::TERRAIN_PART_LEN)?;
                let part = TerrainPart::parse(d.bytes, at, pl.order).map_err(|refusal| SceneError::Terrain {
                    object: String::from_utf8_lossy(&pl.name).into_owned(),
                    refusal,
                })?;
                b.terrains.push(TerrainObject { object: index, name: pl.name.clone(), part });
                object.body = Body::Terrain(b.terrains.len() - 1);
                object.name = pl.name.clone();
                object.material = material_index(&pl.name, &pl.placement)?;
                object.placement = Some(pl.placement);
                object.world = if linked_to.is_none() { Some(pl.placement.world()) } else { None };
                b.objects.push(object);
                return Ok(());
            }
            // : a metaball's first part, its placement part (either version is
            // read, as for a group) and the metaball part of 5 bytes.
            if row.kind == ObjectKind::Blob {
                let pl = placed_either(d)?;
                let at = d.bytes.len() - pl.rest.len();
                d.need(at + METABALL_PART_LEN)?;
                let byte = pl.rest[4];
                // : any sign byte other than 0 makes the metaball negative
                object.body = Body::Metaball { negative: byte != 0 };
                object.name = pl.name.clone();
                object.material = material_index(&pl.name, &pl.placement)?;
                object.placement = Some(pl.placement);
                object.world = if linked_to.is_none() { Some(pl.placement.world()) } else { None };
                b.objects.push(object);
                return Ok(());
            }
            // : a group's parts, and a path's
            // placement, the class's own part after it Open.
            if matches!(row.kind, ObjectKind::Group | ObjectKind::Path) {
                // The group part's word is 1 in every group observed; another value is the
                // implementer's choice : it is not used, and a group whose data ends
                // with its placement part, of either version (640 or 1,288 bytes), is read
                // as the same group with the word (revised).
                let pl = placed_either(d)?;
                object.body = if row.kind == ObjectKind::Group {
                    Body::Group
                } else {
                    // : the path part at 640, after Bryce 3D's placement; in
                    // Bryce 5's form the path part is Open.
                    let refuse = |refusal| SceneError::Path { object: String::from_utf8_lossy(&pl.name).into_owned(), refusal };
                    // : Bryce 5's form holds the same part, at 1,288 behind its longer placement
                    let at = if pl.placement.version == 1 {
                        crate::scene_paths::PATH_PART_AT
                    } else {
                        crate::scene_paths::PATH_PART_AT_FIVE
                    };
                    let part = crate::scene_paths::PathPart::parse_at(d.bytes, pl.order, at).map_err(refuse)?;
                    b.paths.push(crate::scene_paths::PathObject { object: index, part });
                    Body::Path(b.paths.len() - 1)
                };
                object.name = pl.name.clone();
                object.material = material_index(&pl.name, &pl.placement)?;
                object.placement = Some(pl.placement);
                object.world = if linked_to.is_none() { Some(pl.placement.world()) } else { None };
                b.objects.push(object);
                return Ok(());
            }
            let has_placement =
                plain_shape(row.kind).is_some()
                    || matches!(
                        row.kind,
                        ObjectKind::Torus
                            | ObjectKind::SphereLight
                            | ObjectKind::ConeLight
                            | ObjectKind::CubeLight
                            | ObjectKind::CylinderLight
                            | ObjectKind::PyramidLight
                            | ObjectKind::Camera
                    );
            if !has_placement {
                // Parts Open : the first part only; data shorter than it is refused
                // and nothing further is (point 8).
                d.need(FIRST_PART_LEN)?;
                object.name = first_part_name(d.bytes).map_err(|e| d.record(e))?;
                b.objects.push(object);
                return Ok(());
            }
            let pl = placed(d)?;
            // A linked object's world is made below, once its parents are read .
            let world = pl.placement.world();
            object.name = pl.name.clone();
            object.material = material_index(&pl.name, &pl.placement)?;
            object.placement = Some(pl.placement);
            object.world = if linked_to.is_none() { Some(world) } else { None };
            object.body = match row.kind {
                ObjectKind::Torus => {
                    d.need(FIRST_PART_LEN + pl.len + TORUS_PART_LEN)?;
                    let t = TorusPart::parse(&pl.rest[..TORUS_PART_LEN], pl.order).map_err(|e| d.record(e))?;
                    b.torus_tracks.push((index, t.tracks));
                    Body::Shape(Shape::Torus { tube_radius: t.tube_radius, ring_radius: t.ring_radius })
                }
                ObjectKind::SphereLight
                | ObjectKind::ConeLight
                | ObjectKind::CubeLight
                | ObjectKind::CylinderLight
                | ObjectKind::PyramidLight => {
                    let lp = light_part(d, &pl)?;
                    // : the beam record after the light part; data shorter than
                    // the parts refused , longer data read (our choice: beam-lights.md)
                    let class = match row.kind {
                        ObjectKind::ConeLight => Some(BeamClass::Spot),
                        ObjectKind::CubeLight => Some(BeamClass::Parallel),
                        ObjectKind::CylinderLight => Some(BeamClass::Cylinder),
                        ObjectKind::PyramidLight => Some(BeamClass::Square),
                        _ => None,
                    };
                    let beam = match class {
                        None => None,
                        Some(class) => {
                            let at = LightPart::len_of_version(lp.version).expect("a light part read is of a known version");
                            d.need(FIRST_PART_LEN + pl.len + at + class.record_len())?;
                            Some(BeamRecord::parse(class, &pl.rest[at..], pl.order))
                        }
                    };
                    let (colour, gel) = match object.material.or_else(|| unmaterialled_light_material(beam.is_some(), &b.objects)) {
                        Some(m) if materials[m].part.is_plain() => {
                            let diffuse = materials[m].part.colours[0].a;
                            (std::array::from_fn(|k| lp.colour[k] * diffuse[k]), None)
                        }
                        // point 4 (a gel: Open; the own colour, marked)
                        Some(m) => (lp.colour, Some(m)),
                        // point 4 (no material even after the fallback: the own colour)
                        None => (lp.colour, None),
                    };
                    // A linked light's place is its world's translation as each version
                    // makes it (`Scene::light_position`); this one is 3.0's, set below.
                    b.lights.push(Light::Radial(RadialLight {
                        object: index,
                        own_colour: lp.colour,
                        colour,
                        gel,
                        intensity: lp.intensity,
                        position: world.translation,
                        switches: pl.placement.second_flags,
                        part: lp,
                        beam,
                        falloff_opened: None,
                        gradient: None, // read below, with the gradient references
                    }));
                    Body::RadialLight(b.lights.len() - 1)
                }
                // (a camera of a linked list is read as one of the root
                // list: its view takes its parent's chain, `linked_view`)
                ObjectKind::Camera => {
                    let at = FIRST_PART_LEN + pl.len;
                    d.need(at + 4)?;
                    d.need(at + CAMERA_PART_LEN)?;
                    let part = CameraPart::parse(&pl.rest[..CAMERA_PART_LEN], pl.order).map_err(|e| d.record(e))?;
                    b.cameras.push(Camera {
                        name: pl.name.clone(),
                        object: index,
                        placement: pl.placement,
                        part,
                        projection_word: pl.placement.second_flags,
                    });
                    Body::Camera(b.cameras.len() - 1)
                }
                k => Body::Shape(plain_shape(k).expect("a shape with a placement")),
            };
            b.objects.push(object);
            Ok(())
        };
        let mut b = Built { objects: Vec::with_capacity(list.rows.len()), lights: Vec::new(), cameras: Vec::new(), trees: Vec::new(), mesh_resources: Vec::new(), terrains: Vec::new(), paths: Vec::new(), torus_tracks: Vec::new() };
        for (row_no, row) in list.rows.iter().enumerate() {
            let d = doc.find(row_no, row.kind.class_tag(), row.identity)?;
            read_row(&mut b, &d, row, None)?;
        }

        // , (a
        // linked object's parent). Each root
        // list object's lists are walked depth first, every row followed by the objects
        // linked to it, after the root list's objects; each list once (a list named a second
        // time, or by an object it holds, is not walked again). (
        // the order of linked objects)
        let root_objects = b.objects.len();
        let mut seen: Vec<u32> = Vec::new();
        for owner in 0..root_objects {
            let mut stack: Vec<ListWalk> = Vec::new();
            let open_list = |id: u32, parent: usize, seen: &mut Vec<u32>| -> Result<Option<ListWalk>, SceneError> {
                if id == 0 || seen.contains(&id) {
                    return Ok(None);
                }
                seen.push(id);
                let res = doc
                    .resources(LINKED_LIST)
                    .find(|r| r.identity == id)
                    .ok_or(SceneError::NoSuchLinked { list: id, row: 0, class: LINKED_LIST, identity: id })?;
                let ld = doc.data(LINKED_LIST, res)?;
                let rows = ObjectList::parse(ld.bytes).map_err(|e| ld.record(e))?.rows;
                Ok(Some((id, rows, 0, parent)))
            };
            if let Some(top) = open_list(b.objects[owner].linked_list, owner, &mut seen)? {
                stack.push(top);
            }
            while let Some(top) = stack.last_mut() {
                let (id, row_no, parent) = (top.0, top.2, top.3);
                let Some(&row) = top.1.get(row_no) else {
                    stack.pop();
                    continue;
                };
                top.2 += 1;
                let class = row.kind.class_tag();
                let res = doc
                    .resources(class)
                    .find(|r| r.identity == row.identity)
                    .ok_or(SceneError::NoSuchLinked { list: id, row: row_no, class, identity: row.identity })?;
                let d = doc.data(class, res)?;
                let index = b.objects.len();
                read_row(&mut b, &d, &row, Some(parent))?;
                if let Some(next) = open_list(row.linked_list, index, &mut seen)? {
                    stack.push(next);
                }
            }
        }
        debug_assert!(b.objects[root_objects..].iter().all(|o| o.linked_to.is_some()));
        linked_worlds(&mut b.objects, &mut b.lights, &mut b.trees);
        let Built { objects, mut lights, cameras, trees, mesh_resources, terrains, paths, torus_tracks } = b;

        // : the sun, last.
        let sd = doc.the_named(SUN, b"sunlight")?;
        let pl = placed(&sd)?;
        material_index(&pl.name, &pl.placement)?;
        let light = light_part(&sd, &pl)?;
        let at = LightPart::len_of_version(light.version).expect("a light part read is of a known version");
        let before = FIRST_PART_LEN + pl.len + at;
        sd.need(before + 4)?;
        sd.need(before + SUN_PART_LEN)?;
        let sun = SunPart::parse(&pl.rest[at..at + SUN_PART_LEN], pl.order).map_err(|e| sd.record(e))?;
        lights.push(Light::Sun(Sun {
            name: pl.name,
            colour: sun.colour,
            glow_colour: sun.glow_colour,
            direction: sun.direction,
            intensity: light.intensity,
            switches: pl.placement.second_flags,
            light_colour: light.colour,
            placement: pl.placement,
            light,
            sun,
        }));

        // : the gradient each light names is read (its data refused where shorter than
        // the counts need), and kept with a radial light. A sun's is read for its errors only
        // ('s gradient is a radial light's or a light with a beam's).
        for l in &mut lights {
            let (part, name) = match &*l {
                Light::Radial(r) => (&r.part, &objects[r.object].name),
                Light::Sun(s) => (&s.light, &s.name),
            };
            let Some(id) = part.five.gradient_resource else { continue };
            let Some(res) = doc.resources(GRADIENT).find(|r| r.identity == id) else {
                return Err(SceneError::NoSuchGradient { light: String::from_utf8_lossy(name).into_owned(), identity: id });
            };
            let d = doc.data(GRADIENT, res)?;
            d.need(FIRST_PART_LEN + 4)?;
            let gradient = GradientPart::parse(&d.bytes[FIRST_PART_LEN..], order_of(d.bytes)).map_err(|e| d.record(e))?;
            if let Light::Radial(r) = l {
                r.gradient = Some(gradient);
            }
        }

        let sun_placement = match lights.last() {
            Some(Light::Sun(s)) => s.placement,
            _ => unreachable!("the sun was just pushed"),
        };
        let any_v2 = objects.iter().filter_map(|o| o.placement.as_ref()).chain([&sun_placement]).any(|p| p.version == PLACEMENT_V2_VERSION);
        let form = if any_v2 { PartsForm::Bryce5 } else { PartsForm::Bryce3d };

        let mut view = View::choose_ranked(
            &cameras,
            &{
                let rank = walk_ranks(&objects);
                cameras.iter().map(|c| rank[c.object]).collect::<Vec<_>>()
            },
            settings.opening_inputs(form),
        )?;
        view.transforms = linked_view(&objects, &cameras, &view, settings.opening_inputs(form));

        let kd = doc.the_named(SKY, b"current sky")?;
        kd.need(FIRST_PART_LEN + 4)?;
        let mut sky = Sky::parse(&kd.bytes[FIRST_PART_LEN..], order_of(kd.bytes)).map_err(|e| match e {
            RecordError::WrongLength { len, expected, .. } => SceneError::ShortData {
                class: SKY.to_string(),
                name: kd.name(),
                len: len + FIRST_PART_LEN,
                needed: expected + FIRST_PART_LEN,
            },
            e => kd.record(e),
        })?;
        // ,: a real part of version 3 to 7 read as 3.0 is
        // read at version 2's places; the words there of the cloud textures (228 to 243) and
        // the track identities (244 to 415) must name textures and tracks the document holds
        // or the document is not opened. Version 3's own reading is at those places already.
        if (3..=crate::scene_sky::SKY_VERSION_SEVEN).contains(&sky.version) {
            let (refs, ids): (&[Option<TextureRef>; 2], &[u32]) = match &sky.as_bryce30 {
                Some(b) => (&b.cloud_textures, &b.tracks),
                None => (&sky.cloud_textures, &sky.tracks),
            };
            sky.bryce30_unopened = !refs.iter().all(|r| doc.has_texture(*r)) || ids.iter().any(|&i| i != 0 && !doc.has_track(i));
        }
        let view_name = cameras[view.camera].name.as_slice();
        if AXIS_VIEWS.contains(&view_name) {
            sky.flags |= SKY_AXIS_VIEW;
        } else {
            sky.flags &= !SKY_AXIS_VIEW;
        }

        // : the textures the materials' channels and the sky's layers name, last, so
        // that every refusal of the parts above comes first, as before textures were read.
        let mut textures = Textures { doc: &doc, list: Vec::new(), pictures: Vec::new() };
        for m in materials.iter_mut() {
            textures.material(m)?;
        }
        let [stratus, cumulus] = sky.cloud_textures;
        let clouds = CloudTextures {
            stratus: textures.resolve(stratus, || "the sky's Stratus layer".to_string())?,
            cumulus: textures.resolve(cumulus, || "the sky's Cumulus layer".to_string())?,
        };

        // : the object track list (none or one), its rows resolved to the objects of
        // the lists.
        let lists: Vec<_> = doc.resources(TRACK_LIST).collect();
        if lists.len() > 1 {
            return Err(SceneError::TrackLists { count: lists.len() });
        }
        let track_list = match lists.first() {
            Some(res) => Some(TrackList::parse(doc.data(TRACK_LIST, res)?.bytes).map_err(SceneError::TrackList)?),
            None => None,
        };
        let mut tracking = Vec::new();
        // : a row that names an object no object list holds does
        // not refuse the document: the rows before it are taken, it and every row after it
        // are not.
        for r in track_list.iter().flat_map(|l| l.rows.iter()) {
            let find = |o: &ListedObject| {
                let kind = o.object_kind();
                objects.iter().position(|x| Some(x.kind) == kind && x.identity == o.identity)
            };
            let (Some(tracker), Some(target)) = (find(&r.tracker), find(&r.target)) else { break };
            tracking.push(TrackRow { tracker, target });
        }

        // : the tracks the owners' slots name, last, so that every refusal of
        // the parts comes first.
        let mut bind = TrackBinder { doc: &doc, tracks: SceneTracks::default() };
        let mut object_slots = Vec::with_capacity(objects.len());
        for (i, o) in objects.iter().enumerate() {
            let mut s = ObjectTrackSlots::default();
            if let Some(p) = &o.placement {
                for (k, class) in track_slots::PLACEMENT.iter().enumerate() {
                    if let Some(class) = class {
                        s.placement[k] = bind.slot(class, p.tracks[k], &o.name)?;
                    }
                }
            }
            if let Some((_, ids)) = torus_tracks.iter().find(|(j, _)| *j == i) {
                for k in 0..2 {
                    s.torus[k] = bind.slot(track_slots::TORUS[k], ids[k], &o.name)?;
                }
            }
            object_slots.push(s);
        }
        let mut light_slots = Vec::with_capacity(lights.len());
        let mut beam_falloff = Vec::with_capacity(lights.len());
        let mut sun_slots = [None; 3];
        for l in &lights {
            beam_falloff.push(match l {
                Light::Radial(r) => match r.beam {
                    Some(b) => bind.slot(track_slots::BEAM_FALLOFF, b.falloff_track(), &objects[r.object].name)?,
                    None => None,
                },
                Light::Sun(_) => None,
            });
            let (part, name) = match l {
                Light::Radial(r) => (&r.part, &objects[r.object].name),
                Light::Sun(s) => (&s.light, &s.name),
            };
            let mut s = [None; 8];
            for k in 0..8 {
                s[k] = bind.slot(track_slots::LIGHT[k], part.tracks[k], name)?;
            }
            light_slots.push(s);
            if let Light::Sun(sun) = l {
                for k in 0..3 {
                    sun_slots[k] = bind.slot(track_slots::SUN[k], sun.sun.tracks[k], name)?;
                }
            }
        }
        // : found by class among the
        // sky part's identities, as the slot of each is not released.
        let mut sky_slots = [None; 9];
        let mut named = vec![false; sky.tracks.len()];
        for (k, class) in track_slots::SKY.iter().enumerate() {
            for (j, &id) in sky.tracks.iter().enumerate() {
                if let Some(i) = bind.find(class, id, true, b"the sky")? {
                    sky_slots[k] = Some(i);
                    named[j] = true;
                    break;
                }
            }
        }
        // ,: the identities
        // that name none of the applied classes
        // ("The wind": slots 40 to 42 drive the direction, speed and turbulence; the
        // cloud offset's own tracks are replaced by the wind's sums, so they are not read)
        let mut sky_wind = [None; 3];
        for (k, class) in track_slots::SKY_WIND.iter().enumerate() {
            let slot = track_slots::SKY_WIND_FIRST_SLOT + k;
            if let Some(&id) = sky.tracks.get(slot) {
                sky_wind[k] = bind.find(class, id, true, b"the sky")?;
                named[slot] |= id != 0;
            }
        }
        for (j, &id) in sky.tracks.iter().enumerate() {
            let offset_track = track_slots::SKY_CLOUD_OFFSET.iter().any(|c| bind.doc.resources(c).any(|r| r.identity == id));
            named[j] |= id != 0 && offset_track;
        }
        let sky_other = sky.tracks.iter().zip(&named).filter(|&(&id, &n)| id != 0 && !n).count();
        let mut camera_slots = Vec::with_capacity(cameras.len());
        for c in &cameras {
            let mut s = [None; 7];
            for k in 0..7 {
                s[k] = if k == track_slots::CAM_ROLL {
                    bind.camera_roll_slot(c.part.tracks[k])?
                } else {
                    bind.slot(track_slots::CAMERA[k], c.part.tracks[k], &c.name)?
                };
            }
            camera_slots.push(s);
        }
        let mut material_slots = Vec::with_capacity(materials.len());
        for m in &materials {
            let mut s = [None; 19];
            for k in 0..19 {
                // : slot 13, Mat_eMatFlags, is never read: the
                // material's reader asks for no track there, whatever the slot holds
                if k == track_slots::MAT_FLAGS {
                    continue;
                }
                s[k] = bind.slot(track_slots::MATERIAL[k], m.part.tracks[k], &m.name)?;
                // (a key's references name textures the material reaches between
                // keys): read with the textures, once each
                if let Some(i) = s[k] {
                    let refs: Vec<TextureRef> = match &bind.tracks.tracks[i].values {
                        TrackValues::ValueChannel(t) => t.keys.iter().flat_map(|key| key.value.refs).flatten().collect(),
                        TrackValues::ColourChannel(t) => t.keys.iter().flat_map(|key| key.value.refs).flatten().collect(),
                        _ => Vec::new(),
                    };
                    for r in refs {
                        textures.resolve(Some(r), || format!("a track of material \"{}\"", String::from_utf8_lossy(&m.name)))?;
                    }
                }
            }
            material_slots.push(s);
        }
        let mut texture_slots = Vec::with_capacity(textures.list.len());
        for t in &textures.list {
            let mut s = [None; 5];
            for k in 0..4 {
                s[k] = bind.slot(track_slots::TEXTURE_MAPPING[k], t.mapping.tracks[k], &t.name)?;
            }
            if let Some(p) = t.procedural() {
                s[4] = bind.slot(track_slots::TEXTURE_RECORD, p.track, &t.name)?;
            }
            texture_slots.push(s);
        }
        // : the terrain's fourth and fifth track words are
        // tracks of the lower and the upper bound; a missing track fails the terrain's reading
        let mut terrain_slots = Vec::with_capacity(terrains.len());
        for t in &terrains {
            let mut s = [None; 2];
            for k in 0..2 {
                s[k] = bind.slot(track_slots::TERRAIN_BOUNDS[k], t.part.tracks[3 + k], &t.name)?;
            }
            terrain_slots.push(s);
        }
        let Textures { list: textures, pictures, .. } = textures;
        let mut animation = bind.tracks;
        animation.terrains = terrain_slots;
        animation.objects = object_slots;
        animation.lights = light_slots;
        animation.beam_falloff = beam_falloff;
        animation.sun = sun_slots;
        animation.sky = sky_slots;
        animation.sky_other_tracks = sky_other;
        animation.sky_wind = sky_wind;
        animation.cameras = camera_slots;
        animation.materials = material_slots;
        animation.textures = texture_slots;

        Ok(Scene {
            settings,
            objects,
            materials,
            lights,
            cameras,
            view,
            sky,
            trees,
            stored_trees,
            terrains,
            paths,
            textures,
            pictures,
            clouds,
            meshes: Vec::new(), // a document holds no description mesh
            mesh_resources,
            form,
            animation,
            track_list,
            tracking,
            frame_time: None,
            older: None,
        })
    }
}

/// Reads each track resource an owner's slot names, once, and binds the slot to it.
/// (Bryce 5's documents: the tracks)
struct TrackBinder<'a, 'd> {
    doc: &'a Doc<'d>,
    tracks: SceneTracks,
}

impl TrackBinder<'_, '_> {
    /// The track of class `class` with `identity`, read if not yet: `None` for identity 0;
    /// `None` too where no resource of the class has it and `missing_ok`.
    fn find(&mut self, class: &'static str, identity: u32, missing_ok: bool, owner: &[u8]) -> Result<Option<usize>, SceneError> {
        if identity == 0 {
            return Ok(None);
        }
        if let Some(i) = self.tracks.tracks.iter().position(|t| t.tag == class && t.identity == identity) {
            return Ok(Some(i));
        }
        let Some(res) = self.doc.resources(class).find(|r| r.identity == identity) else {
            if missing_ok {
                return Ok(None);
            }
            return Err(SceneError::NoSuchTrack {
                owner: String::from_utf8_lossy(owner).into_owned(),
                class,
                identity,
            });
        };
        let d = self.doc.data(class, res)?;
        let track = DocTrack::read(class, identity, d.bytes).map_err(|refusal| SceneError::Track {
            class,
            identity,
            refusal,
        })?;
        self.tracks.tracks.push(track);
        Ok(Some(self.tracks.tracks.len() - 1))
    }

    /// An owner's slot of class `class`. (every identity in the parts names a
    /// resource of the slot's class)
    fn slot(&mut self, class: &'static str, identity: u32, owner: &[u8]) -> Result<Option<usize>, SceneError> {
        self.find(class, identity, false, owner)
    }

    /// The camera part's slot 1: the track of class `Cam_eBank` with the identity, else no track
    /// (the document is read). A `Cam_eCorpOrient` track named there is never read: no program
    /// knows the class, and the slot gives no track (-10-22).
    fn camera_roll_slot(&mut self, identity: u32) -> Result<Option<usize>, SceneError> {
        self.find(track_slots::CAMERA[track_slots::CAM_ROLL], identity, true, b"")
    }
}

/// The texture resources read so far, each once, in the order first named.
/// (Textures)
pub(crate) struct Textures<'a, 'd> {
    pub(crate) doc: &'a Doc<'d>,
    pub(crate) list: Vec<Texture>,
    /// The pictures the picture textures read so far name. (Pictures)
    pub(crate) pictures: Vec<HeldPicture>,
}

impl Textures<'_, '_> {
    /// Resolves every channel's references of `m`, reading each texture when first named.
    pub(crate) fn material(&mut self, m: &mut Material) -> Result<(), SceneError> {
        let material = String::from_utf8_lossy(&m.name).into_owned();
        for (k, ch) in m.part.colours.iter().enumerate() {
            m.textures.colours[k] = self.channel(&ch.refs, &material, crate::scene_parts::COLOUR_CHANNELS[k])?;
        }
        for (k, ch) in m.part.values.iter().enumerate() {
            m.textures.values[k] = self.channel(&ch.refs, &material, crate::scene_parts::VALUE_CHANNELS[k])?;
        }
        Ok(())
    }

    /// A channel's references A, B and alpha.
    fn channel(
        &mut self,
        refs: &[Option<TextureRef>; 3],
        material: &str,
        channel: &str,
    ) -> Result<[Option<usize>; 3], SceneError> {
        const SLOTS: [&str; 3] = ["A", "B", "alpha"];
        let mut out = [None; 3];
        for (k, r) in refs.iter().enumerate() {
            out[k] = self.resolve(*r, || format!("material \"{material}\", {channel} {}", SLOTS[k]))?;
        }
        Ok(out)
    }

    /// The index of the texture a reference names, reading it when first named; `None`
    /// for none (type code 0).
    fn resolve(&mut self, r: Option<TextureRef>, referrer: impl Fn() -> String) -> Result<Option<usize>, SceneError> {
        let Some(r) = r else { return Ok(None) };
        let refuse = |refusal| SceneError::Texture { referrer: referrer(), refusal };
        let class = TextureClass::of_code(r.code).ok_or_else(|| refuse(TextureRefusal::Code(r.code)))?;
        if let Some(i) = self.list.iter().position(|t| t.class == class && t.identity == r.identity) {
            return Ok(Some(i));
        }
        let tag = class.tag();
        let res = self
            .doc
            .resources(tag)
            .find(|res| res.identity == r.identity)
            .ok_or_else(|| refuse(TextureRefusal::NoSuchTexture { class: tag, identity: r.identity }))?;
        let d = self.doc.data(tag, res)?;
        let as_scene = |e| match e {
            ReadError::Scene(e) => e,
            ReadError::Refused(refusal) => SceneError::Texture {
                referrer: format!("texture \"{}\", named by {}", d.name(), referrer()),
                refusal,
            },
        };
        let mut texture = read_texture(&d, class).map_err(as_scene)?;
        if let TextureBody::Picture(p) = &mut texture.body {
            match self.picture(p.picture) {
                Ok(i) => p.index = i,
                // : a picture the document does not hold is not
                // an error; the texture is taken as absent, every reference to it as none.
                Err(ReadError::Refused(TextureRefusal::NoSuchPicture { .. })) => return Ok(None),
                Err(e) => return Err(as_scene(e)),
            }
        }
        self.list.push(texture);
        Ok(Some(self.list.len() - 1))
    }

    /// The index of the picture with `identity`, decoding it when first named.
    fn picture(&mut self, identity: u32) -> Result<usize, ReadError> {
        if let Some(i) = self.pictures.iter().position(|p| p.identity == identity) {
            return Ok(i);
        }
        let class = PICTURE_RESOURCE_CLASS;
        let res = self
            .doc
            .resources(class)
            .find(|r| r.identity == identity)
            .ok_or(ReadError::Refused(TextureRefusal::NoSuchPicture { identity }))?;
        let entry = |n: u32| {
            self.doc.container.entry(n as usize).ok_or_else(|| {
                ReadError::Scene(SceneError::NoDataEntry { class: class.to_string(), identity, entry: n })
            })
        };
        let (packed, header) = (entry(res.entries[0])?, entry(res.entries[1])?);
        let name = String::from_utf8_lossy(&res.name).into_owned();
        let decoded = decode_held(header, packed)
            .map_err(|refusal| ReadError::Scene(SceneError::Picture { name, identity, refusal }))?;
        self.pictures.push(HeldPicture {
            identity,
            name: res.name.clone(),
            picture: std::sync::Arc::new(decoded.picture),
            as_channels: decoded.as_channels.map(std::sync::Arc::new),
            no_pixels: decoded.no_pixels,
        });
        Ok(self.pictures.len() - 1)
    }
}

pub(crate) enum ReadError {
    Scene(SceneError),
    Refused(TextureRefusal),
}

/// A C3dBaseMaterial resource: its first part and its material part (version 1 only), its
/// textures not yet resolved (`Textures::material`).
pub(crate) fn read_material(doc: &Doc<'_>, res: &Resource) -> Result<Material, SceneError> {
    let d = doc.data(MATERIAL, res)?;
    d.need(FIRST_PART_LEN + 4)?;
    let order = order_of(d.bytes);
    let name = first_part_name(d.bytes).map_err(|e| d.record(e))?;
    let after = &d.bytes[FIRST_PART_LEN..];
    d.need(FIRST_PART_LEN + crate::scene_parts::MATERIAL_PART_LEN)?;
    let part =
        MaterialPart::parse(&after[..crate::scene_parts::MATERIAL_PART_LEN], order).map_err(|e| d.record(e))?;
    Ok(Material { identity: res.identity, name, part, textures: ChannelTextures::default(), second: SecondChannels::default() })
}

/// A texture resource's data: the first part, the mapping part, and the record part of a
/// procedural texture or the picture part of a picture texture (whose picture index the
/// caller fills). Data longer than these parts: the rest ignored .
pub(crate) fn read_texture(d: &Data<'_>, class: TextureClass) -> Result<Texture, ReadError> {
    let needed = match class {
        TextureClass::Procedural => PROCEDURAL_DATA_LEN,
        TextureClass::Picture => PICTURE_DATA_LEN,
    };
    d.need(needed).map_err(ReadError::Scene)?;
    let order = order_of(d.bytes);
    let name = first_part_name(d.bytes).map_err(|e| ReadError::Scene(d.record(e)))?;
    let mapping_end = FIRST_PART_LEN + MAPPING_PART_LEN;
    let mapping = Mapping::read(&d.bytes[FIRST_PART_LEN..mapping_end], order);
    let body = match class {
        TextureClass::Procedural => TextureBody::Procedural(Box::new({
            let b = &d.bytes[mapping_end..PROCEDURAL_DATA_LEN];
            // : as 3.0 the record's version word is not tested, every
            // record is read as version 1
            match d.reading {
                Reading::ThreeZero => ProceduralTexture::read_key_as(b, order, RecordForm::Whole),
                _ => ProceduralTexture::read(b, order),
            }
            .map_err(ReadError::Refused)?
        })),
        TextureClass::Picture => {
            let (version, picture) = picture_part(&d.bytes[PICTURE_READ_LEN..PICTURE_DATA_LEN], order);
            TextureBody::Picture(PictureTexture { version, picture, index: usize::MAX })
        }
    };
    Ok(Texture { class, identity: d.res.identity, name, mapping, body })
}

/// The identity of the first tree resource met in the order of the object lists' rows: each
/// row's object as it comes, then the list linked to the row (each list once), deeper lists
/// likewise.. (Trees)
fn first_tree_in_list_order(doc: &Doc<'_>, root: &ObjectList) -> Result<Option<u32>, SceneError> {
    fn walk(doc: &Doc<'_>, rows: &[ObjectRow], seen: &mut Vec<u32>) -> Result<Option<u32>, SceneError> {
        for row in rows {
            if row.kind == ObjectKind::Tree {
                return Ok(Some(row.identity));
            }
            let id = row.linked_list;
            if id == 0 || seen.contains(&id) {
                continue;
            }
            seen.push(id);
            let res = doc
                .resources(LINKED_LIST)
                .find(|r| r.identity == id)
                .ok_or(SceneError::NoSuchLinked { list: id, row: 0, class: LINKED_LIST, identity: id })?;
            let ld = doc.data(LINKED_LIST, res)?;
            let list = ObjectList::parse(ld.bytes).map_err(|e| ld.record(e))?;
            if let Some(t) = walk(doc, &list.rows, seen)? {
                return Ok(Some(t));
            }
        }
        Ok(None)
    }
    walk(doc, &root.rows, &mut Vec::new())
}

/// A tree's name, its placement (Bryce 5's version 2) and its tree part; `holds` says
/// whether it is the tree that holds the stored trees.
fn tree_parts(d: &Data<'_>, holds: bool) -> Result<(Vec<u8>, PlacementPart, TreePart), SceneError> {
    d.need(FIRST_PART_LEN + 4)?;
    let name = first_part_name(d.bytes).map_err(|e| d.record(e))?;
    let refuse = |refusal| SceneError::Tree { object: String::from_utf8_lossy(&name).into_owned(), refusal };
    let order = order_of(d.bytes);
    let after = &d.bytes[FIRST_PART_LEN..];
    // The version is checked before the length .
    let version = crate::scene_records::word(after, 0, order);
    let len = PlacementPart::len_of_version(version)
        .ok_or_else(|| d.record(RecordError::UnknownVersion { record: "placement", version }))?;
    d.need(FIRST_PART_LEN + len)?;
    let placement = PlacementPart::parse_either(&after[..len], order).map_err(|e| d.record(e))?;
    let part = TreePart::parse(d.bytes, FIRST_PART_LEN + len, order, holds).map_err(|r| match r {
        TreeRefusal::Short { needed } => SceneError::ShortData {
            class: d.class.to_string(),
            name: d.name(),
            len: d.bytes.len(),
            needed,
        },
        r => refuse(r),
    })?;
    Ok((name, placement, part))
}

/// The light part after a placement: version 1 (192 bytes) or 3 (204); version 2, whose layout
/// is Open, and any other are refused.
fn light_part(d: &Data<'_>, pl: &Placed<'_>) -> Result<LightPart, SceneError> {
    let at = FIRST_PART_LEN + pl.len;
    d.need(at + 4)?;
    let version = crate::scene_records::word(pl.rest, 0, pl.order);
    // : as 3.1 a light part of version 2 or 3 is read as 192 bytes of
    // version 1's layout, what follows it from the bytes after those.
    if d.reading == Reading::ThreeOne && matches!(version, 2 | 3) {
        let len = LightPart::len_of_version(1).expect("version 1 is known");
        d.need(at + len)?;
        let mut one = pl.rest[..len].to_vec();
        match pl.order {
            ByteOrder::Little => one[..4].copy_from_slice(&1u32.to_le_bytes()),
            ByteOrder::Big => one[..4].copy_from_slice(&1u32.to_be_bytes()),
        }
        let part = LightPart::parse(&one, pl.order).map_err(|e| d.record(e))?;
        return Ok(part.with_third_flags(pl.placement.third_flags));
    }
    let len = match LightPart::len_of_version(version) {
        Some(len) => len,
        None => return Err(d.record(RecordError::UnknownVersion { record: "light", version })),
    };
    d.need(at + len)?;
    let part = LightPart::parse(&pl.rest[..len], pl.order).map_err(|e| d.record(e))?;
    Ok(part.with_third_flags(pl.placement.third_flags))
}
