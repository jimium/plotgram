//! Phase 5+: 重叠消除、钳制与画布尺寸。

use crate::layout::NodeLayout;
use std::collections::HashMap;

use super::constants::PADDING;

// ─── Phase 5.5: 钳制到非负区域 ───────────────────────────

/// 防御性保险：把任何落在画布外的节点位置平移回画布内。
///
/// 算法：找出所有节点最小的 x 坐标；如果小于 PADDING，整体平移；
/// 同时逐个把 x 钳到不低于 PADDING。
pub(crate) fn clamp_to_canvas(
    nodes: &mut HashMap<String, NodeLayout>,
    _sizes: &HashMap<String, (f64, f64)>,
) {
    if nodes.is_empty() {
        return;
    }

    let min_x = nodes.values().map(|n| n.x).fold(f64::INFINITY, f64::min);
    if min_x < PADDING {
        let shift = PADDING - min_x;
        for nl in nodes.values_mut() {
            nl.x += shift;
        }
    }

    // 再次逐个保险：x 至少为 PADDING
    for nl in nodes.values_mut() {
        if nl.x < PADDING {
            nl.x = PADDING;
        }
        if nl.y < PADDING {
            nl.y = PADDING;
        }
    }
}

// ═══════════════════════════════════════════════════════════
//  单元测试
// ═══════════════════════════════════════════════════════════
