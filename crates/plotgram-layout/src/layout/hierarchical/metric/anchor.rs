//! Port point expansion: the single implementation of `along_spec × frame`
//! (architecture.md §3.2 — `port_points` is Metric's deterministic expansion
//! of the Compose-frozen `PortPlan`; Ink reads the same function, never owns
//! the formula). Canonical (TB) space; `ResolvedPort` carries the slot order,
//! which is stable ordering only — the pixel anchor is derived here.

use plotgram_algo::orientation::Side::*;
use plotgram_model::geometry::{Point, Rect};

use crate::layout::hierarchical::compose::ports::ResolvedPort;

/// Pixel anchor of a finalized port on `frame`'s boundary. Slot `t` spreads
/// `count` ports evenly along the side: `(slot + 1) / (count + 1)`.
pub fn port_anchor(frame: Rect, port: ResolvedPort) -> Point {
    let t = (port.slot as f64 + 1.0) / (port.count as f64 + 1.0);
    match port.side {
        North => Point {
            x: frame.x + t * frame.width,
            y: frame.y,
        },
        South => Point {
            x: frame.x + t * frame.width,
            y: frame.bottom(),
        },
        West => Point {
            x: frame.x,
            y: frame.y + t * frame.height,
        },
        East => Point {
            x: frame.right(),
            y: frame.y + t * frame.height,
        },
    }
}
