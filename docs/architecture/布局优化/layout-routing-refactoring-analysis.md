# 布局与边路由实现复盘分析报告

> 日期：2026-06-25  
> 范围：`crates/drawify-core/src/layout/`（约 98 个 Rust 文件）  
> 状态：P0 + P1 已落地（见 §八、§九）

---

## 一、总体结论

当前实现已经具备较好的架构骨架，**不建议做「推倒重来」式重构**。近期重构的方向（引擎/门面分离、三层 Group Frame、正交路由子模块化、LayoutPlan 前置解析）是正确且有效的。

**建议采取「外科手术式」优化**：优先解决管线编排膨胀、超大单文件、重复路由/快照逻辑三类问题；算法层面以 P2-1 空间索引、管线阶段对象化为主；扩展能力通过 Pipeline Stage 渐进引入，而非一次性抽象框架。

| 维度 | 评分 | 说明 |
|------|------|------|
| 架构清晰度 | ★★★★☆ | 双层 trait + 注册表清晰，但主编排函数过长 |
| 可读性 | ★★★☆☆ | 子模块文档好，几个 God file 拖累整体 |
| 算法效率 | ★★★☆☆ | 中小图足够，大图 refine/spline 有瓶颈 |
| 可维护性 | ★★★☆☆ | 测试内嵌充分，但管线分支多、重复逻辑多 |
| 扩展能力 | ★★★★☆ | trait + plan + catalog 已支持插件化 |

---

## 二、当前架构亮点（应保留）

### 2.1 清晰的双层可插拔框架

```text
LayoutStrategy（节点布局）  →  EdgeRoutingStrategy（边路由）
         ↑                              ↑
    registry::build_layout_strategy   registry::build_edge_routing_strategy
         ↑                              ↑
              LayoutPlan（prepare 阶段一次性解析）
```

- `LayoutPlan` 在 prepare 阶段解析完毕，运行时只读 `self.config`，不再碰 AST。
- `produces_edge_geometry()`、`supports_refine()` 等契约让 sequence 等特殊图类型能绕过通用路由。

### 2.2 Sugiyama v2 共享引擎 + 门面模式

```text
sugiyama_v2/engine  ←── flowchart / er（preset 差异）
                   ←── architecture_v2（two_phase / group_layout_hint）
```

`engine.rs` 四阶段流水线（去环 → rank → order → coordinate）职责单一，是本次重构最成功的部分之一。

### 2.3 三层 Group Frame 模型

```text
L2 Intra Frame  — group_layout_hint（组内排列）
L3 Node Frame   — grid_snap（rank/layer 对齐 + 8px 量化）
L1 Group Frame  — group_frame（组间 stack/Equal/对齐/量化）
```

概念与 [group-frame-spec](../../../已经实现的方案/group-frame-spec.md) 对齐，L1/L2/L3 分工明确。

### 2.4 正交路由子模块化

`edge_routing_orthogonal/` 已拆为 `slot / path / scoring / context / simplify`，设计文档（磁吸点 + 确定性端口 + 候选路径打分）与实现一致。

### 2.5 工程纪律

- **确定性迭代**：关键路径有显式排序，符合 AGENTS.md §2。
- **A/B 基础设施**：`backup/sugiyama` + `drawify-eval` + readme 中「改动 >30% 新建文件」原则。
- **内嵌测试丰富**：约 60+ 个模块含 `#[cfg(test)]`。

---

## 三、主要问题诊断

### 3.1 【高优先级】主编排函数是「上帝函数」

`compute_layout_with_plan_and_overlay`（`mod.rs`）承担了过多职责：

```text
拓扑意图校验 → 节点布局 → 几何意图 Pin/Align
  → L3 grid_snap + L1 group_frame
  → Friendliness V1/V2
  → 边路由 + refine
  → snap + repulse
  → group_frame 恢复 + route2 + snap2 + lint 回滚
  → edge bundling
  → finalize_canvas_bounds
```

**问题表现：**

1. ~450 行单函数，含 6+ 个条件分支、3 次 group_frame 调用、2 次路由路径。
2. 架构图特例散落主编排层（`center_single_group_rows`、`realign_group_rows`）。
3. V2 friendliness 路径可能双次路由。
4. `snap_edge_waypoints` + `repulse_edges_from_group_borders` 在三处几乎相同。

### 3.2 【高优先级】超大单文件

