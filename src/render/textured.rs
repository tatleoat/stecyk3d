// SPDX-License-Identifier: MIT
//! Textured materials in the render: a material's textures evaluated at the point their
//! mapping gives, its channels from their results, and a gel's Diffuse colour at the gel
//! point of the light it colours.
//!
//! (the bump, through `evaluate_channels`), (the texture point, the frame, the gradient
//! the channels read; the object's own box; mirroring), (the
//! surface context from the texture point and the shading normal), (Gels: the gel
//! point is the hit, the light's own object the object), (a gel's hit, its colour as
//! the binary32 product),
//! (Textured materials in the render),
//! (Picture textures: in the render);

use std::sync::Arc;

use crate::geom::Vec3;
use crate::geometry::{Placement, Shape};
use crate::scene::Scene;
use crate::scene_textures::{TextureBody, TextureClass};
use crate::textures::picture::Picture;
use crate::shading::OpenItem;
use crate::shading::channels::{
    COLOUR_CHANNELS, ChannelHit, ChannelSource, MaterialAtHit, MaterialChannels, TEXTURES_ON, TextureAtHit,
    Mixed, SecondMaterial, VALUE_CHANNELS, evaluate_mixed_channels,
};
use crate::shading::gel::{GelHit, GelKind, GelOrders, gel_hit, gel_point, light_space, normal as gel_normal, world as gel_world};
use crate::textures::mapping::{
    MapHit, MapObject, MapSettings, MappingRefusal, SWITCH_CENTRED, SWITCH_COLOUR_TAKEN, SWITCH_ONE_PIXEL, SWITCH_SCALE_PICT, SWITCH_SYMMETRIC,
    SWITCH_TILE, SWITCH_VALUE_TAKEN, frame_gradient,
    map_point_tabled, mirrored,
};
use crate::textures::lattice::Lattice;
use crate::textures::Kept;
use crate::textures::orders::TextureOrders;
use crate::textures::settings::{SurfaceContext, TextureSettings};
use crate::textures::tables::TextureTables;
use crate::version::RenderAs;

/// What these rules leave Open, or refuse as the implementer's choice.
pub mod open {
    use crate::shading::OpenItem;

    const fn item(entry: &'static str, subject: &'static str, question: &'static str) -> OpenItem {
        OpenItem { entry, subject, question }
    }

    /// Not reachable from a document: the reader refuses a picture texture without its picture.
    pub const PICTURE_MISSING: OpenItem =
        item("SCN-19", "a picture texture whose picture the scene does not hold", "");
    pub const PICTURE_POINT: OpenItem = item(
        "TX-4.3",
        "a picture read at a point whose place in the picture is not a finite number below 2^31 in size",
        "",
    );
    pub const TEXTURE_OPEN: OpenItem =
        item("TX-1.1", "a material's texture reaches a stage the texture entries leave Open", "");
    pub const TEXTURE_EVALUATION: OpenItem = item(
        "TX-1.1",
        "a material's texture could not be evaluated at a hit (a table the substrate lacks, SUB-8, or a result the program refuses)",
        "",
    );
    pub const MAPPING_MODE: OpenItem = item(
        "TX-4.2",
        "a mapping mode outside 0 to 16 (11 to 16 below 5.0)",
        "",
    );
    pub const MAPPING_SWITCHES: OpenItem =
        item("SH-2.1", "a texture whose mapping switches are other than 0 and 0x20", "");
    pub const SCENE_TOP: OpenItem = item(
        "SH-2.1",
        "a channel mixed by height (switch bit 0x2): the box of the scene's top is Open",
        "",
    );
    pub const TEXTURE_OUTPUTS: OpenItem = item(
        "TX-1.2",
        "a material's channel reads an output its texture is not asked for, or a colour no component makes (what it carries from earlier evaluations)",
        "",
    );
    pub const TEXTURED_LIGHT_OBJECT: OpenItem = item(
        "SH-2.1",
        "a ray meets a light's own object whose material is a gel (how it is shaded is Open)",
        "",
    );
    pub const GEL_LIT_AIR: OpenItem =
        item("SK-2.3", "the light in the air with a gel (which evaluations of the gel it makes)", "");
}

