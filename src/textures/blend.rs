// SPDX-License-Identifier: MIT
//! The components folded into one result , and what the texture gives for each asked
//! output (the texture's map, the outputs it derives, the quarter move, the colour
//! space conversion).
//!
//! (revised 7e05925
//! bump-colour.jsonl),
//! (the texture's filter on the blended value)
//! (Colour maps, blends and the bump's colour)

use crate::version::RenderAs;

use super::catalogue::OpenPoint;
use super::colours::{
    Colour, PermNoise, bump_colour, clamp, cut, from_hls_bytes, held, map_colour, to_hls_bytes, two_colour,
};
use super::filters::filtered;
use super::orders::TextureOrders;
use super::settings::{ColourMap, ColourSpace, OutputSet, SurfaceContext, TextureSettings};

/// The last blend mode number.
pub const LAST_BLEND: i64 = 18;
/// The blend mode whose weight reads `perm`.
pub const PERM_BLEND: i64 = 14;
/// The double nearest 1/765 (a colour's value).
const THIRD_OF_255: f64 = 1.0 / 765.0;
/// The double nearest 1/3 (a bump's value).
const THIRD: f64 = 1.0 / 3.0;

/// What one component gives the fold: its value (always), its colour where it produces one,
/// its bump gradient where it produces one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComponentResult {
    pub produces: OutputSet,
    pub value: f64,
    pub colour: Option<Colour>,
    pub gradient: Option<[f64; 3]>,
    /// As 3.0, the component produces a bump that the render turned off: it gives no gradient,
    /// and one made from it rests on what the texture kept .
    pub kept: bool,
    /// The component's colour map (mode 1's blend takes it, with the accumulated colour as A).
    pub map: ColourMap,
}

/// The accumulated side.
#[derive(Clone, Copy, Debug)]
struct Accumulated {
    produces: OutputSet,
    value: f64,
    colour: Colour,
    gradient: [f64; 3],
    /// The gradient rests on a turned-off bump as 3.0 (`ComponentResult::kept`).
    kept: bool,
    /// The colour space of the component folded in last.
    space: ColourSpace,
}

fn union(a: OutputSet, b: OutputSet) -> OutputSet {
    OutputSet { colour: a.colour || b.colour, value: a.value || b.value, bump: a.bump || b.bump }
}

/// x put into `to`'s colour space from `from`'s.
fn converted(x: Colour, from: ColourSpace, to: ColourSpace, version: RenderAs) -> Result<Colour, OpenPoint> {
    match (from, to) {
        (ColourSpace::Hls, ColourSpace::Rgb) => from_hls_bytes(x, version),
        (ColourSpace::Rgb, ColourSpace::Hls) => to_hls_bytes(x),
        _ => Ok(x),
    }
}

/// The weight of modes 8 to 16.
fn weight(mode: i64, a: f64, b: f64, s: SurfaceContext, o: &TextureOrders, pn: &PermNoise<'_>) -> Result<f64, OpenPoint> {
    Ok(match mode {
        8 => a,
        9 => b,
        10 => s.slope,
        11 => clamp(s.altitude),
        12 => s.orientation,
        13 => o.ease.eval(clamp(4.0 * s.slope - 0.125)),
        15 => clamp(3.0 * (a - b)),
        16 => clamp(3.0 * (b - a)),
        PERM_BLEND => clamp(o.ease.eval(o.ease.eval(pn.blend14()?))),
        _ => 0.0,
    })
}

/// Two colours by the mode, channel by channel.
fn colours(mode: i64, x: Colour, y: Colour, a: f64, b: f64, w: f64) -> Colour {
    match mode {
        4 => return if b > a { y } else { x },
        5 => return if b < a { y } else { x },
        8..=16 => return two_colour(x, y, w),
        _ => {}
    }
    std::array::from_fn(|k| {
        let (x, y) = (i64::from(x[k]), i64::from(y[k]));
        let c = match mode {
            2 => (x + y) / 2,
            3 => (x * y) / 256,
            6 => (x + y).min(255),
            7 => (x - y).rem_euclid(256),
            17 if x < 128 => (x * y) / 128,
            17 => 255 - ((255 - y) * (255 - x)) / 128,
            18 => (x - y).abs(),
            _ => x, // 0 and 1
        };
        c as u8
    })
}

