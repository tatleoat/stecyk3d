// SPDX-License-Identifier: MIT
//! Addition, subtraction, multiplication and division of two doubles with the x87 unit's
//! rule for operands that are not numbers.
//!
//! (two quiet NaNs: the larger significand, equal: the positive one; one NaN: that
//! NaN; a signalling NaN loaded becomes quiet; an invalid operation gives the indefinite
//! value; the memory forms: a signalling second operand loses to a NaN in the first)
//! ("NaN operands")

use super::extended::{X87_INDEFINITE, nan_of, quiet};

/// The four operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

fn is_signalling(x: f64) -> bool {
    x.is_nan() && x.to_bits() & 0x0008_0000_0000_0000 == 0
}

/// `a op b` as the unit gives it. With `second_from_memory` the second operand is taken by the
/// operation itself (it is not loaded into the register first).
pub fn binary(op: Op, a: f64, b: f64, second_from_memory: bool) -> f64 {
    if a.is_nan() || b.is_nan() {
        if second_from_memory && is_signalling(b) && a.is_nan() {
            return quiet(a); // the memory operand's signalling NaN loses to a NaN in the first
        }
        return nan_of(a, b);
    }
    let r = match op {
        Op::Add => a + b,
        Op::Sub => a - b,
        Op::Mul => a * b,
        Op::Div => a / b,
    };
    if r.is_nan() { X87_INDEFINITE } else { r }
}
