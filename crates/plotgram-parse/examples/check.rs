//! Parse-check `.pgm` files (parse only, no layout/render).
//!
//! Usage: `cargo run -p plotgram-parse --example check -- <files...>`

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut total = 0usize;
    let mut failed = 0usize;

    for path in std::env::args().skip(1) {
        total += 1;
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                println!("READ-ERR {path}: {e}");
                failed += 1;
                continue;
            }
        };
        match plotgram_parse::parse(&src) {
            Ok(out) => {
                if out.warnings.is_empty() {
                    println!("OK   {path}");
                } else {
                    let msgs: Vec<&str> = out.warnings.iter().map(|w| w.message.as_str()).collect();
                    println!("WARN {path}: {}", msgs.join("; "));
                }
            }
            Err(e) => {
                println!("FAIL {path}: {e}");
                failed += 1;
            }
        }
    }

    println!("-- {total} files, {failed} failed");
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
