// SPDX-License-Identifier: MIT
//! The released shading and sky rules as the render's `SurfaceRules` and `SkyRules`: plain
//! materials lit by the sun and radial lights with shadows, ambient and dome light, and the
//! plain sky; the further rays of, queued for the render's
//! stages and joined by `SurfaceRules::join`; the air of over every ray of the tree
//! (`SurfaceRules::over_joined`, after the join, before the blended replacement), and the
//! light in the air of after it over a camera ray.
//!
//! revised at: the media, the angle law, the blended material;
//! The media corrected at and at, : the medium
//! beyond a held solid, through `render::media::PathMedia`; (fuzzy solids, the
//! continuing ray, the weight and the shadow share, through `shading::fuzzy`;:
//! the fuzzy bit, the volume bits refused as's with and
//! the medium of a fuzzy solid, a shadow ray at a fuzzy surface of Transparency 0
//! , revised 8cbba36);
//! (revised 3e6e0f0:
//! Further rays), (and after: the gathering's shadow ray from 0.0001, As
//! 5.0); (the star field over a ray that meets nothing, its refusals;
//! , : the render's draws start where the list left the generator)
//! (Gels: each gel's colour at the lit point: the lights with a
//! beam, their factor at the lit point),;
//! Inputs (a terrain that lets rays
//! through: Open, refused); with's and's Open (a member of a combined
//! group whose material lets rays through or is fuzzy: refused;,
//! Combined groups);
//! : the blurred rays' cones (point 4) and true ambience's ambient ray
//! (point 5) asked of the stage, Variants (soft shadows);
//! point 5 revised at (the ambient ray held by's L, the document's ray
//! depth as 5.0: `depth_limit`); revised at (Bryce 5's render options: L and M
//! from Bryce 5's part, the four options through the switch word);
//! revised at;
//! (Bryce 5's soft shadows, through `shading::soft`: the radius, the n_s²
//! rays or the numbered ray, the give-up, the mean, the draws); revised
//! (gradients in the render, a gel with a gradient),
//! (Bryce 5's lights: in the render),
//! (Shading that draws)
//! (Premium samples that draw in their trees),
//! (Shadow rays; The renderer's rules; Textured materials in
//! the render; Fuzzy solids),,
//! (The renderer's rules; The air in the render; The air over a tree of rays; Light in the air;
//! Stars and comets; Where the render's draws start; Light in the air revised),

use std::cell::{Cell, RefCell};
use std::sync::{Arc, Mutex};

use crate::geom::Vec3;
use crate::geometry::{Hit, Placement, Range, Ray, Shape, ShapeContext};
use crate::sampling::batch::CameraRay;
use crate::shading::further::{self, BroughtBack, Medium, RayHit, RayKind, RayMaterial, RayMedia, Replace};
use crate::shading::media::{AngleLaw, MediumLaw, ShareOrders, blended_shadow_colour, shadow_share};
use crate::scene::{Light, Scene};
use crate::scene_beams::BeamRecord;
use crate::shading::gel::{GelKind, light_space_as};
use crate::shading::lights::{
    Beam, ColourKey, ConeOrder, KeyList, LightGradient, ValueKey, beam_factor, gradient_light, tinted_where_lit,
};
use crate::shading::soft::{self, DiscLight, ShadowMean, ShadowRays};
use crate::scene_parts::{MATERIAL_CURRENT, MATERIAL_FUZZY, MATERIAL_VOLUME_BITS, MaterialPart};
use crate::shading::OpenItem;
use crate::shading::fuzzy::{self, FuzzyOrders};
use crate::shading::lighting::{
    Bryce5Light, DotOrders, LightPlace, LightSource, LitLight, PlainMaterial, SceneLight, Shadow, ShadowQuery,
    LightTerms, ShadowSource, SumOrder, SurfaceInputs, light_terms, material_flag, scene_switch,
};
use crate::sky::air::{Air, AirInputs, AirPath, HazeFive, haze_rates};
use crate::sky::view_sun::{SUN_IN_VIEW, sun_in_world};
use crate::sky::rainbow::{Rainbow, RainbowInputs};
use crate::sky::tints::Tints;
use crate::sky::stars::{FieldEntry, FieldSwitches, Laying, StarList, StarOrders, held, seen_after_layers};
use crate::sky::star_grid::WideStarGrid;
use crate::shading::lighting::light_switch;
use crate::shading::channels::{TEXTURES_ON, channels_evaluated};
use crate::sky::clouds::{CloudFive, CloudRender, CloudSky, CloudSun, CloudTextures};
use crate::sky::lit_air::{LIT_AIR_COARSE, LIT_AIR_PLAIN_SKY, LitAir, LitAirScene, PathRay};
use crate::sky::sky_colours::{SkyColours, SkySettings};
use crate::textures::settings::TextureSettings;
use crate::textures::tables::{OwnedTables, TextureTables};
use crate::sky::colours::{AIR_ON, MainLight, PreparedSky, SkyInputs, SunInputs, in_scope, plain_sky, prepare};
use crate::numerics::CRand;

/// Where the path of the ray asking the sky started, which as 5.0 is where a cloud layer is met from:
/// the ray's own origin, except for a continued ray, whose path is its parent's. The original's ray
/// record keeps this point (RayState.start): the ray's origin for a camera, reflected, transmitted
/// or ambient ray, the parent's for the ray straight on through a blended surface. (As
/// 5.0),
fn cloud_path_start(request: &SkyRequest) -> Vec3 {
    match request.air {
        AirPath::Own => request.ray.origin,
        AirPath::Continued { start, .. } => start,
    }
}
use crate::version::RenderAs;

use super::eye::EyeMedium;
use super::textured::{self, GelLight, TexturedMaterial, own_box};
use crate::scene::Body;
use crate::shading::channels::{ChannelHit, MaterialAtHit};
use crate::textures::mapping::{MapHit, MapObject};
use super::media::{PathMedia, PathState};
use super::plan::{GeometryBuild, PictureWay, RenderError, RenderPlan, StarListUse};
use super::stages::{
    self, Picture, RayTurn, RenderCounts, SecondaryKind, SecondaryQueue, SecondaryRay, SkyRequest, SkyRules,
    SurfaceRequest, SurfaceRules, render_picture_progress,
};
use crate::sampling::{self, AaSetting, SamplePlacer, premium};
use crate::shading::volume::MediumTint;

/// Volume materials in the render: the march of a ray's medium, a volume material's surface,
/// shadow rays through volumes.; (Volume
/// materials: in the render)
mod volume;
use volume::VolumeMaterial;

/// The sky as 5.0 : the sky part as it reads it, the refusals, the sky with its field.
mod sky_five;
pub use sky_five::takes_catalogue;

/// The wash of a Bryce 5 sky's bit 0x1000000 in the render.
mod wash;

/// What these rules leave to entries not yet released, or that the entries mark Open.
pub mod open {
    use crate::shading::OpenItem;

    const fn item(entry: &'static str, subject: &'static str, question: &'static str) -> OpenItem {
        OpenItem { entry, subject, question }
    }

    /// Open in the entries themselves; asked in general by.
    const ENTRY_OPEN: &str = "";

    pub const TEXTURED_MATERIAL: OpenItem = item(
        "SH-1.1",
        "a material whose flag word has bits other than 0x10000, 0x20 and 0x10",
        ENTRY_OPEN,
    );
    /// evaluates the channels from textures at the hit (revised,
    /// gives the point): such a material is prepared by `object_material`, never by the
    /// plain material's rule, which refuses it with this item.
    pub const TEXTURE_MAPPING: OpenItem = item(
        "SH-2.1",
        "a material whose channels are evaluated from textures, read as a plain material",
        "",
    );
    pub const NO_MATERIAL: OpenItem = item("SH-1.1", "a shape object with no material", ENTRY_OPEN);
    /// Open: a radius below 0 or not a number; which third flag words other than 0,
    /// 0x2000 and 0x2002 keep the radius; a track's radius where the word lacks bit 0x2000
    /// (`scene_parts::gated_radius` gives `None`).
    pub const LIGHT_RADIUS: OpenItem = item(
        "SH-1.5",
        "as 5.0, a light whose radius is Open (below 0 or not a number, a third flag word other than 0, 0x2000 and 0x2002 with the bit, or a track's radius with the bit clear)",
        ENTRY_OPEN,
    );
    /// Open: a sun with a radius above 0 (its rays, its disc).
    pub const SUN_RADIUS: OpenItem = item("SH-1.5", "as 5.0, a sun whose radius is above 0", ENTRY_OPEN);
    /// : a volume material's march asks a light with a radius for
    /// its shadow by's rules; the lit air's gathering  is not given the ray's sample
    /// number here and refuses it. (Revision of onward)
    pub const RADIUS_MARCH: OpenItem = item(
        "SH-1.5",
        "a shadow asked of a light with a radius from the light in the air (not implemented here)",
        "",
    );
    /// Open: the shadow grid n_s of the quick preview , so a preview before a
    /// render whose shading draws. (Shading that draws)
    pub const SOFT_PREVIEW: OpenItem = item(
        "SH-1.5",
        "the quick preview of a document with a light whose radius is above 0 (its shadow grid)",
        ENTRY_OPEN,
    );
    /// Errors: a light with the gradient bit that names no gradient (the implementer's
    /// choice, refusing advised; refused where a render as 5.0 would draw it).
    pub const GRADIENT_ABSENT: OpenItem = item(
        "SCN-23",
        "as 5.0, a light whose switch word has the gradient bit 0x200000 and that names no gradient",
        "",
    );
    /// does not state a light with a gradient (as for a gel or a beam).
    pub const LIT_AIR_GRADIENT: OpenItem =
        item("SK-2.3", "the light in the air with a light whose gradient is on (SH-1.3)", ENTRY_OPEN);
    /// : p is carried into the light's own space by the
    /// inverse of its matrix; a light with no transform or no inverse: our choice, refused.
    pub const BEAM_TRANSFORM: OpenItem = item(
        "SH-1.3",
        "a spot or parallel light whose object has no transform, or one with no inverse",
        "",
    );
    /// : the gel point needs the light's own object's transform and its inverse.
    pub const GEL: OpenItem = item(
        "SH-1.3",
        "a gel whose light's own object has no transform, or one with no inverse",
        "",
    );
    pub const SKY_SWITCHES: OpenItem = item(
        "SK-1.1",
        "a sky switch word with bits outside 0x2, 0x4, 0x8, 0x80 and 0x100 (as 5.0 other than 0x100 or 0), or a sky part that is not Bryce 3D's",
        "",
    );
    /// Errors, Errors: a version 7 sky part (Bryce 5's) rendered as 3.0 or
    /// 3.1 is the implementer's choice (02-contents Errors); this renderer refuses it.
    pub const SKY_FIVE_AS_THREE: OpenItem = item(
        "SCN-14a",
        "a sky part of version 7 (Bryce 5's) rendered as 3.0 or 3.1: refused (the implementer's choice, SCN-6)",
        "",
    );
    /// "As 5.0" : the air as 5.0 takes `haze-five`'s numbers; without the
    /// table the render cannot be made. (The air in the render)
    pub const HAZE_FIVE_TABLE: OpenItem = item(
        "SK-2.1",
        "haze or fog as 5.0 (sky switch bit 0x80) needs the substrate table haze-five, not supplied",
        "",
    );
    /// Open: the order of the air against the eye's volume colour .
    pub const AIR_HELD_EYE: OpenItem = item(
        "SK-2.1",
        "the air with the camera's eye inside a solid (its order against the volume colour)",
        "",
    );
    /// point 3 Open: whether the sky's distances along a lens ray are measured from
    /// the point of the lens.
    pub const AIR_LENS: OpenItem = item(
        "SO-2.5a",
        "the air with depth of field (what a lens ray's length and start height are measured from)",
        "",
    );
    /// Applies to: a sky with animation tracks (wind and drift: Open).
    pub const CLOUD_TRACKS: OpenItem =
        item("SK-2.2", "clouds in a sky with animation tracks (wind and drift)", "");
    /// A cloud layer that names a picture texture: reads a picture for a material's
    /// channels; no sky entry says how a cloud layer reads one. (Picture
    /// textures: in the render)
    pub const CLOUD_PICTURE: OpenItem = item(
        "SK-2.2",
        "a cloud layer whose texture is a picture texture (no sky entry says how a layer reads a picture)",
        "",
    );
    /// : the original does not draw a
    /// document whose sky's first cloud texture reference names a picture texture, in any version
    /// and whatever the layers' bits; a program of ours refuses it, saying why.
    pub const CLOUD_PICTURE_FIRST: OpenItem = item(
        "SK-2.2",
        "a sky whose first cloud texture reference names a picture texture (the original does not draw such a document)",
        "",
    );
    /// : a cloud layer's texture reaches a stage the texture entries leave Open.
    pub const CLOUD_TEXTURE_OPEN: OpenItem =
        item("TX-1.1", "a cloud layer's texture reaches a stage the texture entries leave Open", "");
    /// A cloud reading refused while rendering (a table the substrate lacks).
    pub const CLOUD_EVALUATION: OpenItem = item(
        "SK-2.2",
        "a cloud layer's texture could not be evaluated (a table the substrate lacks: SUB-8)",
        "",
    );
    /// Open: further rays (their places, step and start).
    /// (Applies to: a material
    /// with a volume bit is's, bit 0x2 or not).
    pub const VOLUME_MATERIAL: OpenItem = item(
        "SH-2.4",
        "a volume material (flag word bit 0x100, 0x200 or 0x400) read as a plain material (a fault of these rules)",
        "",
    );
    /// does not state the medium colour's bits beside the fuzzy bit (states
    /// them for plain and volume materials).
    pub const FUZZY_TINT: OpenItem = item(
        "SH-2.6",
        "a fuzzy material whose flag word has 0x4000 or 0x8000 (the medium colour of SH-2.4)",
        ENTRY_OPEN,
    );
    pub const FUZZY_SHAPE: OpenItem = item(
        "SH-2.6",
        "a fuzzy material on a shape other than the sphere, the cone and the plane",
        ENTRY_OPEN,
    );
    /// : bit 0x4 beside the fuzzy bit 0x2; a material whose channels
    /// have textures is tried on an ordinary object only (a light's own object with textures is refused).
    pub const LIGHT_MODE_BITS: OpenItem = item(
        "SH-2.5",
        "the Light mode on a light's own object, or with the fuzzy bit 0x2, whose channels have textures",
        ENTRY_OPEN,
    );
    /// : the order of the start ray and the ambient ray of Bryce 5's gathered
    /// light is not stated.
    pub const LIGHT_MODE_GATHERED: OpenItem = item(
        "SH-2.5",
        "a Light-mode surface, or one with bit 0x2000, on which Bryce 5's gathered light also sends an ambient ray (their order)",
        ENTRY_OPEN,
    );
    /// with: the order of a fuzzy surface's continuing ray
    /// and the start ray is not stated.
    pub const ADDITIVE_FUZZY: OpenItem = item(
        "SH-1.1",
        "a fuzzy surface whose flag word has bit 0x2000 (the order of its start ray and its continuing ray)",
        ENTRY_OPEN,
    );
    // FUZZY_MEDIUM is gone: (The medium of a fuzzy solid, to question
    // 0334) states a fuzzy solid's medium.
    // FUZZY_LIMIT is gone: states the continuing ray's limit.
    // FUZZY_NO_EXIT and FUZZY_BETWEEN are gone: states Q where something
    // else stops the continuing ray, and the pictures hold surfaces between; a ray that meets nothing
    // counts F = 0.
    /// (Limits, Open: a Transparency t below 0, and t equal to 0 where a is not; answer
    /// to states t = 0 with a = 0). (Fuzzy solids)
    pub const FUZZY_SHADOW_OPAQUE: OpenItem = item(
        "SH-2.6",
        "a shadow ray meeting a fuzzy surface whose Transparency is below 0, or 0 where its A value is not",
        ENTRY_OPEN,
    );
    // LIT_AIR_FURTHER is gone:
    // states the light along every ray of the tree.
    /// : as 5.0 a sky whose number of steps is 0 with the lit-air bit: the original
    /// never finishes the render and gives no picture; the entry allows stopping
    /// and saying why.
    pub const LIT_AIR_NO_STEPS: OpenItem = item(
        "SK-2.3",
        "as 5.0 a sky part whose air_steps is 0 with the lit-air bit: the original's render never ends and gives no picture",
        "",
    );
    /// Open: an eye inside a solid.
    pub const LIT_AIR_HELD_EYE: OpenItem =
        item("SK-2.3", "the light in the air with the eye inside a solid", "");
    /// point 3 Open: the eye E of a lens ray.
    pub const LIT_AIR_LENS: OpenItem = item(
        "SO-2.5a",
        "the light in the air with depth of field (what a lens ray's places are measured from)",
        "",
    );
    /// Applies to: a sky part of version 1 as 5.0 keeps sky 01's Scope, and its field
    /// is not stated (a version 1 sky as 5.0 is Open).
    pub const STARS_FIVE: OpenItem = item(
        "SK-1.4",
        "a star field as 5.0 in a sky part of version 1 (SK-1.4 states versions 2 and 7)",
        "",
    );
    /// Effects Open: as 5.0 a seeded field with a light that has a radius.
    pub const STARS_LIGHT_RADIUS: OpenItem = item(
        "SK-1.3",
        "as 5.0 a seeded star field with a light that has a radius (where the generator stands for the frame after the preview's shadow rays)",
        "",
    );
    /// Open, each point named by the sky module.
    pub fn sky_five(what: crate::sky::five::FiveOpen) -> OpenItem {
        item("SK-1.4", what.describe(), "none (SK-1.4: Open)")
    }
    /// Applies to Bryce 3D documents.
    pub const LIT_AIR_BRYCE5: OpenItem = item(
        "SK-2.3",
        "the light in the air of a Bryce 5 sky (sky part of version 7)",
        "",
    );
    /// : stated , not implemented here.
    pub const WASH_LIT_AIR: OpenItem = item(
        "SK-1.4",
        "a Bryce 5 sky with the wash (bit 0x1000000) and the light in the air (bit 0x800000): the gathering's asks of the sun at point 9 with their draws are not implemented by this renderer",
        "",
    );
    /// : there is always a view; where its transform is Open here (the view's
    /// turn), clouds that follow it cannot be placed.
    pub const CLOUDS_FOLLOW_NO_VIEW: OpenItem = item(
        "SK-2.2d",
        "clouds that follow the view (sky switch bit 0x80000) where the view's transform is Open (SCN-13)",
        "",
    );
    /// Errors: the kept file and the table a catalogue sky needs.
    pub const STAR_CATALOGUE_MISSING: OpenItem = item(
        "SK-1.3",
        "the substrate keeps no star catalogue (SUB-10, from Bryce 5), which this sky's stars come from",
        "",
    );
    pub const STAR_COLOURS_MISSING: OpenItem = item(
        "SK-1.3",
        "the substrate lacks the table star-colours (SUB-3, from Bryce 5), which this sky's star colours come from",
        "",
    );
    pub const STAR_FILES_MISSING: OpenItem = item(
        "SK-1.3",
        "the substrate keeps no star catalogue (SUB-10) and lacks the table star-colours (SUB-3), both from Bryce 5, which this sky's stars come from",
        "",
    );
    pub const STAR_CATALOGUE_DAMAGED: OpenItem = item(
        "SK-1.3",
        "the star catalogue kept beside the substrate could not be read or is damaged (SUB-4)",
        "",
    );
    /// Errors: the moon's picture a sky with bit 0x20000 shows.
    pub const MOON_PICTURE_MISSING: OpenItem = item(
        "SK-1.3",
        "the substrate keeps no picture of the moon (SUB-9), which this sky's moon disc shows",
        "",
    );
    pub const MOON_PICTURE_DAMAGED: OpenItem = item(
        "SK-1.3",
        "the picture of the moon kept beside the substrate could not be read or is damaged (SUB-4)",
        "",
    );
    /// Open: a sun whose light falls off.
    pub const LIT_AIR_SUN_FALLOFF: OpenItem = item(
        "SK-2.3",
        "the light in the air with a sun whose light falls off (switch bit 0x400 clear)",
        "",
    );
}

/// Whether, as 5.0, the air's results depend on `haze-five`: bit 0x80 with some haze or
/// fog; without them no result does (As 5.0), so the table is needed only then.
fn check_haze_fog(scene: &Scene, version: RenderAs, haze_five: Option<[f64; 3]>) -> Result<(), OpenItem> {
    let s = &scene.sky;
    let some = s.haze > 0.0 || (s.fog_height > 0.0 && s.fog > 0.0);
    // A Bryce 5 sky's air is refused where it would act (`sky_five`).
    let bryce3d = s.form() != crate::scene_sky::SKY_VERSION_SEVEN;
    if version == RenderAs::Bryce50 && bryce3d && s.flags & AIR_ON != 0 && some && haze_five.is_none() {
        return Err(open::HAZE_FIVE_TABLE);
    }
    Ok(())
}

/// The air of a render (`None` where it does not act); as 5.0 with `haze-five`'s numbers
/// (a render that needs them and lacks them is refused before).
fn scene_air(scene: &Scene, version: RenderAs, main: &MainLight, haze_five: Option<[f64; 3]>) -> Option<Air> {
    let inputs = air_inputs(scene, main);
    let air = if version == RenderAs::Bryce50 {
        if scene.sky.form() == crate::scene_sky::SKY_VERSION_SEVEN {
            bryce5_air(scene, inputs)
        } else {
            let [offset, thickness, fog_offset] = haze_five?;
            // , (A Bryce 3D sky as 5.0:
            // the stored bit 0x80000000 is kept, the shift colour is's value)
            let shift = (scene.sky.flags & crate::sky::five::COLOR_PERSPECTIVE != 0)
                .then(|| scene.sky.as_five([0.0; 3]).map(|f| f.haze_shift_colour))
                .flatten();
            Air::new_five(&inputs, HazeFive { offset, thickness, fog_offset }, shift)
        }
    } else {
        Air::new(version, &inputs)
    };
    // (The tints, bits 0x2000 and 0x200000: S', the sun's colour C of and the
    // third and fourth of the fifteen numbers, in the three versions)
    let tints = Tints {
        sun: sun_direction(scene, version),
        colour: scene.sun().colour,
        overall: scene.sky.discs[2],
        toward_sun: scene.sky.discs[3],
    };
    air.map(|a| a.with_tints(scene.sky.flags, tints))
}

/// The air of a Bryce 5 sky (sky part of version 7) as 5.0: W is the stored word, O, T and P
/// the sky part's own numbers, c the cloud height when W has bit 0x20 or 0x40 or the second
/// word has bit 0x1 and 5 otherwise, the haze's reach by the second word's bit 0x1, Color
/// Perspective by W's bit 0x80000000. The clip distances are ignored (Open, "a program may
/// ignore them").
/// (The air of a Bryce 5 sky)
fn bryce5_air(scene: &Scene, mut inputs: AirInputs) -> Option<Air> {
    let five = scene.sky.five?;
    let w = scene.sky.flags;
    let wide = five.second_flags & 1 != 0;
    if w & (0x20 | 0x40) == 0 && !wide {
        inputs.cloud_height = 5.0;
    }
    let shift = (w & crate::sky::five::COLOR_PERSPECTIVE != 0).then_some(five.haze_shift_colour);
    let numbers =
        HazeFive { offset: five.haze_offset, thickness: five.haze_thickness, fog_offset: five.fog_offset };
    Air::new_bryce5(&inputs, numbers, wide, shift)
}

/// The sky part's numbers the air reads.,
fn air_inputs(scene: &Scene, main: &MainLight) -> AirInputs {
    let s = &scene.sky;
    AirInputs {
        switches: s.flags,
        haze_colour: s.haze_colour,
        haze: s.haze,
        fog_colour: s.fog_colour,
        fog_height: s.fog_height,
        fog: s.fog,
        cloud_height: s.cloud_height,
        light_height: main.direction[1],
    }
}

/// Bit 0x8000 of the sky's switch word: the star field.
/// A camera sample whose hit's normal is not a number: the middle of level 1 on the unit
/// scale (1.5 / 255), level 1 once scaled by 255. (Observed: level 1 on each
/// channel) (A normal that is not a number)
const NAN_NORMAL_SAMPLE: f32 = 1.5 / 255.0;
const STAR_FIELD: u32 = 0x8000;

/// A sky's star field as the render lays it (3.0 and 3.1). (Stars and comets)
#[derive(Clone, Debug)]
struct SkyStars {
    list: std::sync::Arc<Vec<FieldEntry>>,
    /// The widened grid of `list` (a speed-up only), made once per render.
    grid: std::sync::Arc<WideStarGrid>,
    laying: Laying,
}

/// The sky's star field for one render: what the rays lay, and where making the list left
/// the generator. (Where the render's draws start)
#[derive(Clone, Debug, Default)]
struct FieldMade {
    /// The field the sky lays, `None` where there is none or it does not show.
    laid: Option<SkyStars>,
    /// The generator as the list made for this render left it; `None` where this render
    /// makes no list.
    list_end: Option<CRand>,
}

/// The star field of a render: the list made where this render makes one (as 3.0 and 3.1
/// whether or not the field shows), laid where it shows; refused where leaves it
/// Open. (When there is a field, and where it shows; Errors; Catalogue stars;
/// Effects),
fn scene_stars(scene: &Scene, plan: &RenderPlan, main: &MainLight, shows: bool) -> Result<FieldMade, OpenItem> {
    let s = &scene.sky;
    let version = plan.version;
    if s.flags & STAR_FIELD == 0 || (s.star_count == 0 && s.comet_count == 0) {
        return Ok(FieldMade::default());
    }
    if plan.star_list == StarListUse::Skip {
        return Ok(FieldMade::default());
    }
    if version == RenderAs::Bryce50 {
        // Versions 2 and 7 as 5.0 lay their field by; this is reached
        // only by a sky part of version 1.
        return Err(open::STARS_FIVE);
    }
    // : a later render of the document held makes no list; one that
    // finds none makes it, whether or not the field shows (as 3.0 and 3.1 it shows only in
    // the sky with atmosphere), and leaves the generator where its last draw left it.
    let makes = plan.star_list == StarListUse::Make;
    if !shows && !makes {
        return Ok(FieldMade::default());
    }
    let list = StarList::seeded(version, s.star_seed as u32, s.star_count, s.comet_count);
    let list_end = makes.then_some(list.generator_after);
    if !shows {
        return Ok(FieldMade { laid: None, list_end });
    }
    let laying = Laying {
        version,
        sun: sun_inputs(scene, version).direction,
        haze: s.haze,
        rates: haze_rates(version, &air_inputs(scene, main)),
        haze_colour: s.haze_colour,
        orders: StarOrders::DECIDED,
    };
    let grid = std::sync::Arc::new(WideStarGrid::new(&list.entries));
    Ok(FieldMade { laid: Some(SkyStars { list: std::sync::Arc::new(list.entries), grid, laying }), list_end })
}

