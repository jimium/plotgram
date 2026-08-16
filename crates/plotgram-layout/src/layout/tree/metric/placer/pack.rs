//! Child-shape packing helpers. No placer policy; callers write parent + routes.

use plotgram_model::geometry::Rect;
use plotgram_model::port::Side;

use super::super::geom::{opposite, parent_child_route, port_point};
use super::super::shape::SubtreeShape;
use super::PlaceCtx;

pub fn clone_kids(kids: &[(String, SubtreeShape)]) -> Vec<(String, SubtreeShape)> {
    kids.iter().map(|(k, s)| (k.clone(), s.clone())).collect()
}

pub fn row(children: Vec<(String, SubtreeShape)>, gap: f64) -> SubtreeShape {
    let mut placed =
        SubtreeShape::from_parts(String::new(), Default::default(), Default::default());
    let mut x = 0.0;
    for (_k, mut cp) in children {
        if let Some(b) = cp.bounds() {
            cp.translate(x - b.x, -b.y);
            x = cp.bounds().map(|bb| bb.right()).unwrap_or(x) + gap;
        }
        placed.merge(cp);
    }
    placed
}

pub fn column(children: Vec<(String, SubtreeShape)>, gap: f64) -> SubtreeShape {
    let mut placed =
        SubtreeShape::from_parts(String::new(), Default::default(), Default::default());
    let mut y = 0.0;
    for (_k, mut cp) in children {
        if let Some(b) = cp.bounds() {
            cp.translate(-b.x, y - b.y);
            y = cp.bounds().map(|bb| bb.bottom()).unwrap_or(y) + gap;
        }
        placed.merge(cp);
    }
    placed
}

pub fn grid(children: Vec<(String, SubtreeShape)>, rows: usize, gap: f64) -> SubtreeShape {
    let n = children.len();
    if n == 0 || rows == 0 {
        return SubtreeShape::from_parts(String::new(), Default::default(), Default::default());
    }
    let cols = n.div_ceil(rows);
    let mut placed =
        SubtreeShape::from_parts(String::new(), Default::default(), Default::default());
    let mut y = 0.0;
    for chunk in children.chunks(cols) {
        let mut x = 0.0;
        let mut row_bottom = y;
        let mut row = Vec::new();
        for (_k, mut cp) in chunk.iter().cloned() {
            if let Some(b) = cp.bounds() {
                cp.translate(x - b.x, y - b.y);
                x = cp.bounds().map(|bb| bb.right()).unwrap_or(x) + gap;
                row_bottom =
                    row_bottom.max(cp.bounds().map(|bb| bb.bottom()).unwrap_or(row_bottom));
            }
            row.push(cp);
        }
        for cp in row {
            placed.merge(cp);
        }
        y = row_bottom + gap;
    }
    placed
}

pub fn grid_columns(children: Vec<(String, SubtreeShape)>, cols: usize, gap: f64) -> SubtreeShape {
    let n = children.len();
    if n == 0 || cols == 0 {
        return SubtreeShape::from_parts(String::new(), Default::default(), Default::default());
    }
    let rows = n.div_ceil(cols);
    let mut placed =
        SubtreeShape::from_parts(String::new(), Default::default(), Default::default());
    let mut x = 0.0;
    for chunk in children.chunks(rows) {
        let mut y = 0.0;
        let mut col_right: f64 = x;
        let mut col = Vec::new();
        for (_k, mut cp) in chunk.iter().cloned() {
            if let Some(b) = cp.bounds() {
                cp.translate(x - b.x, y - b.y);
                y = cp.bounds().map(|bb| bb.bottom()).unwrap_or(y) + gap;
                col_right = col_right.max(cp.bounds().map(|bb| bb.right()).unwrap_or(col_right));
            }
            col.push(cp);
        }
        for cp in col {
            placed.merge(cp);
        }
        x = col_right + gap;
    }
    placed
}

