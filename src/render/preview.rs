// SPDX-License-Identifier: MIT
//! The quick preview: the small picture the program renders before a frame that is asked
//! for, whose only use here is the state it leaves (the materials its rays meet). Its picture
//! is never output.
//!
//! , (blocks traced at their corners, the test of 20
//! levels and the division), (the camera for an 80 by 60 picture, in
//! perspective),
//! (Effects: the preview makes no star list)
//! (The quick preview)

use crate::numerics::CRand;
use crate::sampling::blocks::{CornerBlocks, FIRST_SIDE, corners_on_edge};
use crate::sampling::camera::{CameraModel, Projection};
use crate::sampling::samples::clamp_channel;
use crate::scene::Scene;
use crate::scene_records::Region;

use super::plan::{RenderError, RenderPlan, camera_input};
use super::stages::{RenderCounts, SkyRules, Stages, SurfaceRules};

/// The preview's picture, whatever the document's size and region.
pub const PREVIEW_WIDTH: u32 = 80;
pub const PREVIEW_HEIGHT: u32 = 60;

/// A preview ray whose depth is this or more sends no further ray. (revised log
/// 7168: whether, as 5.0, it follows a frame's L other than 6 is Open; kept at 3 for every L,
/// ;, Release of)
pub const PREVIEW_DEPTH_LIMIT: u32 = 3;

/// A corner's ray goes through the middle of its pixel.
const PLACE_OFFSET: f64 = 0.5;

/// The preview's plan: the frame's, with the camera of an 80 by 60 picture in perspective
/// (a parallel or panorama camera is previewed in perspective), the whole picture as its
/// region, no lens and's way. The camera is the view the frame opens the document with
/// (takes the camera as read, before animation moved it; the objects it meets are
/// those after the tracks applied on opening).
/// (The quick preview)
pub fn preview_plan(as_read: &Scene, frame: &RenderPlan) -> Result<RenderPlan, RenderError> {
    // (Inputs: the camera is the camera as it stood when the document was read,
    // before the tracks applied on opening moved it; the objects are those after the tracks,
    // which the plan `frame` and the caller's scene hold)
    let mut input = camera_input(as_read, frame.version)?;
    input.width = PREVIEW_WIDTH;
    input.height = PREVIEW_HEIGHT;
    input.projection = Projection::Perspective;
    let mut plan = frame.clone();
    plan.camera = CameraModel::new(&input, frame.version).for_preview();
    plan.camera_input = input;
    plan.width = PREVIEW_WIDTH;
    plan.height = PREVIEW_HEIGHT;
    plan.region = Region { top: 0, left: 0, bottom: PREVIEW_HEIGHT, right: PREVIEW_WIDTH };
    plan.lens = None;
    plan.edge_pass = None;
    plan.saved = None;
    // , (Effects: the preview makes no
    // star list and moves nothing)
    plan.star_list = super::plan::StarListUse::Skip;
    Ok(plan)
}

/// A corner's levels: each channel of the ray's colour clamped as clamps a sample's,
/// times 255 in binary64 and cut to a whole number (no rounding to binary32 between, no gamma
/// and no dithering).
fn corner_level(rgb: [f32; 3]) -> [u8; 3] {
    rgb.map(|c| (f64::from(clamp_channel(c)) * 255.0).trunc() as u8)
}

/// The side of the grid of a soft light's shadow rays in the preview: 9 rays, so 18 draws for
/// each shadow its rays ask of such a light.,
pub const PREVIEW_SOFT_GRID: u32 = 3;

/// The preview's walk: blocks taken depth first in's order, each ray traced alone with
/// the generator lent to the shading (the preview's draws).
struct Walk<'a> {
    scene: &'a Scene,
    plan: &'a RenderPlan,
    surface: &'a mut dyn SurfaceRules,
    sky: &'a mut dyn SkyRules,
    generator: &'a mut CRand,
    stages: Stages,
    counts: RenderCounts,
    /// Grid of places, for the index of a place (stage 1 asks `corner_places` by grid index).
    grid: CornerBlocks,
    /// Per place of the picture: its level once traced (a corner inside the picture is
    /// traced once,; one beyond it is traced each time a block needs it).
    kept: Vec<Option<[u8; 3]>>,
}

impl Walk<'_> {
    /// The level of the ray through place (x, y): kept where it lies inside the picture.
    fn corner(&mut self, x: u32, y: u32) -> Result<[u8; 3], RenderError> {
        let (w, h) = (self.plan.width, self.plan.height);
        let inside = x < w && y < h;
        if inside {
            if let Some(level) = self.kept[(y * w + x) as usize] {
                return Ok(level);
            }
        }
        let index = y * self.grid.stride() + x;
        self.stages.corner_places(self.grid.stride(), PLACE_OFFSET, &[index]);
        self.stages.rays(self.plan);
        self.stages.hits(self.plan);
        let mut traced = [0.0f32; 3];
        self.stages.colours(
            self.scene,
            self.plan,
            self.surface,
            self.sky,
            &mut self.counts,
            Some(&mut *self.generator),
            |_, rgb| traced = rgb,
        )?;
        let level = corner_level(traced);
        if inside {
            self.kept[(y * w + x) as usize] = Some(level);
        }
        Ok(level)
    }

    /// One block of side `s` with top-left place (x, y)., (The
    /// test; On an edge)
    fn block(&mut self, x: u32, y: u32, s: u32) -> Result<(), RenderError> {
        if s == 1 {
            self.corner(x, y)?;
            return Ok(());
        }
        // The corners in the order top left, top right, bottom right, bottom left.
        let tl = self.corner(x, y)?;
        let tr = self.corner(x + s, y)?;
        let br = self.corner(x + s, y + s)?;
        let bl = self.corner(x, y + s)?;
        if corners_on_edge(tl, tr, bl, br) {
            let h = s / 2;
            for (qx, qy) in [(x, y), (x + h, y), (x + h, y + h), (x, y + h)] {
                // Those that begin beyond the last column or row are left out.
                if qx < self.plan.width && qy < self.plan.height {
                    self.block(qx, qy, h)?;
                }
            }
        }
        Ok(())
    }
}

/// Render the preview with the given rules (built on `plan`), for its effect on their state
/// and on `generator`, the generator as it stands when the preview begins and as its last draw
/// leaves it. Blocks 4 a side over the whole picture, traced at their corners in's order
/// (depth first).
pub fn render_preview(
    scene: &Scene,
    plan: &RenderPlan,
    surface: &mut dyn SurfaceRules,
    sky: &mut dyn SkyRules,
    generator: &mut CRand,
) -> Result<RenderCounts, RenderError> {
    let grid = CornerBlocks::new(plan.width, plan.height, plan.region);
    let kept = vec![None; (plan.width * plan.height) as usize];
    let mut walk =
        Walk { scene, plan, surface, sky, generator, stages: Stages::new(), counts: RenderCounts::default(), grid, kept };
    let r = plan.region;
    for y in (r.top..r.bottom).step_by(FIRST_SIDE as usize) {
        for x in (r.left..r.right).step_by(FIRST_SIDE as usize) {
            walk.block(x, y, FIRST_SIDE)?;
        }
    }
    Ok(walk.counts)
}
