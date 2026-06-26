# 布局与边路由重构执行计划（面向 AI Agent）

> **目标**：通过小步、原子化、可验证的重构，消除代码重复、提升可读性和可扩展性，**不改变任何外部行为**。
>
> **重构铁则**：
> 1. 每完成一个任务必须运行 `cargo test -p drawify-core` 确保所有测试通过
> 2. 不允许在重构过程中修复 bug 或新增功能（纯结构调整）
> 3. 所有 HashMap 迭代必须显式排序（遵循 AGENTS.md §2 确定性要求）
> 4. 使用现有测试作为安全网，测试失败立即回退该步骤
> 5. 每一步都是独立可提交的 PR 粒度

---

## 前置检查清单（Agent 开始前必须确认）

- [ ] 代码库在 main 分支，工作区干净
- [ ] 运行 `cargo test -p drawify-core` 全部通过（基线）
- [ ] 运行 `cargo check` 无警告
- [ ] 记录当前 commit hash，便于回退

基线测试命令：
```bash
cd /Users/jimichan/zaprt-projects/flowml
cargo test -p drawify-core -- --nocapture 2>&1 | tail -50
```

---

## Phase 1：几何原语抽象（零风险，高收益）

**预计执行时间**：3-5 轮 Agent 任务
**核心目标**：引入 Point/Rect/Axis，消除 path.rs 中的镜像重复代码

---

### Task 1.1：创建 geometry.rs 基础模块

**文件操作**：
- 新建：`crates/drawify-core/src/layout/geometry.rs`
- 修改：`crates/drawify-core/src/layout/mod.rs`（添加 `pub mod geometry;`）

**具体步骤**：

1. 创建 `geometry.rs`，定义以下类型：