/// The own box of a shape.
pub fn own_box(shape: &Shape) -> (Vec3, Vec3) {
    let b = |l: Vec3, h: Vec3| (l, h);
    match *shape {
        Shape::Sphere | Shape::Cylinder | Shape::Cone => b([-1.0; 3], [1.0; 3]),
        Shape::Cube | Shape::Pyramid => b([-0.5; 3], [0.5; 3]),
        Shape::Plane => b([-10.0, 0.0, -10.0], [10.0, 0.0, 10.0]),
        Shape::Slab => b([-10.0, -1.0, -10.0], [10.0, 1.0, 10.0]),
        Shape::Square | Shape::Disc => b([-1.0, 0.0, -1.0], [1.0, 0.0, 1.0]),
        Shape::Torus { tube, ring } => {
            let e = ring + tube;
            b([-e, -tube, -e], [e, tube, e])
        }
        // Unreachable: a mesh comes only from the Blender description, whose materials are
        // plain , and's box is read only for a textured material.
        Shape::Mesh(_) => b([-1.0; 3], [1.0; 3]),
        // Unread: a document mesh's box is `StoredMesh::own_box` , which
        // the render asks instead. (Meshes)
        Shape::StoredMesh(_) => b([f64::NAN; 3], [f64::NAN; 3]),
        // A terrain's own box needs its grid's side: `Relief::own_box` , which the
        // render asks instead. These numbers reach nothing.
        Shape::Terrain(_) | Shape::Path(_) => b([f64::NAN; 3], [f64::NAN; 3]),
        // A group's hit reports the member whose material shows , whose own box is
        // read. These numbers reach nothing. (Combined groups)
        Shape::Group(_) => b([f64::NAN; 3], [f64::NAN; 3]),
        // A tree's coordinates come with its hit ; these numbers reach nothing.
        Shape::Tree { .. } | Shape::TreeLeaves { .. } => b([f64::NAN; 3], [f64::NAN; 3]),
        Shape::Metaball(_) => b([f64::NAN; 3], [f64::NAN; 3]),
        // : the stand-in a tree's wood is reported for has the cube -1 to 1
        Shape::MetaSet => b([-1.0; 3], [1.0; 3]),
        // : a light's object is its plain shape (the endless cone: its old box; Open).
        Shape::Light(l) => own_box(&l.plain()),
    }
}

/// A channel's mix by height.
const MIX_HEIGHT: u32 = 0x2;

/// A radial light's own object's box (the gels of).
const LIGHT_BOX: (Vec3, Vec3) = ([-1.0; 3], [1.0; 3]);

/// The colour a texture holds : a cell that may sit in structures shared between threads
/// (relaxed atomics: the multithreaded render never lets two threads use one holder, because a
/// document that holds a colour renders serially).
#[derive(Debug, Default)]
struct Held(std::sync::atomic::AtomicU32);

impl Held {
    fn new(c: [u8; 3]) -> Held {
        Held(std::sync::atomic::AtomicU32::new(u32::from_le_bytes([c[0], c[1], c[2], 0])))
    }
    fn get(&self) -> [u8; 3] {
        let b = self.0.load(std::sync::atomic::Ordering::Relaxed).to_le_bytes();
        [b[0], b[1], b[2]]
    }
    fn set(&self, c: [u8; 3]) {
        self.0.store(u32::from_le_bytes([c[0], c[1], c[2], 0]), std::sync::atomic::Ordering::Relaxed);
    }
}
impl Clone for Held {
    fn clone(&self) -> Held {
        Held::new(self.get())
    }
}
impl PartialEq for Held {
    fn eq(&self, o: &Held) -> bool {
        self.get() == o.get()
    }
}

/// What a texture evaluates. (Picture textures: in the render)
#[derive(Clone, Debug, PartialEq)]
enum TextureSource {
    /// The settings, the result as saved, and the colour the texture holds now .
    Procedural(TextureSettings, Kept, Held),
    /// The picture, shared with `Scene::pictures`.
    Picture(Arc<Picture>),
}

/// One texture a material names, ready to evaluate.
#[derive(Clone, Debug, PartialEq)]
struct MaterialTexture {
    source: TextureSource,
    map: MapSettings,
    /// The material's Bump Height channel names it as A or B. (Which textures make
    /// a bump)
    bump_on: bool,
}

/// What the material's channels read of a texture, by the references that name it: the colour
/// (a colour channel's A or B), the value (a value channel's A or B, or any alpha), the
/// gradient (Bump Height's A or B). (Textured materials in the render)
#[derive(Clone, Copy, Debug, Default)]
struct ReadFor {
    colour: bool,
    value: bool,
    bump: bool,
}

/// The index of Bump Height among the value channels (channel 10).
const BUMP_HEIGHT: usize = 5;

