# 布局/路由算法简化重构（第二轮）Spec

> 日期：2026-07-20
> 范围：`crates/plotgram-core/src/layout/` 全模块（架构图 + 流程图布局 / 正交边路由）
> 前置工作：第一轮已落地 Phase 1 死代码清理（~1,144 行）+ Phase 2 §3.2.1 C/D 去重（~15 行），S4 删除实验失败回滚
> 约束：遵守 AGENTS.md §1–§7；允许性能基线略有下降（用户明确许可），但 product-gate 正确性不退化

---

## Why

第一轮简化已把"易摘的果子"摘完。`docs/总结经验/简化重构经验-哪些不可精简-2026-07.md` 沉淀了 4 个被门禁否决/回滚的尝试，证实剩余复杂度多为**本质异构**而非可合并冗余；但原方案 §3.2.2 / §3.2.4 / §3.2.5 / §3.3.2 / §3.3.3 / §3.3.4 六大块简化**均未实施**，且其中部分项的"随 S4 移除"前提已随 S4 回滚而失效，需要基于当前真实代码状态重新规划。

本轮目标：
1. 基于 2026-07-20 审计后的真实代码状态（非原方案过时描述）重写简化方案；
2. 按"低风险 → 中风险 → 高风险"分阶段，每阶段独立可回滚、可量化；
3. 在用户许可的"性能基线略有下降"窗口内，把 §3.2.2/§3.2.4/§3.2.5/§3.3.2/§3.3.3/§3.3.4 中**真冗余**部分消化掉；
4. 建立**临时备份 + 执行笔记**机制，门禁失败时按文件级精确回滚（吸取第一轮 `git restore` 误伤 Phase1/2 成果的教训）。

---

## What Changes

- **新增**：本轮简化方案的现状分析、简化策略、预期结果三章节（本 spec §1–§3）
- **新增**：临时备份机制（`/tmp/plotgram-simplify-r2-backup/` 按阶段分子目录，存原始文件副本 + `git diff` patch）
- **新增**：执行笔记文件（`/tmp/plotgram-simplify-r2-backup/NOTES.md`，每步追加：改了什么、门禁结果、是否回滚）
- **修改**：`crates/plotgram-core/src/layout/` 下若干文件（具体清单见 §2 简化策略矩阵）
- **修改**：`docs/布局路由算法简化重构方案-2026-07.md` 末尾追加"第二轮执行结论"
- **修改**：`docs/总结经验/简化重构经验-哪些不可精简-2026-07.md` 追加新一轮 keep-list 收敛
- **不修改**：keep-list 13 项（详见 §1.4）—— 经第一轮门禁实测全部承重，本轮不动

### BREAKING

无对外 API breaking。AGENTS.md §1 明确本项目无向后兼容约束，但本轮简化不重命名公共类型/trait，仅删/合并内部 pass 与重复代码。

---

## Impact

- **Affected code**:
  - `crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/scoring.rs`（§3.2.2，1240 行）
  - `crates/plotgram-core/src/layout/pipeline.rs`（§3.2.4，467 行）
  - `crates/plotgram-core/src/layout/node/architecture_v2/pipeline.rs`（§3.2.5，228 行）
  - `crates/plotgram-core/src/layout/demand/{grid.rs,pierce.rs,features.rs}`（§3.3.2）
  - `crates/plotgram-core/src/layout/edge/route_annotation.rs`（§3.3.3，765 行）
  - `crates/plotgram-core/src/layout/refine/spline_fallback.rs`（§3.3.4，1002 行）
  - 顶层 env var 清理涉及：`space_budget.rs` / `group_frame/spec.rs` / `phases/port_slot.rs` / `phases/refine.rs`
- **Affected specs**: 无对外 spec（核心算法非 spec 化）
- **Affected docs**: `docs/布局路由算法简化重构方案-2026-07.md`、`docs/总结经验/简化重构经验-哪些不可精简-2026-07.md`
- **Benchmarks**: 每阶段产出新 baseline（`benchmarks/baselines/YYYY-MM-DD-HHMMSS-<phase>.{json,md}`）+ `compare.sh` 对比上一阶段
- **WASM**: `plotgram-core` 编译到 WASM，全程 `cargo check -p plotgram-wasm --target wasm32-unknown-unknown`；禁 `std::time::{Instant,SystemTime}`

