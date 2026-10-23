// SPDX-License-Identifier: MIT
//! What a render needs before its first ray: the camera, the geometry scene, the region and
//! order of pixels, the sample placer, the generator and the output options, built from a
//! read document and a version.
//!
//! (a radial light's switch 0x200), (the view's transform and the camera's numbers,
//! as's inputs),
//! (switch bit 0x4000,
//! with,
//! (Effects: which render makes the star
//! list), (combined groups: who takes part, hidden objects, the members drawn
//! only through their group),
//! (Plan; Open choices; implementer's choices)
//! (Combined groups)
//!
//! Where an entry leaves a behaviour Open, the plan does not choose: either the caller's
//! `OpenChoices` gives it, or the plan is refused with an `OpenItem` naming the entry.
//! Where an entry leaves it to the implementer, the plan decides as render.md records, and
//! a refusal by that decision is `RenderError::Refused`.

use std::fmt;

use crate::geom::dot;
use crate::geometry::structure::{Layout, Listed, Structure};
use crate::geometry::group::{self, Fate, GroupOpen, Kind};
use crate::geometry::{self, Placement, ShapeContext};
use crate::scene_records::ObjectKind;
use crate::numerics::CRand;
use crate::sampling::aa_pass::edge_limit;
use crate::sampling::output::PostOptions;
use crate::sampling::lens::Lens;
use crate::sampling::samples::{ChannelScale, MAX_RAYS_A_SIDE};
use crate::sampling::{
    AaSetting, CameraInput, CameraModel, PixelVisit, Projection, RenderOrder, SamplePlacer, SavedPlace,
};
use crate::scene::{Body, Light, LightKind, Scene, Shape};
use crate::scene_records::{DocSettings, FourCc, Region};
use crate::shading::OpenItem;
use crate::version::RenderAs;

/// Open subjects the plan meets. (Open choices)
pub mod open {
    use crate::shading::OpenItem;

    const fn item(entry: &'static str, subject: &'static str, question: &'static str) -> OpenItem {
        OpenItem { entry, subject, question }
    }

    /// point 1, point 1 : Open as 3.1
    /// and 5.0 with the animation switch not 0, including where 5.0 applies the other
    /// owners' tracks (a document in Bryce 5's form saved at 0)
    pub const VIEW_TURN: OpenItem = item(
        "SCN-13",
        "the perspective view's rows as 3.1 and 5.0 in a document whose animation switch (SCN-7 offset 92) is not 0, or a view camera whose tracks' application on opening is Open (AN-1.6b)",
        "",
    );
    // The Premium switches' refusal (PREMIUM_BLURS) is gone: states
    // the blurred rays, the ambient ray and where their draws fall; soft shadows are refused
    // by.
    // The lens with the eye inside a solid (LENS_HELD_EYE) is no longer refused.
    // The structure code that is neither bspT nor grid and the option above 2 are no longer
    // refused: settles them.
    /// (bit 0x200 of a spot or parallel light: rays meet its own object,
    /// component geometry, Open)
    pub const BEAM_LIGHT_OBJECT: OpenItem = item(
        "SCN-12c",
        "a cylindrical parallel or square spot light, or an endless parallel light, whose switch word has 0x200 (rays meet the light's own object: not built here)",
        "",
    );
    pub const UNREAD_SHAPE: OpenItem =
        item("GEO-1", "objects other than the primitive shapes (paths, ...)", "");
    /// (Applies to: renders as 5.0; the original 3.0 and 3.1 do not open a document
    /// with a tree and show their default empty scene,: not a case, refused with a reason)
    pub const TREE_VERSION: OpenItem = item(
        "GEO-22",
        "a tree object in a render as 3.0 or 3.1 (the entry applies to renders as 5.0)",
        "",
    );
    pub const GROUP_ROLES: OpenItem = item(
        "GEO-18",
        "a group member with two or three of the role bits 0x8, 0x10 and 0x20 (and 0x4 clear)",
        "",
    );
    // The Solo mode's mark is no longer refused: draws it.
    /// (meshes, terrains, the lights' own shapes, pyramids, and metaballs and trees
    /// apart from 5.0's rule, as members that take part: out of scope, Open)
    pub const GROUP_MEMBER: OpenItem = item(
        "GEO-18",
        "a member of a group that combines, taking part, that is a mesh, a terrain, a pyramid, a light, or (as 3.0 and 3.1) a metaball or a tree",
        "",
    );
    /// , (Open: objects in
    /// combined groups as fuzzy solids)
    pub const GROUP_MEDIUM: OpenItem = item(
        "SH-2.2",
        "a member of a combined group whose material lets rays through or is fuzzy: a group as a medium or a fuzzy solid is Open",
        "",
    );
    /// (The kinds of triangle: any other combination of corner words and flag
    /// word, or a mesh whose triangles differ among them, is Open; Errors: a number out of
    /// range is Open)
    pub const MESH_DRAWING: OpenItem = item(
        "GEO-15",
        "a document's mesh that names a point, normal or coordinate number out of range where a corner reads it",
        "",
    );
    /// (Limits, revised: the original draws the powers of two from 16 to 4,096 as
    /// 5.0; for any other side a program may refuse the document or draw it (Freedom): refused)
    pub const TERRAIN_SIDE: OpenItem = item(
        "GEO-16",
        "a terrain whose grid side is not one of the powers of two from 16 to 4,096 (the original draws none other)",
        "",
    );
    pub const TERRAIN_VOLUME: OpenItem = item(
        "SH-2.2",
        "a terrain whose material lets rays through (Transparency above 0): a terrain as a solid that holds a ray is Open",
        "",
    );
    pub const PYRAMID_TABLE: OpenItem = item("GEO-2", "a pyramid needs the substrate table pyramid, not supplied", "");
    pub const AA_OFFSETS: OpenItem =
        item("SO-2.3", "the pass over edges needs the substrate table aa-offsets, not supplied", "");
    pub const DITHER_TABLE: OpenItem =
        item("SO-3.3", "dithering as 5.0 needs the substrate table dither, not supplied", "");
    pub const SECONDARY_RAYS: OpenItem =
        item(
            "SH-2.2",
            "further rays queued by surface rules that do not join what they bring back, or a shadow ray queued (SH-1.4 traces them inline)",
            "",
        );
}

/// Why a render could not be planned or finished.
#[derive(Clone, Debug, PartialEq)]
pub enum RenderError {
    /// A behaviour the entries leave Open and the caller did not decide.
    Open(OpenItem),
    /// The same, for one object of the root list (by its index in `Scene::objects`).
    OpenObject { object: usize, name: String, item: OpenItem },
    /// An input the entries leave to the implementer, which this renderer refuses, saying
    /// why. (implementer's choices)
    Refused { entry: &'static str, reason: String },
    EmptyPicture { width: u32, height: u32 },
    Write(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::Open(i) => write!(f, "cannot render: {i}"),
            RenderError::OpenObject { object, name, item } => {
                write!(f, "cannot render object {object} (\"{name}\"): {item}")
            }
            RenderError::Refused { reason, .. } => write!(f, "cannot render: {reason}"),
            RenderError::EmptyPicture { width, height } => {
                write!(f, "cannot render a picture of {width} x {height} pixels")
            }
            RenderError::Write(e) => write!(f, "the picture could not be written: {e}"),
        }
    }
}

impl std::error::Error for RenderError {}

impl From<OpenItem> for RenderError {
    fn from(i: OpenItem) -> Self {
        RenderError::Open(i)
    }
}

/// The document as `version` opens it: its tracks applied where applies them ,
/// or a refusal, saying why, where what opening applies, or a track's value, is Open. Every
/// render starts from it.. (Opening)
pub fn opened(scene: &Scene, version: RenderAs) -> Result<std::borrow::Cow<'_, Scene>, RenderError> {
    // ,: the document as 3.0 or 3.1 reads it
    if let Some(read) = scene.read_by(version).map_err(|e| refused("SCN-2", e.to_string()))? {
        let opened = opened_read(&read, version)?;
        return Ok(std::borrow::Cow::Owned(opened.into_owned()));
    }
    opened_read(scene, version)
}

/// `opened`, and for a render as 5.0 the Bryce 5 sky part's haze and fog numbers that the document
/// did not give (held as NaN) taken from the substrate's `haze-five`. Without the table
/// the part is left out, so that the sky is refused as before.,
/// (Sky: unset haze numbers)
pub fn opened_with_haze_five<'a>(
    scene: &'a Scene,
    version: RenderAs,
    haze_five: Option<[f64; 3]>,
) -> Result<std::borrow::Cow<'a, Scene>, RenderError> {
    let opened = opened(scene, version)?;
    let unset = |f: &crate::scene_sky::SkyFive| f.haze_offset.is_nan() || f.haze_thickness.is_nan() || f.fog_offset.is_nan();
    if version != RenderAs::Bryce50 || !opened.sky.five.as_ref().is_some_and(unset) {
        return Ok(opened);
    }
    let mut s = opened.into_owned();
    match (haze_five, s.sky.five.as_mut()) {
        (Some([o, t, p]), Some(f)) => {
            if f.haze_offset.is_nan() { f.haze_offset = o; }
            if f.haze_thickness.is_nan() { f.haze_thickness = t; }
            if f.fog_offset.is_nan() { f.fog_offset = p; }
        }
        _ => s.sky.five = None,
    }
    Ok(std::borrow::Cow::Owned(s))
}

