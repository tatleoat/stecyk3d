// SPDX-License-Identifier: MIT
//! What the Blender add-on reads of a preset library, a document, the extractor's kept
//! libraries and a new relief, written as JSON for it (the program's `read` form).
//!
//! (what an
//! import makes: the materials, the sky, the objects of the document in its order, the sun as
//! one lamp, the camera in use), (the
//! settings' names the fields take)
//! (The Rust side; what a document's JSON
//! holds; what is not carried)

use crate::geom::Vec3;
use crate::numerics::CRand;
use crate::scene::{Body, Light, Material, Scene, Shape};
use crate::scene_beams::BeamRecord;
use crate::scene_library::{Library, PresetData};
use crate::scene_parts::{Channel, Rgb};
use crate::scene_pictures::HeldPicture;
use crate::scene_records::{ObjectKind, Transform};
use crate::scene_textures::{Mapping, Texture, TextureBody};
use crate::substrate::{KeptRole, Substrate};
use crate::terrain::{HeightGrid, new_relief};
use crate::textures::settings::{ColourMap, ColourSpace, Component, Dimension, Filter, NoiseSettings, OutputSet, Phase};
use crate::version::RenderAs;

// ---------------------------------------------------------------------------------------
// A small JSON writer (std only).
// ---------------------------------------------------------------------------------------

/// A JSON value to be written.
#[derive(Clone, Debug, PartialEq)]
pub enum J {
    Null,
    Bool(bool),
    /// Written as the shortest decimal that reads back to the same double; `null` when not finite.
    Num(f64),
    /// As `Num`, but a number that is not finite is written as the tokens `NaN`, `Infinity` and `-Infinity` (the
    /// dialect Python's `json` reads and writes, and this crate's own reader takes), so that an import can tell a
    /// document's NaN from its infinity (a sky's rainbow numbers: tests/blender_vs_bryce, sky/word-pictures).
    NumX(f64),
    Int(i64),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    pub fn obj(fields: Vec<(&str, J)>) -> J {
        J::Obj(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    fn nums(values: &[f64]) -> J {
        J::Arr(values.iter().map(|&v| J::Num(v)).collect())
    }

    fn nums_x(values: &[f64]) -> J {
        J::Arr(values.iter().map(|&v| J::NumX(v)).collect())
    }

    fn singles(values: &[f32]) -> J {
        J::Arr(values.iter().map(|&v| J::Num(f64::from(v))).collect())
    }

    fn singles_x(values: &[f32]) -> J {
        J::Arr(values.iter().map(|&v| J::NumX(f64::from(v))).collect())
    }

    fn word(w: u32) -> J {
        J::Int(i64::from(w))
    }

    fn int(v: i64) -> J {
        J::Int(v)
    }

    /// Bytes as text, each byte its Latin-1 character (single-byte characters; the
    /// platform's characters for the bytes above 0x7f: ours).
    fn text(bytes: &[u8]) -> J {
        J::Str(bytes.iter().map(|&b| char::from(b)).collect())
    }

    fn opt_index(i: Option<usize>) -> J {
        i.map_or(J::Null, |i| J::Int(i as i64))
    }

    /// The JSON text.
    pub fn write(&self) -> String {
        let mut out = String::new();
        self.write_to(&mut out);
        out
    }

    fn write_to(&self, out: &mut String) {
        match self {
            J::Null => out.push_str("null"),
            J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            J::Num(v) => {
                if v.is_finite() {
                    // Rust's `{:?}` of a double is the shortest decimal that reads back to it.
                    out.push_str(&format!("{v:?}"));
                } else {
                    out.push_str("null");
                }
            }
            J::NumX(v) => {
                if v.is_nan() {
                    out.push_str("NaN");
                } else if v.is_infinite() {
                    out.push_str(if *v < 0.0 { "-Infinity" } else { "Infinity" });
                } else {
                    out.push_str(&format!("{v:?}"));
                }
            }
            J::Int(v) => out.push_str(&v.to_string()),
            J::Str(s) => write_string(s, out),
            J::Arr(items) => {
                out.push('[');
                for (k, item) in items.iter().enumerate() {
                    if k > 0 {
                        out.push(',');
                    }
                    item.write_to(out);
                }
                out.push(']');
            }
            J::Obj(fields) => {
                out.push('{');
                for (k, (key, value)) in fields.iter().enumerate() {
                    if k > 0 {
                        out.push(',');
                    }
                    write_string(key, out);
                    out.push(':');
                    value.write_to(out);
                }
                out.push('}');
            }
        }
    }
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || (c as u32) == 0x7f => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Standard base64 (RFC 4648, with padding).
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * k)) & 0x3f) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------
// Materials and textures ('s names)
// ---------------------------------------------------------------------------------------

