// SPDX-License-Identifier: MIT
//! The case command: `stecyk3d-cases <case file> <substrate file>`.
//!
//! Reads a case file (JSON Lines), writes one result line per case to standard output, and
//! ends with status 0 when every case got a line (a refused case counts).
//!
//! (What the program provides; Case files; result lines);
//! (Arguments and exit status)

use std::io::{BufWriter, Write};
use std::path::Path;
use std::process::ExitCode;

use stecyk3d::cases::{self, Context};
use stecyk3d::substrate;

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let [case_file, substrate_file] = args.as_slice() else {
        eprintln!("usage: stecyk3d-cases <case file> <substrate file> (got {} arguments)", args.len());
        return ExitCode::from(2);
    };
    let case_path = Path::new(case_file);
    let text = match std::fs::read(case_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("stecyk3d-cases: cannot read the case file {}: {e}", case_path.display());
            return ExitCode::from(2);
        }
    };
    let loaded = substrate::load(Path::new(substrate_file));
    if let Err(e) = &loaded {
        eprintln!("stecyk3d-cases: the substrate cannot be used, cases that need it are refused: {e}");
    }
    let ctx = Context {
        case_folder: case_path.parent().unwrap_or(Path::new(".")),
        substrate: loaded.as_ref().ok(),
    };

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let mut unanswered = 0usize;
    for (n, raw) in text.split(|&b| b == b'\n').enumerate() {
        let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
        if raw.iter().all(|b| b.is_ascii_whitespace()) {
            continue;
        }
        let answered = match std::str::from_utf8(raw) {
            Ok(line) => cases::answer(line, ctx).map_err(|e| e.to_string()),
            Err(_) => Err("not UTF-8".to_owned()),
        };
        match answered {
            Ok(a) => {
                if let cases::Outcome::Refused { why } = &a.outcome {
                    eprintln!("{}: refused: {why}", a.case);
                }
                if let Err(e) = writeln!(out, "{}", a.line()) {
                    eprintln!("stecyk3d-cases: cannot write: {e}");
                    return ExitCode::from(3);
                }
            }
            Err(e) => {
                eprintln!("stecyk3d-cases: line {}: no result line can be written: {e}", n + 1);
                unanswered += 1;
            }
        }
    }
    if let Err(e) = out.flush() {
        eprintln!("stecyk3d-cases: cannot write: {e}");
        return ExitCode::from(3);
    }
    if unanswered > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS }
}
