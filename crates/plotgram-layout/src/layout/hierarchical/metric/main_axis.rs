//! P4.1 main axis: stack ranks, each layer's thickness = max member height
//! (dummies contribute 0). Canonical space only (main = y, TB).

use plotgram_algo::orientation::Size;

use crate::layout::hierarchical::model::PlanGraph;

/// Per-elem canonical main-axis start (top edge for real nodes; the point's
/// own coordinate for dummies, since their height is 0).
///
/// `layer_gaps[r]` is the main-axis gap between layer `r` and `r+1`
/// (DemandBoard-resolved; length `layers.len() - 1`).
pub fn assign_main_axis(
    plan: &PlanGraph,
    size_of: &dyn Fn(usize) -> Size,
    layer_gaps: &[f64],
    layer_alignment: f64,
) -> Vec<f64> {
    let mut top = vec![0.0; plan.elems.len()];
    let mut cursor = 0.0;
    for (r, layer) in plan.layers.iter().enumerate() {
        let thickness = layer
            .iter()
            .map(|&e| size_of(e).height)
            .fold(0.0_f64, f64::max);
        for &e in layer {
            let h = size_of(e).height;
            // Zero-height elems (dummies / pads) stay on the layer-band leading
            // edge. Centering them (h=0 → mid-band) puts shared horizontal
            // corridors through same-layer node interiors.
            top[e] = if h <= 0.0 {
                cursor
            } else {
                cursor + (thickness - h) * layer_alignment
            };
        }
        cursor += thickness;
        if r + 1 < plan.layers.len() {
            let gap = layer_gaps.get(r).copied().unwrap_or(0.0);
            cursor += gap;
        }
    }
    top
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::model::{Elem, ElemKey};

    fn plan_with_ranks(ranks: &[u32]) -> PlanGraph {
        let elems: Vec<Elem> = ranks
            .iter()
            .enumerate()
            .map(|(i, &r)| Elem {
                key: ElemKey::Real(format!("n{i}")),
                group_path: Vec::new(),
                rank: r,
            })
            .collect();
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let decl_index = (0..elems.len()).collect();
        let max_rank = *ranks.iter().max().unwrap();
        let mut layers = vec![Vec::new(); max_rank as usize + 1];
        for (i, &r) in ranks.iter().enumerate() {
            layers[r as usize].push(i);
        }
        PlanGraph {
            elems,
            index_of,
            decl_index,
            segments: Vec::new(),
            layers,
            ..Default::default()
        }
    }

    #[test]
    fn stacks_layers_with_gap_and_max_height() {
        let plan = plan_with_ranks(&[0, 0, 1]);
        let sizes = [
            Size::new(10.0, 20.0),
            Size::new(10.0, 30.0),
            Size::new(10.0, 15.0),
        ];
        let top = assign_main_axis(&plan, &|e| sizes[e], &[40.0], 0.0);
        assert_eq!(top[0], 0.0);
        assert_eq!(top[1], 0.0);
        // layer0 thickness = max(20,30) = 30; next layer starts at 30+40
        assert_eq!(top[2], 70.0);
    }

    #[test]
    fn layer_alignment_centers_shorter_nodes() {
        let plan = plan_with_ranks(&[0, 0]);
        let sizes = [Size::new(10.0, 20.0), Size::new(10.0, 30.0)];
        let top = assign_main_axis(&plan, &|e| sizes[e], &[], 0.5);
        // thickness=30; n0 h=20 → offset 5; n1 h=30 → offset 0
        assert_eq!(top[0], 5.0);
        assert_eq!(top[1], 0.0);
    }

    #[test]
    fn no_trailing_gap_after_last_layer() {
        let plan = plan_with_ranks(&[0, 1]);
        let sizes = [Size::new(10.0, 10.0), Size::new(10.0, 10.0)];
        let top = assign_main_axis(&plan, &|e| sizes[e], &[5.0], 0.0);
        assert_eq!(top, vec![0.0, 15.0]);
    }

    #[test]
    fn per_gap_demands_expand_later_layers() {
        let plan = plan_with_ranks(&[0, 1, 2]);
        let sizes = [
            Size::new(10.0, 10.0),
            Size::new(10.0, 10.0),
            Size::new(10.0, 10.0),
        ];
        let top = assign_main_axis(&plan, &|e| sizes[e], &[40.0, 72.0], 0.0);
        assert_eq!(top[0], 0.0);
        assert_eq!(top[1], 50.0);
        assert_eq!(top[2], 50.0 + 10.0 + 72.0);
    }
}