pub fn staggered_two_rows(children: Vec<(String, SubtreeShape)>, gap: f64) -> SubtreeShape {
    let mut row0 = Vec::new();
    let mut row1 = Vec::new();
    for (i, pair) in children.into_iter().enumerate() {
        if i % 2 == 0 {
            row0.push(pair);
        } else {
            row1.push(pair);
        }
    }
    let mut placed =
        SubtreeShape::from_parts(String::new(), Default::default(), Default::default());
    let mut x = 0.0;
    let mut row0_bottom: f64 = 0.0;
    for (_k, mut cp) in row0 {
        if let Some(b) = cp.bounds() {
            cp.translate(x - b.x, -b.y);
            x = cp.bounds().map(|bb| bb.right()).unwrap_or(x) + gap;
            row0_bottom = row0_bottom.max(cp.bounds().map(|bb| bb.bottom()).unwrap_or(row0_bottom));
        }
        placed.merge(cp);
    }
    let offset = placed
        .bounds()
        .map(|b| b.width / 2.0 + gap / 2.0)
        .unwrap_or(0.0);
    let row1_y = row0_bottom + gap;
    let mut x1 = offset;
    for (_k, mut cp) in row1 {
        if let Some(b) = cp.bounds() {
            cp.translate(x1 - b.x, row1_y - b.y);
            x1 = cp.bounds().map(|bb| bb.right()).unwrap_or(x1) + gap;
        }
        placed.merge(cp);
    }
    placed
}

pub fn with_parent_above(ctx: &PlaceCtx<'_>, root: &str, mut body: SubtreeShape) -> SubtreeShape {
    let sz = ctx.size_of[root];
    let mut parent = Rect::new(0.0, 0.0, sz.width, sz.height);
    let child_y = parent.bottom() + ctx.params.layer_gap;
    let mut placed = SubtreeShape::from_parts(root, Default::default(), Default::default());
    if let Some(b) = body.bounds() {
        body.translate(-b.x, child_y - b.y);
        if let Some(bb) = body.bounds() {
            parent.x = (bb.x + bb.right()) / 2.0 - parent.width / 2.0;
        }
    }
    placed.frames.insert(root.to_string(), parent);
    placed.merge(body);
    placed
}

pub fn with_parent_corner(ctx: &PlaceCtx<'_>, root: &str, mut body: SubtreeShape) -> SubtreeShape {
    let sz = ctx.size_of[root];
    let parent = Rect::new(0.0, 0.0, sz.width, sz.height);
    let child_y = parent.bottom() + ctx.params.layer_gap;
    let mut placed = SubtreeShape::from_parts(root, Default::default(), Default::default());
    if let Some(b) = body.bounds() {
        body.translate(-b.x, child_y - b.y);
    }
    placed.frames.insert(root.to_string(), parent);
    placed.merge(body);
    placed
}

pub fn attach_below(placed: &mut SubtreeShape, parent_id: &str, mut body: SubtreeShape, gap: f64) {
    let Some(parent) = placed.frames.get(parent_id).copied() else {
        return;
    };
    let top = placed
        .bounds()
        .map(|b| b.bottom())
        .unwrap_or(parent.bottom())
        + gap;
    if let Some(b) = body.bounds() {
        body.translate(parent.center().x - (b.x + b.right()) / 2.0, top - b.y);
    }
    placed.merge(body);
}

pub fn write_local_routes(
    placed: &mut SubtreeShape,
    ctx: &PlaceCtx<'_>,
    parent: &str,
    kids: &[String],
) {
    let Some(pf) = placed.frames.get(parent).copied() else {
        return;
    };
    for k in kids {
        let Some(eid) = ctx.plan.edge_of_child.get(k) else {
            continue;
        };
        let Some(cf) = placed.frames.get(k).copied() else {
            continue;
        };
        let to_side = ctx
            .plan
            .child_connectors
            .get(k)
            .copied()
            .unwrap_or(Side::North);
        let from_side = opposite(to_side);
        let route = parent_child_route(
            port_point(&pf, from_side),
            from_side,
            port_point(&cf, to_side),
            to_side,
            ctx.params.routing_style,
            ctx.params.min_first_segment,
        );
        placed.routes.insert(eid.clone(), route);
    }
}

pub fn split_assistants(
    ctx: &PlaceCtx<'_>,
    children: Vec<(String, SubtreeShape)>,
) -> (Vec<(String, SubtreeShape)>, Vec<(String, SubtreeShape)>) {
    let mut asst = Vec::new();
    let mut regular = Vec::new();
    for pair in children {
        if ctx.plan.is_assistant(&pair.0) {
            asst.push(pair);
        } else {
            regular.push(pair);
        }
    }
    (asst, regular)
}

pub fn regular_ids(kids: &[(String, SubtreeShape)]) -> Vec<String> {
    kids.iter().map(|(k, _)| k.clone()).collect()
}

pub fn empty_parent(ctx: &PlaceCtx<'_>, root: &str) -> SubtreeShape {
    let sz = ctx.size_of[root];
    let mut placed = SubtreeShape::from_parts(root, Default::default(), Default::default());
    placed
        .frames
        .insert(root.to_string(), Rect::new(0.0, 0.0, sz.width, sz.height));
    placed
}