/// The sky's cloud layers with their textures and the tables they read.
/// (The clouds in the render)
#[derive(Clone, Debug)]
struct SceneClouds {
    render: CloudRender,
    upper: Option<TextureSettings>,
    lower: Option<TextureSettings>,
    tables: OwnedTables,
}


impl SceneClouds {
    fn textures(&self) -> CloudTextures<'_> {
        CloudTextures { upper: self.upper.as_ref(), lower: self.lower.as_ref(), tables: self.tables.view() }
    }
}

/// The sky part's bits for the clouds.
const SKY_UPPER_LAYER: u32 = 0x20;
const SKY_LOWER_LAYER: u32 = 0x40;
const SKY_EFFECTS_OFF: u32 = 0x800;

/// The clouds of a render, `None` where they neither lay a layer nor cast a shadow.
fn scene_clouds(scene: &Scene, version: RenderAs, tables: &OwnedTables) -> Result<Option<SceneClouds>, OpenItem> {
    let s = &scene.sky;
    let f = s.flags;
    // : the first reference
    if scene.clouds.stratus.is_some_and(|i| scene.textures[i].procedural().is_none()) {
        return Err(open::CLOUD_PICTURE_FIRST);
    }
    if !crate::sky::clouds::clouds_asked(version, f) {
        return Ok(None);
    }
    // (a Bryce 3D sky taken over plain has its layer bits cleared as 5.0; a
    // Bryce 5 sky's layers are refused before)
    if version == RenderAs::Bryce50 && sky_five::five_clouds_off(scene) {
        return Ok(None);
    }
    // , (the sky's
    // colour, haze and shadow tracks are applied on opening and the clouds drawn with them,
    // ): refused where the sky names a track of another
    // class (or, in a scene the reader did not make, any track).
    let bound = &scene.animation;
    if bound.sky_other_tracks > 0 || (bound.tracks.is_empty() && s.tracks.iter().any(|&t| t != 0)) {
        return Err(open::CLOUD_TRACKS);
    }
    // A layer acts with its bit; with its bit and no texture it lays nothing (Errors:
    // the clouds module's choice).
    let layer = |bit: u32, texture: Option<usize>| -> Result<Option<TextureSettings>, OpenItem> {
        let Some(i) = texture.filter(|_| f & bit != 0) else { return Ok(None) };
        let settings = &scene.textures[i].procedural().ok_or(open::CLOUD_PICTURE)?.settings;
        if !crate::textures::open_points(settings, version).is_empty() {
            return Err(open::CLOUD_TEXTURE_OPEN);
        }
        Ok(Some(settings.clone()))
    };
    let upper = layer(SKY_UPPER_LAYER, scene.clouds.stratus)?;
    let lower = layer(SKY_LOWER_LAYER, scene.clouds.cumulus)?;
    let sun = scene.sun();
    // : the translation of
    // the view's transform; there is always a view in use.
    let view = match scene.view.transform(version).given() {
        Some(t) => Some([t.translation[0], t.translation[2]]),
        None if f & crate::sky::clouds::FOLLOW_VIEW != 0 => {
            return Err(open::CLOUDS_FOLLOW_NO_VIEW);
        }
        None => None,
    };
    let more = s.as_five([0.0; 3]);
    // : where the sky's tracks were applied the two numbers are the
    // wind's (alpha the turbulence, zeta half the steps), in every version; without tracks a
    // sky as 5.0 has the numbers its part holds (0.5 and 0.5 for a Bryce 3D sky), and as 3.0 and
    // 3.1 alpha = zeta = 0, so that its clouds take no edge noise
    let (edge_amount, edge_second) = match s.wind_edge {
        Some([alpha, zeta]) => (alpha, zeta),
        None if version == RenderAs::Bryce50 => {
            (more.map_or(0.5, |m| m.cloud_detail), more.map_or(0.5, |m| m.cloud_detail_z))
        }
        None => (0.0, 0.0),
    };
    let sky = CloudSky {
        switches: f,
        cloud_height: s.cloud_height,
        cover: s.cloud_cover,
        amplitude: s.cloud_amplitude,
        frequency: s.cloud_frequency,
        offset: s.cloud_offset,
        lower_colour: s.lower_cloud_colour,
        ambient_colour: s.ambient_colour,
        horizon_colour: s.horizon_colour,
        five: CloudFive { edge_amount, edge_second, view },
    };
    let render = CloudRender::new(version, &sky, &CloudSun { direction: sun_direction(scene, version), colour: sun.colour })
        .map_err(|_| open::CLOUD_EVALUATION)?;
    Ok(Some(SceneClouds { render, upper, lower, tables: tables.clone() }))
}

/// The offset of a shadow ray's origin along the normal, and the step past a transparent
/// surface: the double nearest 0.0001.
const SHADOW_OFFSET: f64 = 0.0001;

/// What the surface rules need of a plain or blended material:'s numbers and's.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SurfaceMaterial {
    light: PlainMaterial,
    rays: RayMaterial,
    /// The material's flag word (bit 0x20 asks no shadow).
    flags: u32,
    /// A fuzzy material's Transparency: its A value and its value at the hit.
    fuzzy: Option<FuzzyTerms>,
    /// A Light-mode material of a light's own object: the shape whose glow it takes at a hit.
    /// The channels above are the document's; the mode rewrites them at each hit.
    mode: Option<OwnShape>,
}

/// The own object of a visible light whose Light-mode surface is shaded: the radial light's
/// sphere or the spot light's cone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OwnShape {
    Sphere,
    /// The cone, cube, cylinder or pyramid of a spot, parallel, cylindrical or square spot
    /// light's own object: g = 0.7.
    Cone,
    /// Any other object (not a light's own, or a sun): g = 1.
    Other,
}

/// A fuzzy material's a (the Transparency channel's A value) and t (the Transparency at the
/// hit; a for a plain material).
#[derive(Clone, Copy, Debug, PartialEq)]
struct FuzzyTerms {
    a: f64,
    t: f64,
}

impl SurfaceMaterial {
    /// A fuzzy surface lit and coloured as the same channels with Transparency 0: to
    /// read t as 0, so it sends no transmitted and no blended ray (bit 0x8
    /// beside 0x2 changes nothing) and has no angle law.
    fn fuzzy_lit(mut self) -> SurfaceMaterial {
        self.light.transparency = 0.0;
        self.rays.transparency = 0.0;
        self.rays.law_transparency = 0.0;
        self.rays.blended = false;
        self
    }
}

/// The material flag bits the render states: 0x10000 ('s plain material), 0x20 (takes
/// no shadows, step 5), 0x10 (the bump's fade,, which acts only where the
/// channels are evaluated), 0x8 (the blended material) and 0x2 (the fuzzy material,
/// ), and 0x4000 and 0x8000 (the medium colour Φ',, which change nothing else).
/// 0x2000 ('s start ray,'s Additive surface), 0x80 (shadow rays pass)
/// and 0x200000 (as 5.0 no shadow on itself), which change nothing else in the
/// shading (lists them). The volume bits 0x700 are's (`lit::volume`).
const MATERIAL_FLAGS_STATED: u32 = MATERIAL_CURRENT
    | material_flag::TAKES_NO_SHADOWS
    | crate::shading::channels::BUMP_FADES
    | MATERIAL_BLENDED
    | MATERIAL_FUZZY
    | super::media::MEDIUM_TINT_BITS
    | ADDITIVE
    | SHADOW_PASS
    | NO_SELF_SHADOW;

/// Of a plain material's other bits, those names as read by a render (0x80, 0x2000
/// , as 5.0 0x200000) whose surface rules this program does not hold for
/// a surface that is not a volume material's: refused. Every other bit changes nothing (
/// : "a render reads no bit besides these";, The media).
const PLAIN_FLAGS_UNBUILT: u32 = 0x80 | 0x2000 | 0x20_0000;

/// A guard: a volume material reaches the plain material's rules only by a fault of these
/// rules (`object_material` takes it first).
fn not_a_volume(part: &MaterialPart) -> Result<(), OpenItem> {
    if part.flags & MATERIAL_VOLUME_BITS != 0 {
        return Err(open::VOLUME_MATERIAL);
    }
    Ok(())
}

/// Which fuzzy shape a geometry shape is: a solid (the sphere, the cube, the cylinder, the cone,
/// the pyramid, the torus, the slab) or the plane (the square and the disc have no inside:
/// F = 1 - t, as the plane); `None` for the shapes leaves Open. (Applies to;
/// The weight of a solid)
fn fuzzy_shape(shape: &Shape) -> Option<FuzzyShape> {
    match shape {
        Shape::Sphere
        | Shape::Cube
        | Shape::Cylinder
        | Shape::Cone
        | Shape::Pyramid
        | Shape::Torus { .. }
        | Shape::Slab => Some(FuzzyShape::Solid),
        Shape::Tree { .. } | Shape::TreeLeaves { .. } | Shape::MetaSet => Some(FuzzyShape::Solid),
        Shape::Plane => Some(FuzzyShape::Plane),
        // Limits : a radial light's own object draws as the sphere of the same
        // size, place and material (observed: light-object-medium-pictures fz-lamp, fz-ball)
        // revised (beam-light-pictures): the other kinds' objects draw as the plain shape
        // of the same size and place (with the shape's own box)
        Shape::Light(_) => Some(FuzzyShape::Solid),
        _ => None,
    }
}

/// A fuzzy surface's kind for a shape of the scene, where it encloses a volume or not: a mesh does
///, a terrain where a closing face is on (then it is as a plane: F = 1 - t).
fn fuzzy_kind(shape: &Shape, encloses: bool) -> Option<FuzzyShape> {
    match shape {
        Shape::StoredMesh(_) => Some(FuzzyShape::Solid),
        Shape::Terrain(_) => Some(if encloses { FuzzyShape::Solid } else { FuzzyShape::Plane }),
        // : a set led by a fuzzy material, measured in the identity space of the
        // object standing for every set, with its own box the cube -1 to 1 (`fuzzy_box_scale`)
        Shape::MetaSet => Some(FuzzyShape::Solid),
        other => fuzzy_shape(other),
    }
}

/// The scale on each axis that makes the shape's own box measure 2: the cube and the pyramid
/// are -0.5 to 0.5, the cylinder -1 to 1, the slab -10 to 10 in x and z and -1 to 1 in y, the
/// torus -(R + r) to R + r in x and z and -r to r in y, the sphere and the cone -1 to 1; a mesh
/// its stored box, a terrain its own box .
fn fuzzy_box_scale(scene: &crate::geometry::Scene, shape: &Shape) -> Vec3 {
    let scale = |h: [f64; 3]| -> Vec3 { std::array::from_fn(|k| fuzzy::box_scale(-h[k], h[k])) };
    match shape {
        Shape::StoredMesh(k) => {
            let b = scene.stored_meshes[*k].bounds;
            std::array::from_fn(|a| fuzzy::box_scale(b[a], b[3 + a]))
        }
        Shape::Terrain(k) => match scene.terrains.get(*k) {
            Some(t) => {
                let (lo, hi) = t.own_box();
                std::array::from_fn(|a| fuzzy::box_scale(lo[a], hi[a]))
            }
            None => scale([1.0; 3]),
        },
        Shape::Light(l) => fuzzy_box_scale(scene, &l.plain()),
        Shape::Cube | Shape::Pyramid => scale([0.5; 3]),
        Shape::Slab => scale([10.0, 1.0, 10.0]),
        Shape::Torus { tube, ring } => scale([ring + tube, *tube, ring + tube]),
        // : a shape object's own box is its shape's box ; the stand-in's
        // the cube -1 to 1 (the default below).
        Shape::TreeLeaves { model, shape } => {
            let b = scene.trees[*model].shape_box(*shape);
            std::array::from_fn(|a| fuzzy::box_scale(b[a], b[3 + a]))
        }
        _ => scale([1.0; 3]),
    }
}

/// World point `p` in the own space of geometry object `object` for the weight of a fuzzy solid:
/// carried by the inverse of its map, except the stand-in (a tree's wood, a set) and a shape
/// object, whose map is the identity: the world point itself. (The weight of a
/// solid), (measured in the stand-in's space, d the distance between the two world
/// points; a shape object's map the identity)
fn fuzzy_own(scene: &crate::geometry::Scene, object: usize, p: Vec3) -> Vec3 {
    let o = &scene.objects[object];
    match o.shape {
        Shape::Tree { .. } | Shape::TreeLeaves { .. } | Shape::MetaSet => p,
        _ => fuzzy::to_own_space(&o.placement.inverse_rows(), o.placement.translation(), p),
    }
}

/// A fuzzy surface's shape kind. (a sphere or a cone: the weight of a solid; a
/// plane: 1 − t)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FuzzyShape {
    Solid,
    Plane,
}

/// The blended material's bit (flag word 0x10008 with the plain bit). (The
/// blended material, 0x8 [format])
const MATERIAL_BLENDED: u32 = 0x8;

/// A document object's material: plain, or one whose channels are evaluated at each hit.
#[derive(Clone, Debug, PartialEq)]
enum ObjectMaterial {
    Plain(SurfaceMaterial),
    Textured(Box<TexturedMaterial>),
    /// A volume material : its surface is never shaded.
    Volume(Box<VolumeMaterial>),
}

/// The material of document object `object`. A material whose channels are evaluated is
/// prepared for its textures; on a light's own object it is refused (how such an object is
/// shaded is Open).; (Textured materials in the render)
fn object_material(scene: &Scene, version: RenderAs, object: usize) -> Result<ObjectMaterial, OpenItem> {
    let m = scene.objects[object].material.ok_or(open::NO_MATERIAL)?;
    let part = &scene.materials[m].part;
    if let Some(v) = volume::volume_material(scene, version, object) {
        return Ok(ObjectMaterial::Volume(Box::new(v?)));
    }
    // : bit 0x4 and no volume bit is the Light mode; no other bit plays a part
    // A material whose channels have textures is evaluated at each hit and rewritten there
    // (Inputs: after's evaluation); a light's own object with textures is refused.
    let own = matches!(scene.objects[object].body, Body::RadialLight(_));
    let textured_mode = crate::shading::visible::is_light_mode(part.flags)
        && part.flags & MATERIAL_FUZZY == 0
        && !own
        && channels_evaluated(part.channel_words(), scene.settings.switches);
    if crate::shading::visible::is_light_mode(part.flags) && !textured_mode {
        return light_mode_material(scene, version, object, part).map(ObjectMaterial::Plain);
    }
    // (Applies to: a sphere, a cone or a plane; a light's own object, a group or a
    // mesh is not one of them).
    // revised: gives the weight of the mesh and the terrain, and of a group's member.
    if part.flags & MATERIAL_FUZZY != 0
        && !matches!(scene.objects[object].body, Body::Shape(_) | Body::MeshResource(_) | Body::Terrain(_) | Body::Metaball { .. } | Body::Tree(_) | Body::TreeLeaves(_) | Body::RadialLight(_))
    {
        return Err(open::FUZZY_SHAPE);
    }
    if part.flags & MATERIAL_FUZZY != 0 && part.flags & super::media::MEDIUM_TINT_BITS != 0 {
        return Err(open::FUZZY_TINT);
    }
    // : a terrain is a solid that holds a ray where a closing face is on, and is as
    // a plane where none is (`encloses_in`); its material needs no other rule.
    if !channels_evaluated(part.channel_words(), scene.settings.switches) {
        return Ok(ObjectMaterial::Plain(plain_material(part, scene.settings.switches)?));
    }
    if part.flags & MATERIAL_CURRENT == 0 || (part.flags & !MATERIAL_FLAGS_STATED != 0 && !textured_mode) {
        return Err(open::TEXTURED_MATERIAL);
    }
    if matches!(scene.objects[object].body, Body::RadialLight(_)) {
        return Err(textured::open::TEXTURED_LIGHT_OBJECT);
    }
    let t = TexturedMaterial::new(scene, m, version)?;
    Ok(ObjectMaterial::Textured(Box::new(t)))
}

/// What shading reads of a textured material at a hit:'s channels as's and
/// 's numbers, the refraction value and the angle law's Transparency the material's own
/// (A value). A blended one that sends rays is Open. (Inputs: the channels as
/// gives them, the values doubles),
fn textured_surface(
    at: &crate::shading::channels::MaterialAtHit,
    material: &TexturedMaterial,
) -> Result<SurfaceMaterial, OpenItem> {
    let flags = material.flags();
    let fuzzy = (flags & MATERIAL_FUZZY != 0).then(|| FuzzyTerms { a: material.transparency_a(), t: at.transparency });
    let mode = crate::shading::visible::is_light_mode(flags);
    // : the blended material with the channels
    // and the normal at the hit; eta counts as 1 and there is no angle law (RayMaterial::blended).
    let light = PlainMaterial {
        diffuse: at.diffuse,
        diffusion: at.diffusion,
        ambient: at.ambient,
        ambience: at.ambience,
        specular: at.specular,
        specularity: at.specularity,
        halo: at.specular_halo,
        transparency: at.transparency,
        metallicity: at.metallicity,
    };
    // (Materials whose channels have textures: every channel's at the hit;
    // η and Φ the material's own; the angle law from the A value of Transparency)
    let rays = RayMaterial {
        diffuse: at.diffuse,
        transparent_colour: at.transparent_colour,
        halo: at.specular_halo,
        transparency: at.transparency,
        law_transparency: material.transparency_a(),
        reflection: at.reflection,
        metallicity: at.metallicity,
        refraction: material.refraction(),
        blended: flags & MATERIAL_BLENDED != 0,
    };
    // : a Light-mode ball with textures; not a light's own object (refused before)
    Ok(SurfaceMaterial { light, rays, flags, fuzzy, mode: mode.then_some(OwnShape::Other) })
}

/// The single numbers of a plain or blended material, or why it cannot be shaded.
/// (Inputs: plain or blended,
/// any refraction value)
fn plain_material(part: &MaterialPart, scene_switches: u32) -> Result<SurfaceMaterial, OpenItem> {
    not_a_volume(part)?;
    if part.flags & MATERIAL_CURRENT == 0 || part.flags & !MATERIAL_FLAGS_STATED & PLAIN_FLAGS_UNBUILT != 0 {
        return Err(open::TEXTURED_MATERIAL);
    }
    // : else every channel is its A value.
    if channels_evaluated(part.channel_words(), scene_switches) {
        return Err(open::TEXTURE_MAPPING);
    }
    let (light, rays) = plain_channels(part);
    let v = part.values.map(|c| c.a);
    let fuzzy = (part.flags & MATERIAL_FUZZY != 0).then(|| FuzzyTerms { a: f64::from(v[3]), t: f64::from(v[3]) });
    Ok(SurfaceMaterial { light, rays, flags: part.flags, fuzzy, mode: None })
}

/// A plain material's A values as's and's numbers.
fn plain_channels(part: &MaterialPart) -> (PlainMaterial, RayMaterial) {
    let [diffuse, ambient, specular, halo, transparent] = part.colours.map(|c| c.a);
    let v = part.values.map(|c| c.a);
    let light = PlainMaterial {
        diffuse,
        diffusion: f64::from(v[0]),
        ambient,
        ambience: f64::from(v[1]),
        specular,
        specularity: f64::from(v[2]),
        halo,
        transparency: f64::from(v[3]),
        metallicity: f64::from(v[6]),
    };
    let rays = RayMaterial {
        diffuse,
        transparent_colour: transparent,
        transparency: f64::from(v[3]),
        law_transparency: f64::from(v[3]),
        reflection: f64::from(v[4]),
        metallicity: f64::from(v[6]),
        halo,
        refraction: part.refraction,
        blended: part.flags & MATERIAL_BLENDED != 0,
    };
    (light, rays)
}

/// The material of a light's own object in the Light mode (bit 0x4, no volume bit), with its
/// channels as the document holds them; `begin_surface` rewrites them at each hit. A radial
/// light's object is the sphere, the other kinds' the cone (g 0.7); any other object takes g = 1.
fn light_mode_material(scene: &Scene, version: RenderAs, object: usize, part: &MaterialPart) -> Result<SurfaceMaterial, OpenItem> {
    use crate::scene::LightKind;
    let shape = match scene.objects[object].body {
        Body::RadialLight(k) => match &scene.lights[k] {
            Light::Radial(r) => match r.kind() {
                LightKind::Radial => OwnShape::Sphere,
                LightKind::Spot | LightKind::Parallel | LightKind::Cylinder | LightKind::Square => OwnShape::Cone,
            },
            Light::Sun(_) => OwnShape::Other,
        },
        _ => OwnShape::Other,
    };
    // : with bit 0x2 a Light-mode material as any, the fuzzy bit playing
    // no part (no fuzzy weight, no fuzzy share; `plain_channels` gives no fuzzy terms).
    if channels_evaluated(part.channel_words(), scene.settings.switches) {
        return Err(open::LIGHT_MODE_BITS);
    }
    let _ = version;
    let (light, rays) = plain_channels(part);
    Ok(SurfaceMaterial { light, rays, flags: part.flags, fuzzy: None, mode: Some(shape) })
}

/// A textured material's channels at hit `h` on a geometry object, `distance`'s
/// distance for the fade. (the texture point from the object's own box
/// and placement),
fn textured_at(
    t: &TexturedMaterial,
    version: RenderAs,
    geometry: &GeometryBuild,
    h: &Hit,
    distance: f64,
    switches: u32,
    tables: TextureTables<'_>,
) -> Result<MaterialAtHit, OpenItem> {
    // : placed by the surface member, the
    // material by the carried member
    let o = &geometry.scene.objects[h.texture_object.unwrap_or(h.object)];
    let (low, high) = match o.shape {
        Shape::Terrain(k) => geometry.scene.terrains[k].own_box(),
        Shape::StoredMesh(k) => geometry.scene.stored_meshes[k].own_box(),
        // (a leaf is reported for the object of its shape, whose own box is the shape's box,
        // ; the wood for the stand-in, whose own box is -1 to 1). Without this the mapping of a leaf's
        // texture (object space) read a box of no numbers.
        Shape::TreeLeaves { model, shape } => {
            let b = geometry.scene.trees[model].shape_box(shape);
            ([b[0], b[1], b[2]], [b[3], b[4], b[5]])
        }
        Shape::Tree { .. } | Shape::MetaSet => ([-1.0; 3], [1.0; 3]),
        _ => own_box(&o.shape),
    };
    let object = MapObject { rows: o.placement.rows(), low, high, sphere: matches!(o.shape.plain(), Shape::Sphere) };
    // A document mesh reports its shading normal here ("the normal that shading
    // reads"; our reading for); every other shape's is its normal.
    let hit = MapHit { world: h.point, object: h.local_point, normal: h.shading_normal, u: h.uv[0], v: h.uv[1], du: h.du, dv: h.dv };
    // The scene's top is read only by a mix by height, which is refused before.
    let ch = ChannelHit { height: h.point[1], top: f64::NAN, normal: h.shading_normal, distance };
    t.at_hit(version, &object, &hit, &ch, switches, tables)
}

/// Whether a shape encloses a volume: the sphere, the cube, the cylinder, the cone, the
/// torus, the slab and the pyramid do; the plane, the square and the disc do not.
fn encloses_volume(shape: &Shape) -> bool {
    match shape {
        Shape::Sphere | Shape::Cube | Shape::Cylinder | Shape::Cone | Shape::Torus { .. } => true,
        Shape::Slab | Shape::Pyramid => true, // Inputs
        Shape::Plane | Shape::Square | Shape::Disc => false, // Inputs
        // A mesh encloses no modelled volume (as `geometry::interior`).
        Shape::Mesh(_) => false,
        // : a path is a thin line, not a volume.
        Shape::Path(_) => false,
        // Inputs (every mesh, closed or not, either sign of its normals)
        Shape::StoredMesh(_) => true,
        // point 1: a terrain encloses a volume where a closing face is on
        // (`encloses_in` asks the terrain); a group does whatever its members.
        // (Solid media)
        Shape::Terrain(_) => true,
        Shape::Group(_) => true,
        // : the stand-in encloses a volume, a leaves' object does not (the media
        // of trees: wiring pending, handoff-trees.md).
        Shape::Tree { .. } => true,
        Shape::TreeLeaves { .. } => true,
        // : the object that stands for every set encloses a volume; a metaball
        // of the document is never the object a hit names.
        Shape::MetaSet => true,
        Shape::Metaball(_) => false,
        // : a light's object is the plain shape of its kind.
        Shape::Light(l) => encloses_volume(&l.plain()),
    }
}

/// Whether the geometry object of shape `shape` encloses a volume: `encloses_volume`, except a
/// terrain, which does where a closing face is on (as 3.0 the word 0x40 is no face).
fn encloses_in(scene: &crate::geometry::Scene, shape: &Shape) -> bool {
    match shape {
        Shape::Terrain(k) => scene.terrains.get(*k).is_some_and(|r| r.closed()),
        Shape::Light(l) => encloses_in(scene, &l.plain()),
        other => encloses_volume(other),
    }
}

