//! 边-边重叠/交叉检测（平行重叠 + 垂直交叉）。

use crate::layout::constants::ORTHO_PARALLEL_GAP;
use crate::layout::geometry::Point;
use crate::layout::LayoutResult;
use std::collections::{HashMap, HashSet};

pub(crate) const EPS: f64 = 0.1;

/// bbox 预筛选扩张量（含 parallel gap + 余量，与 orthogonal scoring 对齐）。
const BBOX_PAD: f64 = ORTHO_PARALLEL_GAP + 2.0;

/// 均匀网格 cell 边长（像素）。
const CELL_SIZE: f64 = 64.0;

type Segment = (usize, Point, Point);

/// P2-1: 均匀网格空间索引，将边段重叠检测从 O(S²) 降到 O(S·k)。
struct SegmentSpatialIndex {
    /// segment 在 `segments` 向量中的下标 → 落入的 cell 列表（用于查询）
    segment_cells: Vec<Vec<(i32, i32)>>,
    cells: HashMap<(i32, i32), Vec<usize>>,
}

impl SegmentSpatialIndex {
    fn build(segments: &[Segment]) -> Self {
        let mut cells: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        let mut segment_cells = Vec::with_capacity(segments.len());

        for (idx, (_edge_idx, p1, p2)) in segments.iter().enumerate() {
            let xmin = p1.x.min(p2.x) - BBOX_PAD;
            let xmax = p1.x.max(p2.x) + BBOX_PAD;
            let ymin = p1.y.min(p2.y) - BBOX_PAD;
            let ymax = p1.y.max(p2.y) + BBOX_PAD;

            let cx0 = cell_coord(xmin);
            let cx1 = cell_coord(xmax);
            let cy0 = cell_coord(ymin);
            let cy1 = cell_coord(ymax);

            let mut touched = Vec::new();
            for cx in cx0..=cx1 {
                for cy in cy0..=cy1 {
                    touched.push((cx, cy));
                    cells.entry((cx, cy)).or_default().push(idx);
                }
            }
            segment_cells.push(touched);
        }

        // 确定性：每个 cell 内按 segment 下标排序
        for list in cells.values_mut() {
            list.sort_unstable();
            list.dedup();
        }

        Self {
            segment_cells,
            cells,
        }
    }

    fn count_overlaps(&self, segments: &[Segment]) -> usize {
        let mut overlaps = 0usize;
        let mut checked: HashSet<(usize, usize)> = HashSet::new();

        for (i, cells_i) in self.segment_cells.iter().enumerate() {
            let mut candidates: HashSet<usize> = HashSet::new();
            for cell in cells_i {
                if let Some(list) = self.cells.get(cell) {
                    for &j in list {
                        if j > i {
                            candidates.insert(j);
                        }
                    }
                }
            }

            let mut js: Vec<usize> = candidates.into_iter().collect();
            js.sort_unstable();

            let (ei, a1, a2) = segments[i];
            for j in js {
                if !checked.insert((i, j)) {
                    continue;
                }
                let (ej, b1, b2) = segments[j];
                if ei == ej {
                    continue;
                }
                if segments_conflict_xy(a1, a2, b1, b2) {
                    overlaps += 1;
                }
            }
        }

        overlaps
    }
}

fn cell_coord(v: f64) -> i32 {
    (v / CELL_SIZE).floor() as i32
}

fn collect_polyline_segments(result: &LayoutResult) -> Vec<Segment> {
    let mut all_segments = Vec::new();
    for (edge_idx, edge) in result.edges.iter().enumerate() {
        if !edge.is_polyline() || edge.path_len() < 2 {
            continue;
        }
        let path = edge.path_points();
        for window in path.windows(2) {
            all_segments.push((edge_idx, window[0], window[1]));
        }
    }
    all_segments
}

/// 检测边-边重叠/交叉次数（平行重叠 + 垂直交叉）。
///
/// 仅检测 Polyline 边。使用均匀网格空间索引（P2-1）避免 O(E²S²) 全量两两比较。
pub(crate) fn analyze_edge_overlaps(result: &LayoutResult) -> usize {
    let segments = collect_polyline_segments(result);
    if segments.len() < 2 {
        return 0;
    }
    let index = SegmentSpatialIndex::build(&segments);
    index.count_overlaps(&segments)
}

/// 检测两条线段是否冲突（平行重叠或垂直交叉）
pub(crate) fn segments_conflict_xy(
    a1: Point,
    a2: Point,
    b1: Point,
    b2: Point,
) -> bool {
    let a_horiz = (a1.y - a2.y).abs() < EPS;
    let b_horiz = (b1.y - b2.y).abs() < EPS;
    let a_vert = (a1.x - a2.x).abs() < EPS;
    let b_vert = (b1.x - b2.x).abs() < EPS;

    if a_horiz && b_horiz {
        let gap = (a1.y - b1.y).abs();
        if gap > ORTHO_PARALLEL_GAP {
            return false;
        }
        let a_min = a1.x.min(a2.x);
        let a_max = a1.x.max(a2.x);
        let b_min = b1.x.min(b2.x);
        let b_max = b1.x.max(b2.x);
        return a_max > b_min + EPS && b_max > a_min + EPS;
    }

    if a_vert && b_vert {
        let gap = (a1.x - b1.x).abs();
        if gap > ORTHO_PARALLEL_GAP {
            return false;
        }
        let a_min = a1.y.min(a2.y);
        let a_max = a1.y.max(a2.y);
        let b_min = b1.y.min(b2.y);
        let b_max = b1.y.max(b2.y);
        return a_max > b_min + EPS && b_max > a_min + EPS;
    }

    if a_horiz && b_vert {
        return segments_cross_perpendicular_xy(a1, a2, b1, b2);
    }
    if a_vert && b_horiz {
        return segments_cross_perpendicular_xy(b1, b2, a1, a2);
    }

    false
}

fn segments_cross_perpendicular_xy(
    h1: Point,
    h2: Point,
    v1: Point,
    v2: Point,
) -> bool {
    let h_y = h1.y;
    let v_x = v1.x;
    let h_x_min = h1.x.min(h2.x);
    let h_x_max = h1.x.max(h2.x);
    let v_y_min = v1.y.min(v2.y);
    let v_y_max = v1.y.max(v2.y);
    v_x > h_x_min + EPS
        && v_x < h_x_max - EPS
        && h_y > v_y_min + EPS
        && h_y < v_y_max - EPS
}
