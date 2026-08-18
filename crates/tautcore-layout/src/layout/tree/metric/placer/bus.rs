//! Vertical bus: children left / right of a downward rail; optional last child below.

use tautcore_engine_api::LayoutError;
use tautcore_model::geometry::{Point, Rect};
use tautcore_model::port::Side;

use super::super::geom::port_point;
use super::super::shape::SubtreeShape;
use super::{ISubtreePlacer, PlaceCtx};
use crate::layout::tree::params::BusSlot;
use crate::layout::tree::plan::TreeRoute;

#[derive(Debug, Default, Clone, Copy)]
pub struct BusPlacer;

impl ISubtreePlacer for BusPlacer {
    fn place_subtree(
        &self,
        ctx: &PlaceCtx<'_>,
        root: &str,
        children: Vec<(String, SubtreeShape)>,
    ) -> Result<SubtreeShape, LayoutError> {
        arrange(ctx, root, children)
    }
}

impl BusPlacer {
    pub fn place(&self, ctx: &PlaceCtx<'_>, root: &str) -> Result<SubtreeShape, LayoutError> {
        let kids = super::place_children(ctx, root)?;
        self.place_subtree(ctx, root, kids)
    }
}

pub fn arrange(
    ctx: &PlaceCtx<'_>,
    id: &str,
    children: Vec<(String, SubtreeShape)>,
) -> Result<SubtreeShape, LayoutError> {
    let parent_sz = ctx.size_of[id];
    let mut placed = SubtreeShape::from_parts(id, Default::default(), Default::default());
    let parent_frame = Rect::new(0.0, 0.0, parent_sz.width, parent_sz.height);
    let bus_x = parent_frame.center().x;
    let bus_top = parent_frame.bottom();
    let gap = ctx.params.node_gap;
    let layer = ctx.params.layer_gap;

    let mut left_y = bus_top + layer;
    let mut right_y = bus_top + layer;
    let mut bottom_y = bus_top + layer;
    let mut left_max_bottom = left_y;
    let mut right_max_bottom = right_y;

    for (k, mut cp) in children {
        let slot = ctx.plan.bus_slot.get(&k).copied().unwrap_or(BusSlot::Left);
        let Some(b) = cp.bounds() else {
            continue;
        };
        match slot {
            BusSlot::Left => {
                let dx = (bus_x - gap) - b.right();
                let dy = left_y - b.y;
                cp.translate(dx, dy);
                left_y = cp.bounds().map(|bb| bb.bottom()).unwrap_or(left_y) + gap;
                left_max_bottom = left_y;
            }
            BusSlot::Right => {
                let dx = (bus_x + gap) - b.x;
                let dy = right_y - b.y;
                cp.translate(dx, dy);
                right_y = cp.bounds().map(|bb| bb.bottom()).unwrap_or(right_y) + gap;
                right_max_bottom = right_y;
            }
            BusSlot::Bottom => {
                bottom_y = left_max_bottom.max(right_max_bottom).max(bottom_y);
                let dx = bus_x - b.center().x;
                let dy = bottom_y - b.y;
                cp.translate(dx, dy);
            }
        }
        if let Some(cf) = cp.frames.get(&k) {
            if let Some(eid) = ctx.plan.edge_of_child.get(&k) {
                let to_side = ctx
                    .plan
                    .child_connectors
                    .get(&k)
                    .copied()
                    .unwrap_or(Side::East);
                let start = port_point(&parent_frame, Side::South);
                let end = port_point(cf, to_side);
                let mid_y = (start.y + end.y) / 2.0;
                let route = TreeRoute::Polyline {
                    points: vec![
                        start,
                        Point {
                            x: start.x,
                            y: mid_y,
                        },
                        Point { x: bus_x, y: mid_y },
                        Point { x: bus_x, y: end.y },
                        end,
                    ],
                };
                placed.routes.insert(eid.clone(), route);
            }
        }
        placed.merge(cp);
    }
    placed.frames.insert(id.to_string(), parent_frame);
    Ok(placed)
}
