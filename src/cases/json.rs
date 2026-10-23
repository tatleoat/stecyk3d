// SPDX-License-Identifier: MIT
//! A small JSON reader for case files: one value per line, errors reported, never a panic.
//!
//! (Parsing a case; std only, no new dependency)
//! (Case files: UTF-8 JSON Lines)

use std::collections::BTreeMap;

/// A JSON value. Numbers are kept as their text: the case files write doubles as hexadecimal
/// strings, and whole numbers are read only when a field needs them.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    /// The member `key` of an object.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(m) => m.get(key),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }
}

/// Why a line is not JSON: a message and the byte offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub at: usize,
    pub message: &'static str,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at byte {}", self.message, self.at)
    }
}

/// Nesting deeper than this is refused (no case file nests more than a few levels).
const MAX_DEPTH: usize = 64;

/// Parse one complete JSON text (surrounding white space allowed).
pub fn parse(text: &str) -> Result<Value, ParseError> {
    let mut p = Parser { s: text.as_bytes(), i: 0 };
    p.space();
    let v = p.value(0)?;
    p.space();
    if p.i != p.s.len() {
        return Err(p.err("text after the value"));
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn err(&self, message: &'static str) -> ParseError {
        ParseError { at: self.i, message }
    }

    fn space(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.s.get(self.i) {
            self.i += 1;
        }
    }

    fn eat(&mut self, b: u8) -> bool {
        if self.s.get(self.i) == Some(&b) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn literal(&mut self, word: &[u8], v: Value) -> Result<Value, ParseError> {
        if self.s[self.i..].starts_with(word) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(self.err("unknown word"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, ParseError> {
        if depth > MAX_DEPTH {
            return Err(self.err("nested too deeply"));
        }
        match self.s.get(self.i) {
            None => Err(self.err("unexpected end")),
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Value::String),
            Some(b't') => self.literal(b"true", Value::Bool(true)),
            Some(b'f') => self.literal(b"false", Value::Bool(false)),
            Some(b'n') => self.literal(b"null", Value::Null),
            Some(b'-' | b'0'..=b'9' | b'N' | b'I') => self.number(),
            Some(_) => Err(self.err("unexpected character")),
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, ParseError> {
        self.i += 1;
        let mut m = BTreeMap::new();
        self.space();
        if self.eat(b'}') {
            return Ok(Value::Object(m));
        }
        loop {
            self.space();
            if self.s.get(self.i) != Some(&b'"') {
                return Err(self.err("expected a member name"));
            }
            let k = self.string()?;
            self.space();
            if !self.eat(b':') {
                return Err(self.err("expected ':'"));
            }
            self.space();
            let v = self.value(depth + 1)?;
            m.insert(k, v);
            self.space();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b'}') {
                return Ok(Value::Object(m));
            }
            return Err(self.err("expected ',' or '}'"));
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, ParseError> {
        self.i += 1;
        let mut a = Vec::new();
        self.space();
        if self.eat(b']') {
            return Ok(Value::Array(a));
        }
        loop {
            self.space();
            a.push(self.value(depth + 1)?);
            self.space();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b']') {
                return Ok(Value::Array(a));
            }
            return Err(self.err("expected ',' or ']'"));
        }
    }

    fn hex4(&mut self) -> Result<u32, ParseError> {
        let h = self.s.get(self.i..self.i + 4).ok_or_else(|| self.err("short \\u escape"))?;
        let t = std::str::from_utf8(h).map_err(|_| self.err("bad \\u escape"))?;
        let v = u32::from_str_radix(t, 16).map_err(|_| self.err("bad \\u escape"))?;
        self.i += 4;
        Ok(v)
    }

    fn string(&mut self) -> Result<String, ParseError> {
        self.i += 1; // the opening quote
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(&b) = self.s.get(self.i) else { return Err(self.err("unterminated string")) };
            self.i += 1;
            match b {
                b'"' => break,
                b'\\' => {
                    let Some(&e) = self.s.get(self.i) else { return Err(self.err("unterminated escape")) };
                    self.i += 1;
                    let c = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hi = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&hi) {
                                if !(self.eat(b'\\') && self.eat(b'u')) {
                                    return Err(self.err("lone surrogate"));
                                }
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return Err(self.err("bad surrogate pair"));
                                }
                                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                            } else {
                                hi
                            };
                            char::from_u32(code).ok_or_else(|| self.err("bad code point"))?
                        }
                        _ => return Err(self.err("unknown escape")),
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                0x00..=0x1f => return Err(self.err("control character in string")),
                _ => out.push(b),
            }
        }
        String::from_utf8(out).map_err(|_| self.err("string is not UTF-8"))
    }

    fn number(&mut self) -> Result<Value, ParseError> {
        let start = self.i;
        self.eat(b'-');
        // The tokens Python's json writes for numbers that are not finite (the Blender add-on's description).
        for token in [&b"NaN"[..], &b"Infinity"[..]] {
            if self.s[self.i..].starts_with(token) {
                self.i += token.len();
                return Ok(Value::Number(String::from_utf8_lossy(&self.s[start..self.i]).into_owned()));
            }
        }
        let digits = |p: &mut Self| {
            let s = p.i;
            while let Some(b'0'..=b'9') = p.s.get(p.i) {
                p.i += 1;
            }
            p.i - s
        };
        if digits(self) == 0 {
            return Err(self.err("expected digits"));
        }
        if self.eat(b'.') && digits(self) == 0 {
            return Err(self.err("expected digits after '.'"));
        }
        if self.eat(b'e') || self.eat(b'E') {
            let _ = self.eat(b'+') || self.eat(b'-');
            if digits(self) == 0 {
                return Err(self.err("expected exponent digits"));
            }
        }
        // The slice is ASCII digits and signs only.
        Ok(Value::Number(String::from_utf8_lossy(&self.s[start..self.i]).into_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_case_line() {
        let v = parse(r#"{"case":"a-1","kind":"picture","n":[1,-2.5e3,true,null],"o":{}}"#).unwrap();
        assert_eq!(v.get("case").and_then(Value::as_str), Some("a-1"));
        assert_eq!(v.get("n").and_then(Value::as_array).map(<[Value]>::len), Some(4));
        assert_eq!(v.get("n").unwrap().as_array().unwrap()[1], Value::Number("-2.5e3".into()));
    }

    #[test]
    fn escapes() {
        let v = parse(r#""a\"b\\c\u00e9\ud83d\ude00\n""#).unwrap();
        assert_eq!(v, Value::String("a\"b\\c\u{e9}\u{1F600}\n".into()));
    }

    #[test]
    fn errors_not_panics() {
        for bad in ["", "{", "{\"a\"}", "[1,]", "{\"a\":1}x", "\"abc", "tru", "-", "1.", "\"\\x\"", "{1:2}"] {
            assert!(parse(bad).is_err(), "{bad:?} should not parse");
        }
        let deep = "[".repeat(1000);
        assert!(parse(&deep).is_err());
    }
}