/// The thirteen channels by's input identifiers, in's order.
pub const CHANNEL_NAMES: [&str; 13] = [
    "diffuse_color",
    "ambient_color",
    "specular_color",
    "specular_halo",
    "transparent_color",
    "diffusion",
    "ambience",
    "specularity",
    "transparency",
    "reflection",
    "bump",
    "metallicity",
    "volume_density",
];

/// A material:'s flag word, channels (A, B, alpha, the channel's word and its three
/// references as indices of the texture list), volume colour, refraction and volume numbers.
pub fn material_json(m: &Material) -> J {
    let p = &m.part;
    let colour = |ch: &Channel<Rgb>, refs: &[Option<usize>; 3]| {
        J::obj(vec![
            ("a", J::singles(&ch.a)),
            ("b", J::singles(&ch.b)),
            ("alpha", J::Num(f64::from(ch.alpha))),
            ("word", J::word(ch.flags)),
            ("refs", J::Arr(refs.iter().map(|r| J::opt_index(*r)).collect())),
        ])
    };
    let value = |ch: &Channel<f32>, refs: &[Option<usize>; 3]| {
        J::obj(vec![
            ("a", J::Num(f64::from(ch.a))),
            ("b", J::Num(f64::from(ch.b))),
            ("alpha", J::Num(f64::from(ch.alpha))),
            ("word", J::word(ch.flags)),
            ("refs", J::Arr(refs.iter().map(|r| J::opt_index(*r)).collect())),
        ])
    };
    let mut channels = Vec::new();
    for (k, ch) in p.colours.iter().enumerate() {
        channels.push((CHANNEL_NAMES[k].to_string(), colour(ch, &m.textures.colours[k])));
    }
    for (k, ch) in p.values.iter().enumerate() {
        channels.push((CHANNEL_NAMES[5 + k].to_string(), value(ch, &m.textures.values[k])));
    }
    let mut fields = vec![
        ("name", J::text(&m.name)),
        ("flags", J::word(p.flags)),
        ("channels", J::Obj(channels)),
        ("volume_color", J::singles(&p.volume_colour)),
        ("refraction", J::Num(f64::from(p.refraction))),
        ("volume_numbers", J::singles(&p.volume_numbers)),
    ];
    // The second channels opening at a time makes between two keys that name different textures : the
    // channel as the next key holds it and the share m of each (0 where a channel has none, its entry null).
    // (Animation: the document at its opening time)
    if m.second.colours.iter().any(Option::is_some) || m.second.values.iter().any(Option::is_some) {
        let mut amounts = Vec::new();
        let colours: Vec<J> = m
            .second
            .colours
            .iter()
            .map(|c| {
                amounts.push(c.as_ref().map_or(0.0, |c| c.amount));
                c.as_ref().map_or(J::Null, |c| colour(&c.channel, &c.textures))
            })
            .collect();
        let values: Vec<J> = m
            .second
            .values
            .iter()
            .map(|c| {
                amounts.push(c.as_ref().map_or(0.0, |c| c.amount));
                c.as_ref().map_or(J::Null, |c| value(&c.channel, &c.textures))
            })
            .collect();
        fields.push((
            "mix",
            J::obj(vec![("amounts", J::nums(&amounts)), ("colors", J::Arr(colours)), ("values", J::Arr(values))]),
        ));
    }
    J::obj(fields)
}

fn outputs_word(o: &OutputSet) -> u32 {
    u32::from(o.value) | (u32::from(o.colour) << 1) | (u32::from(o.bump) << 2)
}

fn dims(d: Dimension) -> i64 {
    match d {
        Dimension::One => 1,
        Dimension::Two => 2,
        Dimension::Three => 3,
        Dimension::Other(n) => n,
    }
}

fn noise_json(n: &NoiseSettings) -> J {
    J::obj(vec![
        ("kind", J::int(n.type_number)),
        ("method", J::int(n.method)),
        ("count", J::int(n.octaves)),
        ("dims", J::int(dims(n.dimension))),
    ])
}

fn colour_word(c: [u8; 3]) -> J {
    J::word((u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]))
}

fn space_word(s: ColourSpace) -> u32 {
    match s {
        ColourSpace::Rgb => 0,
        ColourSpace::Hls => 0x40,
    }
}

fn colours_json(c: &ColourMap) -> J {
    J::obj(vec![
        ("mode", J::int(c.mode)),
        ("space", J::word(space_word(c.space))),
        ("first", colour_word(c.a)),
        ("second", colour_word(c.b)),
        ("third", colour_word(c.c)),
    ])
}

fn filter_json(f: &Filter) -> J {
    J::obj(vec![
        ("kind", J::int(f.number)),
        ("n", J::int(f.n)),
        ("a", J::Num(f64::from(f.a))),
        ("b", J::Num(f64::from(f.b))),
    ])
}