---

## 1. 现状分析（基于 2026-07-20 审计）

### 1.1 第一轮执行结论回顾

| 阶段 | 内容 | 状态 | 净减行数 |
|------|------|------|----------|
| Phase 1（死代码清理） | 6 项安全清理：`corridor_stick.rs` / `channel_occupancy.rs` / `demand/dump.rs` 精简 / `PLOTGRAM_PORT_PRESSURE_PREFER` / `PLOTGRAM_CHECK_D_IDEMPOTENT` / `edge_stages.rs` 写权审计表 | ✅ 已完成，门禁 PASS | ~1,144 |
| Phase 2 §3.2.1（C/D 去重） | run.rs C 末 `dock_sep` 删除（实验 A PASS）+ phase_lane C 期 `min_gap` 删除（实验 C PASS）+ `gap_fixed` 死代码清除 | ✅ 已完成，门禁 PASS | ~15 |
| Phase 3 §3.2.3（S4 删除） | 删 S4/S4.x/B.2 监控走廊特化 + 失活 trunk/feedback helper/`force_outer_escape`/`protected_trunk`/`outer_ring` | ⛔ FAIL 回滚，`typical-microservice-architecture` tight_sev 1048→3044（~3×） | 0 |
| Phase 2 §3.2.2 / §3.2.4 / §3.2.5 | Scoring / D-pipeline / Architecture 后处理精简 | ⏭️ 未实施 | 0 |
| Phase 3 §3.3.1 / §3.3.2 / §3.3.3 / §3.3.4 | path/demand/annotation/refine 精简 | ⏭️ 未实施（§3.3.2 dump.rs 已顺带精简） | 0 |

### 1.2 当前代码规模（实测，2026-07-20）

| 模块 | 当前行数 | 第一轮前 | 差值 | 备注 |
|------|---------|----------|------|------|
| `edge/edge_routing_orthogonal/`（含 phases/） | 16,803 | 17,225 | -422 | corridor_stick 321 + PREFER 91 + 杂项 |
| `node/architecture_v2/`（含 two_phase/ + layout/） | 7,685 | 7,685 | 0 | **未动** |
| `refine/` | 2,842 | 2,852 | -10 | 注释微调 |
| `demand/` | 1,164 | 1,263 | -99 | dump.rs 精简 ~96 |
| `edge/common/` | 5,477 | 5,477 | 0 | 未动 |
| `group_frame/` | 3,518 | 3,518 | 0 | 未动 |
| `lint/` | 2,462 | 2,462 | 0 | 未动 |
| 顶层（pipeline/grid_snap/space_budget/...） | ~3,500 | ~3,600 | -100 | edge_stages 审计表 -259 + pipeline CHECK_D -30 + 杂项 +space_budget_guard +110 |
| **合计** | **~63,451** | **~69,700** | **~-1,249** | 第一轮净减约 1.8% |

### 1.3 残留环境变量（13 处，原方案 23 处）

`crates/plotgram-core/src/layout/` 内 `std::env::var` 命中 13 处：

