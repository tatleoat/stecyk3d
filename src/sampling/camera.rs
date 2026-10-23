// SPDX-License-Identifier: MIT
//! The camera, its projections, and the ray of a picture point.
//!
//! points 1 and 10 ;
//! (camera)
//!
//! Every operation is one IEEE binary64 operation in the order written (no fused
//! multiply-add); "f32(...)" marks a rounding to binary32 the entry or the vectors call for.

use super::batch::{CameraRay, PointBatch, RayBatch};
use crate::numerics::{sqrt_pair_as, sqrt_as, x87};
use crate::version::RenderAs;

#[cfg(test)]
#[path = "camera_tests.rs"]
mod tests;

/// The camera's projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Projection {
    Perspective,
    Parallel,
    Panorama,
}

/// The four bits of the camera's projection word (its second flag word) that
/// decide the projection; every other bit changes nothing in the rays. point 1
const WORD_PERSPECTIVE: u32 = 0x4000; // point 1
const WORD_PARALLEL: u32 = 0x8000; // point 1
const WORD_PANORAMA: u32 = 0x10000; // point 1
const WORD_WITH_PERSPECTIVE: u32 = 0x80000; // point 1 (changes nothing beside 0x4000)

impl Projection {
    /// The projection a camera's projection word gives. `None` for the combinations
    /// leaves to the implementer (none of the four bits, 0x80000 alone, or two or
    /// more other than 0x4000 with 0x80000); the render decides those
    /// ). Parallel is what the word says; a render as 3.1 or 5.0
    /// traces it in perspective (`CameraModel::new`). point 1; (Inputs,
    /// Errors: the same four bits and the same implementer's choice, as revised)
    pub fn from_word(word: u32) -> Option<Projection> {
        let bits = word & (WORD_PERSPECTIVE | WORD_PARALLEL | WORD_PANORAMA | WORD_WITH_PERSPECTIVE);
        match bits {
            WORD_PANORAMA => Some(Projection::Panorama),
            WORD_PARALLEL => Some(Projection::Parallel),
            WORD_PERSPECTIVE => Some(Projection::Perspective),
            b if b == WORD_PERSPECTIVE | WORD_WITH_PERSPECTIVE => Some(Projection::Perspective),
            _ => None,
        }
    }
}

/// The camera quantities of a document and the picture size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraInput {
    /// The document's rows right, up, forward. The right row is not used .
    pub right: [f64; 3],
    pub up: [f64; 3],
    pub forward: [f64; 3],
    /// The centre of the image plane.
    pub centre: [f64; 3],
    /// Field of view, radians.
    pub fov: f64,
    pub size: f64,
    pub aspect: f64,
    pub pan: [f64; 2],
    pub zoom: [f64; 2],
    pub projection: Projection,
    pub width: u32,
    pub height: u32,
}

/// The plane height before size and zoom.
const PLANE_HEIGHT: f64 = 1e-5;
/// The share of the field of view the plane's width spans.
const FOV_SHARE: f64 = 0.8;
/// The largest 0.8 · fov before halving, renders as 3.1 and 5.0.
const MAX_SHARED_FOV: f64 = 3.12413936107;

fn f32r(x: f64) -> f64 {
    x as f32 as f64
}

/// The pass a camera ray is made for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pass {
    First,
    Edges,
}

/// Which starting length a ray gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Start {
    Zero,
    /// 's r of (y² + z²) + x².
    Length,
    /// 's r of (x² + z²) + y².
    LengthXzY,
}

/// A panorama ray with the intermediate values names, each binary64.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanoramaSteps {
    pub angle: f64,
    pub sine: f64,
    pub cosine: f64,
    pub height_term: f64,
    pub sum: [f64; 3],
    pub moved: [f64; 3],
    pub direction_before_scaling: [f64; 3],
    pub ray: CameraRay,
}

/// (y² + z²) + x²: a direction's sum of squares in all three versions.
fn len2(v: [f64; 3]) -> f64 {
    (v[1] * v[1] + v[2] * v[2]) + v[0] * v[0]
}