fn opened_read(scene: &Scene, version: RenderAs) -> Result<std::borrow::Cow<'_, Scene>, RenderError> {
    // : a picture of height 0 holds no pixel and has nothing to read
    // from it: a texture that names one is not drawn here (refused)
    if let Some(p) = scene.pictures.iter().find(|p| p.no_pixels) {
        let named = scene.textures.iter().any(|t| matches!(&t.body, crate::scene_textures::TextureBody::Picture(pt) if scene.pictures.get(pt.index).is_some_and(|q| q.identity == p.identity)));
        if named {
            return Err(refused("SCN-19", format!("the picture \"{}\" has a height of 0 and holds no pixel for a texture to read", String::from_utf8_lossy(&p.name))));
        }
    }
    // : a terrain whose word is neither 0 nor 1 is drawn as a
    // mirrored one in all but the place of its bottom face ; an implementation may
    // refuse it, saying why, and this one does
    if let Some(t) = scene.terrains.iter().find(|t| t.part.mirrored_word > 1) {
        return Err(refused(
            "SCN-18",
            format!("terrain \"{}\" has the mirrored word {}, neither 0 nor 1 (the document is refused: SCN-18 allows it)", String::from_utf8_lossy(&t.name), t.part.mirrored_word),
        ));
    }
    // : as 3.0 every texture record is read as version 1, the bits of each
    // single of a record of version 2 taken as a whole number.
    let swapped = (version == RenderAs::Bryce30
        && scene.textures.iter().any(|t| t.procedural().is_some_and(|p| p.as_bryce30.is_some())))
    .then(|| {
        let mut s = scene.clone();
        for t in &mut s.textures {
            if let crate::scene_textures::TextureBody::Procedural(p) = &mut t.body {
                if let Some(whole) = p.as_bryce30.take() {
                    p.settings = *whole;
                    p.form = crate::scene_textures::RecordForm::Whole;
                }
            }
        }
        s
    });
    let opened = match &swapped {
        Some(s) => std::borrow::Cow::Owned(s.opened(version).map_err(|e| refused("AN-1.6", e.to_string()))?.into_owned()),
        None => scene.opened(version).map_err(|e| refused("AN-1.6", e.to_string()))?,
    };
    let mut opened = with_leaves_objects(opened);
    // : a sky part of a version above 2 read as 3.0 or 3.1. As 3.1 the
    // versions 4 to 7 are not opened; as 3.0 the version word is not tested but for 1, so the
    // part is read with version 2's layout.
    let part = opened.sky.version;
    // : as 3.0 a real part of version 3 to 7 whose words at version 2's
    // places name a texture or a track the document does not hold is not opened.
    if version == RenderAs::Bryce30 && opened.sky.bryce30_unopened {
        return Err(refused(
            "SCN-14a",
            format!("the sky part of version {part} names, at version 2's places, a texture or a track the document does not hold: Bryce 3.0 does not open it"),
        ));
    }
    if version == RenderAs::Bryce31 && (4..=7).contains(&part) {
        return Err(refused("SCN-14a", format!("the sky part is of version {part}, which Bryce 3.1 does not open")));
    }
    if version != RenderAs::Bryce50 && part == 3 {
        // version 3 is read as version 2 (its further 1,000 bytes are stored data)
        let mut s = opened.into_owned();
        s.sky.version = crate::scene_sky::SKY_VERSION_TWO;
        opened = std::borrow::Cow::Owned(s);
    }
    if version == RenderAs::Bryce30 && opened.sky.as_bryce30.is_some() {
        // The tracks' places differ between the layouts (version 2's are at 244): a keyed sky
        // track of such a part is not settled by the entries .
        if scene.animation.sky.iter().any(Option::is_some) {
            return Err(refused("SCN-14a", format!("a sky part of version {part} with keyed sky tracks, read as 3.0 with version 2's layout (Open)")));
        }
        let mut s = opened.into_owned();
        if let Some(mut b) = s.sky.as_bryce30.take() {
            b.flags = s.sky.flags;
            s.sky = *b;
        }
        opened = std::borrow::Cow::Owned(s);
    }
    if version == RenderAs::Bryce50 {
        // (`shadows`: "a sky part of version 3 is taken over ... as one of version 2",
        // versions 5 and 6 are drawn as version 7 with the same values; version 4 takes the
        // substrate's haze numbers and stays refused )
        let as_form = match opened.sky.version {
            3 => Some(crate::scene_sky::SKY_VERSION_TWO),
            5 | 6 => Some(crate::scene_sky::SKY_VERSION_SEVEN),
            _ => None,
        };
        if let Some(form) = as_form {
            let mut s = opened.into_owned();
            s.sky.version = form;
            return Ok(std::borrow::Cow::Owned(s));
        }
        return Ok(opened);
    }
    let word = if scene.sky.flags_opened_by_bryce3d() == scene.sky.flags { opened.sky.flags } else { opened.sky.flags & !0x80 };
    if word == opened.sky.flags {
        return Ok(opened);
    }
    let mut s = opened.into_owned();
    s.sky.flags = word;
    Ok(std::borrow::Cow::Owned(s))
}

/// The document's objects followed, for each tree, by the object a hit on that tree's leaves is
/// reported for, holding the leaves' material (`Body::TreeLeaves`). Not an object of the
/// document; a scene that has them already is returned as it is.
/// . (The skins)
fn with_leaves_objects(scene: std::borrow::Cow<'_, Scene>) -> std::borrow::Cow<'_, Scene> {
    let has = |s: &Scene| s.objects.iter().any(|o| matches!(o.body, Body::TreeLeaves(_)));
    if scene.trees.is_empty() || has(&scene) {
        return scene;
    }
    let mut s = scene.into_owned();
    for k in 0..s.trees.len() {
        let t = &s.trees[k];
        let object = crate::scene::SceneObject {
            name: t.name.clone(),
            kind: ObjectKind::Tree,
            identity: 0,
            linked_list: 0,
            linked_to: None,
            placement: None,
            world: None,
            linked_worlds: None,
            world_open: None,
            listing_worlds: None,
            material: t.leaves_material,
            body: Body::TreeLeaves(k),
        };
        s.objects.push(object);
    }
    std::borrow::Cow::Owned(s)
}

fn refused(entry: &'static str, reason: impl Into<String>) -> RenderError {
    RenderError::Refused { entry, reason: reason.into() }
}

/// The caller's decisions on what the entries leave Open, and the inputs a render takes
/// beside the document. A field left `None` (or empty) makes a plan that needs it fail with
/// the matching `open` item, unless the field says what `None` means.
/// (Open choices)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenChoices {
    /// Worker threads for the first pass and the pass over edges. `None` or `Some(1)`: one
    /// thread, the serial render; `Some(0)`: every core. The picture is the same bytes either
    /// way (a render the threads could change runs serially).
    pub threads: Option<usize>,
    /// The generator as the render starts. `None`: as a program just started, seeded with 1
    /// (point 3). A later render of the same document held starts where the render
    /// before it left the generator (Effects): give it here, with
    /// `star_list_held`. A sky with a seeded star field whose list this render makes starts
    /// the draws where the list left the generator instead (Effects).
    pub generator: Option<CRand>,
    /// The document held already has its star list: this render is a later render of the
    /// same document in one run (as 3.0 and 3.1, no Starfield or Comets button switched
    /// since), so it lays the list but makes none and draws nothing for one. `false`: the
    /// first render of a document just read, which makes the list.,
    /// (Where the render's draws start)
    pub star_list_held: bool,
    /// The substrate table `pyramid` , needed only when a pyramid is present.
    pub pyramid_normal: Option<[f64; 2]>,
    /// The substrate table `aa-offsets` : the seven x, then the seven y.
    /// Needed only when the pass over edges runs ('s switch bit 0x2000).
    pub aa_offsets: Option<[f64; 14]>,
    /// The substrate table `dither` , needed only when a render as 5.0
    /// dithers (point 5).
    pub dither_table: Option<Box<[u8; 256]>>,
    /// The substrate table `haze-five` (haze offset, haze thickness, fog offset), which
    /// a render as 5.0 of a Bryce 3D document with haze or fog depends on (As 5.0).
    /// Taken from the user's substrate only, never embedded; lacking, such a render is
    /// refused (`lit::open::HAZE_FIVE_TABLE`). (`haze-five`)
    pub haze_five: Option<[f64; 3]>,
    /// The substrate's texture tables (perm, waves, noise-bits-five), which the sky's
    /// cloud layers read . (The clouds in the render)
    pub texture_tables: crate::textures::tables::OwnedTables,
    /// Bryce 5's star catalogue , a file kept beside the user's substrate, which a sky
    /// as 5.0 with a negative star seed takes its stars from (Catalogue stars).
    /// `None`: not given (such a render is refused, saying so); `Some(Err(why))`: the file
    /// could not be read or is damaged . (Stars and comets)
    pub star_catalogue: Option<Result<std::sync::Arc<Vec<crate::substrate_kept::StarRecord>>, String>>,
    /// The substrate table `star-colours` , which a catalogue star's colour is made
    /// from . `None`: not held (such a render is refused, saying so).
    pub star_colours: Option<[u8; 105]>,
    /// The user's picture of the moon (a file kept beside the substrate: the one of the
    /// version rendered), which a sky with bit 0x20000 shows in the moon's disc (
    /// The moon's picture). `None`: not given (such a render is refused, saying so);
    /// `Some(Err(why))`: the file could not be read or is damaged .
    pub moon_picture: Option<Result<std::sync::Arc<crate::sky::moon_image::MoonImage>, String>>,
    /// The levels the picture holds before the render (row order, red, green, blue, the
    /// whole picture's size): the pixels outside the region keep them (point 6).
    /// `None`: white .
    pub earlier_picture: Option<Vec<u8>>,
    /// Render the whole picture whatever the document's region says (point 6: the
    /// implementer's choice to offer).
    pub whole_picture: bool,
    /// Go on from where the document says a saved render stood ('s progress), as the
    /// render the program makes by itself on opening a document , over
    /// `earlier_picture` (the earlier work). `false`: start over, as a render that is asked
    /// for. With the pass over edges on, the pass refines the pixels its edge test selects
    /// on the whole picture's levels, the earlier work's included (point 4, the
    /// implementer's choice).
    pub go_on: bool,
    /// Render the frame with no quick preview before it (the description's `preview` off,
    /// component `blender-addon`): the frame's first ray finds no material met. `false`: a
    /// frame asked for, which the quick preview renders before ; a render that goes
    /// on (`go_on`, the render on opening) has no preview before it either way.
    pub no_preview: bool,
    /// No longer read: settles a code that is neither `bspT` nor `grid` (walked as
    /// `grid`) and an option above 2 (as 2), so no render is refused for either. Kept so that
    /// callers that set it still build. (The structure code as 5.0)
    pub ignore_structure_code: bool,
    /// The live picture's scattered order (the add-on's, T2): `Some(seed)` asks
    /// `render_lit_streamed` to hand the picture out in a fixed random permutation of the
    /// pixels (the seed of its shuffle) rather than's order — exactly, when the
    /// anti-aliasing setting takes no draws of the generator, and as a quick one-ray-a-pixel
    /// preview before the exact frame when it does. `None`: the normal order. The finished
    /// picture is the same either way. (the live
    /// picture)
    pub scattered: Option<u32>,
    /// With `scattered` set: take the pixels row by row from the top (`--rows`) instead of in the shuffle.
    /// Same rules and same finished picture as the shuffle; only the order of the hand-out differs.
    pub scattered_rows: bool,
    /// How a frame whose shading or sample places draw from the generator (soft shadows, the
    /// wash of a Bryce 5 sky, jittered samples) may use several cores.
    pub shared_draws: SharedDraws,
    /// The retry of a frame that gave up its exact order (`render_lit_streamed`): the quick scattered preview was
    /// already shown by the attempt before it, and with a place of its own for every pixel it is the same picture
    /// again, so it is not made twice (wash-vol: 20 s of 69).
    pub skip_scattered_preview: bool,
}

