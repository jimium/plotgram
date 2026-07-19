# P0 缺陷根因与修复方案（2026-07）

> 面向对象：布局/路由维护者与 AI agent。  
> 前置阅读（硬红线）：[`docs/总结经验/布局与路由核心手册-2026-07.md`](../总结经验/布局与路由核心手册-2026-07.md) §1 法则、§3 正交管线。  
> 验证入口：一律用 `cargo run -p plotgram-cli`，禁信 `./target/release/plotgram` 陈旧 binary。  
> 约束：禁止图名特判；退化用「节点坐标 diff + 全量 showcase 重叠严重度」量化；不为消 lint 引入穿模。

---

## 0. 三个 P0 一览

| 编号 | 缺陷（lint 指标） | 症状 | 根因一句话 | 文档 |
|------|-------------------|------|-----------|------|
| P0-1 | `edge_node_crossings` / `edge_through_groups` | 边穿过无关节点、边穿过分组内部 | 候选生成降级输出脏路径后，**事后阶段（conflict_reroute/sanitize/lane）没有任何一个以"消穿越"为目标**；缺统一障碍集合 + 统一穿障查询接口 | [P0-1-统一障碍模型.md](./P0-1-统一障碍模型.md) |
| P0-2 | `edge_parallel_overlap` | 大量无关平行长边完全贴合走线（如 `c.plotgram-core-mod-deps` 33 处） | `edge_bundling/` 目录空未实现；`semantic_trunk_merge` 只处理 architecture FanIn；`lane_assignment` 事后逐段偏移在密集区常被 `validate_shift` 拒绝 | [P0-2-平行边分槽bundling.md](./P0-2-平行边分槽bundling.md) |
| P0-3 | state 秩空洞 / 面积利用率 | `c.layout-stress-transitions` 面积利用率 1.5%、宽高比 4.49、initial/final 被孤立 | `repair_rank_monotonicity` 对自环边 `(u,u)` 不收敛（`rank(u) >= rank(u)` 恒真），每轮 +1 并级联膨胀下游 → 巨大空 rank | [P0-3-state秩压缩.md](./P0-3-state秩压缩.md) |

---

## 1. 修复顺序建议

1. **P0-3 优先**：改动最小、根因最确定、风险最低（自环边过滤 + 空 rank 压缩），可先落地建立信心。
2. **P0-1 次之**：统一障碍模型是 P0-1 与 P0-2 的共同基础设施，先把 `PreparedObstacles` 升级为可查询的障碍集合。
3. **P0-2 最后**：依赖 P0-1 的障碍查询接口，实现全局平行长边归组 + 等距分槽预规划。

---

## 2. 公共基础设施：统一障碍查询

P0-1 与 P0-2 都需要"给定一条 polyline，判断它穿了哪些障碍 / 与哪些边平行贴合"。当前 [`PreparedObstacles`](../../crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/context.rs) 仅是两个排序 ID 列表，没有查询能力。

建议在 `context.rs` 为 `PreparedObstacles` 增加统一查询方法（复用 [`common/geom_obstacle.rs`](../../crates/plotgram-core/src/layout/edge/common/geom_obstacle.rs) 的 `segment_pierces_node` / `segment_pierces_group_interior` primitive，保证 router 与 lint 判定一致）：

```text
PreparedObstacles::path_violations(
    path, exempt_endpoints, nodes, group_ctx
) -> ObstacleViolations { node_hits: Vec<String>, group_hits: Vec<String> }
```

- 豁免规则与 [`scoring.rs`](../../crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/scoring.rs) 的 `path_is_clean` / `path_avoids_group_interiors` 保持一致（源首段 stub、宿末段 stub 豁免）。
- **确定性**：查询内部遍历用已排序的 `sorted_node_ids` / `sorted_group_ids`，不依赖 HashMap key 序（AGENTS.md §2）。

---

## 3. 通用回归门禁

每个 P0 修复完成后按此清单验证（手册 §1.3 DoD）：

1. `cargo run -p plotgram-cli` 渲染目标图，肉眼确认症状消除。
2. 全量 showcase 重跑，比对 lint 指标（`edge_node_crossings` / `edge_through_groups` / `edge_parallel_overlap` / state 面积利用率）**总量下降或持平**，无新增穿模。
3. 节点坐标 diff：仅预期节点移动；若整体漂移须给出解释。
4. 既有单测通过；仓库既有失败先钉死基线，不与本次改动混谈。
5. 无图名分支。

各 P0 文档内附带该问题专属的验证图与量化指标。