/// Whether two texture resources give the same copy. (Which references share one
/// evaluation; Which stored words two copies are compared in): the same resource; or both
/// procedural with the same mapping mode and switches, scale and offset (each number equal as a
/// number) and the same words of the record part (`CopyWords`: the fields, the words marks
/// not used that decide, the colour words with their top bytes, the result as saved; numbers as
/// singles; as 3.0 every record read as version 1); or both picture textures with the same mapping
/// and the same picture. Not compared: the mapping's turn and track identities, the texture's
/// track, the record's version word, the name.
fn same_copy(a: &crate::scene_textures::Texture, b: &crate::scene_textures::Texture, version: RenderAs) -> bool {
    if std::ptr::eq(a, b) {
        return true;
    }
    let (ma, mb) = (&a.mapping, &b.mapping);
    if ma.mode != mb.mode || ma.switches != mb.switches || ma.scale != mb.scale || ma.offset != mb.offset {
        return false;
    }
    match (&a.body, &b.body) {
        (TextureBody::Procedural(x), TextureBody::Procedural(y)) => match (&x.copy_words, &y.copy_words) {
            (Some(p), Some(q)) if version == RenderAs::Bryce30 => p.whole == q.whole,
            (Some(p), Some(q)) => p.stored == q.stored,
            // a texture of a description has no record: its settings and result decide
            _ => {
                x.settings == y.settings
                    && x.saved_result.colour == y.saved_result.colour
                    && x.saved_result.numbers.iter().zip(&y.saved_result.numbers).all(|(p, q)| p.to_bits() == q.to_bits())
            }
        },
        (TextureBody::Picture(x), TextureBody::Picture(y)) => x.index == y.index,
        _ => false,
    }
}

/// The entry of `list` whose copy the reference `r` reads: the first same copy; for a reference
/// of the other material (`second` differs from the entry's origin) one with the same turn too;
/// in the second taking (`Taking::Second`) not one whose bump is off where `r`'s is on.
/// (material-mix-pictures mt-e2); (Which references share one
/// evaluation; Tracks applied as a document is opened),
fn find_copy(
    scene: &Scene,
    list: &[usize],
    from_second: &[bool],
    r: usize,
    second: bool,
    version: RenderAs,
    taking: &Taking,
) -> Option<usize> {
    list.iter().zip(from_second).position(|(&x, &s)| {
        same_copy(&scene.textures[x], &scene.textures[r], version)
            && (s == second || scene.textures[x].mapping.turn == scene.textures[r].mapping.turn)
            && taking.compatible(scene, x, r)
    })
}

/// Which taking of a material's references is made. (Which references share one
/// evaluation; Tracks applied as a document is opened)
enum Taking {
    /// As the document is read, before any track: copies are the same by their words alone.
    First,
    /// Once the tracks are applied, for a material with a track of its own: a copy's bump being
    /// on is compared too. `evaluated` are the copies the first taking evaluated (their bump is
    /// off but for those a Bump Height reference names itself) and `bump_named` the resources
    /// the material's Bump Height channel names as its A or B.
    Second { evaluated: Vec<usize>, bump_named: Vec<usize> },
}

impl Taking {
    /// Whether the reference `r` may read the evaluated copy `e`: in the second taking not where `e`
    /// has its bump off and `r`'s copy makes one (a component whose outputs include the bump).
    fn compatible(&self, scene: &Scene, e: usize, r: usize) -> bool {
        match self {
            Taking::First => true,
            Taking::Second { evaluated, bump_named } => {
                let off = evaluated.contains(&e) && !bump_named.contains(&e);
                let bumps = scene.textures[r]
                    .procedural()
                    .is_some_and(|p| p.settings.components.iter().any(|c| c.outputs.bump));
                !(off && bumps)
            }
        }
    }
}

/// The copies a material's own references evaluate in the first taking, in order: each reference
/// of channels 0 to 12 (A, B, alpha) reads the first same copy or is a new one.
fn first_taking(scene: &Scene, material: &crate::scene::Material, version: RenderAs) -> Vec<usize> {
    let mut list: Vec<usize> = Vec::new();
    let mut from_second: Vec<bool> = Vec::new();
    let slots = material.textures.colours.iter().chain(&material.textures.values).flatten().flatten();
    for &r in slots {
        if find_copy(scene, &list, &from_second, r, false, version, &Taking::First).is_none() {
            list.push(r);
            from_second.push(false);
        }
    }
    list
}