/// For each geometry object the object whose surface a ray crosses where it hits it: itself,
/// except a member of a combined group, whose crossing is a crossing of the surface of the
/// combined solid (the group that no other group holds). Also, per object, whether it is
/// held by a group that another group holds (a group inside a group).
/// (Outputs, point 2: "for a group the surface of the combined solid,, not
/// a member's own surface"; Limits: a member that is itself a group is Open)
fn solid_identities(scene: &crate::geometry::Scene) -> (Vec<usize>, Vec<bool>) {
    use crate::geometry::group::Source;
    let n = scene.objects.len();
    let combinations = &scene.groups.combinations;
    let mut member_of: Vec<Option<usize>> = vec![None; n];
    let mut inside: Vec<Option<usize>> = vec![None; combinations.len()];
    for (k, c) in combinations.iter().enumerate() {
        for m in &c.members {
            match m.source {
                Source::Object(i) => member_of[i] = Some(k),
                Source::Group(j) => inside[j] = Some(k),
                Source::Hidden(_) => {}
            }
        }
    }
    let mut group_object = vec![None; combinations.len()];
    for (g, o) in scene.objects.iter().enumerate() {
        if let Shape::Group(k) = o.shape {
            group_object[k] = Some(g);
        }
    }
    let mut solid = (0..n).collect::<Vec<_>>();
    let mut nested = vec![false; n];
    for i in 0..n {
        let Some(mut k) = member_of[i] else { continue };
        nested[i] = inside[k].is_some();
        while let Some(outer) = inside[k] {
            k = outer;
        }
        if let Some(g) = group_object[k] {
            solid[i] = g;
        }
    }
    (solid, nested)
}

/// The numbers of that a hit on a set shows, as a plain material's part (flag word 0x10000,
/// channels without textures).
fn made_part(m: &crate::geometry::metaball::MixInput) -> MaterialPart {
    let channel = |a: [f32; 3]| crate::scene_parts::Channel { refs: [None, None, None], flags: 0, a, b: a, alpha: 1.0 };
    let value = |a: f32| crate::scene_parts::Channel { refs: [None, None, None], flags: 0, a, b: a, alpha: 1.0 };
    MaterialPart {
        flags: m.flags,
        colours: m.colours.map(channel),
        values: m.values.map(value),
        volume_colour: m.volume_colour,
        refraction: m.refraction,
        volume_numbers: m.volume_numbers,
        tracks: [0; 19],
    }
}

/// What the render reads of the one object that stands for every set of metaballs: the hit's
/// own showing (`meta`), or the latest recorded hit's where none is given (what is asked of the
/// object after the hit). `None` where the object is not the standing one (or nothing was
/// recorded). A hit on a set led by a material shown as it is (fuzzy, Light mode, a volume) is
/// Open.
fn standing(geometry: &GeometryBuild, object: usize, meta: Option<u32>) -> Option<Standing> {
    use crate::geometry::metaball::Showing;
    let ms = geometry.scene.metaballs.as_ref().filter(|m| m.stand == object)?;
    let showing = match meta {
        Some(id) => ms.showing(id),
        None => ms.latest()?,
    };
    Some(match showing {
        Showing::Made(m) => Standing::Set(Ok(m)),
        // (observed as 5.0: metaball-medium-pictures vm-01 to vm-04)
        // Limits : a set led by the Light mode (bit 0x4 [format]), with or
        // without bit 0x2, shows its own material as any object's (observed: vm-06 to vm-08)
        Showing::Own(ball) => Standing::Own(ms.object_of[ball]),
        // : the wood material of the tree that handed it over.
        Showing::Wood(tree) => Standing::Wood(tree),
    })
}

/// What the object standing for every set and every tree's wood shows: a set's made material,
/// or a tree's wood material (the standing object is handed either).
enum Standing {
    /// The material made for a hit on a set, or Open where the set is led by a material shown as
    /// it is (fuzzy, Light mode, a volume).
    Set(Result<crate::geometry::metaball::MixInput, OpenItem>),
    /// The leader's own material, as it is (a fuzzy, Light-mode, volume material): that of the
    /// metaball whose geometry object this is.
    Own(usize),
    /// The wood material of the tree whose geometry object this is.
    Wood(usize),
}

/// The material entry (by document object) a hit on geometry object `object`, or the latest
/// showing where `meta` is `None`, reads: a set's own leader's or a tree's wood for the object
/// standing for every set; the object's own otherwise. `None` where it shows a made material.
/// point 3
pub(super) fn material_entry(geometry: &GeometryBuild, object: usize, meta: Option<u32>) -> Option<usize> {
    match standing(geometry, object, meta) {
        None => Some(geometry.scene_object[object]),
        Some(Standing::Set(_)) => None,
        Some(Standing::Own(o) | Standing::Wood(o)) => Some(geometry.scene_object[o]),
    }
}

/// What a hit counts for in the angle law's state: the document object whose surface it is
/// (a hit on the wood reported for the shared stand-in counts for its tree), and whether the
/// material is one made for the hit (a set's), which waits for nothing. `None` for an object
/// without a material.,
fn material_slot(material_of: &[Option<usize>], geometry: &GeometryBuild, object: usize, meta: Option<u32>) -> Option<(usize, bool)> {
    let stand = geometry.scene.metaballs.as_ref().map(|m| m.stand);
    let made = |o: usize| material_of[o].map(|_| (geometry.scene_object[o], stand == Some(o)));
    match standing(geometry, object, meta) {
        Some(Standing::Wood(tree) | Standing::Own(tree)) => made(tree),
        _ => made(object),
    }
}

/// The shadow surface of geometry object `object` as a hit or the latest recorded hit shows it
/// (the object standing for every set); the table's own entry for every other object.
fn surface_of(
    table: &[Result<ShadowSurface, OpenItem>],
    geometry: &GeometryBuild,
    object: usize,
    meta: Option<u32>,
    switches: u32,
    version: RenderAs,
) -> Result<ShadowSurface, OpenItem> {
    match standing(geometry, object, meta) {
        None => table[object],
        Some(Standing::Wood(tree) | Standing::Own(tree)) => table[tree],
        Some(Standing::Set(Err(e))) => Err(e),
        Some(Standing::Set(Ok(m))) => shadow_surface(Some(&made_part(&m)), &Shape::MetaSet, false, true, switches, version),
    }
}

/// What a shadow ray reads of a surface it meets: Transparency, the Transparent colour, the
/// refraction value (the angle law), whether the material is blended, and for a shape that
/// encloses a volume its medium. Where the channels are evaluated, Transparency and the
/// colour are their A values (the angle law's), and's at each hit replace them.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ShadowSurface {
    transparency: f32,
    colour: [f32; 3],
    /// Whether the material's channels are evaluated at the shadow ray's hit .
    textured: bool,
    refraction: f32,
    blended: bool,
    /// The solid's medium; `None` for a shape that encloses no volume.
    medium: Option<MediumLaw>,
    /// A fuzzy surface: its shape kind (its a is `transparency`).
    fuzzy: Option<FuzzyShape>,
    /// A volume material's surface: its flag word (bits 0x2000 and 0x80 make its medium none
    /// for a shadow ray).
    volume: Option<u32>,
    /// The medium colour Φ' in place of the medium's Φ.
    tint: Option<MediumTint>,
    /// The flag word a shadow ray reads at the surface: the material's, or the Light mode's
    /// replaced word (0x00a0, 0x20a8, 0x2020a8 by version). Its bits 0x80, 0x200000 (as 5.0) and
    /// 0x2000 decide whether the surface counts.,
    word: u32,
    /// The material's own flag word, which a shadow ray's media read (bits 0x80 and 0x2000: no
    /// medium).,
    own_word: u32,
}

/// Bit 0x80 of a flag word: Cast Shadows off; a shadow ray passes the surface.
const SHADOW_PASS: u32 = 0x80;
/// As 5.0, bit 0x200000: the object casts no shadow on itself.
const NO_SELF_SHADOW: u32 = 0x20_0000;
/// Bit 0x2000, the Additive modifier: the surface counts only on the first stretch leaving the
/// object; and's start ray.
const ADDITIVE: u32 = 0x2000;
/// As 5.0, the start ray is sent only while the rays the path has gone through before the
/// surface are below this.,
const START_RAY_LIMIT_FIVE: u32 = 100;
/// The stretch number from which a shadow ray goes no further after a surface that counts.
const SHADOW_STRETCH_BOUND: u32 = 100;

/// The Light mode's replaced flag word by version. (Outputs: 0x00a0 as 3.0, 0x20a8
/// as 3.1, 0x2020a8 as 5.0)
fn light_mode_word(version: RenderAs) -> u32 {
    match version {
        RenderAs::Bryce30 => 0x00a0,
        RenderAs::Bryce31 => 0x20a8,
        RenderAs::Bryce50 => 0x2020a8,
    }
}

/// What a shadow ray reads of the surfaces of a geometry object of shape `shape` with the
/// material `part`; `shape_object` whether the document object is a shape (not a light's own
/// object, a group or a mesh).,
fn shadow_surface(
    part: Option<&MaterialPart>,
    shape: &Shape,
    shape_object: bool,
    encloses: bool,
    scene_switches: u32,
    version: RenderAs,
) -> Result<ShadowSurface, OpenItem> {
    let part = part.ok_or(open::NO_MATERIAL)?;
    // (Shadow rays: a volume material's surface takes no share; its solid's medium
    // is marched, F = 1 and Φ')
    if let Some(v) = volume::shadow_surface(part, shape, shape_object, encloses, version) {
        return v;
    }
    if part.flags & MATERIAL_FUZZY != 0 && part.flags & super::media::MEDIUM_TINT_BITS != 0 {
        return Err(open::FUZZY_TINT);
    }
    let (t, c) = (&part.values[3], &part.colours[4]);
    // : a Light-mode surface of a light's own object; the shadow ray
    // passes it as if it were not there, and a solid's medium is the blended one's as 3.1 and
    // 5.0 (a blended surface), a plain one's as 3.0.
    if crate::shading::visible::is_light_mode(part.flags) {
        // (Outputs: bit 0x2 plays no
        // part: the medium read from the material's own word in every version): the
        // material's bit 0x8 plays no part in the surface and makes the medium's Φ white. The
        // refraction value counts as 1 where a ray crosses the surface as a medium's boundary.
        let blended = version != RenderAs::Bryce30;
        let white_medium = part.flags & MATERIAL_BLENDED != 0;
        return Ok(ShadowSurface {
            transparency: t.a,
            colour: c.a,
            textured: false,
            refraction: part.refraction,
            blended,
            medium: encloses.then(|| MediumLaw::of_solid(part.volume_colour, 1.0, white_medium)),
            fuzzy: None,
            volume: None,
            tint: None,
            word: light_mode_word(version),
            own_word: part.flags,
        });
    }
    // : where the channels are not evaluated, every channel is its A value; where
    // they are, evaluates them at the shadow ray's hit.
    let textured = channels_evaluated(part.channel_words(), scene_switches);
    let fuzzy = if part.flags & MATERIAL_FUZZY != 0 {
        if !shape_object && !matches!(shape, Shape::StoredMesh(_) | Shape::Terrain(_) | Shape::Tree { .. } | Shape::TreeLeaves { .. } | Shape::MetaSet) && fuzzy_shape(shape).is_none() {
            return Err(open::FUZZY_SHAPE);
        }
        Some(fuzzy_kind(shape, encloses).ok_or(open::FUZZY_SHAPE)?)
    } else {
        None
    };
    // (Applies to: beside bit 0x2 the blended bit changes nothing; a shadow ray's
    // share is the fuzzy one)
    // : its share t is the Transparency channel
    // at the hit, with no angle law; a shadow ray reads it as `surface_share` does for any
    // textured surface.
    let blended = fuzzy.is_none() && part.flags & MATERIAL_BLENDED != 0;
    // (The medium of a fuzzy solid): a fuzzy solid's Φ and η are its own as a
    // plain material's (Φ white where the material is also blended, bit 0x8), its factor F 1.
    let medium = encloses.then(|| match fuzzy {
        Some(_) => MediumLaw::of_fuzzy_solid(part.volume_colour, part.refraction, part.flags & MATERIAL_BLENDED != 0),
        None => MediumLaw::of_solid(part.volume_colour, part.refraction, blended),
    });
    // (The medium colour: a plain material's flag word with 0x4000 or 0x8000; bit 8,
    // the blended material's, white)
    let tint = if blended { None } else { MediumTint::of(part.flags, super::media::extent(shape)) };
    Ok(ShadowSurface {
        transparency: t.a,
        colour: c.a,
        textured,
        refraction: part.refraction,
        blended,
        medium,
        fuzzy,
        volume: None,
        tint,
        word: part.flags,
        own_word: part.flags,
    })
}

/// Where one shadow ray went into each fuzzy solid it met (its latest hit on that solid from
/// outside), a few at a time.; (Fuzzy solids)
#[derive(Clone, Copy, Debug, Default)]
struct FuzzyEntries {
    slots: [(usize, Vec3); FUZZY_ENTRIES],
    len: usize,
}

/// (as many as a path may hold solids, twice over)
const FUZZY_ENTRIES: usize = 8;

impl FuzzyEntries {
    /// Records `point` as where the ray went into `object`; false when no slot is left.
    fn set(&mut self, object: usize, point: Vec3) -> bool {
        if let Some(s) = self.slots[..self.len].iter_mut().find(|s| s.0 == object) {
            s.1 = point;
            return true;
        }
        if self.len == FUZZY_ENTRIES {
            return false;
        }
        self.slots[self.len] = (object, point);
        self.len += 1;
        true
    }

    fn get(&self, object: usize) -> Option<Vec3> {
        self.slots[..self.len].iter().find(|s| s.0 == object).map(|s| s.1)
    }
}

/// The medium law of a path in the solid whose shadow surface is `s`: its law, or the Open
/// subject of a fuzzy solid's medium.; (Fuzzy solids)
fn law_of(s: &Result<ShadowSurface, OpenItem>) -> Result<Option<MediumLaw>, OpenItem> {
    Ok((*s)?.medium)
}

/// Where a surface's shadow rays start, in the media. (The media: in the medium
/// of the ray whose hit p is, except as 3.1 and 5.0 where the object hit encloses a volume
/// and v·g' < 0: on the surface's far side)
#[derive(Clone, Copy, Debug)]
struct ShadowStart {
    /// The medium of the ray whose hit is lit.
    incoming: PathMedia,
    /// The media on the surface's far side (`None` for a shape that encloses no volume), or
    /// the Open case that keeps them from being known.
    far: Option<Result<PathMedia, OpenItem>>,
    /// v, toward the eye.
    toward_eye: Vec3,
    /// The geometry object of the lit point, the one the ray the first stretch is sent from met
    /// (`None` for a point in the air or a volume's sample).
    object: Option<usize>,
}

/// The shadow rays of, through the render's geometry. (Shadow rays)
struct ShadowTracer<'a> {
    version: RenderAs,
    /// The depth of the ray whose point the shadow ray lights (0 for a point in the air): the
    /// depth of every stretch's march as 5.0.
    ray_depth: u32,
    start: ShadowStart,
    geometry: &'a GeometryBuild,
    ctx: &'a ShapeContext,
    /// Per geometry object.
    surfaces: &'a [Result<ShadowSurface, OpenItem>],
    /// Per document object: the materials whose channels are evaluated at a hit.
    materials: &'a [Result<ObjectMaterial, OpenItem>],
    /// Per geometry object: the object whose surface a hit on it crosses (itself, or for a member
    /// of a combined group the group).
    solid_of: &'a [usize],
    /// Per geometry object: the index of its material in the document, if any.
    material_of: &'a [Option<usize>],
    /// The materials met so far (the angle law's state).
    met: &'a MetMaterials,
    tables: TextureTables<'a>,
    /// The scene's switch word.
    switches: u32,
    transmission: bool,
    /// The clouds a shadow ray that meets nothing passes.
    clouds: Option<&'a SceneClouds>,
    /// Per light, its radius ρ (0 for none).
    radii: &'a [(f64, bool)],
    /// The render's generator while a surface or a march is shaded, for a jittered volume's
    /// sample places; `None` where the tracer has none (the light in the air).
    generator: Option<&'a Cell<CRand>>,
    /// Where the shadow is asked of a surface's point (a hit of the tracer): what its soft
    /// shadows need; `None` elsewhere (the light in the air, a volume's march), where a shadow
    /// of a light with a radius is Open.
    soft: Option<SoftAt<'a>>,
    /// The first Open subject met, reported after the sample.
    error: Option<OpenItem>,
}

/// What the soft shadows at a surface's point need: the generator lent for the surface, the
/// sample number the ray carries (with the rays a side), and the shadow grid n_s.
/// ; (Bryce 5's lights: in the render)
#[derive(Clone, Copy)]
struct SoftAt<'a> {
    generator: &'a Cell<CRand>,
    sample: Option<(u32, u32)>,
    grid: u32,
}

impl SoftAt<'_> {
    /// The generator's next two draws, the first drawn first.,
    fn two(&self) -> [u16; 2] {
        let mut g = self.generator.get();
        let d = [g.next_u15(), g.next_u15()];
        self.generator.set(g);
        d
    }
}

const FULL_SHADOW: Shadow = Shadow { colour: [0.0; 3], amount: 0.0 };

/// A shadow's colour times a colour, one binary32 product per channel.
fn tint(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|k| (f64::from(a[k]) * f64::from(b[k])) as f32)
}

/// Which of the document's objects have been met (the angle law's state, "a material met
/// before", kept for each object: two objects that name one material are met apart): none when
/// the document has just been read; every surface any ray meets (a camera ray, a further ray or
/// a shadow ray, of any render of the document, the quick preview included) makes its object
/// met for every later ray and render. Shared by the surface and sky rules of a render and of
/// the preview before it (its state is behind a lock; the multithreaded render gives each worker its own). (The angle law, revised
/// , to; kept for each object
/// point 7),
#[derive(Clone, Debug, Default)]
pub struct MetMaterials(Arc<Mutex<(Vec<bool>, Vec<bool>)>>);

impl MetMaterials {
    /// None met, for a document of `count` materials.
    pub fn none(count: usize) -> MetMaterials {
        MetMaterials(Arc::new(Mutex::new((vec![false; count], vec![false; count]))))
    }

    /// Which objects are met now (for the multithreaded render, which hands each chunk of the
    /// picture the state the serial order would have had).
    pub(super) fn snapshot(&self) -> Vec<bool> {
        self.0.lock().unwrap().0.clone()
    }

    /// Make `met` the state (and forget which objects were asked about).
    pub(super) fn load(&self, met: &[bool]) {
        let mut s = self.0.lock().unwrap();
        s.0.clear();
        s.0.extend_from_slice(met);
        s.1.clear();
        s.1.resize(met.len(), false);
    }

    /// The objects `meet` was asked about since `load` (each is met now).
    pub(super) fn asked(&self) -> Vec<bool> {
        self.0.lock().unwrap().1.clone()
    }

    /// A ray meets a surface of the object `slot.0` (see `material_slot`): whether the object
    /// had been met before (that surface is shaded with that state); it is met from now on. An
    /// object without a material (refused where it is shaded) meets nothing here.
    pub fn meet(&self, slot: Option<(usize, bool)>) -> bool {
        let Some((o, made)) = slot else { return false };
        // ,: the material a hit on a
        // set shows is made for that hit and never waits to be met: its law, where it has one,
        // is in force at every hit, the first included, and such a hit makes nothing met. A hit
        // that shows a member's own material keeps's state of that member's material
        // ('s fallback): `material_slot` names the member, not made.
        // (The stand-in's slot)
        if made {
            return true;
        }
        let mut s = self.0.lock().unwrap();
        let (met, asked) = &mut *s;
        if o >= met.len() {
            met.resize(o + 1, false);
        }
        if o >= asked.len() {
            asked.resize(o + 1, false);
        }
        asked[o] = true;
        std::mem::replace(&mut met[o], true)
    }
}

impl ShadowTracer<'_> {
    /// The channels of the textured material of the object a shadow ray meets, at its hit.
    fn textured_at(&self, hit: &Hit, distance: f64) -> Result<MaterialAtHit, OpenItem> {
        // : a hit on the stand-in shows the wood (or set) the latest hand-over left
        let object = material_entry(self.geometry, hit.object, hit.meta).unwrap_or(self.geometry.scene_object[hit.object]);
        match &self.materials[object] {
            Err(e) => Err(*e),
            Ok(ObjectMaterial::Textured(t)) => {
                textured_at(t, self.version, &self.geometry, hit, distance, self.switches, self.tables)
            }
            Ok(ObjectMaterial::Plain(_) | ObjectMaterial::Volume(_)) => {
                unreachable!("a shadow surface is textured only where its channels are evaluated")
            }
        }
    }

    /// The medium of a shadow ray's stretch in `media` that met `hit` (its parameter from the
    /// stretch's start and its point) or nothing: its law with Φ' (The medium colour);
    /// `None` outside every object, and in a volume material's solid whose flag word has bit
    /// 0x2000 or 0x80 (as a stretch outside every object).,
    /// (Shadow rays; The medium colour)
    fn stretch_medium(&self, media: &PathMedia, hit: Option<(f64, Vec3)>) -> Result<Option<MediumLaw>, OpenItem> {
        let Some(i) = media.current_material() else { return Ok(None) };
        let s = surface_of(self.surfaces, self.geometry, i, None, self.switches, self.version);
        let Some(law) = law_of(&s)? else { return Ok(None) };
        let s = s?;
        // (The media: for a shadow ray, a solid whose material has bit 0x2000 or 0x80
        // is no medium, for any material),
        if volume::no_shadow_medium(s.own_word) {
            return Ok(None);
        }
        super::media::tinted(self.version, law, s.tint, &self.geometry.scene.objects[i].placement, hit).map(Some)
    }
}

/// A shadow ray's amount, with the factors from beyond the first volume material's surface it
/// went on through kept apart., (Shadow rays: "Where what the
/// shadow ray gives from beyond that surface has the amount 0, the result is the colour
/// (0, 0, 0) and the amount 0")
#[derive(Clone, Debug)]
struct Amount {
    shadow: Shadow,
    /// The product of the factors since the first volume material's surface.
    beyond: Option<f64>,
    /// Per stretch, first stretch first: the march's (1 - alpha) (1 where the stretch is not
    /// marched) and the share of the surface that ends it (already times the medium's factor;
    /// 1 for a volume material's surface or where it ends in nothing). The amount is formed from
    /// them last stretch first.
    stretches: Vec<(f64, f64)>,
    /// The colour factors, the lit point's end first: each counted surface's ck = ⟨Θk·Φk⟩, and
    /// the Φ of a last stretch in a medium that meets nothing. They are multiplied from the
    /// light's end back.
    colours: Vec<[f32; 3]>,
    /// `shadow.amount` holds the nest (a cloud's shadow may then change it).
    settled: bool,
}

impl Amount {
    fn new() -> Amount {
        Amount {
            shadow: Shadow { colour: [1.0; 3], amount: 1.0 },
            beyond: None,
            stretches: Vec::new(),
            colours: Vec::new(),
            settled: false,
        }
    }

    /// A colour factor of the ray, in the order met.
    fn colour(&mut self, c: [f32; 3]) {
        self.colours.push(c);
    }

    /// A stretch ended: the march's alpha (if it was marched) and the share of its end.
    fn stretch(&mut self, alpha: Option<f64>, share: f64) {
        let m = alpha.map_or(1.0, |a| 1.0 - a);
        self.stretches.push((m, share));
        if let Some(b) = self.beyond.as_mut() {
            *b *= m;
            *b *= share;
        }
    }

    /// The amount of the ray: each stretch's (1 - alpha) times the product of what the rest of
    /// the ray gives and its own share, the last stretch first.
    fn settle(&mut self) {
        let mut amount = 1.0f64;
        for &(m, share) in self.stretches.iter().rev() {
            amount = m * (amount * share);
        }
        self.shadow.amount = amount;
        // : the colour from the light's end back, channel by channel.
        let mut colour = [1.0f32; 3];
        if let Some((&last, rest)) = self.colours.split_last() {
            colour = last;
            for &c in rest.iter().rev() {
                colour = tint(c, colour);
            }
        }
        self.shadow.colour = colour;
        self.settled = true;
    }

    fn result(mut self, version: RenderAs) -> Shadow {
        if !self.settled {
            self.settle();
        }
        if self.beyond == Some(0.0) && version != RenderAs::Bryce50 { FULL_SHADOW } else { self.shadow }
    }
}

impl ShadowSource for ShadowTracer<'_> {
    fn shadow(&mut self, q: &ShadowQuery) -> Shadow {
        match self.soft_shadow(q) {
            Ok(s) => s,
            Err(e) => {
                self.error.get_or_insert(e);
                FULL_SHADOW
            }
        }
    }
}