/// A camera axis scaled to unit length: each coordinate times's reciprocal root of
/// the sum of squares, (y² + z²) + x² as 3.0 and 3.1, (x² + y²) + z² as 5.0. does
/// not give the grouping; this is the one `camera-panorama-steps.jsonl` fits on every
/// point (confirms it gives every released vector).
fn unit_axis(version: RenderAs, v: [f64; 3]) -> [f64; 3] {
    let l = match version {
        RenderAs::Bryce30 | RenderAs::Bryce31 => len2(v),
        RenderAs::Bryce50 => (v[0] * v[0] + v[1] * v[1]) + v[2] * v[2],
    };
    let q = sqrt_pair_as(version, l).1;
    [v[0] * q, v[1] * q, v[2] * q]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// A camera prepared for one picture: everything that does not depend on the point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraModel {
    version: RenderAs,
    projection: Projection,
    right: [f64; 3],
    up: [f64; 3],
    forward: [f64; 3],
    centre: [f64; 3],
    eye: [f64; 3],
    width: f64,
    height: f64,
    aspect: f64,
    /// The plane's zoomed width and height (binary32 values).
    zoomed_w: f64,
    zoomed_h: f64,
    /// Pan as lengths on the plane (binary32 values).
    pan_w: f64,
    pan_h: f64,
    /// The quick preview's camera: its rays are formed as says, not as's.
    preview: bool,
}

impl CameraModel {
    /// . Computed from the rows as they are, with no check (Errors: rows
    /// perpendicular to within a relative 1e-15, and a field of view of π, are traced as
    /// computed); the cameras leaves to the implementer are refused by the render
    /// (`render::plan`, check_camera).
    pub fn new(c: &CameraInput, version: RenderAs) -> CameraModel {
        let up = unit_axis(version, c.up);
        let forward = unit_axis(version, c.forward);
        // The cross product of the document's up and forward rows as they are, then scaled
        // ("the cross product of up and forward, scaled to unit length"); the rows'
        // scaled copies give other last bits, which camera-panorama-steps.jsonl rules out
        // (this arithmetic gives every released vector).
        let right = unit_axis(version, cross(c.up, c.forward));
        // Rows that are not perpendicular (Errors leaves them to the implementer): the
        // forward axis is made perpendicular to the right and up axes, forward = (right x up)
        // scaled to unit length. This choice gives the original's pictures of every camera of
        // whose view rows are scaled or sheared (13
        // pictures, every pixel), where the other arrangements tried miss. Perpendicular rows
        // (the dot product at most 1e-15 times the product of the lengths) are traced as they are.
        // (Cameras whose rows are not perpendicular)
        let dot = c.up[0] * c.forward[0] + c.up[1] * c.forward[1] + c.up[2] * c.forward[2];
        let perpendicular = dot.abs() <= 1e-15 * (len2(c.up).sqrt() * len2(c.forward).sqrt());
        let forward = if perpendicular { forward } else { unit_axis(version, cross(right, up)) };

        let width = c.width as f64;
        let height = c.height as f64;
        // The camera part's scale is's size as 3.0; 3.1 ignores it (Variants),
        // and so does 5.0. point 10, question 1
        let h = match version {
            RenderAs::Bryce30 => c.size * PLANE_HEIGHT,
            RenderAs::Bryce31 | RenderAs::Bryce50 => PLANE_HEIGHT,
        };
        let plane_h = f32r(h);
        // From the unrounded h, rounded once. question 3 (factor order free)
        let plane_w = f32r(h * (width / height) * c.aspect);

        let mut a = FOV_SHARE * c.fov;
        if version != RenderAs::Bryce30 && a > MAX_SHARED_FOV {
            a = MAX_SHARED_FOV;
        }
        let tangent = x87::tan_ext(a / 2.0);
        let dist = x87::div_f64_by(plane_w / 2.0, tangent); // rounded once to binary64
        let eye = [
            c.centre[0] - forward[0] * dist,
            c.centre[1] - forward[1] * dist,
            c.centre[2] - forward[2] * dist,
        ];

        let zoomed_w = f32r(plane_w * c.zoom[0]);
        let zoomed_h = f32r(plane_h * c.zoom[1]);
        let pan_w = f32r(c.pan[0] * zoomed_w);
        let pan_h = f32r(c.pan[1] * zoomed_h);

        // 3.1 has no parallel projection ; 5.0 renders it in perspective too
        // (question 4; point 1: 0x8000 alone is perspective as 3.1 and 5.0).
        let projection = match (c.projection, version) {
            (Projection::Parallel, RenderAs::Bryce31 | RenderAs::Bryce50) => Projection::Perspective,
            (p, _) => p,
        };

        CameraModel {
            version,
            projection,
            right,
            up,
            forward,
            centre: c.centre,
            eye,
            width,
            height,
            aspect: c.aspect,
            zoomed_w,
            zoomed_h,
            pan_w,
            pan_h,
            preview: false,
        }
    }

    /// The quick preview's camera: the same camera, in perspective, whose rays `ray` forms
    /// as says (`preview_ray`).
    pub fn for_preview(mut self) -> CameraModel {
        self.projection = Projection::Perspective;
        self.preview = true;
        self
    }