| 文件 | 行数 | 问题 |
|------|------|------|
| `layout/mod.rs` | ~2861 | 类型定义 + trait + 编排 + 辅助函数 |
| `group_frame/mod.rs` | ~2767 | Spec 解析 + apply 逻辑 + 大量测试 |
| `architecture_v2/layout.rs` | ~2789 | 单阶段/两阶段/后处理混在一起 |
| `edge_routing_orthogonal/mod.rs` | ~2397 | 路由编排 + 大量内嵌测试 |
| `architecture_v2/two_phase.rs` | ~1722 | 两阶段布局完整实现 |
| `refine.rs` | ~1483 | 穿障检测 + 推节点 + 重叠分析 + 测试 |
| `orthogonal/path.rs` | ~1371 | 候选路径生成 |

### 3.3 【中优先级】性能热点（已有注释但未落地）

| 热点 | 复杂度 | 现状 |
|------|--------|------|
| `refine::analyze_edge_node_crossings` | O(E·S·V) | 每轮 refine 全量扫描 |
| `refine::analyze_edge_overlaps` | O(E²·S²) | 注释写明「P2-1 空间索引可加速」，未实现 |
| `ObstacleIndex` 构建 | O(V²·N) | spline/circular 每条边 Dijkstra |
| 正交路由贪心逐边 | O(E × P) | 先路由边占通道，后路由边易退化 |
| V2 路径 | 2× clone + 可能 3× route | 内存与 CPU 双开销 |

### 3.4 【中优先级】概念边界模糊

1. Group Frame ↔ Border Shell ↔ 路由 的 handoff 在 `mod.rs`、`group_frame`、`group/` 三处交叉。
2. Friendliness V1/V2 与 refine 职责重叠。
3. Bundling 与 refine 契约冲突：bundling 故意制造 trunk 重叠，放在 refine 之后。

### 3.5 【低优先级】可清理项

- `backup/sugiyama` 仍注册在 `registry.rs`；eval 基线迁移后可删除。
- Phase 标记（P1-1、P2-1）散落代码中，适合收敛到 issue/CHANGELOG 后移除。

---

## 四、重构建议（按优先级）

### P0 — 立即可做，低风险高收益

1. **提取管线编排器 `LayoutPipeline`**：将 `compute_layout_with_plan_and_overlay` 拆为显式阶段方法。
2. **抽取 `EdgePostProcess` 去重**：合并三处 `snap_edge_waypoints` + `repulse_edges_from_group_borders`。
3. **迁移架构图特例出 `mod.rs`**：`center_single_group_rows` → `architecture_v2/post_layout.rs`；`realign_group_rows` → `group_frame/realign.rs`。

### P1 — 短期（1–2 周）

4. 拆分 God files（`group_frame`、`orthogonal/mod`、`architecture_v2/layout`、`refine`）。
5. 实现 P2-1 空间索引（refine 边重叠检测）。
6. 统一 Group Frame handoff（`GroupFramePass`）。

### P2 — 中期

7. 正交路由全局协调（分层批量路由 / constrained A*）。
8. Friendliness 与 Refine 合并为 `LayoutRouteFeedback`。
9. Sugiyama v2 算法演进（完整 NS / Brandes-Köpf）。

### P3 — 长期 / 可选

- 删除 `backup/sugiyama`
- `force_directed` Barnes-Hut 默认开启
- spline `ObstacleIndex` 懒构建
- Pipeline Stage trait（仅当阶段数 >10 且需插件化时）

---

## 五、不建议做的重构

1. **不要合并 LayoutStrategy 与 EdgeRoutingStrategy** — 两阶段分离是正确的。
2. **不要把正交路由改回单体 A\*** — 当前 slot + 打分方案确定性好、可调试。
3. **不要过早引入 `geo` / `nalgebra` 等重依赖** — WASM 包体积敏感。
4. **不要一次性重写 group_frame** — 应拆分而非重写。
5. **不要删除 refine** — 应优化其检测效率，而非移除。

---

## 六、建议执行路线图

```text
Week 1–2   P0: LayoutPipeline + EdgePostProcess 去重 + 架构特例迁移
Week 3–4   P1: God file 拆分 + GroupFramePass 统一
Week 5–6   P1: refine 空间索引（P2-1 落地）
Week 7+    P2: Friendliness/Refine 合并、正交分层路由（按需）
```

每步配合 `drawify-eval` 跑 showcase 回归（`showcase/architecture/*.dfy`）。

