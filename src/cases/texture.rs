// SPDX-License-Identifier: MIT
//! Kind `texture`: one evaluation of a procedural texture .
//!
//! The case's `texture`, `point` and `surface` are read into `textures::TextureSettings` (the
//! vectors of use the same form, so the vector test reads them with
//! `parse_case` too), then `textures::evaluate` runs with the substrate's tables borrowed. A case
//! that reaches a stage the released entries still leave Open (the lattice noises and the types
//! built on them, type 17, Bryce 5's types, method 11 over a noise, colour maps, blends, bump) is
//! refused, with the Open points on standard error; the rest are computed. Nothing is guessed.
//!
//! (Kind `texture`: fields, numbers as bit patterns, result line,
//! `<case> refused`);
//! (Case command),

use super::{Outcome, TextureOutputs, Value};
use crate::substrate::Substrate;
use crate::textures::{
    self, ColourMap, ColourSpace, Component, Dimension, Filter, NoiseSettings, NoiseSpace, OutputSet, Phase,
    SurfaceContext, TextureSettings, TextureTables,
};
use crate::version::RenderAs;

/// One texture case's inputs.
#[derive(Clone, Debug, PartialEq)]
pub struct TextureCase {
    pub settings: TextureSettings,
    pub point: [f64; 3],
    pub surface: SurfaceContext,
}

/// Compute a texture case. (Kind `texture`)
pub fn compute(v: &Value, version: RenderAs, substrate: Option<&Substrate>) -> Outcome {
    let case = match parse_case(v) {
        Ok(c) => c,
        Err(e) => return Outcome::refused(format!("texture case not understood: {e}")),
    };
    let tables = TextureTables::from_substrate(substrate);
    match textures::evaluate(&case.settings, case.point, case.surface, tables, version) {
        Ok(r) => Outcome::Texture(TextureOutputs { colour: r.colour, value: r.value, bump: r.bump }),
        Err(e) => Outcome::refused(e.to_string()),
    }
}

/// Read `texture`, `point` and `surface` of a case (or of a vector line).
pub fn parse_case(v: &Value) -> Result<TextureCase, String> {
    let t = field(v, "texture")?;
    let settings = parse_texture(t)?;
    let point = doubles3(field(v, "point")?).map_err(|e| format!("point: {e}"))?;
    let [altitude, slope, orientation] = doubles3(field(v, "surface")?).map_err(|e| format!("surface: {e}"))?;
    Ok(TextureCase { settings, point, surface: SurfaceContext { altitude, slope, orientation } })
}

/// Read a `texture` object. (Kind `texture`, field `texture`)
pub fn parse_texture(t: &Value) -> Result<TextureSettings, String> {
    let outputs = outputs(field(t, "outputs")?)?;
    let list = field(t, "components")?.as_array().ok_or("components: not a list")?;
    let mut components = Vec::with_capacity(list.len());
    for (i, c) in list.iter().enumerate() {
        components.push(parse_component(c).map_err(|e| format!("component {}: {e}", i + 1))?);
    }
    let blend_list = field(t, "blend")?.as_array().ok_or("blend: not a list")?;
    let [b0, b1] = blend_list else {
        return Err(format!("blend: {} numbers, not 2", blend_list.len()));
    };
    let blend = [whole(b0).map_err(|e| format!("blend: {e}"))?, whole(b1).map_err(|e| format!("blend: {e}"))?];
    Ok(TextureSettings {
        outputs,
        components,
        blend,
        phase: parse_phase(field(t, "phase")?).map_err(|e| format!("phase: {e}"))?,
        filter: parse_filter(field(t, "filter")?).map_err(|e| format!("filter: {e}"))?,
        colours: parse_colours(field(t, "colours")?).map_err(|e| format!("colours: {e}"))?,
    })
}