    /// The preview's ray through the middle of pixel (x, y): with p_w = w / width and p_h =
    /// h / height (w, h the zoomed plane's sides), u = (x · p_w + 0.5 · p_w) − o_x and
    /// v = (y · p_h + 0.5 · p_h) − o_y (o the pan's offsets on the plane); the plane's top left
    /// corner c = (m − (w / 2) · r′) + (h / 2) · t′, with r′ and t′ the unit right and up axes
    /// scaled to unit length a second time; d = ((c + u · r) − v · t) − e per coordinate, each
    /// step rounded to binary64; the start the square root of (d_y² + d_z²) + d_x², the
    /// direction d times the reciprocal square root of that sum as 3.0 and 3.1 and of
    /// (d_x² + d_y²) + d_z² as 5.0.,
    pub fn preview_ray(&self, x: u32, y: u32) -> CameraRay {
        let pw = self.zoomed_w / self.width;
        let ph = self.zoomed_h / self.height;
        let u = (x as f64 * pw + 0.5 * pw) - self.pan_w;
        let v = (y as f64 * ph + 0.5 * ph) - self.pan_h;
        let r2 = unit_axis(self.version, self.right);
        let t2 = unit_axis(self.version, self.up);
        let (half_w, half_h) = (self.zoomed_w / 2.0, self.zoomed_h / 2.0);
        let (m, r, t, e) = (self.centre, self.right, self.up, self.eye);
        let mut d = [0.0; 3];
        for k in 0..3 {
            let c = (m[k] - half_w * r2[k]) + half_h * t2[k];
            d[k] = ((c + u * r[k]) - v * t[k]) - e[k];
        }
        let start = sqrt_as(self.version, len2(d));
        let l = match self.version {
            RenderAs::Bryce30 | RenderAs::Bryce31 => len2(d),
            RenderAs::Bryce50 => (d[0] * d[0] + d[1] * d[1]) + d[2] * d[2],
        };
        let q = sqrt_pair_as(self.version, l).1;
        CameraRay { origin: e, dir: [d[0] * q, d[1] * q, d[2] * q], start }
    }

    /// The eye point.
    pub fn eye(&self) -> [f64; 3] {
        self.eye
    }

    /// The unit axes and the image plane's centre, for the lens: the same axes as the
    /// camera rays', to the last bit. point 3
    pub fn lens_frame(&self) -> super::lens::LensFrame {
        super::lens::LensFrame { right: self.right, up: self.up, forward: self.forward, centre: self.centre }
    }

    /// The projection actually traced (after's version rules).
    pub fn projection(&self) -> Projection {
        self.projection
    }

    /// u = x / width - 1/2 and v = -(y / height - 1/2), each step rounded to binary64.
    fn picture_uv(&self, x: f64, y: f64) -> (f64, f64) {
        (x / self.width - 0.5, -(y / self.height - 0.5))
    }

    /// The point's place on the image plane: a = Wz · u - pu, b = Hz · v + pv, then
    /// (a R + b U) + C per coordinate.,
    fn plane_point(&self, x: f64, y: f64) -> [f64; 3] {
        let (u, v) = self.picture_uv(x, y);
        let a = self.zoomed_w * u - self.pan_w;
        let b = self.zoomed_h * v + self.pan_h;
        let c = self.centre;
        let (r, p) = (self.right, self.up);
        [(a * r[0] + b * p[0]) + c[0], (a * r[1] + b * p[1]) + c[1], (a * r[2] + b * p[2]) + c[2]]
    }

    /// Direction and start from an unnormalised direction: each coordinate times's q
    /// of (y² + z²) + x²; the start as `start` says.,
    fn finish(&self, origin: [f64; 3], d: [f64; 3], start: Start) -> CameraRay {
        let (r, q) = sqrt_pair_as(self.version, len2(d));
        let start = match start {
            Start::Zero => 0.0,
            Start::Length => r,
            Start::LengthXzY => sqrt_as(self.version, (d[0] * d[0] + d[2] * d[2]) + d[1] * d[1]),
        };
        CameraRay { origin, dir: [d[0] * q, d[1] * q, d[2] * q], start }
    }

    /// The ray of picture point (x, y) in the first pass.
    pub fn ray(&self, x: f64, y: f64) -> CameraRay {
        self.ray_in(x, y, Pass::First)
    }

    /// The ray of picture point (x, y) for the pass over edges: as `ray`, except that a
    /// perspective ray's start as 5.0 takes (y² + z²) + x² as 3.0 and 3.1 do.
    /// , point 3
    pub fn edge_ray(&self, x: f64, y: f64) -> CameraRay {
        self.ray_in(x, y, Pass::Edges)
    }