/// Two values by the mode.
fn values(mode: i64, a: f64, b: f64, w: f64, o: &TextureOrders) -> f64 {
    match mode {
        2 => (a + b) / 2.0,
        3 => a * b,
        4 => if a < b { b } else { a },
        5 => if a < b { a } else { b },
        6 => clamp(o.value6.sum([a, b, -0.5])),
        7 => clamp(a - b),
        17 if a < 0.5 => 2.0 * a * b,
        17 => 1.0 - 2.0 * (1.0 - a) * (1.0 - b),
        18 => (b - a).abs(),
        8..=16 => a + (b - a) * w,
        _ => a, // 0 and 1
    }
}

/// Two gradients by the mode, axis by axis.
fn gradients(mode: i64, g: [f64; 3], h: [f64; 3], a: f64, b: f64, w: f64) -> [f64; 3] {
    match mode {
        4 => return if a < b { h } else { g },
        5 => return if b < a { h } else { g },
        _ => {}
    }
    std::array::from_fn(|k| {
        let (g, h) = (g[k], h[k]);
        match mode {
            2 => (g + h) / 2.0,
            3 => g * h,
            6 => (g + h).clamp(-1.0, 1.0),
            7 | 18 => g - h,
            17 if g < 0.0 => g * h,
            17 => 1.0 - (1.0 - g) * (1.0 - h),
            8..=16 => g + (h - g) * w,
            _ => g, // 0 and 1
        }
    })
}

/// One blend of the new component b into the accumulated side.
#[allow(clippy::too_many_arguments)]
fn blend(
    acc: &mut Accumulated,
    b: &ComponentResult,
    b_space: ColourSpace,
    mode: i64,
    s: SurfaceContext,
    version: RenderAs,
    o: &TextureOrders,
    pn: &PermNoise<'_>,
) -> Result<(), OpenPoint> {
    let (pa, pb) = (acc.produces, b.produces);
    let (av, bv) = (acc.value, b.value);
    let w = weight(mode, av, bv, s, o, pn)?;
    let only = |p: OutputSet, colour: bool, value: bool, bump: bool| {
        p.colour == colour && p.value == value && p.bump == bump
    };

    // The colour.
    if pa.colour && pb.colour {
        let x = converted(acc.colour, acc.space, b_space, version)?;
        let y = b.colour.expect("produced");
        acc.colour = if mode == 1 {
            map_colour(&ColourMap { a: x, ..b.map }, bv, s, o.ease, pn)?
        } else {
            colours(mode, x, y, av, bv, w)
        };
    } else if pb.colour {
        acc.colour = b.colour.expect("produced");
    } else if only(pa, true, false, false) && mode != 0 && (pb.value || pb.bump) {
        let x = converted(acc.colour, acc.space, b_space, version)?;
        acc.colour = if pb.bump {
            if b.kept {
                return Err(OpenPoint::BumpOffKept);
            }
            let y = bump_colour(b.gradient.expect("produced"), version);
            if mode == 1 { x } else { colours(mode, x, y, av, bv, w) }
        } else {
            let grey = held(cut(255.0 * bv));
            let y = [grey; 3];
            match mode {
                1 | 4 | 5 => x,
                _ => colours(mode, x, y, av, bv, w),
            }
        };
    }

    // The bump.
    if pa.bump && pb.bump {
        acc.kept |= b.kept;
        acc.gradient = gradients(mode, acc.gradient, b.gradient.unwrap_or([0.0; 3]), av, bv, w);
    } else if pb.bump {
        acc.kept = b.kept;
        let h = b.gradient.unwrap_or([0.0; 3]);
        acc.gradient = if mode > 1 && pa.value && only(pb, false, false, true) {
            gradients(mode, [av; 3], h, av, bv, w)
        } else {
            h
        };
    } else if only(pa, false, false, true) && pb.value {
        acc.gradient = gradients(mode, acc.gradient, [bv; 3], av, bv, w);
    }

    // The value.
    acc.value = if pb.value && !pa.value {
        bv
    } else if pa.value && !pb.value {
        av
    } else {
        values(mode, av, bv, w, o)
    };

    acc.produces = union(pa, pb);
    acc.space = b_space;
    Ok(())
}

