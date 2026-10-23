// SPDX-License-Identifier: MIT
//! The sky as 5.0 in the render: the sky part read as reads it, the refusals of what it
//! and the entries it points to leave Open, and the sky with its field for a ray that meets
//! nothing.
//!
//! (Applies to, Errors, Limits, What the bits of W
//! do, Stars as 5.0, Where the other Sky Lab settings are, Open),,
//! , (Applies
//! to: Bryce 3D documents),

use super::*;
use crate::scene_sky::{SKY_VERSION_SEVEN, SKY_VERSION_TWO};
use crate::sky::five::{
    self, FieldSource, FivePart, FiveSky, SkyAsFive,
};
use crate::sky::stars::catalogue_colour;
use crate::substrate_kept::StarRecord;

/// Whether a render as 5.0 reads this sky part by (versions 2 and 7; version 1 keeps
/// sky 01's Scope).,
pub(super) fn reads_five(scene: &Scene, version: RenderAs) -> bool {
    version == RenderAs::Bryce50 && matches!(scene.sky.form(), SKY_VERSION_TWO | SKY_VERSION_SEVEN)
}

/// Whether a render as `version` takes the sky's stars from the star catalogue: a sky read as
/// 5.0 with a field (stars or comets, a count) and a negative seed. (Catalogue
/// stars),
pub fn takes_catalogue(scene: &Scene, version: RenderAs) -> bool {
    reads_five(scene, version)
        && five_settings(scene)
            .is_ok_and(|(part, w)| crate::sky::five::has_field(&part, &w) && part.star_seed < 0)
}

/// The sky part as reads it, and its settings as 5.0; refused where, or the
/// entries that hold the air, the light in the air and the clouds, leave the sky Open.
pub(super) fn five_settings(scene: &Scene) -> Result<(FivePart, SkyAsFive), OpenItem> {
    let s = &scene.sky;
    // The numbers version 7 adds, or's values for version 2 (haze-five's three are
    // not read here: the air takes them from the plan).
    let more = s.as_five([0.0; 3]).ok_or(open::SKY_SWITCHES)?;
    let part = FivePart {
        part_version: s.form(),
        switches: s.flags,
        second_switches: more.second_flags,
        cloud_colour: s.lower_cloud_colour,
        sky_colour: s.sky_colour,
        horizon_colour: s.horizon_colour,
        dome_colour: s.dome_colour,
        plain_colour: s.plain_sky_colour,
        haze_colour: s.haze_colour,
        haze: s.haze,
        fog: s.fog,
        fog_height: s.fog_height,
        discs: s.discs,
        moon: s.moon,
        lit_air: s.lit_air,
        star_seed: s.star_seed,
        star_count: s.star_count,
        comet_count: s.comet_count,
        strengths: crate::sky::stars::Strengths { stars: more.star_intensity, comets: more.comet_intensity },
        orientation: more.star_orientation,
        glow_and_disc: [more.sun_glow_weight, more.moon_glow_weight, more.sun_disc_weight, more.moon_disc_weight],
        haze_shift: more.haze_shift_colour,
    };
    let w = five::as_five(&part);
    if let Some(o) = five::five_open(&part, &w) {
        return Err(open::sky_five(o));
    }
    // (As 5.0: Color Perspective in the air of a Bryce 3D sky taken over and of a
    // Bryce 5 sky are both stated; nothing else of the air changes with the bit),
    // , : the lit air of a Bryce 5 sky is gathered with bit
    // 0x800000 and its air_steps places. Nothing of the air is Open here; bit 0x1000000 is the
    // wash (`lit/wash.rs`).
    // : a Bryce 5 sky's layers, on a dome or following the view, are stated
    Ok((part, w))
}

/// Whether the cloud layers are off as 5.0 although the stored word asks for them: a Bryce 3D
/// sky taken over plain (its word as 5.0 has bits 0x20 and 0x40 cleared), and a Bryce 5 sky
/// (whose layers are refused before).,
pub(super) fn five_clouds_off(scene: &Scene) -> bool {
    if !reads_five(scene, RenderAs::Bryce50) {
        return false;
    }
    match five_settings(scene) {
        Ok((_, w)) => w.taken_over_plain,
        Err(_) => true,
    }
}

/// The sky as 5.0 for the rays that meet nothing, with its field, and where a seeded list made
/// for this render left the generator.,
/// (Errors: a missing catalogue or star-colours refuses before any pixel; Effects),
pub(super) fn five_sky(
    scene: &Scene,
    plan: &RenderPlan,
    part: &FivePart,
    w: &SkyAsFive,
) -> Result<(FiveSky, Option<CRand>), OpenItem> {
    // (As 5.0: n = 1000 with bit 0x1000000; the vectors m11 hold a Bryce 3D document
    // with it, whose sky colour is's for the word's other bits), (bit 0x1000000 in a
    // Bryce 5 sky: Open)
    // (Stated now: gives the colours of a Bryce 5 sky's bit 0x1000000.)
    let sun = sun_inputs(scene, plan.version);
    let make_list = plan.star_list != StarListUse::Skip;
    let mut made = if make_list && five::has_field(part, w) && part.star_seed < 0 {
        // (Errors, as: a negative seed with stars or comets on and no
        // catalogue or no star-colours: refused, saying which; a damaged kept file)
        let catalogue = match &plan.star_catalogue {
            None => None,
            Some(Err(_)) => return Err(open::STAR_CATALOGUE_DAMAGED),
            Some(Ok(c)) => Some(c.clone()),
        };
        let (records, table) = match (catalogue, plan.star_colours) {
            (None, None) => return Err(open::STAR_FILES_MISSING),
            (None, Some(_)) => return Err(open::STAR_CATALOGUE_MISSING),
            (Some(_), None) => return Err(open::STAR_COLOURS_MISSING),
            (Some(c), Some(t)) => (c, t),
        };
        let colour = |r: &StarRecord| catalogue_colour(&table, r);
        FiveSky::prepare(&sun, part, w, FieldSource::Catalogue { records: &records, colour: &colour }, true)
    } else {
        FiveSky::prepare(&sun, part, w, FieldSource::Seeded, make_list)
    }
    .map_err(open::sky_five)?;
    // (The moon's picture, bit 0x20000 of W: the moon's disc where the sky draws
    // one, with bit 0x2, 0x4 or 0x8),
    if w.switches & five::MOON_PICTURE != 0 {
        if let Some(c) = made.sky.colours.take() {
            made.sky.colours = Some(c.with_moon_picture(moon_image(plan)?));
        }
    }
    // : a later render of a document held makes no list (`Held`: laid,
    // the generator not moved).
    let list_end = if plan.star_list == StarListUse::Make { made.list_end } else { None };
    // (Effects, Open: as 5.0 with a light that has a radius, where the generator
    // stands for a frame after the preview's shadow rays draw). refuses such lights as
    // 5.0 today; this keeps the field's part refused if that changes.
    if list_end.is_some() {
        let radius = |l: &Light| match l {
            Light::Radial(r) => r.part.five.radius,
            Light::Sun(s) => s.light.five.radius,
        };
        if scene.lights.iter().any(|l| radius(l) != Some(0.0)) {
            return Err(open::STARS_LIGHT_RADIUS);
        }
    }
    Ok((made.sky, list_end))
}
