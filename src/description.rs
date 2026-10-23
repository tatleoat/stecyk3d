// SPDX-License-Identifier: MIT
//! The Blender description (component `blender-addon`) to a renderable scene.
//!
//! (the protocol with the render program; the
//! description's field names are the add-on's `convert.py`), (what the description
//! holds), and the crate's own scene parts as `scene.rs` reads them from documents.
//! BS-1 (4.4, 4.5: the camera's `frame_angle`, `pan` and
//! `pixel_aspect`, which the add-on computes from Blender's fit rules and shifts)
//!
//! The description names no resource identities and holds no byte layout: this module
//! builds the same `Scene` a document would give, as the implementer's arrangement (
//! leaves the description's shape to the add-on and its reader). Deviations, all recorded
//! for the owner:
//! - as 5.0 the render's `ray_depth` and `tir_depth` are the document's own depth limit and limit of total
//! internal reflections (revised, offsets 180 and 184), carried in Bryce 5's part; left out they
//! are 6 and 2. As 3.0 and 3.1 the limits are fixed ('s `DEPTH_LIMIT`), so the fields are ignored.
//! - the structure code's word is carried and walked as says; a code other than
//! `bspT` and `grid` (Open, Errors) takes the tree
//!   (`OpenChoices::ignore_structure_code`);
//! - trees (grown with the substrate's tables, `Tables`), terrains ('s own part) and groups (members
//! linked by name,'s role bits) are read (roadmap item 4); a metaball is read as's object (its own transform
//! from its matrix, its sign), and drawn as 5.0 ;
//! - lights : radial, `cone` (the spot beam record) and `box` (the
//! parallel record), `pyramid` and `cylinder` are read with their own transform, and a gradient
//!   with them; a gel is refused by name (its own material's texture is not described);
//! - a texture is procedural or (`kind` picture) a picture texture with its pixels ;
//! channels carry their texture references and switch words ;
//! the texture's settings are's texture form (`texture` and `mapping` of each entry of `textures`);
//! - a Bryce 5 sky part is read from the sky's fields  when the three haze and
//!   fog numbers are given; the lit air from `lit_air`;
//! - a mesh is a triangle soup with both sides meeting the ray, holds no volume, and its
//! materials are plain ; see `geometry::Mesh`.

use std::collections::BTreeMap;

mod trees;

use crate::cases::json::Value;
use crate::geom::Vec3;
use crate::geometry::{Mesh, Triangle};
use crate::numerics::x87;
use crate::render::OpenChoices;
use crate::scene::{
    Body, Camera, ChannelTextures, CloudTextures, Light, Material, RadialLight, Scene, SceneObject, SecondChannel,
    SecondChannels, Shape, Sun,
    TerrainObject, TreeObject, View,
};
use crate::scene_height_fields::{Packing, TerrainPart};
use crate::scene_trees::StoredTree;
use crate::trees::LeafMesh;
use crate::scene_parts::{Channel, LightPart, MaterialPart, Rgb, Sky, SunPart, TextureRef, MATERIAL_CURRENT};
use crate::scene_beams::BeamRecord;
use crate::scene_records::{DocSettings, ObjectKind, PlacementPart, Progress, Region, Transform};
use crate::scene_textures::{
    Mapping, PictureTexture, ProceduralTexture, RecordForm, SavedResult, Texture, TextureBody, TextureClass,
    PICTURE_CODE, PROCEDURAL_CODE,
};
use crate::scene_pictures::HeldPicture;
use crate::scene_records::FourCc;
use crate::textures::picture::{Picture, Pixel};
use crate::scene_view::{ViewTransform, ViewTransforms};
use crate::textures::settings::{
    ColourMap, ColourSpace, Component, Dimension, Filter, MAX_COMPONENTS, NoiseSettings, NoiseSpace, OutputSet, Phase,
    TextureSettings,
};
use crate::version::RenderAs;

/// Why a description could not be read. The description format is the add-on's and this
/// reader's , so the messages are our own (refused with a message).
#[derive(Clone, Debug, PartialEq)]
pub struct DescriptionError(pub String);

impl std::fmt::Display for DescriptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for DescriptionError {}

type Rd<T> = Result<T, DescriptionError>;

fn err<T>(msg: impl Into<String>) -> Rd<T> {
    Err(DescriptionError(msg.into()))
}

/// A description read, ready to render: the scene, the version to render it as, and the
/// choices the description decides that `OpenChoices` carries.
pub struct Described {
    pub scene: Scene,
    pub version: RenderAs,
    /// The frame is asked for with the quick preview off (`render.preview` false).
    pub no_preview: bool,
    /// The render as 5.0 ignores the structure code (Open): a description render is
    /// never a reproduction of a stored picture.
    pub ignore_structure_code: bool,
}

impl Described {
    /// The choices for a render of this description, over the substrate's tables.
    pub fn choices(&self, base: OpenChoices) -> OpenChoices {
        OpenChoices { no_preview: self.no_preview, ignore_structure_code: self.ignore_structure_code, ..base }
    }
}

// ---------------------------------------------------------------------------------------
// Reading values
// ---------------------------------------------------------------------------------------

fn f64_of(v: &Value) -> Rd<f64> {
    match v {
        Value::Number(t) => t.parse::<f64>().map_err(|_| DescriptionError(format!("{t} is not a number"))),
        Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        other => err(format!("expected a number, found {other:?}")),
    }
}

fn u32_of(v: &Value) -> Rd<u32> {
    let x = f64_of(v)?;
    if x >= 0.0 && x <= 4294967295.0 && x == x.trunc() {
        Ok(x as u32)
    } else {
        err(format!("{x} is not a 32-bit word"))
    }
}

fn vec3_of(v: &Value, what: &str) -> Rd<Vec3> {
    let a = v.as_array().ok_or_else(|| DescriptionError(format!("{what} is not a list")))?;
    if a.len() != 3 {
        return err(format!("{what} is not three numbers"));
    }
    let out = [f64_of(&a[0])?, f64_of(&a[1])?, f64_of(&a[2])?];
    if !out.iter().all(|c| c.is_finite()) {
        return err(format!("{what} has a coordinate that is not finite"));
    }
    Ok(out)
}

/// A colour of the description: three numbers to the binary32 the parts hold (the add-on
/// writes each as its binary32 widened, so the cast gives the stored bits back).
fn rgb_of(v: &Value, what: &str) -> Rd<Rgb> {
    let [r, g, b] = vec3_of(v, what)?;
    Ok([r as f32, g as f32, b as f32])
}

/// A sky colour: as `rgb_of`, but a number that is not finite is kept, as the document holds it (the original draws
/// a sky colour of NaN or infinity, and Limits states what comes out; tests/blender_vs_bryce np-03 to np-05).
fn sky_rgb_of(v: &Value, what: &str) -> Rd<Rgb> {
    let a = v.as_array().ok_or_else(|| DescriptionError(format!("{what} is not a list")))?;
    if a.len() != 3 {
        return err(format!("{what} is not three numbers"));
    }
    Ok([f64_of(&a[0])? as f32, f64_of(&a[1])? as f32, f64_of(&a[2])? as f32])
}

fn doubles_of(v: &Value, n: usize, what: &str) -> Rd<Vec<f64>> {
    let a = v.as_array().ok_or_else(|| DescriptionError(format!("{what} is not a list")))?;
    if a.len() != n {
        return err(format!("{what} is not {n} numbers"));
    }
    a.iter().map(f64_of).collect()
}

/// `v[key]` or the default if the member is absent or null.
fn member<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.get(key).filter(|m| !matches!(m, Value::Null))
}

fn f64_at(v: &Value, key: &str, default: f64) -> Rd<f64> {
    member(v, key).map(f64_of).transpose().map(|o| o.unwrap_or(default))
}

fn u32_at(v: &Value, key: &str, default: u32) -> Rd<u32> {
    member(v, key).map(u32_of).transpose().map(|o| o.unwrap_or(default))
}

// ---------------------------------------------------------------------------------------
// The transform: Blender's axes and lengths to Bryce's
// ---------------------------------------------------------------------------------------

/// The description's world is (units · x, units · z, units · y) of Blender's: rows are
/// `units · C·Mᵀ·C` with M the object's Blender world matrix as rows, and the translation
/// `units · t` with y and z swapped ('s change of axes, applied to a 4x4 matrix).
/// (axes and units)
fn bryce_transform(matrix: &Value, units: f64, what: &str) -> Rd<Transform> {
    let rows = matrix.as_array().ok_or_else(|| DescriptionError(format!("{what} is not a matrix")))?;
    if rows.len() != 4 {
        return err(format!("{what} is not four rows"));
    }
    let m: Vec<Vec<f64>> = rows
        .iter()
        .map(|r| {
            let a = r.as_array().ok_or_else(|| DescriptionError(format!("{what} has a row that is not a list")))?;
            if a.len() != 4 {
                return err(format!("{what} has a row that is not four numbers"));
            }
            a.iter().map(f64_of).collect::<Rd<Vec<f64>>>()
        })
        .collect::<Rd<_>>()?;
    let swap = |i: usize| if i == 1 { 2 } else if i == 2 { 1 } else { i };
    // rows_b[i][j] = units · (C·Mᵀ·C)[i][j] = units · Mᵀ[swap(i)][swap(j)] = units · M[swap(j)][swap(i)]
    let mut out_rows = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out_rows[i][j] = units * m[swap(j)][swap(i)];
        }
    }
    let t = &m[3];
    let translation = [units * t[0], units * t[2], units * t[1]];
    if !out_rows.iter().flatten().chain(&translation).all(|c| c.is_finite()) {
        return err(format!("{what} has a number that is not finite"));
    }
    Ok(Transform { rows: out_rows, translation })
}

