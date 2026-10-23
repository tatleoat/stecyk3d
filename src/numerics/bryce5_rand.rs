// SPDX-License-Identifier: MIT
//! Bryce 5's own pseudo-random generator.
//!
//! ; ("The leaves' run": starting
//! states other than a seed's)
//!
//! A combined generator: four periodic sequences read from the substrate table
//! `noise-bits-five` (periods 47, 53, 59, 61; strides 1, 3, 6, 7; turned left by 1, 2, 3, 0
//! bits) are joined by exclusive or and drive an additive recurrence modulo 2^32.

/// The table words the generator reads: `T[0]` to `T[60]`.
pub const TABLE_WORDS_READ: usize = 61;

/// One periodic sequence: its period, its stride and its left turn.
struct Lane {
    period: u32,
    stride: u32,
    turn: u32,
}

const LANES: [Lane; 4] = [
    Lane { period: 47, stride: 1, turn: 1 },
    Lane { period: 53, stride: 3, turn: 2 },
    Lane { period: 59, stride: 6, turn: 3 },
    Lane { period: 61, stride: 7, turn: 0 },
];

/// The generator's state: the last output and, per lane, the table index of the next draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bryce5Rand {
    table: [u32; TABLE_WORDS_READ],
    sum: u32,
    /// `(s - stride * n) mod period` for the next draw n, per lane.
    index: [u32; 4],
}

impl Bryce5Rand {
    /// The periods of the four lanes: the bounds of a starting state's positions.
    pub const PERIODS: [u32; 4] = [47, 53, 59, 61];

    /// Seeds the generator with `seed` over the table `noise-bits-five` (64 words; the first
    /// 61 are read): the run with sum s and positions s mod 47, 53, 59 and 61.
    pub fn new(seed: u32, noise_bits_five: &[u32; 64]) -> Bryce5Rand {
        let positions = std::array::from_fn(|k| seed % LANES[k].period);
        Bryce5Rand::from_state(seed, positions, noise_bits_five)
    }

    /// A run started with the sum `sum` and the positions a, b, c, d (each below its lane's
    /// period; reduced modulo it otherwise):'s formula with u(0) = v and s - n, s - 3n,
    /// s - 6n, s - 7n replaced by a - n, b - 3n, c - 6n, d - 7n.
    pub fn from_state(sum: u32, positions: [u32; 4], noise_bits_five: &[u32; 64]) -> Bryce5Rand {
        let mut table = [0u32; TABLE_WORDS_READ];
        table.copy_from_slice(&noise_bits_five[..TABLE_WORDS_READ]);
        let mut index = [0u32; 4];
        for (k, lane) in LANES.iter().enumerate() {
            // Index for n = 1: (position - stride) mod period, from 0 to period - 1.
            let s = positions[k] % lane.period;
            index[k] = (s + lane.period - lane.stride % lane.period) % lane.period;
        }
        Bryce5Rand { table, sum, index }
    }

    /// The next 32-bit draw.
    pub fn next_u32(&mut self) -> u32 {
        let mut w = 0u32;
        for (k, lane) in LANES.iter().enumerate() {
            w ^= self.table[self.index[k] as usize].rotate_left(lane.turn);
            self.index[k] = (self.index[k] + lane.period - lane.stride) % lane.period;
        }
        self.sum = self.sum.wrapping_add(w);
        self.sum
    }

    /// The next draw as a single from 0 up to 1: the low 23 bits over 2^23, exact.
    pub fn next_unit(&mut self) -> f32 {
        let m = self.next_u32() & 0x7f_ffff;
        m as f32 / 8_388_608.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The entry's formula written out directly, draw by draw.
    fn by_formula(seed: u32, t: &[u32; 64], draws: usize) -> Vec<u32> {
        let m = |a: i64, p: i64| a.rem_euclid(p) as usize;
        let s = seed as i64;
        let mut u = seed;
        (1..=draws as i64)
            .map(|n| {
                let w = t[m(s - n, 47)].rotate_left(1)
                    ^ t[m(s - 3 * n, 53)].rotate_left(2)
                    ^ t[m(s - 6 * n, 59)].rotate_left(3)
                    ^ t[m(s - 7 * n, 61)];
                u = u.wrapping_add(w);
                u
            })
            .collect()
    }

    fn made_up_table() -> [u32; 64] {
        let mut t = [0u32; 64];
        let mut x = 0x2545_f491u32;
        for v in t.iter_mut() {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *v = x;
        }
        t
    }

    #[test]
    fn matches_the_formula() {
        let t = made_up_table();
        for seed in [0u32, 1, 46, 47, 1000, 0x7fff_ffff, u32::MAX] {
            let mut g = Bryce5Rand::new(seed, &t);
            let got: Vec<u32> = (0..3000).map(|_| g.next_u32()).collect();
            assert_eq!(got, by_formula(seed, &t, 3000), "seed {seed}");
        }
    }

    /// 's starting state written out from its formula, draw by draw.
    fn by_state_formula(v: u32, p: [i64; 4], t: &[u32; 64], draws: usize) -> Vec<u32> {
        let m = |a: i64, q: i64| a.rem_euclid(q) as usize;
        let mut u = v;
        (1..=draws as i64)
            .map(|n| {
                let w = t[m(p[0] - n, 47)].rotate_left(1)
                    ^ t[m(p[1] - 3 * n, 53)].rotate_left(2)
                    ^ t[m(p[2] - 6 * n, 59)].rotate_left(3)
                    ^ t[m(p[3] - 7 * n, 61)];
                u = u.wrapping_add(w);
                u
            })
            .collect()
    }

    #[test]
    fn starting_states_match_their_formula() {
        let t = made_up_table();
        let states = [(0u32, [0u32, 0, 0, 0]), (3_421_969_743, [24, 38, 29, 13]), (u32::MAX, [46, 52, 58, 60]), (7, [1, 2, 3, 4])];
        for (v, p) in states {
            let mut g = Bryce5Rand::from_state(v, p, &t);
            let got: Vec<u32> = (0..500).map(|_| g.next_u32()).collect();
            assert_eq!(got, by_state_formula(v, p.map(i64::from), &t, 500), "state {v} {p:?}");
        }
    }

    #[test]
    fn a_seed_starts_its_own_state() {
        // : the run a seed starts is v = s, a = s mod 47, b = s mod 53, c = s mod 59, d = s mod 61.
        let t = made_up_table();
        for s in [0u32, 1, 2_184_841_607, u32::MAX] {
            let a = Bryce5Rand::new(s, &t);
            let b = Bryce5Rand::from_state(s, [s % 47, s % 53, s % 59, s % 61], &t);
            assert_eq!(a, b);
        }
    }

    #[test]
    fn last_three_words_are_not_read() {
        let t = made_up_table();
        let mut u = t;
        u[61] ^= 1;
        u[62] ^= 2;
        u[63] ^= 3;
        let (mut a, mut b) = (Bryce5Rand::new(9, &t), Bryce5Rand::new(9, &u));
        assert!((0..500).all(|_| a.next_u32() == b.next_u32()));
    }

    #[test]
    fn unit_is_low_23_bits() {
        let t = made_up_table();
        let mut a = Bryce5Rand::new(5, &t);
        let mut b = a.clone();
        for _ in 0..100 {
            let d = a.next_u32();
            let f = b.next_unit();
            assert_eq!(f, (d & 0x7f_ffff) as f32 / 8_388_608.0);
            assert!((0.0..1.0).contains(&f));
        }
    }
}
