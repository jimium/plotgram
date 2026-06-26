//! 组间优先路由走廊：几何推导、布局注入合并。

use std::collections::HashMap;

use crate::layout::geometry::Point;
use crate::layout::GroupLayout;

use super::constants::EPS;

/// 组间优先路由走廊方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CorridorAxis {
    Vertical,
    Horizontal,
}

/// 相邻分组之间的轴对齐走廊（布局产出，路由消费）。
#[derive(Debug, Clone)]
pub struct GroupCorridor {
    pub axis: CorridorAxis,
    /// 竖走廊为 x；横走廊为 y。
    pub coord: f64,
    pub span_min: f64,
    pub span_max: f64,
    pub group_a: String,
    pub group_b: String,
}

fn corridor_pair_key(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

fn pair_covered(corridors: &[GroupCorridor], group_a: &str, group_b: &str) -> bool {
    let (ka, kb) = corridor_pair_key(group_a, group_b);
    corridors.iter().any(|c| {
        let (ca, cb) = corridor_pair_key(&c.group_a, &c.group_b);
        ca == ka && cb == kb
    })
}

/// 合并布局注入走廊与几何 fallback（注入优先，补全未覆盖邻接对）。
pub fn merge_corridors(
    injected: &[GroupCorridor],
    groups: &HashMap<String, GroupLayout>,
) -> Vec<GroupCorridor> {
    let mut merged = injected.to_vec();
    for c in build_corridors_from_groups(groups) {
        if !pair_covered(&merged, &c.group_a, &c.group_b) {
            merged.push(c);
        }
    }
    merged.sort_by(|a, b| {
        a.axis
            .cmp(&b.axis)
            .then_with(|| a.group_a.cmp(&b.group_a))
            .then_with(|| a.group_b.cmp(&b.group_b))
    });
    merged
}

/// 从 group 包围框推导相邻组对的走廊中线（确定性：group id 排序）。
pub fn build_corridors_from_groups(groups: &HashMap<String, GroupLayout>) -> Vec<GroupCorridor> {
    let mut ids: Vec<&String> = groups.keys().collect();
    ids.sort();
    let mut corridors = Vec::new();
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let id_a = ids[i].as_str();
            let id_b = ids[j].as_str();
            let Some(ga) = groups.get(id_a) else {
                continue;
            };
            let Some(gb) = groups.get(id_b) else {
                continue;
            };
            push_corridor_between(ga, gb, id_a, id_b, &mut corridors);
        }
    }
    corridors
}

/// 流程图堆叠排列：按拓扑序在相邻 group 之间导出走廊。
pub fn build_stacking_corridors(
    order: &[String],
    groups: &HashMap<String, GroupLayout>,
    vertical_stack: bool,
) -> Vec<GroupCorridor> {
    let mut corridors = Vec::new();
    let present: Vec<&String> = order
        .iter()
        .filter(|id| groups.contains_key(id.as_str()))
        .collect();
    for w in present.windows(2) {
        let id_a = w[0].as_str();
        let id_b = w[1].as_str();
        let Some(ga) = groups.get(id_a) else {
            continue;
        };
        let Some(gb) = groups.get(id_b) else {
            continue;
        };
        if vertical_stack {
            let bottom_a = ga.y + ga.height;
            let top_b = gb.y;
            if top_b > bottom_a - EPS {
                corridors.push(GroupCorridor {
                    axis: CorridorAxis::Horizontal,
                    coord: (bottom_a + top_b) * 0.5,
                    span_min: ga.x.max(gb.x),
                    span_max: (ga.x + ga.width).min(gb.x + gb.width),
                    group_a: id_a.to_string(),
                    group_b: id_b.to_string(),
                });
            }
        } else {
            let right_a = ga.x + ga.width;
            let left_b = gb.x;
            if left_b > right_a - EPS {
                corridors.push(GroupCorridor {
                    axis: CorridorAxis::Vertical,
                    coord: (right_a + left_b) * 0.5,
                    span_min: ga.y.max(gb.y),
                    span_max: (ga.y + ga.height).min(gb.y + gb.height),
                    group_a: id_a.to_string(),
                    group_b: id_b.to_string(),
                });
            }
        }
    }
    corridors
}

