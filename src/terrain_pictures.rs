// SPDX-License-Identifier: MIT
//! The Pictures tab's result picture as 3.0 and 3.1: what the Blend control and the menu under
//! the result picture do to each pixel (the slots' values a and b, 0 to 255, and the control's
//! setting s, 0 to 256, 128 as the tab opens), and how a drag moves the control. 5.0's forms are
//! `terrain_editor::pictures_blend_v50`.

/// The menu's five items, in the menu's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultMode {
    Blend,
    Minimum,
    Subtract,
    Add,
    PictBlend,
}

/// The value of one colour channel of the result picture at a pixel, from the first slot's value
/// `a` and the second slot's value `b` (0 to 255 each) and the control's setting `s` (0 to 256):
/// Blend and Pict Blend b + cut down((a - b) s / 256); Minimum cut toward zero((b (256 - s) + m s) /
/// 256), m the lesser of a and b; Subtract limited to 0 to 255 of cut toward zero((b s - a (256 - s)) /
/// 256); Add limited of cut toward zero((a (256 - s) + b s) / 256).
pub fn result_value(mode: ResultMode, a: i64, b: i64, s: i64) -> i64 {
    let limited = |v: i64| v.clamp(0, 255);
    match mode {
        ResultMode::Blend | ResultMode::PictBlend => b + ((a - b) * s).div_euclid(256),
        ResultMode::Minimum => (b * (256 - s) + a.min(b) * s) / 256,
        ResultMode::Subtract => limited((b * s - a * (256 - s)) / 256),
        ResultMode::Add => limited((a * (256 - s) + b * s) / 256),
    }
}

/// The control after one reading of the pointer while the button is down: s becomes limited to 0
/// to 256 of cut toward zero(s + d / 2), d the pointer's distance in pixels to the left of the place
/// it was pressed; the editor then puts the pointer back at that place, so each move counts once,
/// half a unit a pixel, to the left raising s.
pub fn control_after_reading(s: f64, pixels_left: i64) -> f64 {
    (s + pixels_left as f64 / 2.0).trunc().clamp(0.0, 256.0)
}

/// The five fills of the slot menu that give slot B one height at every point (5.0; the menu also
/// holds Load Image, Current Terrain and Original Terrain): Black 0, White 65535 (the slot as the
/// tab opens), Gray 50% 32767 (observed), Gray 25% 16383 and Gray 75% 49151 (code only: read, not
/// run).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotFill {
    Black,
    Gray25,
    Gray50,
    Gray75,
    White,
}

impl SlotFill {
    /// The height the fill gives slot B at every point.
    pub fn height(self) -> u16 {
        match self {
            SlotFill::Black => 0,
            SlotFill::Gray25 => 16383,
            SlotFill::Gray50 => 32767,
            SlotFill::Gray75 => 49151,
            SlotFill::White => 65535,
        }
    }
}