impl ShadowTracer<'_> {
    /// The shadow of a light:'s one ray, or, for a light with a radius above 0, the
    /// rays sends (the n_s² rays and their mean, or one turned ray), each pair of draws
    /// taken before its ray is traced.
    fn soft_shadow(&mut self, q: &ShadowQuery) -> Result<Shadow, OpenItem> {
        let (radius, sun) = self.radii.get(q.light).copied().unwrap_or((0.0, false));
        if radius == 0.0 {
            return self.trace(q, q.direction);
        }
        let Some(soft) = self.soft else { return Err(open::RADIUS_MARCH) };
        let light = if sun { DiscLight::Sun { radius } } else { DiscLight::Placed { radius, reach: q.reach } };
        let rays = soft::shadow_rays(self.version, light, soft.sample, self.switches, soft.grid);
        match rays {
            ShadowRays::One => self.trace(q, q.direction),
            ShadowRays::Grid { grid, disc } => {
                let mut mean = ShadowMean::new();
                for k in 0..grid * grid {
                    let point = soft::grid_point(grid, k, soft.two(), disc.radius);
                    let w = soft::turn(q.direction, point, disc.distance, soft::SOFT_ORDERS);
                    if q.offset && soft::given_up(w, q.normal, soft::GIVE_UP_ORDER) {
                        mean.add(soft::GIVEN_UP);
                    } else {
                        mean.add(self.trace(q, w)?);
                    }
                }
                Ok(mean.mean(grid))
            }
            ShadowRays::Numbered { sample, rays_a_side, disc } => {
                let p = soft::numbered_point(sample, rays_a_side, soft.two(), disc.radius);
                let w = soft::turn(q.direction, [f64::from(p[0]), f64::from(p[1])], disc.distance, soft::SOFT_ORDERS);
                self.trace(q, w)
            }
        }
    }

    /// 's shadow ray along `dir` (L, or a direction turned from it), or the Open
    /// subject it reaches. The origin and the side it starts on are L's ("same origin,
    /// same reach ℓ along its own direction").,
    /// (Shadow rays),
    fn trace(&mut self, q: &ShadowQuery, dir: Vec3) -> Result<Shadow, OpenItem> {
        let g = q.geometric_normal;
        // g turned toward the light; g·L and v·g' are z last as 3.0 and 3.1 and y last as 5.0.
        let dot_order = if self.version == RenderAs::Bryce50 { SumOrder::XzY } else { SumOrder::XyZ };
        let g = if dot_order.dot(g, q.direction) < 0.0 { [-g[0], -g[1], -g[2]] } else { g };
        let l = dir;
        // : a shadow ray from a point in the air leaves p itself.
        let origin: Vec3 = if q.offset { std::array::from_fn(|k| q.point[k] + g[k] * SHADOW_OFFSET) } else { q.point };
        let mut media = self.start.incoming;
        if let Some(far) = self.start.far {
            let behind = dot_order.dot(self.start.toward_eye, g) < 0.0;
            if self.version != RenderAs::Bryce30 && behind {
                media = far?;
            }
        }
        let mut ray = Ray { origin, dir: l };
        let mut result = Amount::new();
        // : a shadow ray from a point in the air counts surfaces
        // from the distance 0.0001 along it (its origin not moved);'s from its origin.
        let mut near = if q.offset { 0.0 } else { SHADOW_OFFSET };
        // Where the present stretch left, as a parameter of its ray: each stretch is a ray of its
        // own that leaves from its origin (below), so 0.
        let left_at = 0.0;
        // (Shadow rays: N is where this shadow ray went into O, or the origin of the
        // stretch that meets the surface: p moved along g', or the last surface it went on from)
        let mut stretch_origin = origin;
        let mut fuzzy_entries = FuzzyEntries::default();
        let orders = ShareOrders::of(self.version);
        // : the first stretch has the depth 0, each after a surface
        // it goes on through one more; as 3.0, past a volume material's surface, without end.
        let mut reach = q.reach;
        let mut depth = 0u32;
        let mut past_volume_30 = false;
        // : set once the ray
        // has gone on through a surface that is not a volume material's after a volume's.
        let mut nothing_left = false;
        // The object the ray this stretch was sent from met: the lit point's for the first
        // stretch, then the surface the stretch before passed.
        let mut sent_from = self.start.object;
        loop {
            // (a shadow ray does not meet a light's own object, and a light's list
            // marks read the ray's kind)
            let shadow_ctx = ShapeContext { shadow: true, ..*self.ctx };
            // : the stretch after that surface ends before it
            // starts and meets nothing (no march: it holds no sample).
            let hit = if nothing_left { None } else { self.geometry.scene.intersect(&ray, Range { near, far: reach }, &shadow_ctx) };
            // : the stretch in a volume's
            // medium is marched once traced, before what it meets is evaluated.
            let end = reach.is_finite().then_some(reach);
            let alpha = if nothing_left { None } else { self.march_stretch(&media, ray.origin, l, near, end, hit.map(|h| h.point), depth)? };
            let Some(hit) = hit else {
                // : a stretch in a medium that meets nothing more ends
                // with the amount times F (that of a ray that meets nothing) and the colour by Φ.
                let mut share = 1.0;
                if let Some(m) = self.stretch_medium(&media, None)? {
                    share = m.factor(self.version, None);
                    result.colour(m.volume_colour);
                }
                result.stretch(alpha, share);
                result.settle();
                // (Cloud shadows: a shadow ray that meets nothing, from its
                // origin; as 3.1 only toward the sun, whose reach has no end)
                if let Some(cl) = self.clouds {
                    let toward_sun = q.reach == f64::INFINITY;
                    match cl.render.shadow(cl.textures(), origin, l, toward_sun, result.shadow.amount, crate::textures::cloud_reading_context(self.version)) {
                        Ok(a) => result.shadow.amount = a,
                        Err(_) => {
                            self.error.get_or_insert(open::CLOUD_EVALUATION);
                        }
                    }
                }
                return Ok(result.result(self.version));
            };
            // (The angle law: a shadow ray's surfaces are met from its origin
            // outward; the surface itself takes the state from before)
            let met_before = self.met.meet(material_slot(self.material_of, self.geometry, hit.object, hit.meta));
            let surface = surface_of(self.surfaces, self.geometry, hit.object, hit.meta, self.switches, self.version)?;
            // ,: with the switch
            // word lacking bit 0x2 any surface ends the ray, a volume material's surface excepted
            // in every version (the test for a volume material's surface comes first).
            if !self.transmission && surface.volume.is_none() {
                return Ok(FULL_SHADOW);
            }
            if surface.volume.is_some() {
                // : a volume material's surface takes no share, and
                // the stretch it ends no medium colour and no factor; the shadow ray goes on
                // through it, in the medium on its far side, as 3.1 to its end, as 3.0 without
                // end.
                result.stretch(alpha, 1.0);
                result.beyond.get_or_insert(1.0);
                // : a terrain that encloses no volume is no medium (document 13):
                // the ray goes on in the medium it was in.
                if surface.medium.is_some() {
                    let solid = self.solid_of[hit.object];
                    media = media.met(solid).through_as(solid, hit.medium_object(self.version))?;
                }
                if self.version == RenderAs::Bryce30 {
                    reach = f64::INFINITY;
                    past_volume_30 = true;
                }
            } else if self.passes(&surface, hit.object, depth, &media, sent_from) {
                // : the ray passes the surface as if it were not
                // there (colour (1, 1, 1), amount 1) and goes on from the hit;:
                // it takes none of the medium of the stretch that ends at a surface it passes. Its
                // media change through the surface as at any other.
                result.stretch(alpha, 1.0);
                if surface.medium.is_some() {
                    let solid = self.solid_of[hit.object];
                    media = media.met(solid).through_as(solid, hit.object)?;
                }
            } else {
                let mut colour = self.surface_share(&surface, &hit, &media, left_at, stretch_origin, met_before, &mut fuzzy_entries, l, orders)?;
                let Some((mut share, c)) = colour.take() else { return Ok(FULL_SHADOW) };
                // (Shadow rays: as 3.0, after a volume's surface, the next such
                // surface leaves the ray nothing of its way: `nothing_left` below)
                // : the surface that ends a stretch in a medium has its
                // share's amount times F (for the stretch's own parameter) and the colour ⟨Θk·Φk⟩
                // (Φ' of).
                let mut c = c;
                if let Some(m) = self.stretch_medium(&media, Some((hit.t - left_at, hit.point)))? {
                    share *= m.factor(self.version, Some(hit.t - left_at));
                    c = tint(c, m.volume_colour);
                }
                // : an amount of 0 (the share times the
                // medium's factor) ends the ray with ((0, 0, 0), 0), whatever the Transparency
                if share == 0.0 {
                    return Ok(FULL_SHADOW);
                }
                result.stretch(alpha, share);
                result.colour(c);
                // : each surface it goes on through
                // changes its medium as's transmitted ray's, the medium beyond a held
                // solid by's rule with the shadow ray's stretches as rays of the path
                // (this stretch met the surface, the next is sent through it); a plane, a square
                // or a disc: no change. (The held solids and the meetings)
                if surface.medium.is_some() {
                    let solid = self.solid_of[hit.object];
                    media = media.met(solid).through_as(solid, hit.medium_object(self.version))?;
                }
                // : the share taken, a stretch numbered 100 or
                // more goes no further: the ray comes back with what it has, as if it had reached
                // the light.
                if depth >= SHADOW_STRETCH_BOUND {
                    result.settle();
                    return Ok(result.result(self.version));
                }
            }
            sent_from = Some(hit.object);
            // : after a volume material's surface, the next
            // surface it goes on through that is not a volume material's leaves it nothing of its
            // way.
            if past_volume_30 && surface.volume.is_none() {
                nothing_left = true;
            }
            depth += 1;
            // On from the surface as a new ray: its origin the surface's point, counting surfaces
            // from 0.0001 along it, its reach what is left of the light's. (Outputs,
            // : "goes on as a new ray from the surface's point (the point the hit
            // reports: for the stand-in of a tree,'s crossing) ... the distance the shadow
            // ray had left less the distance t at which it met the surface"; after the stand-in
            // of a tree too: the rest less t, observed in the original's pictures as 3.0, 3.1 and
            // 5.0 , so about 0.001 past the light for each surface of
            // the stand-in gone through; no longer Open, only the reach's last bits are free),
            //. Stated, no longer free: the original's pictures (tree-pictures
            // tr-03 and tr-06) separate this form from the same ray counting from t + 0.0001 and
            // from a new ray from the point at t .
            // (Shadow rays)
            stretch_origin = hit.point;
            // A set's t is measured from the shifted origin : its share of the reach
            // is the distance along the ray to the surface's point.
            let travelled = if hit.meta.is_some() && !matches!(standing(self.geometry, hit.object, hit.meta), Some(Standing::Wood(_))) {
                (0..3).map(|k| (hit.point[k] - ray.origin[k]) * l[k]).sum::<f64>()
            } else {
                hit.t
            };
            ray = Ray { origin: hit.point, dir: l };
            reach -= travelled;
            near = SHADOW_OFFSET;
        }
    }

    /// Whether the shadow ray passes a surface that is not a volume material's, by its flag word,
    /// in this order: bit 0x80; as 5.0 bit 0x200000 where `object` is the object the ray of the
    /// stretch was sent from met (`sent_from`); bit 0x2000 except on the first stretch (`depth`
    /// 0, sent from a ray that is not a shadow ray) leaving the object met (its solid held by
    /// the path's media)., (the Light mode's
    /// word has bit 0x80)
    fn passes(&self, surface: &ShadowSurface, object: usize, depth: u32, media: &PathMedia, sent_from: Option<usize>) -> bool {
        let w = surface.word;
        if w & SHADOW_PASS != 0 {
            return true;
        }
        if self.version == RenderAs::Bryce50 && w & NO_SELF_SHADOW != 0 && sent_from == Some(object) {
            return true;
        }
        if w & ADDITIVE != 0 {
            let leaving = media.holds(self.solid_of[object]);
            return !(depth == 0 && leaving);
        }
        false
    }

    /// The share and colour a shadow ray takes at a surface that is not a volume material's
    /// (before the medium's factor), or `None` where the surface stops it.
    /// (Outputs),
    #[allow(clippy::too_many_arguments)]
    fn surface_share(
        &self,
        surface: &ShadowSurface,
        hit: &Hit,
        media: &PathMedia,
        left_at: f64,
        stretch_origin: Vec3,
        met_before: bool,
        fuzzy_entries: &mut FuzzyEntries,
        l: Vec3,
        orders: ShareOrders,
    ) -> Result<Option<(f64, [f32; 3])>, OpenItem> {
        // : a material whose channels evaluates is evaluated
        // at the hit, with the hit's normal (before the bump) and the distance from where
        // this stretch left; its t, Θ and shading normal (after the bump) are's.
        let (t, mut colour, normal) = if surface.textured {
            let at = self.textured_at(hit, hit.t - left_at)?;
            (at.transparency, at.transparent_colour, at.normal)
        } else {
            (f64::from(surface.transparency), surface.colour, hit.shading_normal)
        };
        let share;
        if let Some(kind) = surface.fuzzy {
            // : every fuzzy surface
            // counts, a Transparency of 0 included; its share in place of's t' and
            // colour. Limits, Open: t below 0, and t = 0 where a is not.
            let a = f64::from(surface.transparency);
            if !(t > 0.0 || (t == 0.0 && a == 0.0)) {
                return Err(open::FUZZY_SHADOW_OPAQUE);
            }
            colour = [1.0; 3];
            let entering = kind == FuzzyShape::Solid && !media.holds(self.solid_of[hit.object]);
            share = if entering {
                // Going in costs nothing; the way in is N for the way out.
                if !fuzzy_entries.set(hit.object, hit.point) {
                    return Err(super::media::open::DEEP);
                }
                1.0
            } else {
                let f = match kind {
                    FuzzyShape::Plane => fuzzy::plane_weight(t),
                    FuzzyShape::Solid => {
                        let n = fuzzy_entries.get(hit.object).unwrap_or(stretch_origin);
                        let own = |p: Vec3| fuzzy_own(&self.geometry.scene, hit.object, p);
                        let orders = FuzzyOrders::of(self.version);
                        let scale = fuzzy_box_scale(&self.geometry.scene, &self.geometry.scene.objects[hit.object].shape);
                        fuzzy::solid_weight_scaled(self.version, own(n), own(hit.point), scale, a, t, orders)
                    }
                };
                fuzzy::shadow_amount(f, a)
            };
            // : an amount of 0 ends the ray with
            // ((0, 0, 0), 0), as at any surface (a fuzzy plane of Transparency 0 at once).
            if share == 0.0 {
                return Ok(None);
            }
        } else {
            // : a Transparency
            // of exactly 0 ends the ray; one below 0 is the share's amount as it stands (the angle
            // law and its second term act only above 0), the ray going on with its amount
            // multiplied by that negative number and its colour by the share's colour. A number
            // that is none stops it, as before .
            if t.is_nan() || t == 0.0 {
                return Ok(None);
            }
            if t < 0.0 {
                let mut colour = colour;
                if surface.blended {
                    colour = blended_shadow_colour(colour, t);
                }
                return Ok(Some((t, colour)));
            }
            // (Outputs: the share t', with the angle law where it is in force, for
            // a material with textures decided by the A value of its Transparency; in force
            // once the material has been met)
            let law = if surface.blended {
                None
            } else {
                AngleLaw::in_force(self.version, f64::from(surface.transparency), surface.refraction, met_before)
            };
            share = shadow_share(law, t, l, normal, orders);
            // (The blended material: toward a shadow ray its share's colour goes
            // toward white by the amount, α the share's amount: the surface's own share t,
            // before the medium's factor;, media-04 decides)
            if surface.blended {
                colour = blended_shadow_colour(colour, share);
            }
        }
        Ok(Some((share, colour)))
    }
}

/// The light in the air, with the lights and what its shadow rays meet (a copy of the
/// tracer's inputs, so that the sky rules gather as the surface rules do).
/// (Light in the air: In the render)
#[derive(Clone, Debug)]
struct LitAirGather {
    version: RenderAs,
    air: LitAir,
    lights: Vec<LightSource>,
    geometry: GeometryBuild,
    ctx: ShapeContext,
    surfaces: Vec<Result<ShadowSurface, OpenItem>>,
    materials: Vec<Result<ObjectMaterial, OpenItem>>,
    solid_of: Vec<usize>,
    material_of: Vec<Option<usize>>,
    met: MetMaterials,
    tables: OwnedTables,
    switches: u32,
    transmission: bool,
    clouds: Option<SceneClouds>,
    /// Per light, its radius (a shadow of a light with one is refused here: Open).
    radii: Vec<(f64, bool)>,
    /// Per light (in `lights`' order): its beam, whose factor is taken at every place.
    beams: Vec<Option<BeamAtPoint>>,
    /// Per light: Bryce 5's range, shadow share and shadow colour, which the gathering reads as
    /// 5.0.
    bryce5: Vec<Bryce5Light>,
}

impl LitAirGather {
    /// The ray's colour with V added; `hit` is the ray's own parameter at its surface, if it
    /// meets one; `depth`, `path` and `media` are the ray's depth, the path its air is measured
    /// along and its media. A ray that gathers nothing (3.0 and 3.1 in a solid's medium) keeps
    /// its colour. (Inputs: E, D, m, l, d, the medium; the shadow ray from p
    /// itself, whatever the scene's shadow bit)
    fn add(
        &self,
        ray: &CameraRay,
        hit: Option<f64>,
        depth: u32,
        path: AirPath,
        media: PathMedia,
        colour: [f32; 3],
    ) -> Result<[f32; 3], OpenItem> {
        Ok(match self.along_ray(ray, hit, depth, path, media)? {
            Some(v) => LitAir::over(colour, v),
            None => colour,
        })
    }

    /// V along `ray` of the tree (the ray's own `hit`, depth, path and media), or `None` where it
    /// gathers nothing.;
    fn along_ray(&self, ray: &CameraRay, hit: Option<f64>, depth: u32, path: AirPath, media: PathMedia) -> Result<Option<[f32; 3]>, OpenItem> {
        let along = PathRay {
            start: path.start_point(ray.origin),
            direction: ray.dir,
            depth,
            before: path.travelled(),
            hit,
            in_solid: media.current().is_some(),
        };
        self.gather_path(&along, media)
    }

    /// V along the ray from `eye` along the unit `dir`.
    fn gather(&self, eye: Vec3, dir: Vec3, surface: Option<f64>) -> Result<[f32; 3], OpenItem> {
        Ok(self.gather_path(&PathRay::camera(eye, dir, surface), PathMedia::OUTSIDE)?.unwrap_or([0.0; 3]))
    }

    /// V along a ray of the tree, or `None` where it gathers nothing. Its shadow rays travel in
    /// the ray's media.
    fn gather_path(&self, ray: &PathRay, media: PathMedia) -> Result<Option<[f32; 3]>, OpenItem> {
        if !self.air.gathers(ray) {
            return Ok(None);
        }
        let mut tracer = ShadowTracer {
            version: self.version,
            ray_depth: ray.depth,
            start: ShadowStart { incoming: media, far: None, toward_eye: [0.0; 3], object: None },
            geometry: &self.geometry,
            ctx: &self.ctx,
            surfaces: &self.surfaces,
            materials: &self.materials,
            solid_of: &self.solid_of,
            material_of: &self.material_of,
            met: &self.met,
            tables: self.tables.view(),
            switches: self.switches,
            transmission: self.transmission,
            clouds: self.clouds.as_ref(),
            radii: &self.radii,
            generator: None,
            soft: None,
            error: None,
        };
        let mut shadow = |light: usize, point: Vec3, direction: Vec3, reach: f64| {
            tracer.shadow(&ShadowQuery { light, point, geometric_normal: [0.0; 3], normal: [0.0; 3], direction, reach, offset: false })
        };
        // with: at every place the beam's factor of a spot or
        // parallel light, from the place itself; none outside the beam.
        let mut beam_error = None;
        let mut factor = |light: usize, p: Vec3| -> Option<f64> {
            let Some(b) = &self.beams[light] else { return Some(1.0) };
            let u = light_space_as(self.version, &b.placement, p);
            match beam_factor(self.version, b.beam, u, ConeOrder::of(self.version)) {
                Ok(phi) => phi,
                Err(e) => {
                    beam_error.get_or_insert(e);
                    None
                }
            }
        };
        let v = self.air.gather_path(ray, &self.lights, &self.bryce5, &mut factor, &mut shadow);
        if let Some(e) = tracer.error {
            return Err(e);
        }
        if let Some(e) = beam_error {
            return Err(e);
        }
        Ok(v)
    }
}

/// The moon's picture a render shows, as the caller gave it; refused saying which way it is
/// missing.,
pub(super) fn moon_image(plan: &RenderPlan) -> Result<std::sync::Arc<crate::sky::moon_image::MoonImage>, OpenItem> {
    match &plan.moon_picture {
        None => Err(open::MOON_PICTURE_MISSING),
        Some(Err(_)) => Err(open::MOON_PICTURE_DAMAGED),
        Some(Ok(image)) => Ok(image.clone()),
    }
}

/// The rainbow of a render (bit 0x4000), or `None` where none can show: the word's bit clear,
/// the sun's height below 0 or not a number, or as 5.0 a plain word with N8 not above 0. The
/// word is the stored one as 3.0 and 3.1, and W as 5.0 (where reads the sky: its bits 0x20
/// and 0x40 are cleared for a sky taken over plain).
fn scene_rainbow(scene: &Scene, version: RenderAs, main: &MainLight) -> Option<Rainbow> {
    let s = &scene.sky;
    let five = version == RenderAs::Bryce50;
    let word = if sky_five::reads_five(scene, version) { sky_five::five_settings(scene).ok()?.1.switches } else { s.flags };
    Rainbow::new(
        version,
        &RainbowInputs {
            switches: word,
            discs: s.discs,
            sun: sun_direction(scene, version),
            cloud_height: s.cloud_height,
            light_colour: main.colour,
            light_intensity: main.intensity,
        },
        five && s.flags & crate::sky::five::SKY_OWN_BITS == 0,
    )
}

/// Whether the air acts on a ray that meets nothing as 3.0 and 3.1 say: bit 0x80 set and bits
/// 0x400 and 0x800 clear (the rainbow's step 1 asks it).
fn air_word_acts(flags: u32) -> bool {
    flags & AIR_ON != 0 && flags & (0x400 | 0x800) == 0
}

/// The sun and sky as reads them from the document.
fn prepared_sky(scene: &Scene, version: RenderAs, haze_five: Option<[f64; 3]>) -> Result<PreparedSky, OpenItem> {
    let sky = &scene.sky;
    // ,: a version 7 sky as 3.0 or 3.1 is the
    // implementer's choice; refused, saying so. (The render)
    if sky.version == crate::scene_sky::SKY_VERSION_SEVEN && version != RenderAs::Bryce50 {
        return Err(open::SKY_FIVE_AS_THREE);
    }
    check_haze_fog(scene, version, haze_five)?;
    // : as 5.0 a sky part of version 2 or 7 has the main light and the dome light
    // of as 5.0 for its word W.
    if sky_five::reads_five(scene, version) {
        let (part, w) = sky_five::five_settings(scene)?;
        return Ok(crate::sky::five::prepare_lights(&sun_inputs(scene, version), &part, &w));
    }
    let switches = sky_colour_switches(version, sky.flags).ok_or(open::SKY_SWITCHES)?;
    if !matches!(sky.version, 1 | 2) {
        return Err(open::SKY_SWITCHES);
    }
    let sky_in = SkyInputs { switches, cloud_colour: sky.lower_cloud_colour, dome_colour: sky.dome_colour };
    let prepared = prepare(version, &sun_inputs(scene, version), &sky_in);
    Ok(prepared)
}

/// The sun as reads it.
fn sun_inputs(scene: &Scene, version: RenderAs) -> SunInputs {
    let sun = scene.sun();
    SunInputs {
        direction: sun_direction(scene, version),
        colour: sun.colour,
        glow_colour: sun.glow_colour,
        intensity: sun.intensity,
    }
}

/// The direction every rule of the sky means by "the sun's direction": the light part's, or with
/// the switch word's bit 0x200 its image in the world's frame S' (the camera's rows as the
/// document gives them, as `version` opens it). Where the view's transform is Open the render is
/// refused (`camera_input`) and S stands.
fn sun_direction(scene: &Scene, version: RenderAs) -> crate::geom::Vec3 {
    let s = scene.sun().direction;
    if scene.sky.flags & SUN_IN_VIEW == 0 {
        return s;
    }
    match scene.view.transform(version).given() {
        Some(t) => sun_in_world(version == RenderAs::Bryce50, t.rows, s),
        None => s,
    }
}

/// The bits that ask for the cloud layers and their shadows, which change none of's
/// results and none of's colours, in every version (sky 01's Scope).
const CLOUD_WORD_BITS: u32 = 0x10 | 0x20 | 0x40 | 0x1000; // Scope (0x10, 0x20, 0x40, 0x1000 [format])
/// The layers' fixed third coordinates: outside sky 01's Scope; taken as changing no colour
/// of.
const FIXED_THIRD_BIT: u32 = 0x40_0000;

/// The switch word are given, or `None` where the word is outside what is
/// stated: the word with the clouds' bits taken off (they change none of their results, sky
/// 01's Scope), and bit 0x400000 with them ("The clouds in the render").
fn sky_colour_switches(version: RenderAs, flags: u32) -> Option<u32> {
    // The star field's bit: the field is laid over's colour and changes none of
    // 's results (chosen;"Stars and comets").
    let mut w = flags & !(CLOUD_WORD_BITS | FIXED_THIRD_BIT | STAR_FIELD);
    if version == RenderAs::Bryce50 {
        // taken as changing none of's results, as bit 0x800 does as 3.0 and 3.1 (
        // Further bits;"The clouds in the render")
        w &= !SKY_EFFECTS_OFF;
    }
    if version != RenderAs::Bryce50 {
        // sky 01 ("Every switch word as 3.0 and 3.1"): bits 0x40000000 and 0x80000000
        // change nothing; bit 0x80000 acts only on the layers' places; and without bit 0x1 (the
        // sky with atmosphere) none of the further bits acts, the colour of a ray that meets
        // nothing being the plain colour. So every word is stated.
        w &= !(NO_RENDER_READS | LAYERS_PLACE);
        if w & crate::sky::colours::ATMOSPHERE == 0 {
            w &= !ATMOSPHERE_ONLY_BITS;
        }
    }
    in_scope(version, w).then_some(w)
}

/// Bits no render as 3.0 or 3.1 reads. spec: sky 01 (0x40000000 and 0x80000000 [format])
const NO_RENDER_READS: u32 = 0x4000_0000 | 0x8000_0000;
/// The clouds follow the view: acts on the layers only.
const LAYERS_PLACE: u32 = 0x8_0000;
/// The bits that act only in the sky with atmosphere: the halos (0x10000, 0x2000000), the moon's
/// picture (0x20000) and phase (0x40000), the discs' growth (0x100000), the stars (0x8000) and
/// bits 0x400 and 0x800 (the air reads them from the stored word, not from this one).
/// sky 01 ("Every switch word as 3.0 and 3.1", second bullet),
const ATMOSPHERE_ONLY_BITS: u32 = 0x400 | 0x800 | 0x1_0000 | 0x2_0000 | 0x4_0000 | 0x10_0000 | 0x200_0000 | 0x8000;