fn phase_json(p: &Phase) -> J {
    J::obj(vec![
        ("amount", J::Num(p.amount)),
        ("noise", noise_json(&p.noise)),
        ("rotation", J::nums(&p.space.angles)),
        ("frequency", J::nums(&p.space.frequencies)),
    ])
}

fn component_json(c: &Component) -> J {
    J::obj(vec![
        ("flags", J::word(outputs_word(&c.outputs))),
        ("space", J::word(space_word(c.colours.space))),
        ("noise", noise_json(&c.noise)),
        ("rotation", J::nums(&c.space.angles)),
        ("frequency", J::nums(&c.space.frequencies)),
        ("phase", phase_json(&c.phase)),
        ("filter", filter_json(&c.filter)),
        ("colors", colours_json(&c.colours)),
    ])
}

fn mapping_json(m: &Mapping) -> J {
    J::obj(vec![
        ("mode", J::word(m.mode)),
        ("flags", J::word(m.switches)),
        ("scale", J::nums(&m.scale)),
        ("offset", J::nums(&m.offset)),
        ("rotation", J::nums(&m.turn)),
    ])
}

/// A texture resource: its mapping and its record ('s settings) or its picture.
pub fn texture_json(t: &Texture) -> J {
    let mut fields = vec![("name", J::text(&t.name)), ("mapping", mapping_json(&t.mapping))];
    match &t.body {
        TextureBody::Procedural(p) => {
            let s = &p.settings;
            fields.insert(0, ("kind", J::Str("procedural".into())));
            fields.push((
                "record",
                J::obj(vec![
                    ("outputs", J::word(outputs_word(&s.outputs))),
                    ("blend", J::Arr(s.blend.iter().map(|&b| J::int(b)).collect())),
                    ("color_space", J::word(space_word(s.colours.space))),
                    // : the result as saved (colour word, then the four doubles), which a
                    // texture no evaluation has written holds; the add-on keeps it with the texture node.
                    // The saved gradient is often not a number (most of Bryce 5.0's texture presets
                    // whose result is a bump): written as NaN or Infinity, not null, so it survives.
                    ("saved", J::obj(vec![
                        ("colour", J::word(p.saved_result.colour)),
                        ("numbers", J::nums_x(&p.saved_result.numbers)),
                    ])),
                    ("colors", colours_json(&s.colours)),
                    ("filter", filter_json(&s.filter)),
                    ("phase", phase_json(&s.phase)),
                    ("components", J::Arr(s.components.iter().map(component_json).collect())),
                ]),
            ));
        }
        TextureBody::Picture(p) => {
            fields.insert(0, ("kind", J::Str("picture".into())));
            fields.push(("picture", J::Int(p.index as i64)));
        }
    }
    J::obj(fields)
}

/// A picture: its name, size and pixels (ARGB words, big-endian, as gives them, row
/// after row in's order).
pub fn picture_json(p: &HeldPicture) -> J {
    let pic = &p.picture;
    let bytes: Vec<u8> = pic.pixels().iter().flat_map(|px| px.word().to_be_bytes()).collect();
    J::obj(vec![
        ("name", J::text(&p.name)),
        ("width", J::word(pic.width())),
        ("height", J::word(pic.height())),
        ("argb_base64", J::Str(base64(&bytes))),
    ])
}

// ---------------------------------------------------------------------------------------
// : a library
// ---------------------------------------------------------------------------------------

/// The characters of the Mac OS Roman bytes 0x80 to 0xFF, in order.
const MAC_ROMAN_HIGH: [char; 128] = [
    '\u{C4}', '\u{C5}', '\u{C7}', '\u{C9}', '\u{D1}', '\u{D6}', '\u{DC}', '\u{E1}', '\u{E0}', '\u{E2}', '\u{E4}',
    '\u{E3}', '\u{E5}', '\u{E7}', '\u{E9}', '\u{E8}', '\u{EA}', '\u{EB}', '\u{ED}', '\u{EC}', '\u{EE}', '\u{EF}',
    '\u{F1}', '\u{F3}', '\u{F2}', '\u{F4}', '\u{F6}', '\u{F5}', '\u{FA}', '\u{F9}', '\u{FB}', '\u{FC}', '\u{2020}',
    '\u{B0}', '\u{A2}', '\u{A3}', '\u{A7}', '\u{2022}', '\u{B6}', '\u{DF}', '\u{AE}', '\u{A9}', '\u{2122}',
    '\u{B4}', '\u{A8}', '\u{2260}', '\u{C6}', '\u{D8}', '\u{221E}', '\u{B1}', '\u{2264}', '\u{2265}', '\u{A5}',
    '\u{B5}', '\u{2202}', '\u{2211}', '\u{220F}', '\u{3C0}', '\u{222B}', '\u{AA}', '\u{BA}', '\u{3A9}', '\u{E6}',
    '\u{F8}', '\u{BF}', '\u{A1}', '\u{AC}', '\u{221A}', '\u{192}', '\u{2248}', '\u{2206}', '\u{AB}', '\u{BB}',
    '\u{2026}', '\u{A0}', '\u{C0}', '\u{C3}', '\u{D5}', '\u{152}', '\u{153}', '\u{2013}', '\u{2014}', '\u{201C}',
    '\u{201D}', '\u{2018}', '\u{2019}', '\u{F7}', '\u{25CA}', '\u{FF}', '\u{178}', '\u{2044}', '\u{20AC}',
    '\u{2039}', '\u{203A}', '\u{FB01}', '\u{FB02}', '\u{2021}', '\u{B7}', '\u{201A}', '\u{201E}', '\u{2030}',
    '\u{C2}', '\u{CA}', '\u{C1}', '\u{CB}', '\u{C8}', '\u{CD}', '\u{CE}', '\u{CF}', '\u{CC}', '\u{D3}', '\u{D4}',
    '\u{F8FF}', '\u{D2}', '\u{DA}', '\u{DB}', '\u{D9}', '\u{131}', '\u{2C6}', '\u{2DC}', '\u{AF}', '\u{2D8}',
    '\u{2D9}', '\u{2DA}', '\u{B8}', '\u{2DD}', '\u{2DB}', '\u{2C7}',
];

