//! 多连通分量矩形打包模块
//!
//! 当图有多个不连通分量时，将每个分量独立布局后，
//! 用矩形打包算法紧凑排列，节省画布空间。
//!
//! 参考 Graphviz pack 库的思路，使用 shelf-packing 算法：
//! - 按高度（或面积）降序排列分量
//! - 使用 shelf（货架）策略：在当前行放置，放不下则换行
//! - 保证分量不重叠，并尽量紧凑
//!
//! 支持两种打包模式：
//! - `row`: 简单行排列（从左到右）
//! - `shelf`: 货架式打包（更紧凑，类似二维装箱）

use crate::layout::{LayoutResult, NodeLayout};
use std::collections::HashMap;

/// 分量边界框
#[derive(Debug, Clone)]
pub struct ComponentBounds {
    /// 分量内的节点布局
    pub nodes: HashMap<String, NodeLayout>,
    /// 包围框宽度
    pub width: f64,
    /// 包围框高度
    pub height: f64,
}

/// 打包模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackMode {
    /// 简单行排列
    Row,
    /// 货架式打包
    Shelf,
}

/// 打包结果
pub struct PackResult {
    /// 打包后的节点布局
    pub nodes: HashMap<String, NodeLayout>,
    /// 总画布宽度
    pub total_width: f64,
    /// 总画布高度
    pub total_height: f64,
}

/// 将多个分量打包排列
///
/// # Arguments
/// - `components`: 各分量的包围框信息
/// - `mode`: 打包模式
/// - `gap`: 分量间距
/// - `padding`: 画布边距
pub fn pack_components(
    components: Vec<ComponentBounds>,
    mode: PackMode,
    gap: f64,
    padding: f64,
) -> PackResult {
    if components.is_empty() {
        return PackResult {
            nodes: HashMap::new(),
            total_width: padding * 2.0,
            total_height: padding * 2.0,
        };
    }

    match mode {
        PackMode::Row => pack_row(components, gap, padding),
        PackMode::Shelf => pack_shelf(components, gap, padding),
    }
}

/// 简单行排列：所有分量排成一行
fn pack_row(
    mut components: Vec<ComponentBounds>,
    gap: f64,
    padding: f64,
) -> PackResult {
    // 按宽度降序排列（大分量先放）
    components.sort_by(|a, b| b.width.partial_cmp(&a.width).unwrap_or(std::cmp::Ordering::Equal));

    let mut nodes = HashMap::new();
    let mut cursor_x = padding;
    let mut max_height = 0.0f64;

    for comp in &components {
        // 将分量内节点平移到当前位置
        let min_x = comp.nodes.values().map(|n| n.x).fold(f64::MAX, f64::min);
        for (id, nl) in &comp.nodes {
            nodes.insert(id.clone(), NodeLayout {
                x: nl.x - min_x + cursor_x,
                y: nl.y,
                width: nl.width,
                height: nl.height,
                ..Default::default()
            });
        }

        cursor_x += comp.width + gap;
        max_height = max_height.max(comp.height);
    }

    // 重新计算总尺寸
    let total_width = if nodes.is_empty() {
        padding * 2.0
    } else {
        nodes.values().map(|n| n.x + n.width).fold(0.0_f64, f64::max) + padding
    };
    let total_height = max_height + padding;

    PackResult {
        nodes,
        total_width,
        total_height,
    }
}

/// 货架式打包：类似二维装箱的 shelf 算法
///
/// 策略：
/// 1. 按高度降序排列分量
/// 2. 在当前行放置分量（从左到右）
/// 3. 如果当前行放不下，新开一行（以当前分量高度为行高）
/// 4. 在每行内部，按剩余宽度贪心放置
fn pack_shelf(
    mut components: Vec<ComponentBounds>,
    gap: f64,
    padding: f64,
) -> PackResult {
    if components.is_empty() {
        return PackResult {
            nodes: HashMap::new(),
            total_width: padding * 2.0,
            total_height: padding * 2.0,
        };
    }

    // 按高度降序排列
    components.sort_by(|a, b| b.height.partial_cmp(&a.height).unwrap_or(std::cmp::Ordering::Equal));

    let mut nodes = HashMap::new();
    let mut cursor_y = padding;
    let mut max_total_width = 0.0f64;
    let mut i = 0;

    while i < components.len() {
        let shelf_height = components[i].height;
        let mut shelf_x = padding;

        while i < components.len() {
            let comp = &components[i];

            // 检查当前行能否放下（宽度约束）
            if shelf_x > padding && shelf_x + comp.width > max_total_width.max(800.0) && shelf_x > padding + 200.0 {
                // 换行
                break;
            }

            let min_x = comp.nodes.values().map(|n| n.x).fold(f64::MAX, f64::min);
            let min_y = comp.nodes.values().map(|n| n.y).fold(f64::MAX, f64::min);
            for (id, nl) in &comp.nodes {
                nodes.insert(id.clone(), NodeLayout {
                    x: nl.x - min_x + shelf_x,
                    y: nl.y - min_y + cursor_y,
                    width: nl.width,
                    height: nl.height,
                    ..Default::default()
                });
            }

            shelf_x += comp.width + gap;
            max_total_width = max_total_width.max(shelf_x);
            i += 1;
        }

        cursor_y += shelf_height + gap;
    }

    let total_width = max_total_width + padding;
    let total_height = cursor_y + padding;

    PackResult {
        nodes,
        total_width,
        total_height,
    }
}

