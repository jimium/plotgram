//! Barnes-Hut 四叉树加速
//!
//! 将力导向布局中 O(V²) 的全对斥力计算降到 O(V log V)。
//! 通过空间划分树近似远距离节点簇的斥力。
//!
//! # 算法
//! 1. 将所有节点插入四叉树，每个内部节点存储子区域的质心和质量
//! 2. 对每个节点遍历树计算斥力：
//!    - 叶子节点：精确计算
//!    - 远距离内部节点（`cell_size / distance < theta`）：用质心近似
//!    - 近距离内部节点：递归子节点
//! 3. `theta` 越小越精确（theta=0 退化为 O(V²)），典型值 1.0~1.5

/// 四叉树节点
struct QuadTreeNode {
    /// 区域边界 (min_x, min_y, max_x, max_y)
    bounds: (f64, f64, f64, f64),
    /// 质心 x
    cx: f64,
    /// 质心 y
    cy: f64,
    /// 质量（节点数）
    mass: f64,
    /// 子节点（None = 叶子）
    children: Option<Box<[QuadTreeNode; 4]>>,
    /// 叶子节点中的单体索引（内部节点为 None）
    body: Option<usize>,
}

impl QuadTreeNode {
    fn new(bounds: (f64, f64, f64, f64)) -> Self {
        Self {
            bounds,
            cx: 0.0,
            cy: 0.0,
            mass: 0.0,
            children: None,
            body: None,
        }
    }

    fn is_leaf(&self) -> bool {
        self.children.is_none()
    }

    fn width(&self) -> f64 {
        self.bounds.2 - self.bounds.0
    }

    /// 插入一个节点（索引 idx，位置来自 positions 数组）
    fn insert(&mut self, idx: usize, positions: &[(f64, f64)]) {
        let (px, py) = positions[idx];

        if self.is_leaf() {
            if self.body.is_none() {
                self.body = Some(idx);
                self.cx = px;
                self.cy = py;
                self.mass = 1.0;
                return;
            }
            // 已有体，分裂
            let existing = self.body.take().unwrap();
            self.subdivide();
            self.insert_into_child(existing, positions);
            self.insert_into_child(idx, positions);
        } else {
            self.insert_into_child(idx, positions);
        }

        self.update_center_of_mass();
    }

    fn subdivide(&mut self) {
        let (min_x, min_y, max_x, max_y) = self.bounds;
        let mid_x = (min_x + max_x) / 2.0;
        let mid_y = (min_y + max_y) / 2.0;
        self.children = Some(Box::new([
            QuadTreeNode::new((min_x, min_y, mid_x, mid_y)),
            QuadTreeNode::new((mid_x, min_y, max_x, mid_y)),
            QuadTreeNode::new((min_x, mid_y, mid_x, max_y)),
            QuadTreeNode::new((mid_x, mid_y, max_x, max_y)),
        ]));
    }

    fn insert_into_child(&mut self, idx: usize, positions: &[(f64, f64)]) {
        let (px, py) = positions[idx];
        let (min_x, min_y, max_x, max_y) = self.bounds;
        let mid_x = (min_x + max_x) / 2.0;
        let mid_y = (min_y + max_y) / 2.0;
        let quadrant = if px < mid_x {
            if py < mid_y { 0 } else { 2 }
        } else if py < mid_y { 1 } else { 3 };
        self.children.as_mut().unwrap()[quadrant].insert(idx, positions);
    }

    fn update_center_of_mass(&mut self) {
        if let Some(children) = &self.children {
            let mut total_mass = 0.0;
            let mut total_cx = 0.0;
            let mut total_cy = 0.0;
            for child in children.iter() {
                if child.mass > 0.0 {
                    total_mass += child.mass;
                    total_cx += child.cx * child.mass;
                    total_cy += child.cy * child.mass;
                }
            }
            if total_mass > 0.0 {
                self.mass = total_mass;
                self.cx = total_cx / total_mass;
                self.cy = total_cy / total_mass;
            }
        }
    }

