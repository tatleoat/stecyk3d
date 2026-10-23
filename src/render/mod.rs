// SPDX-License-Identifier: MIT
//! render: a read document in, a picture's levels and a PNG out.
//!
//! ; the shading
//! and sky rules are parameters (`SurfaceRules`, `SkyRules`); `lit` implements them from
//! ; `modes` implements's render modes.

pub mod eye;
pub mod frames;
pub mod movie;
pub mod lit;
pub mod media;
pub mod modes;
pub mod parallel;
pub mod plan;
pub mod preview;
pub mod stages;
pub mod textured;

pub use eye::{EyeDecision, EyeMedium};
pub use lit::{PlainSky, PlainSurface, render_lit, render_lit_ending, render_lit_streamed};
pub use media::{PathMedia, PathState};
pub use modes::{RenderMode, render_mode};
pub use plan::{
    Bryce5Part, EdgePass, GeometryBuild, OpenChoices, PictureWay, RenderError, RenderPlan, SharedDraws, StarListUse, bryce5_post, bryce5_rays_a_side,
    camera_input, geometry_scene, post_options, projection_of_word, rays_a_side, structure_code,
};
pub use stages::{
    HitBatch, Picture, PixelObserver, RenderCounts, SampleTag, SecondaryKind, SecondaryQueue, SecondaryRay,
    SkyRequest, SkyRules, CAMERA_TOTAL, GENERATION_GUARD, Stages, SurfaceRequest, SurfaceRules, render_picture,
    render_picture_progress,
};

use crate::scene::Scene;
use crate::version::RenderAs;

/// Render `scene` as `version`, with the caller's rules and Open choices.
pub fn render(
    scene: &Scene,
    version: RenderAs,
    choices: &OpenChoices,
    surface: &mut dyn SurfaceRules,
    sky: &mut dyn SkyRules,
) -> Result<(Picture, RenderCounts), RenderError> {
    let opened = plan::opened_with_haze_five(scene, version, choices.haze_five)?;
    let scene: &Scene = &opened;
    let mut plan = RenderPlan::new(scene, version, choices)?;
    stages::render_picture(scene, &mut plan, surface, sky)
}
