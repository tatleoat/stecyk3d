// SPDX-License-Identifier: MIT
//! Kind `picture`: a whole picture of a document, answered with the SHA-256 of its levels.
//!
//! (Kind `picture`);;
//! (the tables a render uses: pyramid for, aa-offsets for, dither
//! for; star-colours and the star catalogue for); the render itself is `render::render_lit`
//! (Pictures)

use std::path::Path;
use std::sync::Arc;

use crate::substrate::KeptFileError;
use crate::substrate_kept::StarRecord;

use super::{Context, Outcome, Value};
use crate::render::{OpenChoices, render_lit};
use crate::textures::tables::OwnedTables;
use crate::scene::Scene;
use crate::scene_file::Flavour;
use crate::substrate::Substrate;
use crate::version::RenderAs;

/// The document's flavour from its extension: `.br5` Bryce 5, otherwise Bryce 3D (`.br3`).
fn flavour_of(path: &Path) -> Flavour {
    match path.extension().and_then(|e| e.to_str()) {
        Some(e) if e.eq_ignore_ascii_case("br5") => Flavour::Bryce5,
        _ => Flavour::Bryce3d,
    }
}

/// The render's inputs beside the document: only the substrate's tables, each when held.
/// A table the render uses and the substrate lacks makes the render refuse .
pub fn choices_from(substrate: &Substrate) -> OpenChoices {
    OpenChoices {
        pyramid_normal: substrate.pyramid().ok().copied(),
        aa_offsets: substrate.aa_offsets().ok().copied(),
        dither_table: substrate.dither().ok().map(|d| Box::new(*d)),
        haze_five: substrate.haze_five().ok().copied(), // As 5.0,
        texture_tables: OwnedTables::from_substrate(substrate),
        ..OpenChoices::default()
    }
}

/// Whether a render as `version` takes the sky's stars from Bryce 5's catalogue: as 5.0 a
/// field (bit 0x8000 and a count) with a negative seed.,
pub fn catalogue_sky(scene: &Scene, version: RenderAs) -> bool {
    let s = &scene.sky;
    version == RenderAs::Bryce50
        && s.flags & 0x8000 != 0
        && (s.star_count != 0 || s.comet_count != 0)
        && s.star_seed < 0
}

/// What a render of `scene` as `version` needs of the substrate: the tables of its version, of a
/// Bryce 5 document and of trees, the moon's picture where the sky shows it, the star catalogue
/// (and star-colours) where the sky takes its stars from it. The installed leaves of trees are
/// asked for where they are used.,
pub fn render_needs(scene: &Scene, version: RenderAs, path: &Path) -> crate::substrate::RenderNeeds {
    crate::substrate::RenderNeeds {
        version,
        bryce5_document: flavour_of(path) == Flavour::Bryce5,
        has_trees: !scene.trees.is_empty(),
        moon_picture: scene.sky.flags & crate::sky::sky_colours::MOON_PICTURE != 0,
        catalogue_stars: crate::render::lit::takes_catalogue(scene, version),
        leaves: Vec::new(),
        install_folder_named: false,
    }
}

/// The star catalogue kept beside the substrate: `None` where the substrate keeps none for a
/// render as 5.0, `Some(Err)` where it cannot be read or is damaged.
fn star_catalogue(substrate: &Substrate) -> Option<Result<Arc<Vec<StarRecord>>, String>> {
    match substrate.read_star_catalogue() {
        Ok(records) => Some(Ok(Arc::new(records))),
        Err(KeptFileError::NoneForRole { .. }) => None,
        Err(e) => Some(Err(e.to_string())),
    }
}

/// The moon's picture kept beside the substrate for the version rendered: `None` where the
/// substrate keeps none, `Some(Err)` where it cannot be read or is damaged.,
fn moon_picture(
    substrate: &Substrate,
    version: RenderAs,
) -> Option<Result<Arc<crate::sky::moon_image::MoonImage>, String>> {
    match substrate.read_moon_picture(version) {
        Ok(m) => Some(m.image().map(Arc::new).map_err(|e| e.to_string())),
        Err(KeptFileError::NoneForRole { .. }) => None,
        Err(e) => Some(Err(e.to_string())),
    }
}

/// Compute a picture case. (Kind `picture`)
pub fn compute(v: &Value, version: RenderAs, ctx: Context<'_>) -> Outcome {
    let Some(document) = v.get("document").and_then(Value::as_str) else {
        return Outcome::refused("no document");
    };
    let Some(substrate) = ctx.substrate else {
        return Outcome::refused("no substrate could be read (SUB-8: no render without its tables)");
    };
    let path = ctx.case_folder.join(document);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => return Outcome::refused(format!("document {}: {e}", path.display())),
    };
    let scene = match Scene::read(&bytes, flavour_of(&path)) {
        Ok(s) => s,
        Err(e) => return Outcome::refused(format!("document {}: {e:?}", path.display())),
    };
    let mut choices = choices_from(substrate);
    if catalogue_sky(&scene, version) {
        choices.star_catalogue = star_catalogue(substrate);
        choices.star_colours = substrate.star_colours().ok().copied();
    }
    // : the file of the version rendered.
    if scene.sky.flags & crate::sky::sky_colours::MOON_PICTURE != 0 {
        choices.moon_picture = moon_picture(substrate, version);
    }
    // , : a document whose shading draws from the
    // generator is answered as the render a document gets on opening (README-w13, "The pass
    // over edges": no quick preview before it, the generator in its state before any seeding),
    // which comes before the preview.
    if crate::render::lit::shading_draws(&scene, version, &choices).unwrap_or(false) {
        choices.no_preview = true;
    }
    match render_lit(&scene, version, &choices) {
        Ok((picture, _)) => Outcome::Levels(picture.levels_sha256_hex()),
        Err(e) => Outcome::refused(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flavours() {
        assert_eq!(flavour_of(Path::new("a/b.br3")), Flavour::Bryce3d);
        assert_eq!(flavour_of(Path::new("a/b.BR5")), Flavour::Bryce5);
    }
}