/// A material whose channels are evaluated: its channels with references into its own list
/// of textures. (Textured materials in the render)
#[derive(Clone, Debug, PartialEq)]
pub struct TexturedMaterial {
    channels: MaterialChannels,
    textures: Vec<MaterialTexture>,
    /// The refraction value η, the material's own (not a channel).
    refraction: f32,
    /// The second material a channel track makes between keys ; empty for none.
    second: SecondMaterial,
    /// Y, the scene's top for the height ratio; not a number where no channel mixes by height.
    top: f64,
}

impl TexturedMaterial {
    /// The material `m` of the scene, its textures checked.
    pub fn new(scene: &Scene, m: usize, version: RenderAs) -> Result<TexturedMaterial, OpenItem> {
        let material = &scene.materials[m];
        let part = &material.part;
        let mut list: Vec<usize> = Vec::new();
        // : a reference whose copy is the same
        // as the copy of an earlier reference reads that copy's results; the references are taken
        // in channel order, within a channel A, B, alpha. A material has copies of its own.
        // (material-mix-pictures mt-e2): a reference of the second material
        // shares a copy with an earlier reference of the other material only where the turn is
        // the same too (the original's picture of mt-e2 shows a turned copy of the second
        // material drawn turned, where the turn is not compared within one material).
        let mut from_second: Vec<bool> = Vec::new();
        // : where the document's tracks are
        // applied as it is opened (as 3.0 always, as 3.1 and 5.0 where the animation switch is not
        // 0) a material with a track of its own has its references taken a second time.
        let has_track = scene.animation.material(m).iter().any(|&slot| scene.animation.keyed(slot).is_some());
        let applied = crate::animation::Opening::new(version, scene.opening_inputs()).is_ok_and(|o| o.applies_tracks);
        let taking = if has_track && applied {
            let named = |slots: &[Option<usize>; 3]| slots[..2].iter().flatten().copied().collect::<Vec<usize>>();
            let mut bump_named = named(&material.textures.values[BUMP_HEIGHT]);
            if let Some(s) = &material.second.values[BUMP_HEIGHT] {
                bump_named.extend(named(&s.textures));
            }
            Taking::Second { evaluated: first_taking(scene, material, version), bump_named }
        } else {
            Taking::First
        };
        let mut local = |r: Option<usize>, second: bool| -> Option<usize> {
            let r = r?;
            Some(match find_copy(scene, &list, &from_second, r, second, version, &taking) {
                Some(k) => k,
                None => {
                    list.push(r);
                    from_second.push(second);
                    list.len() - 1
                }
            })
        };
        let mut refs = |slots: [Option<usize>; 3], second: bool| slots.map(|r| local(r, second));
        let colours: [ChannelSource<[f32; 3]>; COLOUR_CHANNELS] = std::array::from_fn(|i| {
            let c = &part.colours[i];
            ChannelSource { switches: c.flags, a: c.a, b: c.b, alpha: c.alpha, textures: [None; 3] }
        });
        let values: [ChannelSource<f32>; VALUE_CHANNELS] = std::array::from_fn(|i| {
            let c = &part.values[i];
            ChannelSource { switches: c.flags, a: c.a, b: c.b, alpha: c.alpha, textures: [None; 3] }
        });
        let mut channels = MaterialChannels { flags: part.flags, colours, values };
        for i in 0..COLOUR_CHANNELS {
            channels.colours[i].textures = refs(material.textures.colours[i], false);
        }
        for i in 0..VALUE_CHANNELS {
            channels.values[i].textures = refs(material.textures.values[i], false);
        }
        // (A channel's sources: the height ratio is y / Y, Y the largest y of the box
        // that holds the scene,'s scene boxes; Open: which objects it holds beyond them)
        let words = part.channel_words();
        let mixes_height = words.iter().any(|&(w, _)| w & MIX_HEIGHT != 0)
            || material.second.colours.iter().any(|c| c.as_ref().is_some_and(|s| s.channel.flags & MIX_HEIGHT != 0))
            || material.second.values.iter().any(|c| c.as_ref().is_some_and(|s| s.channel.flags & MIX_HEIGHT != 0));
        let top = if mixes_height {
            super::modes::scene_boxes(scene, version).map_err(|_| open::SCENE_TOP)?.height.1
        } else {
            f64::NAN
        };
        // ,: the second channels, their references into
        // the same list of textures
        let mut second = SecondMaterial::default();
        // animation : the second material has copies of its own: its reference
        // names the listed copy of that very resource, or the first same copy among the second
        // material's own, else a new one (a turned copy is then its own, as mt-e2's pictures show).
        let first_len = list.len();
        let mut refs2 = |slots: [Option<usize>; 3]| -> ([Option<usize>; 3], [Option<usize>; 3]) {
            let mut ids = [None; 3];
            let mut places = [None; 3];
            for (j, r) in slots.iter().enumerate() {
                let Some(r) = *r else { continue };
                let place = match list.iter().position(|&x| x == r) {
                    Some(k) => k,
                    None => match list[first_len..].iter().position(|&x| same_copy(&scene.textures[x], &scene.textures[r], version)) {
                        Some(k) => first_len + k,
                        None => {
                            list.push(r);
                            list.len() - 1
                        }
                    },
                };
                places[j] = Some(place);
                ids[j] = Some(list[place]);
            }
            (places, ids)
        };
        // The second channels' (switch word, scene indices of the copies read) in channel order.
        let mut second_sources: Vec<Option<(u32, [Option<usize>; 3])>> = vec![None; COLOUR_CHANNELS + VALUE_CHANNELS];
        for i in 0..COLOUR_CHANNELS {
            if let Some(s) = &material.second.colours[i] {
                let (textures, ids) = refs2(s.textures);
                second_sources[i] = Some((s.channel.flags, ids));
                let channel = ChannelSource { switches: s.channel.flags, a: s.channel.a, b: s.channel.b, alpha: s.channel.alpha, textures };
                second.colours[i] = Some(Mixed { channel, amount: s.amount });
            }
        }
        for i in 0..VALUE_CHANNELS {
            if let Some(s) = &material.second.values[i] {
                let (textures, ids) = refs2(s.textures);
                second_sources[COLOUR_CHANNELS + i] = Some((s.channel.flags, ids));
                let channel = ChannelSource { switches: s.channel.flags, a: s.channel.a, b: s.channel.b, alpha: s.channel.alpha, textures };
                second.values[i] = Some(Mixed { channel, amount: s.amount });
            }
        }
        let named = &material.textures;
        let mut textures = Vec::with_capacity(list.len());
        for &t in &list {
            // The copy a reference reads: the first same copy (`list` holds those, in order).
            let canonical = |r: Option<usize>| -> Option<usize> {
                let r = r?;
                list[..first_len]
                    .iter()
                    .copied()
                    .find(|&x| same_copy(&scene.textures[x], &scene.textures[r], version) && taking.compatible(scene, x, r))
            };
            let in_a_or_b = |slots: &[Option<usize>; 3]| slots[..2].iter().any(|s| canonical(*s) == Some(t));
            let in_a_or_b_2 = |slots: &[Option<usize>; 3]| slots[..2].contains(&Some(t));
            // The bump: only the copy the Bump Height channel itself names (its own resource) makes
            // one; a reference whose copy is the same as an earlier reference's reads that copy,
            // whose bump is off.
            let names_t = |slots: &[Option<usize>; 3]| slots[..2].contains(&Some(t));
            // A channel reads its textures only where it is taken (its switch word not
            // 0, or as 3.0 its A a texture); taking every such channel reads at least as much.
            let all: Vec<&[Option<usize>; 3]> = named.colours.iter().chain(&named.values).collect();
            let taken_by = |word: u32, slots: &[Option<usize>; 3]| word != 0 || (version == RenderAs::Bryce30 && slots[0].is_some());
            let taken = |i: usize| taken_by(words[i].0, all[i]);
            // a second channel is read where it is taken by its own word
            let second_reads = |i: usize| {
                second_sources[i].is_some_and(|(w, r)| taken_by(w, &r) && in_a_or_b_2(&r))
            };
            let second_alpha = |i: usize| second_sources[i].is_some_and(|(w, r)| taken_by(w, &r) && r[2] == Some(t));
            let reads = |i: usize| (taken(i) && in_a_or_b(all[i])) || second_reads(i);
            let read = ReadFor {
                colour: (0..COLOUR_CHANNELS).any(reads),
                value: (COLOUR_CHANNELS..all.len()).any(|i| i != COLOUR_CHANNELS + BUMP_HEIGHT && reads(i))
                    || (0..all.len()).any(|i| (taken(i) && canonical(all[i][2]) == Some(t)) || second_alpha(i)),
                bump: reads(COLOUR_CHANNELS + BUMP_HEIGHT),
            };
            let bump_on = names_t(&named.values[BUMP_HEIGHT])
                || second_sources[COLOUR_CHANNELS + BUMP_HEIGHT].is_some_and(|(_, r)| names_t(&r));
            let tex = &scene.textures[t];
            let source = match (tex.class, &tex.body) {
                (TextureClass::Procedural, TextureBody::Procedural(procedural)) => {
                    if !crate::textures::open_points(&procedural.settings, version).is_empty() {
                        return Err(open::TEXTURE_OPEN);
                    }
                    // : read only
                    // for outputs it is asked for, which each evaluation writes; any other read
                    // would be of what earlier evaluations left (refused). A colour asked where
                    // no component produces any output is Open .
                    // The bump is the exception as 3.1 and 5.0: every evaluation there writes a
                    // gradient , none derived for an output not asked, so a Bump Height
                    // channel that reads a texture not asked for its bump reads the gradient
                    // (0, 0, 0) and the surface is not bumped (chosen, from).
                    let o = procedural.settings.outputs;
                    let bump_stale = read.bump && !o.bump && version == RenderAs::Bryce30;
                    if (read.colour && !o.colour) || (read.value && !o.value) || bump_stale {
                        return Err(open::TEXTURE_OUTPUTS);
                    }
                    // : a texture no evaluation has written a
                    // colour into holds the saved colour word (its top byte not part of it) and
                    // the saved gradient (the first three saved doubles); where no component
                    // produces any output the colour asked is that colour.
                    let word = procedural.saved_result.colour;
                    let kept = Kept {
                        colour: [(word >> 16) as u8, (word >> 8) as u8, word as u8],
                        gradient: [
                            procedural.saved_result.numbers[0],
                            procedural.saved_result.numbers[1],
                            procedural.saved_result.numbers[2],
                        ],
                    };
                    TextureSource::Procedural(procedural.settings.clone(), kept, Held::new(kept.colour))
                }
                // (Outputs: all three, whichever the material asks for;
                // does not apply)
                (TextureClass::Picture, TextureBody::Picture(p)) => {
                    let held = scene.pictures.get(p.index).ok_or(open::PICTURE_MISSING)?;
                    TextureSource::Picture(Arc::clone(held.read_by(version == RenderAs::Bryce50)))
                }
                _ => unreachable!("the reader gives each class its own body"),
            };
            let mp = &tex.mapping;
            let is_picture = matches!(source, TextureSource::Picture(_));
            // : a picture texture reads, as 3.0
            // and 3.1, 0x1 and 0x2 only (0x20 and the rest are not read: clear); as 5.0 it reads
            // 0x20 and draws 0x4, 0x40 and the bits not drawn as clear (Freedom). 0x1 and 0x2 as
            // 5.0 act in the original and how is Open (the Freedom's stand-in is not built: refused).
            // ,: each version reads some bits
            // of the word (0x1; 0x4 as 3.1 and 5.0; 0x20 as 5.0; for a picture 0x2, and 0x40 as
            // 5.0), and no other bit acts on the mapping. Open here (not the mapping's): bits 0x8
            // and 0x10 as 3.1 and 5.0 change how a channel takes the colour and the value
            // , and a picture's bit 0x2 as 5.0 shows it once, giving no result outside it.
            if version != RenderAs::Bryce30 && mp.switches & (SWITCH_COLOUR_TAKEN | SWITCH_VALUE_TAKEN) != 0 {
                return Err(open::MAPPING_SWITCHES);
            }
            if is_picture && version == RenderAs::Bryce50 && mp.switches & SWITCH_SCALE_PICT != 0 {
                return Err(open::MAPPING_SWITCHES);
            }
            let read = match (version, is_picture) {
                (RenderAs::Bryce30, false) => SWITCH_SYMMETRIC,
                (RenderAs::Bryce30, true) => SWITCH_SYMMETRIC | SWITCH_SCALE_PICT,
                (RenderAs::Bryce31, false) => SWITCH_SYMMETRIC | SWITCH_CENTRED,
                (RenderAs::Bryce31, true) => SWITCH_SYMMETRIC | SWITCH_SCALE_PICT | SWITCH_CENTRED,
                (RenderAs::Bryce50, false) => SWITCH_SYMMETRIC | SWITCH_CENTRED | SWITCH_TILE,
                (RenderAs::Bryce50, true) => SWITCH_SYMMETRIC | SWITCH_CENTRED | SWITCH_TILE | SWITCH_ONE_PIXEL,
            };
            let switches = mp.switches & read;
            // : a mode outside the version's gives t = (0, 0, 0)
            // and the tiling step (the mapping does it; mode 8 reads `perm` at the hit).
            let map = MapSettings {
                mode: mp.mode,
                switches,
                scale: mp.scale,
                offset: mp.offset,
                turn: mp.turn,
                picture: matches!(source, TextureSource::Picture(_)),
            };
            textures.push(MaterialTexture { source, map, bump_on });
        }
        Ok(TexturedMaterial { channels, textures, refraction: part.refraction, second, top })
    }

