// SPDX-License-Identifier: MIT
//! The x87 unit's sine as an extended value, for the orientation track .
//!
//! ("The extended sine")
//!
//! The emulation lives in the numerics component (src/numerics/x87.rs), shared with the
//! camera's sine, cosine and tangent.

pub use crate::numerics::ExtValue;
pub use crate::numerics::x87::sin_ext as x87_sine;
