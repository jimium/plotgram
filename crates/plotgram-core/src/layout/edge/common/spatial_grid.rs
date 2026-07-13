//! 通用均匀网格空间索引。
//!
//! 将画布按固定大小 cell 划分，支持按 bbox 范围插入和查询。
//! `SegmentGrid`（正交路由段索引）和 `ObstacleGrid`（可见性图障碍物索引）
//! 分别实例化此结构，共享 cell 坐标计算与存储逻辑。

use std::collections::HashMap;

/// 均匀网格空间索引，泛型化 cell 存储值类型。
///
/// 每个 cell 存储一组 `V` 值；插入时按 bbox 覆盖的 cells 写入，
/// 查询时按 bbox 覆盖的 cells 读取（不去重，由调用方处理）。
pub struct SpatialGrid<V> {
    cell_size: f64,
    cells: HashMap<(i32, i32), Vec<V>>,
}

impl<V> SpatialGrid<V> {
    /// 创建指定 cell 大小的空网格。
    pub fn new(cell_size: f64) -> Self {
        Self {
            cell_size,
            cells: HashMap::new(),
        }
    }

    /// 返回 cell 边长。
    #[inline]
    pub fn cell_size(&self) -> f64 {
        self.cell_size
    }

    /// 将坐标转换为 cell 索引。
    #[inline]
    pub fn cell_of(&self, x: f64, y: f64) -> (i32, i32) {
        (
            (x / self.cell_size).floor() as i32,
            (y / self.cell_size).floor() as i32,
        )
    }

    /// 返回 bbox 覆盖的 cell 范围 `(cx0, cx1, cy0, cy1)`（含端点）。
    #[inline]
    pub fn cell_range(
        &self,
        xmin: f64,
        xmax: f64,
        ymin: f64,
        ymax: f64,
    ) -> (i32, i32, i32, i32) {
        (
            (xmin / self.cell_size).floor() as i32,
            (xmax / self.cell_size).floor() as i32,
            (ymin / self.cell_size).floor() as i32,
            (ymax / self.cell_size).floor() as i32,
        )
    }

    /// 在指定 cell 范围内的每个 cell 插入一个值（值会被复制到每个 cell）。
    pub fn insert_range(&mut self, cx0: i32, cx1: i32, cy0: i32, cy1: i32, value: V)
    where
        V: Copy,
    {
        for cx in cx0..=cx1 {
            for cy in cy0..=cy1 {
                self.cells.entry((cx, cy)).or_default().push(value);
            }
        }
    }

    /// 获取所有 cell 的只读引用（用于自定义查询循环）。
    pub fn cells(&self) -> &HashMap<(i32, i32), Vec<V>> {
        &self.cells
    }

    /// 获取所有 cell 的可变引用（用于排序/去重等后处理）。
    pub fn cells_mut(&mut self) -> &mut HashMap<(i32, i32), Vec<V>> {
        &mut self.cells
    }

    /// 清空所有 cell。
    pub fn clear(&mut self) {
        self.cells.clear();
    }

    /// 返回 cell 数量。
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// 是否为空。
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
}