fn push_corridor_between(
    ga: &GroupLayout,
    gb: &GroupLayout,
    id_a: &str,
    id_b: &str,
    out: &mut Vec<GroupCorridor>,
) {
    let a = (ga.x, ga.y, ga.x + ga.width, ga.y + ga.height);
    let b = (gb.x, gb.y, gb.x + gb.width, gb.y + gb.height);

    let y_overlap = a.1 < b.3 - EPS && b.1 < a.3 - EPS;
    let x_overlap = a.0 < b.2 - EPS && b.0 < a.2 - EPS;

    if y_overlap && a.2 <= b.0 - EPS {
        let gap = b.0 - a.2;
        if gap < f64::INFINITY {
            out.push(GroupCorridor {
                axis: CorridorAxis::Vertical,
                coord: a.2 + gap * 0.5,
                span_min: a.1.max(b.1),
                span_max: a.3.min(b.3),
                group_a: id_a.to_string(),
                group_b: id_b.to_string(),
            });
        }
    } else if y_overlap && b.2 <= a.0 - EPS {
        let gap = a.0 - b.2;
        if gap < f64::INFINITY {
            out.push(GroupCorridor {
                axis: CorridorAxis::Vertical,
                coord: b.2 + gap * 0.5,
                span_min: a.1.max(b.1),
                span_max: a.3.min(b.3),
                group_a: id_a.to_string(),
                group_b: id_b.to_string(),
            });
        }
    }

    if x_overlap && a.3 <= b.1 - EPS {
        let gap = b.1 - a.3;
        if gap < f64::INFINITY {
            out.push(GroupCorridor {
                axis: CorridorAxis::Horizontal,
                coord: a.3 + gap * 0.5,
                span_min: a.0.max(b.0),
                span_max: a.2.min(b.2),
                group_a: id_a.to_string(),
                group_b: id_b.to_string(),
            });
        }
    } else if x_overlap && b.3 <= a.1 - EPS {
        let gap = a.1 - b.3;
        if gap < f64::INFINITY {
            out.push(GroupCorridor {
                axis: CorridorAxis::Horizontal,
                coord: b.3 + gap * 0.5,
                span_min: a.0.max(b.0),
                span_max: a.2.min(b.2),
                group_a: id_a.to_string(),
                group_b: id_b.to_string(),
            });
        }
    }
}

/// 在走廊列表中查找与给定轴、跨度匹配的最近走廊坐标。
pub fn prefer_corridor_coord(
    axis: CorridorAxis,
    default: f64,
    span_min: f64,
    span_max: f64,
    corridors: &[GroupCorridor],
    max_distance: f64,
) -> f64 {
    let mut best: Option<(f64, f64)> = None;
    for c in corridors {
        if c.axis != axis {
            continue;
        }
        if c.span_max <= span_min + EPS || c.span_min >= span_max - EPS {
            continue;
        }
        let dist = (c.coord - default).abs();
        if dist > max_distance {
            continue;
        }
        if best.is_none_or(|(_, d)| dist < d) {
            best = Some((c.coord, dist));
        }
    }
    best.map(|(coord, _)| coord).unwrap_or(default)
}

/// 路径沿走廊对齐的软惩罚（越低越好；未对齐时加小惩罚）。
pub fn corridor_misalignment_penalty(
    path: &[Point],
    corridors: &[GroupCorridor],
    misalign_penalty: f64,
) -> f64 {
    if corridors.is_empty() || path.len() < 2 {
        return 0.0;
    }
    const ALIGN_EPS: f64 = 6.0;
    let mut penalty = 0.0;
    for w in path.windows(2) {
        let a = w[0];
        let b = w[1];
        let span_min;
        let span_max;
        let axis;
        let coord;
        if (a.x - b.x).abs() < EPS {
            axis = CorridorAxis::Vertical;
            coord = a.x;
            span_min = a.y.min(b.y);
            span_max = a.y.max(b.y);
        } else if (a.y - b.y).abs() < EPS {
            axis = CorridorAxis::Horizontal;
            coord = a.y;
            span_min = a.x.min(b.x);
            span_max = a.x.max(b.x);
        } else {
            continue;
        }
        let mut aligned = false;
        for c in corridors {
            if c.axis != axis {
                continue;
            }
            if c.span_max <= span_min + EPS || c.span_min >= span_max - EPS {
                continue;
            }
            if (c.coord - coord).abs() <= ALIGN_EPS {
                aligned = true;
                break;
            }
        }
        if !aligned {
            penalty += misalign_penalty;
        }
    }
    penalty
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_corridors_vertical_gap() {
        let mut groups = HashMap::new();
        groups.insert(
            "a".to_string(),
            GroupLayout {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            },
        );
        groups.insert(
            "b".to_string(),
            GroupLayout {
                x: 140.0,
                y: 20.0,
                width: 80.0,
                height: 80.0,
            },
        );
        let corridors = build_corridors_from_groups(&groups);
        assert_eq!(corridors.len(), 1);
        assert_eq!(corridors[0].axis, CorridorAxis::Vertical);
        assert!((corridors[0].coord - 120.0).abs() < EPS);
    }

    #[test]
    fn merge_corridors_injected_takes_precedence() {
        let mut groups = HashMap::new();
        groups.insert(
            "a".to_string(),
            GroupLayout {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            },
        );
        groups.insert(
            "b".to_string(),
            GroupLayout {
                x: 140.0,
                y: 20.0,
                width: 80.0,
                height: 80.0,
            },
        );
        let injected = vec![GroupCorridor {
            axis: CorridorAxis::Vertical,
            coord: 121.0,
            span_min: 0.0,
            span_max: 100.0,
            group_a: "a".into(),
            group_b: "b".into(),
        }];
        let merged = merge_corridors(&injected, &groups);
        assert_eq!(merged.len(), 1);
        assert!((merged[0].coord - 121.0).abs() < EPS);
    }
}
