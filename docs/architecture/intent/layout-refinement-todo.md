# 布局 Refinement 实施备忘（TODO）

> 更新：2026-06-15  
> 用途：跟踪 Grid Snap 与 Layout Intent 两条线的实现进度与依赖关系。

相关设计文档：

- [布局网格吸附（Grid Snap）](./layout-grid-snap-refinement.md)
- [布局意图与增量约束（Layout Intent）](./layout-intent-refinement.md)

代码入口：

- `crates/drawify-core/src/layout/grid_snap.rs`
- `crates/drawify-core/src/layout/mod.rs` → `compute_layout()`

---

## 1. 当前状态总览

| 能力 | 状态 | 说明 |
|------|------|------|
| Grid Snap Phase 1（节点） | ✅ 已实现 | rank 轴对齐 + 层内槽位；ER 仅 rank 轴 |
| Grid Snap Phase 2（组框 + 边） | ✅ 已实现 | `snap_group_bounds` + `snap_edge_waypoints` |
| Grid Snap Phase 3（Intent 集成） | ⏸ 阻塞 | **依赖 Layout Intent 未实现** |
| Layout Intent P1a（幽灵图） | ❌ 未开始 | phantom 注入、拓扑意图 |
| Layout Intent P1b（几何微调） | ❌ 未开始 | align / pin |

**结论：** Grid Snap Phase 3 **现在做不了**，必须先落地 Layout Intent 至少到 P1b（`Pin` + `Align`），才有东西可集成。

---

## 2. 已完成：Grid Snap Phase 1 & 2

### Phase 1 — 节点 snap

- [x] `GridSnapConfig` + 常量（`layout/constants.rs`）
- [x] rank 轴同层对齐（TB/LR，y 聚类推断层）
- [x] layer 轴槽位吸附（ER 跳过 x 槽位）
- [x] snap 后层内重叠消除
- [x] 接入 `compute_layout`（边路由前）
- [x] 单元测试 + `compute_layout` 集成测试

**启用范围：** 仅 `sugiyama-v2`、`architecture-v2`；无 DSL 开关。

### Phase 2 — 组框 + 边拐点

- [x] `snap_group_bounds`（8px 网格，floor 原点 / ceil 远端）
- [x] `refresh_layout_bounds` + `update_canvas_bounds`
- [x] `snap_edge_waypoints`（中间拐点 snap，端点不动，共线简化）
- [x] 边路由后调用 snap
- [x] 单元测试 + 集成测试

### 有意未做（可后续独立推进）

- [ ] `drawify-eval` 指标：`layer_alignment_score`、`snap_displacement_avg`
- [ ] showcase 前后视觉对比 + 交叉数回归报告
- [x] diagram 属性 `snap: false` 关闭开关（Playground 面板同步）
- [ ] 从布局算法导出 layer 元数据（替代 y 聚类，复杂 arch 图更准确）
- [ ] 更新 [layout-grid-snap-refinement.md](./layout-grid-snap-refinement.md) 实现状态章节

---

## 3. 阻塞项：Grid Snap Phase 3

Phase 3 任务（设计文档 §8.3）全部依赖 Layout Intent：

| 任务 | 依赖 |
|------|------|
| `Pin` 节点跳过 snap | `LayoutIntentOverlay` + `LayoutIntent::Pin` |
| `AlignHorizontal/Vertical` 优先于 snap | P1b `apply_geometric_refinement` |

**解锁条件：** Layout Intent **P1b 完成** 后，再改 `compute_layout` 管线：

```text
translate_intents → base layout
  → apply_geometric_refinement(overlay)   // Intent P1b
  → snap_layout_to_grid(pinned_nodes)     // Phase 3：传入 Pin 集合
  → snap_group_bounds
  → reroute_edges
  → snap_edge_waypoints
```

---

## 4. 推荐实施顺序：Layout Intent 主线

按 [layout-intent-refinement.md §15](./layout-intent-refinement.md) 与 Grid Snap 依赖，建议顺序如下。

### Step A — 可选前置（可与其它工作并行）

