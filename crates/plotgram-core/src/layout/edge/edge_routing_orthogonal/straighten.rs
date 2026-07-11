//! Preferred-alignment straightening for opposite-port edges.

use super::*;
use crate::layout::{NodeLayout, Port};
use std::collections::HashMap;

/// 判断两端口是否为正对端口对（可直线连接）。
/// 正对：Bottom-Top, Top-Bottom, Left-Right, Right-Left。
fn is_opposite_port_pair(from: Port, to: Port) -> bool {
    matches!(
        (from, to),
        (Port::Bottom, Port::Top)
            | (Port::Top, Port::Bottom)
            | (Port::Left, Port::Right)
            | (Port::Right, Port::Left)
    )
}

/// 直连偏好对齐：修正正对端口边因 slot 不对称导致的锚点错位。
///
/// 核心逻辑：
/// 1. 统计每个 (node_id, side, is_from) 上的端点数，识别 Single 端点（自由度最高）。
/// 2. 遍历所有边，检测正对端口对且节点在切线方向有投影重叠的边。
/// 3. 若一端是 Single（该侧该方向只有这一条边），调整其锚点切线坐标与另一端对齐。
/// 4. 若两端都是 Single，取两端节点中心连线的位置作为对齐坐标。
///
/// `parallel_offsets`：reverse pair 边的切线偏移；对齐到中线时写入 `center±offset`，
/// 避免抹平 step2 已施加的正反向分离。
pub fn straighten_preferred_alignments(
    nodes: &HashMap<String, NodeLayout>,
    n: usize,
    from_side: &[Port],
    to_side: &[Port],
    endpoint_map: &mut HashMap<(usize, bool), Endpoint>,
    parallel_offsets: &[f64],
) {
    use std::collections::HashMap;

    // 统计每个 (node_id, side, is_from) 上的端点数
    let mut side_dir_count: HashMap<(String, Port, bool), usize> = HashMap::new();
    for i in 0..n {
        if let Some(ep) = endpoint_map.get(&(i, true)) {
            *side_dir_count.entry((ep.node_id.clone(), ep.side, true)).or_insert(0) += 1;
        }
        if let Some(ep) = endpoint_map.get(&(i, false)) {
            *side_dir_count.entry((ep.node_id.clone(), ep.side, false)).or_insert(0) += 1;
        }
    }

    // 收集需要对齐的边及其目标切线坐标
    // 使用 Vec 收集后一次性应用，避免迭代过程中借用冲突
    let mut alignments: Vec<(usize, bool, f64)> = Vec::new(); // (edge_index, is_from, target_tangent)

    for i in 0..n {
        let fs = from_side[i];
        let ts = to_side[i];

        if !is_opposite_port_pair(fs, ts) {
            continue;
        }

        let Some(from_ep) = endpoint_map.get(&(i, true)) else { continue };
        let Some(to_ep) = endpoint_map.get(&(i, false)) else { continue };

        let Some(from_nl) = nodes.get(&from_ep.node_id) else { continue };
        let Some(to_nl) = nodes.get(&to_ep.node_id) else { continue };

        let vertical = is_vertical_port(fs); // Top/Bottom 端口 → 垂直连接，需对齐 x

        // 检查节点在切线方向上的投影重叠（垂直连接看 x 范围，水平连接看 y 范围）
        let overlap = if vertical {
            let fx1 = from_nl.x;
            let fx2 = from_nl.x + from_nl.width;
            let tx1 = to_nl.x;
            let tx2 = to_nl.x + to_nl.width;
            // 投影有重叠或非常接近（间隙 < 16px）即认为可直连
            let overlap_amt = range_overlap_local(fx1, fx2, tx1, tx2);
            let gap = if fx2 < tx1 {
                tx1 - fx2
            } else if tx2 < fx1 {
                fx1 - tx2
            } else {
                0.0
            };
            overlap_amt > EPS || gap < 16.0
        } else {
            let fy1 = from_nl.y;
            let fy2 = from_nl.y + from_nl.height;
            let ty1 = to_nl.y;
            let ty2 = to_nl.y + to_nl.height;
            let overlap_amt = range_overlap_local(fy1, fy2, ty1, ty2);
            let gap = if fy2 < ty1 {
                ty1 - fy2
            } else if ty2 < fy1 {
                fy1 - ty2
            } else {
                0.0
            };
            overlap_amt > EPS || gap < 16.0
        };

        if !overlap {
            continue;
        }

        let from_count = side_dir_count.get(&(from_ep.node_id.clone(), fs, true)).copied().unwrap_or(0);
        let to_count = side_dir_count.get(&(to_ep.node_id.clone(), ts, false)).copied().unwrap_or(0);

        let from_single = from_count == 1;
        let to_single = to_count == 1;

        let from_tangent = if vertical { from_ep.anchor.x } else { from_ep.anchor.y };
        let to_tangent = if vertical { to_ep.anchor.x } else { to_ep.anchor.y };

        let tangent_diff = (from_tangent - to_tangent).abs();

        // 已经对齐（差值 < 1px），无需调整
        if tangent_diff < 1.0 {
            continue;
        }

        match (from_single, to_single) {
            (true, true) => {
                // 两端都是 Single：取两端节点中心连线位置作为对齐坐标，最自然
                let base = if vertical {
                    let fc = from_nl.x + from_nl.width / 2.0;
                    let tc = to_nl.x + to_nl.width / 2.0;
                    (fc + tc) / 2.0
                } else {
                    let fc = from_nl.y + from_nl.height / 2.0;
                    let tc = to_nl.y + to_nl.height / 2.0;
                    (fc + tc) / 2.0
                };
                let offset = parallel_offsets.get(i).copied().unwrap_or(0.0);
                let target = base + offset;
                // 限制目标在节点边的有效范围内
                let target_clamped = if vertical {
                    let margin = from_nl.width * SLOT_MARGIN_RATIO;
                    target.clamp(from_nl.x + margin, from_nl.x + from_nl.width - margin)
                } else {
                    let margin = from_nl.height * SLOT_MARGIN_RATIO;
                    target.clamp(from_nl.y + margin, from_nl.y + from_nl.height - margin)
                };
                alignments.push((i, true, target_clamped));
                // to 端需要单独 clamp
                let target_clamped_to = if vertical {
                    let margin = to_nl.width * SLOT_MARGIN_RATIO;
                    target.clamp(to_nl.x + margin, to_nl.x + to_nl.width - margin)
                } else {
                    let margin = to_nl.height * SLOT_MARGIN_RATIO;
                    target.clamp(to_nl.y + margin, to_nl.y + to_nl.height - margin)
                };
                alignments.push((i, false, target_clamped_to));
            }
            (true, false) => {
                // from 端是 Single，to 端有多个边：将 from 端对齐到 to 端
                let target = to_tangent;
                let target_clamped = if vertical {
                    let margin = from_nl.width * SLOT_MARGIN_RATIO;
                    target.clamp(from_nl.x + margin, from_nl.x + from_nl.width - margin)
                } else {
                    let margin = from_nl.height * SLOT_MARGIN_RATIO;
                    target.clamp(from_nl.y + margin, from_nl.y + from_nl.height - margin)
                };
                alignments.push((i, true, target_clamped));
            }
            (false, true) => {
                // to 端是 Single，from 端有多个边：将 to 端对齐到 from 端
                let target = from_tangent;
                let target_clamped = if vertical {
                    let margin = to_nl.width * SLOT_MARGIN_RATIO;
                    target.clamp(to_nl.x + margin, to_nl.x + to_nl.width - margin)
                } else {
                    let margin = to_nl.height * SLOT_MARGIN_RATIO;
                    target.clamp(to_nl.y + margin, to_nl.y + to_nl.height - margin)
                };
                alignments.push((i, false, target_clamped));
            }
            (false, false) => {
                // 两端都有多个边：仅当两端节点的中心线高度对齐时（差值<16px）才强制对齐
                // 这种情况通常是垂直堆叠或水平排列的同级节点，直线连接视觉效果最佳
                let center_aligned = if vertical {
                    let fc = from_nl.x + from_nl.width / 2.0;
                    let tc = to_nl.x + to_nl.width / 2.0;
                    (fc - tc).abs() < 16.0
                } else {
                    let fc = from_nl.y + from_nl.height / 2.0;
                    let tc = to_nl.y + to_nl.height / 2.0;
                    (fc - tc).abs() < 16.0
                };

                if center_aligned {
                    let base = if vertical {
                        let fc = from_nl.x + from_nl.width / 2.0;
                        let tc = to_nl.x + to_nl.width / 2.0;
                        (fc + tc) / 2.0
                    } else {
                        let fc = from_nl.y + from_nl.height / 2.0;
                        let tc = to_nl.y + to_nl.height / 2.0;
                        (fc + tc) / 2.0
                    };
                    let offset = parallel_offsets.get(i).copied().unwrap_or(0.0);
                    let target = base + offset;
                    let target_clamped_from = if vertical {
                        let margin = from_nl.width * SLOT_MARGIN_RATIO;
                        target.clamp(from_nl.x + margin, from_nl.x + from_nl.width - margin)
                    } else {
                        let margin = from_nl.height * SLOT_MARGIN_RATIO;
                        target.clamp(from_nl.y + margin, from_nl.y + from_nl.height - margin)
                    };
                    let target_clamped_to = if vertical {
                        let margin = to_nl.width * SLOT_MARGIN_RATIO;
                        target.clamp(to_nl.x + margin, to_nl.x + to_nl.width - margin)
                    } else {
                        let margin = to_nl.height * SLOT_MARGIN_RATIO;
                        target.clamp(to_nl.y + margin, to_nl.y + to_nl.height - margin)
                    };
                    alignments.push((i, true, target_clamped_from));
                    alignments.push((i, false, target_clamped_to));
                }
            }
        }
    }

    // 应用对齐调整
    for (ei, is_from, target_tangent) in alignments {
        if let Some(ep) = endpoint_map.get_mut(&(ei, is_from)) {
            let vertical = is_vertical_port(ep.side);
            if vertical {
                ep.anchor.x = target_tangent;
            } else {
                ep.anchor.y = target_tangent;
            }
        }
    }
}
