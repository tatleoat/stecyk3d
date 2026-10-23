// SPDX-License-Identifier: MIT
//! The beam record of a spot light or a parallel light: the last bytes of its resource, after
//! the light part.
//!
//! (the records' layouts and
//! the data lengths),
//! (What is read)

use crate::scene_records::{ByteOrder, double, single, word};

/// The spot light's record: 16 bytes.
pub const SPOT_RECORD_LEN: usize = 16;
/// The parallel light's record: 20 bytes.
pub const PARALLEL_RECORD_LEN: usize = 20;

/// Offsets within the records.
const SPOT_EXPONENT_AT: usize = 4;
const SPOT_TRACK_AT: usize = 12;
const PARALLEL_FIRST_AT: usize = 4;
const PARALLEL_SECOND_AT: usize = 8;

/// Which record a light class carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeamClass {
    /// C3dConeLight. [format]
    Spot,
    /// C3dCubeLight. [format]
    Parallel,
    /// C3dCylLight: laid out as the spot light's. [format]
    Cylinder,
    /// C3dPyrLightType: laid out as the parallel light's. [format]
    Square,
}

impl BeamClass {
    /// The record's length.
    pub fn record_len(self) -> usize {
        match self {
            BeamClass::Spot | BeamClass::Cylinder => SPOT_RECORD_LEN,
            BeamClass::Parallel | BeamClass::Square => PARALLEL_RECORD_LEN,
        }
    }
}

/// A beam record as read. The first word (1 in every document) and the parallel light's two
/// last words are not kept (Freedom).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BeamRecord {
    /// The spot light: the exponent e of's cone, and the identity of its falloff track
    /// (class ConL_eFallOff), 0 for none.
    Spot { exponent: f64, falloff_track: u32 },
    /// The parallel light: the two exponents of's box: the first the exponent across the
    /// light's own x axis (ex), the second across its z axis (ez).
    Parallel { exponents: [f32; 2] },
    /// The cylindrical parallel light: the exponent of's cylinder and the identity of its
    /// falloff track, in the spot light's layout.
    Cylinder { exponent: f64, falloff_track: u32 },
    /// The square spot light: the two exponents of's pyramid (ex, ez as the parallel
    /// light's), in the parallel light's layout.
    Square { exponents: [f32; 2] },
}

impl BeamRecord {
    /// Reads the record from `b`, the bytes after the light part (at least the record's
    /// length; the caller checks the data's length first).
    pub fn parse(class: BeamClass, b: &[u8], order: ByteOrder) -> BeamRecord {
        debug_assert!(b.len() >= class.record_len());
        match class {
            BeamClass::Spot => BeamRecord::Spot {
                exponent: double(b, SPOT_EXPONENT_AT, order),
                falloff_track: word(b, SPOT_TRACK_AT, order),
            },
            BeamClass::Cylinder => BeamRecord::Cylinder {
                exponent: double(b, SPOT_EXPONENT_AT, order),
                falloff_track: word(b, SPOT_TRACK_AT, order),
            },
            BeamClass::Parallel => BeamRecord::Parallel {
                exponents: [single(b, PARALLEL_FIRST_AT, order), single(b, PARALLEL_SECOND_AT, order)],
            },
            BeamClass::Square => BeamRecord::Square {
                exponents: [single(b, PARALLEL_FIRST_AT, order), single(b, PARALLEL_SECOND_AT, order)],
            },
        }
    }

    /// The falloff track's identity, 0 for none (and for a parallel light).
    pub fn falloff_track(&self) -> u32 {
        match *self {
            BeamRecord::Spot { falloff_track, .. } | BeamRecord::Cylinder { falloff_track, .. } => falloff_track,
            BeamRecord::Parallel { .. } | BeamRecord::Square { .. } => 0,
        }
    }

    /// The exponents as doubles (a single widens exactly), in the record's order.
    pub fn exponents(&self) -> Vec<f64> {
        match *self {
            BeamRecord::Spot { exponent, .. } | BeamRecord::Cylinder { exponent, .. } => vec![exponent],
            BeamRecord::Parallel { exponents } | BeamRecord::Square { exponents } => {
                exponents.iter().map(|&e| f64::from(e)).collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spot_record_in_both_orders() {
        let mut le = Vec::new();
        le.extend_from_slice(&1u32.to_le_bytes());
        le.extend_from_slice(&2.5f64.to_le_bytes());
        le.extend_from_slice(&77u32.to_le_bytes());
        let mut be = Vec::new();
        be.extend_from_slice(&1u32.to_be_bytes());
        be.extend_from_slice(&2.5f64.to_be_bytes());
        be.extend_from_slice(&77u32.to_be_bytes());
        let want = BeamRecord::Spot { exponent: 2.5, falloff_track: 77 };
        assert_eq!(BeamRecord::parse(BeamClass::Spot, &le, ByteOrder::Little), want);
        assert_eq!(BeamRecord::parse(BeamClass::Spot, &be, ByteOrder::Big), want);
        assert_eq!(want.exponents(), vec![2.5]);
    }

    #[test]
    fn parallel_record_keeps_its_order() {
        let mut b = Vec::new();
        for w in [1u32, 0.5f32.to_bits(), 3.0f32.to_bits(), 0, 0] {
            b.extend_from_slice(&w.to_be_bytes());
        }
        assert_eq!(b.len(), PARALLEL_RECORD_LEN);
        let r = BeamRecord::parse(BeamClass::Parallel, &b, ByteOrder::Big);
        assert_eq!(r, BeamRecord::Parallel { exponents: [0.5, 3.0] });
        assert_eq!(r.exponents(), vec![0.5, 3.0]);
        assert_eq!(r.falloff_track(), 0);
    }
}
