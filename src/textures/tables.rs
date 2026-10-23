// SPDX-License-Identifier: MIT
//! The substrate tables a texture may read, borrowed from the user's substrate per evaluation.
//! Nothing of them is embedded; a table the substrate lacks stays `None` and a noise that needs
//! it will be refused , never given a default.
//!
//! (noise-bits-five)
//! (Tables)

use crate::substrate::{Substrate, TableName};

#[derive(Clone, Copy, Debug, Default)]
pub struct TextureTables<'a> {
    /// The permutation of 0..255 .
    pub perm: Option<&'a [i32; 256]>,
    /// The six Waves centres .
    pub waves: Option<[[f64; 3]; 6]>,
    /// Bryce 5's noise-bits table .
    pub noise_bits_five: Option<&'a [u32; 64]>,
}

impl<'a> TextureTables<'a> {
    /// No table at all: what the vectors of are computed with.
    pub fn none() -> TextureTables<'static> {
        TextureTables::default()
    }

    /// Borrow what the substrate holds.
    pub fn from_substrate(substrate: Option<&'a Substrate>) -> TextureTables<'a> {
        match substrate {
            None => TextureTables::default(),
            Some(s) => TextureTables {
                perm: s.perm().ok(),
                waves: s.waves_centres().ok(),
                noise_bits_five: s.noise_bits_five().ok(),
            },
        }
    }

    /// Whether a table is held.
    pub fn holds(&self, name: TableName) -> bool {
        match name {
            TableName::Perm => self.perm.is_some(),
            TableName::Waves => self.waves.is_some(),
            TableName::NoiseBitsFive => self.noise_bits_five.is_some(),
            _ => false,
        }
    }
}

/// The tables a render keeps for the whole render, copied from the user's substrate (never
/// embedded or defaulted), lent out as `TextureTables` per evaluation.
/// (The clouds in the render)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OwnedTables {
    pub perm: Option<Box<[i32; 256]>>,
    pub waves: Option<[[f64; 3]; 6]>,
    pub noise_bits_five: Option<Box<[u32; 64]>>,
}

impl OwnedTables {
    /// Copy what the substrate holds.
    pub fn from_substrate(s: &Substrate) -> OwnedTables {
        OwnedTables {
            perm: s.perm().ok().map(|p| Box::new(*p)),
            waves: s.waves_centres().ok(),
            noise_bits_five: s.noise_bits_five().ok().map(|n| Box::new(*n)),
        }
    }

    pub fn view(&self) -> TextureTables<'_> {
        TextureTables { perm: self.perm.as_deref(), waves: self.waves, noise_bits_five: self.noise_bits_five.as_deref() }
    }
}