/// A preset's name or description: the bytes 0x80 and above as the Mac OS Roman characters they stand for
/// (0xAA is the trade mark sign).
pub fn mac_roman(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| if b < 0x80 { char::from(b) } else { MAC_ROMAN_HIGH[usize::from(b - 0x80)] }).collect()
}

/// A library's presets, in the order of the preset class's rows.
pub fn library_json(lib: &Library) -> J {
    let presets = lib
        .presets
        .iter()
        .map(|p| {
            let mut fields = vec![
                ("index", J::Int(p.index as i64)),
                ("name", J::Str(mac_roman(&p.name))),
                ("description", J::Str(mac_roman(&p.description))),
            ];
            match &p.data {
                PresetData::Material(m) => fields.push(("material", material_json(m))),
                PresetData::Texture(t) => fields.push(("texture", texture_json(t))),
                // : the reader's reason; the add-on refuses this preset alone
                PresetData::Unreadable(reason) => fields.push(("unreadable", J::text(reason.as_bytes()))),
            }
            J::obj(fields)
        })
        .collect();
    J::obj(vec![
        (
            "kind",
            J::Str(
                match lib.kind {
                    crate::scene_library::LibraryKind::Materials => "materials",
                    crate::scene_library::LibraryKind::Textures => "textures",
                }
                .into(),
            ),
        ),
        ("presets", J::Arr(presets)),
        ("textures", J::Arr(lib.textures.iter().map(texture_json).collect())),
        ("pictures", J::Arr(lib.pictures.iter().map(picture_json).collect())),
    ])
}

/// The extractor's preset libraries (role 4): those of the folders of `version`'s order first
/// (the program chosen), then every other in the substrate's listing order.
pub fn kept_libraries_json(substrate: &Substrate, version: RenderAs) -> J {
    let role = KeptRole::PresetLibrary;
    let order = substrate.folder_order(version, role);
    let kept: Vec<_> = substrate.kept_files().iter().filter(|k| k.role == role).collect();
    let folder_of = |path: &str| crate::substrate::split_build(path).map(|(f, _)| f.to_string());
    let mut listed: Vec<&crate::substrate::KeptFile> = Vec::new();
    for folder in &order {
        listed.extend(kept.iter().copied().filter(|k| folder_of(&k.path).as_deref() == Some(*folder)));
    }
    listed.extend(kept.iter().copied().filter(|k| !order.iter().any(|f| folder_of(&k.path).as_deref() == Some(*f))));
    J::Arr(
        listed
            .into_iter()
            .map(|k| {
                J::obj(vec![
                    ("path", J::Str(k.path.clone())),
                    ("folder", folder_of(&k.path).map_or(J::Null, J::Str)),
                ])
            })
            .collect(),
    )
}

// ---------------------------------------------------------------------------------------
// : a new relief
// ---------------------------------------------------------------------------------------

/// 's 128 by 128 relief for `seed`.
pub fn relief(seed: u32) -> HeightGrid {
    new_relief(&mut CRand::new(seed))
}

pub fn grid_json(grid: &HeightGrid) -> J {
    J::obj(vec![("size", J::Int(grid.side() as i64)), ("heights_base64", J::Str(base64(&grid.to_le_bytes())))])
}

// ---------------------------------------------------------------------------------------
// : a document
// ---------------------------------------------------------------------------------------

fn transform_json(t: &Transform) -> J {
    let mut v = Vec::with_capacity(12);
    for r in &t.rows {
        v.extend_from_slice(r);
    }
    v.extend_from_slice(&t.translation);
    J::nums(&v)
}

