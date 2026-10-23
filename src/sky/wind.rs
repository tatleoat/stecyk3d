// SPDX-License-Identifier: MIT
//! The wind: what a sky's tracks make of the cloud offsets and the edge noise's second number.
//!
//! ("The wind and the edge noise as 3.0 and 3.1")
//!
//! The function here is the loop alone: the caller reads the tracks (it knows where they are
//! applied and at what time) and hands a function that gives the direction and the speed at a
//! time. Every sum is a binary64 sum in the order written.

use crate::numerics::x87::{cos_ext, sin_ext};
use crate::version::RenderAs;

/// The program's own degree: the double nearest 0.01745329251994, not the one nearest pi/180.
const DEGREE: f64 = 0.01745329251994;
/// A step's length in seconds: the double nearest 0.005.
const STEP: f64 = 0.005;
/// What each step adds to the sum for the edge noise's second number.
const ZETA_PER_STEP: f64 = 0.5;
/// More steps than this would not finish in reasonable time: the loop is refused.
/// sky-clouds.md (the entry gives the loop only for the animation lengths of documents)
const MOST_STEPS: u64 = 50_000_000;

/// The loop's results.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindSums {
    /// The two sums that become the cloud offsets (ox, oz).
    pub offset: [f64; 2],
    /// The sum for zeta: half the number of steps.
    pub zeta: f64,
    /// The number of steps made.
    pub steps: u64,
}

/// Why the loop is not run.
#[derive(Clone, Debug, PartialEq)]
pub enum WindError<E> {
    /// tE is infinite, or the steps would be more than `MOST_STEPS`: the original's loop would
    /// not end in any time (a tE that is not a number ends it after one step).
    NoEnd,
    /// The caller's function refused.
    Value(E),
}

/// One step's two terms: cos(d K) v and sin(d K) v, K the program's degree, as `version`
/// forms them.
fn terms(version: RenderAs, direction: f64, speed: f64) -> (f64, f64) {
    let angle = direction * DEGREE;
    match version {
        // as 3.0 the cosine is rounded to a double before its product, the sine is not
        RenderAs::Bryce30 => (cos_ext(angle).to_f64() * speed, sin_ext(angle).mul_to_f64(speed)),
        // as 3.1 neither is rounded
        RenderAs::Bryce31 => (cos_ext(angle).mul_to_f64(speed), sin_ext(angle).mul_to_f64(speed)),
        // as 5.0 the cosine is the extended result; the sine is taken of the angle rounded to
        // a single and rounded to a double
        RenderAs::Bryce50 => {
            let single = angle as f32 as f64;
            (cos_ext(angle).mul_to_f64(speed), sin_ext(single).to_f64() * speed)
        }
    }
}

/// The wind's steps from `t0` to `t_end`: tau starts at t0; at each step the direction and the
/// speed at tau are read, the terms added; the steps end with the one at which tau equals tE;
/// otherwise tau moves on by 0.005 and a tau beyond tE is taken as tE.
pub fn wind_sums<E>(
    version: RenderAs,
    t0: f64,
    t_end: f64,
    mut at: impl FnMut(f64) -> Result<(f64, f64), E>,
) -> Result<WindSums, WindError<E>> {
    // : the original's end test is
    // "equal" read from the processor's flags, which a compare with a number that is not a
    // number also sets, so a tE that is not a number ends the loop after its first step. An
    // infinite tE never ends in the original (its tau never equals it): refused.
    if t_end.is_infinite() || !t0.is_finite() {
        return Err(WindError::NoEnd);
    }
    let (mut x, mut z, mut zeta) = (0.0f64, 0.0f64, 0.0f64);
    let mut tau = t0;
    let mut steps = 0u64;
    loop {
        let (direction, speed) = at(tau).map_err(WindError::Value)?;
        let (cos_term, sin_term) = terms(version, direction, speed);
        x += cos_term;
        z += sin_term;
        zeta += ZETA_PER_STEP;
        steps += 1;
        if tau == t_end || t_end.is_nan() {
            break;
        }
        tau += STEP;
        if tau > t_end {
            tau = t_end;
        }
        if steps >= MOST_STEPS {
            return Err(WindError::NoEnd);
        }
    }
    Ok(WindSums { offset: [x, z], zeta, steps })
}

#[cfg(test)]
mod tests {
    use super::*;

    // : keys from 0, saved at 2 seconds (tE = 2): "the
    // steps are 402 because the binary64 sums of tau pass just below 2 at the 401st step, so
    // that the next step is at tau taken as tE"; zeta is then 201 (half of 402)
    #[test]
    fn two_seconds_make_402_steps() {
        for v in [RenderAs::Bryce30, RenderAs::Bryce31, RenderAs::Bryce50] {
            let s = wind_sums::<()>(v, 0.0, 2.0, |_| Ok((30.0, 2.0))).unwrap();
            assert_eq!((s.steps, s.zeta), (402, 201.0), "{v:?}");
        }
    }

    // : a length of 1 second with keys from 0 gives 201 steps,
    // and keys from 1.005 seconds to T = 2 give 201 as well
    #[test]
    fn one_second_and_a_late_start_make_201_steps() {
        let a = wind_sums::<()>(RenderAs::Bryce50, 0.0, 1.0, |_| Ok((30.0, 2.0))).unwrap();
        let b = wind_sums::<()>(RenderAs::Bryce50, 1.005, 2.0, |_| Ok((30.0, 2.0))).unwrap();
        assert_eq!((a.steps, b.steps), (201, 201));
    }

    // : one step, direction 30, speed 30 at time 0 gives
    // the offset (30 cos(30 K), 30 sin(30 K)) and zeta 0.5
    #[test]
    fn one_step_at_time_zero() {
        let s = wind_sums::<()>(RenderAs::Bryce50, 0.0, 0.0, |_| Ok((30.0, 30.0))).unwrap();
        assert_eq!((s.steps, s.zeta), (1, 0.5));
        let a = 30.0 * DEGREE;
        // the sine is taken of the angle rounded to a single as 5.0: equal to about 1e-6
        assert!((s.offset[0] - 30.0 * a.cos()).abs() < 1e-12 && (s.offset[1] - 30.0 * a.sin()).abs() < 1e-5);
    }

    #[test]
    fn a_loop_that_cannot_end_is_refused() {
        assert_eq!(wind_sums::<()>(RenderAs::Bryce30, 0.0, f64::INFINITY, |_| Ok((0.0, 0.0))), Err(WindError::NoEnd));
        // a tE that is not a number ends the loop after the first step
        let s = wind_sums::<()>(RenderAs::Bryce30, 0.0, f64::NAN, |_| Ok((30.0, 2.0))).unwrap();
        assert_eq!((s.steps, s.zeta), (1, 0.5));
    }
}