```rust
//! 布局几何原语
//!
//! 提供 Point / Rect / Axis 统一抽象，消除 (f64,f64) 元组的语义模糊，
//! 并通过 Axis 统一水平/垂直方向的镜像代码。

use std::fmt;

/// 二维点
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn zero() -> Self {
        Self::new(0.0, 0.0)
    }

    pub fn add(self, other: Point) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }

    pub fn sub(self, other: Point) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }

    pub fn scale(self, s: f64) -> Self {
        Self::new(self.x * s, self.y * s)
    }

    pub fn dot(self, other: Point) -> f64 {
        self.x * other.x + self.y * other.y
    }

    pub fn length(self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn distance_to(self, other: Point) -> f64 {
        (self - other).length()
    }

    pub fn lerp(self, other: Point, t: f64) -> Point {
        self + (other - self).scale(t)
    }
}

impl std::ops::Add for Point {
    type Output = Point;
    fn add(self, rhs: Point) -> Self::Output { self.add(rhs) }
}

impl std::ops::Sub for Point {
    type Output = Point;
    fn sub(self, rhs: Point) -> Self::Output { self.sub(rhs) }
}

impl std::ops::Mul<f64> for Point {
    type Output = Point;
    fn mul(self, rhs: f64) -> Self::Output { self.scale(rhs) }
}

impl From<(f64, f64)> for Point {
    fn from((x, y): (f64, f64)) -> Self { Self::new(x, y) }
}

impl From<Point> for (f64, f64) {
    fn from(p: Point) -> Self { (p.x, p.y) }
}

/// 轴对齐矩形
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }

    pub fn from_points(min: Point, max: Point) -> Self {
        Self::new(min.x, min.y, max.x - min.x, max.y - min.y)
    }

    pub fn left(&self) -> f64 { self.x }
    pub fn top(&self) -> f64 { self.y }
    pub fn right(&self) -> f64 { self.x + self.width }
    pub fn bottom(&self) -> f64 { self.y + self.height }
    pub fn center(&self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
    pub fn top_left(&self) -> Point { Point::new(self.x, self.y) }
    pub fn top_right(&self) -> Point { Point::new(self.right(), self.y) }
    pub fn bottom_left(&self) -> Point { Point::new(self.x, self.bottom()) }
    pub fn bottom_right(&self) -> Point { Point::new(self.right(), self.bottom()) }

    pub fn size(&self) -> (f64, f64) { (self.width, self.height) }

    /// 扩展/收缩矩形（正数向外，负数向内）
    pub fn expanded(&self, pad: f64) -> Self {
        Self::new(self.x - pad, self.y - pad, self.width + 2.0 * pad, self.height + 2.0 * pad)
    }

    /// 平移矩形
    pub fn translate(&self, dx: f64, dy: f64) -> Self {
        Self::new(self.x + dx, self.y + dy, self.width, self.height)
    }

    /// 点是否在矩形内（含边界，pad 为额外膨胀量）
    pub fn contains_point(&self, p: Point, pad: f64) -> bool {
        let r = self.expanded(pad);
        p.x >= r.x && p.x <= r.right() && p.y >= r.y && p.y <= r.bottom()
    }

    /// 两个矩形是否相交（pad 为额外膨胀量）
    pub fn intersects_rect(&self, other: &Rect, pad: f64) -> bool {
        let a = self.expanded(pad);
        let b = other.expanded(pad);
        a.x < b.right() && a.right() > b.x && a.y < b.bottom() && a.bottom() > b.y
    }

    /// 线段是否穿过矩形
    pub fn intersects_segment(&self, a: Point, b: Point, pad: f64) -> bool {
        let r = self.expanded(pad);
        // 端点在矩形内
        if r.contains_point(a, 0.0) || r.contains_point(b, 0.0) {
            return true;
        }
        // 线段与矩形四条边相交检测（分离轴定理简化版）
        segment_intersects_segment(a, b, r.top_left(), r.top_right())
            || segment_intersects_segment(a, b, r.top_right(), r.bottom_right())
            || segment_intersects_segment(a, b, r.bottom_right(), r.bottom_left())
            || segment_intersects_segment(a, b, r.bottom_left(), r.top_left())
    }

    /// 返回矩形沿 axis 主轴的 (min, max) 范围
    pub fn range_on_axis(&self, axis: Axis) -> (f64, f64) {
        match axis {
            Axis::Horizontal => (self.left(), self.right()),
            Axis::Vertical => (self.top(), self.bottom()),
        }
    }

    /// 返回矩形沿 axis 交叉轴的 (min, max) 范围
    pub fn cross_range_on_axis(&self, axis: Axis) -> (f64, f64) {
        match axis {
            Axis::Horizontal => (self.top(), self.bottom()),
            Axis::Vertical => (self.left(), self.right()),
        }
    }
}

/// 坐标轴方向（用于消除水平/垂直镜像代码）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    /// 主轴沿 x 方向（left-to-right 布局时的主轴）
    Horizontal,
    /// 主轴沿 y 方向（top-to-bottom 布局时的主轴，默认）
    Vertical,
}

impl Axis {
    pub fn other(self) -> Self {
        match self {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        }
    }

    /// 获取点在主轴上的坐标
    pub fn main_coord(self, p: Point) -> f64 {
        match self {
            Axis::Horizontal => p.x,
            Axis::Vertical => p.y,
        }
    }

    /// 获取点在交叉轴上的坐标
    pub fn cross_coord(self, p: Point) -> f64 {
        match self {
            Axis::Horizontal => p.y,
            Axis::Vertical => p.x,
        }
    }

    /// 用主轴坐标和交叉轴坐标构建点
    pub fn point(self, main: f64, cross: f64) -> Point {
        match self {
            Axis::Horizontal => Point::new(main, cross),
            Axis::Vertical => Point::new(cross, main),
        }
    }

    /// 主轴方向的单位向量
    pub fn main_dir(self, sign: f64) -> Point {
        match self {
            Axis::Horizontal => Point::new(sign, 0.0),
            Axis::Vertical => Point::new(0.0, sign),
        }
    }

    /// 交叉轴方向的单位向量
    pub fn cross_dir(self, sign: f64) -> Point {
        match self {
            Axis::Horizontal => Point::new(0.0, sign),
            Axis::Vertical => Point::new(sign, 0.0),
        }
    }

    /// 对应 Port 侧是否沿该轴方向
    pub fn is_port_on_axis(self, port: crate::layout::Port) -> bool {
        use crate::layout::Port;
        match self {
            Axis::Horizontal => matches!(port, Port::Left | Port::Right),
            Axis::Vertical => matches!(port, Port::Top | Port::Bottom),
        }
    }
}

/// 线段相交检测（辅助函数）
fn segment_intersects_segment(a1: Point, a2: Point, b1: Point, b2: Point) -> bool {
    let d1 = cross(b2 - b1, a1 - b1);
    let d2 = cross(b2 - b1, a2 - b1);
    let d3 = cross(a2 - a1, b1 - a1);
    let d4 = cross(a2 - a1, b2 - a1);

    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    false
}

fn cross(a: Point, b: Point) -> f64 {
    a.x * b.y - a.y * b.x
}

// ─── NodeLayout → Rect 转换 ───
impl From<&crate::layout::NodeLayout> for Rect {
    fn from(nl: &crate::layout::NodeLayout) -> Self {
        Self::new(nl.x, nl.y, nl.width, nl.height)
    }
}

impl From<&crate::layout::GroupLayout> for Rect {
    fn from(gl: &crate::layout::GroupLayout) -> Self {
        Self::new(gl.x, gl.y, gl.width, gl.height)
    }
}

/// 计算节点中心（便利函数，取代 (nl.x + nl.width/2, nl.y + nl.height/2) 的重复）
pub fn node_center(nl: &crate::layout::NodeLayout) -> Point {
    Point::new(nl.x + nl.width / 2.0, nl.y + nl.height / 2.0)
}

/// 浮点比较容差常量
pub const EPS: f64 = 0.1;
```

