//! Thin CLI: args + file I/O only. Build orchestration lives in `plotgram-compile`.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use plotgram_compile::{build_svg, BuildOptions};

#[derive(Debug, Parser)]
#[command(name = "plotgram", about = "Plotgram DSL → SVG")]
struct Args {
    /// Input `.pgm` file
    input: PathBuf,

    /// Output SVG path (default: stdout)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Theme id override
    #[arg(long)]
    theme: Option<String>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let source = match fs::read_to_string(&args.input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("read {}: {e}", args.input.display());
            return ExitCode::FAILURE;
        }
    };

    let options = BuildOptions {
        theme: args.theme,
        ..BuildOptions::default()
    };

    match build_svg(&source, &options) {
        Ok(svg) => {
            if let Some(path) = args.output {
                if let Err(e) = fs::write(&path, svg) {
                    eprintln!("write {}: {e}", path.display());
                    return ExitCode::FAILURE;
                }
            } else {
                print!("{svg}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("plotgram: {e}");
            ExitCode::FAILURE
        }
    }
}