const IDENTITY_ROWS: [Vec3; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn identity_placement() -> PlacementPart {
    PlacementPart {
        version: 1,
        third_flags: 0,
        tracks: [0; 7],
        saved_distance: 0.0,
        first_flags: 0,
        second_flags: 0,
        material: None,
        first: Transform { rows: IDENTITY_ROWS, translation: [0.0; 3] },
        second: Transform { rows: IDENTITY_ROWS, translation: [0.0; 3] },
    }
}

/// A placement with the entry's `placement_flags` (the three flag words of a document's placement, as an import
/// keeps them: hidden, mesh-solid and the like), the second word as `second` where given.
fn placement_with_flags(entry: &Value, second: Option<u32>) -> Rd<PlacementPart> {
    let mut p = identity_placement();
    if let Some(list) = member(entry, "placement_flags") {
        let w = doubles_of(list, 3, "placement_flags")?;
        for x in &w {
            if *x < 0.0 || x.fract() != 0.0 || *x > f64::from(u32::MAX) {
                return err(format!("placement_flags holds {x}, not a word"));
            }
        }
        p.first_flags = w[0] as u32;
        p.second_flags = w[1] as u32;
        p.third_flags = w[2] as u32;
    }
    if let Some(sw) = second {
        p.second_flags |= sw;
    }
    Ok(p)
}

// ---------------------------------------------------------------------------------------
// The parts
// ---------------------------------------------------------------------------------------

fn sky_of(v: &Value, version: RenderAs) -> Rd<Sky> {
    let array = |v: &Value, key: &str, n: usize| -> Rd<Option<Vec<f64>>> {
        member(v, key).map(|m| doubles_of(m, n, key)).transpose()
    };
    let colours = |v: &Value, key: &str| -> Rd<Rgb> {
        member(v, key).map(|m| sky_rgb_of(m, key)).transpose().map(|o| o.unwrap_or([0.0; 3]))
    };
    let flags = u32_at(v, "flags", 0)? & !0x400; // the reader sets 0x400 from a named view; there is none
    let moon = array(v, "moon", 4)?.unwrap_or(vec![0.0; 4]);
    let discs = array(v, "discs", 15)?.unwrap_or(vec![0.0; 15]);
    let star_field = match member(v, "stars") {
        None => [0; 3],
        Some(s) => {
            let a = s.as_array().ok_or_else(|| DescriptionError("stars is not a list".to_owned()))?;
            if a.len() != 3 {
                return err("stars is not three words");
            }
            [u32_of(&a[0])?, u32_of(&a[1])?, u32_of(&a[2])?]
        }
    };
    let cloud_offset = array(v, "cloud_offset", 2)?.unwrap_or(vec![0.0, 0.0]);
    // : a document that holds a Bryce 5 part holds the sky part in version 7's form,
    // which is what a render as 5.0 reads it by; without one the part is Bryce 3D's (version 1).
    let five = sky_five_of(v)?;
    Ok(Sky {
        // A render as 5.0 reads a Bryce 3D part of version 2 by its takeover (the switch bits, the star
        // field, the moon and the discs) and a version 1 part not at all (05b: no document holds one),
        // so a sky without the Bryce 5 part is version 2 there; 3.0 and 3.1 read the two alike.
        version: if five.is_some() {
            crate::scene_sky::SKY_VERSION_SEVEN
        } else if version == RenderAs::Bryce50 {
            crate::scene_sky::SKY_VERSION_TWO
        } else {
            1
        },
        flags,
        cloud_height: f64_at(v, "cloud_height", 0.0)?,
        lower_cloud_colour: colours(v, "cloud_colour")?,
        upper_cloud_colour: colours(v, "upper_cloud_colour")?,
        cloud_offset: [cloud_offset[0], cloud_offset[1]],
        cloud_cover: f64_at(v, "cover", 0.0)?,
        cloud_amplitude: f64_at(v, "amplitude", 0.0)?,
        cloud_frequency: f64_at(v, "frequency", 0.0)?,
        spare_80: [0.0; 3],
        sky_colour: colours(v, "sky_colour")?,
        horizon_colour: colours(v, "horizon_colour")?,
        dome_colour: colours(v, "dome_colour")?,
        plain_sky_colour: colours(v, "plain_colour")?,
        shadow_intensity: f64_at(v, "shadows", 0.0)?,
        ambient_colour: colours(v, "ambient_colour")?,
        ambience: f64_at(v, "ambience", 0.0)?,
        haze_colour: colours(v, "haze_colour")?,
        haze: f64_at(v, "haze", 0.0)?,
        fog_colour: colours(v, "fog_colour")?,
        fog_height: f64_at(v, "fog_height", 0.0)?,
        fog: f64_at(v, "fog", 0.0)?,
        cloud_textures: [None, None],
        tracks: vec![0; 43],
        moon: [moon[0], moon[1], moon[2], moon[3]],
        discs: discs.try_into().map_err(|_| DescriptionError("discs is not fifteen numbers".to_owned()))?,
        lit_air: f64_at(v, "lit_air", 0.0)?,
        wind: [0.0; 3],
        wind_edge: None,
        star_seed: star_field[0] as i32,
        star_count: star_field[1],
        comet_count: star_field[2],
        five,
        as_bryce30: None,
        bryce30_unopened: false,
    })
}

/// 's Bryce 5 sky part from the sky's fields : present when `bryce5_part` is true. The three
/// haze and fog numbers are carried only when the document gave them ; without them the part holds
/// NaN for each and the render takes the substrate's `haze-five` (`render::plan::with_haze_five`), a
/// render without that table being refused rather than invented. The clip distances are infinity (Counts).
fn sky_five_of(v: &Value) -> Rd<Option<crate::scene_parts::SkyFive>> {
    if !matches!(member(v, "bryce5_part"), Some(Value::Bool(true))) {
        return Ok(None);
    }
    let triple = |key: &str| -> Rd<Vec3> {
        let d = doubles_of(member(v, key).ok_or_else(|| DescriptionError(format!("a Bryce 5 sky has no {key}")))?, 3, key)?;
        Ok([d[0], d[1], d[2]])
    };
    Ok(Some(crate::scene_parts::SkyFive {
        haze_shift_colour: { let t = triple("haze_shift_colour")?; [t[0] as f32, t[1] as f32, t[2] as f32] },
        haze_offset: f64_at(v, "haze_offset", f64::NAN)?,
        haze_clip: f64::INFINITY,
        haze_thickness: f64_at(v, "haze_thickness", f64::NAN)?,
        fog_offset: f64_at(v, "fog_offset", f64::NAN)?,
        fog_clip: f64::INFINITY,
        air_steps: Some(u32_at(v, "air_steps", 500)?),
        star_intensity: f64_at(v, "star_intensity", 1.0)?,
        comet_intensity: f64_at(v, "comet_intensity", 1.0)?,
        cloud_detail_z: f64_at(v, "cloud_detail_z", 0.5)?,
        cloud_detail: f64_at(v, "cloud_detail", 0.5)?,
        second_flags: u32_at(v, "second_flags", 0)?,
        star_orientation: triple("star_orientation")?,
        sun_glow_weight: f64_at(v, "sun_glow_weight", 1.0)?,
        moon_glow_weight: f64_at(v, "moon_glow_weight", 1.0)?,
        sun_disc_weight: f64_at(v, "sun_disc_weight", 1.0)?,
        moon_disc_weight: f64_at(v, "moon_disc_weight", 1.0)?,
    }))
}

fn light_part(colour: Rgb, intensity: f64) -> LightPart {
    // A Bryce 3D light (version 1): Bryce 5's numbers as reads such a light, all 0.
    let five = crate::scene_parts::LightFive {
        radius: Some(0.0),
        shadow_ambience: 0.0,
        range: 0.0,
        gradient: [0.0; 2],
        shadow_colour: [0.0; 3],
        gradient_resource: None,
    };
    LightPart { version: 1, colour, intensity, open_24: [0.0; 2], tracks: [0; 8], five }
}

/// The twelve numbers of's form (three rows, then the translation), as the add-on
/// writes a beam light's own transform .
fn twelve_of(v: &Value, what: &str) -> Rd<Transform> {
    let d = doubles_of(v, 12, what)?;
    if !d.iter().all(|c| c.is_finite()) {
        return err(format!("{what} has a number that is not finite"));
    }
    Ok(Transform {
        rows: [[d[0], d[1], d[2]], [d[3], d[4], d[5]], [d[6], d[7], d[8]]],
        translation: [d[9], d[10], d[11]],
    })
}

/// One light of the description: a radial light, or one with a beam (the
/// add-on's `cone`, `pyramid`, `cylinder` and `box`). The cone and the cylinder are's
/// spot and cylindrical records (one exponent), the box and the pyramid its parallel and square
/// records (two exponents, in the order given). Bryce 5's numbers, when the add-on gives them,
/// make the light part's version 3 . A gradient (`gradient_keys`) is's resource,
/// read with the light's range and offset (`gradient`); the third item returned is the Diffuse A
/// of its own material (`tint_colour`) where there is one, which the caller makes a material of
/// (reads D from the light's object's material).
/// (The reader takes trees, terrains, ..., gradients and pictures)
fn light_of(entry: &Value, object: usize, index: usize) -> Rd<(SceneObject, Light, Option<Rgb>)> {
    let kind = entry.get("kind").and_then(Value::as_str).unwrap_or("");
    let name = entry.get("name").and_then(Value::as_str).unwrap_or("light").to_owned();
    let exponents = |n: usize| -> Rd<Vec<f64>> {
        let e = member(entry, "exponents")
            .ok_or_else(|| DescriptionError(format!("light {name:?} has no exponents")))?;
        doubles_of(e, n, &format!("light {name:?}'s exponents"))
    };
    let (object_kind, beam) = match kind {
        "radial" => (ObjectKind::SphereLight, None),
        "cone" => (ObjectKind::ConeLight, Some(BeamRecord::Spot { exponent: exponents(1)?[0], falloff_track: 0 })),
        "box" => {
            let e = exponents(2)?;
            (ObjectKind::CubeLight, Some(BeamRecord::Parallel { exponents: [e[0] as f32, e[1] as f32] }))
        }
        // : the cylindrical parallel light in the spot light's layout,
        // the square spot light in the parallel light's
        "cylinder" => {
            (ObjectKind::CylinderLight, Some(BeamRecord::Cylinder { exponent: exponents(1)?[0], falloff_track: 0 }))
        }
        "pyramid" => {
            let e = exponents(2)?;
            (ObjectKind::PyramidLight, Some(BeamRecord::Square { exponents: [e[0] as f32, e[1] as f32] }))
        }
        other => {
            return err(format!(
                "light {name:?}: a light of kind {other:?} is not one of radial, cone, box, pyramid, cylinder"
            ));
        }
    };
    let colour = rgb_of(
        member(entry, "colour").ok_or_else(|| DescriptionError(format!("light {name:?} has no colour")))?,
        &format!("light {name:?}'s colour"),
    )?;
    let intensity = f64_at(entry, "intensity", 0.0)?;
    let switches = u32_at(entry, "switches", 0x100)?;
    let position = vec3_of(
        member(entry, "position").ok_or_else(|| DescriptionError(format!("light {name:?} has no position")))?,
        &format!("light {name:?}'s position"),
    )?;
    let world = match beam {
        // a radial light's object is a sphere the size of the entry's `world` (a document's light object),
        // else the unit sphere at its position
        None => match member(entry, "world") {
            Some(w) => twelve_of(w, &format!("light {name:?}'s world"))?,
            None => Transform { rows: IDENTITY_ROWS, translation: position },
        },
        Some(_) => twelve_of(
            member(entry, "world").ok_or_else(|| DescriptionError(format!("light {name:?} has no world")))?,
            &format!("light {name:?}'s world"),
        )?,
    };
    let mut part = light_part(colour, intensity);
    let gradient_keys = member(entry, "gradient_keys");
    let five_keys = ["radius", "shadow_ambience", "shadow_colour", "range"];
    if gradient_keys.is_some() || five_keys.iter().any(|k| member(entry, k).is_some()) {
        let radius = f64_at(entry, "radius", 0.0)?;
        let shadow_ambience = f64_at(entry, "shadow_ambience", 0.0)?;
        let shadow_colour = match member(entry, "shadow_colour") {
            None => [0.0; 3],
            Some(c) => rgb_of(c, &format!("light {name:?}'s shadow colour"))?,
        };
        part.version = crate::scene_parts::LIGHT_VERSION_FIVE;
        part.open_24 = [radius, shadow_ambience];
        part.five = crate::scene_parts::LightFive {
            radius: Some(radius),
            shadow_ambience,
            range: f64_at(entry, "range", 0.0)?,
            gradient: [0.0; 2],
            shadow_colour,
            gradient_resource: None,
        };
    }
    let mut own_colour = colour;
    let mut tint = None;
    let mut gradient = None;
    if let Some(keys) = gradient_keys {
        // : the
        // description holds no texture of the light's own material, so a gel is refused by name
        if matches!(member(entry, "gel"), Some(Value::Bool(true))) {
            return err(format!(
                "light {name:?}: a gradient on a gel (its own material has a texture on Diffuse) is not carried by the description"
            ));
        }
        let range_offset = doubles_of(
            member(entry, "gradient")
                .ok_or_else(|| DescriptionError(format!("light {name:?} has gradient_keys but no gradient")))?,
            2,
            &format!("light {name:?}'s gradient"),
        )?;
        part.five.gradient = [range_offset[0], range_offset[1]];
        gradient = Some(gradient_of(keys, &name)?);
        if let Some(c) = member(entry, "own_colour") {
            own_colour = rgb_of(c, &format!("light {name:?}'s own colour"))?;
        }
        if let Some(t) = member(entry, "tint_colour") {
            tint = Some(rgb_of(t, &format!("light {name:?}'s tint colour"))?);
        }
        part.colour = own_colour;
    }
    let scene_object = SceneObject {
        name: name.into_bytes(),
        kind: object_kind,
        identity: 0,
        linked_list: 0,
        linked_to: None,
        placement: Some(placement_with_flags(entry, Some(switches))?),
        world: Some(world),
        linked_worlds: None,
        world_open: None,
        listing_worlds: None,
        material: None,
        body: Body::RadialLight(index),
    };
    let light = Light::Radial(RadialLight {
        object,
        own_colour,
        colour,
        gel: None,
        intensity,
        position: world.translation,
        switches,
        part,
        beam,
        falloff_opened: None,
        gradient,
    });
    Ok((scene_object, light, tint))
}

/// 's resource from the description's `gradient_keys`: `colour_keys` (`colour`, `place`, `midpoint`),
/// `value_keys` (`value`, `place`, `midpoint`) and the two switch words, each list in the order given.
fn gradient_of(v: &Value, light: &str) -> Rd<crate::scene_parts::GradientPart> {
    use crate::scene_parts::{GradientColourKey, GradientPart, GradientValueKey};
    let list = |key: &str| -> Rd<Vec<Value>> {
        match member(v, key) {
            None => Ok(Vec::new()),
            Some(a) => Ok(a
                .as_array()
                .ok_or_else(|| DescriptionError(format!("light {light:?}'s gradient {key} is not a list")))?
                .to_vec()),
        }
    };
    let single = |k: &Value, key: &str| -> Rd<f32> {
        let m = member(k, key)
            .ok_or_else(|| DescriptionError(format!("a gradient key of light {light:?} has no {key}")))?;
        Ok(f64_of(m)? as f32)
    };
    let mut colour_keys = Vec::new();
    for k in list("colour_keys")? {
        let colour = rgb_of(
            member(&k, "colour")
                .ok_or_else(|| DescriptionError(format!("a gradient colour key of light {light:?} has no colour")))?,
            "a gradient key's colour",
        )?;
        colour_keys.push(GradientColourKey { colour, place: single(&k, "place")?, midpoint: single(&k, "midpoint")? });
    }
    let mut value_keys = Vec::new();
    for k in list("value_keys")? {
        value_keys.push(GradientValueKey {
            value: single(&k, "value")?,
            place: single(&k, "place")?,
            midpoint: single(&k, "midpoint")?,
        });
    }
    Ok(GradientPart {
        colour_switches: u32_at(v, "colour_switches", 0)?,
        value_switches: u32_at(v, "value_switches", 0)?,
        colour_keys,
        value_keys,
    })
}

fn sun_of(v: &Value) -> Rd<Light> {
    let colour = rgb_of(member(v, "colour").ok_or_else(|| DescriptionError("the sun has no colour".to_owned()))?, "the sun's colour")?;
    let glow = member(v, "glow_colour").map(|m| rgb_of(m, "the sun's glow colour")).transpose()?.unwrap_or(colour);
    let direction = vec3_of(
        member(v, "direction").ok_or_else(|| DescriptionError("the sun has no direction".to_owned()))?,
        "the sun's direction",
    )?;
    if direction.iter().all(|c| *c == 0.0) {
        return err("the sun's direction has length 0 (a sun lamp or the sky's sun direction is needed)");
    }
    let intensity = f64_at(v, "intensity", 0.0)?;
    let switches = u32_at(v, "switches", 0x100)?;
    // The sun lamp's own colour is the light part's , which a document keeps apart from the sun's colour; only
    // the wash of reads it. Without one the part has the sun's colour, as before.
    let light_colour = match member(v, "light_colour") {
        Some(m) => rgb_of(m, "the sun's light colour")?,
        None => colour,
    };
    Ok(Light::Sun(Sun {
        name: b"sun".to_vec(),
        colour,
        glow_colour: glow,
        direction,
        intensity,
        switches,
        light_colour,
        placement: identity_placement(),
        light: light_part(light_colour, intensity),
        sun: SunPart { colour, glow_colour: glow, direction, tracks: [0; 3] },
    }))
}


fn channel_c(a: Rgb) -> Channel<Rgb> {
    Channel { refs: [None; 3], flags: 0, a, b: a, alpha: 1.0 }
}

fn channel_v(a: f32) -> Channel<f32> {
    Channel { refs: [None; 3], flags: 0, a, b: a, alpha: 1.0 }
}

const COLOUR_CHANNELS: [&str; 5] =
    ["diffuse_color", "ambient_color", "specular_color", "specular_halo", "transparent_color"];
const VALUE_CHANNELS: [&str; 8] =
    ["diffusion", "ambience", "specularity", "transparency", "reflection", "bump", "metallicity", "volume_density"];

// ------------------------------------------------------------------ textures

fn whole_of(v: &Value, what: &str) -> Rd<i64> {
    let x = f64_of(v)?;
    if x != x.trunc() {
        return err(format!("{what} {x} is not a whole number"));
    }
    Ok(x as i64)
}

fn i64_at(v: &Value, key: &str, default: i64) -> Rd<i64> {
    match member(v, key) {
        None => Ok(default),
        Some(m) => whole_of(m, key),
    }
}

/// The outputs asked, as the letters of testing-interface.md ("c", "v", "b" and their sums).
fn outputs_of(v: &Value, what: &str) -> Rd<OutputSet> {
    if let Ok(x) = f64_of(v) {
        // 's word: bit 0x1 value, 0x2 colour, 0x4 bump
        let bits = whole_of_f(x, &format!("the outputs of {what}"))?;
        return Ok(OutputSet { value: bits & 1 != 0, colour: bits & 2 != 0, bump: bits & 4 != 0 });
    }
    let s = v.as_str().ok_or_else(|| DescriptionError(format!("the outputs of {what} are not a string of letters")))?;
    OutputSet::from_letters(s).ok_or_else(|| {
        DescriptionError(format!("the outputs of {what} are {s:?}, with a letter other than c, v and b"))
    })
}

fn whole_of_f(x: f64, what: &str) -> Rd<i64> {
    if x != x.trunc() || !(0.0..=4294967295.0).contains(&x) {
        return err(format!("{what} {x} is not a word"));
    }
    Ok(x as i64)
}

fn all_outputs() -> OutputSet {
    OutputSet { colour: true, value: true, bump: true }
}

/// A noise as's texture form names it: `type`, `method`, `octaves` and `dimension`.
fn noise_of(v: &Value) -> Rd<NoiseSettings> {
    Ok(NoiseSettings {
        type_number: i64_at(v, "type", 0)?,
        method: i64_at(v, "method", 0)?,
        octaves: i64_at(v, "octaves", 0)?,
        dimension: Dimension::from_number(i64_at(v, "dimension", 3)?),
    })
}

/// Two angles, then a frequency per axis ('s `angles` and `frequencies`). Nine numbers
/// (`map`,'s custom space) are an add-on extra that no document holds: refused.
fn space_of(v: &Value, what: &str) -> Rd<NoiseSpace> {
    if member(v, "map").is_some() {
        return err(format!("{what} holds a space of nine numbers (map), which the renderer does not take (BA-13a)"));
    }
    let angles = match member(v, "angles") {
        None => [0.0, 0.0],
        Some(r) => {
            let d = doubles_of(r, 2, &format!("{what}'s angles"))?;
            [d[0], d[1]]
        }
    };
    let frequencies = match member(v, "frequencies") {
        None => [1.0, 1.0, 1.0],
        Some(f) => {
            let d = doubles_of(f, 3, &format!("{what}'s frequencies"))?;
            [d[0], d[1], d[2]]
        }
    };
    Ok(NoiseSpace { angles, frequencies })
}

/// 's phase: the amount, the phase noise and its space, held as one object the way
/// 's nodes hold them (the noise's own settings named beside the amount).
fn phase_of(v: &Value) -> Rd<Phase> {
    let noise = match member(v, "noise") {
        None => noise_of(&Value::Null)?,
        Some(n) => noise_of(n)?,
    };
    Ok(Phase { amount: f64_at(v, "amount", 0.0)?, noise, space: space_of(v, "a phase")? })
}

/// No phase: an amount of 0 and a noise type of 0, what's vectors hold for one.
fn no_phase() -> Phase {
    Phase { amount: 0.0, noise: NoiseSettings { type_number: 0, method: 0, octaves: 0, dimension: Dimension::Three }, space: NoiseSpace { angles: [0.0, 0.0], frequencies: [1.0, 1.0, 1.0] } }
}

/// 's filter: `kind` (the number), `n`, and the singles `a` and `b` ('s filter node).
fn filter_of(v: &Value) -> Rd<Filter> {
    Ok(Filter {
        number: i64_at(v, "number", 0)?,
        n: i64_at(v, "n", 0)?,
        a: f64_at(v, "a", 0.0)? as f32,
        b: f64_at(v, "b", 0.0)? as f32,
    })
}

fn no_filter() -> Filter {
    Filter { number: 0, n: 0, a: 0.0, b: 0.0 }
}

/// A colour map's colour: three whole levels 0 to 255, as the document holds them.
fn levels_of(v: &Value, what: &str) -> Rd<[u8; 3]> {
    let d = doubles_of(v, 3, what)?;
    let level = |x: f64, k: usize| -> Rd<u8> {
        if !(0.0..=255.0).contains(&x) || x != x.round() {
            return err(format!("{what}: channel {} is {x}, not a whole number of levels 0 to 255", k + 1));
        }
        Ok(x as u8)
    };
    Ok([level(d[0], 0)?, level(d[1], 1)?, level(d[2], 2)?])
}

/// 's colour map: the mode, the colour space ('s `color_space`) and A, B, C.
fn colours_of(v: &Value) -> Rd<ColourMap> {
    // : the colour space word is 0 (rgb) or 0x40 (hls); the testing form's letters are read too
    let space = match member(v, "space") {
        None => ColourSpace::Rgb,
        Some(Value::String(t)) if t == "rgb" => ColourSpace::Rgb,
        Some(Value::String(t)) if t == "hls" => ColourSpace::Hls,
        Some(w) => match f64_of(w) {
            Ok(x) if x == 0.0 => ColourSpace::Rgb,
            Ok(x) if x == 64.0 => ColourSpace::Hls,
            _ => return err("a texture colour space is neither 0 (rgb) nor 0x40 (hls)".to_owned()),
        },
    };
    // a colour is six hexadecimal digits (a colour word's 24 bits, red first) or three levels
    let one = |key: &str| match member(v, key) {
        None => Ok([0u8; 3]),
        Some(Value::String(t)) => {
            let w = u32::from_str_radix(t, 16)
                .ok()
                .filter(|_| t.len() == 6)
                .ok_or_else(|| DescriptionError(format!("a texture colour's {key} is {t:?}, not six hexadecimal digits")))?;
            Ok([(w >> 16) as u8, (w >> 8) as u8, w as u8])
        }
        Some(c) => levels_of(c, &format!("a texture colour's {key}")),
    };
    Ok(ColourMap { mode: i64_at(v, "mode", 0)?, space, a: one("a")?, b: one("b")?, c: one("c")? })
}

fn no_colours() -> ColourMap {
    ColourMap { mode: 0, space: ColourSpace::Rgb, a: [0; 3], b: [0; 3], c: [0; 3] }
}

/// One of a texture's one to three components ('s component and noise nodes).
fn component_of(v: &Value, k: usize) -> Rd<Component> {
    let what = format!("component {}", k + 1);
    Ok(Component {
        outputs: match member(v, "outputs") {
            None => all_outputs(),
            Some(o) => outputs_of(o, &what)?,
        },
        noise: noise_of(member(v, "noise").unwrap_or(&Value::Null))?,
        space: space_of(v, &what)?,
        phase: match member(v, "phase") {
            None => no_phase(),
            Some(p) => phase_of(p)?,
        },
        filter: match member(v, "filter") {
            None => no_filter(),
            Some(f) => filter_of(f)?,
        },
        colours: match member(v, "colours") {
            None => no_colours(),
            Some(c) => colours_of(c)?,
        },
    })
}

/// 's mapping part ('s mapping node): mode, switches, scale, offset and the
/// quaternion of the turn. Tracks are the description's absence: 0, none.
fn mapping_of(v: &Value) -> Rd<Mapping> {
    let three = |key: &str, default: [f64; 3]| -> Rd<Vec3> {
        match member(v, key) {
            None => Ok(default),
            Some(s) => {
                let d = doubles_of(s, 3, &format!("a mapping's {key}"))?;
                Ok([d[0], d[1], d[2]])
            }
        }
    };
    // the pictures' mapping (pictures-lights-other.jsonl) names the switch word `flags`
    // and the turn `rotation`; the textures' (texture.jsonl) `switches` and `turn`: one reader for both
    let turn = match member(v, "turn").or_else(|| member(v, "rotation")) {
        None => [0.0, 0.0, 0.0, 1.0],
        Some(t) => {
            let d = doubles_of(t, 4, "a mapping's turn")?;
            [d[0], d[1], d[2], d[3]]
        }
    };
    Ok(Mapping { mode: u32_at(v, "mode", 0)?, switches: u32_at(v, "switches", u32_at(v, "flags", 0)?)?, scale: three("scale", [1.0; 3])?, offset: three("offset", [0.0; 3])?, turn, tracks: [0; 4] })
}

/// A picture texture of the description (`kind` "picture": `picture` with `width`, `height` and `pixels`, eight
/// hexadecimal digits each, alpha red green blue, the top row first, and the `mapping`):'s picture texture
/// and's picture, the picture's identity being `picture_identity`; the texture's index in the scene's
/// pictures is the caller's to set.
fn picture_texture_of(entry: &Value, name: &str, position: u32, picture_identity: u32) -> Rd<(Texture, HeldPicture)> {
    let p = member(entry, "picture").ok_or_else(|| DescriptionError(format!("picture texture {name:?} has no picture")))?;
    let width = u32_of(member(p, "width").ok_or_else(|| DescriptionError(format!("picture {name:?} has no width")))?)?;
    let height = u32_of(member(p, "height").ok_or_else(|| DescriptionError(format!("picture {name:?} has no height")))?)?;
    let list = member(p, "pixels")
        .and_then(Value::as_array)
        .ok_or_else(|| DescriptionError(format!("picture {name:?} has no pixels")))?;
    let mut pixels = Vec::with_capacity(list.len());
    for px in list {
        let t = px.as_str().filter(|t| t.len() == 8);
        let w = t.and_then(|t| u32::from_str_radix(t, 16).ok());
        pixels.push(Pixel::from_word(w.ok_or_else(|| {
            DescriptionError(format!("picture {name:?} has a pixel that is not eight hexadecimal digits"))
        })?));
    }
    let picture = Picture::new(width, height, pixels).map_err(|e| DescriptionError(format!("picture {name:?}: {e}")))?;
    let mapping = match member(entry, "mapping") {
        // : a picture with no mapping lies once on the surface's own coordinates: mode 2, scale 0
        None => Mapping { mode: 2, switches: 0, scale: [0.0; 3], offset: [0.0; 3], turn: [0.0, 0.0, 0.0, 1.0], tracks: [0; 4] },
        Some(m) => mapping_of(m)?,
    };
    let held = HeldPicture {
        identity: picture_identity,
        name: name.as_bytes().to_vec(),
        picture: std::sync::Arc::new(picture),
        as_channels: None,
        no_pixels: false,
    };
    let texture = Texture {
        class: TextureClass::Picture,
        identity: position,
        name: name.as_bytes().to_vec(),
        mapping,
        body: TextureBody::Picture(PictureTexture { version: 1, picture: picture_identity, index: 0 }),
    };
    Ok((texture, held))
}

/// One texture of the description's `textures` : procedural, or (`kind` "picture") a picture
/// texture (`picture_texture_of`, whose picture is the second item).
fn texture_of(entry: &Value, position: u32, picture_identity: u32) -> Rd<(Texture, Option<HeldPicture>)> {
    let name = entry.get("name").and_then(Value::as_str).unwrap_or("texture").to_owned();
    let kind = entry.get("kind").and_then(Value::as_str).unwrap_or("procedural");
    if kind == "picture" {
        let (t, p) = picture_texture_of(entry, &name, position, picture_identity)?;
        return Ok((t, Some(p)));
    }
    if kind != "procedural" {
        return err(format!("texture {name:?} is of kind {kind:?}, not procedural or picture"));
    }
    let body = member(entry, "texture").unwrap_or(entry);
    let mapping = match member(entry, "mapping") {
        None => Mapping { mode: 0, switches: 0, scale: [1.0; 3], offset: [0.0; 3], turn: [0.0, 0.0, 0.0, 1.0], tracks: [0; 4] },
        Some(m) => mapping_of(m)?,
    };
    let components = match member(body, "components").and_then(Value::as_array) {
        None => return err(format!("texture {name:?} has no components")),
        Some(list) => {
            if list.is_empty() || list.len() > MAX_COMPONENTS {
                return err(format!("texture {name:?} has {} components, not 1 to 3", list.len()));
            }
            list.iter().enumerate().map(|(k, c)| component_of(c, k)).collect::<Rd<Vec<_>>>()?
        }
    };
    let blend = match member(body, "blend").and_then(Value::as_array) {
        None => [0, 0],
        Some(a) => {
            if a.len() != 2 {
                return err(format!("texture {name:?}'s blend is not two numbers"));
            }
            [whole_of(&a[0], "a blend mode")?, whole_of(&a[1], "a blend mode")?]
        }
    };
    // : the add-on keeps a document texture's saved colour word and four doubles.
    let saved_result = match member(body, "saved") {
        None => SavedResult { colour: 0, numbers: [0.0; 4] },
        Some(s) => {
            let colour = member(s, "colour").map(|c| whole_of(c, "the saved colour")).transpose()?.unwrap_or(0) as u32;
            let mut numbers = [0.0; 4];
            if let Some(a) = member(s, "numbers").and_then(Value::as_array) {
                for (k, v) in a.iter().take(4).enumerate() {
                    numbers[k] = f64_of(v)?;
                }
            }
            SavedResult { colour, numbers }
        }
    };
    let settings = TextureSettings {
        outputs: match member(body, "outputs") {
            None => all_outputs(),
            Some(o) => outputs_of(o, &format!("texture {name:?}"))?,
        },
        components,
        blend,
        phase: match member(body, "phase") {
            None => no_phase(),
            Some(p) => phase_of(p)?,
        },
        filter: match member(body, "filter") {
            None => no_filter(),
            Some(f) => filter_of(f)?,
        },
        colours: match member(body, "colours") {
            None => no_colours(),
            Some(c) => colours_of(c)?,
        },
    };
    let texture = Texture {
        class: TextureClass::Procedural,
        identity: position,
        name: name.into_bytes(),
        mapping,
        body: TextureBody::Procedural(Box::new(ProceduralTexture {
            form: RecordForm::Single, // the numbers arrive as doubles; the form only told the reader how to read them
            settings,
            saved_result,
            track: 0,
            copy_words: None,
            as_bryce30: None,
        })),
    };
    Ok((texture, None))
}

/// The description's textures by name: the place in the scene's list and the class code a reference names it by.
type TexIndex = BTreeMap<String, (usize, FourCc)>;

/// A channel's three texture references, by name, resolved to the description's textures.
fn texture_refs(
    v: &Value,
    tex: &TexIndex,
    what: &str,
) -> Rd<([Option<TextureRef>; 3], [Option<usize>; 3])> {
    let mut refs = [None; 3];
    let mut indices = [None; 3];
    for (k, key) in ["a_tex", "b_tex", "alpha_tex"].iter().enumerate() {
        if let Some(t) = member(v, key) {
            let name = t.as_str().ok_or_else(|| DescriptionError(format!("{what}'s {key} is not a texture name")))?;
            let &(i, code) = tex
                .get(name)
                .ok_or_else(|| DescriptionError(format!("{what} names texture {name:?}, which the description lacks")))?;
            refs[k] = Some(TextureRef { code, identity: i as u32 });
            indices[k] = Some(i);
        }
    }
    Ok((refs, indices))
}

/// A colour channel: the plain colour of today, or an object with a side of its own
/// per source and the textures it names .
fn colour_channel_of(
    c: &Value,
    what: &str,
    tex: &TexIndex,
) -> Rd<(Channel<Rgb>, [Option<usize>; 3])> {
    if !matches!(c, Value::Object(_)) {
        return Ok((channel_c(rgb_of(c, what)?), [None; 3]));
    }
    let a = rgb_of(member(c, "a").ok_or_else(|| DescriptionError(format!("{what} has no a"))) ?, &format!("{what}'s a"))?;
    let b = match member(c, "b") {
        None => a,
        Some(vv) => rgb_of(vv, &format!("{what}'s b"))?,
    };
    let (refs, indices) = texture_refs(c, tex, what)?;
    Ok((Channel { refs, flags: u32_at(c, "switches", 0)?, a, b, alpha: f64_at(c, "alpha", 1.0)? as f32 }, indices))
}

/// A value channel, plain or with sides and textures as `colour_channel_of` reads them.
fn value_channel_of(
    c: &Value,
    what: &str,
    tex: &TexIndex,
) -> Rd<(Channel<f32>, [Option<usize>; 3])> {
    if !matches!(c, Value::Object(_)) {
        return Ok((channel_v(f64_of(c)? as f32), [None; 3]));
    }
    let a = f64_of(member(c, "a").ok_or_else(|| DescriptionError(format!("{what} has no a")))?)? as f32;
    let b = match member(c, "b") {
        None => a,
        Some(vv) => f64_of(vv)? as f32,
    };
    let (refs, indices) = texture_refs(c, tex, what)?;
    Ok((Channel { refs, flags: u32_at(c, "switches", 0)?, a, b, alpha: f64_at(c, "alpha", 1.0)? as f32 }, indices))
}

/// One material of the description : its flag word with the older-material rule the
/// document reader applies (`MATERIAL_CURRENT`), and each channel plain or textured.
fn material_of(name: &str, v: &Value, tex: &TexIndex) -> Rd<Material> {
    let flags = u32_at(v, "flags", 0)? | MATERIAL_CURRENT;
    let channels = member(v, "channels").ok_or_else(|| DescriptionError(format!("material {name:?} has no channels")))?;
    let mut colours = std::array::from_fn(|_| channel_c([0.0; 3]));
    let mut textures = ChannelTextures::default();
    for (k, key) in COLOUR_CHANNELS.iter().enumerate() {
        if let Some(c) = member(channels, key) {
            let (channel, refs) = colour_channel_of(c, &format!("material {name:?}: {key}"), tex)?;
            colours[k] = channel;
            textures.colours[k] = refs;
        }
    }
    let mut values = std::array::from_fn(|_| channel_v(0.0));
    for (k, key) in VALUE_CHANNELS.iter().enumerate() {
        if let Some(c) = member(channels, key) {
            let (channel, refs) = value_channel_of(c, &format!("material {name:?}: {key}"), tex)?;
            values[k] = channel;
            textures.values[k] = refs;
        }
    }
    let second = second_of(name, v, tex)?;
    let volume_colour = member(v, "volume_colour").map(|m| rgb_of(m, "volume_colour")).transpose()?.unwrap_or([0.0; 3]);
    let refraction = f64_at(v, "refraction", 1.5)? as f32;
    let volume_numbers = match member(v, "volume_numbers") {
        None => [0.0; 3],
        Some(n) => {
            let d = doubles_of(n, 3, "volume_numbers")?;
            [d[0] as f32, d[1] as f32, d[2] as f32]
        }
    };
    Ok(Material {
        identity: 0, // filled by the caller
        name: name.as_bytes().to_vec(),
        part: MaterialPart {
            flags,
            colours,
            values,
            volume_colour,
            refraction,
            volume_numbers,
            tracks: [0; 19],
        },
        textures,
        second,
    })
}

/// A material's `mix` (`materials[].mix`): the second material's channels and the share m of each,
/// as the second channels a track makes between keys . A channel whose amount is 0 has none.
/// (Mix Materials feed)
fn second_of(name: &str, v: &Value, tex: &TexIndex) -> Rd<SecondChannels> {
    let mut second = SecondChannels::default();
    let Some(mix) = member(v, "mix") else { return Ok(second) };
    if !matches!(mix, Value::Object(_)) {
        return Ok(second); // the earlier add-on's text form is not a mix of ours
    }
    let amounts = doubles_of(
        member(mix, "amounts").ok_or_else(|| DescriptionError(format!("material {name:?}: mix has no amounts")))?,
        13,
        "mix.amounts",
    )?;
    let list = |key: &str, n: usize| -> Rd<Vec<Value>> {
        match member(mix, key) {
            Some(Value::Array(a)) if a.len() == n => Ok(a.clone()),
            _ => Err(DescriptionError(format!("material {name:?}: mix.{key} is not a list of {n}"))),
        }
    };
    for (k, c) in list("colors", 5)?.iter().enumerate() {
        if amounts[k] != 0.0 {
            let (channel, textures) = colour_channel_of(c, &format!("material {name:?}: mix colour {k}"), tex)?;
            second.colours[k] = Some(SecondChannel { channel, textures, amount: amounts[k] });
        }
    }
    for (k, c) in list("values", 8)?.iter().enumerate() {
        if amounts[5 + k] != 0.0 {
            let (channel, textures) = value_channel_of(c, &format!("material {name:?}: mix value {k}"), tex)?;
            second.values[k] = Some(SecondChannel { channel, textures, amount: amounts[5 + k] });
        }
    }
    Ok(second)
}

/// The material of an object the description gives no name: a plain grey diffuse.
/// (objects without a material)
fn default_material() -> Material {
    let mut m = Material {
        identity: 0,
        name: b"default".to_vec(),
        part: MaterialPart {
            flags: MATERIAL_CURRENT,
            colours: [
                channel_c([0.5; 3]),
                channel_c([0.0; 3]),
                channel_c([0.0; 3]),
                channel_c([0.0; 3]),
                channel_c([0.0; 3]),
            ],
            values: [
                channel_v(1.0), // diffusion
                channel_v(0.0),
                channel_v(0.0),
                channel_v(0.0), // transparency 0: opaque
                channel_v(0.0),
                channel_v(0.0),
                channel_v(0.0),
                channel_v(0.0),
            ],
            volume_colour: [0.0; 3],
            refraction: 1.5,
            volume_numbers: [0.0; 3],
            tracks: [0; 19],
        },
        textures: ChannelTextures::default(),
        second: Default::default(),
    };
    m.identity = 1;
    m
}

fn shape_of(name: &str, entry: &Value) -> Rd<(Shape, ObjectKind)> {
    let (shape, kind) = match name {
        "sphere" => (Shape::Sphere, ObjectKind::Sphere),
        "cube" => (Shape::Cube, ObjectKind::Cube),
        "cylinder" => (Shape::Cylinder, ObjectKind::Cylinder),
        "cone" => (Shape::Cone, ObjectKind::Cone),
        "plane" => (Shape::Plane, ObjectKind::Plane),
        "torus" => {
            let t = entry
                .get("torus")
                .ok_or_else(|| DescriptionError("a torus needs its two radii (tube, ring)".to_owned()))?;
            let d = doubles_of(t, 2, "torus radii")?;
            (Shape::Torus { tube_radius: d[0], ring_radius: d[1] }, ObjectKind::Torus)
        }
        "slab" => (Shape::Slab, ObjectKind::Slab),
        "square" => (Shape::Square, ObjectKind::Square),
        "pyramid" => (Shape::Pyramid, ObjectKind::Pyramid),
        "disc" => (Shape::Disc, ObjectKind::Square), // the disc's class is the description's, not's
        other => return err(format!("shape {other:?} is not one of the shapes")),
    };
    Ok((shape, kind))
}

/// The scene's lists that an object's place decides: the terrains and trees (each holds its object's index) and
/// the groups' places by name.
#[derive(Default)]
struct Placer {
    terrains: Vec<TerrainObject>,
    trees: Vec<TreeObject>,
    group_at: BTreeMap<String, usize>,
}

impl Placer {
    /// Append `p` to `objects`; `linked` is the group it is a member of (its index) and its second flag word.
    /// A member's own transform is a document's linked list's: its placement followed by the group's chain, which
    /// is the identity, every member of the description carrying its world .
    fn place(&mut self, objects: &mut Vec<SceneObject>, p: Pending, linked: Option<(usize, u32)>) -> usize {
        let Pending { mut object, terrain, tree } = p;
        let index = objects.len();
        // A document's rows have their own identities (1 is the root list's) and a group its list's: the reader
        // skips as 3.1 and 5.0 a row whose identity is its ancestor's list , so none may be left at 0.
        object.identity = index as u32 + 2;
        if object.body == Body::Group {
            object.linked_list = index as u32 + 0x10000;
        }
        if let Some((parent, flags)) = linked {
            let world = object.world.unwrap_or(Transform { rows: IDENTITY_ROWS, translation: [0.0; 3] });
            let mut placement = object.placement.unwrap_or_else(identity_placement);
            placement.second = if object.body == Body::Group { placement.second } else { world };
            placement.second_flags |= flags; // the role bits, beside the flag words the entry carried
            object.linked_to = Some(parent);
            object.placement = Some(placement);
            object.world = None;
        }
        if let Some(part) = terrain {
            self.terrains.push(TerrainObject { object: index, name: object.name.clone(), part });
            object.body = Body::Terrain(self.terrains.len() - 1);
        }
        if let Some((stored_tree, leaves_material)) = tree {
            let world = object.world.unwrap_or(Transform { rows: IDENTITY_ROWS, translation: [0.0; 3] });
            self.trees.push(TreeObject {
                object: index,
                name: object.name.clone(),
                placement: identity_placement(),
                world,
                wood_material: object.material,
                leaves_material: Some(leaves_material),
                stored_tree,
            });
            object.body = Body::Tree(self.trees.len() - 1);
        }
        if object.body == Body::Group {
            self.group_at.insert(String::from_utf8_lossy(&object.name).into_owned(), index);
        }
        objects.push(object);
        index
    }
}

/// An object not yet placed in the scene's list: its terrain's part and its tree's stored tree (an index into the
/// stored trees, and the leaves' material) wait for the object's place, which the part's `object` holds.
struct Pending {
    object: SceneObject,
    terrain: Option<TerrainPart>,
    tree: Option<(usize, usize)>,
}

/// An object's own transform: the entry's `world` ('s twelve numbers, as the add-on makes them:, the
/// torus-shear cube's and a pyramid's axes times 2, a terrain's own box) where it is given, else its
/// matrix through the change of axes and units.
fn own_world_of(entry: &Value, units: f64, name: &str) -> Rd<Transform> {
    if let Some(w) = member(entry, "world") {
        return twelve_of(w, &format!("object {name:?}'s world"));
    }
    let matrix = entry.get("matrix").ok_or_else(|| DescriptionError(format!("object {name:?} has no matrix")))?;
    bryce_transform(matrix, units, &format!("object {name:?}'s matrix"))
}

/// A group member's second flag word: its role's bit (POSITIVE 0x8, NEGATIVE 0x10, INTERSECT 0x20;
/// NEUTRAL none) and 0x80 for a negative member whose cut shows the positive material.
fn role_word_of(name: &str, m: &Value) -> Rd<u32> {
    let word = match member(m, "role").and_then(Value::as_str).unwrap_or("NEUTRAL") {
        "NEUTRAL" => 0,
        "POSITIVE" => 0x8,
        "NEGATIVE" => 0x10,
        "INTERSECT" => 0x20,
        other => return err(format!("member {name:?} has the role {other:?}, not NEUTRAL, POSITIVE, NEGATIVE or INTERSECT")),
    };
    let cut = matches!(member(m, "cut_shows_positive"), Some(Value::Bool(true)));
    Ok(if word == 0x10 && cut { word | 0x80 } else { word })
}

/// A terrain's own part from the entry's `terrain` (`size`, `heights`, `lower`, `upper`, `mirrored`, `smooth`,
/// `faces`, `solid_faces`): plain packing, no tracks.,
fn terrain_of(entry: &Value, name: &str) -> Rd<TerrainPart> {
    let t = member(entry, "terrain").ok_or_else(|| DescriptionError(format!("terrain {name:?} has no terrain")))?;
    let size = u32_of(member(t, "size").ok_or_else(|| DescriptionError(format!("terrain {name:?} has no size")))?)? as usize;
    let list = member(t, "heights")
        .and_then(Value::as_array)
        .ok_or_else(|| DescriptionError(format!("terrain {name:?} has no heights")))?;
    let mut heights = Vec::with_capacity(list.len());
    for h in list {
        let x = f64_of(h)?;
        if !(0.0..=65535.0).contains(&x) || x != x.trunc() {
            return err(format!("terrain {name:?} has a height {x}, not a whole number from 0 to 65535"));
        }
        heights.push(x as u16);
    }
    let grid = crate::terrain::HeightGrid::new(size, heights)
        .map_err(|e| DescriptionError(format!("terrain {name:?}: {e:?}")))?;
    let flag = |key: &str| -> Rd<bool> {
        match member(t, key) {
            None => Ok(false),
            Some(Value::Bool(b)) => Ok(*b),
            Some(other) => Ok(f64_of(other)? != 0.0),
        }
    };
    let mirrored = flag("mirrored")?;
    // : the description stores no box; its y numbers are those the programs' terrain
    // editing makes (-32767.51 to 32767.51, -65535.01 to 65535.01 for a mirrored terrain)
    let box_y = if mirrored { 65535.01 } else { 32767.51 };
    Ok(TerrainPart {
        grid,
        packing: Packing::Plain,
        mirrored,
        mirrored_word: mirrored as u32,
        smooth: flag("smooth")?,
        stored_bounds: [f64_at(t, "lower", 0.0)?, f64_at(t, "upper", 1.0)?],
        stored_box_y: [-box_y, box_y],
        closing_faces: u32_at(t, "faces", 0)?,
        stored_solid_faces: u32_at(t, "solid_faces", 0)?,
        tracks: [0; 5],
    })
}

fn mesh_of(entry: &Value) -> Rd<Mesh> {
    let tris = entry
        .get("triangles")
        .ok_or_else(|| DescriptionError("a mesh has no triangles".to_owned()))?
        .as_array()
        .ok_or_else(|| DescriptionError("a mesh's triangles are not a list".to_owned()))?;
    let mut triangles = Vec::with_capacity(tris.len());
    for t in tris {
        let corners = t.as_array().ok_or_else(|| DescriptionError("a mesh triangle is not a list".to_owned()))?;
        if corners.len() != 3 {
            return err("a mesh triangle is not three points");
        }
        triangles.push(Triangle {
            corners: [
                vec3_of(&corners[0], "a mesh triangle's corner")?,
                vec3_of(&corners[1], "a mesh triangle's corner")?,
                vec3_of(&corners[2], "a mesh triangle's corner")?,
            ],
            normal: None,
        });
    }
    Ok(Mesh::new(triangles))
}

/// A triangle soup as a document's mesh part: its corner points once each, every triangle's plane normal, and no
/// normals or coordinates (corner words 3, 3, 3 and flag word 0:'s "neither"). A group holds its mesh so.
/// An entry that carries `normals` and `uvs` (Blender's corner normals of the smooth faces and the active
/// texture coordinates, per triangle `corner_normals`, `corner_uvs` and `flags`: bit 0 smooth, bit 1 coordinates)
/// gives its triangles the kinds of: mixed normals (flag word 1 or 3), coordinates (2 or 3), the stored box
/// as the documents have it (the points' bounds moved out by a hundredth of the extent, a flat axis by 0.001).
fn resource_of_soup(mesh: &Mesh, entry: Option<&Value>) -> Rd<crate::scene_meshes::MeshResource> {
    use crate::scene_meshes::{CornerUse, MeshResource, MeshTriangle};
    let mut points: Vec<Vec3> = Vec::new();
    let mut at: BTreeMap<[u64; 3], u32> = BTreeMap::new();
    let mut triangles = Vec::with_capacity(mesh.triangles.len());
    let mut bounds = [f64::INFINITY, f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    let list = |key: &str| -> Rd<Vec<Value>> {
        match entry.and_then(|e| e.get(key)) {
            None | Some(Value::Null) => Ok(Vec::new()),
            Some(v) => Ok(v.as_array().ok_or_else(|| DescriptionError(format!("a mesh's {key} is not a list")))?.to_vec()),
        }
    };
    let normals: Vec<Vec3> = list("normals")?.iter().map(|v| vec3_of(v, "a mesh normal")).collect::<Rd<_>>()?;
    let mut coords: Vec<[f32; 2]> = Vec::new();
    for c in list("uvs")? {
        let a = c.as_array().filter(|a| a.len() == 2).ok_or_else(|| DescriptionError("a mesh coordinate is not two numbers".to_owned()))?;
        coords.push([f64_of(&a[0])? as f32, f64_of(&a[1])? as f32]);
    }
    let (corner_normals, corner_uvs, flags) = (list("corner_normals")?, list("corner_uvs")?, list("flags")?);
    let numbers = |v: Option<&Value>, count: usize, what: &str| -> Rd<Option<[u32; 3]>> {
        let Some(v) = v.filter(|v| !matches!(v, Value::Null)) else { return Ok(None) };
        let a = v.as_array().filter(|a| a.len() == 3).ok_or_else(|| DescriptionError(format!("a mesh triangle's {what} are not three numbers")))?;
        let n = [u32_of(&a[0])?, u32_of(&a[1])?, u32_of(&a[2])?];
        if n.iter().any(|&k| k as usize >= count) {
            return err(format!("a mesh triangle's {what} name one the mesh does not hold"));
        }
        Ok(Some(n))
    };
    for (index, t) in mesh.triangles.iter().enumerate() {
        let mut numbers_of_points = [0u32; 3];
        for (k, c) in t.corners.iter().enumerate() {
            let key = [c[0].to_bits(), c[1].to_bits(), c[2].to_bits()];
            numbers_of_points[k] = *at.entry(key).or_insert_with(|| {
                points.push(*c);
                for a in 0..3 {
                    bounds[a] = bounds[a].min(c[a]);
                    bounds[a + 3] = bounds[a + 3].max(c[a]);
                }
                (points.len() - 1) as u32
            });
        }
        let [a, b, c] = t.corners;
        let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let plane = if length > 0.0 { [n[0] / length, n[1] / length, n[2] / length] } else { [0.0; 3] };
        let word = match flags.get(index) {
            Some(f) => u32_of(f)? & 3,
            None => 0,
        };
        let mixed = if word & 1 != 0 { numbers(corner_normals.get(index), normals.len(), "normals")? } else { None };
        let textured = if word & 2 != 0 { numbers(corner_uvs.get(index), coords.len(), "coordinates")? } else { None };
        let (mixed, textured) = (mixed.filter(|_| word & 1 != 0), textured.filter(|_| word & 2 != 0));
        let (words, flag_word) = match (mixed.is_some(), textured.is_some()) {
            (false, false) => ([3, 3, 3], 0),
            (m, c) => ([0, 0, 0], u32::from(m) | (u32::from(c) << 1)),
        };
        triangles.push(MeshTriangle {
            points: numbers_of_points,
            normals: mixed,
            coords: textured,
            plane,
            corner_use: CornerUse::of(words, flag_word),
            corner_words: words,
            flag_word,
        });
    }
    if !normals.is_empty() || !coords.is_empty() {
        for a in 0..3 {
            let pad = (bounds[a + 3] - bounds[a]) / 100.0;
            let pad = if pad > 0.0 { pad } else { 0.001 };
            bounds[a] -= pad;
            bounds[a + 3] += pad;
        }
    }
    Ok(MeshResource { object: 0, version: 2, bounds, points, normals, coords, triangles })
}

/// The view: the camera's rows right, up and forward and the image plane's centre, which
/// the description's camera gives as the eye plus the forward row times the eye distance
/// `CameraModel` will compute from the same numbers (`sampling::camera`).
fn view_of(v: &Value, width: u32, height: u32, version: RenderAs) -> Rd<(Vec<SceneObject>, Vec<Camera>, View, Vec<usize>)> {
    let position = vec3_of(
        member(v, "position").ok_or_else(|| DescriptionError("the camera has no position".to_owned()))?,
        "the camera's position",
    )?;
    let forward = vec3_of(
        member(v, "forward").ok_or_else(|| DescriptionError("the camera has no forward".to_owned()))?,
        "the camera's forward",
    )?;
    let up = vec3_of(member(v, "up").ok_or_else(|| DescriptionError("the camera has no up".to_owned()))?, "the camera's up")?;
    // The projection the add-on chose for the Blender camera (Outputs):
    // a description without `projection` is a perspective camera, as before.
    let kind = v.get("type").and_then(Value::as_str).unwrap_or("PERSP");
    let projection = match v.get("projection").and_then(Value::as_str) {
        None | Some("perspective") => {
            if kind != "PERSP" {
                return err(format!("the camera's type {kind:?} is not a perspective camera"));
            }
            0
        }
        Some("parallel") => {
            if version != RenderAs::Bryce30 {
                return err("a parallel camera renders only as 3.0 (SO-1.2)");
            }
            1
        }
        Some("panorama") => 2,
        Some(other) => return err(format!("the camera's projection {other:?} is not perspective, parallel or panorama")),
    };
    // The angle the picture's width spans, as the add-on computes it from Blender's fit rules
    // (BS-1 4.4, 4.5;, the camera's
    // frame); a description without it gives the lens and the sensor width, read as a
    // horizontal fit.
    let hfov = match member(v, "frame_angle") {
        None if projection != 0 => 1.0, // the field of view does not enter a parallel or panoramic picture
        Some(a) => {
            let a = f64_of(a)?;
            if !(a > 0.0 && a < std::f64::consts::PI) {
                return err(format!("the camera's frame angle {a} is not between 0 and pi"));
            }
            a
        }
        None => {
            let lens = f64_of(member(v, "lens").ok_or_else(|| DescriptionError("the camera has no lens".to_owned()))?)?;
            let sensor = f64_of(member(v, "sensor_width").unwrap_or(&Value::Number("36".into())))?;
            if !(lens > 0.0) || !(sensor > 0.0) {
                return err("the camera's lens and sensor width must be above 0");
            }
            // Blender's horizontal frame is 2·atan(sensor / (2·lens)).
            2.0 * (sensor / (2.0 * lens)).atan()
        }
    };
    // `pan`: fractions of the image plane's width and height ('s pan), as the add-on
    // turns Blender's shift into them; `shift`, from an earlier add-on, is read the same way.
    let shift = match member(v, "pan").or_else(|| member(v, "shift")) {
        None => [0.0; 2],
        Some(s) => {
            let d = doubles_of(s, 2, "the camera's pan")?;
            [d[0], d[1]]
        }
    };
    // The pixels' width-to-height ratio (Blender's pixel aspects): the plane's shape is the
    // picture's width over height times it,'s aspect.
    let aspect = match member(v, "pixel_aspect") {
        None => 1.0,
        Some(a) => {
            let a = f64_of(a)?;
            if !(a > 0.0 && a.is_finite()) {
                return err(format!("the camera's pixel aspect {a} is not above 0"));
            }
            a
        }
    };
    let norm = |v: Vec3| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l == 0.0 {
            return err(format!("the camera's axis {:?} has length 0", v));
        }
        Ok([v[0] / l, v[1] / l, v[2] / l])
    };
    let (f, u) = (norm(forward)?, norm(up)?);
    // The plane spans 0.8 of the field of view , so the camera part's field of view
    // is the frame over 0.8.
    let fov = hfov / 0.8;
    // The eye distance `CameraModel::new` will compute, so that the eye lands on the
    // camera's position.
    // 3.0 reads the camera part's size : a document of 3.0 may hold another than 1 (size 1e5 -> a plane 1 high),
    // which the add-on sends as `plane_height` also for a perspective camera; 3.1 and 5.0 ignore it.
    let plane_size = match (member(v, "plane_height"), version) {
        (Some(h), RenderAs::Bryce30) => {
            let h = f64_of(h)?;
            if !(h > 0.0 && h.is_finite()) {
                return err(format!("the camera's plane height {h} is not above 0"));
            }
            h
        }
        _ => 1e-5,
    };
    let plane_w = f32r(plane_size * (width as f64 / height as f64) * aspect);
    // A parallel or panoramic camera has its plane's centre at the camera's position (the add-on's choice,
    // a parallel ray leaves the plane, a panoramic one the plane's centre).
    let dist = if projection == 0 {
        x87::div_f64_by(plane_w / 2.0, x87::tan_ext(0.8 * fov / 2.0))
    } else {
        0.0
    };
    // A parallel camera's plane: its zoomed height is `plane_height` Bryce units, held as size * 1e-5 (
    // only 3.0 reads the size, and only 3.0 draws a parallel camera).
    let scale = if projection == 1 {
        let h = f64_of(member(v, "plane_height").ok_or_else(|| DescriptionError("a parallel camera has no plane_height".to_owned()))?)?;
        if !(h > 0.0 && h.is_finite()) {
            return err(format!("the camera's plane height {h} is not above 0"));
        }
        h / 1e-5
    } else if version == RenderAs::Bryce30 {
        plane_size / 1e-5 // a perspective or panoramic 3.0 camera's size, 1 when the description gives none
    } else {
        1.0 // the picture's shape is the size, not the camera
    };
    let mut centre = [position[0] + f[0] * dist, position[1] + f[1] * dist, position[2] + f[2] * dist];
    let right = [u[1] * f[2] - u[2] * f[1], u[2] * f[0] - u[0] * f[2], u[0] * f[1] - u[1] * f[0]];
    let mut rows = [right, u, f];
    // The view's rows as the document holds them (a camera linked below a scaled object has a right row of length
    // 1.5, say): Blender's camera carries axes of length 1 only, so the add-on sends the document's own rows (and the
    // plane's centre) beside forward and up, and they replace the derived ones. All doubles; a description without
    // them is drawn as before. (the camera's view rows).
    if let Some(r) = member(v, "view_rows") {
        let list = r.as_array().filter(|a| a.len() == 3).ok_or_else(|| DescriptionError("the camera's view_rows are not three rows".to_owned()))?;
        for (i, row) in list.iter().enumerate() {
            rows[i] = vec3_of(row, "the camera's view row")?;
        }
    }
    if let Some(c) = member(v, "view_centre") {
        centre = vec3_of(c, "the camera's view_centre")?;
    }
    let world = Transform { rows, translation: centre };
    let object = SceneObject {
        name: b"camera".to_vec(),
        kind: ObjectKind::Camera,
        identity: 0,
        linked_list: 0,
        linked_to: None,
        placement: Some(identity_placement()),
        world: Some(world),
        linked_worlds: None,
        world_open: None,
        listing_worlds: None,
        material: None,
        body: Body::Camera(0),
    };
    let camera = Camera {
        name: b"camera".to_vec(),
        object: 0,
        placement: identity_placement(),
        part: crate::scene_parts::CameraPart {
            angles: [0.0; 3],
            roll: 0.0,
            field_of_view: fov,
            scale,
            aspect,
            pan: shift,
            zoom: [1.0; 2],
            tracks: [0; 7],
            saved_view_word: 0,
        },
        // point 1 [format]: perspective 0x4000, parallel 0x8000, panorama 0x10000
        projection_word: [0x4000, 0x8000, 0x10000][projection],
    };
    let view = View { camera: 0, transforms: ViewTransforms::all(ViewTransform::Stated(world)) };
    Ok((vec![object], vec![camera], view, vec![0]))
}

fn f32r(x: f64) -> f64 {
    x as f32 as f64
}

// ---------------------------------------------------------------------------------------
// The whole description
// ---------------------------------------------------------------------------------------

/// The most pixels a rendered picture may hold. (picture size)
const MAX_PICTURE_PIXELS: u64 = 1 << 28;

/// What a description cannot hold and growing a tree needs: the substrate's `noise-bits-five`  and its
/// three leaf tables . Without them a description with a tree is refused, saying so (Errors).
#[derive(Clone, Copy, Default)]
pub struct Tables<'a> {
    pub noise_bits_five: Option<&'a [u32; 64]>,
    pub leaf: Option<LeafMesh<'a>>,
}