/// The vectors' word for an object's kind. (`objects_made`: the document's shape
/// names, `light`, `camera`, `tree`, `terrain`, `group`); ours for the others
fn kind_word(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Sphere => "sphere",
        ObjectKind::Cube => "cube",
        ObjectKind::Cylinder => "cylinder",
        ObjectKind::Cone => "cone",
        ObjectKind::Plane => "plane",
        ObjectKind::Torus => "torus",
        ObjectKind::Slab => "slab",
        ObjectKind::Square => "square",
        ObjectKind::Pyramid => "pyramid",
        ObjectKind::HeightField => "terrain",
        ObjectKind::TriMesh => "mesh",
        ObjectKind::Group => "group",
        ObjectKind::Path => "path",
        ObjectKind::Blob => "metaball",
        ObjectKind::Tree => "tree",
        ObjectKind::SphereLight
        | ObjectKind::ConeLight
        | ObjectKind::CubeLight
        | ObjectKind::CylinderLight
        | ObjectKind::PyramidLight => "light",
        ObjectKind::Camera => "camera",
    }
}

const VERSIONS: [RenderAs; 3] = [RenderAs::Bryce30, RenderAs::Bryce31, RenderAs::Bryce50];

fn sky_json(scene: &Scene) -> J {
    let s = &scene.sky;
    let sun = scene.sun();
    let mut fields = vec![
        ("flags", J::word(s.flags)),
        ("cloud_height", J::NumX(s.cloud_height)),
        ("cumulus_color", J::singles_x(&s.lower_cloud_colour)),
        ("stratus_color", J::singles_x(&s.upper_cloud_colour)),
        ("cloud_offset", J::nums(&s.cloud_offset)),
        ("cover", J::NumX(s.cloud_cover)),
        ("cover_amplitude", J::NumX(s.cloud_amplitude)),
        ("cloud_frequency", J::NumX(s.cloud_frequency)),
        ("zenith", J::singles_x(&s.sky_colour)),
        ("horizon", J::singles_x(&s.horizon_colour)),
        ("ambient", J::singles_x(&s.dome_colour)),
        ("plain", J::singles_x(&s.plain_sky_colour)),
        ("cumulus_shade", J::singles_x(&s.ambient_colour)),
        ("haze_color", J::singles_x(&s.haze_colour)),
        ("haze", J::NumX(s.haze)),
        ("fog_color", J::singles_x(&s.fog_colour)),
        ("fog_height", J::NumX(s.fog_height)),
        ("fog", J::NumX(s.fog)),
        ("moon", J::nums_x(&s.moon)),
        ("discs", J::nums_x(&s.discs)),
        ("stars", J::Arr(vec![J::word(s.star_seed as u32), J::word(s.star_count), J::word(s.comet_count)])),
        ("ambient_color", J::singles_x(&s.ambient_colour)),
        ("ambient_amount", J::NumX(s.ambience)),
        ("shadow_lift", J::NumX(s.shadow_intensity)),
        ("scene_flags", J::word(scene.settings.switches)),
        ("sun_flags", J::word(sun.switches)),
        ("sun_intensity", J::NumX(sun.intensity)),
        ("sun_color", J::singles_x(&sun.colour)),
        // the sun's light part's colour : another colour than the sun's own (the wash of reads this)
        ("sun_light_color", J::singles_x(&sun.light.colour)),
        ("sun_glow_color", J::singles_x(&sun.glow_colour)),
        ("sun_direction_bryce", J::nums(&sun.direction)),
        ("stratus_texture", J::opt_index(scene.clouds.stratus)),
        ("cumulus_texture", J::opt_index(scene.clouds.cumulus)),
    ];
    fields.push(("sky5_lit_air", J::NumX(s.lit_air)));
    if let Some(r) = sun.light.five.radius {
        fields.push(("sun_radius", J::NumX(r)));
    }
    if let Some(f) = &s.five {
        fields.extend([
            ("sky5", J::Bool(true)),
            ("sky5_flags2", J::word(f.second_flags)),
            ("sky5_haze_offset", J::NumX(f.haze_offset)),
            ("sky5_haze_thickness", J::NumX(f.haze_thickness)),
            ("sky5_fog_offset", J::NumX(f.fog_offset)),
            ("sky5_haze_shift", J::singles_x(&f.haze_shift_colour)),
            ("sky5_cloud_detail", J::nums(&[f.cloud_detail, f.cloud_detail_z])),
            ("sky5_sun_glow_size", J::NumX(f.sun_glow_weight)),
            ("sky5_moon_glow_size", J::NumX(f.moon_glow_weight)),
            ("sky5_sun_disc", J::NumX(f.sun_disc_weight)),
            ("sky5_moon_disc", J::NumX(f.moon_disc_weight)),
            ("sky5_star_intensity", J::NumX(f.star_intensity)),
            ("sky5_comet_intensity", J::NumX(f.comet_intensity)),
            ("sky5_star_orientation", J::nums(&f.star_orientation)),
        ]);
        if let Some(steps) = f.air_steps {
            fields.push(("sky5_air_steps", J::word(steps)));
        }
    }
    J::obj(fields)
}

