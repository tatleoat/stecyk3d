// SPDX-License-Identifier: MIT
// A small JSON reader for the vector tests, and the vector files' number encodings.
// tests only (DESIGN.md: no dependencies)
#![allow(dead_code)]

use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(BTreeMap<String, Json>),
}

impl Json {
    pub fn get(&self, k: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.get(k),
            _ => None,
        }
    }
    pub fn at(&self, k: &str) -> &Json {
        self.get(k).unwrap_or_else(|| panic!("missing key {k}"))
    }
    pub fn arr(&self) -> &[Json] {
        match self {
            Json::Arr(a) => a,
            _ => panic!("not an array"),
        }
    }
    pub fn str(&self) -> &str {
        match self {
            Json::Str(s) => s,
            _ => panic!("not a string"),
        }
    }
    pub fn num(&self) -> f64 {
        match self {
            Json::Num(n) => *n,
            _ => panic!("not a number"),
        }
    }
    /// A binary64 given as 16 hexadecimal digits.
    pub fn f64(&self) -> f64 {
        f64::from_bits(u64::from_str_radix(self.str(), 16).expect("hex bits"))
    }
    /// A binary32 given as 8 hexadecimal digits.
    pub fn f32(&self) -> f32 {
        f32::from_bits(u32::from_str_radix(self.str(), 16).expect("hex bits"))
    }
    /// A switch word given as 8 hexadecimal digits.
    pub fn word(&self) -> u32 {
        u32::from_str_radix(self.str(), 16).expect("hex word")
    }
    pub fn vec3(&self) -> [f64; 3] {
        let a = self.arr();
        [a[0].f64(), a[1].f64(), a[2].f64()]
    }
    pub fn rgb(&self) -> [f32; 3] {
        let a = self.arr();
        [a[0].f32(), a[1].f32(), a[2].f32()]
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn expect(&mut self, c: u8) {
        self.ws();
        assert_eq!(self.s[self.i], c, "JSON: expected {}", c as char);
        self.i += 1;
    }
    fn value(&mut self) -> Json {
        self.ws();
        match self.s[self.i] {
            b'{' => {
                self.i += 1;
                let mut m = BTreeMap::new();
                loop {
                    self.ws();
                    if self.s[self.i] == b'}' {
                        self.i += 1;
                        return Json::Obj(m);
                    }
                    let k = match self.value() {
                        Json::Str(s) => s,
                        _ => panic!("JSON: key"),
                    };
                    self.expect(b':');
                    let v = self.value();
                    m.insert(k, v);
                    self.ws();
                    if self.s[self.i] == b',' {
                        self.i += 1;
                    }
                }
            }
            b'[' => {
                self.i += 1;
                let mut a = Vec::new();
                loop {
                    self.ws();
                    if self.s[self.i] == b']' {
                        self.i += 1;
                        return Json::Arr(a);
                    }
                    a.push(self.value());
                    self.ws();
                    if self.s[self.i] == b',' {
                        self.i += 1;
                    }
                }
            }
            b'"' => {
                self.i += 1;
                let st = self.i;
                while self.s[self.i] != b'"' {
                    self.i += 1;
                }
                let s = String::from_utf8(self.s[st..self.i].to_vec()).unwrap();
                self.i += 1;
                Json::Str(s)
            }
            b't' => {
                self.i += 4;
                Json::Bool(true)
            }
            b'f' => {
                self.i += 5;
                Json::Bool(false)
            }
            b'n' => {
                self.i += 4;
                Json::Null
            }
            _ => {
                let st = self.i;
                while self.i < self.s.len() && b"+-0123456789.eE".contains(&self.s[self.i]) {
                    self.i += 1;
                }
                Json::Num(std::str::from_utf8(&self.s[st..self.i]).unwrap().parse().unwrap())
            }
        }
    }
}

pub fn parse(line: &str) -> Json {
    Parser { s: line.as_bytes(), i: 0 }.value()
}

/// The non-empty lines of a vector file, or `None` (the test skips) when it is missing.
pub fn read_lines(path: &str) -> Option<Vec<String>> {
    match std::fs::read_to_string(path) {
        Ok(t) => Some(t.lines().filter(|l| !l.trim().is_empty()).map(str::to_string).collect()),
        Err(_) => {
            eprintln!("skipping: {path} not found");
            None
        }
    }
}

pub const VERSIONS: [&str; 3] = ["3.0", "3.1", "5.0"];

pub fn version(s: &str) -> stecyk3d::version::RenderAs {
    use stecyk3d::version::RenderAs;
    match s {
        "3.0" => RenderAs::Bryce30,
        "3.1" => RenderAs::Bryce31,
        "5.0" => RenderAs::Bryce50,
        other => panic!("unknown program {other}"),
    }
}

pub fn bits3_32(c: [f32; 3]) -> [u32; 3] {
    c.map(f32::to_bits)
}

pub fn bits3_64(c: [f64; 3]) -> [u64; 3] {
    c.map(f64::to_bits)
}