2. 在 `mod.rs` 添加 `pub mod geometry;`（在现有 mod 声明附近）

3. 运行验证：
```bash
cargo check -p drawify-core
cargo test -p drawify-core geometry 2>&1 | tail -20
```

**完成标准**：
- [ ] `cargo check` 通过
- [ ] 不修改任何其他文件
- [ ] 可以运行 `cargo test -p drawify-core` 全部通过

---

### Task 1.2：迁移 PathGeometry 到 Point

**文件操作**：
- 修改：`crates/drawify-core/src/layout/mod.rs`（PathGeometry 定义处）
- 影响：所有使用 PathGeometry 的文件（主要是 edge/ 目录）

**具体步骤**：

1. 将 `PathGeometry` 中的 `(f64, f64)` 替换为 `Point`：

```rust
pub enum PathGeometry {
    Straight {
        start: Point,
        end: Point,
    },
    Bezier {
        start: Point,
        end: Point,
        controls: [Point; 2],
    },
    Polyline {
        points: Vec<Point>,
    },
}
```

2. 更新 `PathGeometry` 的所有方法签名（`translate`、`sample`、`anchor_points` 等），内部使用 Point 运算

3. **这一步会产生大量编译错误**，按以下顺序修复：
   - `edge/edge_routing_orthogonal/path.rs`：所有返回 `Vec<(f64,f64)>` 的路径函数改为 `Vec<Point>`
   - `edge/edge_routing_orthogonal/mod.rs`：`set_polyline_points` 接受 `Vec<Point>`
   - `edge/edge_routing_bezier.rs`：控制点计算改为 Point
   - `edge/edge_routing_spline.rs`：路径点改为 Point
   - `edge/edge_routing_organic.rs`：路径点改为 Point
   - `edge/common/*.rs`：所有点运算改为 Point
   - `refine/*.rs`：穿障检测改用 Rect/Pt

4. **关键**：不要重写任何算法逻辑，只做类型替换。例如：
   - `(x, y)` → `Point::new(x, y)`
   - `p.0` → `p.x`, `p.1` → `p.y`
   - 两点距离：`((x1-x2).powi(2) + (y1-y2).powi(2)).sqrt()` → `p1.distance_to(p2)`
   - 节点中心：`(nl.x + nl.width/2.0, nl.y + nl.height/2.0)` → `node_center(nl)` 或 `Rect::from(nl).center()`

5. 更新 `EdgeLayout::set_polyline_points`：
```rust
pub fn set_polyline_points(&mut self, points: Vec<Point>) {
    self.geometry = if points.len() <= 2 {
        PathGeometry::Straight {
            start: points[0],
            end: points[1],
        }
    } else {
        PathGeometry::Polyline { points }
    };
}
```

