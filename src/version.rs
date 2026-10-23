// SPDX-License-Identifier: MIT
//! The program version a scene is rendered as.
//!
//! (shared vocabulary). The three versions are those the entries name
//! (for example: "Bryce 3D 3.0, 3.1, or Bryce 5.0").

/// Which program's behaviour a render reproduces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RenderAs {
    Bryce30,
    Bryce31,
    Bryce50,
}