/// The clouds' edge noise's two numbers (`detail` and `detail_z`) as 3.0 and 3.1 give them for the
/// document's sky at the document's time: the wind's turbulence and half its steps where the sky's tracks are
/// applied (The wind), else 0 and 0; as `{"3.0": [amount, second], "3.1": [amount, second]}`. A version
/// whose opening is refused gives 0 and 0 (the import's own refusal is the reader's).
fn edge_noise_json(scene: &Scene) -> J {
    let one = |version: RenderAs| -> J {
        let [amount, second] = match scene.opened(version) {
            Ok(opened) => opened.sky.wind_edge.unwrap_or([0.0, 0.0]),
            Err(_) => [0.0, 0.0],
        };
        J::nums(&[amount, second])
    };
    J::obj(vec![("3.0", one(RenderAs::Bryce30)), ("3.1", one(RenderAs::Bryce31))])
}

fn light_json(scene: &Scene, index: usize) -> J {
    let Light::Radial(l) = &scene.lights[index] else { return J::Null };
    let five = &l.part.five;
    let mut fields = vec![
        ("flags", J::word(l.switches)),
        ("intensity", J::Num(l.intensity)),
        ("color", J::singles(&l.own_colour)),
        ("shadow_ambience", J::Num(five.shadow_ambience)),
        ("shadow_color", J::singles(&five.shadow_colour)),
        ("range", J::Num(five.range)),
        ("gradient_range", J::Num(five.gradient[0])),
        ("gradient_offset", J::Num(five.gradient[1])),
    ];
    if let Some(r) = five.radius {
        fields.push(("radius", J::Num(r)));
    }
    let (beam, edge) = match l.beam {
        None => ("radial", None),
        Some(BeamRecord::Spot { exponent, .. }) => ("cone", Some([exponent, 0.0])),
        Some(BeamRecord::Parallel { exponents }) => ("box", Some([f64::from(exponents[0]), f64::from(exponents[1])])),
        Some(BeamRecord::Cylinder { exponent, .. }) => ("cylinder", Some([exponent, 0.0])),
        Some(BeamRecord::Square { exponents }) => ("pyramid", Some([f64::from(exponents[0]), f64::from(exponents[1])])),
    };
    fields.push(("beam", J::Str(beam.into())));
    if let Some(e) = edge {
        fields.push(("edge", J::nums(&e)));
    }
    J::obj(fields)
}

fn camera_json(scene: &Scene) -> J {
    let camera = scene.view_camera();
    let p = &camera.part;
    let views = VERSIONS
        .iter()
        .map(|&v| match scene.view.transform(v).given() {
            Some(t) => J::obj(vec![
                ("right", J::nums(&t.rows[0])),
                ("up", J::nums(&t.rows[1])),
                ("forward", J::nums(&t.rows[2])),
                ("centre", J::nums(&t.translation)),
            ]),
            None => J::Null,
        })
        .collect();
    J::obj(vec![
        ("object", J::Int(camera.object as i64)),
        ("name", J::text(&camera.name)),
        ("views", J::Arr(views)),
        ("field_of_view", J::Num(p.field_of_view)),
        ("scale", J::Num(p.scale)),
        ("aspect", J::Num(p.aspect)),
        ("pan", J::nums(&p.pan)),
        ("zoom", J::nums(&p.zoom)),
        ("projection_word", J::word(camera.projection_word)),
    ])
}

