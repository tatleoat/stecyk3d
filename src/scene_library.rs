// SPDX-License-Identifier: MIT
//! A preset library of the user's own Bryce install: the presets of a `.mat` (materials) or
//! a `.brt` (textures) file, each with its name, its description and the material or texture
//! it holds.
//!
//! (the classes of each kind of library; a preset is one row of the preset class
//! with the description resource of the same identity; the 321-byte description resource;
//! the presets in the order of the preset class's rows; a library with no preset row holds no
//! presets),
//! (The Rust side)

use crate::scene::{Doc, Material, ReadError, SceneError, Textures, read_material, read_texture};
use crate::scene_file::{Container, Flavour, ResourceIndex};
use crate::scene_pictures::HeldPicture;
use crate::scene_textures::{Texture, TextureClass};

/// The preset classes and the description classes. [format]
const MATERIAL_PRESET_CLASS: &str = "C3dBaseMaterial";
const TEXTURE_PRESET_CLASS: &str = "C3dTexture";
const MATERIAL_DESCRIPTION_CLASS: &str = "CPresetDescr";
const TEXTURE_DESCRIPTION_CLASS: &str = "txtpresetdesc";
/// The description resource: the name in the first 64 bytes, the description in the next 256,
/// each ended by a zero byte.
const NAME_FIELD: usize = 64;
const DESCRIPTION_FIELD: usize = 256;

/// Which kind of library a file is; told by its extension (`.mat` or `.brt`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryKind {
    Materials,
    Textures,
}

impl LibraryKind {
    fn preset_class(self) -> &'static str {
        match self {
            LibraryKind::Materials => MATERIAL_PRESET_CLASS,
            LibraryKind::Textures => TEXTURE_PRESET_CLASS,
        }
    }

    fn description_class(self) -> &'static str {
        match self {
            LibraryKind::Materials => MATERIAL_DESCRIPTION_CLASS,
            LibraryKind::Textures => TEXTURE_DESCRIPTION_CLASS,
        }
    }
}

/// What a preset holds.
#[derive(Clone, Debug, PartialEq)]
pub enum PresetData {
    /// A material; its `textures` index `Library::textures`.
    Material(Box<Material>),
    Texture(Box<Texture>),
    /// The preset's record cannot be read (the reader's reason); the library's other presets stay usable.
    Unreadable(String),
}

/// One preset.
#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    /// Its place among the rows of the preset class, from 0.
    pub index: usize,
    pub identity: u32,
    /// The name field's bytes before the first zero (tabs at its end kept: is the
    /// offering's business).
    pub name: Vec<u8>,
    pub description: Vec<u8>,
    pub data: PresetData,
}

/// A library read.
#[derive(Clone, Debug, PartialEq)]
pub struct Library {
    pub kind: LibraryKind,
    pub presets: Vec<Preset>,
    /// The textures the material presets' channels name, each once .
    pub textures: Vec<Texture>,
    /// The pictures the picture textures name, each once .
    pub pictures: Vec<HeldPicture>,
}

/// The bytes of `field` before its first zero (all of them when it holds none).
fn up_to_zero(field: &[u8]) -> Vec<u8> {
    field.iter().take_while(|&&c| c != 0).copied().collect()
}

/// The name and the description a description resource holds; a resource shorter than 321
/// bytes gives what it holds (our choice; none observed).
fn name_and_description(bytes: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let name = &bytes[..bytes.len().min(NAME_FIELD)];
    let rest = bytes.get(NAME_FIELD..).unwrap_or(&[]);
    let description = &rest[..rest.len().min(DESCRIPTION_FIELD)];
    (up_to_zero(name), up_to_zero(description))
}

impl Library {
    /// Reads a library: refused, with the reader's reason, when it is not a composite file
    /// (a preset's record that cannot be read is
    /// `PresetData::Unreadable`: the library's other presets stay usable, Errors). A library with no row of its preset class holds no presets.
    pub fn read(file: &[u8], kind: LibraryKind) -> Result<Library, SceneError> {
        // The libraries start with CCmF ('s first form).
        let container = Container::parse(file, Flavour::Bryce3d)?;
        let index = ResourceIndex::read(&container)?;
        let doc = Doc { container: &container, index: &index, reading: crate::scene::Reading::Normal };
        let preset_class = kind.preset_class();
        let description_class = kind.description_class();

        let mut presets = Vec::new();
        let mut textures = Textures { doc: &doc, list: Vec::new(), pictures: Vec::new() };
        for (i, res) in doc.resources(preset_class).enumerate() {
            let (name, description) = match doc.resources(description_class).find(|d| d.identity == res.identity) {
                Some(d) => name_and_description(doc.data(description_class, d)?.bytes),
                None => (Vec::new(), Vec::new()), // none observed; our choice
            };
            let data = match kind {
                LibraryKind::Materials => {
                    // : a record that cannot be read is that preset's alone
                    match read_material(&doc, res).and_then(|mut m| textures.material(&mut m).map(|_| m)) {
                        Ok(material) => PresetData::Material(Box::new(material)),
                        Err(e) => PresetData::Unreadable(e.to_string()),
                    }
                }
                LibraryKind::Textures => {
                    let texture = doc
                        .data(preset_class, res)
                        .map_err(ReadError::Scene)
                        .and_then(|d| read_texture(&d, TextureClass::Procedural));
                    match texture {
                        Ok(texture) => PresetData::Texture(Box::new(texture)),
                        Err(ReadError::Scene(e)) => PresetData::Unreadable(e.to_string()),
                        Err(ReadError::Refused(refusal)) => PresetData::Unreadable(
                            SceneError::Texture { referrer: format!("texture preset {i}"), refusal }.to_string(),
                        ),
                    }
                }
            };
            presets.push(Preset { index: i, identity: res.identity, name, description, data });
        }
        let Textures { list, pictures, .. } = textures;
        Ok(Library { kind, presets, textures: list, pictures })
    }
}
