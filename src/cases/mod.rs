// SPDX-License-Identifier: MIT
//! The case command's work: one line of a case file to one result line.
//!
//! (Case files; Kind `texture`; Kind `picture`; Kind `tree`; result lines)

pub mod json;
pub mod picture;
pub mod texture;
pub mod tree;

use std::fmt::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use crate::substrate::Substrate;
use crate::version::RenderAs;

pub use json::{ParseError, Value};

/// What a case's computation gave.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// The case is not computed; `why` goes to standard error only.
    Refused { why: String },
    /// A picture: the SHA-256 of its levels, 64 lowercase hexadecimal digits.
    /// (Kind `picture`)
    Levels(String),
    /// A texture's asked outputs. (Kind `texture`)
    Texture(TextureOutputs),
    /// A tree's counts and the SHA-256 of its text. (Kind `tree`)
    Tree(tree::TreeResult),
}

impl Outcome {
    pub fn refused(why: impl Into<String>) -> Outcome {
        Outcome::Refused { why: why.into() }
    }
}

/// The outputs a texture case asks for, each present only when asked.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextureOutputs {
    pub colour: Option<[u8; 3]>,
    pub value: Option<f64>,
    pub bump: Option<[f64; 3]>,
}

/// One answered case: its identity and its outcome.
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    pub case: String,
    pub outcome: Outcome,
}

impl Answer {
    /// The result line, without its line end. (result lines)
    pub fn line(&self) -> String {
        match &self.outcome {
            Outcome::Refused { .. } => format!("{} refused", self.case),
            Outcome::Levels(hex) => format!("{} levels={hex}", self.case),
            Outcome::Texture(t) => texture_line(&self.case, t),
            Outcome::Tree(t) => format!("{} tree={},{},{},{}", self.case, t.pieces, t.places, t.leaves, t.sha256_hex),
        }
    }
}

/// `<case> colour=<6 hex> value=<16 hex> bump=<16 hex>,<16 hex>,<16 hex>`, only the outputs
/// present, in that order, single spaces. (Kind `texture`, result line)
pub fn texture_line(case: &str, t: &TextureOutputs) -> String {
    let mut s = String::from(case);
    if let Some([r, g, b]) = t.colour {
        let _ = write!(s, " colour={r:02x}{g:02x}{b:02x}");
    }
    if let Some(v) = t.value {
        let _ = write!(s, " value={:016x}", v.to_bits());
    }
    if let Some([x, y, z]) = t.bump {
        let _ = write!(s, " bump={:016x},{:016x},{:016x}", x.to_bits(), y.to_bits(), z.to_bits());
    }
    s
}

/// Why a line of the case file gets no result line: its identity cannot be known.
#[derive(Clone, Debug, PartialEq)]
pub enum LineError {
    NotJson(ParseError),
    NoCase,
    /// An identity holding a line break or nothing cannot be written as one line.
    UnwritableCase,
}

impl std::fmt::Display for LineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LineError::NotJson(e) => write!(f, "not JSON: {e}"),
            LineError::NoCase => write!(f, "no string field \"case\""),
            LineError::UnwritableCase => write!(f, "the case identity is empty or holds a line break"),
        }
    }
}

/// What every case of a run shares.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    /// The folder of the case file: picture documents are relative to it.
    pub case_folder: &'a Path,
    /// The substrate, when it could be read .
    pub substrate: Option<&'a Substrate>,
}

/// The version a case asks for. (`version`: 3.0, 3.1 or 5.0)
pub fn version_of(text: &str) -> Option<RenderAs> {
    match text {
        "3.0" => Some(RenderAs::Bryce30),
        "3.1" => Some(RenderAs::Bryce31),
        "5.0" => Some(RenderAs::Bryce50),
        _ => None,
    }
}

/// Answer one non-blank line of a case file. (Case files: unknown
/// fields ignored; an unknown kind refused)
pub fn answer(line: &str, ctx: Context<'_>) -> Result<Answer, LineError> {
    let v = json::parse(line).map_err(LineError::NotJson)?;
    let case = v.get("case").and_then(Value::as_str).ok_or(LineError::NoCase)?;
    if case.is_empty() || case.contains(['\n', '\r']) {
        return Err(LineError::UnwritableCase);
    }
    // A panic here is our bug; the case is refused and the run goes on .
    let outcome = catch_unwind(AssertUnwindSafe(|| compute(&v, ctx)))
        .unwrap_or_else(|_| Outcome::refused("internal error (panic) while computing the case"));
    Ok(Answer { case: case.to_owned(), outcome })
}