/// A light with a beam as the render evaluates it:'s beam with its exponents, and the
/// light's object-to-world placement (p is carried into its own space by the inverse).
#[derive(Clone, Debug)]
struct BeamAtPoint {
    beam: Beam,
    placement: Placement,
}

impl BeamAtPoint {
    /// The kind of gel point the light's gel takes.
    fn gel_kind(&self) -> GelKind {
        match self.beam {
            Beam::Cone { .. } => GelKind::Cone,
            Beam::Cylinder { .. } => GelKind::Cylinder,
            Beam::Pyramid { .. } => GelKind::Pyramid,
            Beam::Boxed { .. } => GelKind::Box,
        }
    }
}

/// 's beam for a document's beam record: the spot light's cone, the parallel light's
/// box. The cone's exponent is the falloff track's value where the opening applied the track
/// (at least one key), the stored exponent otherwise . A parallel light
/// whose two exponents differ takes the first as ex (across the light's own x axis) and the
/// second as ez ("the exponents along the light's own x axis and
/// then its z axis"); the cylindrical parallel light is's cylinder with the spot light's
/// exponent, the square spot light's pyramid with the parallel light's two.
/// . (In the render)
fn shading_beam(record: BeamRecord, falloff_opened: Option<f64>) -> Result<Beam, OpenItem> {
    Ok(match record {
        BeamRecord::Spot { exponent, .. } => Beam::Cone { exponent: falloff_opened.unwrap_or(exponent) },
        BeamRecord::Parallel { exponents: [a, b] } => Beam::Boxed { ex: f64::from(a), ez: f64::from(b) },
        BeamRecord::Cylinder { exponent, .. } => Beam::Cylinder { exponent },
        BeamRecord::Square { exponents: [a, b] } => Beam::Pyramid { ex: f64::from(a), ez: f64::from(b) },
    })
}

/// The surface rules: for plain materials.
#[derive(Clone, Debug)]
pub struct PlainSurface {
    version: RenderAs,
    geometry: GeometryBuild,
    ctx: ShapeContext,
    scene_light: SceneLight,
    /// The place of the sun's object, the translation of its matrix: where a sun whose switch word
    /// lacks bit 0x400 stands for the lit kind's falloff.
    sun_place: Vec3,
    /// The lights with their Bryce 5 numbers as the version reads them (as 3.0 and 3.1 none).
    lit: Vec<LitLight>,
    /// Per document object.
    materials: Vec<Result<ObjectMaterial, OpenItem>>,
    /// Per light (in `lights`' order): its gel, whose colour is taken at each lit point.
    gels: Vec<Option<GelLight>>,
    /// Per light (in `lights`' order): its beam, whose factor is taken at each lit point.
    beams: Vec<Option<BeamAtPoint>>,
    /// The tables the textures read.
    tables: OwnedTables,
    /// Per geometry object.
    shadow_surfaces: Vec<Result<ShadowSurface, OpenItem>>,
    /// Per geometry object: the index of its material in the document, if any.
    material_of: Vec<Option<usize>>,
    /// The materials met so far, shared with the sky rules and the quick preview.
    met: MetMaterials,
    /// The depth at which a ray sends no further ray: 6 in a frame, 3 in the quick preview.
    pub(crate) depth_limit: u32,
    /// Per geometry object: whether it encloses a volume.,
    encloses: Vec<bool>,
    /// Per geometry object: the object whose surface a hit on it crosses (itself, or for a member
    /// of a combined group the group).
    solid_of: Vec<usize>,
    /// Per geometry object: for a terrain that encloses no volume, its slot among such terrains
    /// (what a path counts crossings by).
    terrain_slot: Vec<Option<usize>>,
    /// The solid that holds the camera's eye, if any.
    eye: EyeMedium,
    /// As 5.0, the scene's limit of total internal reflections. step 4
    tir_limit: u32,
    /// The air laid over a camera ray's colour (3.0, 3.1).
    air: Option<Air>,
    /// The rainbow laid over a ray that meets a surface, after the air.
    rainbow: Option<Rainbow>,
    /// The light in the air added after it (3.0, 3.1).
    lit_air: Option<LitAirGather>,
    /// The clouds, for the cloud shadows.
    clouds: Option<SceneClouds>,
    /// Surfaces whose ambient ray is being traced, innermost last (the walk is depth first).
    /// (Premium samples that draw in their trees: the ambient ray)
    pending: Vec<Pending>,
    /// Fuzzy surfaces whose continuing ray is being traced, innermost last (the walk is depth
    /// first). (Fuzzy solids)
    fuzzy_pending: Vec<FuzzyPending>,
    /// Set while a fuzzy surface whose lighting was deferred is lit after its continuing ray's
    /// tree: the continuing ray is not sent again.,
    skip_continuing: bool,
    /// The ambient rays the shaded samples of the march just made ask for, in the order asked
    /// (pass 1 of `march_ray`), and what they brought back, handed to its pass 2.
    /// (Revision of)
    march_asks: RefCell<Vec<SecondaryRay>>,
    march_given: RefCell<Option<std::collections::VecDeque<[f32; 3]>>>,
    /// The share of each blended ray sent and not yet back, innermost last (a double: a
    /// Light-mode surface's V is not a single).,
    blend_shares: Vec<f64>,
    /// Per light (in `lights`' order): its gradient, as 5.0 with the gradient bit.
    gradients: Vec<Option<LightGradient>>,
    /// Per light: its radius ρ as 5.0 (0 for none, and as 3.0 and 3.1).
    radii: Vec<(f64, bool)>,
    /// The render's generator while a surface is shaded (`swap_generator`); the shadow rays
    /// of a light with a radius draw from it.
    /// (Shading that draws)
    generator: Cell<CRand>,
    /// The sample number and count of the Premium ray along which a march runs (its samples'
    /// shadow rays are numbered as that ray is).
    march_sample: Cell<Option<(u32, u32)>>,
    /// n_s, the side of the grid of a light's soft shadow rays: 8 in a frame, 3 in the quick
    /// preview.,
    pub(crate) soft_grid: u32,
    /// The sun as the wash reads it, where the sky part has bit 0x1000000 (a Bryce 5 sky): the
    /// sun's light at a lit point is its light at the point.
    wash: Option<crate::sky::wash::WashSun>,
    /// Whether the wash is laid over the colour of each ray.
    wash_rays: bool,
}

/// What a fuzzy surface keeps until its continuing ray comes back: the object O (a geometry
/// index), its kind, a and t, and whether the ray met it from inside.
#[derive(Clone, Copy, Debug)]
struct FuzzyPending {
    object: usize,
    kind: FuzzyShape,
    a: f64,
    t: f64,
    inside: bool,
}

/// A surface waiting for the one ray it sent first: the ambient ray of Bryce 5's gathered
/// light, or the start ray of a Light-mode surface. point 5,
#[derive(Clone, Copy, Debug)]
enum Pending {
    Gathering(PendingSurface),
    /// A surface whose start ray (bit 0x2000) is being traced: its material
    /// (a Light-mode one rewritten), its shading normal and whether it was met before.
    Start { material: SurfaceMaterial, shading_normal: Vec3, met_before: bool },
    /// A fuzzy surface (outside its solid) whose continuing ray is being traced: sends
    /// that ray before the surface is lit, so the lighting (and its draws) come after the
    /// ray's tree. Kept only where the lighting draws because of the wash .
    FuzzyLit { material: SurfaceMaterial, shading_normal: Vec3, met_before: bool },
}

/// What a surface that sent its ambient ray keeps until the ray's colour comes back: its
/// light terms, its material at the hit and whether it was met before. point 5
#[derive(Clone, Copy, Debug)]
struct PendingSurface {
    terms: LightTerms,
    material: SurfaceMaterial,
    met_before: bool,
}

impl PlainSurface {
    /// The place of the sun's object.
    pub(crate) fn sun_place(&self) -> Vec3 {
        self.sun_place
    }

    /// Everything that is fixed before the first ray. (the lights in
    /// 's order, the sun last)
    pub fn new(scene: &Scene, plan: &RenderPlan, met: MetMaterials) -> Result<PlainSurface, OpenItem> {
        let version = plan.version;
        let sky = prepared_sky(scene, version, plan.haze_five)?;
        // : a Bryce 5 sky's word with bit 0x1000000; rays are washed
        // where one of bits 0x2, 0x4, 0x8 is there and not 0x800 (`wash_rays`), the sun's light at
        // a lit point changes whatever those bits are
        let wash_bit = wash::bit_of(scene, version);
        let wash = wash_bit.then(|| wash::sun_of(scene));
        let wash_rays = wash_bit && scene.sky.flags & wash::RAY_BITS != 0 && scene.sky.flags & wash::SKY_EFFECTS_OFF == 0;
        let mut lights = Vec::with_capacity(scene.lights.len());
        let mut gels = Vec::with_capacity(scene.lights.len());
        let mut beams = Vec::with_capacity(scene.lights.len());
        let mut gradients = Vec::with_capacity(scene.lights.len());
        for light in &scene.lights {
            let mut gel = None;
            let mut beam = None;
            let mut gradient = None;
            lights.push(match light {
                Light::Radial(r) => {
                    // (In the render)
                    if let Some(record) = r.beam {
                        let world = scene.objects[r.object].world_as(version).ok_or(open::BEAM_TRANSFORM)?;
                        let placement = Placement::new(world.rows, world.translation).ok_or(open::BEAM_TRANSFORM)?;
                        beam = Some(BeamAtPoint { beam: shading_beam(record, r.falloff_opened)?, placement });
                    }
                    // : a gel where some channel of the light's material has
                    // both a switch word that is not 0 and a reference, whatever the scene's bit
                    // 0x200; otherwise's colour with Diffuse's A, the binary32 product.
                    let colour = match r.gel {
                        None => r.colour,
                        Some(m) => {
                            let part = &scene.materials[m].part;
                            if channels_evaluated(part.channel_words(), TEXTURES_ON) {
                                // : the colour at each lit point; the light's
                                // own object's transform. (gels)
                                let world = scene.objects[r.object].world_as(version).ok_or(open::GEL)?;
                                let placement = Placement::new(world.rows, world.translation).ok_or(open::GEL)?;
                                let material = TexturedMaterial::new(scene, m, version)?;
                                gel = Some(match &beam {
                                    None => GelLight::new(material, placement, r.own_colour),
                                    Some(b) => GelLight::of_kind(material, placement, r.own_colour, b.gel_kind()),
                                });
                                r.own_colour
                            } else {
                                let diffuse = part.colours[0].a;
                                std::array::from_fn(|k| r.own_colour[k] * diffuse[k])
                            }
                        }
                    };
                    // (Bryce 5's gradients, as 5.0 with bit 0x200000; a gel with a
                    // gradient): c0 the own colour, D the material's Diffuse A value
                    // ((1, 1, 1) without a material, the product then leaves out); for a
                    // gel D is its Diffuse colour at the gel point, at each lit point.
                    if version == RenderAs::Bryce50 && r.switches & light_switch::GRADIENT != 0 {
                        let g = r.gradient.as_ref().ok_or(open::GRADIENT_ABSENT)?;
                        let diffuse = scene.objects[r.object].material.map_or([1.0; 3], |m| scene.materials[m].part.colours[0].a);
                        gradient = Some(LightGradient {
                            range: r.part.five.gradient[0],
                            offset: r.part.five.gradient[1],
                            own_colour: r.own_colour,
                            material_diffuse: diffuse,
                            colours: KeyList {
                                switches: g.colour_switches,
                                keys: g.colour_keys.iter().map(|k| ColourKey { colour: k.colour, place: k.place, midpoint: k.midpoint }).collect(),
                            },
                            values: KeyList {
                                switches: g.value_switches,
                                keys: g.value_keys.iter().map(|k| ValueKey { value: k.value, place: k.place, midpoint: k.midpoint }).collect(),
                            },
                        });
                    }
                    LightSource {
                        place: LightPlace::Radial { position: scene.light_position(r, version) },
                        colour,
                        intensity: r.intensity,
                        switches: r.switches,
                    }
                }
                Light::Sun(s) => LightSource {
                    // : with bit 0x1000000 the main light's direction
                    // is the sun's own whatever its height (no moon)
                    place: LightPlace::Sun {
                        direction: if wash_bit { sun_direction(scene, version) } else { sky.main.direction },
                    },
                    colour: sky.main.colour,
                    intensity: sky.main.intensity,
                    switches: s.switches,
                },
            });
            gels.push(gel);
            beams.push(beam);
            gradients.push(gradient);
        }
        let part_of = |object: usize| scene.objects[object].material.map(|m| &scene.materials[m].part);
        let (solid_of, _nested) = solid_identities(&plan.geometry.scene);
        let materials: Vec<Result<ObjectMaterial, OpenItem>> =
            (0..scene.objects.len()).map(|i| object_material(scene, version, i)).collect();
        let mut shadow_surfaces: Vec<Result<ShadowSurface, OpenItem>> = (plan.geometry.scene_object.iter().zip(&plan.geometry.scene.objects))
            .enumerate()
            .map(|(g, (&i, o))| {
                // (point 1: a group encloses a volume whatever its members; point
                // 3: its medium's material is the member the hit names): a member of a combined
                // group has the medium of its material whatever its own shape encloses.
                let encloses = encloses_in(&plan.geometry.scene, &o.shape) || solid_of[g] != g;
                // : a metaball's own material is only ever read for the
                // object standing for every set (a hit on a set it leads), which encloses a volume
                if let Shape::Metaball(_) = o.shape {
                    return shadow_surface(part_of(i), &Shape::MetaSet, false, true, scene.settings.switches, version);
                }
                shadow_surface(part_of(i), &o.shape, matches!(scene.objects[i].body, Body::Shape(_)), encloses, scene.settings.switches, version)
            })
            .collect();
        // : bit 0x8000 of a member's
        // material takes the group's map and the group's own extent in y, in all three versions.
        for (g, shadow) in shadow_surfaces.iter_mut().enumerate() {
            let group = solid_of[g];
            let (Ok(s), true) = (shadow, group != g) else { continue };
            let Some(tint) = s.tint.as_mut() else { continue };
            if tint.altitude.is_none() {
                continue;
            }
            if let Some(space) = super::media::group_space(scene, &plan.geometry, group, version) {
                tint.group = Some(space);
            }
        }
        // (a group is a solid that holds a ray; its medium's material is the member
        // the hit names, in 3.0, 3.1 and 5.0: Variants),
        // point 3 : no member of a combined group is refused for its
        // material any more (a Light-mode member, one that lets rays through, a group inside a
        // group); observed: light-object-medium-pictures lm-lamp-grp, lm-ball-grp, gm-lamp.
        // (Combined groups), shading.md (Solid media)
        // : a Light-mode surface is never one that has been met (the angle
        // law is off), so it keeps no place among the materials met.
        let mut material_of: Vec<Option<usize>> = plan
            .geometry
            .scene_object
            .iter()
            .map(|&i| match &materials[i] {
                Ok(ObjectMaterial::Plain(m)) if m.mode.is_some() => None,
                _ => scene.objects[i].material,
            })
            .collect();
        // (Metaballs): the object standing for every set keeps a slot of its own
        // in the materials met, after the document's.
        if let Some(ms) = &plan.geometry.scene.metaballs {
            material_of[ms.stand] = Some(scene.materials.len());
        }
        let encloses = plan.geometry.scene.objects.iter().map(|o| encloses_in(&plan.geometry.scene, &o.shape)).collect();
        // , : each terrain that encloses no volume gets a slot
        // (the plane, the square and the disc none: they are never held).
        let terrain_slot: Vec<Option<usize>> = {
            let mut next = 0;
            plan.geometry
                .scene
                .objects
                .iter()
                .map(|o| match o.shape {
                    Shape::Terrain(_) if !encloses_in(&plan.geometry.scene, &o.shape) => {
                        next += 1;
                        Some(next - 1)
                    }
                    _ => None,
                })
                .collect()
        };
        let eye = EyeMedium::new(scene, plan)?;
        // (Limits: the depth limit 6; step 4: the limit of total internal
        // reflections 2 for every Bryce 3D document rendered as 5.0), revised (offsets
        // 180 and 184: as 5.0, where Bryce 5's part is present, the ray depth and the limit of
        // total internal reflections are the document's own)
        let (depth_limit, tir_limit) = match scene.settings.depths() {
            Some(depths) if version == RenderAs::Bryce50 => depths,
            _ => (further::DEPTH_LIMIT, further::TIR_LIMIT_BRYCE3D),
        };        // (as 5.0 a light's shadow share, shadow colour and range;
        // Open: a sun with the range bit), (the radius ρ, the gate applied by the reader;
        // Open: a radius the reader leaves Open, a sun's radius above 0),
        let mut lit = Vec::with_capacity(lights.len());
        let mut radii = Vec::with_capacity(lights.len());
        for (light, source) in scene.lights.iter().zip(&lights) {
            let mut radius = 0.0;
            let mut sun_light = false;
            let bryce5 = if version == RenderAs::Bryce50 {
                let (part, sun) = match light {
                    Light::Radial(r) => (&r.part, false),
                    Light::Sun(s) => (&s.light, true),
                };
                radius = part.five.radius.ok_or(open::LIGHT_RADIUS)?;
                sun_light = sun;
                // : with bit 0x400 the
                // range bit is not read; without it the sun's distance term +infinity gives
                // λ = +infinity for a range of 0 or above (a = 0: the sun adds nothing), and
                // -infinity or not a number for a range below 0, -0 or not a number (a = 1: full
                // intensity), as `falloff` gives; nothing is Open here.
                Bryce5Light {
                    range: part.five.range,
                    shadow_share: part.five.shadow_ambience,
                    shadow_colour: part.five.shadow_colour,
                }
            } else {
                Bryce5Light::NONE
            };
            lit.push(LitLight { source: *source, bryce5 });
            radii.push((radius, sun_light));
        }
        // (a lens ray is a camera ray: it starts in the medium of the solid that
        // holds the eye, which the eye point decides, also where the lens point lies outside that
        // solid; its distances in that medium are measured from the lens point, the ray's origin:
        // the entry's fallback, the measure being Open).
        // : the air of an eye in
        // the open, the medium's weight after it; a lens ray's path starts at the eye
        // (`camera_state_of`). Neither is Open now.
        let air = scene_air(scene, version, &sky.main, plan.haze_five);
        let s = &scene.sky;
        let transmission = scene.settings.switches & scene_switch::TRANSMISSION != 0;
        let clouds = scene_clouds(scene, version, &plan.texture_tables)?;
        let lit_scene = LitAirScene {
            shadows: scene.settings.switches & scene_switch::SHADOWS != 0,
            shadow_setting: if version == RenderAs::Bryce50 { s.shadow_setting_as_five() } else { s.shadow_intensity },
            sun_position: scene.sun().placement.world().translation,
        };
        // : a Bryce 5 sky part gathers with its own air_steps places
        let bryce5_sky = version == RenderAs::Bryce50 && s.form() == crate::scene_sky::SKY_VERSION_SEVEN;
        let air_steps = s.five.as_ref().and_then(|f| f.air_steps).unwrap_or(0);
        // : with the gathering on and
        // air_steps 0 the original's render never finishes and gives no picture; refused here
        if bryce5_sky && s.flags & LIT_AIR_COARSE != 0 && s.flags & LIT_AIR_PLAIN_SKY == 0 && air_steps == 0 {
            return Err(open::LIT_AIR_NO_STEPS);
        }
        let lit_air_made = if bryce5_sky {
            LitAir::of_bryce5_sky(s.flags, s.lit_air, air_steps, lit_scene)
        } else {
            LitAir::new(version, s.flags, s.lit_air, lit_scene)
        };
        let lit_air = match lit_air_made {
            None => None,
            Some(air) => {
                // : the gathering
                // asks the sun for point 9's light at each place, with its draws, before the sun's
                // shadow ray there. Stated, but not implemented here (the gathering has no access
                // to the render's generator): refused rather than gathered with the sun's plain
                // colour.
                if wash_bit {
                    return Err(open::WASH_LIT_AIR);
                }
                // (When it acts: an eye inside a solid gathers nothing as 3.0 and
                // 3.1; The places: a sun that falls off; As 5.0: depth of field): all stated.
                if gels.iter().any(Option::is_some) {
                    return Err(textured::open::GEL_LIT_AIR);
                }
                let gradient_on = |(l, g): (&LightSource, &Option<LightGradient>)| {
                    g.is_some() && l.switches & light_switch::ON != 0
                };
                if lights.iter().zip(&gradients).any(gradient_on) {
                    return Err(open::LIT_AIR_GRADIENT);
                }
                // The spot and parallel lights are gathered with their beam's factor at every
                // place; no refusal.
                Some(LitAirGather {
                    version,
                    air,
                    lights: lights.clone(),
                    geometry: plan.geometry.clone(),
                    ctx: plan.shape_context,
                    surfaces: shadow_surfaces.clone(),
                    materials: materials.clone(),
                    solid_of: solid_of.clone(),
                    material_of: material_of.clone(),
                    met: met.clone(),
                    tables: plan.texture_tables.clone(),
                    switches: scene.settings.switches,
                    transmission,
                    clouds: clouds.clone(),
                    radii: radii.clone(),
                    beams: beams.clone(),
                    bryce5: lit.iter().map(|l| l.bryce5).collect(),
                })
            }
        };
        Ok(PlainSurface {
            version,
            geometry: plan.geometry.clone(),
            ctx: plan.shape_context,
            sun_place: scene.sun().placement.world().translation,
            scene_light: SceneLight {
                switches: scene.settings.switches,
                ambient_colour: s.ambient_colour,
                ambience: s.ambience,
                shadow_setting: if version == RenderAs::Bryce50 { s.shadow_setting_as_five() } else { s.shadow_intensity },
                sky_switches: s.flags,
                dome: sky.dome,
            },
            lit,
            materials,
            gels,
            beams,
            tables: plan.texture_tables.clone(),
            shadow_surfaces,
            material_of,
            met,
            depth_limit,
            encloses,
            solid_of,
            terrain_slot,
            eye,
            tir_limit,
            air,
            rainbow: scene_rainbow(scene, version, &sky.main),
            lit_air,
            clouds,
            pending: Vec::new(),
            fuzzy_pending: Vec::new(),
            skip_continuing: false,
            march_asks: RefCell::new(Vec::new()),
            march_given: RefCell::new(None),
            blend_shares: Vec::new(),
            gradients,
            radii,
            generator: Cell::new(CRand::new(1)),
            march_sample: Cell::new(None),
            soft_grid: soft::FRAME_GRID,
            wash,
            wash_rays,
        })
    }
}

impl PlainSurface {
    /// The lights as they shine toward the point p: a light with a beam with its intensity
    /// times the beam's factor at p (0 outside the beam: the light then adds nothing, and its
    /// gel is not evaluated), a gel with its colour at p.
    fn lights_at(&self, p: Vec3) -> Result<Vec<LitLight>, OpenItem> {
        let mut ls = self.lit.clone();
        for (((l, g), b), gr) in ls.iter_mut().zip(&self.gels).zip(&self.beams).zip(&self.gradients) {
            if let Some(b) = b {
                let u = light_space_as(self.version, &b.placement, p);
                match beam_factor(self.version, b.beam, u, ConeOrder::of(self.version))? {
                    Some(phi) => l.source.intensity *= phi,
                    None => {
                        l.source.intensity = 0.0;
                        continue;
                    }
                }
            }
            if let Some(gr) = gr {
                // (Bryce 5's gradients; Gels: the gradient first, on c0 and I, then
                // where I is not 0 the Diffuse colour D: the gel's at the gel point, evaluated
                // only there, or the material's A value)
                let LightPlace::Radial { position } = l.source.place else {
                    unreachable!("a gradient is a radial or beam light's")
                };
                let before = gradient_light(self.version, gr, position, p, l.source.intensity);
                let lit = before.intensity != 0.0 && !before.intensity.is_nan();
                let diffuse = match g {
                    Some(g) if lit => g.diffuse_at(self.version, p, self.scene_light.switches, self.tables.view())?,
                    _ => gr.material_diffuse,
                };
                l.source.colour = tinted_where_lit(before, diffuse);
                l.source.intensity = before.intensity;
                continue;
            }
            if let Some(g) = g {
                l.source.colour = g.colour_at(self.version, p, self.scene_light.switches, self.tables.view())?;
            }
        }
        Ok(ls)
    }

    /// The colour the light in the air adds along a camera ray from `eye` along the unit
    /// `dir` that meets a surface at `hit` (its distance) or nothing (`None`): V, three singles;
    /// `Ok(None)` where the light in the air does not act. For the vectors of
    pub fn lit_air_along(&self, eye: Vec3, dir: Vec3, hit: Option<f64>) -> Result<Option<[f32; 3]>, OpenItem> {
        match &self.lit_air {
            None => Ok(None),
            Some(lit) => lit.gather(eye, dir, hit).map(Some),
        }
    }

    /// As `lit_air_along` for any ray of the tree: `Ok(None)` also where the ray gathers
    /// nothing (as 3.0 and 3.1 in a solid's medium). The shadow rays start in the open (the
    /// vectors hold no shadow-casting light in a medium). For the vectors of
    pub fn lit_air_along_path(&self, ray: &PathRay) -> Result<Option<[f32; 3]>, OpenItem> {
        match &self.lit_air {
            None => Ok(None),
            Some(lit) => lit.gather_path(ray, PathMedia::OUTSIDE),
        }
    }

