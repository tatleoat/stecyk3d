// SPDX-License-Identifier: MIT
//! Make the substrate file and the folder of kept files from the user's own installs.
//!
//!     stecyk3d-extract [--out SUBSTRATE.bin] [--bryce 3.0=FOLDER ...] [INSTALL_FOLDER ...]
//!
//! A folder may be the install itself or hold it up to two levels below; `--bryce V=FOLDER` also
//! refuses a folder that holds another version than V.
//!
//! Writes the substrate (format version 4) at the path given (where none is given, the default
//! place of: `substrate.bin` in the program's folder of the user's application data) and
//! the folder `SUBSTRATE.files` beside it, only when no folder is refused; every refusal is printed to standard error and
//! the exit status is 1. The report says what each folder was recognised as and what was taken.
//!
//! (the command: its name and options are free; where no output path is given, the
//! default place of)
//! (Extracting from installs)

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut out: Option<PathBuf> = None;
    let mut folders: Vec<(Option<String>, PathBuf)> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--out" {
            out = args.next().map(PathBuf::from);
        } else if a == "--bryce" {
            // --bryce 3.0=FOLDER: the folder must hold that version (3.0, 3.1 or 5.0)
            let spec = args.next().unwrap_or_default();
            match spec.split_once('=') {
                Some((v @ ("3.0" | "3.1" | "5.0"), f)) if !f.is_empty() => {
                    folders.push((Some(v.to_owned()), PathBuf::from(f)));
                }
                _ => {
                    eprintln!("usage: --bryce 3.0=FOLDER (or 3.1, 5.0); got {spec:?}");
                    return ExitCode::from(2);
                }
            }
        } else {
            folders.push((None, PathBuf::from(a)));
        }
    }
    let out = match out {
        Some(out) => out,
        None => match stecyk3d::substrate::default_folder() {
            Some(folder) => {
                if let Err(e) = std::fs::create_dir_all(&folder) {
                    eprintln!("refused: {}: cannot be made: {e}", folder.display());
                    return ExitCode::FAILURE;
                }
                folder.join(stecyk3d::substrate::DEFAULT_FILE_NAME)
            }
            None => {
                eprintln!("usage: stecyk3d-extract [--out SUBSTRATE.bin] INSTALL_FOLDER [INSTALL_FOLDER ...]");
                eprintln!("no --out given and the default place cannot be told (no application-data folder in the environment)");
                return ExitCode::from(2);
            }
        },
    };
    match stecyk3d::extract::extract_expected(&folders, &out) {
        Ok(report) => {
            for line in report {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        Err(refused) => {
            for m in refused.messages {
                eprintln!("refused: {m}");
            }
            ExitCode::FAILURE
        }
    }
}
