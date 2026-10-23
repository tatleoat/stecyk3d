// SPDX-License-Identifier: MIT
//! Shading: material channels, secondary rays, bump, volume materials.
//! ; in `lighting`
//!
//! Everything the entries leave Open is either a parameter (a trait the caller implements,
//! or a settings field without a default) or an `open::OpenItem`. Nothing here is presented
//! as a value of the original's.

pub mod arith;
pub mod bump;
pub mod channels;
pub mod gel;
pub mod further;
pub mod lighting;
pub mod lights;
pub mod media;
pub mod material;
pub mod open;
pub mod fuzzy;
pub mod visible;
pub mod volume;
pub mod solid;
pub mod soft;

pub use arith::{Arith, StdArith};
pub use open::OpenItem;