impl<'a> Tables<'a> {
    /// No tables: a description with a tree is refused.
    pub fn none() -> Tables<'a> {
        Tables::default()
    }

    /// The tables of a substrate, those it holds.
    pub fn of(s: &'a crate::substrate::Substrate) -> Tables<'a> {
        let leaf = match (s.leaf_points(), s.leaf_uvs(), s.leaf_triangles()) {
            (Ok(points), Ok(uvs), Ok(triangles)) => Some(LeafMesh { points, uvs, triangles }),
            _ => None,
        };
        Tables { noise_bits_five: s.noise_bits_five().ok(), leaf }
    }
}

/// Read the description ('s JSON, with the `size` the engine adds) into a scene, with no substrate tables.
pub fn describe(text: &str) -> Rd<Described> {
    describe_with(text, &Tables::none())
}

/// Read the description into a scene; a tree is grown with `tables` .
pub fn describe_with(text: &str, tables: &Tables<'_>) -> Rd<Described> {
    let v = crate::cases::json::parse(text).map_err(|e| DescriptionError(format!("the description is not JSON: {e}")))?;
    describe_value(&v, tables)
}

/// The same from the description as parsed: the long-lived renderer keeps the parsed text between requests and
/// changes only the members a request names (camera, render, size).
pub fn describe_value(v: &Value, tables: &Tables<'_>) -> Rd<Described> {
    let (width, height) = match v.get("size").and_then(Value::as_array) {
        Some(a) if a.len() == 2 => (u32_of(&a[0])?, u32_of(&a[1])?),
        _ => return err("the description has no size [width, height]"),
    };
    if width == 0 || height == 0 {
        return err("the description's size is empty");
    }
    // Limits (revised): the format sets no largest size; a renderer refuses, saying why, a size whose
    // pixels it cannot hold. this renderer holds at most MAX_PICTURE_PIXELS pixels a picture.
    if (width as u64) * (height as u64) > MAX_PICTURE_PIXELS {
        return err(format!(
            "a picture of {width} by {height} pixels is larger than this renderer can hold ({MAX_PICTURE_PIXELS} pixels)"
        ));
    }
    let render = v.get("render").ok_or_else(|| DescriptionError("the description has no render".to_owned()))?;
    let version = match render.get("version").and_then(Value::as_str) {
        Some("3.0") => RenderAs::Bryce30,
        Some("3.1") => RenderAs::Bryce31,
        Some("5.0") => RenderAs::Bryce50,
        _ => return err("the render's version is not one of 3.0, 3.1 and 5.0"),
    };
    let units = f64_at(render, "units", 1.0)?;
    if !(units > 0.0) {
        return err("the render's units must be above 0");
    }
    let antialias = u32_at(render, "antialias", 0)?;
    if !matches!(antialias, 0 | 1 | 2 | 4) {
        return err(format!("the anti-aliasing setting {antialias} is not one of 0, 1, 2 and 4"));
    }

    // Bryce 5's part of the settings: present as 5.0, whose render reads it .
    let mut words = [0u32; 10];
    if version == RenderAs::Bryce50 {
        words[0] = 1;
        words[1] = u32_at(render, "rays_a_side", 4)?;
        words[2] = u32_at(render, "premium_rays", 4)?;
        words[3] = match render.get("gamma") {
            Some(Value::Bool(b)) => *b as u32,
            Some(other) => (f64_of(other)? != 0.0) as u32,
            None => 0,
        };
        words[4] = match render.get("dithering") {
            Some(Value::Bool(b)) => *b as u32,
            Some(other) => (f64_of(other)? != 0.0) as u32,
            None => 0,
        };
        // Offsets 180 and 184: the depth limit and the limit of total internal reflections a 5.0 render takes
        // from this part. Left at 0 they stopped every reflected and transmitted ray (glass came out opaque).
        words[6] = u32_at(render, "ray_depth", 6)?;
        words[7] = u32_at(render, "tir_depth", 2)?;
        // The structure code: carried for the record; the render is asked to ignore it.
        words[5] = match member(render, "structure").and_then(Value::as_str) {
            None => 0,
            Some(code) => {
                let b = code.as_bytes();
                if b.len() != 4 {
                    return err(format!("the structure code {code:?} is not four characters"));
                }
                u32::from_be_bytes(b.try_into().unwrap())
            }
        };
        let bits = |key: &str| -> Rd<u32> {
            match member(render, key) {
                None => Ok(0),
                Some(m) => Ok((f64_of(m)? as f32).to_bits()),
            }
        };
        words[8] = bits("focal_distance")?;
        words[9] = bits("lens_radius")?;
    }

    let settings = DocSettings {
        picture_width: width,
        picture_height: height,
        region: Region { top: 0, left: 0, bottom: height, right: width },
        progress: Progress { block: 16, mask: 0, column: 0, row: 0 }, // start over
        switches: u32_at(render, "switches", 0)?,
        animation_length: 0.0,
        frame_rate: 0.0,
        current_time: 0.0,
        anti_aliasing: antialias,
        spatial_option: u32_at(render, "spatial", 0)?,
        animation_switch: 0,
        opening_factor: 1.0,
        bryce5_words: words,
    };

    let sky = match member(&v, "sky") {
        None => return err("the description has no sky"),
        Some(s) => sky_of(s, version)?,
    };
    // (`detail` and `detail_z`, rendered as 3.0 or 3.1: the clouds' edge noise's amount and second
    // number,; as 5.0 not used): given as they are, where the sky names them.
    let mut sky = sky;
    if version != RenderAs::Bryce50 {
        if let Some(s) = v.get("sky") {
            let amount = f64_at(s, "detail", 0.0)?;
            let second = f64_at(s, "detail_z", 0.0)?;
            if amount != 0.0 || second != 0.0 {
                sky.wind_edge = Some([amount, second]);
            }
        }
    }

    // The textures first, in order: a channel names one by name (resolves a
    // reference by class and identity; here the identity is the place in this list).
    let mut textures: Vec<Texture> = Vec::new();
    let mut tex_by_name: TexIndex = BTreeMap::new();
    let mut pictures: Vec<HeldPicture> = Vec::new();
    for entry in v.get("textures").and_then(Value::as_array).map(<[Value]>::to_vec).unwrap_or_default() {
        let (mut texture, held) = texture_of(&entry, textures.len() as u32, pictures.len() as u32 + 1)?;
        if let (Some(held), TextureBody::Picture(p)) = (held, &mut texture.body) {
            p.index = pictures.len();
            pictures.push(held);
        }
        let name = String::from_utf8_lossy(&texture.name).to_string();
        let code = match texture.class {
            TextureClass::Procedural => PROCEDURAL_CODE,
            TextureClass::Picture => PICTURE_CODE,
        };
        if tex_by_name.insert(name.clone(), (textures.len(), code)).is_some() {
            return err(format!("two textures are named {name:?}"));
        }
        textures.push(texture);
    }

    // Materials by name; the default comes first so an object without a name has one.
    let mut materials = vec![default_material()];
    let mut by_name: BTreeMap<String, usize> = BTreeMap::new();
    if let Some(m) = v.get("materials").filter(|m| matches!(m, Value::Object(_))) {
        if let Value::Object(map) = m {
            for (name, settings) in map {
                materials.push(material_of(name, settings, &tex_by_name)?);
                by_name.insert(name.clone(), materials.len() - 1);
            }
        }
    }
    for (k, m) in materials.iter_mut().enumerate() {
        m.identity = (k + 1) as u32;
    }
    let default_material_index = 0usize;

    // The synthetic materials (a gradient light's tint) follow the description's own.
    let mut pending: Vec<Pending> = Vec::new();
    let mut meshes: Vec<Mesh> = Vec::new();
    let mut stored_trees: Vec<StoredTree> = Vec::new();
    let mut shared_trees: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    let entries = v.get("objects").and_then(Value::as_array).map(<[Value]>::to_vec).unwrap_or_default();
    let material_named = |name: &str, key: &str, entry: &Value| -> Rd<Option<usize>> {
        match member(entry, key).and_then(Value::as_str) {
            None => Ok(None),
            // the add-on writes an empty name for an object with no material: the default one
            Some("") => Ok(None),
            Some(n) => by_name
                .get(n)
                .copied()
                .map(Some)
                .ok_or_else(|| DescriptionError(format!("object {name:?} names material {n:?}, which is absent"))),
        }
    };
    let mut mesh_members: BTreeMap<String, usize> = BTreeMap::new();
    let mut mesh_resources: Vec<crate::scene_meshes::MeshResource> = Vec::new();
    let mut membership: BTreeMap<String, (String, u32)> = BTreeMap::new();
    for entry in &entries {
        if entry.get("shape").and_then(Value::as_str) == Some("group") {
            let group = entry.get("name").and_then(Value::as_str).unwrap_or("object").to_owned();
            if let Some(Value::Object(map)) = member(entry, "members") {
                for (member_name, m) in map {
                    membership.insert(member_name.clone(), (group.clone(), role_word_of(member_name, m)?));
                }
            }
        }
    }
    for entry in &entries {
        let name = entry.get("name").and_then(Value::as_str).unwrap_or("object").to_owned();
        let shape_name = entry.get("shape").and_then(Value::as_str).unwrap_or("");
        let material = material_named(&name, "material", entry)?.unwrap_or(default_material_index);
        let mut terrain = None;
        let (body, kind, world) = if shape_name == "mesh" {
            // The add-on carries the triangles in the world's space already : the
            // object's own matrix is theirs, so the placement is the identity.
            let mesh = mesh_of(entry)?;
            if mesh.triangles.is_empty() {
                return err(format!("mesh {name:?} holds no triangles"));
            }
            let identity = Transform { rows: IDENTITY_ROWS, translation: [0.0; 3] };
            if membership.contains_key(&name) {
                // : a mesh held by a group takes part as a stored mesh (a document's mesh part:
                // the world's triangles as its own points, no normals or coordinates), met only through its group;
                // its `object` is the place it is given among the objects, set when the member is placed
                mesh_members.insert(name.clone(), mesh_resources.len());
                mesh_resources.push(resource_of_soup(&mesh, Some(entry))?);
                (Body::MeshResource(mesh_resources.len() - 1), ObjectKind::TriMesh, identity)
            } else if entry.get("normals").is_some() || entry.get("uvs").is_some() {
                // : smooth shading and texture coordinates: the mesh is a document's ('s kinds).
                // With the object's `origin` (its place in the world) the points are given in the mesh's own
                // space and the placement is that translation, as a document holds a mesh: a texture mapped in
                // object space reads the own point , which the world's points would shift.
                match entry.get("origin") {
                    Some(o) if !matches!(o, Value::Null) => {
                        let at = vec3_of(o, "a mesh's origin")?;
                        let moved: Vec<Triangle> = mesh
                            .triangles
                            .iter()
                            .map(|t| Triangle {
                                corners: t.corners.map(|c| [c[0] - at[0], c[1] - at[1], c[2] - at[2]]),
                                normal: t.normal,
                            })
                            .collect();
                        mesh_resources.push(resource_of_soup(&Mesh::new(moved), Some(entry))?);
                        let placed = Transform { rows: IDENTITY_ROWS, translation: at };
                        (Body::MeshResource(mesh_resources.len() - 1), ObjectKind::TriMesh, placed)
                    }
                    _ => {
                        mesh_resources.push(resource_of_soup(&mesh, Some(entry))?);
                        (Body::MeshResource(mesh_resources.len() - 1), ObjectKind::TriMesh, identity)
                    }
                }
            } else {
                meshes.push(mesh);
                (Body::Mesh(meshes.len() - 1), ObjectKind::TriMesh, identity)
            }
        } else if shape_name == "metaball" {
            // : a metaball's own transform is's form of its matrix
            // (rows C(X), C(Z), C(Y), then C(T), each times units: `bryce_transform`), its sign
            // -1 with bryce_negative (the description's `sign`, or `negative`).
            let negative = match entry.get("sign").map(f64_of).transpose()? {
                Some(s) if s == -1.0 => true,
                Some(s) if s == 1.0 => false,
                Some(s) => return err(format!("metaball {name:?} has the sign {s}, not 1 or -1")),
                None => matches!(entry.get("negative"), Some(Value::Bool(true))),
            };
            (Body::Metaball { negative }, ObjectKind::Blob, own_world_of(entry, units, &name)?)
        } else if shape_name == "group" {
            // . Every member of the description
            // carries its world already , so the group's own placement is the identity.
            (Body::Group, ObjectKind::Group, Transform { rows: IDENTITY_ROWS, translation: [0.0; 3] })
        } else if shape_name == "terrain" {
            // : the own part, plain heights; the object's transform is the add-on's
            // `world` (its `terrain_world`,'s own box)
            terrain = Some(terrain_of(entry, &name)?);
            (Body::Terrain(0), ObjectKind::HeightField, own_world_of(entry, units, &name)?)
        } else {
            let (shape, kind) = shape_of(shape_name, entry)?;
            (Body::Shape(shape), kind, own_world_of(entry, units, &name)?)
        };
        let _ = entry.get("role"); // roles, cuts and negatives  act through a group's members
        let mut placement = placement_with_flags(entry, None)?;
        if shape_name == "mesh" && matches!(body, Body::MeshResource(_)) && world.translation != [0.0; 3] {
            placement.second = world; // a document's mesh holds its place in the second transform (reads it)
        }
        pending.push(Pending {
            object: SceneObject {
                name: name.into_bytes(),
                kind,
                identity: 0,
                linked_list: 0,
                linked_to: None,
                placement: Some(placement),
                world: Some(world),
                linked_worlds: None,
                world_open: None,
                listing_worlds: None,
                material: Some(material),
                body,
            },
            terrain,
            tree: None,
        });
    }
    for entry in v.get("trees").and_then(Value::as_array).map(<[Value]>::to_vec).unwrap_or_default() {
        let name = entry.get("name").and_then(Value::as_str).unwrap_or("tree").to_owned();
        // Trees that name the same `stored_id` show one stored tree (items shared by trees of one).
        let shared = member(&entry, "stored_id").and_then(|v| f64_of(v).ok()).map(|f| f as i64).and_then(|id| shared_trees.get(&id).copied());
        let stored_index = match shared {
            Some(index) => index,
            None => {
                let stored = match member(&entry, "stored") {
                    Some(given) => trees::stored_tree_given(given, &name)?, // a document's tree, already grown
                    None => {
                        let settings = member(&entry, "tree").ok_or_else(|| DescriptionError(format!("tree {name:?} has no settings")))?;
                        trees::stored_tree_of(settings, &name, tables)?
                    }
                };
                stored_trees.push(stored);
                if let Some(id) = member(&entry, "stored_id").and_then(|v| f64_of(v).ok()).map(|f| f as i64) {
                    shared_trees.insert(id, stored_trees.len() - 1);
                }
                stored_trees.len() - 1
            }
        };
        let wood = material_named(&name, "material", &entry)?.unwrap_or(default_material_index);
        let leaves = material_named(&name, "leaves_material", &entry)?.unwrap_or(wood);
        pending.push(Pending {
            object: SceneObject {
                name: name.clone().into_bytes(),
                kind: ObjectKind::Tree,
                identity: 0,
                linked_list: 0,
                linked_to: None,
                placement: Some(placement_with_flags(&entry, None)?),
                world: Some(own_world_of(&entry, units, &name)?),
                linked_worlds: None,
                world_open: None,
                listing_worlds: None,
                material: Some(wood),
                body: Body::Tree(0),
            },
            terrain: None,
            tree: Some((stored_index, leaves)),
        });
    }

    // The objects in the order of a document's lists: the root list (the objects that are no group's member, then
    // the lights and the camera), then each group's members, a parent before its children .
    let mut objects: Vec<SceneObject> = Vec::new();
    let mut placer = Placer::default();
    let mut members: Vec<Pending> = Vec::new();
    for p in pending {
        if membership.contains_key(&*String::from_utf8_lossy(&p.object.name)) {
            members.push(p);
        } else {
            placer.place(&mut objects, p, None);
        }
    }

    // The lights: the radial ones with the object that carries their place, then the sun.
    let mut lights: Vec<Light> = Vec::new();
    let light_entries = v.get("lights").and_then(Value::as_array).map(<[Value]>::to_vec).unwrap_or_default();
    // , (the light's
    // own object has a material: the entry's `material`, the default one for a member or a visible light's)
    let mut light_members: BTreeMap<String, usize> = BTreeMap::new();
    for entry in &light_entries {
        let (mut object, mut light, tint) = light_of(entry, objects.len(), lights.len())?;
        let lname = String::from_utf8_lossy(&object.name).into_owned();
        let is_member = membership.contains_key(&lname);
        let shows = matches!(&light, Light::Radial(l) if l.switches & 0x200 != 0);
        object.material = match material_named(&lname, "material", entry)? {
            Some(m) => Some(m),
            None if is_member || shows => Some(default_material_index),
            None => None,
        };
        // , point 4: a radial light whose own material has a texture reference is a gel;
        // its colour as the reader gives it is the own colour (the stand-in), the material read at each lit point.
        if let (Some(m), Light::Radial(l)) = (object.material, &mut light) {
            if !materials[m].part.is_plain() {
                l.gel = Some(m);
                l.colour = l.own_colour;
            }
        }
        if let Some(colour) = tint {
            let mut m = default_material();
            m.name = format!("tint of {}", String::from_utf8_lossy(&object.name)).into_bytes();
            m.part.colours[0] = channel_c(colour);
            m.identity = materials.len() as u32 + 1;
            materials.push(m);
            object.material = Some(materials.len() - 1);
        }
        if is_member {
            light_members.insert(lname, lights.len());
            members.push(Pending { object, terrain: None, tree: None });
        } else {
            objects.push(object);
        }
        lights.push(light);
    }
    match member(&v, "sun") {
        None => return err("the description has no sun"),
        Some(s) => lights.push(sun_of(s)?),
    }

    let (camera_objects, cameras, view, _) = view_of(
        member(&v, "camera").ok_or_else(|| DescriptionError("the description has no camera".to_owned()))?,
        width,
        height,
        version,
    )?;
    let mut cameras = cameras;
    cameras[0].object = objects.len();
    objects.extend(camera_objects);
    if cameras.is_empty() {
        return err("the description's camera made no camera");
    }
    // The members, a parent before its children; a member whose group is not among the objects is an error.
    while !members.is_empty() {
        let before = members.len();
        let mut rest = Vec::new();
        for p in members {
            let name = String::from_utf8_lossy(&p.object.name).into_owned();
            let (group, flags) = membership[&name].clone();
            match placer.group_at.get(&group) {
                Some(&parent) => {
                    let at = placer.place(&mut objects, p, Some((parent, flags)));
                    if let Some(&k) = mesh_members.get(&name) {
                        mesh_resources[k].object = at;
                    }
                    if let Some(&k) = light_members.get(&name) {
                        if let Light::Radial(l) = &mut lights[k] {
                            l.object = at;
                        }
                    }
                }
                None => rest.push(p),
            }
        }
        members = rest;
        if members.len() == before {
            let name = String::from_utf8_lossy(&members[0].object.name).into_owned();
            return err(format!("object {name:?} is a member of a group that is not placed (a cycle, or not a group)"));
        }
    }
    crate::scene::linked_worlds(&mut objects, &mut lights, &mut placer.trees);

    // `preview` off asks for the frame alone; with no word the frame is asked for as the
    // case command's render: with the quick preview before it .
    let no_preview = matches!(member(render, "preview"), Some(Value::Bool(false)));
    // : the place in `textures` of each layer's cloud texture
    let cloud_texture = |key: &str| -> Rd<Option<usize>> {
        let Some(at) = v.get("sky").and_then(|s| member(s, key)) else { return Ok(None) };
        let place = f64_of(at)?;
        if place < 0.0 || place.fract() != 0.0 || place as usize >= textures.len() {
            return err(format!("sky.{key} is {place}, not a place in the description's {} textures", textures.len()));
        }
        Ok(Some(place as usize))
    };
    let clouds = CloudTextures { stratus: cloud_texture("stratus_texture")?, cumulus: cloud_texture("cumulus_texture")? };
    Ok(Described {
        scene: Scene {
            settings,
            objects,
            materials,
            lights,
            cameras,
            view,
            sky,
            trees: placer.trees,
            stored_trees,
            terrains: placer.terrains,
            paths: Vec::new(), // a description holds no path (is a document's)
            textures,
            pictures,
            clouds,
            meshes,
            mesh_resources, // a document's mesh parts, and the meshes a group holds (Meshes)
            form: crate::animation::PartsForm::Bryce3d,
            animation: crate::scene_tracks::SceneTracks::default(), // a description has no tracks
            track_list: None, // nor a track list
            tracking: Vec::new(),
            frame_time: None,
            older: None,
        },
        version,
        no_preview,
        ignore_structure_code: version == RenderAs::Bryce50,
    })
}