    /// Whether a texture of this material keeps a result that a later evaluation reads (a
    /// procedural texture none of whose components produces anything holds its colour,
    /// ): then the picture depends on the order of evaluations.
    pub fn holds_state(&self) -> bool {
        self.textures.iter().any(|t| match &t.source {
            TextureSource::Procedural(settings, _, _) => {
                !settings.components.iter().any(|c| c.outputs.colour || c.outputs.value || c.outputs.bump)
            }
            TextureSource::Picture(_) => false,
        })
    }

    /// The material's flag word.
    pub fn flags(&self) -> u32 {
        self.channels.flags
    }

    /// The refraction value η.
    pub fn refraction(&self) -> f32 {
        self.refraction
    }

    /// The A value of the Transparency channel, which decides the angle law.
    /// (Materials whose channels have textures: the angle law)
    pub fn transparency_a(&self) -> f64 {
        f64::from(self.channels.values[3].a)
    }

    /// Whether the Transparency channel can be above 0 at a hit: its A or B above 0, or a
    /// texture named in it. (Meshes: refused where a mesh
    /// lets rays through)
    pub fn may_let_through(&self) -> bool {
        let c = &self.channels.values[3];
        c.a > 0.0 || c.b > 0.0 || c.textures.iter().any(Option::is_some)
    }