| # | 文件:行 | 环境变量 | 默认 | 用途 | 处置 |
|---|--------|---------|------|------|------|
| 1 | `edge/edge_routing_orthogonal/run.rs:15` | `PLOTGRAM_EDGE_ORDER_SCORE` | 开启 | 边难度排序 | 保留 |
| 2 | `edge/edge_routing_orthogonal/channel_load.rs:31` | `PLOTGRAM_CORRIDOR_SOFT` | 开启 | 走廊软惩罚 | 保留 |
| 3 | `space_budget.rs:170` | `PLOTGRAM_PRESSURE_BUDGET` | 开启 | 压力预算 | 保留 |
| 4 | `space_budget.rs:354` | `PLOTGRAM_DEBUG_EDGE_PRESSURE` | 关闭 | 调试日志 | **Tier A 统一为 `perf_log!`** |
| 5 | `space_budget.rs:415` | `PLOTGRAM_EDGE_PRESSURE_BUDGET` | 开启 | 边级压力预算 | 保留 |
| 6 | `refine/mod.rs:77` | `PLOTGRAM_SKIP_REFINE` | 不跳过 | 调试：跳过 refine | 保留 |
| 7 | `phases/refine.rs:181` | `PLOTGRAM_DEBUG_EDGE_ORDER` | 关闭 | 调试日志 | **Tier A 统一为 `perf_log!`** |
| 8 | `phases/port_slot.rs:664` | `PLOTGRAM_PORT_PRESSURE_SIDE` | 开启 | 端口压力侧选 | 保留 |
| 9 | `phases/port_slot.rs:671` | `PLOTGRAM_PORT_PRESSURE_RELIEVE` | 开启 | 端口压力释放 | 保留 |
| 10 | `phases/port_slot.rs:679` | `PLOTGRAM_PORT_PRESSURE_SLOT` | 开启 | 端口压力 slot | 保留 |
| 11 | `phases/port_slot.rs:806` | `PLOTGRAM_DEBUG_PORT_PRESSURE` | 关闭 | 调试日志 | **Tier A 统一为 `perf_log!`** |
| 12 | `group_frame/spec.rs:427` | `PLOTGRAM_ARCH_PACK` | 未设 | 实验性排列 | **Tier A 删除（确认无生产 setter）** |
| 13 | `group_frame/spec.rs:444` | `PLOTGRAM_FLOW_ASPECT` | 未设 | 实验性宽高比 | **Tier A 删除（确认无生产 setter）** |

### 1.4 keep-list（不可触碰，13 项，全部活跃在码）

依据 `docs/总结经验/简化重构经验-哪些不可精简-2026-07.md` §3，本轮**不得**提议删除/合并下列任一项：

| keep-list 项 | 当前活跃位置 |
|--------------|-------------|
| S4 监控走廊（S4/S4.x/B.2 reroute） | `run.rs:272-473` |
| `feedback_side` 监控 helper（`monitor_hub_edge_indices`） | `run.rs:279/351/433` 调用 |
| `phases/trunk.rs` | 167 行 |
| `force_outer_escape_path` | `path.rs:876` + `run.rs:383` |
| `protected_trunk_crossing_penalty` + `outer_ring_path_bonus` | `scoring.rs:347/382`（S4 附着代码，随 S4 保留） |
| `ProtectedRun`（route_annotation） | `route_annotation.rs:29` |
| `resolve_exact_stub_occupancy_post_route` | `pipeline.rs:290`（arch D 末守 `cloud-native` node_fp） |
| `phase_lane` C 期 `enforce_reverse_pair_dock_separation` | `phases/refine.rs:410`（C 期承重） |
| `resolve_stub_occupancy_conflicts`（flowchart C 期真修） | `phases/refine.rs:361` |
| `separate_unrelated_trunk_overlaps[_post_route]` | `phases/refine.rs:333` + `pipeline.rs:346` |
| `sanitize.rs` / `snap_and_repulse` / `label_resolve` | 806 行 / `pipeline.rs:234` / `pipeline.rs:369` |
| `slot.rs` / `slot_replan.rs` | 381 / 330 行 |

> **关键约束**：原方案 §3.3.1 / §3.3.3 多处标注"随 S4 移除"——S4 已回滚保留，故 `force_outer_escape_path` / `protected_trunk_crossing_penalty` / `outer_ring_path_bonus` / `ProtectedRun` **必须保留**。本轮简化方案须绕开这四项。

### 1.5 复杂度热点（未简化部分）