#[cfg(test)]
mod tests {
    // (the camera's frame): the description's camera
    // fields `frame_angle`, `pan`, `pixel_aspect` (BS-1 4.4, 4.5) and the earlier reading.
    use super::*;

    // Limits (revised): a size whose pixels the renderer cannot hold is refused, saying why.
    #[test]
    fn a_size_too_large_is_refused_with_a_reason() {
        let text = "{\"size\": [1000000, 1000000], \"render\": {\"version\": \"3.0\"}}";
        let Err(e) = describe(text) else { panic!("refused") };
        assert!(e.0.contains("larger than this renderer can hold"), "{}", e.0);
        let Err(e) = describe("{\"size\": [64, 64]}") else { panic!("no render") };
        assert!(e.0.contains("no render"), "{}", e.0);
    }

    fn camera(fields: &str) -> crate::scene_parts::CameraPart {
        let text = format!("{{\"position\": [0, 0, 0], \"forward\": [0, 0, 1], \"up\": [0, 1, 0], {fields}}}");
        let v = crate::cases::json::parse(&text).expect("json");
        let (_, cameras, _, _) = view_of(&v, 1920, 1080, RenderAs::Bryce50).expect("camera");
        cameras[0].part
    }

    // : a metaball of the description is read as's object: its own
    // transform from its matrix, its sign from `sign`; a sign other than 1 and -1 is refused.
    fn with_object(object: &str) -> Rd<Described> {
        let text = format!(
            "{{\"size\": [8, 6], \"render\": {{\"version\": \"5.0\", \"units\": 1.0}}, \"sky\": {{}}, \
             \"sun\": {{\"direction\": [0.3, 0.6, -0.7], \"colour\": [1, 1, 1]}}, \
             \"camera\": {{\"position\": [0, 0, -10], \"forward\": [0, 0, 1], \"up\": [0, 1, 0], \"frame_angle\": 1.0}}, \
             \"objects\": [{object}]}}"
        );
        describe(&text)
    }

