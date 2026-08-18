//! `double-layer`: children in two staggered rows under a horizontal bus.

use tautcore_engine_api::LayoutError;
use tautcore_model::geometry::Rect;
use tautcore_model::port::Side;

use super::super::geom::{parent_child_route, port_point};
use super::super::shape::SubtreeShape;
use super::layered::apply_root_alignment;
use super::{ISubtreePlacer, PlaceCtx};
use crate::layout::tree::params::TreeRoutingStyle;
use crate::layout::tree::plan::TreeRoute;

#[derive(Debug, Default, Clone, Copy)]
pub struct DoubleLayerPlacer;

impl ISubtreePlacer for DoubleLayerPlacer {
    fn place_subtree(
        &self,
        ctx: &PlaceCtx<'_>,
        root: &str,
        children: Vec<(String, SubtreeShape)>,
    ) -> Result<SubtreeShape, LayoutError> {
        arrange(ctx, root, children)
    }
}

impl DoubleLayerPlacer {
    pub fn place(&self, ctx: &PlaceCtx<'_>, root: &str) -> Result<SubtreeShape, LayoutError> {
        let kids = super::place_children(ctx, root)?;
        self.place_subtree(ctx, root, kids)
    }
}

fn arrange(
    ctx: &PlaceCtx<'_>,
    root: &str,
    children: Vec<(String, SubtreeShape)>,
) -> Result<SubtreeShape, LayoutError> {
    let sz = ctx.size_of[root];
    let parent = Rect::new(0.0, 0.0, sz.width, sz.height);
    let mut placed = SubtreeShape::from_parts(root, Default::default(), Default::default());
    if children.is_empty() {
        placed.frames.insert(root.to_string(), parent);
        return Ok(placed);
    }

    let gap = ctx.params.node_gap;
    let layer = ctx.params.layer_gap;
    let row0_y = parent.bottom() + layer;
    let mut row0: Vec<(String, SubtreeShape)> = Vec::new();
    let mut row1: Vec<(String, SubtreeShape)> = Vec::new();
    for (i, pair) in children.into_iter().enumerate() {
        if i % 2 == 0 {
            row0.push(pair);
        } else {
            row1.push(pair);
        }
    }

    let mut x = 0.0;
    let mut row0_bottom = row0_y;
    for (_k, cp) in row0.iter_mut() {
        if let Some(b) = cp.bounds() {
            cp.translate(x - b.x, row0_y - b.y);
            x = cp.bounds().map(|bb| bb.right()).unwrap_or(x) + gap;
            row0_bottom = row0_bottom.max(cp.bounds().map(|bb| bb.bottom()).unwrap_or(row0_bottom));
        }
    }

    let offset = match row0.first().and_then(|(_, cp)| cp.bounds()) {
        Some(b) => b.width / 2.0 + gap / 2.0,
        None => 0.0,
    };
    let row1_y = row0_bottom + gap;
    let mut x1 = offset;
    for (_k, cp) in row1.iter_mut() {
        if let Some(b) = cp.bounds() {
            cp.translate(x1 - b.x, row1_y - b.y);
            x1 = cp.bounds().map(|bb| bb.right()).unwrap_or(x1) + gap;
        }
    }

    let kids: Vec<String> = ctx.plan.children_of(root).to_vec();
    for (_, cp) in row0.into_iter().chain(row1.into_iter()) {
        placed.merge(cp);
    }

    if let Some(b) = placed.bounds() {
        let mut parent = parent;
        parent.x = (b.x + b.right()) / 2.0 - parent.width / 2.0;
        placed.frames.insert(root.to_string(), parent);
    } else {
        placed.frames.insert(root.to_string(), parent);
    }
    apply_root_alignment(
        &mut placed.frames,
        root,
        &kids,
        ctx.plan,
        ctx.params.root_alignment,
    );

    let parent_frame = placed.frames.get(root).copied().unwrap_or(parent);
    let bus_y = (parent_frame.bottom() + row0_y) / 2.0;

    for k in ctx.plan.children_of(root) {
        let Some(cf) = placed.frames.get(k) else {
            continue;
        };
        let Some(eid) = ctx.plan.edge_of_child.get(k) else {
            continue;
        };
        let start = port_point(&parent_frame, Side::South);
        let end = port_point(cf, Side::North);
        let route = match ctx.params.routing_style {
            TreeRoutingStyle::Straight | TreeRoutingStyle::OrthogonalAtRoot => parent_child_route(
                start,
                Side::South,
                end,
                Side::North,
                ctx.params.routing_style,
                ctx.params.min_first_segment,
            ),
            _ => TreeRoute::HorizontalBus { start, bus_y, end },
        };
        placed.routes.insert(eid.clone(), route);
    }
    Ok(placed)
}