6. 验证：
```bash
cargo check -p drawify-core 2>&1 | head -100  # 确保无编译错误
cargo test -p drawify-core 2>&1 | tail -30
```

**完成标准**：
- [ ] 全部编译通过
- [ ] 全部测试通过
- [ ] 所有路径几何内部使用 Point，不再有裸 `(f64,f64)` 元组
- [ ] `cargo clippy -p drawify-core` 无新增警告

---

### Task 1.3：在 path.rs 引入 Axis 抽象，消除 Z-fold 镜像代码

**文件操作**：
- 修改：`crates/drawify-core/src/layout/edge/edge_routing_orthogonal/path.rs`

**具体步骤**：

1. 在文件顶部添加导入：
```rust
use crate::layout::geometry::{Axis, Point, Rect, EPS as GEOM_EPS};
```

2. 将文件顶部的 `EPS` 常量替换为引用 `GEOM_EPS`（或统一为 `geometry::EPS`）

3. **重构 `build_obstacle_aware_z_folds`**：
   
   当前结构：
   ```rust
   if from_vertical || mixed {
       // 30+ 行 Y 方向折点生成...
   }
   if !from_vertical || mixed {
       // 30+ 行 X 方向折点生成（镜像）...
   }
   ```
   
   重构为一个内部通用函数，通过 Axis 参数区分方向：
   ```rust
   fn build_obstacle_aware_z_folds(...) -> Vec<Vec<Point>> {
       let mut candidates = Vec::new();
       // 垂直轴方向折点（Top/Bottom 端口）
       if from_vertical || mixed {
           candidates.extend(generate_axis_folds(Axis::Vertical, ...));
       }
       // 水平轴方向折点（Left/Right 端口）
       if !from_vertical || mixed {
           candidates.extend(generate_axis_folds(Axis::Horizontal, ...));
       }
       candidates
   }

   fn generate_axis_folds(
       axis: Axis,
       sx: f64, sy: f64, from_side: Port,
       ex: f64, ey: f64, to_side: Port,
       ctx: &RoutingContext,
       endpoint_groups: &HashSet<&str>,
       s1: Point, e1: Point,
       margin: f64,
   ) -> Vec<Vec<Point>> {
       // 单份实现，使用 axis.main_coord / axis.cross_coord / axis.point
       let mut fold_coords: Vec<f64> = Vec::new();
       
       // 收集节点边界
       let mut node_ids: Vec<&String> = ctx.nodes.keys().collect();
       node_ids.sort();
       for nid in &node_ids {
           if nid.as_str() == from_id || nid.as_str() == to_id { continue; }
           let nl = &ctx.nodes[*nid];
           let rect = Rect::from(nl).expanded(NODE_OBSTACLE_PAD);
           let (lo, hi) = rect.range_on_axis(axis);
           fold_coords.push(lo - margin);
           fold_coords.push(hi + margin);
       }
       
       // 收集分组边界
       let mut group_ids: Vec<&String> = ctx.group_ctx.groups.keys().collect();
       group_ids.sort();
       for gid in &group_ids {
           if endpoint_groups.contains(gid.as_str()) { continue; }
           if let Some(gl) = ctx.group_ctx.groups.get(*gid) {
               let rect = Rect::from(gl).expanded(GROUP_OBSTACLE_PAD);
               let (lo, hi) = rect.range_on_axis(axis);
               fold_coords.push(lo - margin);
               fold_coords.push(hi + margin);
           }
       }
       
       // 分组间隙中点（按交叉轴过滤）
       fold_coords.extend(group_gap_midpoints_on_axis(axis, ctx.group_ctx.groups));
       
       // 去重排序
       fold_coords.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
       fold_coords.dedup_by(|a, b| (*a - *b).abs() < 1.0);
       
       // 生成路径：(sx,sy) → s1 → axis.point(s1_main, fold) → axis.point(e1_main, fold) → e1 → (ex,ey)
       let start = Point::new(sx, sy);
       let end = Point::new(ex, ey);
       fold_coords.into_iter().map(|fold| {
           let p2 = axis.point(axis.main_coord(s1), fold);
           let p3 = axis.point(axis.main_coord(e1), fold);
           simplify_path_preserving_stubs_p(vec![start, s1, p2, p3, e1, end])
       }).collect()
   }
   ```