    /// : the edge noise's two numbers reach a render as 3.0 or 3.1 only.
    #[test]
    fn the_edge_noise_numbers_are_given_below_five() {
        let text = |version: &str, sky: &str| {
            format!(
                "{{\"size\": [8, 6], \"render\": {{\"version\": \"{version}\", \"units\": 1.0}}, \"sky\": {{{sky}}}, \
                 \"sun\": {{\"direction\": [0.3, 0.6, -0.7], \"colour\": [1, 1, 1]}}, \
                 \"camera\": {{\"position\": [0, 0, -10], \"forward\": [0, 0, 1], \"up\": [0, 1, 0], \"frame_angle\": 1.0}}, \
                 \"objects\": []}}"
            )
        };
        let sky = "\"detail\": 0.5, \"detail_z\": 1.5";
        assert_eq!(describe(&text("3.1", sky)).unwrap().scene.sky.wind_edge, Some([0.5, 1.5]));
        assert_eq!(describe(&text("3.0", "")).unwrap().scene.sky.wind_edge, None);
        assert_eq!(describe(&text("5.0", sky)).unwrap().scene.sky.wind_edge, None);
    }

    /// : a sky with a Bryce 5 part is read in version 7's form (the form a render as 5.0
    /// reads), its haze and fog numbers NaN where the document gave none (the render then takes
    /// the substrate's `haze-five`); a sky without the part stays Bryce 3D's.
    #[test]
    fn a_bryce_5_sky_part_is_version_seven_with_unset_haze_numbers() {
        let text = |sky: &str| {
            format!(
                "{{\"size\": [8, 6], \"render\": {{\"version\": \"5.0\", \"units\": 1.0}}, \"sky\": {{{sky}}}, \
                 \"sun\": {{\"direction\": [0.3, 0.6, -0.7], \"colour\": [1, 1, 1]}}, \
                 \"camera\": {{\"position\": [0, 0, -10], \"forward\": [0, 0, 1], \"up\": [0, 1, 0], \"frame_angle\": 1.0}}, \
                 \"objects\": []}}"
            )
        };
        let five = "\"bryce5_part\": true, \"haze_shift_colour\": [0, 0.05, 0.25], \"star_orientation\": [0, 1, 0]";
        let part = describe(&text(five)).unwrap().scene.sky;
        assert_eq!(part.version, crate::scene_sky::SKY_VERSION_SEVEN);
        let f = part.five.expect("the part is kept");
        assert!(f.haze_offset.is_nan() && f.haze_thickness.is_nan() && f.fog_offset.is_nan());
        let given = format!("{five}, \"haze_offset\": 0.25, \"haze_thickness\": 0.5, \"fog_offset\": 0.125");
        assert_eq!(describe(&text(&given)).unwrap().scene.sky.five.unwrap().haze_offset, 0.25);
        // without the part the 5.0 render reads a Bryce 3D part of version 2 (a version 1 part is not taken over);
        // 3.0 and 3.1 read the two alike, so there it stays 1
        assert_eq!(describe(&text("")).unwrap().scene.sky.version, crate::scene_sky::SKY_VERSION_TWO);
        let as_31 = text("").replace("\"5.0\"", "\"3.1\"");
        assert_eq!(describe(&as_31).unwrap().scene.sky.version, 1);
        // the render fills the unset three from haze-five, and drops the part where there is no table
        let scene = describe(&text(five)).unwrap().scene;
        let filled = crate::render::plan::opened_with_haze_five(&scene, RenderAs::Bryce50, Some([0.5, 0.5, 0.11])).unwrap();
        let f = filled.sky.five.unwrap();
        assert_eq!((f.haze_offset, f.haze_thickness, f.fog_offset), (0.5, 0.5, 0.11));
        let bare = crate::render::plan::opened_with_haze_five(&scene, RenderAs::Bryce50, None).unwrap();
        assert!(bare.sky.five.is_none());
    }