    /// Whether a texture's mapping reads the object's own box (modes 0, 6, 9, 10, 11
    /// and 12).
    pub fn reads_own_box(&self) -> bool {
        self.textures.iter().any(|t| matches!(t.map.mode, 0 | 6 | 9..=12))
    }

    /// The channels at a hit: each texture evaluated at its mapping's point, its gradient
    /// through the mapping's frame.
    pub fn at_hit(
        &self,
        version: RenderAs,
        object: &MapObject,
        hit: &MapHit,
        channel_hit: &ChannelHit,
        scene_switches: u32,
        tables: TextureTables<'_>,
    ) -> Result<MaterialAtHit, OpenItem> {
        let mut results = Vec::with_capacity(self.textures.len());
        // (mode 8 reads noise type 9 over the user's `perm`, with the smoothstep
        // a texture's own noise takes). (Mode 8, Random)
        let lattice =
            Lattice { perm: tables.perm, waves: tables.waves, version, smooth: TextureOrders::of(version).smoothstep };
        for t in &self.textures {
            let frame = map_point_tabled(version, &t.map, object, hit, &lattice).map_err(|e| match e {
                MappingRefusal::RandomNeedsPerm => open::TEXTURE_EVALUATION,
                MappingRefusal::Mode(_) | MappingRefusal::Random => open::MAPPING_MODE,
            })?;
            let (colour, value, gradient, flips) = match &t.source {
                TextureSource::Procedural(settings, kept, held) => {
                    // (Mirroring)
                    let surface = SurfaceContext::at(frame.point, hit.normal);
                    let (point, flips) = mirrored(version, &t.map, frame.point);
                    let r = crate::textures::evaluate_in_render(settings, point, surface, tables, version, t.bump_on, Some(Kept { colour: held.get(), ..*kept }))
                        .map_err(|_| open::TEXTURE_EVALUATION)?;
                    // : where no component produces any output the
                    // colour the evaluation gave stays as the texture's colour for the next.
                    if !settings.components.iter().any(|c| c.outputs.colour || c.outputs.value || c.outputs.bump) {
                        if let Some(c) = r.colour {
                            held.set(c);
                        }
                    }
                    // an output not asked is never read (`TexturedMaterial::new`): 0
                    // stands in.
                    (r.colour.unwrap_or([0; 3]), r.value.unwrap_or(0.0), r.bump.unwrap_or([0.0; 3]), flips)
                }
                TextureSource::Picture(picture) => {
                    let s = picture
                        .sample_switched(version, frame.point[0], frame.point[1], t.map.switches & SWITCH_ONE_PIXEL != 0)
                        .map_err(|_| open::PICTURE_POINT)?;
                    (s.colour, s.value, s.gradient, [false; 3])
                }
            };
            results.push(TextureAtHit {
                colour,
                value,
                bump: frame_gradient(t.map.mode, &frame, gradient, flips),
                scale_x: t.map.scale[0],
                procedural: !t.map.picture,
            });
        }
        let channel_hit = ChannelHit { top: self.top, ..*channel_hit };
        Ok(evaluate_mixed_channels(version, &self.channels, &self.second, &results, &channel_hit, scene_switches))
    }
}