| 热点 | 文件 | 当前行数 | 简化空间 |
|------|------|---------|----------|
| Scoring 10+ 惩罚因子 | `scoring.rs` | 1,240 | 合并 near_miss / channel_load（保留 protected_trunk/outer_ring）→ ~200-300 行 |
| D 阶段 11 步 | `pipeline.rs` | 467 | 合并相邻 pass 的 bookkeeping（不删 pass）→ ~50 行 |
| Architecture 后处理 7 Phase | `architecture_v2/pipeline.rs` | 228 | 合并为 4 Phase（保留 rebalance/gaps 调用次数）→ ~50 行 |
| `demand/grid.rs` + `pierce.rs` | `demand/` | 186 + 176 | 需先验证无下游消费者，可删 → ~360 行 |
| `route_annotation.rs` | `route_annotation.rs` | 765 | 精简 `validate_route_edit`（保留 ProtectedRun）→ ~150 行 |
| `refine/spline_fallback.rs` | `refine/spline_fallback.rs` | 1,002 | 降级到 dogleg + 显式 degraded（高风险，帕累托评判）→ ~600 行 |

---

## 2. 简化策略

### 2.1 分层策略（按风险/收益）

```
Tier A（零风险清理，预计 ~150 行）
  ├── 统一 DEBUG 日志为 perf_log!（3 处 var_os）
  └── 删 PLOTGRAM_ARCH_PACK / PLOTGRAM_FLOW_ASPECT 实验开关

Tier B（低风险合并，预计 ~100 行 + 减阶段）
  ├── D 阶段 min_gap + dock_sep 合并调用入口（不删 pass，合并 bookkeeping）
  └── Architecture_v2 后处理 7 Phase → 4 Phase 编排合并

Tier C（中风险结构精简，预计 ~600 行）
  ├── Scoring 惩罚项合并（保留 protected_trunk/outer_ring）
  ├── demand/ grid.rs + pierce.rs 移除（验证无消费者后）
  └── route_annotation.rs validate_route_edit 精简（保留 ProtectedRun）

Tier D（高风险架构级，预计 ~600 行，帕累托评判）
  └── refine/spline_fallback.rs 简化为 dogleg + degraded 标注
```

### 2.2 各 Tier 详细方案

#### Tier A：零风险清理

##### A.1 统一 DEBUG 日志为 `perf_log!`

- **位置**：`space_budget.rs:354`（`PLOTGRAM_DEBUG_EDGE_PRESSURE`）、`phases/refine.rs:181`（`PLOTGRAM_DEBUG_EDGE_ORDER`）、`phases/port_slot.rs:806`（`PLOTGRAM_DEBUG_PORT_PRESSURE`）
- **改动**：3 处 `var_os(...).is_some()` 守卫的 `eprintln!` / `println!` 块改用 `perf_log!` 宏（WASM-safe，AGENTS.md §6），并删除对应 `var_os` 读取
- **预期**：~30 行；行为不变（perf_log! 在 release 默认 no-op，与原 var_os 默认关闭等价）
- **风险**：极低；perf_log! 已是仓库统一计时/日志通道

##### A.2 删除实验性 env var `PLOTGRAM_ARCH_PACK` / `PLOTGRAM_FLOW_ASPECT`

- **位置**：`group_frame/spec.rs:427` + `:444`
- **前置验证**：先 Grep 全仓库 + 部署脚本 + playground 配置，确认无生产环境设置这两个变量
- **改动**：删除两个 `var(...)` 读取分支及其 fallback 默认值，保留默认排列/宽高比路径
- **预期**：~50 行
- **风险**：低；前提是无生产 setter（前置验证兜底）

##### A.3 移除 `PLOTGRAM_PRESSURE_BUDGET=0` 分支

- **位置**：`space_budget.rs:170` 附近
- **改动**：`PLOTGRAM_PRESSURE_BUDGET` 默认开启，`=0` 关闭分支为调试用；移除该分支，统一走默认开启路径
- **预期**：~50 行
- **风险**：低；前提是确认 `=0` 分支未被任何测试/CI 显式触发

#### Tier B：低风险合并

##### B.1 D 阶段 `min_gap` + `dock_sep` 合并调用入口