/// Whether a frame whose shading draws may be shared out over cores with a place of its own in
/// the generator's sequence for every pixel (the old recomp's `independent_random`), which is
/// the program's kind of grain but not its numbers. The cargo tests and the documents' pictures
/// use `Exact`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SharedDraws {
    /// The program's order and numbers, one core.
    #[default]
    Exact,
    /// A place of its own for every pixel, on every core asked for.
    Independent,
    /// The program's order until the render has made this many draws of the generator, then (the
    /// render starting over) a place of its own for every pixel: cheap frames stay exact, large
    /// ones finish. The count is the work the frame asks for, not the clock, so the same
    /// description gives the same picture on any machine and under any load.
    Auto(u64),
}

/// The pass over edges as a render runs it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgePass {
    pub limit: u8,
    /// The substrate's `aa-offsets`.
    pub offsets: [f64; 14],
}

/// The geometry scene of a document, with the map back to the document's objects.
#[derive(Clone, Debug, PartialEq)]
pub struct GeometryBuild {
    pub scene: geometry::Scene,
    /// `scene_object[i]` is the index in `Scene::objects` of geometry object i.
    pub scene_object: Vec<usize>,
    /// `light[i]`: geometry object i is a radial light's own object (switch 0x200), which
    /// camera rays meet and shadow rays do not (point 7). A hit on it goes to the
    /// surface rules like any other; how it is shaded is component shading's (Open).
    pub light: Vec<bool>,
    /// Geometry objects that are a light's own object with no bit 0x200, which the structure does
    /// not list (no ray meets them) and which, as 3.0 and 3.1 only, the eye is tested against
    /// (twice, like a visible light's).
    pub unlisted: Vec<usize>,
}

/// A document mesh of more triangles than this is met through a bounding-volume tree: the same nearest triangle
/// (on an exactly equal distance the lowest number, not the greatest).
const BIG_MESH: usize = 1000;

fn name_of(scene: &Scene, i: usize) -> String {
    String::from_utf8_lossy(&scene.objects[i].name).into_owned()
}

/// A camera's projection word to the projection a render traces. The combinations
/// leaves to the implementer are traced in perspective (projection
/// word). point 1
pub fn projection_of_word(word: u32) -> Projection {
    Projection::from_word(word).unwrap_or(Projection::Perspective)
}

/// The six axis views' names. [format]
const AXIS_VIEW_NAMES: [&[u8]; 6] = [b"top", b"right", b"front", b"left", b"back", b"bottom"];
/// The projection word's bits's Variants clear and set.
const AXIS_VIEW_CLEARED: u32 = 0x4000 | 0x10000;
const AXIS_VIEW_SET: u32 = 0x8000;
const AXIS_VIEW_SCALE: f64 = 5_200_000.0;

/// The view camera's projection word and scale as the version reads them: as 3.1 and 5.0 a
/// camera named as one of the six axis views is read parallel, with the scale 5,200,000;
/// 3.0 changes nothing. (The saved views "dot0" to "dot7", which the same rule touches,
/// cannot be the view,.);
/// (`vc-p-top-pano`). (The big release)
fn view_word_and_scale(name: &[u8], word: u32, scale: f64, version: RenderAs) -> (u32, f64) {
    if version != RenderAs::Bryce30 && AXIS_VIEW_NAMES.contains(&name) {
        ((word & !AXIS_VIEW_CLEARED) | AXIS_VIEW_SET, AXIS_VIEW_SCALE)
    } else {
        (word, scale)
    }
}

/// The camera's numbers and the view's transform as's inputs..
/// The view's rows are right, up, forward and its translation the image plane's centre
/// ("the rows and translation of a transform,'s convention"); the camera
/// part's scale is's size of the image plane (point 10). The view's
/// transform is the one `version` opens the document with (as 3.0 the perspective
/// camera's rows are made from its angles).
pub fn camera_input(scene: &Scene, version: RenderAs) -> Result<CameraInput, RenderError> {
    let t = scene.view.transform(version).given().ok_or(open::VIEW_TURN)?;
    let camera = scene.view_camera();
    let part = &camera.part;
    let (projection_word, size) = view_word_and_scale(&camera.name, camera.projection_word, part.scale, version);
    Ok(CameraInput {
        right: t.rows[0],
        up: t.rows[1],
        forward: t.rows[2],
        centre: t.translation,
        fov: part.field_of_view,
        size, // point 10
        aspect: part.aspect,
        pan: part.pan,
        zoom: part.zoom,
        projection: projection_of_word(projection_word), // point 1
        width: scene.settings.picture_width,
        height: scene.settings.picture_height,
    })
}

/// point 8: a camera is traced as computes it from its rows as they are,
/// whatever their angle (rows perpendicular to within a relative 1e-15 must be; the others
/// are the implementer's choice, and this renderer traces them too). Refused (the
/// implementer's choice): a field of view not above 0 or not finite, rows of length 0 or
/// not finite, parallel up and forward rows, and any camera whose eye is not finite.
/// , point 8
/// (camera checks)
fn check_camera(c: &CameraInput, model: &CameraModel) -> Result<(), RenderError> {
    let finite = |v: [f64; 3]| v.iter().all(|x| x.is_finite());
    let (up, fwd) = (c.up, c.forward);
    let lengths_ok = finite(up) && finite(fwd) && dot(up, up) > 0.0 && dot(fwd, fwd) > 0.0;
    if !lengths_ok {
        return Err(refused("SO-1.2b point 8", "the camera's up or forward row has length 0 or is not finite"));
    }
    let cross = [up[1] * fwd[2] - up[2] * fwd[1], up[2] * fwd[0] - up[0] * fwd[2], up[0] * fwd[1] - up[1] * fwd[0]];
    if dot(cross, cross) == 0.0 {
        return Err(refused("SO-1.2b point 8", "the camera's up and forward rows are parallel (no right axis)"));
    }
    if !(c.fov.is_finite() && c.fov > 0.0) {
        return Err(refused("SO-1.2b point 8", format!("the camera's field of view {} gives no finite eye", c.fov)));
    }
    if !finite(model.eye()) {
        return Err(refused("SO-1.2b point 8", "the camera's numbers give no finite eye"));
    }
    Ok(())
}

/// The grid sides a terrain is drawn at.,
/// As 5.0 the original draws the nine powers of two from 16 to 4,096; as 3.0 and 3.1 the seven
/// to 1,024, and 2,048 and 4,096 are drawn by the same rules here (Freedom: not tried in
/// the original). Revised : as 3.0 and 3.1 every side from 2 to 4,096 is drawn (observed);
/// as 5.0 any side but the nine is refused.,
const TERRAIN_SIDES: [usize; 9] = [16, 32, 64, 128, 256, 512, 1024, 2048, 4096]; // Limits

/// Whether a terrain of side `n` is drawn as `version`.
fn side_drawn(version: RenderAs, n: usize) -> bool {
    if version == RenderAs::Bryce50 {
        TERRAIN_SIDES.contains(&n)
    } else {
        (crate::terrain::MIN_SIDE..=crate::terrain::MAX_READ_SIDE).contains(&n)
    }
}
/// The closing word's six faces (bits 0 to 5).

/// A radial light's switch: rays meet the light's own object. point 7
const LIGHT_OWN_OBJECT: u32 = 0x200;
/// The bit of a mesh object's second flag word that makes the mesh a solid between its
/// crossings in a group.
const MESH_SOLID_BIT: u32 = 0x200_0000;
/// Bit 0x1 of an object's second flag word: hidden.
const HIDDEN_BIT: u32 = 0x1;