- [ ] **layout seed** — 同一 AST + overlay 布局可复现（Grid Snap 已部分改善确定性，seed 仍对 phantom/随机启发式有意义）

### Step B — P1a 幽灵图 MVP（产品壁垒，优先）

- [ ] `layout/intent.rs` — `LayoutIntentOverlay`、`LayoutIntent` 枚举
- [ ] `layout/phantom.rs` — `PhantomEdge`、意图 → 幽灵边翻译
- [ ] 拓扑意图 MVP：`below`、`near`、`right_of`（flowchart + architecture）
- [ ] 扩展 `compute_layout` / `compute_layout_with_options` — 布局前注入 phantom
- [ ] `RefinementReport` 基础结构（满足/部分满足/冲突说明）
- [ ] （可选）Server / WASM API：`refine-layout` 或 overlay 参数

**验收：** Agent 说「把 auth 放到 gateway 下方」→ overlay → 重布局 → 拓扑关系正确。

### Step C — P1b 几何微调 MVP

- [ ] `layout/refinement.rs` — `apply_geometric_refinement`
- [ ] 几何意图：`align_vertical`、`align_horizontal`、`pin`
- [ ] 在 `compute_layout` 中：base layout **之后**、grid snap **之前** 调用

**验收：** 「这三个节点左对齐」→ 满足对齐且不重叠。

### Step D — 汇合 Grid Snap Phase 3

- [ ] `snap_layout_to_grid` 增加 `pinned_nodes: &HashSet<String>` 参数
- [ ] Pin 节点跳过 rank/layer snap
- [ ] geometric align 结果优先于默认格点（意图 > 默认 snap）
- [ ] Phase 3 集成测试

### Step E — P2 扩展（靠后）

- [ ] DSL `arrange` 语法（高频意图进 AST）
- [ ] 更多意图：`left_of`、`same_rank`、`straighten_edge` 等

---

## 5. 管线目标态（全部完成后）

```text
Diagram (+ optional LayoutIntentOverlay)
  ↓
prepare
  ↓
inject_phantom_elements()              // Intent P1a
  ↓
strategy.compute()                     // base layout
  ↓
apply_geometric_refinement(overlay)    // Intent P1b
  ↓
snap_layout_to_grid(pinned)            // Grid Snap（Phase 1–3）
snap_group_bounds()
update_canvas_bounds()
  ↓
router.route()
  ↓
snap_edge_waypoints()
  ↓
render::scene::export_scene → render::encode
```

---

## 6. 决策备忘（已确认，实施时勿改除非重评）

| 决策 | 结论 |
|------|------|
| Grid Snap 先做还是 Intent 先做 | **Grid Snap Phase 1–2 已完成**；Intent 为下一主线 |
| Grid Snap 算法白名单 | `sugiyama-v2`、`architecture-v2` |
| ER 图 snap 范围 | 仅 rank 轴对齐，不做 x 槽位 |
| TB + LR | 均支持 |
| DSL 关闭 snap | ✅ `snap: true \| false`（默认 true） |
| eval 指标 | Phase 1–2 **未做**；Layout Intent 稳定后一并补 |

---

## 7. 下一步行动（给接手的人）

1. **不要开始 Grid Snap Phase 3** — 没有 Layout Intent 接口，做了也只能留空 `pinned_nodes`。
2. **从 Layout Intent P1a 开工** — Agent 微调是近期目标时直接 P1a；layout seed 可与 P1a 并行。
3. **P1b 完成后** — 回到本文 §3，做 Grid Snap Phase 3 汇合（预计 1–2 天）。
4. **可选 housekeeping** — eval 指标、showcase 对比、设计文档状态更新（不阻塞 Intent）。

---

## 修订记录

| 日期 | 说明 |
|------|------|
| 2026-06-15 | 初稿：Grid Snap P1–P2 完成，Phase 3 阻塞于 Layout Intent |
| 2026-06-15 | 移除 Intent 路线中的动画 / Diff 过渡项；MVP 聚焦 P1a/P1b |