- **位置**：`pipeline.rs:258-268`（D 期 `enforce_reverse_pair_min_gap` 紧接 `enforce_reverse_pair_dock_separation`）
- **改动**：两者都是几何拉开 + grid 重建模式；合并为一次 `enforce_reverse_pair_separation_combined` 调用，内部仍分别走 min_gap / dock_sep 逻辑，但共享一次排序、一次 grid 重建、一次 stats 汇总
- **关键约束**：
  - 不删 pass（keep-list 已证 phase_lane C 期 dock_sep 承重，但 D 期这两次调用是 D 末最终写者，合并 bookkeeping 不影响几何输出）
  - 合并后跑 product-gate，`node_fp` / `tight_sev` / `exact_sev` 任一退化即回滚
- **预期**：~50 行；D 阶段步骤 11→10
- **风险**：低-中；几何输出应字节级一致

##### B.2 Architecture_v2 后处理 7 Phase → 4 Phase 编排合并

- **位置**：`architecture_v2/pipeline.rs:54-64`（`default_pipeline()` 注册 7 Phase）+ 各 Phase 定义
- **改动**：合并为 4 Phase：
  - Phase 1: `OverlapRemovalPhase` + `ClampPhase`（合并）
  - Phase 2: `NeighborAlignmentPhase` + `HubCenteringPhase`（合并，仅调一次 `rebalance_infrastructure_layers`）
  - Phase 3: `GroupBoundsPhase` + `GroupOverlapPhase`（合并）
  - Phase 4: `GroupAlignmentPhase`（保留，调一次 rebalance + enforce_gaps）
- **关键约束**：
  - `rebalance_infrastructure_layers` 调用次数 3→2（合并后 Phase 2 只调一次）；`enforce_horizontal_demand_gaps` 调用次数 4→3
  - **保留所有 rebalance/enforce_gaps 的实际逻辑**，仅合并编排入口
  - 跑 architecture 样本的 product-gate，`cloud-native` / `typical-microservice-architecture` / `ecommerce-platform` 的 node_fp 不变
- **预期**：~50 行；Phase 数 7→4
- **风险**：中；rebalance 调用次数减少可能影响节点最终坐标，需门禁验证

#### Tier C：中风险结构精简

##### C.1 Scoring 惩罚项合并

- **位置**：`scoring.rs` 10+ 惩罚因子
- **改动**：
  - 合并 `NODE_NEAR_MISS_PENALTY` + `GROUP_NEAR_MISS_PENALTY` 入 `obstacle_penalty` 的距离衰减项（保留衰减曲线，统一函数）
  - 合并 `channel_load_penalty` + `corridor_overflow_penalty` + `corridor_misalignment_penalty` 入 `overlap_penalty`（统一打分入口）
  - **保留** `protected_trunk_crossing_penalty` + `outer_ring_path_bonus`（S4 附着，keep-list）
  - **保留** `NODE_CROSSING_PENALTY` + `NODE_PIERCE_DEPTH_WEIGHT`（核心正确性）
  - **保留** `EDGE_OVERLAP_PENALTY`（核心质量）
  - **保留** `GROUP_TRANSIT_PENALTY`（穿组硬契约）
- **预期**：~200-300 行
- **风险**：中；权重数值变化可能影响路径选择，需 product-gate + 5 个 architecture 样本目视

##### C.2 demand/ `grid.rs` + `pierce.rs` 移除

- **位置**：`demand/grid.rs`（186 行）+ `demand/pierce.rs`（176 行）
- **前置验证**：Grep `demand::grid::` / `demand::pierce::` 全仓库，确认调用方仅限诊断/边序微调，且移除后边序微调可由 scoring 内的 `pierce_depth` 因子兜底
- **改动**：删除两个文件，更新 `demand/mod.rs` 的 re-export
- **预期**：~360 行
- **风险**：中；边序可能微调，需 product-gate 验证

##### C.3 `route_annotation.rs` `validate_route_edit` 精简

- **位置**：`route_annotation.rs:493`（`validate_route_edit`）+ `RouteEditObstacleCtx`（L175）+ `RouteEditValidateOpts`（L184）
- **改动**：管线简化后（Tier B 完成）逐步验证需求降低；将 `validate_route_edit` 的多选项校验合并为单一 `validate_route_edit_fast`（只查穿组 + 穿节点硬错误，移除 pierce_depth / near_miss 软校验）
- **关键约束**：**保留** `ProtectedRun`（S4 keep-list）
- **预期**：~150 行
- **风险**：中；移除软校验可能导致部分边被错误接受，需 product-gate 验证