    /// 计算对目标节点 (target_idx) 的斥力向量。
    ///
    /// - `k`: FR 参数（理想距离）
    /// - `theta`: 开角阈值，越小越精确
    fn compute_repulsion(
        &self,
        target_idx: usize,
        positions: &[(f64, f64)],
        k: f64,
        theta: f64,
    ) -> (f64, f64) {
        let (px, py) = positions[target_idx];

        if self.is_leaf() {
            if let Some(idx) = self.body {
                if idx == target_idx {
                    return (0.0, 0.0);
                }
                let (ox, oy) = positions[idx];
                let dx = px - ox;
                let dy = py - oy;
                let dist = (dx * dx + dy * dy).sqrt().max(1e-6);
                let repulsive = (k * k) / dist;
                let factor = repulsive / dist;
                return (dx * factor, dy * factor);
            }
            return (0.0, 0.0);
        }

        let dx = px - self.cx;
        let dy = py - self.cy;
        let dist = (dx * dx + dy * dy).sqrt().max(1e-6);
        let cell_size = self.width();

        if cell_size / dist < theta {
            // 远距离：用质心近似
            let repulsive = (k * k * self.mass) / dist;
            let factor = repulsive / dist;
            (dx * factor, dy * factor)
        } else {
            // 近距离：递归子节点
            let mut fx = 0.0;
            let mut fy = 0.0;
            for child in self.children.as_ref().unwrap().iter() {
                let (cx, cy) = child.compute_repulsion(target_idx, positions, k, theta);
                fx += cx;
                fy += cy;
            }
            (fx, fy)
        }
    }
}

/// Barnes-Hut 四叉树，封装构建和查询。
pub struct BarnesHutTree {
    root: QuadTreeNode,
}

impl BarnesHutTree {
    /// 从节点位置数组构建四叉树。
    pub fn build(positions: &[(f64, f64)]) -> Self {
        if positions.is_empty() {
            return Self {
                root: QuadTreeNode::new((0.0, 0.0, 1.0, 1.0)),
            };
        }

        // 计算包围盒
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for &(x, y) in positions {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }

        // 确保包围盒为正方形且非零
        let size = (max_x - min_x).max(max_y - min_y).max(1.0) * 1.1;
        let cx = (min_x + max_x) / 2.0;
        let cy = (min_y + max_y) / 2.0;
        let half = size / 2.0;
        let bounds = (cx - half, cy - half, cx + half, cy + half);

        let mut root = QuadTreeNode::new(bounds);
        for idx in 0..positions.len() {
            root.insert(idx, positions);
        }

        Self { root }
    }

    /// 计算对目标节点的斥力向量。
    ///
    /// - `target_idx`: 目标节点索引
    /// - `positions`: 所有节点位置
    /// - `k`: FR 理想距离参数
    /// - `theta`: 开角阈值（推荐 1.0~1.5）
    pub fn repulsion(
        &self,
        target_idx: usize,
        positions: &[(f64, f64)],
        k: f64,
        theta: f64,
    ) -> (f64, f64) {
        self.root
            .compute_repulsion(target_idx, positions, k, theta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_node_no_force() {
        let positions = vec![(0.0, 0.0)];
        let tree = BarnesHutTree::build(&positions);
        let (fx, fy) = tree.repulsion(0, &positions, 10.0, 1.0);
        assert!(fx.abs() < 1e-6);
        assert!(fy.abs() < 1e-6);
    }

    #[test]
    fn test_two_nodes_repel() {
        let positions = vec![(0.0, 0.0), (100.0, 0.0)];
        let tree = BarnesHutTree::build(&positions);
        let (fx, _) = tree.repulsion(0, &positions, 10.0, 1.0);
        // 节点 0 应被推向左侧（负 x 方向）
        assert!(fx < 0.0, "node 0 should be pushed left, got fx={fx}");
    }

    #[test]
    fn test_many_nodes_approximate() {
        // 100 个节点，Barnes-Hut 应正常工作
        let positions: Vec<(f64, f64)> = (0..100)
            .map(|i| ((i as f64) * 10.0, (i as f64 % 7.0) * 10.0))
            .collect();
        let tree = BarnesHutTree::build(&positions);
        let (fx, fy) = tree.repulsion(0, &positions, 50.0, 1.2);
        // 应有非零斥力
        assert!(fx.abs() + fy.abs() > 0.0);
    }
}