    /// The surface met and lit up to's gathered term (its shadow rays traced inline);
    /// then, where true ambience sends its ray (point 5), that ray queued alone and the
    /// surface kept for `resume`; otherwise the surface finished (`finish`).
    /// point 5
    fn begin_surface(&mut self, request: &SurfaceRequest, secondary: &mut SecondaryQueue) -> Result<[f64; 3], OpenItem> {
        let h = &request.hit;
        // (The angle law: a ray's surface is met when the ray meets it, before it
        // is lit; the surface itself is shaded with the state from before)
        let met_before = self.met.meet(material_slot(&self.material_of, &self.geometry, h.object, h.meta));
        // : a hit on a set is shaded with the material its own
        // hit gave; a hit on the wood with the wood material of the tree hit
        let made: Option<Result<ObjectMaterial, OpenItem>> = match standing(&self.geometry, h.object, h.meta) {
            None => None,
            Some(Standing::Set(r)) => {
                Some(r.and_then(|m| plain_material(&made_part(&m), self.scene_light.switches)).map(ObjectMaterial::Plain))
            }
            // : the leader's own material, as it is (its table entry)
            Some(Standing::Own(_)) => None,
            Some(Standing::Wood(tree)) => match &self.materials[self.geometry.scene_object[tree]] {
                Ok(ObjectMaterial::Plain(m)) => Some(Ok(ObjectMaterial::Plain(*m))),
                Err(e) => Some(Err(*e)),
                // : wood of a volume material is marched in the stand-in,
                // its depth the sum of the fields of the set last found or kept
                Ok(ObjectMaterial::Volume(_)) => None,
                // : a textured wood is shaded with the tree's own material
                // at the hit's coordinates, mapped in the stand-in's space (identity, box -1 to 1).
                Ok(ObjectMaterial::Textured(_)) => None,
            },
        };
        let own = match standing(&self.geometry, h.object, h.meta) {
            Some(Standing::Own(o) | Standing::Wood(o)) => self.geometry.scene_object[o],
            _ => request.object,
        };
        let entry = made.as_ref().unwrap_or(&self.materials[own]);
        // (Applies to: a sphere, a cone or a plane; step 2: a ray inside O, that
        // is a path that holds O, does not shade the surface: its continuing ray alone)
        let fuzzy = match entry {
            Err(e) => return Err(*e),
            Ok(ObjectMaterial::Plain(m)) => m.fuzzy.is_some(),
            Ok(ObjectMaterial::Textured(t)) => t.flags() & MATERIAL_FUZZY != 0,
            Ok(ObjectMaterial::Volume(_)) => return self.volume_surface(request, secondary),
        };
        if fuzzy {
            let kind = self.fuzzy_kind_of(h.object).ok_or(open::FUZZY_SHAPE)?;
            // , observed: `fuzzy-more-pictures` x-terrain-0: a terrain that
            // encloses no volume is held by its crossings, so a ray of the path that crossed it once
            // meets it again from inside (not shaded, not counted)
            let inside = match kind {
                FuzzyShape::Solid => request.state.media.holds(self.solid_of[h.object]),
                FuzzyShape::Plane => self.terrain_held_by(h.object, &request.state.media),
            };
            if inside {
                let pending = FuzzyPending { object: h.object, kind, a: f64::NAN, t: f64::NAN, inside: true };
                // not sent (5.0, 100 rays gone through): nothing comes back, as a volume's surface
                self.queue_continuing(request, pending, secondary)?;
                return Ok([0.0; 3]);
            }
        }
        // : a textured material's channels at the mapped point; its bump
        // tilts the shading normal .
        let (material, shading_normal) = match entry {
            Err(e) => return Err(*e),
            // : the shading normal (a mesh's m'; every other shape's normal).
            Ok(ObjectMaterial::Plain(m)) => (*m, h.shading_normal),
            Ok(ObjectMaterial::Textured(t)) => {
                // :'s distance is the hit's parameter
                // along the ray's direction for every ray, a further ray's included.
                let at = textured_at(t, self.version, &self.geometry, h, h.t, self.scene_light.switches, self.tables.view())?;
                (textured_surface(&at, t)?, at.normal)
            }
            Ok(ObjectMaterial::Volume(_)) => unreachable!("a volume material's surface is not shaded (above)"),
        };
        // : lit and coloured as the same channels with Transparency 0.
        let material = if material.fuzzy.is_some() { material.fuzzy_lit() } else { material };
        // , (The wash as 5.0: the wash of a fuzzy surface met from
        // outside comes after the continuing ray's tree and the surface's lighting, with their
        // draws): where the lighting draws (the sun's colour asked at the lit point, point 9), the
        // continuing ray goes alone and `resume` lights the surface when it is back.
        if let (Some(fz), None, true) = (material.fuzzy, material.mode, self.wash_defers_lighting(request)) {
            let kind = self.fuzzy_kind_of(h.object).ok_or(open::FUZZY_SHAPE)?;
            let pending = FuzzyPending { object: h.object, kind, a: fz.a, t: fz.t, inside: false };
            if self.queue_continuing(request, pending, secondary)? {
                self.pending.push(Pending::FuzzyLit { material, shading_normal, met_before });
                secondary.defer_lighting();
                return Ok([0.0; 3]);
            }
        }
        // : a Light-mode surface is rewritten; as 3.1 and 5.0 its start ray
        // is sent first, before the surface is lit, and `resume` lights it with s0 = its colour.
        if let Some(shape) = material.mode {
            let material = self.light_mode_applied(material, shape, request);
            if self.version == RenderAs::Bryce30 || !self.start_ray_sent(request) {
                return self.shade_from(request, material, shading_normal, false, [0.0; 3], secondary);
            }
            self.queue_start(request, secondary)?;
            self.pending.push(Pending::Start { material, shading_normal, met_before: false });
            return Ok([0.0; 3]);
        }
        // : any lit material whose flag word has
        // bit 0x2000 sends the start ray before it is lit, its colour the s0 the sum starts from;
        // as 5.0 only below 100 rays, else s0 is black.
        if material.flags & ADDITIVE != 0 {
            // A fuzzy surface's continuing ray and the start ray: their order is not stated.
            if material.fuzzy.is_some() {
                return Err(open::ADDITIVE_FUZZY);
            }
            if self.start_ray_sent(request) {
                self.queue_start(request, secondary)?;
                self.pending.push(Pending::Start { material, shading_normal, met_before });
                return Ok([0.0; 3]);
            }
        }
        self.shade_from(request, material, shading_normal, met_before, [0.0; 3], secondary)
    }

    /// Whether a fuzzy surface is lit after its continuing ray's tree: where the sky's wash draws
    /// in the lighting (point 9) and the sample is not a numbered one. (The wash as
    /// 5.0: a fuzzy surface's wash comes after the continuing ray's tree and the lighting),
    fn wash_defers_lighting(&self, request: &SurfaceRequest) -> bool {
        self.version == RenderAs::Bryce50 && self.wash.is_some() && request.premium.is_none()
    }

    /// The Light-mode surface at this hit: the material's channels rewritten by with the
    /// glow of taken from the ray in the object's own space (the sphere), or the
    /// cone's 0.7., (the ray carried
    /// into the object's space)
    fn light_mode_applied(&self, mut m: SurfaceMaterial, shape: OwnShape, request: &SurfaceRequest) -> SurfaceMaterial {
        use crate::shading::visible::{CONE_GLOW, LightModeInput, light_mode_surface, sphere_glow};
        let glow = match (self.version, shape) {
            (RenderAs::Bryce30, _) => 1.0, // no glow as 3.0: not read
            (_, OwnShape::Cone) => CONE_GLOW,
            (_, OwnShape::Other) => 1.0,
            (v, OwnShape::Sphere) => {
                let placement = &self.geometry.scene.objects[request.hit.object].placement;
                let local = placement.to_local_as(v, &Ray { origin: request.ray.origin, dir: request.ray.dir });
                sphere_glow(v, local.origin, local.dir)
            }
        };
        let input = LightModeInput { flags: m.flags, transparency: m.light.transparency, glow, diffuse: m.light.diffuse };
        let s = light_mode_surface(self.version, &input).expect("a Light-mode material");
        m.light.diffusion = s.diffusion;
        m.light.specularity = s.specularity;
        m.light.metallicity = s.metallicity;
        m.light.ambience = s.ambience;
        m.light.ambient = s.ambient_colour;
        m.light.transparency = s.transparency;
        m.rays.transparency = s.transparency;
        m.rays.reflection = s.reflection;
        m.rays.metallicity = s.metallicity;
        // : the bit 0x8 does not change the surface as 3.0
        m.rays.blended = s.blended;
        // : no angle law; as 3.0 the refraction value counts as 0 for the tint
        m.rays.law_transparency = 0.0;
        if self.version == RenderAs::Bryce30 {
            m.rays.refraction = 0.0;
        }
        m
    }

    /// Whether the start ray is sent at this surface: as 3.0 and 3.1 always (for the version's
    /// surfaces that send one), as 5.0 while the rays the path has gone through before the
    /// surface are below 100.,
    fn start_ray_sent(&self, request: &SurfaceRequest) -> bool {
        // `PathState::rays` counts the incoming ray too.
        self.version != RenderAs::Bryce50 || request.state.rays - 1 < START_RAY_LIMIT_FIVE
    }

    /// The start ray of a surface (the Light mode's, or bit 0x2000's): straight on from the hit
    /// as the blended ray goes, whatever t' is.,
    fn queue_start(&mut self, request: &SurfaceRequest, secondary: &mut SecondaryQueue) -> Result<(), OpenItem> {
        // : the start ray gathers as any ray
        let state = request.state;
        let (met, far) = self.surface_media(request);
        let media = match far {
            Some(f) => f?,
            None => met,
        };
        let depth = if self.version == RenderAs::Bryce50 { request.depth } else { request.depth + 1 };
        secondary.push(SecondaryRay {
            kind: SecondaryKind::Start,
            ray: CameraRay { origin: request.hit.point, dir: request.ray.dir, start: further::RAY_START },
            weight: [1.0; 3],
            total: [1.0; 3],
            depth,
            pixel: request.pixel,
            sample: request.sample,
            // (the medium beyond, the travelled distance and starting height of the
            // incoming ray): as the blended ray's state
            state: PathState {
                media,
                tir_count: state.tir_count,
                rays: state.rays + 1,
                air: state.air.continued(request.ray.origin, request.hit.t),
                set_showing: state.set_showing,
            },
        });
        Ok(())
    }

    /// The rest of a surface's shading:'s light terms from the colour `start` (s0), then
    /// and the rays sends.
    fn shade_from(
        &mut self,
        request: &SurfaceRequest,
        material: SurfaceMaterial,
        shading_normal: Vec3,
        met_before: bool,
        start: [f32; 3],
        secondary: &mut SecondaryQueue,
    ) -> Result<[f64; 3], OpenItem> {
        let at = crate::shading::lighting::SurfacePoint {
            point: request.hit.point,
            ray_direction: request.ray.dir,
            geometric_normal: request.hit.normal,
            shading_normal,
        };
        let state = request.state;
        // : the meeting, and the far side of a solid's surface; of a
        // shape that encloses no volume, the incoming ray's medium. The shadow rays are rays of
        // the path after the ray whose hit is lit (The media).
        let (met, far) = self.surface_media(request);
        let mut tracer = ShadowTracer {
            version: self.version,
            ray_depth: request.depth,
            start: ShadowStart { incoming: met, far, toward_eye: request.ray.dir.map(|x| -x), object: Some(request.hit.object) },
            geometry: &self.geometry,
            ctx: &self.ctx,
            surfaces: &self.shadow_surfaces,
            materials: &self.materials,
            solid_of: &self.solid_of,
            material_of: &self.material_of,
            met: &self.met,
            tables: self.tables.view(),
            switches: self.scene_light.switches,
            transmission: self.scene_light.switches & scene_switch::TRANSMISSION != 0,
            clouds: self.clouds.as_ref(),
            radii: &self.radii,
            generator: Some(&self.generator),
            // : a hit of the tracer, the ray's sample number, the
            // grid of the render of a frame (the quick preview's is Open: refused there).
            soft: Some(SoftAt {
                generator: &self.generator,
                sample: request.premium.map(|p| (p.number, p.n)),
                grid: self.soft_grid,
            }),
            error: None,
        };
        // : the material's flag word; s0 black, or the
        // colour of the start ray of bit 0x2000 (`start`); the gathered colour comes after the
        // light terms (below).
        let point = SurfaceInputs { flags: material.flags, start, ..SurfaceInputs::PLAIN };
        let orders = DotOrders::of(self.version);
        // (The lights with a beam: I times the beam's factor, 0 outside; Gels:
        // each gel's colour toward this point).
        let gel_lights;
        let lights = if self.gels.iter().all(Option::is_none)
            && self.beams.iter().all(Option::is_none)
            && self.gradients.iter().all(Option::is_none)
        {
            &self.lit
        } else {
            gel_lights = self.lights_at(request.hit.point)?;
            &gel_lights
        };
        // : the sun's colour and intensity at the point, asked
        // before the sun's shadow rays
        let washed_lights;
        let lights = if self.wash.is_some() && (material.light.diffusion > 0.0 || material.light.specularity > 0.0) {
            washed_lights = self.sun_lit_at(lights, request)?;
            &washed_lights
        } else {
            lights
        };
        let terms = light_terms(self.version, orders, &at, &material.light, &point, &self.scene_light, lights, &mut tracer);
        if let Some(e) = tracer.error {
            return Err(e);
        }
        // , point 5: asked where's
        // conditions hold; only a ray with a sample number sends the ambient ray, and a ray
        // without one gathers nothing.
        if !(terms.gathered_asked && request.premium.is_some()) {
            return self.finish(request, &terms, &material, met_before, None, secondary);
        }
        // : the order of
        // the two rays is not stated.
        if material.mode.is_some() || material.flags & ADDITIVE != 0 {
            return Err(open::LIGHT_MODE_GATHERED);
        }
        // The rays the path has gone through before this surface (0 for a camera ray):
        // `PathState::rays` counts the incoming ray too.
        let before = state.rays - 1;
        if before >= self.depth_limit {
            // point 5: no ray; the colour gathered is black (green still rounded).
            return self.finish(request, &terms, &material, met_before, Some([0.0; 3]), secondary);
        }
        // : the ambient ray gathers as any
        // ray of the tree (no picture holds one)
        // point 5: from p, counting from 0.0001, weight and total (1, 1, 1), the
        // depth of the surface's own ray, a count one more, the surface's ray's medium; its
        // direction is turned about g by the stage (two draws). (the count
        // of total internal reflections carried: the surface's ray's)
        secondary.push_turned(
            SecondaryRay {
                kind: SecondaryKind::Ambient,
                ray: CameraRay { origin: request.hit.point, dir: request.hit.normal, start: further::RAY_START },
                weight: [1.0; 3],
                total: [1.0; 3],
                depth: request.depth,
                pixel: request.pixel,
                sample: request.sample,
                // point 5 ("its air is that of any further ray"), (Further
                // rays): a path of its own from p, D the direction the stage turns it to.
                // (The air over blurred and ambient rays)
                // The surface's ray's medium, with its meeting of the surface (a later ray of
                // the path). (The held solids and the meetings)
                state: PathState { media: met, tir_count: state.tir_count, rays: state.rays + 1, air: AirPath::Own, set_showing: state.set_showing },
            },
            RayTurn::Ambient,
        );
        self.pending.push(Pending::Gathering(PendingSurface { terms, material, met_before }));
        Ok([0.0; 3])
    }

    /// The path's media after its ray met this surface (the meeting kept, for a solid), and
    /// for a solid the media of a ray sent through it, or the Open subject that keeps them from
    /// being known.
    /// (The held solids and the meetings)
    fn surface_media(&self, request: &SurfaceRequest) -> (PathMedia, Option<Result<PathMedia, OpenItem>>) {
        let o = request.hit.object;
        let solid = self.solid_of[o];
        if !self.encloses[solid] {
            return (request.state.media, None);
        }
        let met = request.state.media.met(solid);
        (met, Some(met.through_as(solid, request.hit.medium_object(self.version))))
    }

    /// The medium law of the solid a path is in, `None` outside every object.
    fn medium_law(&self, media: &PathMedia) -> Result<Option<MediumLaw>, OpenItem> {
        match media.current_material() {
            None => Ok(None),
            Some(i) => law_of(&surface_of(&self.shadow_surfaces, &self.geometry, i, None, self.scene_light.switches, self.version)),
        }
    }

    /// The medium law of the solid a ray travels in, with the set object's material as the
    /// latest hit on any set had left it when the ray's own hit was found.
    fn medium_law_sent(&self, state: &PathState) -> Result<Option<MediumLaw>, OpenItem> {
        match state.media.current_material() {
            None => Ok(None),
            Some(i) => law_of(&surface_of(&self.shadow_surfaces, &self.geometry, i, state.set_showing, self.scene_light.switches, self.version)),
        }
    }

    /// The surface finished:'s ambient, gathered and dome terms with `gathered`, then the
    /// further rays sends, queued in the order sent, each blurred ray with its cone
    /// (point 4: the stage draws and turns it), and the blended ray.
    /// point 4
    fn finish(
        &mut self,
        request: &SurfaceRequest,
        terms: &LightTerms,
        material: &SurfaceMaterial,
        met_before: bool,
        gathered: Option<[f32; 3]>,
        secondary: &mut SecondaryQueue,
    ) -> Result<[f64; 3], OpenItem> {
        let c = terms.finish(self.version, &material.light, &self.scene_light, gathered);
        let encloses = self.encloses[self.solid_of[request.hit.object]];
        let state = request.state;
        let (met, far) = self.surface_media(request);
        // : r = η1 / η2, η2 of a shape that encloses no volume its own
        // material's; where the ray stays in the medium it meets the surface in, η1 = η2.
        let incoming = self.medium_law(&state.media)?.map(|m| m.refraction);
        // ,, : a terrain that encloses no volume
        // is held by its crossings: at its second, fourth, ... crossing along the path, η2 = η1
        // (r = 1) and the ray goes on in the incoming ray's medium. The plane, the square and
        // the disc have no slot and are never held.
        let terrain_slot = self.terrain_slot[self.solid_of[request.hit.object]];
        let held_terrain = !encloses && terrain_slot.is_some_and(|k| state.media.holds_terrain(k));
        let far_media = match far {
            Some(Ok(f)) => Some(f),
            // The medium beyond Open (or a design guard): refused where a ray could go through.
            Some(Err(e)) if material.rays.transparency > 0.0 => return Err(e),
            Some(Err(_)) | None => None,
        };
        let beyond = match far_media {
            Some(f) => self.medium_law(&f)?.map(|m| m.refraction),
            None if held_terrain => incoming,
            None => Some(material.rays.refraction),
        };
        let hit = RayHit {
            point: request.hit.point,
            ray_direction: request.ray.dir,
            depth: request.depth,
            depth_limit: self.depth_limit,
            total: request.total,
            geometric_normal: request.hit.normal,
            shading_normal: c.shading_normal,
            encloses_volume: encloses,
            switches: self.scene_light.switches,
            media: RayMedia { incoming, beyond },
            met_before,
            tir_count: state.tir_count,
            tir_limit: self.tir_limit,
        };
        let sent = further::send(self.version, &hit, &material.rays);
        // : after the haze, when t > 0, a ray straight on,
        // as 3.0 at any depth, as 3.1 while the incoming ray's depth is below 100, as 5.0 while
        // the rays the path has gone through before this surface are below 100 (0 for a camera
        // ray, point 5: `PathState::rays` less the incoming ray).
        let t = material.rays.transparency;
        let blended = material.rays.blended
            && t > 0.0
            && match self.version {
                RenderAs::Bryce30 => true,
                RenderAs::Bryce31 => request.depth < BLEND_LIMIT,
                RenderAs::Bryce50 => state.rays - 1 < BLEND_LIMIT,
            };
        // : the reflected, the transmitted and
        // the blended rays gather as any ray does (`after_replacement`).
        // A ray sent through a terrain that encloses no volume counts the crossing for it.
        let counted = match terrain_slot {
            Some(k) if !encloses => met.crossed_terrain(k),
            _ => met,
        };
        let far_or_incoming = |medium: Medium, through: bool| -> Result<PathMedia, OpenItem> {
            match (medium, far) {
                (Medium::Incoming, _) | (Medium::Beyond, None) => Ok(if through { counted } else { met }),
                (Medium::Beyond, Some(f)) => f,
            }
        };
        // point 4: a ray with a number, its own bit and ρb ≠ 0 is turned in its
        // cone (after the cut-off and the reflected ray's test, which `send` made); the stage
        // takes its two draws immediately before tracing it.
        let numbered = self.version == RenderAs::Bryce50 && request.premium.is_some();
        // : a placeholder until the ray's own hit is found (`ray_leaves` then
        // notes the latest showing).
        let sent_showing = self.geometry.scene.metaballs.as_ref().and_then(|ms| ms.latest_id()).or(state.set_showing);
        for r in sent.iter() {
            let media = far_or_incoming(r.medium, r.kind == RayKind::Transmitted)?;
            let turn = match premium::cone_radius(self.scene_light.switches, r.kind, numbered, material.rays.halo) {
                Some(radius) => RayTurn::Cone { radius },
                None => RayTurn::None,
            };
            secondary.push_turned(SecondaryRay {
                kind: match r.kind {
                    RayKind::Reflected => SecondaryKind::Reflection,
                    RayKind::Transmitted => SecondaryKind::Transmission,
                },
                ray: CameraRay { origin: r.origin, dir: r.direction, start: r.start },
                // W: the weight as sent, for a ray outside every object .
                weight: r.weight,
                total: r.total,
                depth: r.depth,
                pixel: request.pixel,
                sample: request.sample,
                // : a reflected or transmitted ray starts a path of
                // its own at its origin; a blurred one too, its D the turned direction the
                // stage traces (point 4;, The air over blurred and
                // ambient rays).
                state: PathState { media, tir_count: r.tir_count, rays: state.rays + 1, air: AirPath::Own, set_showing: sent_showing },
            }, turn);
        }
        if blended {
            // point 4: the straight-on ray of a blended material is not turned.
            // : from p, straight on, counting from 0.0001,
            // in the medium on the far side, weight and total weight (1, 1, 1), depth + 1 as 3.0
            // and 3.1 and the same as 5.0. Its weight is not used: the share t rides in it.
            let depth = if self.version == RenderAs::Bryce50 { request.depth } else { request.depth + 1 };
            self.blend_shares.push(t);
            secondary.push(SecondaryRay {
                kind: SecondaryKind::Blended,
                ray: CameraRay { origin: request.hit.point, dir: request.ray.dir, start: further::RAY_START },
                // A plain material's t, a single widened (a blended one with textures is refused).
                weight: [t as f32; 3],
                total: [1.0; 3],
                depth,
                pixel: request.pixel,
                sample: request.sample,
                // (Further rays: a continued ray continues the path of the ray
                // that met the blended surface: its start, and its L so far).
                state: PathState {
                    media: far_or_incoming(Medium::Beyond, true)?,
                    tir_count: state.tir_count,
                    rays: state.rays + 1,
                    air: state.air.continued(request.ray.origin, request.hit.t),
                set_showing: state.set_showing,
                },
            });
        }
        // : the continuing ray, after the reflected ray; its colour
        // b mixes in by F after the air (Fuzzy solids).
        if let (Some(fz), false) = (material.fuzzy, self.skip_continuing) {
            let kind = self.fuzzy_kind_of(request.hit.object).ok_or(open::FUZZY_SHAPE)?;
            let pending = FuzzyPending { object: request.hit.object, kind, a: fz.a, t: fz.t, inside: false };
            self.queue_continuing(request, pending, secondary)?;
        }
        Ok(c.colour.map(f64::from))
    }

    /// Queues a fuzzy surface's continuing ray and keeps what its weight needs until it comes
    /// back: from P straight on, counting from 0.0001, weight and total (1, 1, 1), depth + 1 as
    /// 3.0 and 3.1 and the same as 5.0, the incoming ray's air continued, in the medium on the
    /// far side (a solid) or the incoming ray's (a plane); not turned. Where the blended ray's
    /// limit of 100 would hold it back: Open., (The blended
    /// material); (Fuzzy solids)
    fn queue_continuing(
        &mut self,
        request: &SurfaceRequest,
        pending: FuzzyPending,
        secondary: &mut SecondaryQueue,
    ) -> Result<bool, OpenItem> {
        let state = request.state;
        // ,: as 3.0 and
        // 3.1 the continuing ray is sent at any depth and any count of rays; as 5.0 only while
        // the number of rays the path has gone through before this surface is below 100, held by
        // neither L nor the preview's limit; not sent, F is 1 (the surface counts as an opaque one).
        if self.version == RenderAs::Bryce50 && state.rays - 1 >= BLEND_LIMIT {
            return Ok(false);
        }
        // : the continuing ray gathers as any ray
        let (met, far) = self.surface_media(request);
        let media = match (pending.kind, far) {
            (FuzzyShape::Solid, Some(f)) => f?,
            // , : the crossing of a terrain that encloses no volume
            // is counted for the path
            (FuzzyShape::Plane, _) => match self.terrain_slot[self.solid_of[request.hit.object]] {
                Some(k) if !self.encloses[self.solid_of[request.hit.object]] => met.crossed_terrain(k),
                _ => met,
            },
            _ => met,
        };
        let depth = if self.version == RenderAs::Bryce50 { request.depth } else { request.depth + 1 };
        secondary.push(SecondaryRay {
            kind: SecondaryKind::Fuzzy,
            ray: CameraRay { origin: request.hit.point, dir: request.ray.dir, start: further::RAY_START },
            weight: [1.0; 3],
            total: [1.0; 3],
            depth,
            pixel: request.pixel,
            sample: request.sample,
            // (Further rays: a continued ray continues the path of the ray that
            // met the surface),
            state: PathState {
                media,
                tir_count: state.tir_count,
                rays: state.rays + 1,
                air: state.air.continued(request.ray.origin, request.hit.t),
                set_showing: state.set_showing,
            },
        });
        self.fuzzy_pending.push(pending);
        Ok(true)
    }