4. **重构 `group_x_gap_midpoints` / `group_y_gap_midpoints`** 为统一的 `group_gap_midpoints_on_axis(axis, groups)` 函数

5. 对 `build_channel_detours`、`build_staircase_candidates`、`build_horizontal_first_staircase` 执行同样的 Axis 抽象，逐步消除镜像代码。**一次只重构一个函数，每重构完一个函数就运行测试**。

6. 验证：
```bash
cargo test -p drawify-core --orthogonal 2>&1 | tail -20
cargo test -p drawify-core 2>&1 | tail -20
```

**注意**：
- `build_horizontal_first_staircase` 和 `build_staircase_candidates` 除了折点顺序不同其余逻辑相同，重构后应合并为一个函数，通过 `fold_order: FoldOrder { VerticalFirst, HorizontalFirst }` 参数区分
- 合并后删除 `build_horizontal_first_staircase`

**完成标准**：
- [ ] `build_obstacle_aware_z_folds` 中无镜像代码
- [ ] `build_channel_detours` 中无镜像代码
- [ ] `build_staircase_candidates` 和 `build_horizontal_first_staircase` 合并为一个函数
- [ ] 路径生成函数使用 Point 类型
- [ ] 全部测试通过
- [ ] path.rs 总行数减少 500+ 行

---

### Task 1.4：用 Rect 统一穿障检测

**文件操作**：
- 修改：`crates/drawify-core/src/layout/edge/edge_routing_orthogonal/scoring.rs`
- 修改：`crates/drawify-core/src/layout/edge/edge_routing_orthogonal/path.rs`

**具体步骤**：

1. 将 `path_is_clean`（检测穿节点）和 `path_avoids_group_interiors`（检测穿组）合并为一个函数：

```rust
use crate::layout::geometry::{Rect, Point};

/// 检查路径是否避开所有障碍物（节点+组，端点自身不算障碍）
pub fn path_is_clean(
    path: &[Point],
    from_id: &str,
    to_id: &str,
    nodes: &HashMap<String, NodeLayout>,
    group_ctx: &GroupRoutingContext,
) -> bool {
    if path.len() < 2 {
        return false;
    }

    // 检测穿节点
    for (nid, nl) in nodes {
        if nid == from_id || nid == to_id { continue; }
        let rect = Rect::from(nl).expanded(NODE_OBSTACLE_PAD);
        if path_intersects_rect(path, &rect) {
            return false;
        }
    }

    // 检测穿组内部（非端点所属组）
    let endpoint_groups: HashSet<&str> = group_ctx.node_to_groups
        .get(from_id).into_iter().flatten()
        .chain(group_ctx.node_to_groups.get(to_id).into_iter().flatten())
        .map(|s| s.as_str())
        .collect();

    for (gid, gl) in &group_ctx.groups {
        if endpoint_groups.contains(gid.as_str()) { continue; }
        let rect = Rect::from(gl).expanded(GROUP_OBSTACLE_PAD);
        if path_intersects_rect(path, &rect) {
            return false;
        }
    }

    true
}

/// 折线是否穿过矩形
fn path_intersects_rect(path: &[Point], rect: &Rect) -> bool {
    for w in path.windows(2) {
        if rect.intersects_segment(w[0], w[1], 0.0) {
            return true;
        }
    }
    false
}
```

2. 更新所有调用点：原来调用 `path_is_clean(...) && path_avoids_group_interiors(...)` 的地方改为只调用 `path_is_clean(...)`；原来分两阶段评分（strict / nodes_only）的逻辑调整为：
   - 原 `path_is_clean` 检测 = 只检测穿节点
   - 新 `path_is_clean` = 同时检测节点+组
   - 需要保留一个 `path_avoids_nodes` 函数用于分阶段评分

3. 重新设计三阶段过滤：
```rust
enum PathCleanLevel {
    Dirty,        // 穿节点
    NodeClean,    // 不穿节点，可能穿组
    StrictClean,  // 不穿节点也不穿组
}

fn path_clean_level(path: &[Point], from_id: &str, to_id: &str, ctx: &RoutingContext) -> PathCleanLevel;
```