/// 从 LayoutResult 提取分量边界
///
/// 根据连通分量信息，将 LayoutResult 中的节点按分量分组。
pub fn extract_component_bounds(
    result: &LayoutResult,
    component_ids: &[Vec<String>],
) -> Vec<ComponentBounds> {
    component_ids
        .iter()
        .map(|ids| {
            let mut comp_nodes = HashMap::new();
            let mut min_x = f64::MAX;
            let mut min_y = f64::MAX;
            let mut max_x = 0.0f64;
            let mut max_y = 0.0f64;

            for id in ids {
                if let Some(nl) = result.nodes.get(id) {
                    min_x = min_x.min(nl.x);
                    min_y = min_y.min(nl.y);
                    max_x = max_x.max(nl.x + nl.width);
                    max_y = max_y.max(nl.y + nl.height);
                    comp_nodes.insert(id.clone(), NodeLayout {
                        x: nl.x,
                        y: nl.y,
                        width: nl.width,
                        height: nl.height,
                        ..Default::default()
                    });
                }
            }

            ComponentBounds {
                nodes: comp_nodes,
                width: if max_x >= min_x { max_x - min_x } else { 0.0 },
                height: if max_y >= min_y { max_y - min_y } else { 0.0 },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: &str, x: f64, y: f64, w: f64, h: f64) -> (String, NodeLayout) {
        (id.to_string(), NodeLayout { x, y, width: w, height: h, ..Default::default() })
    }

    #[test]
    fn pack_row_arranges_components_horizontally() {
        let comp1 = ComponentBounds {
            nodes: HashMap::from([make_node("a", 0.0, 0.0, 100.0, 50.0)]),
            width: 100.0,
            height: 50.0,
        };
        let comp2 = ComponentBounds {
            nodes: HashMap::from([make_node("b", 0.0, 0.0, 120.0, 60.0)]),
            width: 120.0,
            height: 60.0,
        };

        let result = pack_components(vec![comp1, comp2], PackMode::Row, 20.0, 10.0);
        assert_eq!(result.nodes.len(), 2);

        let a = result.nodes.get("a").unwrap();
        let b = result.nodes.get("b").unwrap();
        // 按宽度排序，b(120) 比 a(100) 宽，先放置 b
        let _ = (a, b);
        assert!((a.x - b.x).abs() > 50.0, "components should be separated");
    }

    #[test]
    fn pack_shelf_handles_wrapping() {
        let comp1 = ComponentBounds {
            nodes: HashMap::from([make_node("a", 0.0, 0.0, 600.0, 50.0)]),
            width: 600.0,
            height: 50.0,
        };
        let comp2 = ComponentBounds {
            nodes: HashMap::from([make_node("b", 0.0, 0.0, 300.0, 60.0)]),
            width: 300.0,
            height: 60.0,
        };

        let result = pack_components(vec![comp1, comp2], PackMode::Shelf, 20.0, 10.0);
        assert_eq!(result.nodes.len(), 2);
        // 按高度排序，comp2(60) 比 comp1(50) 高，comp2 先放置
        // comp1 宽度大，换行放在 comp2 下方
        let a = result.nodes.get("a").unwrap();
        let b = result.nodes.get("b").unwrap();
        assert!(a.y > b.y + b.height * 0.5, "a should be on row below b");
    }

    #[test]
    fn pack_empty_returns_minimal_bounds() {
        let result = pack_components(vec![], PackMode::Row, 20.0, 10.0);
        assert!(result.nodes.is_empty());
        assert_eq!(result.total_width, 20.0);
        assert_eq!(result.total_height, 20.0);
    }
}