/// The geometry scene: every shape object of the root list, in list order, placed by its
/// world transform, and every radial light whose switch word has 0x200 as's sphere
/// placed by the light's object-to-world transform. (row vectors:
/// world = local · rows + translation, the convention `geometry::Placement` takes),,
/// points 7 and 9
pub fn geometry_scene(scene: &Scene, choices: &OpenChoices, version: RenderAs) -> Result<GeometryBuild, RenderError> {
    let mut objects = Vec::new();
    let mut stored_meshes = Vec::new();
    let mut terrains = Vec::new();
    // : the chords of the paths that are drawn.
    let mut paths = Vec::new();
    // : the models of the trees, the (geometry object, scene tree) of each tree object, and
    // the geometry object of each scene tree's leaves.
    let mut trees: Vec<geometry::tree::TreeModel> = Vec::new();
    let mut tree_objects: Vec<(usize, usize)> = Vec::new();
    let mut leaves_object: Vec<usize> = Vec::new();
    let mut scene_object = Vec::new();
    let mut light = Vec::new();
    let mut unlisted: Vec<usize> = Vec::new();
    // The root list as lists it in the structure.
    let mut listed = Vec::new();
    // : who takes part, which groups combine,
    // which objects are hidden, from the rows and their words alone.
    let resolution = group_resolution(scene, version)?;
    // The combined groups' objects, built once every other object is placed: (geometry
    // object, document object).
    let mut combined = Vec::new();
    // The metaballs, in the order of's entries .
    let mut balls = Vec::new();
    let mut ball_objects = Vec::new();
    let mut ball_materials = Vec::new();
    // : the scene's order of the objects as the version forms it;
    // as 3.0 a list that holds a row whose linked list is that list again opens nothing, as 3.1
    // and 5.0 the rows the reader skips are not objects of the scene.
    if version == RenderAs::Bryce30 && scene.lists_itself() {
        return Err(refused("SCN-8", "a list holds a row whose linked list is that list again: as 3.0 the original opens nothing"));
    }
    for i in scene.order_as(version) {
        let o = &scene.objects[i];
        // (Errors: a time of infinity with the repeat setting 2 gives a place track
        // a place that is not finite: it puts the object nowhere, and it is not drawn). A shape
        // whose world transform holds a number that is not finite is left out of the scene.
        // (The big release: an object placed nowhere)
        if matches!(o.body, Body::Shape(_))
            && o.world_as(version).is_some_and(|w| {
                !w.rows.iter().flatten().chain(w.translation.iter()).all(|x| x.is_finite())
            })
        {
            continue;
        }
        if version != RenderAs::Bryce30 && scene.row_skipped_as_new(i) {
            if matches!(o.body, Body::RadialLight(_) | Body::Camera(_)) {
                return Err(refused(
                    "SCN-8",
                    format!("object {i} (\"{}\"): a light or camera on a row skipped as 3.1 and 5.0 is not supported", name_of(scene, i)),
                ));
            }
            continue;
        }
        // ,: a light's object that takes part in a
        // combined group is met only through its group, as any member is.
        if matches!(o.body, Body::RadialLight(_)) && matches!(resolution.fate(i), Fate::Through) {
            continue;
        }
        if !matches!(o.body, Body::Camera(_) | Body::RadialLight(_) | Body::Path(_) | Body::TreeLeaves(_)) {
            match resolution.fate(i) {
                // : a hidden object is not met; a member that takes part is met
                // only through its group. Neither is listed in the structure (an
                // object hidden by its flags is Open; our choice).
                Fate::Hidden | Fate::Through => continue,
                // : a group that combines is one object a ray meets. It is asked
                // before the structure, as an object gives no box
                // geometry.md, Combined groups).
                Fate::Combined => {
                    listed.push(Listed::Object(objects.len()));
                    combined.push((objects.len(), i));
                    objects.push(geometry::Object { shape: geometry::Shape::Group(usize::MAX), placement: Placement::IDENTITY });
                    scene_object.push(i);
                    light.push(false);
                    continue;
                }
                // : a group that does not combine is not an object a ray meets
                // (its members are met as themselves).
                Fate::Alone if o.body == Body::Group => continue,
                Fate::Alone => {}
            }
            // : listed by the box of the place the
            // document stored, though placed by the path.
            match (&o.body, o.listing_world_as(version)) {
                (Body::Shape(_), Some(w)) => {
                    listed.push(Listed::Stale { object: objects.len(), rows: w.rows, translation: w.translation })
                }
                _ => listed.push(Listed::Object(objects.len())),
            }
        }
        let mut light_object = None;
        let (shape, is_light) = match o.body {
            Body::Shape(s) => (s, false),
            Body::Camera(_) => continue, // a camera has no shape (lists none)
            // Added by `opened` after the document's objects: the object a hit on a leaf of
            // tree k is reported for, never listed in the structure.
            Body::TreeLeaves(k) => {
                if leaves_object.len() <= k {
                    leaves_object.resize(k + 1, usize::MAX);
                }
                // One shape object per leaf shape of the tree's model, consecutive (the tree
                // objects come before: `opened` appends these last). A tree not drawn has none.
                // (Fuzzy wood and fuzzy leaves)
                let model = tree_objects.iter().find(|&&(_, t)| t == k).and_then(|&(at, _)| match objects[at].shape {
                    geometry::Shape::Tree { model, .. } => Some(model),
                    _ => None,
                });
                let Some(model) = model else { continue };
                leaves_object[k] = objects.len();
                for shape in 0..trees[model].shape_count() {
                    objects.push(geometry::Object { shape: geometry::Shape::TreeLeaves { model, shape }, placement: Placement::IDENTITY });
                    scene_object.push(i);
                    light.push(false);
                }
                continue;
            }
            // A tree, as 5.0 : its stored tree built into a model, the object listed by
            // the box of its bounds. Its leaves' object is patched in once every object is
            // pushed..
            Body::Tree(k) => {
                if version != RenderAs::Bryce50 {
                    return Err(RenderError::OpenObject { object: i, name: name_of(scene, i), item: open::TREE_VERSION });
                }
                if let Some(e) = &o.world_open {
                    return Err(refused("AN-1.9", format!("object {i} (\"{}\"): {e}", name_of(scene, i))));
                }
                let world = o.world_as(version).expect("the reader gives every placed object its world transform");
                // : a tree whose placement is singular
                // draws nothing, wood, leaves and shadows alike
                let Some(placement) = geometry::tree::tree_map(world.rows, world.translation) else {
                    listed.pop(); // not listed, nothing held either (the placement is singular)
                    continue;
                };
                let stored = &scene.stored_trees[scene.trees[k].stored_tree];
                let model = match geometry::tree::TreeModel::new(&stored.shapes, &stored.pieces, &stored.leaves) {
                    Ok(m) => m.with_stored(scene.trees[k].stored_tree),
                    // : a stored tree with no pieces and no leaves draws
                    // nothing, holds nothing and changes nothing else of the picture
                    Err(geometry::tree::TreeOpen::Empty) => {
                        listed.pop();
                        continue;
                    }
                    Err(e) => return Err(refused("GEO-22", format!("tree object {i} (\"{}\"): {e}", name_of(scene, i)))),
                };
                if model.is_blank() {
                    // : every piece and leaf singular:
                    // nothing is drawn or listed, but a singular piece holds every point (the eye
                    // among them), so the object stays for's test
                    listed.pop();
                    objects.push(geometry::Object { shape: geometry::Shape::Tree { model: trees.len(), leaves: usize::MAX }, placement });
                    trees.push(model);
                    scene_object.push(i);
                    light.push(false);
                    continue;
                }
                tree_objects.push((objects.len(), k));
                objects.push(geometry::Object { shape: geometry::Shape::Tree { model: trees.len(), leaves: usize::MAX }, placement });
                trees.push(model);
                scene_object.push(i);
                light.push(false);
                continue;
            }
            // A path: one kept out of renders (bit 0x400000 of its second flag word) is met by
            // no ray and is in no leaf and no cell of's structure (
            //); one that is drawn is Open (a renderer may refuse
            // it, and this one does)..
            // (Paths)
            // : a path that is drawn (bits 0x400000 and 0x1 of its
            // second flag word clear) is an entry of the structure like any other, listed in its
            // place by its box; a path of one knot is never met and is left out.
            Body::Path(k) => {
                let flags = o.placement.map_or(0, |p| p.second_flags);
                // : the Solo mode's mark
                // (bit 0x10 of the first flag word) clears bit 0x1 as 3.1 and 5.0 (a path with bit
                // 0x1 and the mark is drawn), and no other bit; as 3.0 it changes nothing.
                let flags = solo_word(o, version, flags);
                let part = &scene.paths[k].part;
                // : 3.1 and 5.0 draw no picture of a version 2 part whose extra word is not 0
                // (the implementer's choice: refused as them); 3.0 does not look at the word.
                if part.extra_word != 0 && version != RenderAs::Bryce30 {
                    return Err(refused("SCN-21", format!("object {i} (\"{}\"): a path part whose extra word is {:#x}", name_of(scene, i), part.extra_word)));
                }
                if flags & (crate::scene_paths::KEPT_OUT_BIT | HIDDEN_BIT) != 0 || part.path_as(version).knots.len() < 2 {
                    continue;
                }
                if let Some(e) = &o.world_open {
                    return Err(refused("AN-1.9", format!("object {i} (\"{}\"): {e}", name_of(scene, i))));
                }
                let world = o.world_as(version).expect("the reader gives every placed object its world transform");
                let placement = Placement::new(world.rows, world.translation).ok_or_else(|| {
                    refused("SO-1.2b point 9", format!("object {i} (\"{}\") has a matrix with no inverse", name_of(scene, i)))
                })?;
                listed.push(Listed::Object(objects.len()));
                objects.push(geometry::Object { shape: geometry::Shape::Path(paths.len()), placement });
                paths.push(geometry::path::PathChords::new(part.path_as(version), version));
                scene_object.push(i);
                light.push(false);
                continue;
            }
            Body::RadialLight(l) => {
                let (switches, kind) = match &scene.lights[l] {
                    Light::Radial(r) => (solo_word(o, version, r.switches), r.kind()),
                    Light::Sun(_) => unreachable!("a root-list light is radial"),
                };
                // (the table: the radial light's object is the sphere, the spot
                // light's the cone; the parallel light's is Open)
                let light_shape = match kind {
                    LightKind::Radial => geometry::light::LightShape::Sphere,
                    LightKind::Spot => geometry::light::LightShape::Cone,
                    LightKind::Parallel => geometry::light::LightShape::Cube,
                    LightKind::Cylinder => geometry::light::LightShape::Cylinder,
                    LightKind::Square => geometry::light::LightShape::Pyramid,
                };
                if light_shape == geometry::light::LightShape::Pyramid
                    && switches & LIGHT_OWN_OBJECT != 0
                    && choices.pyramid_normal.is_none()
                {
                    return Err(RenderError::OpenObject { object: i, name: name_of(scene, i), item: open::PYRAMID_TABLE });
                }
                let own = geometry::light::LightObject { shape: light_shape, flags: switches };
                if switches & LIGHT_OWN_OBJECT == 0 {
                    // No ray meets it (point 7), but the structure lists it by its
                    // kind's box (for the sphere, for the others): listed,
                    // never hit; the endless option (0x2000) gives no box (asked apart, and
                    // never met).
                    if let Some(world) = o.world_as(version).filter(|_| !own.endless()) {
                        listed.push(if light_shape == geometry::light::LightShape::Sphere {
                            Listed::InertSphere { rows: world.rows, translation: world.translation }
                        } else {
                            Listed::InertShape {
                                shape: geometry::Shape::Light(own),
                                rows: world.rows,
                                translation: world.translation,
                            }
                        });
                    }
                    // : as 3.0 and 3.1 every light's own
                    // object, visible or not, is asked (twice); it is an object of the scene that
                    // the structure does not list, so no ray meets it. Not so as 5.0.
                    if version != RenderAs::Bryce50
                        && let Some(world) = o.world_as(version)
                        && let Some(placement) = Placement::new(world.rows, world.translation)
                    {
                        unlisted.push(objects.len());
                        objects.push(geometry::Object { shape: geometry::Shape::Light(own), placement });
                        scene_object.push(i);
                        light.push(false);
                    }
                    continue;
                }
                // With its own object it is's object, listed by its box (the
                // endless spot light has none and is asked apart).
                listed.push(Listed::Object(objects.len()));
                light_object = Some(own);
                (Shape::Sphere, true)
            }
            // A tree's geometry is not released (component geometry's); it is refused as
            // before was read.
            // A group, a mesh or a path: what is drawn for it is not
            // released (gives the primitive shapes only), as before read them.
            Body::Group => unreachable!("a group's fate is decided above"),
            // A metaball : listed in the structure by its box (already pushed above) and
            // asked through the metaballs' walk; its map and material are the set's inputs.
            // . (Metaballs)
            Body::Metaball { negative } => {
                if version != RenderAs::Bryce50 {
                    // (Applies to: a render as 3.0 or 3.1 of a document that
                    // holds a metaball is the implementer's choice: refused, saying why).
                    return Err(refused(
                        "GEO-21",
                        format!("object {i} (\"{}\") is a metaball, which only a render as Bryce 5.0 draws", name_of(scene, i)),
                    ));
                }
                if let Some(e) = &o.world_open {
                    return Err(refused("AN-1.9", format!("object {i} (\"{}\"): {e}", name_of(scene, i))));
                }
                let world = o.world_as(version).expect("the reader gives every placed object its world transform");
                // : a metaball whose map is singular holds every point of space,
                // the eye among them, and meets nothing of its set. A metaball that holds the eye
                // is refused (open::METABALL of the eye rule), so this one is refused, saying why.
                let placement = Placement::new(world.rows, world.translation).ok_or_else(|| {
                    refused("GEO-21", format!("object {i} (\"{}\") is a metaball whose map has no inverse", name_of(scene, i)))
                })?;
                if placement.is_singular() {
                    return Err(refused(
                        "GEO-21",
                        format!(
                            "object {i} (\"{}\") is a metaball whose map is singular: it holds the eye, and the material the set's medium takes before any hit is not given (SH-2.2)",
                            name_of(scene, i)
                        ),
                    ));
                }
                let part = o
                    .material
                    .map(|m| &scene.materials[m].part)
                    .ok_or_else(|| refused("SCN-9", format!("object {i} (\"{}\") has no material", name_of(scene, i))))?;
                let group = o.linked_to;
                ball_objects.push(objects.len());
                balls.push(geometry::metaball::Ball::new(placement, negative, group));
                ball_materials.push(geometry::metaball::MixInput {
                    flags: part.flags,
                    colours: part.colours.map(|c| c.a),
                    values: part.values.map(|c| c.a),
                    volume_colour: part.volume_colour,
                    refraction: part.refraction,
                    volume_numbers: part.volume_numbers,
                });
                objects.push(geometry::Object { shape: geometry::Shape::Metaball(balls.len() - 1), placement });
                scene_object.push(i);
                light.push(false);
                continue;
            }
            Body::Unread => {
                return Err(RenderError::OpenObject { object: i, name: name_of(scene, i), item: open::UNREAD_SHAPE });
            }
            // A document's mesh:'s arrays, drawn by where its triangles are of
            // one of's kinds; otherwise refused (Open)..
            // (Meshes)
            Body::MeshResource(k) => {
                let mut mesh = geometry::StoredMesh::from_resource(&scene.mesh_resources[k])
                    .map_err(|_| RenderError::OpenObject { object: i, name: name_of(scene, i), item: open::MESH_DRAWING })?;
                if mesh.faces.len() > BIG_MESH {
                    mesh.fast = Some(geometry::Fast::of(&mesh.points, &mesh.faces)); // (Speed on mesh scenes)
                }
                if let Some(e) = &o.world_open {
                    return Err(refused("AN-1.9", format!("object {i} (\"{}\"): {e}", name_of(scene, i))));
                }
                let world = o.world_as(version).expect("the reader gives every placed object its world transform");
                let placement = Placement::new(world.rows, world.translation).ok_or_else(|| {
                    refused("SO-1.2b point 9", format!("object {i} (\"{}\") has a matrix with no inverse", name_of(scene, i)))
                })?;
                objects.push(geometry::Object { shape: geometry::Shape::StoredMesh(stored_meshes.len()), placement });
                stored_meshes.push(mesh);
                scene_object.push(i);
                light.push(false);
                continue;
            }
            // A terrain: its grid  drawn as's relief, as the version reads its
            // bounds and solid word..
            Body::Terrain(k) => {
                let part = &scene.terrains[k].part;
                if !side_drawn(version, part.grid.side()) {
                    return Err(RenderError::OpenObject { object: i, name: name_of(scene, i), item: open::TERRAIN_SIDE });
                }
                // The solid word is read only for a combined group's lists , which a
                // render of a terrain does not make; the closing word draws.
                // : every version reads the six lowest bits of the closing
                // word only, so a higher bit (0x40 included) closes nothing and is never refused.
                if let Some(e) = &o.world_open {
                    return Err(refused("AN-1.9", format!("object {i} (\"{}\"): {e}", name_of(scene, i))));
                }
                let world = o.world_as(version).expect("the reader gives every placed object its world transform");
                let placement = Placement::new(world.rows, world.translation).ok_or_else(|| {
                    refused("SO-1.2b point 9", format!("object {i} (\"{}\") has a matrix with no inverse", name_of(scene, i)))
                })?;
                let second_flags = o.placement.as_ref().map_or(0, |p| p.second_flags);
                objects.push(geometry::Object { shape: geometry::Shape::Terrain(terrains.len()), placement });
                let mut relief = geometry::Relief::from_part(part, version, second_flags);
                relief.place(&placement);
                terrains.push(relief);
                scene_object.push(i);
                light.push(false);
                continue;
            }
            // A description mesh: the triangles are the caller's, brought by the side table.
            // (the description's meshes)
            Body::Mesh(k) => {
                let world = o.world_as(version).expect("the reader gives every placed object its world transform");
                let placement = Placement::new(world.rows, world.translation).ok_or_else(|| {
                    refused("SO-1.2b point 9", format!("object {i} (\"{}\") has a matrix with no inverse", name_of(scene, i)))
                })?;
                objects.push(geometry::Object { shape: geometry::Shape::Mesh(k), placement });
                scene_object.push(i);
                light.push(false);
                continue;
            }
        };
        let shape = if let Some(l) = light_object {
            geometry::Shape::Light(l)
        } else { match shape {
            Shape::Sphere => geometry::Shape::Sphere,
            Shape::Cube => geometry::Shape::Cube,
            Shape::Cylinder => geometry::Shape::Cylinder,
            Shape::Cone => geometry::Shape::Cone,
            Shape::Plane => geometry::Shape::Plane,
            Shape::Torus { tube_radius, ring_radius } => {
                geometry::Shape::Torus { tube: tube_radius, ring: ring_radius }
            }
            Shape::Slab => geometry::Shape::Slab,
            Shape::Square => geometry::Shape::Square,
            Shape::Disc => geometry::Shape::Disc, // (a document never names a disc)
            Shape::Pyramid => {
                if choices.pyramid_normal.is_none() {
                    return Err(RenderError::OpenObject {
                        object: i,
                        name: name_of(scene, i),
                        item: open::PYRAMID_TABLE,
                    });
                }
                geometry::Shape::Pyramid
            }
        } };
        // (a linked object's world, per version; Open where it is linked to a
        // path or to an object whose parts are not read)
        if let Some(e) = &o.world_open {
            return Err(refused("AN-1.9", format!("object {i} (\"{}\"): {e}", name_of(scene, i))));
        }
        let world = o.world_as(version).expect("the reader gives every placed object its world transform");
        // point 9: a matrix with no inverse is the implementer's choice; this
        // renderer refuses the document, saying why.
        let placement = Placement::new(world.rows, world.translation).ok_or_else(|| {
            refused("SO-1.2b point 9", format!("object {i} (\"{}\") has a matrix with no inverse", name_of(scene, i)))
        })?;
        objects.push(geometry::Object { shape, placement });
        scene_object.push(i);
        light.push(is_light);
    }
    // : an object whose matrix has the determinant 0 is inverted as
    // any other, its ray is carried to numbers that are not numbers and it is never met and casts
    // no shadow (a sphere of size (2.6, 0, 2.6): the picture equals the scene without it); it holds
    // the eye by its own test . It stays among the objects (the eye asks it) and is left
    // out of the structure. (singular maps)
    listed.retain(|l| !matches!(l, Listed::Object(i) if objects[*i].placement.is_singular()));
    // : the one object that stands for every set, its map the identity; never
    // listed in the structure. Its document object is that of the first metaball (the render
    // reads what a hit shows from the hit, not from this).
    // : a scene
    // with trees and no metaballs holds it too, over no set, so that a taken crossing and a hit
    // hand it the wood material as the render reads it.
    let first_tree = objects.iter().position(|o| matches!(o.shape, geometry::Shape::Tree { .. }));
    let metaballs = if balls.is_empty() && first_tree.is_none() {
        None
    } else {
        let stand = objects.len();
        objects.push(geometry::Object { shape: geometry::Shape::MetaSet, placement: Placement::IDENTITY });
        scene_object.push(scene_object[ball_objects.first().copied().or(first_tree).expect("a ball or a tree")]);
        light.push(false);
        let set = geometry::metaball::Metaballs { balls, reversed: false };
        Some(geometry::metaball::MetaScene::new(set, ball_objects, ball_materials, stand))
    };
    // : each combined group's members that take part, as geometry objects of
    // their own (after every other object), met only through the group.
    let mut groups = geometry::Groups::default();
    for (at, g) in combined {
        let mut member = |m: usize| -> Result<usize, RenderError> {
            let o = &scene.objects[m];
            let mut member_terrain: Option<geometry::Relief> = None;
            let shape = match o.body {
                Body::Shape(Shape::Sphere) => geometry::Shape::Sphere,
                Body::Shape(Shape::Cube) => geometry::Shape::Cube,
                Body::Shape(Shape::Cylinder) => geometry::Shape::Cylinder,
                Body::Shape(Shape::Cone) => geometry::Shape::Cone,
                Body::Shape(Shape::Plane) => geometry::Shape::Plane,
                Body::Shape(Shape::Torus { tube_radius, ring_radius }) => {
                    geometry::Shape::Torus { tube: tube_radius, ring: ring_radius }
                }
                Body::Shape(Shape::Slab) => geometry::Shape::Slab,
                Body::Shape(Shape::Square) => geometry::Shape::Square,
                Body::Shape(Shape::Disc) => geometry::Shape::Disc,
                Body::Shape(Shape::Pyramid) => {
                    if choices.pyramid_normal.is_none() {
                        return Err(RenderError::OpenObject { object: m, name: name_of(scene, m), item: open::PYRAMID_TABLE });
                    }
                    geometry::Shape::Pyramid
                }
                // : a mesh held by a group, with the solid bit of its object
                Body::MeshResource(k) => {
                    let mut mesh = geometry::StoredMesh::from_resource(&scene.mesh_resources[k])
                        .map_err(|_| RenderError::OpenObject { object: m, name: name_of(scene, m), item: open::MESH_DRAWING })?;
                    mesh.solid_in_group = o.placement.as_ref().is_some_and(|p| p.second_flags & MESH_SOLID_BIT != 0);
                    stored_meshes.push(mesh);
                    geometry::Shape::StoredMesh(stored_meshes.len() - 1)
                }
                // : a terrain held by a group (the solid word reads as)
                Body::Terrain(k) => {
                    let part = &scene.terrains[k].part;
                    if !side_drawn(version, part.grid.side()) {
                        return Err(RenderError::OpenObject { object: m, name: name_of(scene, m), item: open::TERRAIN_SIDE });
                    }
                    let second_flags = o.placement.as_ref().map_or(0, |p| p.second_flags);
                    member_terrain = Some(geometry::Relief::from_part(part, version, second_flags));
                    geometry::Shape::Terrain(terrains.len())
                }
                // : a light's own object takes part by its lists and marks;
                // the parallel light's object and the endless spot light's list are Open.
                Body::RadialLight(l) => {
                    let (switches, kind) = match &scene.lights[l] {
                        Light::Radial(r) => (solo_word(o, version, r.switches), r.kind()),
                        Light::Sun(_) => unreachable!("a root-list light is radial"),
                    };
                    // ,: a light of any kind is a member
                    // whose object is its kind's shape, the endless option included
                    let light_shape = match kind {
                        LightKind::Radial => geometry::light::LightShape::Sphere,
                        LightKind::Spot => geometry::light::LightShape::Cone,
                        LightKind::Parallel => geometry::light::LightShape::Cube,
                        LightKind::Cylinder => geometry::light::LightShape::Cylinder,
                        LightKind::Square => {
                            if choices.pyramid_normal.is_none() {
                                return Err(RenderError::OpenObject { object: m, name: name_of(scene, m), item: open::PYRAMID_TABLE });
                            }
                            geometry::light::LightShape::Pyramid
                        }
                    };
                    let own = geometry::light::LightObject { shape: light_shape, flags: switches };
                    geometry::Shape::Light(own)
                }
                // `group_kind` sends every other kind to the refusals of `group_resolution`.
                _ => return Err(RenderError::OpenObject { object: m, name: name_of(scene, m), item: open::GROUP_MEMBER }),
            };
            if let Some(e) = &o.world_open {
                return Err(refused("AN-1.9", format!("object {m} (\"{}\"): {e}", name_of(scene, m))));
            }
            let world = o.world_as(version).expect("the reader gives every placed object its world transform");
            let placement = Placement::new(world.rows, world.translation).ok_or_else(|| {
                refused("SO-1.2b point 9", format!("object {m} (\"{}\") has a matrix with no inverse", name_of(scene, m)))
            })?;
            if let Some(mut relief) = member_terrain {
                relief.place(&placement);
                relief.place_member(&placement);
                terrains.push(relief);
            }
            objects.push(geometry::Object { shape, placement });
            scene_object.push(m);
            light.push(false);
            Ok(objects.len() - 1)
        };
        let k = resolution.build(g, &mut groups, &mut member)?;
        objects[at].shape = geometry::Shape::Group(k);
    }
    for (at, k) in tree_objects {
        let leaves = leaves_object.get(k).copied().filter(|&l| l != usize::MAX).ok_or_else(|| {
            refused("GEO-22.6", format!("tree {k} has no object for its leaves (the scene was not opened by the render)"))
        })?;
        if let geometry::Shape::Tree { model, .. } = objects[at].shape {
            objects[at].shape = geometry::Shape::Tree { model, leaves };
        }
    }
    // : one object stands for every tree's wood and every set of
    // metaballs. With both in the scene it is the metaballs' `MetaScene::stand`: a tree's wood
    // hit is reported for it and a taken crossing hands it the wood material (`ask_tree`).
    // (One stand-in)
    let mut geometry =
        geometry::Scene { objects, meshes: scene.meshes.clone(), stored_meshes, terrains, paths, trees, structure: None, groups, metaballs };
    let (layout, option) = structure_layout(&scene.settings, choices, version)?;
    // : with the code grid the metaballs' entries stand in the reverse of the list.
    if let Some(m) = geometry.metaballs.as_mut() {
        m.set.reversed = layout == Layout::Grid;
    }
    // : the sun's own transform carries its box.
    let sun_world = {
        let w = scene.sun().placement.world();
        (w.rows, w.translation)
    };
    geometry.structure = Some(Structure::build(&geometry, &listed, sun_world, version, layout, option));
    Ok(GeometryBuild { scene: geometry, scene_object, light, unlisted })
}