#### Tier D：高风险架构级（帕累托评判）

##### D.1 `refine/spline_fallback.rs` 简化为 dogleg + degraded

- **位置**：`refine/spline_fallback.rs`（1002 行）
- **改动**：
  - 移除完整样条降级路径（密采样 + 无障碍栅格化，已被手册 §3.5 ★ 红线禁止）
  - 简化为：穿障时优先 `repair_through_edges` 的 dogleg（已有）；找不到干净通道则保留原边 + 显式 `degraded` 标注
  - 保留 `run_refine` 循环（最多 2 轮）+ `crossing.rs`（穿障检测）
- **评判口径**：本项走 **AGENTS.md §7 创新模式例外通道**（算法级重写）
  - 目标维度：refine 模块代码量（1002 → ~400 行）
  - 可接受临时退化：stress/demo 质量可 WARN；product-gate 正确性不退化（穿组 / det 硬）
  - 收敛判据：product-gate `tight_sev` 不劣化超 10%；`ortho.degraded_count` 可上升但每条 degraded 都有明确归因
- **预期**：~600 行
- **风险**：高；需显式抬基线 `raise stress (expected): spline_fallback 简化为 dogleg+degraded；残余: stress.xxx`

### 2.3 不做的事项（与 keep-list + 经验文档对齐）

- **不删** S4 / S4.x / B.2 / `force_outer_escape_path` / `protected_trunk_crossing_penalty` / `outer_ring_path_bonus` / `ProtectedRun`（S4 回滚后全是活跃代码）
- **不删** `resolve_exact_stub_occupancy_post_route`（守 `cloud-native` node_fp）
- **不删** phase_lane C 期 `enforce_reverse_pair_dock_separation`（C 期承重）
- **不删** `resolve_stub_occupancy_conflicts` / `separate_unrelated_trunk_overlaps[_post_route]`
- **不删** `sanitize.rs` / `snap_and_repulse` / `label_resolve` / `slot.rs` / `slot_replan.rs`
- **不建** `SeparationEngine` 统一抽象（已否决，§2.3 of 经验文档）
- **不合并** A 类重路由（`conflict_reroute` + `stub_fix`）—— 重路由能力不可并入微平移引擎
- **不图名特判**（AGENTS.md §5）
- **不裸用** `std::time::{Instant, SystemTime}`（AGENTS.md §6）
- **不引入** `phases/` 目录拆分 `route_edges_orthogonal_inner`（与 `.trae/documents/路由与布局重构执行计划v4` 是不同重构轨道，本轮不混入）

### 2.4 回滚机制（吸取第一轮 `git restore` 误伤教训）

- **临时备份目录**：`/tmp/plotgram-simplify-r2-backup/`
  - 每阶段子目录：`tier-a/` / `tier-b/` / `tier-c/` / `tier-d/`
  - 每步备份：`<tier>/<step-id>/originals/<file-path-slash>`（原文件副本）+ `<tier>/<step-id>/patch.diff`（`git diff` 输出）
- **回滚策略**：
  - **纯本次改动文件**（`git diff --numstat` 与本次删除量吻合 → HEAD == before）：`git checkout HEAD -- file` 精确还原
  - **混合文件**（含更早阶段改动）：手动只逆向本次那部分，保留更早成果；或从 `<tier>/<step-id>/originals/` 恢复
  - 回滚后必须 `compare.sh` vs before，**零漂移**才算干净
- **执行笔记**：`/tmp/plotgram-simplify-r2-backup/NOTES.md`，每步追加：
  ```
  ## <step-id> <timestamp>
  - 改动: <file:line> <what>
  - 基线: <baseline-tag-before> → <baseline-tag-after>
  - 门禁: PASS / FAIL <reason>
  - 处置: 保留 / 回滚 <how>
  ```

---

## 3. 预期结果

### 3.1 代码量变化

