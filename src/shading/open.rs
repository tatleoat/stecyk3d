// SPDX-License-Identifier: MIT
//! Open subjects of the shading entries: behaviour of the Bryce programs that is not known, refused with a message.
//! (open.rs)

use std::fmt;

/// One subject whose behaviour in the Bryce programs is not known. Returned as an error wherever a value for it
/// would be needed and no parameter supplies one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenItem {
    pub entry: &'static str,
    pub subject: &'static str,
    pub question: &'static str,
}

impl fmt::Display for OpenItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (not drawn: what the Bryce programs do here is not known)", self.subject)
    }
}

impl std::error::Error for OpenItem {}

const fn item(entry: &'static str, subject: &'static str, question: &'static str) -> OpenItem {
    OpenItem { entry, subject, question }
}

pub const CHANNEL_LIST: OpenItem =
    item("SH-2.1", "the channels' ranges and what a value outside gives", "");
pub const CHANNEL_BLEND: OpenItem =
    item("SH-2.1", "the rule blending two textures by a third", "");
pub const MATERIAL_MIX: OpenItem =
    item("SH-2.1", "how two materials mix channel by channel, and how the weight is given", "");
pub const EVALUATION_ORDER: OpenItem =
    item("SH-2.1", "which texture evaluation orders are fixed (carried state)", "");
pub const ALTITUDE_BLEND: OpenItem =
    item("SH-2.1", "the rule blending two textures by altitude, and which is at the bottom", "");
pub const MATERIAL_SWITCHES: OpenItem =
    item("SH-2.1", "Bryce 5 material switches (haze in volumes, jittered samples)", "");
// revised  states the angle law, the transmitted colour
// (0307), the weight back through a medium (0308), total internal reflection (0310
//), the blended material (0311) and the blurred cone (0312):
// `shading::further` and `shading::media`. What stays Open of is below.
// revised at states the ray depth;
// revised at states the step, the opacity, the density and its fall, the laying and
// the limits of a march : `shading::volume`, with its own Open items.
pub const BUMP_SCALE: OpenItem =
    item("SH-2.3", "the bump scale, the finite-difference step and the gradient's use", "");
pub const BUMP_FADE: OpenItem = item("SH-2.3", "the bump distance fade", "");
pub const VOLUME_VARIANTS: OpenItem =
    item("SH-2.4", "volume materials as 5.0 (Bryce 5 jitter, lighting options)", "");

/// Every open subject of this module.
pub const ALL: &[OpenItem] = &[
    CHANNEL_LIST,
    CHANNEL_BLEND,
    MATERIAL_MIX,
    EVALUATION_ORDER,
    ALTITUDE_BLEND,
    MATERIAL_SWITCHES,
    BUMP_SCALE,
    BUMP_FADE,
    VOLUME_VARIANTS,
];