/// How reads an object's kind. (Which members take part: not a path or a
/// camera; as 5.0 not a metaball or a tree; meshes, terrains and the lights' own shapes take
/// part as the shapes do, their lists out of scope; the pyramid out of scope)
fn group_kind(body: Body, kind: ObjectKind, version: RenderAs) -> Kind {
    match (body, kind) {
        // : the pyramid, a mesh and a terrain take part with the lists of
        (Body::Shape(_) | Body::MeshResource(_) | Body::Terrain(_), _) => Kind::Shape,
        (Body::Group, _) => Kind::Group,
        (Body::Camera(_) | Body::Path(_), _) | (_, ObjectKind::Path | ObjectKind::Camera) => Kind::Never,
        (_, ObjectKind::Blob | ObjectKind::Tree) if version == RenderAs::Bryce50 => Kind::Never,
        (Body::RadialLight(_), _) | (_, ObjectKind::SphereLight | ObjectKind::ConeLight | ObjectKind::CubeLight) => Kind::Light,
        _ => Kind::OutOfScope,
    }
}

/// 's working out for the scene: one row per object (its second flag word, its kind,
/// the group whose list holds it). Two role bits and a member out of scope that takes part are
/// refused (Open).,
fn group_resolution(scene: &Scene, version: RenderAs) -> Result<group::Resolution, RenderError> {
    let rows: Vec<group::Row> = scene
        .objects
        .iter()
        .map(|o| group::Row {
            word: o.placement.as_ref().map_or(0, |p| p.second_flags) | u32::from(solo_hides(o, version)),
            kind: group_kind(o.body, o.kind, version),
            parent: o.linked_to.filter(|&p| scene.objects[p].body == Body::Group),
            // : bit 0x10 of any object's first flag word is the Solo mode's mark
            solo: o.placement.as_ref().is_some_and(|p| p.first_flags & 0x10 != 0) && !solo_hides(o, version),
        })
        .collect();
    group::resolve(&rows, version).map_err(|e| {
        let (i, item) = match e {
            GroupOpen::TwoRoles(i) => (i, open::GROUP_ROLES),
            GroupOpen::OutOfScope(i) => (i, open::GROUP_MEMBER),
        };
        RenderError::OpenObject { object: i, name: name_of(scene, i), item }
    })
}