    /// The Transparency A value a of the stand-in's material when the weight is asked: the
    /// latest hand-over once the continuing ray is back (its hit, its lighting's shadow rays and
    /// every ray it sent counted), where `object` is the stand-in and a hit on a set or on wood
    /// was recorded; `None` otherwise (the surface's own a stands). t stays the Transparency at
    /// the fuzzy surface's hit. (What is asked of the hit object's material
    /// afterwards reads the material as the latest hand-over left it), (The weight of a
    /// solid: w from a, f from t and a). The moment is ours, fitted to fw-06 and fw-07 (question
    /// 0625). (Fuzzy wood and fuzzy leaves)
    fn stand_transparency(&self, object: usize) -> Option<f64> {
        use crate::geometry::metaball::Showing;
        let ms = self.geometry.scene.metaballs.as_ref().filter(|m| m.stand == object)?;
        match ms.latest()? {
            Showing::Made(m) => Some(f64::from(m.values[3])),
            Showing::Wood(tree) => self.shadow_surfaces[tree].as_ref().ok().map(|s| f64::from(s.transparency)),
            Showing::Own(_) => None,
        }
    }

    /// The kind of fuzzy surface the geometry object `o` is.
    fn fuzzy_kind_of(&self, o: usize) -> Option<FuzzyShape> {
        fuzzy_kind(&self.geometry.scene.objects[o].shape, self.encloses[self.solid_of[o]])
    }

    /// Whether geometry object `object` is a fuzzy solid that `media`'s path holds (a ray of
    /// that path meets its surface from inside).
    fn fuzzy_from_inside(&self, object: usize, media: &PathMedia) -> bool {
        let Ok(s) = surface_of(&self.shadow_surfaces, &self.geometry, object, None, self.scene_light.switches, self.version) else {
            return false;
        };
        match s.fuzzy {
            Some(FuzzyShape::Solid) => media.holds(self.solid_of[object]),
            Some(FuzzyShape::Plane) => self.terrain_held_by(object, media),
            None => false,
        }
    }

    /// Q for a fuzzy solid `object` whose continuing ray `ray` first met `first`: where the ray
    /// stops. A surface of a fuzzy material, or of a blended one with a Transparency above 0, lets
    /// the ray go on (its own ray straight on, in the same direction), so the ray stops at the
    /// first surface of the solid itself or of anything else that does not let it through; a ray
    /// that goes on and meets nothing stops at the last such surface. Observed: the pictures
    /// i-blend, i-fuzzy, i-self and i-opaque of `fuzzy-more-pictures` (a ball of each kind inside
    /// a fuzzy ball); the way the original finds the point is not stated.
    /// (step 4: where the continuing ray leaves O, or where it stops; Open: surfaces of
    /// fuzzy or blended materials between the entry and the exit)
    /// (Revision of)
    fn stopping_point(&self, object: usize, ray: &SecondaryRay, first: Hit) -> Result<Vec3, OpenItem> {
        let mut hit = first;
        // a design guard against a scene that never lets the ray stop
        for _ in 0..64 {
            if self.solid_of[hit.object] == self.solid_of[object] {
                break;
            }
            let s = surface_of(&self.shadow_surfaces, &self.geometry, hit.object, hit.meta, self.scene_light.switches, self.version)?;
            if !(s.fuzzy.is_some() || (s.blended && s.transparency > 0.0)) {
                break;
            }
            let onward = Ray { origin: hit.point, dir: ray.ray.dir };
            let range = Range { near: further::RAY_START, far: f64::INFINITY };
            match self.geometry.scene.intersect(&onward, range, &self.ctx) {
                Some(next) => hit = next,
                None => break,
            }
        }
        Ok(hit.point)
    }

    /// Whether `object` is a terrain that encloses no volume which `media`'s path holds by its
    /// crossings.
    fn terrain_held_by(&self, object: usize, media: &PathMedia) -> bool {
        let solid = self.solid_of[object];
        !self.encloses[solid] && self.terrain_slot[solid].is_some_and(|k| media.holds_terrain(k))
    }

    /// What a fuzzy surface's continuing ray brings back: its colour, and how it replaces the
    /// surface's: itself from inside O; for a plane by F = 1 − t; for a solid by the weight of
    /// the stretch from P (the ray's origin) to Q, where the continuing ray leaves O or is
    /// stopped by another object.
    fn fuzzy_back(&mut self, ray: &SecondaryRay, hit: Option<&Hit>, colour: [f32; 3]) -> Result<BroughtBack, OpenItem> {
        let p = self.fuzzy_pending.pop().expect("a continuing ray comes back to the surface that sent it");
        let replace = if p.inside {
            Replace::Through
        } else {
            let f = match p.kind {
                FuzzyShape::Plane => fuzzy::plane_weight(p.t),
                FuzzyShape::Solid => {
                    // : where the continuing ray from wood (or, the stated
                    // fallback, from a leaf) meets nothing at all, Q is the world's origin.
                    // states Q for the wood and for leaves whatever the other object is
                    // ("or else its first hit on another object"); leaves a fuzzy or
                    // blended surface between the entry and the exit Open for the other solids.
                    let of_tree = matches!(self.geometry.scene.objects[p.object].shape, Shape::Tree { .. } | Shape::TreeLeaves { .. } | Shape::MetaSet);
                    // : Q is the point where the continuing ray
                    // leaves O, or, where something else stops it first, the point where it stops
                    // (surfaces of fuzzy or blended materials between included: pictured, i-blend,
                    // i-fuzzy, i-self of fuzzy-more-pictures).
                    let far = match hit {
                        Some(h) if of_tree => Some(h.point),
                        Some(h) => Some(self.stopping_point(p.object, ray, *h)?),
                        // : where it then
                        // meets nothing at all, as 5.0 Q is the world's origin (pictured:
                        // short-stretch-pictures); as 3.0 and 3.1 Q is a point no rule gives (the
                        // implementer's choice below)
                        None if of_tree || self.version == RenderAs::Bryce50 => Some([0.0; 3]),
                        None => None,
                    };
                    let own = |q: Vec3| fuzzy_own(&self.geometry.scene, p.object, q);
                    let orders = FuzzyOrders::of(self.version);
                    let scale = fuzzy_box_scale(&self.geometry.scene, &self.geometry.scene.objects[p.object].shape);
                    let a = self.stand_transparency(p.object).unwrap_or(p.a);
                    match far {
                        Some(far) => fuzzy::solid_weight_scaled(self.version, own(ray.ray.origin), own(far), scale, a, p.t, orders),
                        // A ray that meets nothing has no Q: the surface does not count (F = 0),
                        // the colour is what the ray brings back. Not stated for the original (
                        // Limits, as 3.0: "what the original 3.0 does there is not found"); the
                        // rules give the sky there. (Revision of)
                        None => 0.0,
                    }
                }
            };
            Replace::Fuzzy(f)
        };
        Ok(BroughtBack { colour, weight: ray.weight, replace: Some(replace) })
    }
}

impl SurfaceRules for PlainSurface {
    /// The local colour , then the further rays sends, queued in the
    /// order sent; or, where true ambience gathers, the ambient ray alone (`resume` finishes).
    fn surface(&mut self, _scene: &Scene, request: &SurfaceRequest, secondary: &mut SecondaryQueue) -> Result<[f64; 3], OpenItem> {
        self.begin_surface(request, secondary)
    }

    /// The set object's material as the latest hit on any set has left it when the ray's own
    /// hit has been found (that hit counted): what the weight of a ray in its medium reads.
    fn ray_leaves(&self, ray: &mut SecondaryRay) {
        if let Some(id) = self.geometry.scene.metaballs.as_ref().and_then(|ms| ms.latest_id()) {
            ray.state.set_showing = Some(id);
        }
    }

    /// The surface whose ambient ray brought back `gathered` (its colour as it comes back, no
    /// weight) finished:'s gathered term ⟨Cr·Ar·ka + or'⟩ and the rest, then its further
    /// rays. point 5,
    fn resume(
        &mut self,
        _scene: &Scene,
        request: &SurfaceRequest,
        gathered: [f32; 3],
        secondary: &mut SecondaryQueue,
    ) -> Result<[f64; 3], OpenItem> {
        match self.pending.pop().expect("resume follows a surface that sent its ambient ray") {
            Pending::Gathering(p) => self.finish(request, &p.terms, &p.material, p.met_before, Some(gathered), secondary),
            // : the start ray's colour is the colour s0 the surface's sum starts from
            Pending::Start { material, shading_normal, met_before } => {
                self.shade_from(request, material, shading_normal, met_before, gathered, secondary)
            }
            // : the fuzzy surface lit after its continuing ray (sent already)
            Pending::FuzzyLit { material, shading_normal, met_before } => {
                self.skip_continuing = true;
                let lit = self.shade_from(request, material, shading_normal, met_before, [0.0; 3], secondary);
                self.skip_continuing = false;
                lit
            }
        }
    }

    /// 's join: each sent ray's colour and weight added to the local colour, in the
    /// order sent.
    fn join(&mut self, local: [f64; 3], brought_back: &[BroughtBack]) -> Result<[f64; 3], OpenItem> {
        Ok(further::join(local.map(|c| c as f32), brought_back).map(f64::from))
    }

    /// The air of the ray's path over the surface's colour joined with what its reflected
    /// and transmitted rays brought back: L the hit's t from the ray's origin (plus the path
    /// so far for a continued ray), s the path's start height, D as sent. Then, for a camera
    /// ray, the light in the air (no further ray is sent while it acts).
    /// (Which rays; Further rays: Order at a surface, a reflected or a
    /// transmitted ray, D as sent, a continued ray, in a solid's medium), (The colour
    /// added: after the surface's colour and the air over it, the places up to d)
    fn over_joined(&mut self, request: &SurfaceRequest, colour: [f32; 3]) -> Result<[f32; 3], OpenItem> {
        // (A volume material, met from outside: the surface lays no air; the continuing
        // ray comes back with the air of the whole path, nothing more), (the same for the
        // light in the air)
        if self.is_volume(request.hit.object, request.hit.meta) {
            return Ok(colour);
        }
        let mut colour = colour;
        if let Some(air) = &self.air {
            let ray = request.state.air.to_hit(request.ray.origin[1], request.ray.dir, request.hit.t);
            colour = air.over(self.version, colour, &ray);
        }
        // (The rainbow, step 1: for a ray that meets a surface, after the fog and the
        // haze, whatever bits 0x1, 0x80 and 0x400 hold); lo the ray's start, hi its hit.
        if let Some(bow) = &self.rainbow {
            colour = bow.over(colour, request.ray.origin, request.ray.dir, request.ray.start, Some(request.hit.t));
        }
        // : the bow first; the light in the air is
        // added after it and after's blended replacement (`after_replacement`).
        // : as 3.0 and 3.1 a volume
        // material's surface lays its ray's light over what the continuing ray brought back
        // (`after_replacement`); as 5.0 the light laid here is replaced with the colour.
        if self.version == RenderAs::Bryce50 {
            if let Some(lit) = &self.lit_air {
                colour = lit.add(&request.ray, Some(request.hit.t), request.depth, request.state.air, request.state.media, colour)?;
            }
        }
        Ok(colour)
    }

    /// The light in the air along the ray that met the surface: its places up to the hit, the
    /// path so far behind a continued ray, V added to the colour the replacement left.
    /// (When it acts; The places; The colour added: after's blended
    /// replacement; As 5.0: the same place)
    fn after_replacement(&mut self, request: &SurfaceRequest, colour: [f32; 3]) -> Result<[f32; 3], OpenItem> {
        match &self.lit_air {
            Some(lit) if self.version != RenderAs::Bryce50 => lit.add(&request.ray, Some(request.hit.t), request.depth, request.state.air, request.state.media, colour),
            _ => Ok(colour),
        }
    }

    /// : the sample times the weight the medium of the solid that holds the eye gives
    /// the camera ray (from its own hit, or its miss). (The medium a camera ray
    /// starts in),
    fn camera_sample(&mut self, ray: &CameraRay, hit: Option<&Hit>, colour: [f32; 3]) -> Result<[f32; 3], OpenItem> {
        // (The weight a ray comes back with: a ray that meets a volume material's
        // surface comes back with the weight it was sent with)
        let as_sent = hit.is_some_and(|h| self.is_volume(h.object, h.meta));
        let colour = self.eye.apply(ray, hit.map(|h| (h.t, h.point)), as_sent, colour)?;
        // (Errors, Observed: a camera hit whose normal is not a number, the torus
        // at rho = 0, draws level 1 on each channel in every version). The colour below is
        // level 1 on the scale of 3.0 and 3.1 and of 5.0 without gamma or dithering; with
        // them, and with several samples a pixel:.
        // (A normal that is not a number)
        if hit.is_some_and(|h| h.normal.iter().any(|c| c.is_nan())) {
            return Ok([NAN_NORMAL_SAMPLE; 3]);
        }
        Ok(colour)
    }

    /// A camera ray starts in the medium of the solid that holds the eye.
    fn camera_state(&self, ray: &CameraRay) -> Result<PathState, OpenItem> {
        if !self.eye.any() {
            return Ok(PathState::CAMERA);
        }
        Ok(PathState { media: self.eye.start_media(ray)?, ..PathState::CAMERA })
    }

    /// W: the weight as sent outside every object; in a solid's medium ⟨⟨F·wk⟩·Φk⟩, F from the
    /// ray's hit (its t along its direction) or its miss. The blended ray brings back its
    /// colour and the share; a fuzzy surface's continuing ray its colour and F .
    fn brought_back(&mut self, ray: &SecondaryRay, hit: Option<&Hit>, colour: [f32; 3]) -> Result<BroughtBack, OpenItem> {
        if ray.kind == SecondaryKind::Blended {
            // The share t as a double (a plain material's is a single widened; a Light-mode
            // surface's V is not a single).,
            let share = self.blend_shares.pop().expect("a blended ray comes back to the surface that sent it");
            return Ok(BroughtBack { colour, weight: ray.weight, replace: Some(Replace::Share(share)) });
        }
        if ray.kind == SecondaryKind::Fuzzy {
            return self.fuzzy_back(ray, hit, colour);
        }
        // (A volume material's surface: the ray that goes on brings back its
        // colour, which replaces the surface's; its weight is not used)
        if ray.kind == SecondaryKind::VolumeThrough {
            return Ok(BroughtBack { colour, weight: ray.weight, replace: Some(Replace::Through) });
        }
        // (The weight a ray comes back with: a ray that meets the surface of a
        // volume material's object comes back with the weight it was sent with)
        if hit.is_some_and(|h| self.is_volume(h.object, h.meta)) {
            return Ok(BroughtBack::further(colour, ray.weight));
        }
        // : a ray that meets a fuzzy solid's surface from inside (its path
        // holds the solid) is not shaded there; "its weight is as it was".
        // (Fuzzy solids: the medium)
        if hit.is_some_and(|h| self.fuzzy_from_inside(h.object, &ray.state.media)) {
            return Ok(BroughtBack::further(colour, ray.weight));
        }
        let weight = match self.medium_law_sent(&ray.state)? {
            None => ray.weight,
            Some(m) => {
                let solid = ray.state.media.current_material().expect("a medium is a solid's");
                let s = surface_of(&self.shadow_surfaces, &self.geometry, solid, ray.state.set_showing, self.scene_light.switches, self.version)?;
                let placement = &self.geometry.scene.objects[solid].placement;
                let m = super::media::tinted(self.version, m, s.tint, placement, hit.map(|h| (h.t, h.point)))?;
                m.weight_back(self.version, ray.weight, hit.map(|h| h.t))
            }
        };
        Ok(BroughtBack::further(colour, weight))
    }

    fn march(
        &mut self,
        ray: &CameraRay,
        depth: u32,
        state: &PathState,
        hit: Option<&Hit>,
        premium: Option<super::stages::PremiumSample>,
    ) -> Result<Option<crate::shading::volume::Laying>, OpenItem> {
        self.march_sample.set(premium.map(|p| (p.number, p.n)));
        let laid = self.march_ray(ray, depth, state, hit, premium.is_some());
        self.march_sample.set(None);
        laid
    }

    /// Where a light has a radius above 0 (as 5.0 only): its shadows draw.
    fn draws(&self) -> bool {
        // : as 5.0 a volume material with the bit draws too
        self.radii.iter().any(|&(r, _)| r > 0.0 || r < 0.0) || self.jitters() || self.wash.is_some()
    }

    /// , point 5
    fn march_asks(&mut self) -> Vec<SecondaryRay> {
        self.take_march_asks()
    }

    /// , point 5
    fn march_gathered(&mut self, gathered: Vec<[f32; 3]>) {
        self.give_march_gathered(gathered);
    }

    fn is_volume_surface(&self, request: &SurfaceRequest) -> bool {
        self.is_volume(request.hit.object, request.hit.meta)
    }

    fn wash(
        &mut self,
        ray: &CameraRay,
        depth: u32,
        state: &PathState,
        hit: Option<f64>,
        premium: Option<stages::PremiumSample>,
        colour: [f32; 3],
    ) -> Result<[f32; 3], OpenItem> {
        self.wash_ray(ray, depth, state, hit, premium, colour)
    }

    fn wash_effect(
        &mut self,
        ray: &CameraRay,
        depth: u32,
        state: &PathState,
        hit: Option<f64>,
        premium: Option<stages::PremiumSample>,
    ) -> Result<Option<crate::sky::wash::WashEffect>, OpenItem> {
        self.wash_effect_of(ray, depth, state, hit, premium)
    }

    /// ; (Shading that draws)
    fn swap_generator(&mut self, generator: &mut CRand) {
        let mine = self.generator.get();
        self.generator.set(*generator);
        *generator = mine;
    }
}

/// The sky rules: the plain sky, with the air over it where bit 0x80 lays it.
#[derive(Clone, Debug)]
pub struct PlainSky {
    version: RenderAs,
    colour: [f32; 3],
    /// The sky's own colours where gives them (as 5.0 with bit 0x2, 0x4 or 0x8; as 3.0
    /// and 3.1 with bit 0x1), else the plain colour.
    colours: Option<SkyColours>,
    air: Option<Air>,
    /// Set by `render_lit` from the surface rules'.
    lit_air: Option<LitAirGather>,
    /// The cloud layers laid over the sky's colour.
    clouds: Option<SceneClouds>,
    /// The star field laid after the layers, before the air.
    stars: Option<SkyStars>,
    /// Where the star list made for this render left the generator: the render's draws
    /// start there (`before_first_pixel`).
    list_end: Option<CRand>,
    /// As 5.0 for a sky part of version 2 or 7: the sky and its field as makes them (in
    /// place of `colours` and `stars`).
    five: Option<crate::sky::five::FiveSky>,
    /// The rainbow, laid over a ray that meets nothing after the air: as 3.0 and 3.1 only where
    /// the air acts on it (`bow_with_air`), as 5.0 always.
    rainbow: Option<Rainbow>,
    bow_with_air: bool,
}

impl PlainSky {
    /// Whether the sky holds nothing that a copy shares with it between threads (the light in
    /// the air is set from the surface rules after it is made).
    pub(super) fn reads_texture_context(&self) -> bool {
        self.clouds.is_some()
    }

    pub(super) fn shareable(&self) -> bool {
        self.lit_air.is_none()
    }

    /// ; as 5.0 for versions 2 and 7,
    pub fn new(scene: &Scene, plan: &RenderPlan) -> Result<PlainSky, OpenItem> {
        let version = plan.version;
        let prepared = prepared_sky(scene, version, plan.haze_five)?;
        if sky_five::reads_five(scene, version) {
            let (part, w) = sky_five::five_settings(scene)?;
            let (five, list_end) = sky_five::five_sky(scene, plan, &part, &w)?;
            return Ok(PlainSky {
                version,
                colour: plain_sky(scene.sky.plain_sky_colour),
                colours: None,
                air: scene_air(scene, version, &prepared.main, plan.haze_five),
                lit_air: None,
                clouds: scene_clouds(scene, version, &plan.texture_tables)?,
                stars: None,
                list_end,
                five: Some(five),
                rainbow: scene_rainbow(scene, version, &prepared.main),
                bow_with_air: false,
            });
        }
        let s = &scene.sky;
        let settings = SkySettings {
            switches: sky_colour_switches(version, s.flags).ok_or(open::SKY_SWITCHES)?,
            sky_colour: s.sky_colour,
            horizon_colour: s.horizon_colour,
            plain_colour: s.plain_sky_colour,
            haze: s.haze,
            moon: s.moon,
            discs: s.discs,
        };
        // ; the air over it
        // there is not stated (refused).
        let plain_five = version == RenderAs::Bryce50 && s.flags & SKY_EFFECTS_OFF != 0;
        let air = scene_air(scene, version, &prepared.main, plan.haze_five);
        if plain_five && air.is_some() {
            return Err(open::SKY_SWITCHES);
        }
        let mut colours = if plain_five { None } else { SkyColours::new(version, &sun_inputs(scene, version), &settings) };
        // : the moon's disc shows the user's
        // picture, which a render lacking is refused (Errors).
        if settings.switches & crate::sky::sky_colours::MOON_PICTURE != 0 {
            if let Some(c) = colours.take() {
                colours = Some(c.with_moon_picture(moon_image(plan)?));
            }
        }
        // (as 3.0 and 3.1 the field shows in the sky with atmosphere: bit 0x1 set
        // and bit 0x800 clear, which is where gives the sky's own colours)
        let field = scene_stars(scene, plan, &prepared.main, colours.is_some())?;
        Ok(PlainSky {
            stars: field.laid,
            list_end: field.list_end,
            version,
            colour: plain_sky(scene.sky.plain_sky_colour),
            colours,
            air,
            lit_air: None,
            clouds: scene_clouds(scene, version, &plan.texture_tables)?,
            five: None,
            rainbow: scene_rainbow(scene, version, &prepared.main),
            bow_with_air: version != RenderAs::Bryce50 && air_word_acts(s.flags),
        })
    }

    /// The sky as 5.0  with the layers: the colour after step 6 and Ψ, the layers over
    /// it with their share of Ψ, the entries, step 7; the plain colour has nothing laid over
    /// it.,
    fn five_colour(&self, five: &crate::sky::five::FiveSky, request: &SkyRequest) -> Result<[f32; 3], OpenItem> {
        let dir = request.ray.dir;
        let Some((mut c, mut seen)) = five.colour_seen(dir) else {
            return Ok(self.colour);
        };
        if let Some(cl) = &self.clouds {
            let (after, covers) = cl
                .render
                .colour_covers(cl.textures(), request.ray.origin, cloud_path_start(request), dir, c, crate::textures::cloud_reading_context(self.version))
                .map_err(|_| open::CLOUD_EVALUATION)?;
            c = after;
            seen = seen_after_layers(self.version, seen, covers);
        }
        Ok(five.finish(dir, c, seen))
    }

    /// The sky's own colour (or the plain colour) with the layers over it, before the air.
    /// (the layers over's colour after its step 6, then step 7:
    /// each channel ⊕ 0; `own` has had it, and it is taken again after the layers)
    fn own_and_clouds(&self, request: &SkyRequest) -> Result<[f32; 3], OpenItem> {
        let own = match &self.colours {
            None => self.colour,
            Some(sc) => sc.colour(request.ray.dir),
        };
        Ok(match &self.clouds {
            None => own,
            Some(cl) => cl
                .render
                .colour(cl.textures(), request.ray.origin, cloud_path_start(request), request.ray.dir, own, crate::textures::cloud_reading_context(self.version))
                .map_err(|_| open::CLOUD_EVALUATION)?
                .map(|c| c + 0.0),
        })
    }

    /// As `own_and_clouds` with a star field: Ψ formed beside's steps and the layers,
    /// the entries laid after the layers, then step 7. (What a ray sees as 3.0
    /// and 3.1; Laying an entry as 3.0 and 3.1),
    fn with_stars(&self, stars: &SkyStars, sc: &SkyColours, request: &SkyRequest) -> Result<[f32; 3], OpenItem> {
        let dir = request.ray.dir;
        let (mut c, mut seen) = sc.colour_seen(dir);
        if let Some(cl) = &self.clouds {
            let (after, covers) = cl
                .render
                .colour_covers(cl.textures(), request.ray.origin, cloud_path_start(request), dir, c, crate::textures::cloud_reading_context(self.version))
                .map_err(|_| open::CLOUD_EVALUATION)?;
            c = after;
            seen = seen_after_layers(self.version, seen, covers);
        }
        let laid = stars.laying.lay_wide(&stars.list, &stars.grid, dir, c, held(seen), FieldSwitches::BOTH);
        Ok(laid.map(|x| x + 0.0)) // step 7
    }
}

impl SkyRules for PlainSky {
    /// , (a ray that meets nothing, bit
    /// 0x1 clear: the plain colour with the air over it, L = |c/(Dy + Dy)| or +infinity).
    /// A further ray's air as's "Further rays" says (`SkyRequest::air`). While the
    /// light in the air acts every ray that reaches here is a camera ray (`PlainSurface`
    /// refuses further rays then).
    fn sky(&mut self, _scene: &Scene, request: &SkyRequest) -> Result<[f64; 3], OpenItem> {
        // : the layers over the sky's own colour, before
        // the air; O the ray's origin (the eye, or the point a further ray leaves).
        // : the sky's own colour in the ray's direction, or the plain colour.
        let below = match (&self.five, &self.stars, &self.colours) {
            (Some(five), _, _) => self.five_colour(five, request)?,
            (None, Some(stars), Some(sc)) => self.with_stars(stars, sc, request)?,
            _ => self.own_and_clouds(request)?,
        };
        let c = match &self.air {
            None => below,
            Some(air) => {
                // : L = |c/(Dy + Dy)| with the ray's own Dy as
                // sent; s its origin's height, or for a continued ray its path's start.
                let ray = request.air.to_nothing(request.ray.origin[1], request.ray.dir, air);
                air.over(self.version, below, &ray)
            }
        };
        // (The rainbow, step 1: for a ray that meets nothing, as 3.0 and 3.1 only
        // where the air acts on it, after the air; as 5.0 whatever bits 0x80 and 0x400 hold).
        // The ray has no upper end of its parameter.
        let c = match &self.rainbow {
            Some(bow) if self.bow_with_air || self.version == RenderAs::Bryce50 => {
                bow.over(c, request.ray.origin, request.ray.dir, request.ray.start, None)
            }
            _ => c,
        };
        // : after the sky's colour and the air over it; a ray
        // that meets nothing gathers up to 64.
        let c = match &self.lit_air {
            None => c,
            Some(lit) => lit.add(&request.ray, None, request.depth, request.air, request.media, c)?,
        };
        Ok(c.map(f64::from))
    }