4. 验证：
```bash
cargo test -p drawify-core 2>&1 | tail -30
```

**完成标准**：
- [ ] 穿节点和穿组检测使用统一的 Rect 方法
- [ ] 不再有重复的线段-AABB 相交代码
- [ ] 全部测试通过

---

## Phase 2：拆分长函数，统一重复逻辑

**预计执行时间**：2-4 轮 Agent 任务
**前置条件**：Phase 1 全部完成，测试通过

---

### Task 2.1：拆分 Orthogonal Router 为阶段结构体

**文件操作**：
- 修改：`crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs`
- 新建：`crates/drawify-core/src/layout/edge/edge_routing_orthogonal/router.rs`（可选，如果单文件太长）

**具体步骤**：

1. 创建 `OrthogonalRouter` 结构体，作为路由执行的上下文：

```rust
struct OrthogonalRouter<'a> {
    diagram: &'a Diagram,
    result: &'a LayoutResult,
    cfg: OrthoConfig,
    group_ctx: GroupRoutingContext,
    relations: &'a [crate::ast::Relation],
    n: usize,

    // 输出/中间状态
    from_side: Vec<Port>,
    to_side: Vec<Port>,
    lane: Vec<usize>,
    endpoint_map: HashMap<(usize, bool), Endpoint>,
    routed_segments: Vec<RoutedSegment>,
    edges: Vec<EdgeLayout>,
    ortho_stats: OrthoDebugStats,
}
```

2. 将 `route_edges_orthogonal_inner` 中的每一步拆为 `impl OrthogonalRouter` 的方法：

| 方法 | 对应原步骤 | 行数估计 |
|------|-----------|---------|
| `new()` | 初始化 | 30 |
| `init_pair_groups_and_ports()` | 步骤1：pair分组+逐对选端口 | 40 |
| `coordinate_port_sides_wrapper()` | 步骤1b：端口全局协调 | 10（委托现有函数）|
| `assign_slot_anchors()` | 步骤2：端点分组+slot分配 | 80 |
| `determine_edge_order()` | 步骤3：边序排序 | 10 |
| `route_all_edges()` | 步骤4：逐边构建路径 | 60 |
| `fix_slot_inversions()` | 步骤4b：倒挂修复（将现有函数改为方法）| 保持不变，改为&mut self方法 |
| `resolve_label_overlaps_wrapper()` | 步骤5：标签避障 | 10 |
| `finalize()` | 生成最终 LayoutResult | 20 |

3. 主函数简化为：
```rust
fn route_edges_orthogonal_inner(
    diagram: &Diagram,
    mut result: LayoutResult,
    cfg: OrthoConfig,
    preserve_edges: Option<HashSet<usize>>,
) -> LayoutResult {
    let mut router = OrthogonalRouter::new(diagram, &result, cfg, preserve_edges);
    router.init_pair_groups_and_ports();
    router.coordinate_port_sides();
    router.assign_slot_anchors();
    router.determine_edge_order();
    router.route_all_edges();
    router.fix_slot_inversions();
    router.resolve_label_overlaps();
    router.finalize(&mut result);
    result
}
```

4. 将 `coordinate_port_sides`、`fix_slot_inversions`、`swap_endpoint_anchors` 等函数从 mod.rs 的 free functions 改为 `OrthogonalRouter` 的方法（或保留为自由函数但传入所需数据，不要过度引入 &mut self）

5. 验证：
```bash
cargo test -p drawify-core 2>&1 | tail -30
```

**完成标准**：
- [ ] `route_edges_orthogonal_inner` 函数体缩短到 50 行以内（纯调度）
- [ ] 每个阶段方法 ≤ 80 行
- [ ] 所有现有测试通过
- [ ] 外部 API 完全不变（`route_edges_orthogonal` 签名不变）

---

### Task 2.2：统一 MacroBlock 和超级图构建

**文件操作**：
- 修改：`crates/drawify-core/src/layout/node/architecture_v2/two_phase.rs`

**具体步骤**：