/// A light's word as its own object reads it: as 3.1 and 5.0 the Solo mode's mark (bit 0x10 of
/// the first flag word) makes the light count as one without bit 0x1; bit 0x400000 still hides
/// it. A light with the mark was not observed: gives this as the fallback.
fn solo_word(o: &crate::scene::SceneObject, version: RenderAs, word: u32) -> u32 {
    let marked = o.placement.as_ref().is_some_and(|p| p.first_flags & 0x10 != 0);
    if solo_hides(o, version) {
        word | 0x1
    } else if marked && version != RenderAs::Bryce30 {
        word & !0x1
    } else {
        word
    }
}

/// Whether an object with the Solo mode's mark is hidden by it: as 5.0 the mark makes bit 0x1 stand
/// for bit 0x1000 of the third flag word, so where that bit is set the object is hidden whether its
/// own bit 0x1 is set or not.,
fn solo_hides(o: &crate::scene::SceneObject, version: RenderAs) -> bool {
    version == RenderAs::Bryce50
        && o.placement.as_ref().is_some_and(|p| p.first_flags & 0x10 != 0 && p.third_flags & 0x1000 != 0)
}

/// The walk a render takes : as 5.0 the structure code of Bryce 5's part (the tree
/// without the part; a code that is neither `bspT` nor `grid` is walked as `grid`), as 3.0
/// and 3.1 the tree; and the spatial option, an option above 2 taking the numbers of 2.
/// Nothing is refused.
pub fn structure_layout(s: &DocSettings, _choices: &OpenChoices, version: RenderAs) -> Result<(Layout, u32), RenderError> {
    // : an option above 2 walks a tree of leaf size 5 and depth
    // limit 5; over a grid it takes the option 2's cells a side (Freedom). `structure` decides.
    let option = s.spatial_option;
    let layout = match structure_code(s, version) {
        None => Layout::Tree,
        Some(FourCc(c)) if &c == b"bspT" => Layout::Tree,
        Some(_) => Layout::Grid,
    };
    Ok((layout, option))
}

/// 's switch bit selecting the other way of making the picture: blocks traced at their
/// corners in place of's passes and's samples.
pub const SWITCH_CORNER_BLOCKS: u32 = 0x4000;
/// A stored block size with all 32 bits set: what 3.1 and 5.0 leave after a finished
/// render.
const STORED_ALL_BITS: u32 = u32::MAX;
/// 's switch bit turning the pass over edges on, as 3.0, 3.1 and 5.0, in both forms of
/// document. point 1
pub const SWITCH_EDGE_PASS: u32 = 0x2000; // point 1 (0x2000 [format])
/// 's switch bits that, without Bryce 5's part, leave gamma and dithering to the
/// implementer as 5.0., point 5
const SWITCH_POST_BITS: u32 = 0x80000 | 0x40000; // Variants (0x80000, 0x40000 [format])
/// Bryce 5's rays a side when the document has no Bryce 5 part. point 4
const BRYCE5_DEFAULT_RAYS: u32 = 4; // point 4

/// Bryce 5's part of the settings as a render reads it: present when its first word
/// (offset 156) is not 0. points 4 and 5;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bryce5Part {
    /// Offset 160: setting 2's rays a side.
    pub rays_a_side: u32,
    /// Offset 168: gamma  on when not 0.
    pub gamma: bool,
    /// Offset 172: dithering  on when not 0.
    pub dither: bool,
    /// Offset 176: the structure code a render as 5.0 takes: the walk
    /// (`structure_layout`).
    pub structure: FourCc,
    /// Offset 164: the Premium setting's rays a side (setting 4). point 1
    pub premium_rays_a_side: u32,
    /// Offset 188: the focal distance f, binary32. point 2
    pub focal_distance: f32,
    /// Offset 192: the lens radius ρ, binary32. point 2
    pub lens_radius: f32,
}

impl Bryce5Part {
    /// points 4 and 5 (words at offsets 156, 160, 168, 172),,
    /// points 1 and 2 (164, 188, 192)
    pub fn of(s: &DocSettings) -> Option<Bryce5Part> {
        let w = &s.bryce5_words; // w[k] is the word at offset 156 + 4k
        (w[0] != 0).then(|| Bryce5Part {
            rays_a_side: w[1],
            gamma: w[3] != 0,
            dither: w[4] != 0,
            structure: s.structure_code(),
            premium_rays_a_side: w[2],          // point 1 (offset 164)
            focal_distance: f32::from_bits(w[8]), // point 2 (offset 188)
            lens_radius: f32::from_bits(w[9]),    // point 2 (offset 192)
        })
    }
}

/// The Premium switches of the switch word (offset 56). point 2 ([format])
pub const SWITCH_SOFT_SHADOWS: u32 = 0x40; // point 2 (0x40 [format])
pub const SWITCH_BLURRED_REFLECTIONS: u32 = 0x80_0000; // point 2 (0x800000 [format])
pub const SWITCH_BLURRED_TRANSMISSIONS: u32 = 0x100_0000; // point 2 (0x1000000 [format])
pub const SWITCH_DEPTH_OF_FIELD: u32 = 0x200_0000; // point 2 (0x2000000 [format])
pub const SWITCH_TRUE_AMBIENCE: u32 = 0x400_0000; // point 2 (0x4000000 [format])

/// The switches by which a Premium sample's tree draws: blurred reflections, blurred
/// transmissions, true ambience.; (the per-sample path)
pub const SWITCHES_DRAWING_IN_TREES: u32 =
    SWITCH_BLURRED_REFLECTIONS | SWITCH_BLURRED_TRANSMISSIONS | SWITCH_TRUE_AMBIENCE;