    /// A sky colour of NaN or infinity is kept as the document holds it (the original draws it; Limits),
    /// where every other colour of a description must be finite.
    #[test]
    fn a_sky_colour_keeps_nan_and_infinity() {
        let text = |colour: &str| {
            format!(
                "{{\"size\": [8, 6], \"render\": {{\"version\": \"5.0\", \"units\": 1.0}}, \"sky\": {{\"plain_colour\": {colour}}}, \
                 \"sun\": {{\"direction\": [0.3, 0.6, -0.7], \"colour\": [1, 1, 1]}}, \
                 \"camera\": {{\"position\": [0, 0, -10], \"forward\": [0, 0, 1], \"up\": [0, 1, 0], \"frame_angle\": 1.0}}, \
                 \"objects\": []}}"
            )
        };
        let c = describe(&text("[NaN, Infinity, 0.5]")).unwrap().scene.sky.plain_sky_colour;
        assert!(c[0].is_nan() && c[1] == f32::INFINITY && c[2] == 0.5);
    }

    /// A 5.0 render takes its depth limit and its limit of total internal reflections from Bryce 5's part of the
    /// settings; with both left at 0 no reflected or transmitted ray was sent and glass came out opaque.
    #[test]
    fn the_depth_limits_reach_the_five_part() {
        let with = |render: &str| {
            let text = format!(
                "{{\"size\": [8, 6], \"render\": {{\"version\": \"5.0\", \"units\": 1.0{render}}}, \"sky\": {{}}, \
                 \"sun\": {{\"direction\": [0.3, 0.6, -0.7], \"colour\": [1, 1, 1]}}, \
                 \"camera\": {{\"position\": [0, 0, -10], \"forward\": [0, 0, 1], \"up\": [0, 1, 0], \"frame_angle\": 1.0}}, \
                 \"objects\": []}}"
            );
            describe(&text).unwrap().scene.settings.depths()
        };
        assert_eq!(with(""), Some((6, 2)));
        assert_eq!(with(", \"ray_depth\": 4, \"tir_depth\": 3"), Some((4, 3)));
        assert_eq!(with(", \"ray_depth\": 6, \"tir_depth\": null"), Some((6, 2)));
    }