/// The texture's outputs from its components' results. A component that produces no bump
/// gives the gradient (0, 0, 0) (as 3.1 and 5.0; as 3.0 the kept one,: the cases of the
/// vectors do not depend on it).,
pub fn finish(
    settings: &TextureSettings,
    results: &[ComponentResult],
    s: SurfaceContext,
    version: RenderAs,
    o: &TextureOrders,
    pn: &PermNoise<'_>,
    kept: Option<&super::Kept>,
) -> Result<super::TextureResult, OpenPoint> {
    let first = &results[0];
    let mut acc = Accumulated {
        produces: first.produces,
        value: first.value,
        colour: first.colour.unwrap_or([0; 3]),
        gradient: first.gradient.unwrap_or([0.0; 3]),
        kept: first.kept,
        space: settings.components[0].colours.space,
    };
    for (i, b) in results.iter().enumerate().skip(1) {
        blend(&mut acc, b, settings.components[i].colours.space, settings.blend[i - 1], s, version, o, pn)?;
    }

    let asked = settings.outputs;
    let p = acc.produces;
    let mut v = acc.value;
    if p.value {
        v = filtered(&settings.filter, v, s, version, o.smoothstep).ok_or(OpenPoint::Filter(settings.filter.number))?;
    }
    let tex_map = &settings.colours;
    let (mut c, mut value, mut g) = (acc.colour, v, acc.gradient);
    if !p.value && !p.colour && !p.bump {
        // : no component produces any output, so the colour
        // asked is the colour the texture holds (the result as saved), and as 3.0 the gradient
        // asked is the one it holds; as 3.1 and 5.0 every evaluation writes a gradient (0, 0, 0).
        if let Some(k) = kept {
            c = k.colour;
            if version == RenderAs::Bryce30 {
                g = k.gradient;
            }
        }
    } else if p.value && !p.colour {
        if asked.colour {
            c = map_colour(tex_map, v, s, o.ease, pn)?;
        }
        if !p.bump {
            g = [v; 3];
        }
    } else if p.colour && !p.value && !p.bump {
        value = (f64::from(c[0]) + f64::from(c[1]) + f64::from(c[2])) * THIRD_OF_255;
        g = c.map(|k| (f64::from(k) - 128.0) / 256.0);
    } else if p.colour && p.value && !p.bump {
        g = c.map(|k| (f64::from(k) - 128.0) / 256.0);
    } else if p.bump && !p.colour && !p.value {
        if acc.kept {
            // (a first component whose bump is off gives as 3.0 the gradient the
            // texture kept), (before any evaluation a texture holds the result as saved;
            // an evaluation that makes no bump writes none): the saved gradient, for one
            // component; a blend with a later component's unset gradient stays refused.
            match kept {
                Some(k) if results.len() == 1 => g = k.gradient,
                _ => return Err(OpenPoint::BumpOffKept),
            }
        }
        c = bump_colour(g, version);
        value = clamp(o.bump_value.sum([g[0], g[1], g[2], 1.5]) * THIRD);
    }

    if asked.colour {
        // The quarter move toward the texture's map, at the value as it now stands.
        let only_bump = p.bump && !p.colour && !p.value;
        let nothing = !p.bump && !p.colour && !p.value && kept.is_some();
        if (p.colour || only_bump || nothing) && tex_map.mode != 0 {
            let at = if asked.value { value } else { v };
            let m = map_colour(tex_map, at, s, o.ease, pn)?;
            c = std::array::from_fn(|k| {
                let x = i64::from(c[k]);
                (x + cut((f64::from(m[k]) - x as f64) * 0.25)) as u8
            });
        }
        c = final_space(settings, c, version)?;
    }
    Ok(super::TextureResult {
        colour: asked.colour.then_some(c),
        value: asked.value.then_some(value),
        bump: asked.bump.then_some(g),
    })
}

/// The colour space conversion at the end.
fn final_space(settings: &TextureSettings, c: Colour, version: RenderAs) -> Result<Colour, OpenPoint> {
    let comps = &settings.components;
    let counted = if comps.len() == 1 { &comps[..] } else { &comps[1..] };
    match settings.colours.space {
        ColourSpace::Rgb => {
            if counted.iter().any(|k| k.colours.mode != 0 && k.colours.space == ColourSpace::Hls) {
                from_hls_bytes(c, version)
            } else {
                Ok(c)
            }
        }
        ColourSpace::Hls => {
            let last = counted.last().expect("a component");
            if last.colours.mode != 0 && last.colours.space == ColourSpace::Rgb {
                to_hls_bytes(c)
            } else {
                from_hls_bytes(c, version)
            }
        }
    }
}