---

## 七、架构现状

```text
                    ┌─────────────────────────────────┐
                    │     compute_layout_with_plan     │  ← 拆为 LayoutPipeline
                    └───────────────┬─────────────────┘
                                    │
         ┌──────────────────────────┼──────────────────────────┐
         ▼                          ▼                          ▼
  ┌─────────────┐           ┌──────────────┐           ┌─────────────┐
  │ Node Layout │           │ Group Frame  │           │ Edge Route  │
  │ Strategies  │           │ L1/L2/L3     │           │ Strategies  │
  └──────┬──────┘           └──────┬───────┘           └──────┬──────┘
         │                         │                          │
  sugiyama_v2 ◄── flowchart/er     group_frame            orthogonal/*
  architecture_v2                  grid_snap                refine
  force_directed/mindmap...        group/ (Border Shell)    bundling
         │                         │                          │
         └─────────────────────────┴──────────────────────────┘
                                   │
                          LayoutResult + Hints
```

---

## 八、P0 落地记录

| 变更 | 文件 | 说明 |
|------|------|------|
| 分析文档 | 本文件 | 复盘结论与路线图 |
| 管线编排器 | `layout/pipeline.rs` | `LayoutPipeline::run` 承接主编排逻辑 |
| 边后处理去重 | `layout/edge_postprocess.rs` | `snap_and_repulse_edges` |
| 架构后处理 | `layout/node/architecture_v2/post_layout.rs` | `center_single_group_rows` |
| 行对齐恢复 | `layout/group_frame/realign.rs` | `realign_group_rows` |

## 九、P1 落地记录

| 变更 | 文件 | 说明 |
|------|------|------|
| Group Frame handoff | `layout/group_frame/pass.rs` | `GroupFramePass` 统一路由前/后 group frame 调用 |
| refine 子模块化 | `layout/refine/{crossing,overlap,push,reroute,geometry}.rs` | 拆分穿障/重叠/推节点/重路由 |
| 边重叠空间索引 | `layout/refine/overlap.rs` | P2-1 均匀网格索引，替代 O(S²) 全量比较 |
| 测试外置 | `orthogonal_tests.rs` / `group_frame_tests.rs` / `refine_tests.rs` | God file 内嵌测试迁出 |
| pipeline 简化 | `layout/pipeline.rs` | 使用 `GroupFramePass`，移除重复 handoff 逻辑 |

## 十、architecture_v2/layout 拆分记录

原 `layout.rs`（~2789 行）拆为目录模块：

| 文件 | 行数 | 职责 |
|------|------|------|
| `layout/mod.rs` | ~165 | `ArchitectureV2Layout` + `LayoutStrategy` 编排 |
| `layout/constants.rs` | ~21 | 布局常量 |
| `layout/types.rs` | ~47 | `GraphIndex` / `GroupMap` |
| `layout/acyclic.rs` | ~48 | 去环 + 意图边注入 |
| `layout/rank.rs` | ~430 | Phase 2 分组感知层分配 |
| `layout/order.rs` | ~447 | Phase 3 层内排序 |
| `layout/coordinate.rs` | ~735 | Phase 4 坐标分配 + 邻接对齐 |
| `layout/postprocess.rs` | ~165 | 重叠消除、钳制、画布尺寸 |
| `layout/layout_tests.rs` | ~728 | 单元测试 |

`two_phase` / `pipeline` / `group_layout_hint` 改为从子模块路径导入（如 `layout::coordinate::`）。

## 十一、P2 落地记录

| 变更 | 文件 | 说明 |
|------|------|------|
| 布局↔路由反馈 | `layout/route_feedback.rs` | `LayoutRouteFeedback` 统一 V1 诊断、V2 调整、route+refine、基线择优 |
| pipeline 简化 | `layout/pipeline.rs` | `run_routing_pipeline` 委托 `LayoutRouteFeedback` |
| 正交分层边序 | `edge/edge_routing_orthogonal/layer_order.rs` | 有 `sugiyama_ranks` 时按 min(rank) 分层批量路由 |

---

## 相关文档

- [布局算法 readme](../../../crates/drawify-core/src/layout/readme.md)
- [Group Frame 规范](../../../已经实现的方案/group-frame-spec.md)
- [正交路由投资计划](../../../已经实现的方案/orthogonal-routing-investment-plan.md)
- [布局算法优化计划](../layout-algorithm-optimization-plan.md)