/// The own box of a pyramid or box light's object (the gels of).
const SQUARE_LIGHT_BOX: (Vec3, Vec3) = ([-0.5; 3], [0.5; 3]);

/// A radial light, or a light with a beam, whose material is a gel.,
/// . (In the render)
#[derive(Clone, Debug, PartialEq)]
pub struct GelLight {
    material: TexturedMaterial,
    placement: Placement,
    /// c0, the light's own colour.
    own_colour: [f32; 3],
    /// Which gel point: the radial light's or a beam's.
    kind: GelKind,
}

impl GelLight {
    /// See `TexturedMaterial::holds_state`.
    pub fn holds_state(&self) -> bool {
        self.material.holds_state()
    }

    /// A radial light's gel.
    pub fn new(material: TexturedMaterial, placement: Placement, own_colour: [f32; 3]) -> GelLight {
        GelLight { material, placement, own_colour, kind: GelKind::Radial }
    }

    /// A gel of the kind of light `kind` ('s gel points).
    pub fn of_kind(material: TexturedMaterial, placement: Placement, own_colour: [f32; 3], kind: GelKind) -> GelLight {
        GelLight { material, placement, own_colour, kind }
    }

    /// ⟨c0k·Dk⟩ at the lit point p, D the Diffuse colour gives at the gel point, the
    /// channels taken with the scene's switch bit 0x200 or without it.,
    pub fn colour_at(
        &self,
        version: RenderAs,
        p: Vec3,
        scene_switches: u32,
        tables: TextureTables<'_>,
    ) -> Result<[f32; 3], OpenItem> {
        let diffuse = self.diffuse_at(version, p, scene_switches, tables)?;
        Ok(std::array::from_fn(|k| self.own_colour[k] * diffuse[k]))
    }

