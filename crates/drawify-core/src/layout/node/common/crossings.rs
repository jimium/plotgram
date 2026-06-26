//! 跨越数（crossing count）计算共享工具
//!
//! 提供 O(E log V) 的层间交叉数计算，基于 Fenwick Tree（BIT）扫描线算法。
//!
//! ## 算法
//!
//! 对相邻两层 (upper, lower) 的边集合，按 upper 节点位置升序遍历，
//! 用 BIT 维护已插入边的 lower 位置分布。对每条边 (u, l)：
//!
//! `crossings += 已插入边数 - bit.query(l + 1)`
//!
//! 即"位置大于 l 的已插入边数"。
//!
//! ## 复杂度
//!
//! - 时间：O(E log V)，E 为跨层边数，V 为 lower 层节点数
//! - 空间：O(V)
//!
//! ## 消费者
//!
//! - `sugiyama_v2::order::count_crossings` —— 通用 Sugiyama 排序阶段
//! - `architecture_v2::layout::count_layer_crossings` —— 架构图分组感知排序阶段
//!
//! ## 确定性
//!
//! 边按 (u_pos, l_pos) 升序排序后扫描，同源边不互相计入交叉，
//! 结果与输入顺序无关，符合 AGENTS.md §2 确定性要求。

/// 树状数组（Fenwick Tree / Binary Indexed Tree）
///
/// 支持 O(log n) 的单点更新与前缀和查询，用于扫描线统计逆序对。
pub(super) struct FenwickTree {
    tree: Vec<i64>,
}

impl FenwickTree {
    /// 构造大小为 `size` 的空树（索引 1..=size 可用）。
    pub fn new(size: usize) -> Self {
        Self {
            tree: vec![0; size + 1],
        }
    }

    /// 在索引 `idx`（1-based）处累加 `delta`。
    pub fn update(&mut self, idx: usize, delta: i64) {
        let mut i = idx;
        while i < self.tree.len() {
            self.tree[i] += delta;
            i += i & i.wrapping_neg();
        }
    }

    /// 查询索引 `idx`（1-based）处的前缀和（即 <= idx 的元素总数）。
    pub fn query(&self, idx: usize) -> i64 {
        let mut sum = 0;
        let mut i = idx.min(self.tree.len() - 1);
        while i > 0 {
            sum += self.tree[i];
            i -= i & i.wrapping_neg();
        }
        sum
    }
}

/// 计算相邻两层之间的边交叉数（O(E log V) 扫描线算法）
///
/// ## 参数
///
/// - `upper_edges`：上层节点索引 → 下层节点索引 的边列表
/// - `lower_len`：下层节点数（用于确定 BIT 大小）
///
/// ## 返回
///
/// 交叉数（同源边不互相计入）
///
/// ## 确定性
///
/// 边按 (u_pos, l_pos) 升序排序后扫描，结果与输入顺序无关。
pub fn count_crossings_from_edges(upper_edges: &[(usize, usize)], lower_len: usize) -> usize {
    if upper_edges.is_empty() {
        return 0;
    }

    // 按 u_pos 升序，同 u_pos 时 l_pos 升序：扫描时同源边不会互相计入交叉。
    let mut edges = upper_edges.to_vec();
    edges.sort();

    // BIT 扫描线：对每条边 (u, l)，交叉数 += 已插入边中 l_pos > 当前 l_pos 的数量。
    let mut bit = FenwickTree::new(lower_len + 1);
    let mut crossings = 0usize;
    let mut inserted = 0usize;

    for (_, l_pos) in &edges {
        // 已插入中 l_pos <= 当前的数量
        let le = bit.query(l_pos + 1) as usize;
        // 交叉 = 已插入总数 - l_pos <= 当前的数量
        crossings += inserted - le;
        bit.update(l_pos + 1, 1);
        inserted += 1;
    }

    crossings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fenwick_tree_basic() {
        let mut bit = FenwickTree::new(8);
        bit.update(3, 1);
        bit.update(5, 1);
        bit.update(3, 1);
        assert_eq!(bit.query(3), 2); // 索引 <=3 的和 = 2
        assert_eq!(bit.query(4), 2); // 索引 <=4 的和 = 2
        assert_eq!(bit.query(5), 3); // 索引 <=5 的和 = 3
        assert_eq!(bit.query(8), 3); // 全部 = 3
    }

    #[test]
    fn count_crossings_empty() {
        assert_eq!(count_crossings_from_edges(&[], 0), 0);
    }

    #[test]
    fn count_crossings_no_crossing() {
        // 两条平行边：(0,0) 和 (1,1)，无交叉
        let edges = vec![(0, 0), (1, 1)];
        assert_eq!(count_crossings_from_edges(&edges, 2), 0);
    }

    #[test]
    fn count_crossings_one_crossing() {
        // 两条交叉边：(0,1) 和 (1,0)，1 个交叉
        let edges = vec![(0, 1), (1, 0)];
        assert_eq!(count_crossings_from_edges(&edges, 2), 1);
    }

    #[test]
    fn count_crossings_same_source_no_crossing() {
        // 同源边：(0,0) 和 (0,1)，不互相计入交叉
        let edges = vec![(0, 0), (0, 1)];
        assert_eq!(count_crossings_from_edges(&edges, 2), 0);
    }

    #[test]
    fn count_crossings_multiple() {
        // 4 条边：(0,0), (0,2), (1,1), (2,0)
        // 排序后：(0,0), (0,2), (1,1), (2,0)
        // (0,0): inserted=0, crossings=0
        // (0,2): inserted=1, le=query(3)=1, crossings=0
        // (1,1): inserted=2, le=query(2)=1, crossings=1
        // (2,0): inserted=3, le=query(1)=1, crossings=3
        let edges = vec![(0, 0), (0, 2), (1, 1), (2, 0)];
        assert_eq!(count_crossings_from_edges(&edges, 3), 3);
    }
}
