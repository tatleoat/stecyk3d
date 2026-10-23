// SPDX-License-Identifier: MIT
//! sampling-output: camera and projections, sample places, passes and order of pixels,
//! the picture's final levels and files.
//!
//!
//! Work is handed between stages as plain batches (`batch`): picture points in, camera
//! rays out, so that a staged (wavefront) renderer can run each stage over a whole batch.

pub mod aa_pass;
pub mod batch;
pub mod blocks;
pub mod camera;
pub mod lens;
pub mod order;
pub mod output;
pub mod png;
pub mod premium;
pub mod samples;

pub use batch::{CameraRay, PointBatch, RayBatch};
pub use camera::{CameraInput, CameraModel, Projection};
pub use order::{PixelVisit, RenderOrder, SavedPlace, row_pixels, scattered_pixels};
pub use samples::{AaSetting, PixelSum, SamplePlacer};
