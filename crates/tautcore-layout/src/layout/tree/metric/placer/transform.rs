//! Local SubtreeTransform wrap (mindmap left/right). Canonical TB in, TB out.
//! 90° around the local-root center; node AABBs stay axis-aligned.

use std::collections::BTreeMap;

use tautcore_model::geometry::{Point, Rect};

use crate::layout::tree::params::SubtreeTransform;

pub fn transform_point(origin: Point, p: Point, t: SubtreeTransform) -> Point {
    let dx = p.x - origin.x;
    let dy = p.y - origin.y;
    match t {
        SubtreeTransform::None => p,
        // y-down: 90° CW → child-below becomes child-left
        SubtreeTransform::RotateLeft => Point {
            x: origin.x - dy,
            y: origin.y + dx,
        },
        SubtreeTransform::RotateRight => Point {
            x: origin.x + dy,
            y: origin.y - dx,
        },
    }
}

pub fn rotate_frames_around(
    frames: &mut BTreeMap<String, Rect>,
    origin: Point,
    t: SubtreeTransform,
) {
    if t == SubtreeTransform::None {
        return;
    }
    for f in frames.values_mut() {
        let c = f.center();
        let p = transform_point(origin, c, t);
        *f = Rect::new(p.x - f.width / 2.0, p.y - f.height / 2.0, f.width, f.height);
    }
}