fn parse_component(c: &Value) -> Result<Component, String> {
    Ok(Component {
        outputs: outputs(field(c, "outputs")?)?,
        noise: parse_noise(field(c, "noise")?).map_err(|e| format!("noise: {e}"))?,
        space: parse_space(c)?,
        phase: parse_phase(field(c, "phase")?).map_err(|e| format!("phase: {e}"))?,
        filter: parse_filter(field(c, "filter")?).map_err(|e| format!("filter: {e}"))?,
        colours: parse_colours(field(c, "colours")?).map_err(|e| format!("colours: {e}"))?,
    })
}

fn parse_noise(n: &Value) -> Result<NoiseSettings, String> {
    Ok(NoiseSettings {
        type_number: whole(field(n, "type")?)?,
        method: whole(field(n, "method")?)?,
        octaves: whole(field(n, "octaves")?)?,
        dimension: Dimension::from_number(whole(field(n, "dimension")?)?),
    })
}

/// `angles` and `frequencies` of the object (a component or a phase). inputs
fn parse_space(o: &Value) -> Result<NoiseSpace, String> {
    let a = numbers(field(o, "angles")?).map_err(|e| format!("angles: {e}"))?;
    let f = numbers(field(o, "frequencies")?).map_err(|e| format!("frequencies: {e}"))?;
    let angles: [f64; 2] = a.try_into().map_err(|a: Vec<f64>| format!("angles: {} numbers, not 2", a.len()))?;
    let frequencies: [f64; 3] =
        f.try_into().map_err(|f: Vec<f64>| format!("frequencies: {} numbers, not 3", f.len()))?;
    Ok(NoiseSpace { angles, frequencies })
}

fn parse_phase(p: &Value) -> Result<Phase, String> {
    Ok(Phase {
        amount: number(field(p, "amount")?).map_err(|e| format!("amount: {e}"))?,
        noise: parse_noise(field(p, "noise")?).map_err(|e| format!("noise: {e}"))?,
        space: parse_space(p)?,
    })
}

fn parse_filter(f: &Value) -> Result<Filter, String> {
    Ok(Filter {
        number: whole(field(f, "number")?)?,
        n: whole(field(f, "n")?)?,
        a: single(field(f, "a")?).map_err(|e| format!("a: {e}"))?,
        b: single(field(f, "b")?).map_err(|e| format!("b: {e}"))?,
    })
}

fn parse_colours(c: &Value) -> Result<ColourMap, String> {
    let space = match field(c, "space")?.as_str() {
        Some("rgb") => ColourSpace::Rgb,
        Some("hls") => ColourSpace::Hls,
        _ => return Err("space: not rgb or hls".into()),
    };
    Ok(ColourMap {
        mode: whole(field(c, "mode")?)?,
        space,
        a: colour(field(c, "a")?)?,
        b: colour(field(c, "b")?)?,
        c: colour(field(c, "c")?)?,
    })
}

fn field<'a>(o: &'a Value, key: &str) -> Result<&'a Value, String> {
    o.get(key).ok_or_else(|| format!("no field {key:?}"))
}

fn outputs(v: &Value) -> Result<OutputSet, String> {
    v.as_str().and_then(OutputSet::from_letters).ok_or_else(|| "outputs: not letters c, v, b".into())
}

/// A whole number written as a JSON number.
fn whole(v: &Value) -> Result<i64, String> {
    match v {
        Value::Number(t) => t.parse::<i64>().map_err(|_| format!("{t:?} is not a whole number")),
        _ => Err("not a number".into()),
    }
}

/// A JSON number (angles, frequencies, amount), read correctly rounded.
fn number(v: &Value) -> Result<f64, String> {
    match v {
        Value::Number(t) => t.parse::<f64>().map_err(|_| format!("{t:?} is not a number")),
        _ => Err("not a number".into()),
    }
}

fn numbers(v: &Value) -> Result<Vec<f64>, String> {
    v.as_array().ok_or("not a list")?.iter().map(number).collect()
}

fn hex_bits(v: &Value, digits: usize) -> Result<u64, String> {
    let s = v.as_str().ok_or("not a string")?;
    if s.len() != digits || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{s:?} is not {digits} hexadecimal digits"));
    }
    u64::from_str_radix(s, 16).map_err(|e| e.to_string())
}

