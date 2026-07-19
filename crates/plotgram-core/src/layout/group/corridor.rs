//! 组间优先路由走廊：几何推导、布局注入合并。

use std::collections::HashMap;

use crate::layout::geometry::Point;
use crate::layout::GroupLayout;

use super::constants::EPS;

/// 组间优先路由走廊方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
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

/// 合并布局注入走廊与最终几何走廊。
///
/// **最终几何优先**：`group_frame` 会移动/拉齐组框，布局阶段注入的 `coord/span`
/// 会过期；若仍以注入为准，走廊路径易脏并触发退化。
/// 几何能覆盖的邻接对一律用当前包围框重建；注入仅补全几何未覆盖的对。
pub fn merge_corridors(
    injected: &[GroupCorridor],
    groups: &HashMap<String, GroupLayout>,
) -> Vec<GroupCorridor> {
    let mut merged = build_corridors_from_groups(groups);
    for c in injected {
        if !pair_covered(&merged, &c.group_a, &c.group_b) {
            merged.push(c.clone());
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
///
/// C2 注：激进「间隙无第三组」清除在 federation 上有效，但会误杀侧旁擦边邻接
/// （ecommerce / multi-namespace 正确性回归）。伪邻接改由 `try_build` 多跳外绕
/// + `validated_corridor_path` 避组门槛消化；间隙清除留给后续更严 betweenness。
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

/// 同父 sibling 组：按堆叠主轴排序后，仅在相邻对之间导出走廊（嵌套架构图）。
///
/// 同时覆盖**顶层** sibling（`parent_id == None`），补齐跨顶层 group 走廊。
pub fn build_sibling_corridors(
    diagram: &crate::ast::Diagram,
    groups: &HashMap<String, GroupLayout>,
) -> Vec<GroupCorridor> {
    use std::collections::HashMap as StdHashMap;

    let mut children_of: StdHashMap<String, Vec<String>> = StdHashMap::new();
    let mut top_level: Vec<String> = Vec::new();
    for group in &diagram.groups {
        if let Some(pid) = &group.parent_id {
            children_of
                .entry(pid.as_str().to_string())
                .or_default()
                .push(group.id.as_str().to_string());
        } else {
            top_level.push(group.id.as_str().to_string());
        }
    }

    let mut corridors = Vec::new();
    let mut parent_ids: Vec<String> = children_of.keys().cloned().collect();
    parent_ids.sort();
    for parent_id in parent_ids {
        let Some(children) = children_of.get_mut(&parent_id) else {
            continue;
        };
        append_adjacent_sibling_corridors(children, groups, &mut corridors);
    }
    // Phase 2：顶层 sibling 走廊（无 parent）
    append_adjacent_sibling_corridors(&mut top_level, groups, &mut corridors);

    corridors.sort_by(|a, b| {
        a.axis
            .cmp(&b.axis)
            .then_with(|| a.group_a.cmp(&b.group_a))
            .then_with(|| a.group_b.cmp(&b.group_b))
    });
    corridors
}

fn append_adjacent_sibling_corridors(
    children: &mut [String],
    groups: &HashMap<String, GroupLayout>,
    corridors: &mut Vec<GroupCorridor>,
) {
    if children.len() < 2 {
        return;
    }
    children.sort();
    sort_siblings_along_stack_axis(children, groups);
    for w in children.windows(2) {
        let id_a = w[0].as_str();
        let id_b = w[1].as_str();
        if pair_covered(corridors, id_a, id_b) {
            continue;
        }
        let (Some(ga), Some(gb)) = (groups.get(id_a), groups.get(id_b)) else {
            continue;
        };
        push_corridor_between(ga, gb, id_a, id_b, corridors);
    }
}

fn sort_siblings_along_stack_axis(children: &mut [String], groups: &HashMap<String, GroupLayout>) {
    let vertical_stack = children
        .iter()
        .filter_map(|id| groups.get(id))
        .collect::<Vec<_>>();
    if vertical_stack.len() < 2 {
        return;
    }
    let mut dy_sum = 0.0;
    let mut dx_sum = 0.0;
    for i in 0..vertical_stack.len() {
        for j in (i + 1)..vertical_stack.len() {
            let ga = vertical_stack[i];
            let gb = vertical_stack[j];
            dy_sum += (ga.y - gb.y).abs();
            dx_sum += (ga.x - gb.x).abs();
        }
    }
    if dy_sum >= dx_sum {
        children.sort_by(|a, b| {
            let ga = groups.get(a).unwrap();
            let gb = groups.get(b).unwrap();
            ga.y
                .partial_cmp(&gb.y)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| ga.x.partial_cmp(&gb.x).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| a.cmp(b))
        });
    } else {
        children.sort_by(|a, b| {
            let ga = groups.get(a).unwrap();
            let gb = groups.get(b).unwrap();
            ga.x
                .partial_cmp(&gb.x)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| ga.y.partial_cmp(&gb.y).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| a.cmp(b))
        });
    }
}

/// 流程图堆叠排列：按拓扑序在 adjacent group 之间导出走廊。
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
    fn build_sibling_corridors_vertical_stack() {
        use crate::ast::{AttributeMap, Diagram, Group, Identifier, Span};
        use crate::types::DiagramType;

        let mut groups = HashMap::new();
        groups.insert(
            "cloud".to_string(),
            GroupLayout {
                x: 0.0,
                y: 0.0,
                width: 400.0,
                height: 400.0,
            },
        );
        groups.insert(
            "private_subnet".to_string(),
            GroupLayout {
                x: 10.0,
                y: 20.0,
                width: 180.0,
                height: 120.0,
            },
        );
        groups.insert(
            "data_subnet".to_string(),
            GroupLayout {
                x: 10.0,
                y: 180.0,
                width: 180.0,
                height: 100.0,
            },
        );
        let diagram = Diagram {
            diagram_type: DiagramType::Architecture,
            attributes: vec![],
            entities: vec![],
            relations: vec![],
            constraints: vec![],
            groups: vec![
                Group {
                    id: Identifier::new_unchecked("cloud"),
                    label: "cloud".to_string(),
                    attributes: AttributeMap::default(),
                    parent_id: None,
                    depth: 0,
                    entity_ids: vec![],
                    child_group_ids: vec![
                        Identifier::new_unchecked("private_subnet"),
                        Identifier::new_unchecked("data_subnet"),
                    ],
                    span: Span::dummy(),
                },
                Group {
                    id: Identifier::new_unchecked("private_subnet"),
                    label: "private".to_string(),
                    attributes: AttributeMap::default(),
                    parent_id: Some(Identifier::new_unchecked("cloud")),
                    depth: 1,
                    entity_ids: vec![],
                    child_group_ids: vec![],
                    span: Span::dummy(),
                },
                Group {
                    id: Identifier::new_unchecked("data_subnet"),
                    label: "data".to_string(),
                    attributes: AttributeMap::default(),
                    parent_id: Some(Identifier::new_unchecked("cloud")),
                    depth: 1,
                    entity_ids: vec![],
                    child_group_ids: vec![],
                    span: Span::dummy(),
                },
            ],
            style_decls: vec![],
            source_info: Default::default(),
            ..Default::default()
        };
        let corridors = build_sibling_corridors(&diagram, &groups);
        assert_eq!(corridors.len(), 1);
        assert_eq!(corridors[0].axis, CorridorAxis::Horizontal);
        let (ka, kb) = corridor_pair_key("private_subnet", "data_subnet");
        let (ca, cb) = corridor_pair_key(&corridors[0].group_a, &corridors[0].group_b);
        assert_eq!((ca, cb), (ka, kb));
    }

    #[test]
    #[ignore = "C2 激进间隙清除已暂缓：误杀 ecommerce 合法邻接；待 L1.1 betweenness 谓词"]
    fn build_corridors_skips_gap_blocked_by_third_group() {
        let mut groups = HashMap::new();
        groups.insert(
            "left".to_string(),
            GroupLayout {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 200.0,
            },
        );
        groups.insert(
            "mid".to_string(),
            GroupLayout {
                x: 120.0,
                y: 40.0,
                width: 80.0,
                height: 120.0,
            },
        );
        groups.insert(
            "right".to_string(),
            GroupLayout {
                x: 220.0,
                y: 0.0,
                width: 100.0,
                height: 200.0,
            },
        );
        let corridors = build_corridors_from_groups(&groups);
        assert!(
            !corridors.iter().any(|c| {
                let (a, b) = corridor_pair_key(&c.group_a, &c.group_b);
                let (l, r) = corridor_pair_key("left", "right");
                a == l && b == r
            }),
            "blocked left-right corridor must not exist: {corridors:?}"
        );
        assert!(
            corridors.iter().any(|c| {
                let (a, b) = corridor_pair_key(&c.group_a, &c.group_b);
                let (l, m) = corridor_pair_key("left", "mid");
                a == l && b == m
            }),
            "left-mid corridor expected"
        );
        assert!(
            corridors.iter().any(|c| {
                let (a, b) = corridor_pair_key(&c.group_a, &c.group_b);
                let (m, r) = corridor_pair_key("mid", "right");
                a == m && b == r
            }),
            "mid-right corridor expected"
        );
    }

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
    fn merge_corridors_prefers_final_geometry_over_stale_injected() {
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
        // 注入坐标故意偏离几何中线（模拟 group_frame 前的过期值）
        let injected = vec![GroupCorridor {
            axis: CorridorAxis::Vertical,
            coord: 999.0,
            span_min: 0.0,
            span_max: 100.0,
            group_a: "a".into(),
            group_b: "b".into(),
        }];
        let merged = merge_corridors(&injected, &groups);
        assert_eq!(merged.len(), 1);
        assert!(
            (merged[0].coord - 120.0).abs() < EPS,
            "geometry mid-gap should win, got {}",
            merged[0].coord
        );
    }
}