1. 提取统一的 `MacroBlock` trait 或泛型结构：

```rust
trait BlockLike {
    fn id(&self) -> &str;
    fn is_group(&self) -> bool;
    fn rect(&self) -> Rect;
    fn set_position(&mut self, x: f64, y: f64);
    fn intra_nodes(&self) -> &HashMap<String, NodeLayout>;
    fn intra_layers(&self) -> &[Vec<String>];
}
```

2. 让 `MacroBlock` 和 `IntraMacroBlock` 都实现 `BlockLike`（或直接合并为一个带 `is_top_level: bool` 标记的结构体）

3. 提取统一的块定位函数：
```rust
fn position_blocks(
    blocks: &mut [impl BlockLike],
    macro_ranks: &HashMap<String, usize>,
    super_edges: &HashSet<(String, String)>,
    pair_edge_counts: &HashMap<(String, String), usize>,
    canvas_padding: f64,
    layer_gap: f64,
    group_gap_x: f64,
);
```

4. 让 `position_macro_blocks` 和 `position_intra_macro_blocks` 都委托给这个统一函数

5. 统一 `build_super_graph` 和 `build_super_graph_for_group`：
   - 差异点：前者的 super_members 来自 `group_map.top_groups` + 无组节点，后者来自子组+直接实体
   - 提取一个核心 `build_super_graph_from_members(...) -> (super_members, super_edges, pair_edge_counts)` 函数
   - 两个 wrapper 负责构造不同的初始 super_members 映射

6. 验证：
```bash
cargo test -p drawify-core architecture 2>&1 | tail -20
cargo test -p drawify-core 2>&1 | tail -20
```

**完成标准**：
- [ ] `MacroBlock` 和 `IntraMacroBlock` 代码重复消除
- [ ] `position_macro_blocks` 和 `position_intra_macro_blocks` 代码重复消除
- [ ] `build_super_graph` 和 `build_super_graph_for_group` 核心逻辑统一
- [ ] two_phase.rs 总行数减少 300+ 行
- [ ] 全部测试通过

---

### Task 2.3：消除 select_best_path 中 Phase1/Phase2 的重复评分逻辑

**文件操作**：
- 修改：`crates/drawify-core/src/layout/edge/edge_routing_orthogonal/path.rs`

**具体步骤**：

1. 提取候选评分收集为一个闭包或局部函数：

```rust
struct ScoredCandidate {
    score: f64,
    path: Vec<Point>,
    level: PathCleanLevel,
}

fn evaluate_candidates(
    candidates: Vec<Vec<Point>>,
    ctx: &RoutingContext,
    pair: &EndpointPair,
    scorer: &dyn CandidateScorer,
) -> (Option<ScoredCandidate>, Option<ScoredCandidate>, Option<ScoredCandidate>, usize) {
    // 返回 (best_strict, best_nodes_only, best_dirty, candidate_count)
    // 统一评分逻辑，消除 Phase1/Phase2 的重复
}
```

2. 简化 `select_best_path_with_scorer_stats`：

```rust
pub fn select_best_path_with_scorer_stats(...) -> Vec<Point> {
    // Phase 1
    let mut phase1 = build_candidate_paths(...);
    phase1.extend(build_channel_detours(...));
    phase1.extend(build_obstacle_aware_z_folds(...));
    let (mut best_strict, mut best_nodes_only, mut best_dirty, mut count) =
        evaluate_candidates(phase1, ctx, pair, scorer);

    // Phase 2（fallback）
    if best_strict.is_none() {
        let mut phase2 = build_staircase_candidates(...);  // 已合并 horizontal-first
        let (s2, n2, d2, c2) = evaluate_candidates(phase2, ctx, pair, scorer);
        best_strict = best_strict.or(s2);
        best_nodes_only = best_nodes_only.or(n2);
        best_dirty = best_dirty.or(d2);
        count += c2;
    }

    // 更新 stats...
    // 返回最优路径
}
```

3. 验证：
```bash
cargo test -p drawify-core 2>&1 | tail -20
```

**完成标准**：
- [ ] Phase1 和 Phase2 的评分循环不重复
- [ ] 全部测试通过

---