    /// Two trees of a document that name one `stored_id` show one stored tree (the items they share), and
    /// a tree with another number or none has its own.
    #[test]
    fn trees_that_name_one_stored_id_share_it() {
        let stored = "{\"pieces\": [{\"parent\": -1, \"matrix\": [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]}], \"leaves\": [], \"shapes\": []}";
        let tree = |name: &str, id: &str| {
            format!("{{\"name\": \"{name}\", \"matrix\": [[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], [0, 0, 0, 1]], \"stored\": {stored}{id}}}")
        };
        let text = format!(
            "{{\"size\": [8, 6], \"render\": {{\"version\": \"5.0\", \"units\": 1.0}}, \"sky\": {{}}, \
             \"sun\": {{\"direction\": [0.3, 0.6, -0.7], \"colour\": [1, 1, 1]}}, \
             \"camera\": {{\"position\": [0, 0, -10], \"forward\": [0, 0, 1], \"up\": [0, 1, 0], \"frame_angle\": 1.0}}, \
             \"objects\": [], \"trees\": [{}, {}, {}, {}]}}",
            tree("a", ", \"stored_id\": 4"),
            tree("b", ", \"stored_id\": 4"),
            tree("c", ", \"stored_id\": 5"),
            tree("d", "")
        );
        let d = describe(&text).expect("trees are read");
        assert_eq!(d.scene.stored_trees.len(), 3);
        let shown: Vec<usize> = d.scene.trees.iter().map(|t| t.stored_tree).collect();
        assert_eq!(shown, vec![0, 0, 1, 2]);
    }

    /// A tree of a document that was already grown (the add-on sends it from the scene a file of the earlier add-on
    /// kept) is read as it is: its pieces, leaves and leaf shapes, not grown again from settings.
    #[test]
    fn a_stored_tree_is_read_as_given() {
        let text = |stored: &str| {
            format!(
                "{{\"size\": [8, 6], \"render\": {{\"version\": \"5.0\", \"units\": 1.0}}, \"sky\": {{}}, \
                 \"sun\": {{\"direction\": [0.3, 0.6, -0.7], \"colour\": [1, 1, 1]}}, \
                 \"camera\": {{\"position\": [0, 0, -10], \"forward\": [0, 0, 1], \"up\": [0, 1, 0], \"frame_angle\": 1.0}}, \
                 \"objects\": [], \"trees\": [{{\"name\": \"t\", \"matrix\": [[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], [0, 0, 0, 1]], \
                 \"stored\": {stored}}}]}}"
            )
        };
        let shape = "{\"vertices\": [[0, 0, 0], [1, 0, 0], [0, 1, 0]], \"normals\": [[0, 0, 1]], \"uvs\": [[0, 0], [1, 0], [0, 1]], \
                     \"triangles\": [{\"v\": [0, 1, 2], \"n\": [0, 0, 0], \"uv\": [0, 1, 2]}]}";
        let piece = |parent: i32| format!("{{\"parent\": {parent}, \"matrix\": [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, {parent}, 0]}}");
        let stored = format!(
            "{{\"pieces\": [{}, {}], \"leaves\": [{{\"shape\": 0, \"matrix\": [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 2, 0]}}], \"shapes\": [{shape}]}}",
            piece(-1),
            piece(0)
        );
        let d = describe(&text(&stored)).expect("a stored tree is read");
        let tree = &d.scene.stored_trees[0];
        assert_eq!((tree.pieces.len(), tree.leaves.len(), tree.shapes.len()), (2, 1, 1));
        assert_eq!((tree.pieces[0].parent, tree.pieces[1].parent), (None, Some(0)));
        assert_eq!(tree.leaves[0].transform.translation, [0.0, 2.0, 0.0]);
        assert_eq!(tree.shapes[0].triangles, vec![[0, 1, 2, 0, 0, 0, 0, 1, 2]]);
        // a leaf that names a shape the tree does not hold is refused
        let bad = stored.replace("\"shape\": 0", "\"shape\": 3");
        assert!(describe(&text(&bad)).is_err());
    }

    #[test]
    fn a_metaball_is_read_with_its_sign() {
        let ball = |sign: &str| {
            with_object(&format!(
                "{{\"name\": \"b\", \"shape\": \"metaball\", \"matrix\": [[2, 0, 0, 0], [0, 2, 0, 0], [0, 0, 2, 0], [1, 2, 3, 1]], {sign}}}"
            ))
        };
        let d = ball("\"sign\": -1").expect("a metaball is read");
        let o = &d.scene.objects[0];
        assert_eq!((o.body, o.kind), (Body::Metaball { negative: true }, ObjectKind::Blob));
        assert_eq!(o.world.map(|w| w.translation), Some([1.0, 3.0, 2.0]));
        let d = ball("\"sign\": 1").expect("a metaball is read");
        assert_eq!(d.scene.objects[0].body, Body::Metaball { negative: false });
        assert!(ball("\"sign\": 0").is_err());
    }

    #[test]
    fn frame_angle_is_the_lens_reading_when_they_agree() {
        let by_lens = camera("\"lens\": 50, \"sensor_width\": 36");
        let angle = 2.0 * (36.0f64 / 100.0).atan();
        let by_angle = camera(&format!("\"frame_angle\": {angle:?}"));
        assert_eq!(by_lens.field_of_view, by_angle.field_of_view);
        assert_eq!(by_angle.aspect, 1.0);
        assert_eq!(by_angle.pan, [0.0, 0.0]);
    }

    #[test]
    fn pan_and_pixel_aspect_are_carried() {
        let c = camera("\"frame_angle\": 1.0, \"pan\": [-0.25, 0.5], \"pixel_aspect\": 2.0");
        assert_eq!(c.pan, [-0.25, 0.5]);
        assert_eq!(c.aspect, 2.0);
        assert_eq!(c.field_of_view, 1.0 / 0.8);
    }

    #[test]
    fn a_frame_angle_out_of_range_is_refused() {
        let text = "{\"position\": [0, 0, 0], \"forward\": [0, 0, 1], \"up\": [0, 1, 0], \"frame_angle\": 4.0}";
        let v = crate::cases::json::parse(text).expect("json");
        assert!(view_of(&v, 10, 10, RenderAs::Bryce50).is_err());
    }

    // Outputs,: a parallel camera (3.0 only) and a panorama.
    fn camera_as(version: RenderAs, fields: &str) -> Rd<Camera> {
        let text = format!("{{\"position\": [0, 0, 0], \"forward\": [0, 0, 1], \"up\": [0, 1, 0], {fields}}}");
        let v = crate::cases::json::parse(&text).expect("json");
        view_of(&v, 1920, 1080, version).map(|(_, mut c, _, _)| c.remove(0))
    }

    #[test]
    fn a_parallel_camera_sets_its_plane_and_word_as_3_0_only() {
        let c = camera_as(RenderAs::Bryce30, "\"type\": \"ORTHO\", \"projection\": \"parallel\", \"plane_height\": 0.5").expect("parallel");
        assert_eq!(c.projection_word, 0x8000);
        assert_eq!(c.part.scale, 0.5 / 1e-5);
        assert!(camera_as(RenderAs::Bryce31, "\"projection\": \"parallel\", \"plane_height\": 0.5").is_err());
        assert!(camera_as(RenderAs::Bryce50, "\"projection\": \"parallel\", \"plane_height\": 0.5").is_err());
        assert!(camera_as(RenderAs::Bryce30, "\"projection\": \"parallel\"").is_err());
    }

    #[test]
    fn a_panorama_needs_no_field_of_view() {
        for version in [RenderAs::Bryce30, RenderAs::Bryce31, RenderAs::Bryce50] {
            let c = camera_as(version, "\"type\": \"PANO\", \"projection\": \"panorama\", \"pan\": [0.1, 0.2]").expect("panorama");
            assert_eq!(c.projection_word, 0x10000);
            assert_eq!(c.part.pan, [0.1, 0.2]);
        }
        assert!(camera_as(RenderAs::Bryce50, "\"type\": \"PANO\", \"frame_angle\": 1.0").is_err());
    }

    // : the add-on's lights with a beam, as the description gives them
    // (case beam-001's numbers).
    fn light_with_tint(fields: &str) -> Rd<(SceneObject, Light, Option<Rgb>)> {
        let text = format!(
            "{{\"name\": \"l\", \"colour\": [0.5, 0.5, 0.5], \"intensity\": 2.0, \"switches\": 256, \
             \"position\": [-26.0, -4.0, 28.0], {fields}}}"
        );
        light_of(&crate::cases::json::parse(&text).expect("json"), 3, 0)
    }

    fn light(fields: &str) -> Rd<(SceneObject, Light)> {
        light_with_tint(fields).map(|(o, l, _)| (o, l))
    }

    const WORLD: &str = "\"world\": [2.0, 0.0, 0.0, 0.0, -2.0, 0.0, 0.0, 0.0, -0.5, -26.0, -4.0, 28.0]";

    #[test]
    fn a_cone_is_a_spot_beam_with_its_world() {
        let (object, l) = light(&format!("\"kind\": \"cone\", \"exponents\": [2.5], {WORLD}")).expect("cone");
        assert_eq!(object.kind, ObjectKind::ConeLight);
        let w = object.world.expect("world");
        assert_eq!(w.rows, [[2.0, 0.0, 0.0], [0.0, -2.0, 0.0], [0.0, 0.0, -0.5]]);
        assert_eq!(w.translation, [-26.0, -4.0, 28.0]);
        match l {
            Light::Radial(r) => {
                assert_eq!(r.beam, Some(BeamRecord::Spot { exponent: 2.5, falloff_track: 0 }));
                assert_eq!(r.position, [-26.0, -4.0, 28.0]);
                assert_eq!(r.object, 3);
                assert_eq!(r.part.version, 1);
            }
            Light::Sun(_) => panic!("not a sun"),
        }
    }

    #[test]
    fn a_box_is_a_parallel_beam_and_five_numbers_make_version_three() {
        let (object, l) = light(&format!(
            "\"kind\": \"box\", \"exponents\": [1.0, 3.0], {WORLD}, \"radius\": 0.0, \"shadow_ambience\": 0.5, \
             \"shadow_colour\": [0.25, 0.5, 1.0], \"range\": 10.0"
        ))
        .expect("box");
        assert_eq!(object.kind, ObjectKind::CubeLight);
        match l {
            Light::Radial(r) => {
                assert_eq!(r.beam, Some(BeamRecord::Parallel { exponents: [1.0, 3.0] }));
                assert_eq!(r.part.version, crate::scene_parts::LIGHT_VERSION_FIVE);
                assert_eq!(r.part.five.range, 10.0);
                assert_eq!(r.part.five.shadow_ambience, 0.5);
                assert_eq!(r.part.five.shadow_colour, [0.25, 0.5, 1.0]);
            }
            Light::Sun(_) => panic!("not a sun"),
        }
    }

    // : the cylindrical parallel light in the spot light's layout (one exponent), the
    // square spot light in the parallel light's (two): both are read, as the add-on's `cylinder` and `pyramid`
    #[test]
    fn a_cylinder_and_a_pyramid_are_read_as_their_records() {
        let (object, l) = light(&format!("\"kind\": \"cylinder\", \"exponents\": [1.5], {WORLD}")).expect("cylinder");
        assert_eq!(object.kind, ObjectKind::CylinderLight);
        match l {
            Light::Radial(r) => {
                assert_eq!(r.beam, Some(BeamRecord::Cylinder { exponent: 1.5, falloff_track: 0 }));
                assert_eq!(r.kind(), crate::scene::LightKind::Cylinder);
            }
            Light::Sun(_) => panic!("not a sun"),
        }
        let (object, l) = light(&format!("\"kind\": \"pyramid\", \"exponents\": [1.0, 3.0], {WORLD}")).expect("pyramid");
        assert_eq!(object.kind, ObjectKind::PyramidLight);
        match l {
            Light::Radial(r) => assert_eq!(r.beam, Some(BeamRecord::Square { exponents: [1.0, 3.0] })),
            Light::Sun(_) => panic!("not a sun"),
        }
        assert!(light(&format!("\"kind\": \"pyramid\", \"exponents\": [1.0], {WORLD}")).is_err());
        assert!(light("\"kind\": \"star\"").is_err());
    }

    // : a gradient is read with its keys in the order given, its range and offset in the light
    // part, the own colour as the part's, and the tint as the Diffuse A of a material (the third item)
    #[test]
    fn a_gradient_is_read_as_the_resource_scn_23_holds() {
        let (_, l, tint) = light_with_tint(&format!(
            "\"kind\": \"cone\", \"exponents\": [1.0], {WORLD}, \"radius\": 1.25, \"shadow_ambience\": 0.5, \
             \"shadow_colour\": [0.75, 0.62, 0.62], \"range\": 1.0, \"gradient\": [2.5, 0.75], \
             \"own_colour\": [0.15, 0.5, 0.5], \"tint_colour\": [0.25, 0.5, 0.9], \
             \"gradient_keys\": {{\"colour_keys\": [{{\"colour\": [0.5, 1.0, 0.15], \"place\": 0.0, \"midpoint\": 0.5}}, \
             {{\"colour\": [0.3, 0.5, 1.0], \"place\": 0.1, \"midpoint\": 0.25}}], \"value_keys\": [{{\"value\": 1.0, \
             \"place\": 0.0, \"midpoint\": 0.5}}], \"colour_switches\": 3, \"value_switches\": 1}}"
        ))
        .expect("gradient");
        assert_eq!(tint, Some([0.25, 0.5, 0.9]));
        let Light::Radial(r) = l else { panic!("not a sun") };
        assert_eq!(r.part.version, crate::scene_parts::LIGHT_VERSION_FIVE);
        assert_eq!(r.part.five.gradient, [2.5, 0.75]);
        assert_eq!(r.own_colour, [0.15, 0.5, 0.5]);
        assert_eq!(r.colour, [0.5, 0.5, 0.5]); // as it shines: the description's `colour`
        let g = r.gradient.expect("the gradient");
        assert_eq!((g.colour_switches, g.value_switches), (3, 1));
        assert_eq!(g.colour_keys.len(), 2);
        assert_eq!(g.colour_keys[1].colour, [0.3, 0.5, 1.0]);
        assert_eq!((g.colour_keys[1].place, g.colour_keys[1].midpoint), (0.1, 0.25));
        assert_eq!(g.value_keys.len(), 1);
        assert_eq!((g.value_keys[0].value, g.value_keys[0].place, g.value_keys[0].midpoint), (1.0, 0.0, 0.5));
        // a gel, or a gradient without its range, is refused by name
        let e = light_with_tint("\"kind\": \"radial\", \"gel\": true, \"gradient\": [1.0, 0.0], \"gradient_keys\": {}")
            .err()
            .expect("a gel is refused");
        assert!(e.0.contains("gel"), "{}", e.0);
        assert!(light_with_tint("\"kind\": \"radial\", \"gradient_keys\": {}").is_err());
    }