    /// D, the Diffuse colour gives the light's material at the gel point of p (for a
    /// gel with a gradient, which takes the gradient's colour in place of c0).
    pub fn diffuse_at(
        &self,
        version: RenderAs,
        p: Vec3,
        scene_switches: u32,
        tables: TextureTables<'_>,
    ) -> Result<[f32; 3], OpenItem> {
        let g = match self.kind {
            GelKind::Radial => gel_hit(version, &self.placement, p),
            kind => {
                // : the beam's gel point, q carried by the light's matrix,
                // the direction (0, −1, 0) through its transposed inverse, in the orders the
                // gel points take.
                let orders = GelOrders::of(version);
                let q = light_space(&self.placement, p);
                let gp = gel_point(version, kind, q);
                GelHit {
                    q,
                    place: gp.q,
                    world: gel_world(gp.q, &self.placement.rows(), self.placement.translation(), orders.world),
                    u: gp.u,
                    v: gp.v,
                    normal: gel_normal(gp.direction, &self.placement.inverse_rows(), orders.normal),
                }
            }
        };
        // (Inputs, revised at log entry 3315: the radial light's own object counts
        // as a sphere, a light with a beam as another object; its own box: −0.5 to 0.5 for the
        // pyramid and box lights, −1 to 1 for the others)
        let (low, high) = match self.kind {
            GelKind::Pyramid | GelKind::Box => SQUARE_LIGHT_BOX,
            _ => LIGHT_BOX,
        };
        let object = MapObject { rows: self.placement.rows(), low, high, sphere: self.kind == GelKind::Radial };
        let hit = MapHit { world: g.world, object: g.place, normal: g.normal, u: g.u, v: g.v, du: [0.0; 3], dv: [0.0; 3] };
        let channel_hit = ChannelHit { height: g.world[1], top: f64::NAN, normal: g.normal, distance: 0.0 };
        let at = self.material.at_hit(version, &object, &hit, &channel_hit, scene_switches | TEXTURES_ON, tables)?;
        Ok(at.diffuse)
    }
}