fn object_json(scene: &Scene, i: usize) -> J {
    let o = &scene.objects[i];
    let worlds: Vec<J> = VERSIONS.iter().map(|&v| o.world_as(v).map_or(J::Null, |t| transform_json(&t))).collect();
    let group = o.linked_to.filter(|&p| scene.objects[p].kind == ObjectKind::Group);
    let mut fields = vec![
        ("kind", J::Str(kind_word(o.kind).into())),
        ("name", J::text(&o.name)),
        ("worlds", J::Arr(worlds)),
        ("material", J::opt_index(o.material)),
        ("group", J::opt_index(group)),
    ];
    if let Some(p) = &o.placement {
        // , kept so an import can carry them
        fields.push(("placement_flags", J::Arr(vec![J::word(p.first_flags), J::word(p.second_flags), J::word(p.third_flags)])));
    }
    if group.is_some() {
        // ,: a member's role bits in its second flag word
        let word = o.placement.map_or(0, |p| p.second_flags);
        let role = match word & 0x38 {
            0x8 => "POSITIVE",
            0x10 => "NEGATIVE",
            0x20 => "INTERSECT",
            _ => "NEUTRAL",
        };
        fields.push(("role", J::Str(role.into())));
        fields.push(("cut_shows_positive", J::Bool(word & 0x10 != 0 && word & 0x80 != 0)));
    }
    if o.kind == ObjectKind::Blob {
        if let Body::Metaball { negative } = o.body {
            fields.push(("negative", J::Bool(negative)));
        }
    }
    match o.body {
        Body::Shape(Shape::Torus { tube_radius, ring_radius }) => {
            fields.push(("torus", J::nums(&[tube_radius, ring_radius])));
        }
        Body::RadialLight(k) => fields.push(("light", light_json(scene, k))),
        Body::Terrain(k) => {
            let t = &scene.terrains[k].part;
            fields.push((
                "terrain",
                J::obj(vec![
                    ("size", J::Int(t.grid.side() as i64)),
                    ("heights_base64", J::Str(base64(&t.grid.to_le_bytes()))),
                    ("lower", J::Num(t.stored_bounds[0])),
                    ("upper", J::Num(t.stored_bounds[1])),
                    ("mirrored", J::Bool(t.mirrored)),
                    ("smooth", J::Bool(t.smooth)),
                    ("faces", J::word(t.closing_faces & 0x3f)),
                    ("solid_faces", J::word(t.stored_solid_faces & 0x3f)),
                ]),
            ));
        }
        Body::Tree(k) => {
            let t = &scene.trees[k];
            // The stored tree the object shows, in the form the description's `stored` takes (description/trees.rs
            // stored_tree_given): a document's tree is drawn as stored , and growth settings cannot make it again.
            let stored = scene.stored_trees.get(t.stored_tree).map(|st| {
                let matrix = |t: &crate::scene_records::Transform| {
                    J::nums(&[
                        t.rows[0][0], t.rows[0][1], t.rows[0][2], t.rows[1][0], t.rows[1][1], t.rows[1][2], t.rows[2][0], t.rows[2][1],
                        t.rows[2][2], t.translation[0], t.translation[1], t.translation[2],
                    ])
                };
                let shapes = st.shapes.iter().map(|sh| {
                    let words = |a: [u32; 3]| J::Arr(a.iter().map(|&w| J::word(w)).collect());
                    J::obj(vec![
                        ("vertices", J::Arr(sh.points.iter().map(|p| J::nums(p)).collect())),
                        ("normals", J::Arr(sh.normals.iter().map(|p| J::nums(p)).collect())),
                        ("uvs", J::Arr(sh.coords.iter().map(|c| J::singles(c)).collect())),
                        (
                            "triangles",
                            J::Arr(
                                sh.triangles
                                    .iter()
                                    .map(|t| J::obj(vec![("v", words([t[0], t[1], t[2]])), ("n", words([t[3], t[4], t[5]])), ("uv", words([t[6], t[7], t[8]]))]))
                                    .collect(),
                            ),
                        ),
                    ])
                });
                J::obj(vec![
                    ("shapes", J::Arr(shapes.collect())),
                    (
                        "pieces",
                        J::Arr(st.pieces.iter().map(|p| J::obj(vec![("parent", J::int(p.parent.map_or(-1, i64::from))), ("matrix", matrix(&p.transform))])).collect()),
                    ),
                    (
                        "leaves",
                        J::Arr(st.leaves.iter().map(|l| J::obj(vec![("shape", J::word(l.shape)), ("matrix", matrix(&l.transform))])).collect()),
                    ),
                ])
            });
            let mut tree = vec![("leaves_material", t.leaves_material.map_or(J::Null, |m| J::Int(m as i64)))];
            if let Some(stored) = stored {
                tree.push(("stored", stored));
                // Trees of one stored tree share it : the number names it for the description.
                tree.push(("stored_id", J::int(t.stored_tree as i64)));
            }
            fields.push(("tree", J::obj(tree)));
        }
        Body::MeshResource(k) => {
            let m = &scene.mesh_resources[k];
            let points: Vec<J> = m.points.iter().map(|p: &Vec3| J::nums(p)).collect();
            // A flat triangle's shading is from its stored plane normal , which Blender cannot hold apart from
            // the winding, and a description takes from the corners' order. Where the stored one opposes the edge
            // product (the torus "wound the other way", mesh-pictures 05), the corners go in the other order, so that
            // the winding says what the document stored. The corners' own normals and coordinates follow.
            let flipped: Vec<bool> = m
                .triangles
                .iter()
                .map(|t| {
                    let p = |i: usize| m.points[t.points[i] as usize];
                    let (a, b, c) = (p(0), p(1), p(2));
                    let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
                    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
                    n[0] * t.plane[0] + n[1] * t.plane[1] + n[2] * t.plane[2] < 0.0
                })
                .collect();
            let order = |k: usize| if flipped[k] { [0usize, 2, 1] } else { [0, 1, 2] };
            let triangles: Vec<J> = m
                .triangles
                .iter()
                .enumerate()
                .map(|(k, t)| J::Arr(order(k).iter().map(|&i| J::word(t.points[i])).collect()))
                .collect();
            // : what a corner reads of the mesh's normals and coordinates ('s kinds): per triangle the
            // flag word's bits (0 mixed normals, 1 coordinates) and the numbers its corners read, else null
            use crate::scene_meshes::CornerUse;
            let kinds: Vec<u32> = m
                .triangles
                .iter()
                .map(|t| match t.corner_use {
                    CornerUse::NormalsAndCoords => 3,
                    CornerUse::NormalsOnly => 1,
                    _ => 0,
                })
                .collect();
            let mut mesh_fields = vec![("points", J::Arr(points)), ("triangles", J::Arr(triangles))];
            if kinds.iter().any(|&k| k != 0) {
                let numbers = |pick: &dyn Fn(&crate::scene_meshes::MeshTriangle) -> Option<[u32; 3]>, bit: u32| {
                    J::Arr(
                        m.triangles
                            .iter()
                            .zip(&kinds)
                            .enumerate()
                            .map(|(i, (t, k))| match pick(t) {
                                Some(n) if k & bit != 0 => J::Arr(order(i).iter().map(|&x| J::word(n[x])).collect()),
                                _ => J::Null,
                            })
                            .collect(),
                    )
                };
                mesh_fields.push(("normals", J::Arr(m.normals.iter().map(|n| J::nums(n)).collect())));
                mesh_fields.push(("uvs", J::Arr(m.coords.iter().map(|c| J::singles(c)).collect())));
                mesh_fields.push(("corner_normals", numbers(&|t| t.normals, 1)));
                mesh_fields.push(("corner_uvs", numbers(&|t| t.coords, 2)));
                mesh_fields.push(("flags", J::Arr(kinds.iter().map(|&k| J::word(k)).collect())));
            }
            fields.push(("mesh", J::obj(mesh_fields)));
        }
        _ => {}
    }
    J::obj(fields)
}

