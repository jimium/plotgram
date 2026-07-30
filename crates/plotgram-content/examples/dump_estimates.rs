//! Dump heuristic width estimates as JSON for offline comparison against real
//! font metrics (`scripts/calibrate_content_measure.py compare`).
//!
//! Usage:
//!   scripts/calibrate_content_measure.py gen-cases > /tmp/cases.json
//!   cargo run -p plotgram-content --example dump_estimates -- /tmp/cases.json > /tmp/estimates.json
//!   scripts/calibrate_content_measure.py compare --estimates /tmp/estimates.json

use plotgram_content::measure::estimate_text_width;
use plotgram_content::RunStyle;

fn main() {
    let path = std::env::args().nth(1).expect("usage: dump_estimates <cases.json>");
    let raw = std::fs::read_to_string(&path).expect("read cases file");
    let cases: Vec<serde_json::Value> = serde_json::from_str(&raw).expect("cases: JSON array");

    let out: Vec<serde_json::Value> = cases
        .into_iter()
        .map(|mut case| {
            let text = case["text"].as_str().expect("case.text").to_string();
            let font_size = case["font_size"].as_f64().expect("case.font_size");
            let style = match case["style"].as_str().expect("case.style") {
                "strong" => RunStyle::Strong,
                "emph" => RunStyle::Emph,
                "code" => RunStyle::Code,
                _ => RunStyle::Plain,
            };
            case["est_width"] = estimate_text_width(&text, style, font_size).into();
            case
        })
        .collect();

    println!("{}", serde_json::to_string_pretty(&out).expect("serialize"));
}