fn compute(v: &Value, ctx: Context<'_>) -> Outcome {
    let Some(kind) = v.get("kind").and_then(Value::as_str) else {
        return Outcome::refused("no kind");
    };
    let Some(version) = v.get("version").and_then(Value::as_str).and_then(version_of) else {
        return Outcome::refused("no known version (3.0, 3.1, 5.0)");
    };
    match kind {
        "picture" => picture::compute(v, version, ctx),
        "texture" => texture::compute(v, version, ctx.substrate),
        // (kind tree).
        "tree" => tree::compute(v, version, ctx.substrate),
        other => Outcome::refused(format!("unknown kind {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context<'static> {
        Context { case_folder: Path::new("."), substrate: None }
    }

    #[test]
    fn refusal_line() {
        let a = Answer { case: "c-1".into(), outcome: Outcome::refused("x") };
        assert_eq!(a.line(), "c-1 refused");
    }

    #[test]
    fn levels_line() {
        let hex = "0123456789abcdef".repeat(4);
        let a = Answer { case: "pic-9".into(), outcome: Outcome::Levels(hex.clone()) };
        assert_eq!(a.line(), format!("pic-9 levels={hex}"));
    }

    #[test]
    fn tree_line() {
        let hex = "0123456789abcdef".repeat(4);
        let t = tree::TreeResult { pieces: 81_891, places: 1, leaves: 0, sha256_hex: hex.clone() };
        let a = Answer { case: "tree-7".into(), outcome: Outcome::Tree(t) };
        assert_eq!(a.line(), format!("tree-7 tree=81891,1,0,{hex}"));
    }

    #[test]
    fn texture_lines_only_asked_outputs_in_order() {
        let all = TextureOutputs { colour: Some([0x80, 0x0a, 0xff]), value: Some(1.0), bump: Some([-0.0, 2.0, f64::NAN]) };
        assert_eq!(
            texture_line("t", &all),
            "t colour=800aff value=3ff0000000000000 bump=8000000000000000,4000000000000000,7ff8000000000000"
        );
        let v = TextureOutputs { value: Some(0.5), ..Default::default() };
        assert_eq!(texture_line("t", &v), "t value=3fe0000000000000");
        let c = TextureOutputs { colour: Some([0, 0, 0]), ..Default::default() };
        assert_eq!(texture_line("t", &c), "t colour=000000");
        let b = TextureOutputs { bump: Some([0.0; 3]), ..Default::default() };
        assert_eq!(texture_line("t", &b), "t bump=0000000000000000,0000000000000000,0000000000000000");
    }

    #[test]
    fn unknown_kind_and_version_refused() {
        let a = answer(r#"{"case":"k","kind":"sound","version":"3.0"}"#, ctx()).unwrap();
        assert_eq!(a.line(), "k refused");
        let a = answer(r#"{"case":"v","kind":"picture","version":"4.0","document":"x.br3"}"#, ctx()).unwrap();
        assert_eq!(a.line(), "v refused");
        let a = answer(r#"{"case":"n","version":"3.0"}"#, ctx()).unwrap();
        assert_eq!(a.line(), "n refused");
    }

    #[test]
    fn unknown_fields_ignored() {
        let a = answer(r#"{"case":"u","kind":"texture","version":"5.0","extra":{"x":[1,2]},"tables":["perm"]}"#, ctx())
            .unwrap();
        assert_eq!(a.case, "u");
        assert_eq!(a.line(), "u refused");
    }

    #[test]
    fn lines_without_identity() {
        assert!(matches!(answer("{", ctx()), Err(LineError::NotJson(_))));
        assert_eq!(answer(r#"{"kind":"picture"}"#, ctx()), Err(LineError::NoCase));
        assert_eq!(answer(r#"{"case":7}"#, ctx()), Err(LineError::NoCase));
        assert_eq!(answer(r#"{"case":"a\nb"}"#, ctx()), Err(LineError::UnwritableCase));
        assert_eq!(answer(r#"{"case":""}"#, ctx()), Err(LineError::UnwritableCase));
    }

    #[test]
    fn versions() {
        assert_eq!(version_of("3.0"), Some(RenderAs::Bryce30));
        assert_eq!(version_of("3.1"), Some(RenderAs::Bryce31));
        assert_eq!(version_of("5.0"), Some(RenderAs::Bryce50));
        assert_eq!(version_of("5"), None);
    }

    #[test]
    fn picture_without_substrate_or_document_refused() {
        let a = answer(r#"{"case":"p","kind":"picture","version":"3.0","document":"none.br3"}"#, ctx()).unwrap();
        assert_eq!(a.line(), "p refused");
        let a = answer(r#"{"case":"q","kind":"picture","version":"3.0"}"#, ctx()).unwrap();
        assert_eq!(a.line(), "q refused");
    }
}