    /// The list made for this render reseeded the one generator with the seed word and left
    /// it at its last draw; every draw of the render goes on from there. With no list made
    /// the generator is left as the plan has it (point 3). (Effects,
    /// ), point 3
    fn before_first_pixel(&mut self, _scene: &Scene, generator: &mut CRand) -> Result<(), OpenItem> {
        if let Some(end) = self.list_end {
            *generator = end;
        }
        Ok(())
    }
}

impl PlainSurface {
    /// Per document object, whether the angle law can be in force for it (any ray surface or
    /// shadow surface with Transparency above 0 and refraction above 1; a textured material
    /// by its A value, or conservatively when its Transparency can be above 0); `None` where
    /// no object can. Only these objects' "met" state shows in a picture.
    pub(super) fn law_capable(&self) -> Option<Vec<bool>> {
        let version = self.version;
        let no_law = |t: f64, eta: f32| AngleLaw::of(version, t, eta).is_none();
        let mut capable: Vec<bool> = self
            .materials
            .iter()
            .map(|m| match m {
                Ok(ObjectMaterial::Plain(p)) => {
                    !(no_law(p.rays.law_transparency, p.rays.refraction) && no_law(p.rays.transparency, p.rays.refraction))
                }
                Ok(ObjectMaterial::Volume(_)) => false,
                Ok(ObjectMaterial::Textured(t)) => t.refraction() > 1.0 && (t.transparency_a() > 0.0 || t.may_let_through()),
                // Refused where it is shaded: a render that meets it fails, and a threaded one
                // that fails runs again serially (so the error is the serial one).
                Err(_) => false,
            })
            .collect();
        // The shadow surfaces are per geometry object; the met state is per document object.
        for (g, s) in self.shadow_surfaces.iter().enumerate() {
            if s.as_ref().map_or(false, |s| !no_law(f64::from(s.transparency), s.refraction)) {
                if let Some(c) = self.geometry.scene_object.get(g).and_then(|&o| capable.get_mut(o)) {
                    *c = true;
                } else {
                    return Some(vec![true; capable.len().max(1)]);
                }
            }
        }
        capable.iter().any(|&c| c).then_some(capable)
    }

    /// Whether some texture keeps a result between evaluations that a later one reads (
    /// a texture none of whose components produces anything holds the colour it was given).
    pub(super) fn holds_texture_state(&self) -> bool {
        self.materials.iter().any(|m| matches!(m, Ok(ObjectMaterial::Textured(t)) if t.holds_state()))
            || self.gels.iter().flatten().any(|g| g.holds_state())
    }

    /// Whether the document has textured materials or gels (their evaluations write the shared
    /// texture context).
    pub(super) fn has_textures(&self) -> bool {
        self.materials.iter().any(|m| matches!(m, Ok(ObjectMaterial::Textured(_)))) || self.gels.iter().any(Option::is_some)
    }

    /// The materials met so far (for the multithreaded render's hand-over of state).
    pub(super) fn met(&self) -> &MetMaterials {
        &self.met
    }
}

/// `plain_rules` with the sky already made (a copy of one made by `PlainSky::new` for this
/// plan, so that the star list is not made again per worker).
pub(super) fn plain_rules_from(scene: &Scene, plan: &RenderPlan, mut sky: PlainSky) -> Result<(PlainSurface, PlainSky), RenderError> {
    let surface = PlainSurface::new(scene, plan, MetMaterials::none(scene.objects.len()))?;
    sky.lit_air = surface.lit_air.clone();
    Ok((surface, sky))
}

/// The blended ray's limit (3.1: the incoming ray's depth; 5.0: the rays the path has gone
/// through before the surface).
const BLEND_LIMIT: u32 = 100;

/// Plan and render `scene` as `version` with these rules. spec: as `super::render`, with
pub fn render_lit(
    scene: &Scene,
    version: RenderAs,
    choices: &super::OpenChoices,
) -> Result<(Picture, RenderCounts), RenderError> {
    render_lit_streamed(scene, version, choices, &mut |_, _, _| {})
}

/// Whether shading `scene` as `version` draws from the generator (a light with a radius as 5.0,
/// a jittered volume's march), so that a render of it depends on where the generator stands
/// . (Shading that draws)
pub fn shading_draws(scene: &Scene, version: RenderAs, choices: &super::OpenChoices) -> Result<bool, RenderError> {
    let opened = super::plan::opened_with_haze_five(scene, version, choices.haze_five)?;
    let plan = RenderPlan::new(&opened, version, choices)?;
    Ok(PlainSurface::new(&opened, &plan, MetMaterials::none(opened.objects.len()))?.draws())
}

/// The frame render, with every finished pixel handed to `emit` (x, y, its three levels) as
/// it is done. With `choices.scattered` set (the add-on's live picture, task T2):
/// - When the anti-aliasing setting takes no draws of the generator (settings 0, 1 and 3,
///   and no lens), the frame's own pixels are taken in a fixed random permutation of the
///   region (the seed's shuffle): each pixel's value is its own, so the finished picture is
/// the same bytes as the normal order's.,
/// - When it draws (the jittered and Premium settings, whose places come from the generator
///   in the render's order), a quick one-ray-a-pixel preview runs scattered first, on a
///   plan, rules, met state and generator of its own — all discarded, so the frame meets
///   the state it would have met without the preview — and the frame then renders in
/// 's order: the finished picture is the exact render's, byte for byte.
/// `Some(_)` with a picture made another way ('s blocks, a render that goes on) keeps
/// the normal order; the pixels still reach `emit`.
/// (the live picture),
pub fn render_lit_streamed(
    scene: &Scene,
    version: RenderAs,
    choices: &super::OpenChoices,
    emit: stages::PixelObserver<'_>,
) -> Result<(Picture, RenderCounts), RenderError> {
    let before = crate::numerics::draws_made();
    let first = render_lit_once(scene, version, choices, emit);
    if std::env::var_os("STECYK3D_TRACE_DRAWS").is_some() {
        eprintln!("draws: {} ({})", crate::numerics::draws_made() - before, if first.is_ok() { "finished" } else { "gave up" });
    }
    match first {
        // The exact order of a frame that draws ran past its time: again, with a place of its own
        // in the generator's sequence for every pixel, on every core .
        Err(RenderError::Refused { entry: "slow-draws", .. }) => {
            let again = super::OpenChoices { shared_draws: super::SharedDraws::Independent, skip_scattered_preview: true, ..choices.clone() };
            render_lit_once(scene, version, &again, emit)
        }
        other => other,
    }
}

fn render_lit_once(
    scene: &Scene,
    version: RenderAs,
    choices: &super::OpenChoices,
    emit: stages::PixelObserver<'_>,
) -> Result<(Picture, RenderCounts), RenderError> {
    let as_read = scene;
    let opened = super::plan::opened_with_haze_five(scene, version, choices.haze_five)?;
    let scene: &Scene = &opened;
    let mut plan = RenderPlan::new(scene, version, choices)?;
    // , (When it
    // renders: before a frame asked for; the render on opening comes before it, so that
    // render finds none met)
    let met = MetMaterials::none(scene.objects.len());
    if !choices.no_preview && !choices.go_on {
        let preview = super::preview::preview_plan(as_read, &plan)?;
        let mut surface = PlainSurface::new(scene, &preview, met.clone())?;
        // , (its draws come from the
        // generator as the render on opening left it; the frame goes on from the last)
        surface.depth_limit = super::preview::PREVIEW_DEPTH_LIMIT;
        surface.soft_grid = super::preview::PREVIEW_SOFT_GRID;
        let mut sky = PlainSky::new(scene, &preview)?;
        sky.lit_air = surface.lit_air.clone();
        let mut generator = plan.generator;
        super::preview::render_preview(scene, &preview, &mut surface, &mut sky, &mut generator)?;
        plan.generator = generator;
    }
    let mut surface = PlainSurface::new(scene, &plan, met)?;
    let mut sky = PlainSky::new(scene, &plan)?;
    sky.lit_air = surface.lit_air.clone();
    if let super::SharedDraws::Auto(budget) = choices.shared_draws {
        if surface.draws() {
            plan.give_up_at = Some(crate::numerics::draws_made().saturating_add(budget));
        }
    }
    // The multithreaded render : `None` where it does not apply or
    // found order-dependent state; the serial render below then runs untouched.
    let centre_places = matches!(plan.placer.setting, AaSetting::Centre | AaSetting::Corner | AaSetting::CentreAgain)
        && plan.lens.is_none();
    // The angle law  follows which objects were met first, so a custom order can end in another picture. A
    // scene where it can act gets the plain order ('s blocks, coarse first): the exact frame at once, rather
    // than a throwaway preview in the custom order and the frame after it, which cost the picture twice.
    let scattered = choices
        .scattered
        .filter(|_| plan.way == PictureWay::Passes && plan.saved.is_none())
        .filter(|_| !(centre_places && surface.law_capable().is_some()));
    let Some(seed) = scattered else {
        if let Some(done) = super::parallel::try_render(scene, version, choices, &mut plan, &surface, None, emit) {
            return done;
        }
        return render_picture_progress(scene, &mut plan, &mut surface, &mut sky, None, emit);
    };
    let r = plan.region;
    let mut visits = if choices.scattered_rows {
        sampling::row_pixels(r.right - r.left, r.bottom - r.top)
    } else {
        sampling::scattered_pixels(r.right - r.left, r.bottom - r.top, seed)
    };
    for v in &mut visits {
        v.x += r.left;
        v.y += r.top;
    }
    // . Shading that draws (a light with a
    // radius, a jittered volume, the wash of a Bryce 5 sky) takes them in the order the pixels are visited, so such a
    // frame is never made pixel by pixel in the scattered order: it takes the preview-then-frame path below, whose
    // frame is's order and so the document's own picture (tests/blender_vs_bryce, sky/wash-pictures).
    let no_draws = centre_places && !surface.draws();
    if no_draws {
        // Each pixel's value is its own: the threads take chunks of the scattered order.
        if let Some(done) =
            super::parallel::try_render(scene, version, choices, &mut plan, &surface, Some(visits.clone()), emit)
        {
            return done;
        }
        return render_picture_progress(scene, &mut plan, &mut surface, &mut sky, Some(visits), emit);
    }
    // The quick scattered preview: one ray through each pixel's centre (no draws of the
    // generator, no lens, no pass over edges), on state of its own — its met, its lit
    // air and its copy of the generator end here with it; the frame below is untouched.
    if !choices.skip_scattered_preview {
        let mut pplan = plan.clone();
        pplan.placer = SamplePlacer::new(AaSetting::Centre, version, 0);
        pplan.edge_pass = None;
        pplan.lens = None;
        let mut psurface = PlainSurface::new(scene, &pplan, MetMaterials::none(scene.objects.len()))?;
        psurface.depth_limit = super::preview::PREVIEW_DEPTH_LIMIT;
        let mut psky = PlainSky::new(scene, &pplan)?;
        psky.lit_air = psurface.lit_air.clone();
        // The preview's pixels are independent too (no draws), so the threads share it as well. Where the shading
        // draws (the wash) the preview's own draws would run serially in the exact order and, being only a thing to
        // look at (it is overwritten by the frame, which starts from its own generator), burn the whole draw budget
        // of the Auto grain before the frame began (wash-vol: 100M draws, 2 minutes, in F12 only); so unless the
        // caller asked for the exact grain it takes a place of its own in the sequence for every pixel, on all cores.
        let preview_choices;
        let pchoices = if psurface.draws() && !matches!(choices.shared_draws, super::SharedDraws::Exact) {
            pplan.give_up_at = None;
            pplan.independent_draws = true;
            preview_choices = super::OpenChoices { shared_draws: super::SharedDraws::Independent, ..choices.clone() };
            &preview_choices
        } else {
            choices
        };
        match super::parallel::try_render(scene, version, pchoices, &mut pplan, &psurface, Some(visits.clone()), emit) {
            Some(done) => {
                done?;
            }
            None => {
                render_picture_progress(scene, &mut pplan, &mut psurface, &mut psky, Some(visits), emit)?;
            }
        }
        if std::env::var_os("STECYK3D_TRACE_DRAWS").is_some() {
            eprintln!("draws after the scattered preview: {}", crate::numerics::draws_made());
        }
    }
    // The exact frame, in's order. Its places draw, so it cannot go in rows without changing the picture;
    // for the row order (the old recomp's live render) the row-order preview above is what is watched, and the
    // frame, whose blocks would show as a mosaic, comes whole in the levels file at the end.
    let mut hidden = |_: u32, _: u32, _: &[u8]| {};
    let emit: &mut dyn FnMut(u32, u32, &[u8]) = if choices.scattered_rows { &mut hidden } else { emit };
    if let Some(done) = super::parallel::try_render(scene, version, choices, &mut plan, &surface, None, emit) {
        return done;
    }
    render_picture_progress(scene, &mut plan, &mut surface, &mut sky, None, emit)
}

/// The render a program makes on opening a document (no quick preview before it), with the
/// generator as it left it, for a caller that starts frames from there (`OpenChoices::generator`
/// of a `FrameSeries`).,
pub fn render_lit_ending(
    scene: &Scene,
    version: RenderAs,
    choices: &super::OpenChoices,
) -> Result<(Picture, RenderCounts, CRand), RenderError> {
    let opened = super::plan::opened_with_haze_five(scene, version, choices.haze_five)?;
    let met = MetMaterials::none(opened.objects.len());
    render_lit_frame(&opened, None, version, choices, &met)
}

/// One frame of a series (`super::frames`): `frame` and `preview` are documents already
/// opened (the frame at its time; the quick preview's document as it opens at its saved
/// time, `None` where no preview renders before this frame); `met` is the series' state of
/// the materials met, shared with every earlier frame; the frame's generator is
/// `choices.generator`. Returns the picture, its counts and the generator as the frame left
/// it..
/// (Frames)
pub(crate) fn render_lit_frame(
    frame: &Scene,
    preview: Option<(&Scene, &Scene)>,
    version: RenderAs,
    choices: &super::OpenChoices,
    met: &MetMaterials,
) -> Result<(Picture, RenderCounts, crate::numerics::CRand), RenderError> {
    let mut plan = RenderPlan::new(frame, version, choices)?;
    if let Some((ps, as_read)) = preview {
        let base = RenderPlan::new(ps, version, choices)?;
        let pplan = super::preview::preview_plan(as_read, &base)?;
        let mut surface = PlainSurface::new(ps, &pplan, met.clone())?;
        surface.depth_limit = super::preview::PREVIEW_DEPTH_LIMIT;
        surface.soft_grid = super::preview::PREVIEW_SOFT_GRID;
        let mut sky = PlainSky::new(ps, &pplan)?;
        sky.lit_air = surface.lit_air.clone();
        let mut generator = plan.generator;
        super::preview::render_preview(ps, &pplan, &mut surface, &mut sky, &mut generator)?;
        plan.generator = generator;
    }
    let mut surface = PlainSurface::new(frame, &plan, met.clone())?;
    let mut sky = PlainSky::new(frame, &plan)?;
    sky.lit_air = surface.lit_air.clone();
    let (picture, counts) = super::stages::render_picture(frame, &mut plan, &mut surface, &mut sky)?;
    let generator = plan.generator_after(&counts)?;
    Ok((picture, counts, generator))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// : met is kept for each object; a made material (the
    /// set's stand-in) is never waiting and makes nothing met; no material meets nothing.
    #[test]
    fn met_is_kept_for_each_object() {
        let met = MetMaterials::none(2);
        assert!(!met.meet(Some((0, false))), "first surface of object 0: no law");
        assert!(!met.meet(Some((1, false))), "object 1 names the same material but is met apart");
        assert!(met.meet(Some((0, false))));
        assert!(met.meet(Some((1, false))));
        assert!(met.meet(Some((5, true))), "a set's made material: the law at every hit");
        assert!(!met.meet(Some((5, false))), "and it made nothing met");
        assert!(!met.meet(None));
    }

    /// Effects : the frame's sky makes the list, even where the
    /// field does not show, and its end is where the render's draws start; the quick
    /// preview's sky makes none and lays nothing; a later render of the document held lays
    /// the list but makes none.,
    #[test]
    fn which_sky_makes_the_star_list() {
        // Needs two reference documents; STECYK3D_REFERENCE_VECTORS (a folder holding sky/star-jitter-pictures/) unset: nothing to check.
        let Some(root) = std::env::var_os("STECYK3D_REFERENCE_VECTORS") else { return };
        let dir = format!("{}/sky/star-jitter-pictures/documents", root.to_string_lossy());
        for (doc, shows) in [("jit-01-stars-jittered", true), ("jit-02-field-not-shown", false)] {
            let Ok(bytes) = std::fs::read(format!("{dir}/{doc}.br3")) else { return };
            let scene = Scene::read(&bytes, crate::scene_file::Flavour::Bryce3d).unwrap();
            let v = RenderAs::Bryce31;
            let s = &scene.sky;
            let end = StarList::seeded(v, s.star_seed as u32, s.star_count, s.comet_count).generator_after;
            let mut plan = RenderPlan::new(&scene, v, &super::super::OpenChoices::default()).unwrap();
            let mut frame = PlainSky::new(&scene, &plan).unwrap();
            assert_eq!(frame.list_end, Some(end), "{doc}");
            assert_eq!(frame.stars.is_some(), shows, "{doc}");
            let mut g = plan.generator;
            assert_eq!(g, CRand::new(1));
            frame.before_first_pixel(&scene, &mut g).unwrap();
            assert_eq!(g, end, "{doc}: the draws start at the list's end");

            let preview = super::super::preview::preview_plan(&scene, &plan).unwrap();
            let p = PlainSky::new(&scene, &preview).unwrap();
            assert!(p.list_end.is_none() && p.stars.is_none(), "{doc}: the preview has no field");

            plan.star_list = StarListUse::Held;
            let held = PlainSky::new(&scene, &plan).unwrap();
            assert!(held.list_end.is_none(), "{doc}: a held list is not made again");
            assert_eq!(held.stars.is_some(), shows, "{doc}");
        }
    }

    /// the slab and the pyramid enclose a volume, the square and the disc
    /// do not, as the plane does not.
    #[test]
    fn which_shapes_enclose_a_volume() {
        let torus = Shape::Torus { tube: 0.25, ring: 1.0 };
        for s in [Shape::Sphere, Shape::Cube, Shape::Cylinder, Shape::Cone, torus, Shape::Slab, Shape::Pyramid] {
            assert!(encloses_volume(&s), "{s:?}");
        }
        for s in [Shape::Plane, Shape::Square, Shape::Disc] {
            assert!(!encloses_volume(&s), "{s:?}");
        }
    }

    /// : a shadow surface of a solid carries its medium (a blended
    /// material's volume colour counting as white); a shape that encloses no volume has none.
    #[test]
    fn shadow_surfaces_and_their_media() {
        let rgb = |c: [f32; 3]| crate::scene_parts::Channel { refs: [None; 3], flags: 0, a: c, b: c, alpha: 1.0 };
        let one = |v: f32| crate::scene_parts::Channel { refs: [None; 3], flags: 0, a: v, b: v, alpha: 1.0 };
        let part = |flags: u32, volume: [f32; 3]| MaterialPart {
            flags,
            colours: [rgb([1.0; 3]), rgb([0.0; 3]), rgb([0.0; 3]), rgb([0.0; 3]), rgb([0.5; 3])],
            values: [one(1.0), one(0.0), one(0.0), one(0.5), one(0.0), one(0.0), one(0.0), one(0.0)],
            volume_colour: volume,
            refraction: 1.5,
            volume_numbers: [0.0; 3],
            tracks: [0; 19],
        };
        let tinted = [0.6, 0.3, 0.9];
        let s = |p: &MaterialPart, sh: Shape| shadow_surface(Some(p), &sh, true, encloses_volume(&sh), 0x3bf, RenderAs::Bryce31).unwrap();
        let sphere = s(&part(MATERIAL_CURRENT, tinted), Shape::Sphere);
        assert_eq!(sphere.medium, Some(MediumLaw { volume_colour: tinted, refraction: 1.5, fades: true }));
        assert!(!sphere.blended);
        let blended = s(&part(MATERIAL_CURRENT | MATERIAL_BLENDED, tinted), Shape::Cube);
        assert_eq!(blended.medium, Some(MediumLaw { volume_colour: [1.0; 3], refraction: 1.5, fades: true }));
        assert!(blended.blended);
        assert_eq!(s(&part(MATERIAL_CURRENT, tinted), Shape::Disc).medium, None, "no volume");
        // : a fuzzy solid is a medium with its own Φ (fuzzy-pictures 03); the blended bit
        // beside it changes nothing; shapes other than the sphere, the cone and the plane, and
        // the volume materials , are refused.
        let fuzzy = s(&part(MATERIAL_CURRENT | MATERIAL_FUZZY | MATERIAL_BLENDED, tinted), Shape::Sphere);
        assert_eq!(fuzzy.fuzzy, Some(FuzzyShape::Solid));
        assert!(!fuzzy.blended);
        // Φ and η its own (Φ white with the blended bit), F = 1.
        assert_eq!(fuzzy.medium, Some(MediumLaw { volume_colour: [1.0; 3], refraction: 1.5, fades: false }));
        let fuzzy_plain = s(&part(MATERIAL_CURRENT | MATERIAL_FUZZY, tinted), Shape::Sphere);
        assert_eq!(fuzzy_plain.medium, Some(MediumLaw { volume_colour: tinted, refraction: 1.5, fades: false }));
        assert_eq!(s(&part(MATERIAL_CURRENT | MATERIAL_FUZZY, tinted), Shape::Plane).fuzzy, Some(FuzzyShape::Plane));
        let refused = |flags: u32, sh: Shape| shadow_surface(Some(&part(flags, tinted)), &sh, true, encloses_volume(&sh), 0x3bf, RenderAs::Bryce31).err();
        assert_eq!(refused(MATERIAL_CURRENT | MATERIAL_FUZZY, Shape::Mesh(0)), Some(open::FUZZY_SHAPE));
        // : a volume material's surface takes no share; its solid's medium has
        // F = 1 (η counted as 1); bit 0x2 beside a volume bit changes nothing; the cube's depth
        // inside is 1 ; two kinds are marched in turn; the plane has no inside.
        let vol = s(&part(MATERIAL_CURRENT | MATERIAL_FUZZY | 0x100, tinted), Shape::Sphere);
        assert_eq!(vol.volume, Some(MATERIAL_CURRENT | MATERIAL_FUZZY | 0x100));
        assert_eq!(vol.medium, Some(MediumLaw { volume_colour: tinted, refraction: 1.5, fades: false }));
        assert_eq!(refused(MATERIAL_CURRENT | 0x200, Shape::Plane), Some(crate::shading::volume::open::SHAPE));
        assert_eq!(refused(MATERIAL_CURRENT | 0x200, Shape::Cube), None);
        assert_eq!(refused(MATERIAL_CURRENT | 0x300, Shape::Sphere), None);
        // Bits 0x4000 and 0x8000 of a plain material: the medium colour .
        assert!(s(&part(MATERIAL_CURRENT | 0xC000, tinted), Shape::Slab).tint.is_some());
        assert_eq!(refused(MATERIAL_CURRENT | MATERIAL_FUZZY | 0x4000, Shape::Sphere), Some(open::FUZZY_TINT));
        // : a shadow ray reads the Light
        // mode's replaced word (bit 0x80: it passes) and the media the material's own word (Φ
        // white only with bit 0x8, in every version); bit 0x2 beside bit 0x4 plays no part.
        for (v, word) in [(RenderAs::Bryce30, 0x00a0), (RenderAs::Bryce31, 0x20a8), (RenderAs::Bryce50, 0x2020a8)] {
            let lm = |flags: u32| shadow_surface(Some(&part(flags, tinted)), &Shape::Sphere, true, true, 0x3bf, v).unwrap();
            let plain = lm(MATERIAL_CURRENT | 0x4);
            assert_eq!((plain.word, plain.own_word), (word, MATERIAL_CURRENT | 0x4), "{v:?}");
            assert!(plain.word & SHADOW_PASS != 0);
            assert_eq!(plain.medium, Some(MediumLaw::of_solid(tinted, 1.0, false)), "{v:?}");
            let white = lm(MATERIAL_CURRENT | 0x4 | MATERIAL_BLENDED);
            assert_eq!(white.medium, Some(MediumLaw::of_solid(tinted, 1.0, true)), "{v:?}");
            let fuzzy = lm(MATERIAL_CURRENT | 0x4 | MATERIAL_FUZZY);
            assert_eq!((fuzzy.fuzzy, fuzzy.word), (None, word), "{v:?}");
        }
        // The bits of are stated: a plain material with them is read.
        let bits = s(&part(MATERIAL_CURRENT | SHADOW_PASS | ADDITIVE | NO_SELF_SHADOW, tinted), Shape::Sphere);
        assert_eq!(bits.word, MATERIAL_CURRENT | SHADOW_PASS | ADDITIVE | NO_SELF_SHADOW);
    }

    /// : the colour along a shadow ray is multiplied from the
    /// light's end back, ⟨c1·⟨c2·⟨c3·Φ⟩⟩⟩, each a binary32 product.
    #[test]
    fn shadow_colour_from_the_light_back() {
        let c = [[0.3f32, 0.7, 0.9], [0.11, 0.13, 0.17], [0.61, 0.67, 0.71], [0.19, 0.23, 0.29]];
        let mut a = Amount::new();
        for k in c {
            a.colour(k);
        }
        let s = a.result(RenderAs::Bryce31);
        let want = tint(c[0], tint(c[1], tint(c[2], c[3])));
        assert_eq!(s.colour, want);
        assert_eq!(Amount::new().result(RenderAs::Bryce30).colour, [1.0; 3]);
    }
}