const NEW_DOCUMENT_PREMIUM_RAYS: u32 = 8; // point 1
const NEW_DOCUMENT_FOCAL_DISTANCE: f32 = 15.0; // point 2
const NEW_DOCUMENT_LENS_RADIUS: f32 = 0.1; // point 2

/// The Premium setting as a render as 5.0 takes it from the document: its rays a side and,
/// with the depth-of-field switch, the lens. Refused where leaves the case to the
/// implementer (no document holds it). The blurred rays and the true ambience are's
/// (the rules read the switch word); soft shadows are the rules' (`shading::soft`).
/// points 1 and 2, (Premium)
fn premium_of(s: &DocSettings) -> Result<(u32, Option<Lens>), RenderError> {
    // points 1 and 2 (replacing 's "implementer's choice"): a
    // document without Bryce 5's part takes, as 5.0 with setting 4, 8 rays a side, the focal
    // distance 15 and the lens radius 0.1 (the single nearest), the numbers a new document's
    // part holds; setting 2's rays a side stay 4.
    // (Premium without the part)
    let part = Bryce5Part::of(s).unwrap_or_else(|| Bryce5Part {
        rays_a_side: BRYCE5_DEFAULT_RAYS,
        gamma: false,
        dither: false,
        structure: s.structure_code(),
        premium_rays_a_side: NEW_DOCUMENT_PREMIUM_RAYS,
        focal_distance: NEW_DOCUMENT_FOCAL_DISTANCE,
        lens_radius: NEW_DOCUMENT_LENS_RADIUS,
    });
    let n = part.premium_rays_a_side;
    if !(1..=MAX_RAYS_A_SIDE).contains(&n) {
        return Err(refused(
            "SO-2.5a point 1",
            format!("Premium rays a side {n}, outside 1 to 16 (the implementer's choice: refused)"),
        ));
    }
    let depth_of_field = s.switches & SWITCH_DEPTH_OF_FIELD != 0;
    let lens = depth_of_field.then_some(Lens { focal_distance: part.focal_distance, radius: part.lens_radius });
    Ok((n, lens))
}

/// The structure code (offset 176) a render as `version` takes: as 5.0 where the
/// document has Bryce 5's part; 3.0 and 3.1 do not read it, nor does 5.0 without the part.
/// What a render as 5.0 does with it. ("The
/// structure code at 176"),
pub fn structure_code(s: &DocSettings, version: RenderAs) -> Option<FourCc> {
    match version {
        RenderAs::Bryce30 | RenderAs::Bryce31 => None,
        RenderAs::Bryce50 => Bryce5Part::of(s).map(|p| p.structure),
    }
}

/// Setting 2's rays a side as 3.0 and 3.1, which do not read Bryce 5's part.
/// (Bryce 5's part: "a render as 3.0 or 3.1 takes 4 rays a side"),
const BRYCE3D_RAYS: u32 = 4;

/// The rays a side of anti-aliasing setting 2 a render as `version` takes from the
/// document., point 4
pub fn rays_a_side(s: &DocSettings, version: RenderAs) -> u32 {
    match version {
        RenderAs::Bryce30 | RenderAs::Bryce31 => BRYCE3D_RAYS,
        RenderAs::Bryce50 => bryce5_rays_a_side(s),
    }
}

/// Whether a render as `version` applies gamma and dithering. (Bryce 5's part,
/// offsets 156, 168 and 172: "3.0 and 3.1 ... apply neither gamma nor dithering, whatever the
/// part holds"), Variants, point 5
pub fn post_options(s: &DocSettings, version: RenderAs) -> PostOptions {
    match version {
        RenderAs::Bryce30 | RenderAs::Bryce31 => PostOptions { gamma: false, dither: false },
        RenderAs::Bryce50 => bryce5_post(s),
    }
}

/// Bryce 5's rays a side for a render as 5.0: the word at 160 where the word at 156 is not
/// 0, else 4. point 4
pub fn bryce5_rays_a_side(s: &DocSettings) -> u32 {
    Bryce5Part::of(s).map_or(BRYCE5_DEFAULT_RAYS, |p| p.rays_a_side)
}

/// Gamma and dithering for a render as 5.0. With Bryce 5's part its words decide, whatever
/// the switch word's bits 0x80000 and 0x40000 ("where they differ the words
/// decide"). Without it neither applies; with's switch bit 0x80000 or 0x40000 set
/// there (the implementer's choice) neither applies either .
/// Variants, point 5
pub fn bryce5_post(s: &DocSettings) -> PostOptions {
    match Bryce5Part::of(s) {
        Some(p) => PostOptions { gamma: p.gamma, dither: p.dither },
        // Both bits 0: neither (Variants); either set: neither too
        // render.md, the implementer's choice).
        None if s.switches & SWITCH_POST_BITS == 0 => PostOptions { gamma: false, dither: false },
        None => PostOptions { gamma: false, dither: false },
    }
}

/// The region a render covers, checked against the picture. point 6.
/// A region that is empty or reaches outside the picture is refused (the implementer's
/// choice: no entry gives it a meaning).
fn region_of(s: &DocSettings, whole_picture: bool) -> Result<Region, RenderError> {
    let whole = Region { top: 0, left: 0, bottom: s.picture_height, right: s.picture_width };
    if whole_picture {
        return Ok(whole);
    }
    let r = s.region;
    if r.top >= r.bottom || r.left >= r.right || r.bottom > s.picture_height || r.right > s.picture_width {
        return Err(refused(
            "SCN-7",
            format!(
                "the region (top {}, left {}, bottom {}, right {}) is empty or outside the {} x {} picture",
                r.top, r.left, r.bottom, r.right, s.picture_width, s.picture_height
            ),
        ));
    }
    Ok(r)
}

/// Everything fixed before the first ray. (Plan)
#[derive(Clone, Debug)]
pub struct RenderPlan {
    pub version: RenderAs,
    /// The whole picture's size : the picture a render writes. point 6
    pub width: u32,
    pub height: u32,
    /// The pixels rendered; the others keep `background`. point 6
    pub region: Region,
    /// The levels the picture holds before the render (whole size, RGB, row order).
    pub background: Vec<u8>,
    pub camera_input: CameraInput,
    pub camera: CameraModel,
    pub geometry: GeometryBuild,
    pub shape_context: ShapeContext,
    pub placer: SamplePlacer,
    /// 's progressive order. Every render of a document is progressive (
    /// Outputs, point 2); a caller that builds its own plan may set `false` for's order in
    /// rows, which no document selects.
    pub progressive: bool,
    pub generator: CRand,
    pub scale: ChannelScale,
    /// Bryce 5's post-processing (None as Bryce 3D, where none applies). point 5
    pub post: Option<PostOptions>,
    /// The substrate's `dither` table, present when `post` dithers.
    pub dither_table: Option<Box<[u8; 256]>>,
    /// The pass over edges, when's switch bit 0x2000 is set (point 1). It
    /// refines exactly the pixels its edge test selects, in every version, and in Bryce 5
    /// whatever the anti-aliasing setting, keeping to the region , after
    /// 's passes or's blocks.,,
    pub edge_pass: Option<EdgePass>,
    /// Where the render goes on from (`OpenChoices::go_on` with a document whose progress
    /// is not "start over"); `None`: the render starts over. point 4
    pub saved: Option<SavedPlace>,
    /// How the first picture is made.
    pub way: PictureWay,
    /// The pass over edges passes by a pixel the render has not written (the render on opening,
    /// as 3.0: the pixel holds the picture file's level and is neither tested nor refined, but
    /// its levels are read when its neighbours are tested).
    pub skip_unwritten: bool,
    /// The lens of the Premium setting with depth of field (5.0, switch bit 0x2000000), for
    /// the first pass's samples; `None` otherwise. point 3
    pub lens: Option<Lens>,
    /// As 5.0 with the Premium setting: the switch word's bits by which a sample's tree draws
    /// (`SWITCHES_DRAWING_IN_TREES`); 0 otherwise.
    pub premium_switches: u32,
    /// The substrate's `haze-five`, where the user gave it: the air as 5.0 takes it.
    pub haze_five: Option<[f64; 3]>,
    /// The substrate's texture tables, for the cloud layers.
    pub texture_tables: crate::textures::tables::OwnedTables,
    /// The star catalogue and the star-colours table, as the caller gave them
    /// (`OpenChoices`).,
    pub star_catalogue: Option<Result<std::sync::Arc<Vec<crate::substrate_kept::StarRecord>>, String>>,
    pub star_colours: Option<[u8; 105]>,
    /// The moon's picture, as the caller gave it (`OpenChoices`).
    pub moon_picture: Option<Result<std::sync::Arc<crate::sky::moon_image::MoonImage>, String>>,
    /// Whether this render makes the sky's star list (and starts its draws where the list
    /// left the generator).,
    pub star_list: StarListUse,
    /// A serial render of a frame that draws gives up once the thread's draw count (`numerics::draws_made`) passes
    /// this (`SharedDraws::Auto`):
    /// the error `Refused { entry: "slow-draws", .. }`, and the caller renders again shared.
    pub give_up_at: Option<u64>,
    /// Every pixel draws from a place of its own in the generator's sequence (`SharedDraws`).
    pub independent_draws: bool,
}

/// What a render does with the sky's seeded star list. (Effects: the list is
/// made once for a document held, by the first render that finds none; the quick preview's
/// sky has no field). (Where the render's draws start)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StarListUse {
    /// The first render of the document: make the list (reseeding the generator) and start
    /// the render's draws where it left the generator.
    #[default]
    Make,
    /// A later render of the document held: lay the list kept, make none, draw nothing.
    Held,
    /// The quick preview: no field at all, so no list and no draw.
    Skip,
}

/// How a render makes its first picture, before the pass over edges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PictureWay {
    /// 's passes, each pixel's samples placed by.
    Passes,
    /// 's blocks traced at their corners (switch bit 0x4000): one ray a place, through
    /// the place plus `offset` each way, no draw of the generator.
    CornerBlocks { offset: f64 },
    /// No pixel is rendered and no pass runs: the picture is the earlier one.
    /// (the render on opening, as 5.0, of a document storing a block size with all bits set)
    Nothing,
}

/// The offset of's one ray a place: the centre for settings 0 and 3, the corner for
/// setting 1. Settings 2 and 4 with the bit are the implementer's choice: the centre, as 0
fn corner_ray_offset(setting: AaSetting) -> f64 {
    match setting {
        AaSetting::Corner => 0.0,
        AaSetting::Centre | AaSetting::CentreAgain => 0.5,
        AaSetting::Jittered | AaSetting::Premium => 0.5, // (Freedom)
    }
}