## Phase 3：建立可插拔扩展点（可选，按需做）

**前置条件**：Phase 1+2 全部完成
**预计执行时间**：3-5 轮 Agent 任务

---

### Task 3.1：引入 Obstacle 抽象

**文件操作**：
- 新建：`crates/drawify-core/src/layout/edge/obstacle.rs`
- 修改：`crates/drawify-core/src/layout/edge/mod.rs`
- 修改：正交路由、spline 路由相关文件

**具体步骤**：

1. 定义 Obstacle trait 和 ObstacleSet：

```rust
pub trait Obstacle {
    fn bounding_rect(&self) -> Rect;
    fn is_endpoint_related(&self, from_id: &str, to_id: &str) -> bool;
}

pub struct ObstacleSet<'a> {
    obstacles: Vec<(Rect, ObstacleKind)>,
}

enum ObstacleKind {
    Node(String),
    Group(String),
}
```

2. 为 NodeLayout 和 GroupLayout 实现 Obstacle

3. 在 RoutingContext 中构建 ObstacleSet，替代当前分开的 nodes/groups 遍历

4. 重构正交路由的穿障检测和 spline 的 ObstacleIndex 共享障碍表示

**注意**：此任务改动面较大，需谨慎小步进行。建议先做正交路由，再改 spline。

---

### Task 3.2：引入 PathPostProcessor 管线

**文件操作**：
- 新建：`crates/drawify-core/src/layout/edge/postprocess_pipeline.rs`
- 修改：正交路由、spline 路由

（略，详细设计待 Phase 1/2 完成后根据代码实际情况细化）

---

### Task 3.3：引入 CandidateGenerator trait

（略，详细设计待 Phase 1/2 完成后细化）

---

## 每步通用验证命令清单

Agent 在完成每个任务后，**必须**依次运行以下命令并全部通过：

```bash
# 1. 编译检查
cargo check -p drawify-core 2>&1

# 2. 单元测试
cargo test -p drawify-core 2>&1 | tail -50

# 3. Clippy 检查（无新增警告）
cargo clippy -p drawify-core -- -D warnings 2>&1 | tail -30

# 4. 可选：集成测试（如果涉及渲染）
cargo test -p drawify-core showcase_smoke 2>&1 | tail -20
```

如果任何命令失败：
1. 不要继续下一个任务
2. 分析失败原因，修正当前任务的代码
3. 如果 15 分钟内无法修复，执行 `git checkout .` 回退当前任务的改动，重新规划

---

## 重构期间禁止事项

1. ❌ 不要修改任何测试的预期输出（除非测试本身因为类型变化需要调整，如 `(f64,f64)` → `Point`）
2. ❌ 不要调整任何算法常量（惩罚权重、PITCH 值、MARGIN 值等）
3. ❌ 不要新增功能（如新的候选路径模式）
4. ❌ 不要重写算法核心逻辑（如 Brandes-Koepf 坐标分配、网络单纯形分层、Dijkstra 路径搜索）
5. ❌ 不要修改公共 API 的签名（`compute_layout`、`EdgeRoutingStrategy` trait 等对外接口）
6. ❌ 不要改变确定性排序逻辑（所有 sort 必须保持原有 tiebreaker）

---

## 回退策略

每个 Task 开始前：
```bash
git checkout -b refactor/phase-N-task-M
```

如果任务失败：
```bash
git checkout main
git branch -D refactor/phase-N-task-M
```

任务完成并验证通过后：
```bash
git add -A
git commit -m "refactor(layout): <task description>"
```

---

## 预期收益

完成 Phase 1 + Phase 2 后：

| 指标 | 当前 | 重构后 |
|-----|------|-------|
| `path.rs` 行数 | ~1400 | ~600-700 |
| `two_phase.rs` 行数 | ~1500 | ~1100-1200 |
| `mod.rs`（orthogonal）行数 | ~1200 | ~800-900（含拆分后的 router） |
| 镜像重复代码 | ~700-800 行 | < 100 行 |
| 最长函数行数 | 326 行 | < 80 行 |
| 编译时间 | 基线 | 相当或略快 |
| 运行时行为 | - | **完全一致** |
| 测试通过率 | 100% | 100% |
