// SPDX-License-Identifier: MIT
//! Supplier of power, exponential and square root for the shading code.
//! (arith.rs)
//!
//! The entries require bit-exact results, and the arithmetic behind them belongs to the
//! `numerics` component, which this crate does not implement yet. All transcendental
//! operations in `shading` go through this trait so that component can be plugged in.

/// Power, exponential and square root.
pub trait Arith {
    fn pow(&self, base: f64, exponent: f64) -> f64;
    fn exp(&self, x: f64) -> f64;
    fn sqrt(&self, x: f64) -> f64;
}

/// placeholder using Rust's `f64` methods. Not Bryce's arithmetic.
#[derive(Clone, Copy, Debug, Default)]
pub struct StdArith;

impl Arith for StdArith {
    fn pow(&self, base: f64, exponent: f64) -> f64 {
        base.powf(exponent)
    }
    fn exp(&self, x: f64) -> f64 {
        x.exp()
    }
    fn sqrt(&self, x: f64) -> f64 {
        x.sqrt()
    }
}