/// The four progress words a document stores after the render on opening has finished (or,
/// as 3.1 and 5.0 with all bits set, was not written): 16, 0, 0, 0 as 3.0; all bits set, 0, 0,
/// 0 as 3.1 and 5.0.
pub fn progress_after_render(version: RenderAs) -> crate::scene_records::Progress {
    let block = match version {
        RenderAs::Bryce30 => 16,
        RenderAs::Bryce31 | RenderAs::Bryce50 => STORED_ALL_BITS,
    };
    crate::scene_records::Progress { block, mask: 0, column: 0, row: 0 }
}

/// The most pixels a picture may have (2^28: three levels a pixel then take 768 MiB).
/// (the implementer's bound of Limits)
const MAX_PICTURE_PIXELS: u64 = 1 << 28;

/// White, the levels outside the region when no earlier picture is given.
const BACKGROUND_LEVEL: u8 = 255; // (region)

impl RenderPlan {
    pub fn new(scene: &Scene, version: RenderAs, choices: &OpenChoices) -> Result<RenderPlan, RenderError> {
        let s = &scene.settings;
        let (width, height) = (s.picture_width, s.picture_height);
        if width == 0 || height == 0 {
            return Err(RenderError::EmptyPicture { width, height });
        }
        // : no largest size is set; a size whose pixels
        // the renderer cannot hold is refused, saying why, the bound being ours.
        // (Release of: the picture's size)
        if u64::from(width) * u64::from(height) > MAX_PICTURE_PIXELS {
            return Err(refused(
                "SCN-7",
                format!("a {width} x {height} picture has more pixels than this renderer holds ({MAX_PICTURE_PIXELS})"),
            ));
        }
        let corner_blocks = s.switches & SWITCH_CORNER_BLOCKS != 0;
        let region = region_of(s, choices.whole_picture)?;
        let pixels = width as usize * height as usize * 3;
        let background = match &choices.earlier_picture {
            None => vec![BACKGROUND_LEVEL; pixels],
            Some(p) if p.len() == pixels => p.clone(),
            Some(p) => {
                return Err(refused(
                    "SO-1.2b point 6",
                    format!("the earlier picture holds {} levels, the {width} x {height} picture {pixels}", p.len()),
                ));
            }
        };
        let camera_input = camera_input(scene, version)?;
        let camera = CameraModel::new(&camera_input, version);
        check_camera(&camera_input, &camera)?;
        let geometry = geometry_scene(scene, choices, version)?;

        let setting = AaSetting::from_document(s.anti_aliasing as i64);
        let mut lens = None;
        let mut premium_switches = 0;
        let rays_a_side = match (version, setting) {
            // With bit 0x4000 no pixel is sampled by: Premium is not reached.
            (_, AaSetting::Premium) if corner_blocks => 0, // (Freedom)
            // point 4: setting 4 as 3.0 or 3.1 is the implementer's choice.
            (RenderAs::Bryce30 | RenderAs::Bryce31, AaSetting::Premium) => {
                return Err(refused(
                    "SO-2.5a point 4",
                    "anti-aliasing setting 4 (Bryce 5's Premium) as 3.0 or 3.1 (the implementer's choice: refused)",
                ));
            }
            (RenderAs::Bryce50, AaSetting::Premium) => {
                let (n, l) = premium_of(s)?; // points 1 and 2
                lens = l;
                premium_switches = s.switches & SWITCHES_DRAWING_IN_TREES;
                n
            }
            (_, AaSetting::Jittered) => rays_a_side(s, version), // point 4
            _ => 0, // not used by the setting
        };
        let placer = SamplePlacer::new(setting, version, rays_a_side);
        // point 3 (seed 1 as a program just started)
        let generator = choices.generator.unwrap_or(CRand::new(1));
        let post = match version {
            RenderAs::Bryce30 | RenderAs::Bryce31 => None, // Variants (neither as 3.0, 3.1)
            RenderAs::Bryce50 => Some(post_options(s, version)),
        };
        let dither_table = match post {
            Some(p) if p.dither => Some(choices.dither_table.clone().ok_or(open::DITHER_TABLE)?),
            _ => None,
        };
        // point 1: bit 0x2000 of the switch word turns the pass on, as 3.0, 3.1
        // and 5.0. A document in Bryce 5's form likewise (Interface:
        // set, the pass runs; clear, there is none).
        // With bit 0x4000 too the pass runs after's blocks . In
        // a render of a part of the picture it keeps to the region .
        let edge_pass = match s.switches & SWITCH_EDGE_PASS != 0 {
            false => None,
            true => Some(EdgePass { limit: edge_limit(version), offsets: choices.aa_offsets.ok_or(open::AA_OFFSETS)? }),
        };
        // (only the render on opening goes on; the stored values that are no
        // place within a pass), point 4. With bit 0x4000 the stored block size is
        // not used ; going on from it is the implementer's choice: the render
        // starts over .
        let p = s.progress;
        let going_on = choices.go_on && !p.is_start_over() && !corner_blocks;
        let mut way = match corner_blocks {
            true => PictureWay::CornerBlocks { offset: corner_ray_offset(setting) },
            false => PictureWay::Passes,
        };
        let mut saved = None;
        let mut edge_pass = edge_pass;
        if going_on {
            match (p.block, version) {
                // (with all bits set, as 3.1 and 5.0, no pixel is rendered and no pass
                // over edges runs, bit 0x2000 set or clear)
                (STORED_ALL_BITS, RenderAs::Bryce31 | RenderAs::Bryce50) => {
                    way = PictureWay::Nothing;
                    edge_pass = None;
                }
                // (with 0, as 3.0, 3.1 and 5.0, no pixel is rendered; where bit 0x2000
                // is set the pass over edges follows: as 3.1 and 5.0 over the file's picture as
                // over a first pass, as 3.0 it passes by every pixel the render has not written,
                // here every pixel, so it changes none)
                (0, _) => {
                    way = PictureWay::Nothing;
                    if version == RenderAs::Bryce30 {
                        edge_pass = None;
                    }
                }
                // (with all bits set, as 3.0 the render goes on as from a stored
                // place whose block size is 2^32 - 1: the passes of `RenderOrder`); any other
                // stored place is gone on from
                _ => saved = Some(SavedPlace { block: p.block, mask: p.mask, column: p.column, row: p.row }),
            }
        }
        // (in the render on opening, as 3.0 the pass over edges passes by a pixel
        // the render has not written, and counts a pixel next to one as on an edge;'s
        // blocks, which do not use the stored place, write every pixel of the region)
        let skip_unwritten = going_on && version == RenderAs::Bryce30 && !matches!(way, PictureWay::CornerBlocks { .. });
        let shape_context = ShapeContext {
            version,
            // Not read unless a pyramid is present, and then the table is required above.
            pyramid_normal: choices.pyramid_normal.unwrap_or([f64::NAN; 2]),
            shadow: false,
        };
        Ok(RenderPlan {
            version,
            width,
            height,
            region,
            background,
            camera_input,
            camera,
            geometry,
            shape_context,
            placer,
            progressive: true,
            generator,
            scale: ChannelScale::for_version(version),
            post,
            dither_table,
            edge_pass,
            saved,
            way,
            skip_unwritten,
            lens,
            premium_switches,
            haze_five: choices.haze_five,
            texture_tables: choices.texture_tables.clone(),
            star_catalogue: choices.star_catalogue.clone(),
            star_colours: choices.star_colours,
            moon_picture: choices.moon_picture.clone(),
            star_list: if choices.star_list_held { StarListUse::Held } else { StarListUse::Make },
            give_up_at: None,
            independent_draws: choices.shared_draws == SharedDraws::Independent,
        })
    }

    /// The plan of a render mode : as `new`, except that a pyramid does not need the
    /// `pyramid` table to be planned. Without the table its sides meet nothing, and the mode's
    /// rules refuse any ray that could have met them (`modes::ModeRules`).
    /// . (A pyramid without the `pyramid` table)
    pub fn new_for_mode(scene: &Scene, version: RenderAs, choices: &OpenChoices) -> Result<RenderPlan, RenderError> {
        if choices.pyramid_normal.is_some() {
            return RenderPlan::new(scene, version, choices);
        }
        let marked = OpenChoices { pyramid_normal: Some([f64::NAN; 2]), ..choices.clone() };
        RenderPlan::new(scene, version, &marked)
    }

    /// Whether a ray's walk changes what later shading reads: a hit on a set of metaballs
    /// hands the object standing for every set its material , so each camera ray is
    /// walked just before its tree is shaded, in the order the render makes them, not in a
    /// batch ahead of the shading. (Inputs: the hits on sets, in the order the
    /// render makes them). (The order of the hits)
    pub fn walks_change_shading(&self) -> bool {
        self.geometry.scene.metaballs.is_some()
    }

    /// Whether a sample's tree can draw from the generator: the Premium setting as 5.0 with
    /// blurred reflections, blurred transmissions or true ambience. Such a render takes its
    /// samples one at a time (stages' per-sample path).
    /// (Premium samples that draw in their trees)
    pub fn draws_in_trees(&self) -> bool {
        self.placer.setting == AaSetting::Premium && self.version == RenderAs::Bryce50 && self.premium_switches != 0
    }

    /// The region's pixels in render order, in picture coordinates:'s order counts
    /// from the region's corner, and from the saved place when the render goes on.
    /// , points 2 and 6,
    /// point 4
    pub fn order(&self) -> impl Iterator<Item = PixelVisit> + use<> {
        let r = self.region;
        // No pixel in's order when the picture is made another way.
        let (w, h) = match self.way {
            PictureWay::Passes => (r.right - r.left, r.bottom - r.top),
            PictureWay::CornerBlocks { .. } | PictureWay::Nothing => (0, 0),
        };
        RenderOrder::new(w, h, self.progressive, self.saved)
            .map(move |v| PixelVisit { x: v.x + r.left, y: v.y + r.top, block: v.block })
    }

    /// The generator as a render with this plan left it, for a caller that goes on drawing
    /// from it (a later render of the same document held: Effects):
    /// `self.generator`. The pass over edges takes no draw for its seven places and nothing
    /// in it draws but the shading of its rays: none as 3.0 and 3.1; as 5.0 the shadows of a
    /// light with a radius , which the stage draws from the plan's generator and
    /// writes back (Shading that draws). question 2 (
    /// settling point 2), point 3,
    pub fn generator_after(&self, _counts: &super::RenderCounts) -> Result<CRand, RenderError> {
        Ok(self.generator)
    }

    /// Whether picture pixel (x, y) is rendered. point 6
    pub fn in_region(&self, x: u32, y: u32) -> bool {
        let r = self.region;
        (r.left..r.right).contains(&x) && (r.top..r.bottom).contains(&y)
    }
}