| 阶段 | 预计减少 | 累计减少 | 剩余总量 |
|------|----------|----------|----------|
| 第一轮已完成 | ~1,249 | ~1,249 | ~63,451 |
| Tier A（零风险清理） | ~150 | ~1,399 | ~63,301 |
| Tier B（低风险合并） | ~100 | ~1,499 | ~63,201 |
| Tier C（中风险精简） | ~700 | ~2,199 | ~62,501 |
| Tier D（高风险架构级） | ~600 | ~2,799 | ~61,901 |

**本轮目标**：从 ~63,451 行降至 ~61,901 行（再减 ~2.4%）；两轮累计减 ~4%。

> 注：原方案"减 17%"目标在第一轮已被证不成立（剩余复杂度多为本质异构）。本轮务实目标为"减 2-3% + 减阶段数 + 固化 keep-list"。

### 3.2 管线步骤变化

| 维度 | 第一轮后 | 本轮目标 |
|------|----------|----------|
| D 阶段步骤数 | 11 | 10（B.1 合并 min_gap+dock_sep） |
| Architecture 后处理 Phase 数 | 7 | 4（B.2 合并编排） |
| 环境变量门控 | 13 | 8（A.1/A.2/A.3 删 5 处） |
| Scoring 惩罚因子数 | 10+ | 6-7（C.1 合并 near_miss/channel_load） |

### 3.3 质量影响预估

| 维度 | 预期影响 | 可接受性 | 评判口径 |
|------|----------|----------|----------|
| product-gate 正确性（穿组/det） | 无影响 | 硬 | `compare.sh` product 硬 FAIL |
| product-gate 质量（exact_sev/tight_sev/node_fp） | 持平或可解释微调 | 硬 | `compare.sh` product 硬 FAIL；若需抬基线须 `raise product:` |
| stress 质量轨 | Tier D 允许 WARN | WARN | `raise stress (expected):` |
| Scoring 权重变化 | 路径选择可能微调 | 需目视 5 个 architecture 样本 | C.1 完成后跑 architecture 子集 |
| WASM 编译 | 无影响 | 硬 | `cargo check -p plotgram-wasm` |
| 大图性能（median_ms） | Tier B 可能有 ±5% | ≤10% 退化 | `bench-phases` 对比 |

### 3.4 维护性收益

- D 阶段步骤数下降，写权更清晰
- Architecture 后处理 Phase 数下降，新开发者更易追踪
- DEBUG 日志统一走 `perf_log!`，WASM-safe 一致性提升
- Scoring 惩罚项收敛，权重调优更可控
- `refine/spline_fallback` 简化后，无障碍栅格化的 ★ 红线风险消除

---

## ADDED Requirements

### Requirement: 分阶段执行与门禁闭环

本轮简化 SHALL 按 Tier A → B → C → D 顺序执行；每个 Tier 内每个 Step 完成后 SHALL 执行以下闭环：

1. 采 before 基线：`./benchmarks/snapshot.sh --tag <tier>-<step>-before`
2. 应用改动
3. 编译验证：`cargo build -p plotgram-core`
4. WASM 验证：`cargo check -p plotgram-wasm --target wasm32-unknown-unknown`
5. 单测验证：`cargo test -p plotgram-core --lib`（仅看新增失败，既有 19 项忽略）
6. 采 after 基线：`./benchmarks/snapshot.sh --tag <tier>-<step>-after`
7. 门禁比对：`./benchmarks/compare.sh <before>.json <after>.json`
8. 记笔记到 `/tmp/plotgram-simplify-r2-backup/NOTES.md`
9. 门禁 FAIL 时从 `<tier>/<step>/originals/` 精确回滚，回滚后再 compare 须零漂移

#### Scenario: Tier A 步骤门禁通过
- **WHEN** 完成 A.1（DEBUG 日志统一）的改动并跑门禁
- **THEN** product-gate PASS（node_fp / exact_sev / tight_sev 全持平），stress 轨 WARN 或 PASS
- **AND** WASM 编译通过，单测无新增失败
- **AND** 笔记记录 before/after 基线 tag + PASS 结论