/// The render settings a document holds that has a setting for: the anti-aliasing and spatial options (every
/// document) and, where Bryce 5's part is present, its rays, depths and structure code.
pub(crate) fn settings_json(scene: &Scene) -> J {
    let s = &scene.settings;
    let mut fields = vec![("antialias", J::word(s.anti_aliasing)), ("spatial", J::word(s.spatial_option))];
    if s.bryce5_words[0] != 0 {
        let w = &s.bryce5_words;
        fields.push(("rays_a_side", J::word(w[1])));
        fields.push(("premium_rays", J::word(w[2])));
        fields.push(("structure", J::Str(String::from_utf8_lossy(&w[5].to_be_bytes()).into_owned())));
        fields.push(("ray_depth", J::word(w[6])));
        fields.push(("tir_depth", J::word(w[7])));
    }
    J::obj(fields)
}

/// What an import makes of a document.
pub fn document_json(scene: &Scene) -> J {
    document_json_as(scene, None)
}

/// `document_json` for the document as `version` opens it : the owners' tracks applied at the opening
/// time, so the import holds the picture the program draws, not the values the records hold before the tracks.
/// Where the version does not open it (what opening applies is Open) the document is given as read: the program's
/// own refusal is the render's. (Animation: the document at its opening time)
pub fn document_json_as(scene: &Scene, version: Option<RenderAs>) -> J {
    match version.map(|v| scene.opened(v)) {
        Some(Ok(opened)) => document_of(&opened, edge_noise_json(scene)),
        _ => document_of(scene, edge_noise_json(scene)),
    }
}

fn document_of(scene: &Scene, edge_noise: J) -> J {
    let view_object = scene.view_camera().object;
    let objects: Vec<J> = (0..scene.objects.len())
        .filter(|&i| scene.objects[i].kind != ObjectKind::Camera || i == view_object)
        .map(|i| {
            let mut o = object_json(scene, i);
            if let J::Obj(f) = &mut o {
                f.push(("index".into(), J::Int(i as i64)));
            }
            o
        })
        .collect();
    J::obj(vec![
        ("materials", J::Arr(scene.materials.iter().map(material_json).collect())),
        ("textures", J::Arr(scene.textures.iter().map(texture_json).collect())),
        ("pictures", J::Arr(scene.pictures.iter().map(picture_json).collect())),
        ("sky", sky_json(scene)),
        ("edge_noise", edge_noise),
        ("objects", J::Arr(objects)),
        ("camera", camera_json(scene)),
        ("settings", settings_json(scene)),
        (
            "picture_size",
            J::Arr(vec![J::word(scene.settings.picture_width), J::word(scene.settings.picture_height)]),
        ),
    ])
}

