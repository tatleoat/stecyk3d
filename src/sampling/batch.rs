// SPDX-License-Identifier: MIT
//! Plain batches of picture points and camera rays, handed between stages.
//!
//! (shape of the API: plain batches)
//!
//! Both batches are structures of arrays. They are cleared and refilled, never shrunk, so
//! a renderer keeps one per stage and does not allocate per ray.

/// Picture points (x right from the left edge, y down from the top edge, in pixels with
/// fractions), each tagged with the pixel it belongs to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PointBatch {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    /// Index of the pixel (in whatever numbering the caller uses) the point samples.
    pub pixel: Vec<u32>,
}

impl PointBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(n: usize) -> Self {
        PointBatch { x: Vec::with_capacity(n), y: Vec::with_capacity(n), pixel: Vec::with_capacity(n) }
    }

    pub fn clear(&mut self) {
        self.x.clear();
        self.y.clear();
        self.pixel.clear();
    }

    pub fn push(&mut self, x: f64, y: f64, pixel: u32) {
        self.x.push(x);
        self.y.push(y);
        self.pixel.push(pixel);
    }

    pub fn len(&self) -> usize {
        self.x.len()
    }

    pub fn is_empty(&self) -> bool {
        self.x.is_empty()
    }
}

/// Camera rays. Ray i comes from point `source[i]` of the batch that made it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RayBatch {
    pub origin: [Vec<f64>; 3],
    pub dir: [Vec<f64>; 3],
    pub start: Vec<f64>,
    pub source: Vec<u32>,
}

/// One camera ray, as a value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraRay {
    pub origin: [f64; 3],
    pub dir: [f64; 3],
    /// Hits count only from this length along the ray on.
    pub start: f64,
}

impl RayBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        for k in 0..3 {
            self.origin[k].clear();
            self.dir[k].clear();
        }
        self.start.clear();
        self.source.clear();
    }

    pub fn push(&mut self, ray: &CameraRay, source: u32) {
        for k in 0..3 {
            self.origin[k].push(ray.origin[k]);
            self.dir[k].push(ray.dir[k]);
        }
        self.start.push(ray.start);
        self.source.push(source);
    }

    pub fn len(&self) -> usize {
        self.start.len()
    }

    pub fn is_empty(&self) -> bool {
        self.start.is_empty()
    }

    /// Replaces ray `i`, keeping its source.
    pub fn set(&mut self, i: usize, ray: &CameraRay) {
        for k in 0..3 {
            self.origin[k][i] = ray.origin[k];
            self.dir[k][i] = ray.dir[k];
        }
        self.start[i] = ray.start;
    }

    pub fn get(&self, i: usize) -> CameraRay {
        CameraRay {
            origin: [self.origin[0][i], self.origin[1][i], self.origin[2][i]],
            dir: [self.dir[0][i], self.dir[1][i], self.dir[2][i]],
            start: self.start[i],
        }
    }
}