#### Scenario: Tier C 步骤门禁失败
- **WHEN** C.2（demand grid.rs/pierce.rs 移除）后 compare 显示 `stress.layout-stress-dag` node_fp 变化
- **THEN** 该 Step 标记为 FAIL
- **AND** 从 `/tmp/plotgram-simplify-r2-backup/tier-c/c-2/originals/` 恢复 `demand/grid.rs` + `demand/pierce.rs` + `demand/mod.rs`
- **AND** 恢复后 `compare.sh` vs before 须零漂移
- **AND** 笔记记录 FAIL 原因 + 回滚范围 + keep-list 收敛建议

### Requirement: 临时备份与笔记机制

本轮简化 SHALL 在 `/tmp/plotgram-simplify-r2-backup/` 建立分层备份目录：

```
/tmp/plotgram-simplify-r2-backup/
├── NOTES.md                    # 执行笔记（每步追加）
├── tier-a/
│   ├── a-1-debug-log/
│   │   ├── originals/          # 原文件副本（按 src 路径命名）
│   │   └── patch.diff          # git diff 输出
│   ├── a-2-arch-pack-flow-aspect/
│   └── a-3-pressure-budget-zero/
├── tier-b/...
├── tier-c/...
└── tier-d/...
```

#### Scenario: 备份目录建立
- **WHEN** 进入 Tier A 第一个 Step 前
- **THEN** `/tmp/plotgram-simplify-r2-backup/` 及 `tier-a/a-1-debug-log/originals/` 子目录存在
- **AND** 涉及修改的每个文件已复制到 `originals/` 下（路径用 `_` 替代 `/`）
- **AND** `NOTES.md` 文件存在并写入首个 Step 的 header

### Requirement: keep-list 不可触碰

本轮简化 SHALL NOT 删除、合并或弱化 `docs/总结经验/简化重构经验-哪些不可精简-2026-07.md` §3 列出的 13 项 keep-list 中的任何一项。

#### Scenario: 简化方案触碰到 keep-list
- **WHEN** 某个 Tier 提案试图删除 `resolve_exact_stub_occupancy_post_route` 或合并 phase_lane C 期 dock_sep
- **THEN** 该提案 SHALL 被拒绝
- **AND** 笔记记录"触 keep-list，已拒绝"

### Requirement: WASM 兼容性

本轮简化 SHALL NOT 在 `crates/plotgram-core/src/` 内引入裸 `std::time::{Instant, SystemTime}` 或 `std::thread::spawn`。所有计时与日志 SHALL 走 `crate::layout::perf::Instant` 与 `perf_log!` 宏。

#### Scenario: DEBUG 日志统一引入 std::time
- **WHEN** A.1 将 `eprintln!` 改为 `perf_log!` 时误引入 `std::time::Instant` 计算 delta
- **THEN** `cargo check -p plotgram-wasm --target wasm32-unknown-unknown` SHALL 失败
- **AND** 该 Step 标记为 FAIL，须改用 `crate::layout::perf::Instant`

### Requirement: 确定性迭代

本轮简化 SHALL NOT 引入依赖 HashMap key 排序的逻辑。任何需要稳定顺序的迭代 SHALL 使用显式排序、`IndexMap` 或 `BTreeMap`。

### Requirement: 禁止图名特判

本轮简化 SHALL NOT 为特定图（如 `typical-microservice-architecture`、`cloud-native`）引入图名分支。所有规则 SHALL 通用化。

---

## MODIFIED Requirements

### Requirement: 简化方案文档与经验文档同步

`docs/布局路由算法简化重构方案-2026-07.md` SHALL 在本轮每 Tier 完成后追加"第二轮 Tier X 执行结论"段落，记录：实际净减行数、门禁结果、与原方案偏差、keep-list 收敛。

`docs/总结经验/简化重构经验-哪些不可精简-2026-07.md` SHALL 在本轮每个 FAIL 回滚后追加新条目（尝试 / 阶段 / 门禁结果 / 关键指标 / 判定）。

---

## REMOVED Requirements

无。本轮不删除任何既有 spec 要求。