    fn ray_in(&self, x: f64, y: f64, pass: Pass) -> CameraRay {
        if self.preview {
            // The preview asks for the middle of a pixel, (x + 0.5, y + 0.5).
            debug_assert!(x.fract() == 0.5 && y.fract() == 0.5);
            return self.preview_ray((x - 0.5) as u32, (y - 0.5) as u32);
        }
        match self.projection {
            Projection::Perspective => {
                let p = self.plane_point(x, y);
                let e = self.eye;
                let d = [p[0] - e[0], p[1] - e[1], p[2] - e[2]];
                // : 5.0's first-pass rays take (x² + z²) + y² for the start.
                let start = match (self.version, pass) {
                    (RenderAs::Bryce50, Pass::First) => Start::LengthXzY,
                    _ => Start::Length,
                };
                self.finish(e, d, start)
            }
            Projection::Parallel => {
                // Renders as 3.0 only : the origin P - F, the direction P minus
                // that origin as formed, the start its length.
                let p = self.plane_point(x, y);
                let f = self.forward;
                let o = [p[0] - f[0], p[1] - f[1], p[2] - f[2]];
                let d = [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
                self.finish(o, d, Start::Length)
            }
            Projection::Panorama => self.panorama_steps(x, y).ray,
        }
    }

    /// A panorama ray of picture point (x, y), with its intermediate values (the pass
    /// makes no difference to it).,
    pub fn panorama_steps(&self, x: f64, y: f64) -> PanoramaSteps {
        use std::f64::consts::PI;
        let (u, v) = self.picture_uv(x, y);
        let (angle, height_term) = match self.version {
            // 3.0: the pan has no part; angle 2π · u, height π · v.
            RenderAs::Bryce30 => (2.0 * PI * u, PI * v),
            // 3.1 and 5.0: angle (u - pu / Wz) · 2π; height g + g with
            // g = (((pv / Hz) + v) · π) / D, D = (width / height) · aspect, a division.
            RenderAs::Bryce31 | RenderAs::Bryce50 => {
                let angle = (u - self.pan_w / self.zoomed_w) * (2.0 * PI);
                let d = (self.width / self.height) * self.aspect;
                let g = ((self.pan_h / self.zoomed_h + v) * PI) / d;
                (angle, g + g)
            }
        };
        let s = x87::sin_f64(angle); // rounded to binary64,
        let c = x87::cos_f64(angle); // rounded to binary64,
        let h = height_term;
        let (r, f, p, o) = (self.right, self.forward, self.up, self.centre);
        let mut sum = [0.0; 3];
        for (k, t) in sum.iter_mut().enumerate() {
            let (sr, hu, cf) = (s * r[k], h * p[k], c * f[k]);
            // The order of the three terms, per version and coordinate.
            *t = match (self.version, k) {
                (RenderAs::Bryce30, 0) => (sr + hu) + cf,
                (RenderAs::Bryce30, 1) => (sr + cf) + hu,
                (RenderAs::Bryce30, _) => (cf + sr) + hu,
                (RenderAs::Bryce31, 0 | 1) => (sr + hu) + cf,
                (RenderAs::Bryce31, _) => (cf + sr) + hu,
                (RenderAs::Bryce50, _) => (cf + hu) + sr,
            };
        }
        // Added to the centre and the centre taken away again.
        let moved = [o[0] + sum[0], o[1] + sum[1], o[2] + sum[2]];
        let d = [moved[0] - o[0], moved[1] - o[1], moved[2] - o[2]];
        // 3.0: the start is the direction's length before scaling; 3.1, 5.0: 0.
        let start = if self.version == RenderAs::Bryce30 { Start::Length } else { Start::Zero };
        let ray = self.finish(o, d, start);
        PanoramaSteps { angle, sine: s, cosine: c, height_term, sum, moved, direction_before_scaling: d, ray }
    }

    /// First-pass rays for every point of a batch, appended to `out` (cleared first).
    pub fn trace_points(&self, points: &PointBatch, out: &mut RayBatch) {
        out.clear();
        for i in 0..points.len() {
            let ray = self.ray(points.x[i], points.y[i]);
            out.push(&ray, i as u32);
        }
    }

    /// Rays of the pass over edges for every point of a batch (see `edge_ray`).
    /// , point 3
    pub fn trace_edge_points(&self, points: &PointBatch, out: &mut RayBatch) {
        out.clear();
        for i in 0..points.len() {
            let ray = self.edge_ray(points.x[i], points.y[i]);
            out.push(&ray, i as u32);
        }
    }
}
