//! WASM bridge for the layout debug inspector (debug-inspector.md T2).
//!
//! Thin bindings over `plotgram-compile`: the page drives the same pipeline
//! the CLI does (`debug-layout`) and consumes the resulting trace JSON. No
//! geometry is written back — the trace is a read-only projection
//! (write-authority discipline).

use plotgram_compile::{build_debug_trace, build_svg, BuildOptions};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
}

/// Build the source into a `LayoutDebugTrace` and return it as JSON.
///
/// Errors (parse / measure / unsupported layout) surface as JS exceptions
/// carrying the pipeline error text.
#[wasm_bindgen(js_name = debugTrace)]
pub fn debug_trace(source: &str) -> Result<String, JsError> {
    let trace = build_debug_trace(source, &BuildOptions::default())
        .map_err(|e| JsError::new(&e.to_string()))?;
    serde_json::to_string(&trace).map_err(|e| JsError::new(&e.to_string()))
}

/// Render the source into product SVG (kept as a bridge capability; the
/// inspector canvas draws from the trace common view instead).
#[wasm_bindgen(js_name = renderSvg)]
pub fn render_svg(source: &str) -> Result<String, JsError> {
    build_svg(source, &BuildOptions::default()).map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