/// A double as 16 hexadecimal digits of its bit pattern. (Case files)
fn double(v: &Value) -> Result<f64, String> {
    hex_bits(v, 16).map(f64::from_bits)
}

/// A single as 8 hexadecimal digits of its bit pattern. (Case files)
fn single(v: &Value) -> Result<f32, String> {
    hex_bits(v, 8).map(|b| f32::from_bits(b as u32))
}

/// A colour as 6 hexadecimal digits, red, green, blue. (Case files)
fn colour(v: &Value) -> Result<[u8; 3], String> {
    let b = hex_bits(v, 6)?;
    Ok([(b >> 16) as u8, (b >> 8) as u8, b as u8])
}

fn doubles3(v: &Value) -> Result<[f64; 3], String> {
    let list = v.as_array().ok_or("not a list")?;
    let [a, b, c] = list else {
        return Err(format!("{} numbers, not 3", list.len()));
    };
    Ok([double(a)?, double(b)?, double(c)?])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cases::json;

    const LINE: &str = r#"{"case":"t","kind":"texture","version":"5.0","texture":{"outputs":"cv","blend":[2,7],"phase":{"amount":0,"noise":{"type":0,"method":0,"octaves":0,"dimension":3},"angles":[0,0],"frequencies":[0,0,0]},"filter":{"number":0,"n":0,"a":"3f800000","b":"00000000"},"colours":{"mode":0,"space":"rgb","a":"808080","b":"000000","c":"ffffff"},"components":[{"outputs":"v","noise":{"type":4,"method":3,"octaves":5,"dimension":1},"angles":[10,-20],"frequencies":[250,2,4],"phase":{"amount":3,"noise":{"type":1,"method":0,"octaves":1,"dimension":2},"angles":[0,0],"frequencies":[1,1,1]},"filter":{"number":6,"n":2,"a":"3fc00000","b":"bf800000"},"colours":{"mode":5,"space":"hls","a":"0a0b0c","b":"000000","c":"ffffff"}}]},"point":["3ff0000000000000","c000000000000000","7ff0000000000000"],"surface":["0000000000000000","3fe0000000000000","3ff0000000000000"],"tables":["perm"]}"#;

    #[test]
    fn parses_every_field() {
        let v = json::parse(LINE).unwrap();
        let c = parse_case(&v).unwrap();
        let s = &c.settings;
        assert_eq!(s.outputs, OutputSet { colour: true, value: true, bump: false });
        assert_eq!(s.blend, [2, 7]);
        assert!(s.phase.is_none());
        let k = &s.components[0];
        assert_eq!(k.noise, NoiseSettings { type_number: 4, method: 3, octaves: 5, dimension: Dimension::One });
        assert_eq!(k.space, NoiseSpace { angles: [10.0, -20.0], frequencies: [250.0, 2.0, 4.0] });
        assert_eq!(k.phase.amount, 3.0);
        assert!(!k.phase.is_none());
        assert_eq!(k.filter, Filter { number: 6, n: 2, a: 1.5, b: -1.0 });
        assert_eq!(k.colours.space, ColourSpace::Hls);
        assert_eq!(k.colours.a, [10, 11, 12]);
        assert_eq!(c.point, [1.0, -2.0, f64::INFINITY]);
        assert_eq!(c.surface, SurfaceContext { altitude: 0.0, slope: 0.5, orientation: 1.0 });
    }

    #[test]
    fn refused_with_open_points() {
        let v = json::parse(LINE).unwrap();
        match compute(&v, RenderAs::Bryce50, None) {
            Outcome::Refused { why } => {
                // Type 4 reads `perm` , which no substrate gives here .
                assert!(why.contains("SUB-8 the substrate lacks the table perm"), "{why}");
                assert!(!why.contains("TX-6.1"), "{why}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn malformed_refused() {
        let v = json::parse(&LINE.replace("\"3fc00000\"", "\"3fc0\"")).unwrap();
        match compute(&v, RenderAs::Bryce50, None) {
            Outcome::Refused { why } => assert!(why.contains("not understood"), "{why}"),
            other => panic!("{other:?}"),
        }
    }
}