    // (the texture form the add-on writes: outputs a word, noise `type`,
    // `octaves`, `dimension`, `angles`, `frequencies`, colours as six hexadecimal digits, the colour
    // space a word) and.
    // The settings are those of, case texture-001.
    #[test]
    fn a_texture_of_the_scn15_form_is_read() {
        let text = "{\"size\": [8, 6], \"render\": {\"version\": \"5.0\", \"units\": 1.0}, \"sky\": {}, \
            \"sun\": {\"direction\": [0.3, 0.6, -0.7], \"colour\": [1, 1, 1]}, \
            \"camera\": {\"position\": [0, 0, -10], \"forward\": [0, 0, 1], \"up\": [0, 1, 0], \"frame_angle\": 1.0}, \
            \"textures\": [{\"name\": \"M:0\", \"kind\": \"procedural\", \"texture\": {\"outputs\": 2, \"blend\": [0, 0], \
              \"phase\": {\"noise\": {\"type\": 0}}, \"filter\": {\"number\": 8, \"n\": 15, \"a\": 1.0, \"b\": 4.0}, \
              \"colours\": {\"mode\": 12, \"space\": 64, \"a\": \"7beaa6\", \"b\": \"153d02\", \"c\": \"347c83\"}, \
              \"components\": [{\"outputs\": 5, \"noise\": {\"type\": 9, \"method\": 1, \"octaves\": 5, \"dimension\": 1}, \
                \"angles\": [0.0, 0.0], \"frequencies\": [13.0, 28.0, 28.0], \
                \"phase\": {\"amount\": 11.0, \"noise\": {\"type\": 26, \"method\": 0, \"octaves\": 4, \"dimension\": 3}, \
                  \"angles\": [0.0, 0.0], \"frequencies\": [23.0, 35.0, 25.0]}, \
                \"filter\": {\"number\": 0}, \
                \"colours\": {\"mode\": 12, \"space\": 0, \"a\": \"4d8b51\", \"b\": \"4d1528\", \"c\": \"daac57\"}}]}, \
              \"mapping\": {\"mode\": 9, \"switches\": 32, \"scale\": [0.5, 1.0, 1.0], \"offset\": [0.0, -1.0, 2.0], \
                \"turn\": [0.0, 0.0, 1.0, 0.0]}}], \
            \"materials\": {\"M\": {\"flags\": 0, \"channels\": {\"diffusion\": {\"switches\": 268435456, \"a\": 0.5, \
              \"b\": 1.0, \"alpha\": 1.0, \"a_tex\": \"M:0\"}}}}, \"objects\": []}";
        let d = describe(text).expect("a textured description is read");
        let t = &d.scene.textures[0];
        assert_eq!(t.mapping.mode, 9);
        assert_eq!(t.mapping.switches, 32);
        assert_eq!(t.mapping.turn, [0.0, 0.0, 1.0, 0.0]);
        match &t.body {
            TextureBody::Procedural(p) => {
                assert_eq!(p.settings.colours.a, [0x7b, 0xea, 0xa6]);
                assert_eq!(p.settings.colours.space, ColourSpace::Hls);
                assert_eq!(p.settings.filter.number, 8);
                let c = &p.settings.components[0];
                assert!(c.outputs.value && c.outputs.bump && !c.outputs.colour); // 5 = 0x1 + 0x4
                assert_eq!(c.noise.type_number, 9);
                assert_eq!(c.noise.octaves, 5);
                assert_eq!(c.space.frequencies, [13.0, 28.0, 28.0]);
                assert_eq!(c.phase.noise.type_number, 26);
                assert_eq!(c.colours.c, [0xda, 0xac, 0x57]);
            }
            _ => panic!("not procedural"),
        }
        let m = d.scene.materials.iter().find(|m| m.name == b"M").expect("material");
        assert!(m.textures.colours.iter().chain(m.textures.values.iter()).any(|r| r[0].is_some()));
        let nine = text.replace("\"angles\": [0.0, 0.0], \"frequencies\": [13.0", "\"map\": [1, 0, 0, 0, 1, 0, 0, 0, 1], \"frequencies\": [13.0");
        let e = describe(&nine).err().expect("a nine-number space is refused");
        assert!(e.0.contains("map"), "{}", e.0);
    }

    // : the own part of a terrain, plain packing, the entry's world as its transform
    #[test]
    fn a_terrain_is_read_as_its_own_part() {
        let d = with_object(
            "{\"name\": \"t\", \"shape\": \"terrain\", \"world\": [1, 0, 0, 0, 1, 0, 0, 0, 1, 4, 5, 6], \
             \"terrain\": {\"size\": 2, \"heights\": [0, 100, 65535, 7], \"lower\": -1.5, \"upper\": 2.5, \
             \"mirrored\": true, \"smooth\": false, \"faces\": 3, \"solid_faces\": 5}}",
        )
        .expect("a terrain is read");
        let o = &d.scene.objects[0];
        assert_eq!((o.kind, o.body), (ObjectKind::HeightField, Body::Terrain(0)));
        assert_eq!(o.world.map(|w| w.translation), Some([4.0, 5.0, 6.0]));
        let t = &d.scene.terrains[0];
        assert_eq!(t.object, 0);
        assert_eq!(t.part.packing, Packing::Plain);
        assert!(t.part.mirrored && !t.part.smooth);
        assert_eq!(t.part.stored_bounds, [-1.5, 2.5]);
        assert_eq!((t.part.closing_faces, t.part.stored_solid_faces), (3, 5));
        // a height out of 16 bits, or a count that is not size squared, is refused
        let bad = |heights: &str| {
            with_object(&format!(
                "{{\"name\": \"t\", \"shape\": \"terrain\", \"matrix\": [[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], [0, 0, 0, 1]], \
                 \"terrain\": {{\"size\": 2, \"heights\": {heights}}}}}"
            ))
        };
        assert!(bad("[0, 1, 2, 65536]").is_err());
        assert!(bad("[0, 1, 2]").is_err());
        assert!(bad("[0, 1, 2, 3]").is_ok());
    }

    // : the members are linked to their group with their role bits, the
    // root objects first, then each group's members after it
    #[test]
    fn a_groups_members_are_linked_with_their_roles() {
        let ident = "\"world\": [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]";
        let moved = "\"world\": [1, 0, 0, 0, 1, 0, 0, 0, 1, 1, 2, 3]";
        let d = with_object(&format!(
            "{{\"name\": \"m1\", \"shape\": \"sphere\", {moved}}}, \
             {{\"name\": \"g\", \"shape\": \"group\", {ident}, \"members\": {{\
               \"m1\": {{\"role\": \"NEGATIVE\", \"cut_shows_positive\": true}}, \
               \"m2\": {{\"role\": \"INTERSECT\"}}, \"m3\": {{\"role\": \"NEUTRAL\"}}}}}}, \
             {{\"name\": \"m2\", \"shape\": \"cube\", {ident}}}, \
             {{\"name\": \"m3\", \"shape\": \"cone\", {ident}}}, \
             {{\"name\": \"loose\", \"shape\": \"disc\", {ident}}}"
        ))
        .expect("a group is read");
        let names: Vec<String> = d.scene.objects.iter().map(|o| String::from_utf8_lossy(&o.name).into_owned()).collect();
        let at = |n: &str| names.iter().position(|x| x == n).expect(n);
        let g = at("g");
        assert_eq!(d.scene.objects[g].body, Body::Group);
        assert!(d.scene.objects[at("loose")].linked_to.is_none());
        for (n, flags) in [("m1", 0x90u32), ("m2", 0x20), ("m3", 0)] {
            let o = &d.scene.objects[at(n)];
            assert_eq!(o.linked_to, Some(g), "{n}");
            let p = o.placement.expect("placement");
            assert_eq!(p.second_flags, flags, "{n}");
            assert!(o.world.is_some(), "{n}: the linked arithmetic gives it its world");
        }
        assert_eq!(d.scene.objects[at("m1")].placement.unwrap().second.translation, [1.0, 2.0, 3.0]);
        assert_eq!(d.scene.objects[at("m1")].world.map(|w| w.translation), Some([1.0, 2.0, 3.0]));
        assert!(at("m1") > at("loose") && at("m2") > at("loose"));
        // a role that is not one is refused
        assert!(with_object(&format!(
            "{{\"name\": \"x\", \"shape\": \"cube\", {ident}}}, \
             {{\"name\": \"h\", \"shape\": \"group\", {ident}, \"members\": {{\"x\": {{\"role\": \"BOTH\"}}}}}}"
        ))
        .is_err());
    }

    fn with_textures(textures: &str) -> Rd<Described> {
        describe(&format!(
            "{{\"size\": [8, 6], \"render\": {{\"version\": \"5.0\", \"units\": 1.0}}, \"sky\": {{}}, \
            \"sun\": {{\"direction\": [0.3, 0.6, -0.7], \"colour\": [1, 1, 1]}}, \
            \"camera\": {{\"position\": [0, 0, -10], \"forward\": [0, 0, 1], \"up\": [0, 1, 0], \"frame_angle\": 1.0}}, \
            \"textures\": [{textures}], \
            \"materials\": {{\"M\": {{\"flags\": 0, \"channels\": {{\"diffusion\": {{\"switches\": 268435456, \"a\": 0.5, \
              \"b\": 1.0, \"alpha\": 1.0, \"a_tex\": \"P\"}}}}}}}}, \"objects\": []}}"
        ))
    }

    // : a texture of kind `picture` holds the pixels
    // alpha first, the top row first, and the material's channel names it
    #[test]
    fn a_picture_texture_is_read_with_its_picture() {
        let tex = |pixels: &str| {
            format!(
                "{{\"name\": \"P\", \"kind\": \"picture\", \"picture\": {{\"width\": 2, \"height\": 1, \"pixels\": [{pixels}]}}, \
                 \"mapping\": {{\"mode\": 2, \"switches\": 0, \"scale\": [1.0, 1.0, 1.0], \"offset\": [0.0, 0.0, 0.0], \
                 \"turn\": [0.0, 0.0, 0.0, 1.0]}}}}"
            )
        };
        let d = with_textures(&tex("\"ff102030\", \"80ffffff\"")).expect("a picture texture is read");
        assert_eq!(d.scene.textures.len(), 1);
        assert_eq!(d.scene.textures[0].class, TextureClass::Picture);
        assert_eq!(d.scene.pictures.len(), 1);
        let m = d.scene.materials.iter().find(|m| m.name == b"M").expect("material");
        assert!(m.textures.colours.iter().chain(m.textures.values.iter()).any(|r| r[0].is_some()));
        // a pixel that is not eight hexadecimal digits, or too few pixels, is refused
        assert!(with_textures(&tex("\"ff102030\", \"80ffff\"")).is_err());
        assert!(with_textures(&tex("\"ff102030\"")).is_err());
    }

    // Errors: a description with a tree and no substrate tables is refused
    #[test]
    fn a_tree_without_tables_is_refused() {
        let text = "{\"size\": [8, 6], \"render\": {\"version\": \"5.0\", \"units\": 1.0}, \"sky\": {}, \
            \"sun\": {\"direction\": [0.3, 0.6, -0.7], \"colour\": [1, 1, 1]}, \
            \"camera\": {\"position\": [0, 0, -10], \"forward\": [0, 0, 1], \"up\": [0, 1, 0], \"frame_angle\": 1.0}, \
            \"objects\": [], \"trees\": [{\"name\": \"T\", \"world\": [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0], \"tree\": {}}]}";
        assert!(describe(text).is_err());
    }

    // (Mix Materials feed): a material's `mix` gives the second channels
    // of the channels whose amount is not 0, and none otherwise.
    #[test]
    fn a_material_mix_gives_second_channels() {
        let text = "{\"channels\": {\"diffuse_color\": [0.9, 0.1, 0.1]}, \"mix\": {\"amounts\": [0.5, 0, 0, 0, 0, 0.25, 0, 0, 0, 0, 0, 0, 0], \
            \"colors\": [[0.1, 0.1, 0.9], [0,0,0], [0,0,0], [0,0,0], [0,0,0]], \"values\": [0.8, 0, 0, 0, 0, 0, 0, 0]}}";
        let v = crate::cases::json::parse(text).expect("json");
        let m = material_of("m", &v, &TexIndex::new()).expect("material");
        let c = m.second.colours[0].as_ref().expect("diffuse mixed");
        assert_eq!((c.amount, c.channel.a), (0.5, [0.1, 0.1, 0.9]));
        assert!(m.second.colours[1].is_none());
        assert_eq!(m.second.values[0].as_ref().map(|s| (s.amount, s.channel.a)), Some((0.25, 0.8)));
        assert!(m.second.values[1].is_none());
    }

    // ,: an import keeps a document's three placement flag words on the entry; an
    // object, a tree and a light carry them (a light keeps its switches in the second word besides), and a flag
    // that is not a word is refused
    #[test]
    fn the_placement_flag_words_are_carried() {
        let ident = "\"world\": [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]";
        let d = with_object(&format!(
            "{{\"name\": \"o\", \"shape\": \"sphere\", {ident}, \"placement_flags\": [5, 6, 7]}}"
        ))
        .expect("flags");
        let p = d.scene.objects[0].placement.expect("placement");
        assert_eq!((p.first_flags, p.second_flags, p.third_flags), (5, 6, 7));
        assert!(with_object(&format!(
            "{{\"name\": \"o\", \"shape\": \"sphere\", {ident}, \"placement_flags\": [5, -1, 7]}}"
        ))
        .is_err());
        // a light: the switches join the second word
        let (object, _) = light("\"kind\": \"radial\", \"placement_flags\": [1, 2, 3]").expect("light");
        let p = object.placement.expect("placement");
        assert_eq!((p.first_flags, p.second_flags, p.third_flags), (1, 2 | 256, 3));
    }

    // : a radial light's object is the sphere of the entry's `world` where one is given, else
    // the unit sphere at its position
    #[test]
    fn a_radial_light_takes_its_worlds_size() {
        let (object, _) = light(&format!("\"kind\": \"radial\", {WORLD}")).expect("sized");
        assert_eq!(object.world.expect("world").rows, [[2.0, 0.0, 0.0], [0.0, -2.0, 0.0], [0.0, 0.0, -0.5]]);
        let (object, _) = light("\"kind\": \"radial\"").expect("unsized");
        let w = object.world.expect("world");
        assert_eq!(w.rows, IDENTITY_ROWS);
        assert_eq!(w.translation, [-26.0, -4.0, 28.0]);
    }

    // : the four roles' bits, and 0x80 for a negative member whose cut shows the positive
    #[test]
    fn the_role_words_are_the_groups_bits() {
        let word = |t: &str| role_word_of("m", &crate::cases::json::parse(t).expect("json"));
        assert_eq!(word("{}").unwrap(), 0);
        assert_eq!(word("{\"role\": \"POSITIVE\"}").unwrap(), 0x8);
        assert_eq!(word("{\"role\": \"NEGATIVE\"}").unwrap(), 0x10);
        assert_eq!(word("{\"role\": \"NEGATIVE\", \"cut_shows_positive\": true}").unwrap(), 0x90);
        assert_eq!(word("{\"role\": \"POSITIVE\", \"cut_shows_positive\": true}").unwrap(), 0x8);
        assert_eq!(word("{\"role\": \"INTERSECT\"}").unwrap(), 0x20);
        assert!(word("{\"role\": \"x\"}").is_err());
    }

    // : the render settings a reader reports (`settings`) carry the depths and the structure code
    // from Bryce 5's part, and for a document without that part only the anti-aliasing and spatial options
    #[test]
    fn the_readers_settings_name_the_five_part() {
        let text = |render: &str| {
            format!(
                "{{\"size\": [8, 6], \"render\": {{\"version\": \"5.0\", \"units\": 1.0{render}}}, \"sky\": {{}}, \
                 \"sun\": {{\"direction\": [0.3, 0.6, -0.7], \"colour\": [1, 1, 1]}}, \
                 \"camera\": {{\"position\": [0, 0, -10], \"forward\": [0, 0, 1], \"up\": [0, 1, 0], \"frame_angle\": 1.0}}, \
                 \"objects\": []}}"
            )
        };
        let five = describe(&text(", \"ray_depth\": 4, \"tir_depth\": 3, \"structure\": \"grid\"")).unwrap();
        let out = crate::scene_export::settings_json(&five.scene).write();
        for want in ["\"ray_depth\":4", "\"tir_depth\":3", "\"structure\":\"grid\"", "\"antialias\"", "\"spatial\""] {
            assert!(out.replace(' ', "").contains(want), "{want} in {out}");
        }
        let three = describe(&text("").replace("\"5.0\"", "\"3.0\"")).unwrap();
        let out = crate::scene_export::settings_json(&three.scene).write();
        assert!(out.contains("antialias") && !out.contains("ray_depth"), "{out}");
    }

    // : the sun lamp's colour is the light part's, apart from the sun's own colour (the wash reads it)
    #[test]
    fn the_suns_light_colour_is_kept_apart() {
        let text = |sun: &str| {
            format!(
                "{{\"size\": [8, 6], \"render\": {{\"version\": \"5.0\", \"units\": 1.0}}, \"sky\": {{}}, \
                 \"sun\": {{\"direction\": [0.3, 0.6, -0.7], \"colour\": [0, 0, 0]{sun}}}, \
                 \"camera\": {{\"position\": [0, 0, -10], \"forward\": [0, 0, 1], \"up\": [0, 1, 0], \"frame_angle\": 1.0}}, \
                 \"objects\": []}}"
            )
        };
        let apart = describe(&text(", \"light_colour\": [1, 0.5, 0.25]")).unwrap();
        assert_eq!(apart.scene.sun().light.colour, [1.0, 0.5, 0.25]);
        assert_eq!(apart.scene.sun().colour, [0.0; 3]);
        let none = describe(&text("")).unwrap(); // without one, the part has the sun's colour as before
        assert_eq!(none.scene.sun().light.colour, [0.0; 3]);
    }
}
