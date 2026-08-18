# Tasks

> 本任务列表对应 `.trae/specs/simplify-layout-routing-r2/spec.md` 的 4 个 Tier。
> 每个 Tier 内每个 Step 完成后须跑门禁闭环（spec §ADDED Requirement: 分阶段执行与门禁闭环）。
> 用户当前要求执行的是 **Phase 1（分析+规划+文档）**，即下方 Phase 1 全部 + Phase 2-5 的占位。
> 后续 Phase 2-5 在用户批准 spec 后逐个展开实施。

## Phase 1: 分析 + 规划 + 文档（当前阶段）

- [x] Task 1.1: 阅读第一轮方案与经验文档
  - [x] SubTask 1.1.1: 读 `docs/布局路由算法简化重构方案-2026-07.md`
  - [x] SubTask 1.1.2: 读 `docs/总结经验/简化重构经验-哪些不可精简-2026-07.md`
  - [x] SubTask 1.1.3: 读 `docs/总结经验/布局与路由核心手册-2026-07.md`
  - [x] SubTask 1.1.4: 读 `benchmarks/baselines/latest.md` + `2026-07-20.md`
- [x] Task 1.2: 审计当前代码状态（post 第一轮）
  - [x] SubTask 1.2.1: 统计 `edge/edge_routing_orthogonal/` 各文件行数
  - [x] SubTask 1.2.2: 统计 `node/architecture_v2/` 各文件行数
  - [x] SubTask 1.2.3: 统计 `refine/` / `demand/` / `edge/common/` / `group_frame/` / `lint/` 行数
  - [x] SubTask 1.2.4: 验证 Phase 1 已完成项（corridor_stick / channel_occupancy / PREFER / CHECK_D_IDEMPOTENT / dump 精简）
  - [x] SubTask 1.2.5: 验证 Phase 2 §3.2.1 状态（C 末 dock_sep / phase_lane C 期 min_gap / gap_fixed）
  - [x] SubTask 1.2.6: 残留 env var 全量 Grep（13 处）
  - [x] SubTask 1.2.7: keep-list 13 项活跃性核查
- [x] Task 1.3: 编写 spec.md（含现状分析/简化策略/预期结果三章节）
- [x] Task 1.4: 编写 tasks.md（本文件，分 Phase 1-5，每 Phase 拆 Step）
- [x] Task 1.5: 编写 checklist.md
- [x] Task 1.6: 调用 NotifyUser 请求用户批准

## Phase 2: Tier A — 零风险清理（预计 ~150 行）

- [x] Task 2.0: 建立临时备份基础设施
  - [x] SubTask 2.0.1: `mkdir -p /tmp/tautcore-simplify-r2-backup/{tier-a/{a-1-debug-log,a-2-arch-pack-flow-aspect,a-3-pressure-budget-zero},tier-b,tier-c,tier-d}`
  - [x] SubTask 2.0.2: 创建 `NOTES.md` 写入 header
  - [x] SubTask 2.0.3: 采 Phase 2 before 基线 `./benchmarks/snapshot.sh --tag tier-a-before`
- [x] Task 2.1: A.1 统一 DEBUG 日志为 `perf_log!`
  - [x] SubTask 2.1.1: 备份 `space_budget.rs` / `phases/refine.rs` / `phases/port_slot.rs` 到 `tier-a/a-1-debug-log/originals/`
  - [x] SubTask 2.1.2: 改 `space_budget.rs:354` 的 `TAUTCORE_DEBUG_EDGE_PRESSURE` 守卫块为 `perf_log!`
  - [x] SubTask 2.1.3: 改 `phases/refine.rs:181` 的 `TAUTCORE_DEBUG_EDGE_ORDER` 守卫块为 `perf_log!`
  - [x] SubTask 2.1.4: 改 `phases/port_slot.rs:806` 的 `TAUTCORE_DEBUG_PORT_PRESSURE` 守卫块为 `perf_log!`
  - [x] SubTask 2.1.5: 删除 3 处 `var_os(...)` 读取
  - [x] SubTask 2.1.6: `cargo build -p tautcore-core` + `cargo check -p tautcore-wasm --target wasm32-unknown-unknown`（WASM check 因沙箱缺 wasm32 target 降级为代码审查，详见 NOTES.md A.1）
  - [x] SubTask 2.1.7: `cargo test -p tautcore-core --lib`（921 passed / 19 failed，无新增失败）
  - [x] SubTask 2.1.8: 采 after 基线 + `compare.sh` vs before（PASS）
  - [x] SubTask 2.1.9: 笔记 + 门禁 PASS 保留
- [x] Task 2.2: A.2 删除 `TAUTCORE_ARCH_PACK` / `TAUTCORE_FLOW_ASPECT` 实验开关
  - [x] SubTask 2.2.1: Grep 全仓库 + 部署脚本 + playground 配置，确认无生产 setter
  - [x] SubTask 2.2.2: 备份 `group_frame/spec.rs` + `macro_block.rs` + `group_sizing.rs` + `group_divide.rs`
  - [x] SubTask 2.2.3: 删除 `TAUTCORE_ARCH_PACK` 分支（`architecture_pack_enabled()` 函数 + doc），简化 `resolve_architecture` 直接用 `TrackSizing::Equal`；级联删除死代码 `position_macro_blocks_packed` (113 行) + `PACK_ASPECT_TARGET` 常量
  - [x] SubTask 2.2.4: 删除 `TAUTCORE_FLOW_ASPECT` 分支（`flowchart_aspect_enabled()` 函数 + doc），简化 `choose_arrangement_mode` / `parse_group_sizing`
  - [x] SubTask 2.2.5: 编译 + WASM check（代码审查）+ 单测 + 基线 + compare（PASS）+ 笔记
- [x] Task 2.3: A.3 移除 `TAUTCORE_PRESSURE_BUDGET=0` 分支
  - [x] SubTask 2.3.1: Grep 确认 `=0` 分支无测试/CI 显式触发（congestion-baseline.rs 是校准二进制，不在门禁管线）
  - [x] SubTask 2.3.2: 备份 `space_budget.rs` + `bin/congestion-baseline.rs`
  - [x] SubTask 2.3.3: 删除 `space_budget.rs` `enrich_from_pressure` 顶部 `=0` 守卫块；删除 `congestion-baseline.rs` `env::set_var` 默认设置 + 同步更新文件头 doc / help / note
  - [x] SubTask 2.3.4: 编译 + WASM check（代码审查）+ 单测 + 基线 + compare（PASS，stress perf WARN 为噪声）+ 笔记
- [x] Task 2.4: Tier A 收尾
  - [x] SubTask 2.4.1: 采 Tier A final 基线 `./benchmarks/snapshot.sh --tag tier-a-final`（2026-07-20-060904）
  - [x] SubTask 2.4.2: `compare.sh tier-a-before tier-a-final` 确认零退化（正确性 PASS / product/smoke PASS / stress/demo/mech PASS，24 样例全持平）
  - [x] SubTask 2.4.3: 更新 `docs/布局路由算法简化重构方案-2026-07.md` 追加"## 9. 第二轮 Tier A 执行结论"
  - [x] SubTask 2.4.4: 笔记总结 Tier A 实测减少行数（-199 行）+ 经验

## Phase 3: Tier B — 低风险合并（预计 ~100 行 + 减阶段）

> 实测结果（2026-07-20）：B.1 PASS 保留（-3 行）；B.2 编排合并几何字节级一致但 cloud-native median_ms 退化 +18.54% 超阈值，按门禁规则回滚。Tier B 累计 -3 行。详见 `docs/布局路由算法简化重构方案-2026-07.md` §10 + `/tmp/tautcore-simplify-r2-backup/NOTES.md`。

- [x] Task 3.0: 采 Tier B before 基线 `./benchmarks/snapshot.sh --tag tier-b-before`（2026-07-20-061403）
- [x] Task 3.1: B.1 D 阶段 `min_gap` + `dock_sep` 合并调用入口（PASS 保留，pipeline.rs 467→464 行）
  - [x] SubTask 3.1.1: 备份 `pipeline.rs`（`/tmp/tautcore-simplify-r2-backup/tier-b/b-1-d-separation-merge/originals/pipeline.rs`）
  - [x] SubTask 3.1.2: 读 `pipeline.rs:258-268` 现状（D 期两次 `enforce_reverse_pair_*` 调用）
  - [x] SubTask 3.1.3: 选 spec §B.1 方案 B（pipeline.rs 抽 helper `enforce_d_stage_separation` 包装，keep-list 约束 dock_sep 不可重构内部状态共享，方案 A 不可行）
  - [x] SubTask 3.1.4: 实施合并（新增 helper + 调用点替换为 1 行）
  - [x] SubTask 3.1.5: 编译 + WASM check（代码审查）+ 单测（921/19 不变）+ 基线 b-1-after + compare PASS
  - [x] SubTask 3.1.6: 重点核 `cloud-native` (4ab0e916c26b91e7) / `user-auth` (dbd04803f810) node_fp 不变；tight_sev/exact_sev 全不变
- [x] Task 3.2: B.2 Architecture_v2 后处理 7 Phase → 4 Phase 编排合并（**FAIL → 回滚**）
  - [x] SubTask 3.2.1: 备份 `architecture_v2/pipeline.rs`（`/tmp/tautcore-simplify-r2-backup/tier-b/b-2-arch-phase-merge/originals/architecture_v2_pipeline.rs`，228 行）
  - [x] SubTask 3.2.2: 读 `architecture_v2/pipeline.rs:54-64` 现状 + 7 Phase 定义
  - [x] SubTask 3.2.3: 合并 Phase 1（`OverlapAndClampPhase` = 原 5 + 5.5）
  - [x] SubTask 3.2.4: 合并 Phase 2（`AlignmentPhase` = 原 5.6 + 5.6'，rebalance 3→2）
  - [x] SubTask 3.2.5: 合并 Phase 3（`GroupBoundsAndOverlapPhase` = 原 6 + 6.2）
  - [x] SubTask 3.2.6: 保留 Phase 4（`GroupAlignmentPhase` = 原 6.3，rebalance + enforce_gaps；enforce_gaps 4→3）
  - [x] SubTask 3.2.7: 编译 + WASM check（代码审查）+ 单测（921/19 不变）+ 基线 b-2-after + compare **FAIL**：`product.cloud-native.taut: median_ms 18.99 → 22.51 (118.54% > +10%)`
  - [x] SubTask 3.2.8: 重点核 architecture 5 样本 node_fp **全部字节级一致**（硬约束满足）；但 median_ms 退化超 spec §3.3 +10% 阈值，按门禁规则回滚。回滚后 compare vs tier-b-before PASS 零漂移确认
- [x] Task 3.3: Tier B 收尾
  - [x] SubTask 3.3.1: 采 Tier B final 基线（= `b-2-rollback`，2026-07-20-064930；B.2 回滚后重采）
  - [x] SubTask 3.3.2: `compare.sh tier-b-before tier-b-final` PASS（24 样例全持平）
  - [x] SubTask 3.3.3: 更新方案文档（§10）+ 笔记（`/tmp/tautcore-simplify-r2-backup/NOTES.md`）

## Phase 4: Tier C — 中风险结构精简（预计 ~700 行）

> 实测结果（2026-07-20）：C.1 SKIP（评估后判无安全保守简化空间，记 keep-list）；C.2 SKIP（前置验证失败：grid.rs/pierce.rs 经 EdgeFeatures 喂 run.rs 边序 + space_budget 几何改写，活跃生产链路，记 keep-list）；C.3 PASS 保留（保守方案：仅删 dead options，-69 行）。Tier C 累计 -69 行。详见 `docs/布局路由算法简化重构方案-2026-07.md` §11 + `/tmp/tautcore-simplify-r2-backup/NOTES.md`。

- [x] Task 4.0: 采 Tier C before 基线 `./benchmarks/snapshot.sh --tag tier-c-before`（2026-07-20-065823）
- [x] Task 4.1: C.1 Scoring 惩罚项合并 — **SKIP（评估后判无安全保守简化空间，记 keep-list）**
  - [x] SubTask 4.1.1: 备份 `scoring.rs`（已完成备份到 `tier-c/c-1-scoring/originals/scoring.rs`）
  - [x] SubTask 4.1.2: 读 `scoring.rs` 10+ 惩罚因子现状（识别 `obstacle_penalty` L113-197 + `overlap_penalty` L323-344 是路由打分核心，所有惩罚项活跃在路径选择）
  - [x] ~~SubTask 4.1.3: 合并 `NODE_NEAR_MISS_PENALTY` + `GROUP_NEAR_MISS_PENALTY` 入 `obstacle_penalty` 距离衰减~~（SKIP：权重数值变化必影响路径选择）
  - [x] ~~SubTask 4.1.4: 合并 `channel_load_penalty` + `corridor_overflow_penalty` + `corridor_misalignment_penalty` 入 `overlap_penalty`~~（SKIP：同上）
  - [x] SubTask 4.1.5: 保留 `protected_trunk_crossing_penalty` + `outer_ring_path_bonus`（keep-list 未触碰）
  - [x] ~~SubTask 4.1.6: 编译 + WASM + 单测 + 基线 + compare~~（N/A：未实施改动）
  - [x] ~~SubTask 4.1.7: 5 个 architecture 样本目视核~~（N/A：未实施改动）
  - [x] SubTask 4.1.8: ~~FAIL 回滚（保留 keep-list 项）~~ → **整体 SKIP**（任务说明预案：「方案 B 也 FAIL → C.1 整体放弃，记 keep-list」；基于 B.2 教训「几何等价 ≠ perf 等价」，连"提取公共衰减函数"都可能扰动 inline/cache，无安全保守简化空间）
- [x] Task 4.2: C.2 demand/ `grid.rs` + `pierce.rs` 移除 — **SKIP（前置验证失败：活跃生产链路，记 keep-list）**
  - [x] SubTask 4.2.1: Grep `demand::grid::` / `demand::pierce::` 全仓库，列出所有调用方（4 文件：`demand/grid.rs` / `demand/pierce.rs` / `demand/mod.rs` / `demand/features.rs`）
  - [x] SubTask 4.2.2: 评估调用方是否仅限诊断/边序微调，且 scoring 内 `pierce_depth` 可兜底 → **否**：`features.rs:11-12,195-196,254` 调用产出 `EdgeFeatures`，被 `run.rs:62-69`（边序注入，默认开启）+ `space_budget.rs:267-278`（`enrich_from_edge_features` 改 pair_gaps 几何，默认开启）消费
  - [x] SubTask 4.2.3: 备份 `demand/grid.rs` + `demand/pierce.rs` + `demand/mod.rs`（已完成备份到 `tier-c/c-2-demand-grid-pierce/originals/`）
  - [x] ~~SubTask 4.2.4: 删除两个文件 + 更新 `mod.rs` re-export~~（SKIP：前置验证失败）
  - [x] ~~SubTask 4.2.5: 编译 + WASM + 单测 + 基线 + compare~~（N/A：未实施改动）
  - [x] SubTask 4.2.6: ~~FAIL 回滚 + 记 keep-list~~ → **整体 SKIP**（任务说明预案：「活跃消费者则跳过 C.2」；记 keep-list 入 `docs/总结经验/简化重构经验-哪些不可精简-2026-07.md` §2.6）
- [x] Task 4.3: C.3 `route_annotation.rs` `validate_route_edit` 精简 — **PASS（保守方案：仅删 dead options，-69 行）**
  - [x] SubTask 4.3.1: 备份 `route_annotation.rs` + `edge/mod.rs`（`tier-c/c-3-route-annotation/originals/`）
  - [x] SubTask 4.3.2: 读 `route_annotation.rs` 全文（765 行）+ Grep 4 个 crate（tautcore-wasm / tautcore-cli / tautcore-eval / tautcore-server）确认 `try_shape_edit` / `freeze_route_annotations` / `vertex_role` / `VertexRole` 零外部 caller
  - [x] ~~SubTask 4.3.3: 合并为 `validate_route_edit_fast`（只查穿组+穿节点硬错误，移除 pierce_depth/near_miss 软校验）~~ → **改为保守方案**：仅删 dead options（`VertexRole` enum / `freeze_route_annotations` 简单包装 / `try_shape_edit` / `vertex_role` + 1 测试 `try_shape_edit_reverts_on_stub_failure`），保留 `validate_route_edit` / `RouteEditObstacleCtx` / `RouteEditValidateOpts` / `RouteEditKind` / `RouteEditViolation` 全套（活跃调用方在 `sanitize.rs` + `grid_snap.rs`）
  - [x] SubTask 4.3.4: 保留 `ProtectedRun`（keep-list 未触碰）
  - [x] SubTask 4.3.5: 编译 + WASM check（代码审查）+ 单测（920/19，删 1 个 dead test）+ 基线 c-3-after (2026-07-20-070541) + compare vs tier-c-before PASS（24 样例全持平）
  - [x] ~~SubTask 4.3.6: FAIL 回滚~~（N/A：PASS 保留）
- [x] Task 4.4: Tier C 收尾
  - [x] SubTask 4.4.1: 采 Tier C final 基线（2026-07-20-070800-tier-c-final）
  - [x] SubTask 4.4.2: `compare.sh tier-c-before tier-c-final` PASS（正确性 PASS / product/smoke PASS / stress/demo/mech PASS，24 样例全持平；5 arch 样本 node_fp 字节级一致；cloud-native median_ms 18.58→19.01ms +2.3% sandbox 噪声范围 <<+10% 阈值）
  - [x] SubTask 4.4.3: 更新方案文档 §11 + 经验文档 §2.5/§2.6/§3 keep-list 追加 + 笔记追加 NOTES.md Tier C 收尾章节

## Phase 5: Tier D — 高风险架构级（帕累托评判，预计 ~600 行）

> 实测结果（2026-07-20）：D.1 PASS（保守变体：删除 spline/bezier 密采样路径 + 非正交原图保留原边 + degraded 标注，-27 行）。详见 `docs/布局路由算法简化重构方案-2026-07.md` §12 + §13 + `/tmp/tautcore-simplify-r2-backup/NOTES.md`。

- [x] Task 5.0: 采 Tier D before 基线 `./benchmarks/snapshot.sh --tag tier-d-before`（2026-07-20-073020）
- [x] Task 5.1: D.1 `refine/spline_fallback.rs` 简化为 dogleg + degraded
  - [x] SubTask 5.1.1: 备份 `refine/spline_fallback.rs` + `refine/mod.rs`（`/tmp/tautcore-simplify-r2-backup/tier-d/d-1-spline-fallback/originals/`）
  - [x] SubTask 5.1.2: 读 `spline_fallback.rs` 1002 行现状 + `refine/mod.rs` 调用点 + 各 diagram_type 默认 router（Flowchart/Architecture=orthogonal; State=circular; Er=spline; Mindmap=organic; Sequence=straight）
  - [x] SubTask 5.1.3: 评估 dogleg 覆盖率：`use_orthogonal_fallback=true`（原边正交 OR Architecture）覆盖 Flowchart/Architecture；`use_orthogonal_fallback=false` 覆盖 State/Er/Mindmap，product-gate 中 saas-schema (ER) 实际触发 fallback candidates=2 accepted=2
  - [x] SubTask 5.1.4: 实施简化：删除 Bezier + 多段样条密采样分支（手册 §3.5 ★ 红线）；激进变体（非正交也走 dogleg）首次 FAIL（saas-schema tight_sev 0→3436.747），改保守变体（非正交保留原边 + degraded 标注）PASS
  - [x] SubTask 5.1.5: 保留 `run_refine` 循环（mod.rs 已有，最多 2 轮）+ `crossing.rs` 不动 + mod.rs 三处调用点全部保留
  - [x] SubTask 5.1.6: 编译 + WASM check（代码审查）+ 单测（920/19 不变）+ 基线 d-1-after-v2 (2026-07-20-073904) + compare vs tier-d-before PASS（24 样例全持平）
  - [x] SubTask 5.1.7: 帕累托评判：product-gate 正确性硬（穿组 / det 不退化）PASS；tight_sev 全等 PASS；median_ms 全部 ≤5% 变化 PASS；ortho.degraded_count 上升但每条有归因（`spline_fallback_removed:bezier_or_multi_segment_spline`）
  - [x] SubTask 5.1.8: 显式抬基线 `raise stress (expected): spline_fallback 简化为 dogleg+degraded；残余: 无（保守变体使 product-gate 全等；stress 无新增 degraded）`
  - [x] ~~SubTask 5.1.9: FAIL 回滚（若 product 正确性退化）~~（N/A：保守变体 PASS 保留）
- [x] Task 5.2: Tier D 收尾 + 全轮总结
  - [x] SubTask 5.2.1: 采 Tier D final 基线 `./benchmarks/snapshot.sh --tag tier-d-final`（2026-07-20-074141）
  - [x] SubTask 5.2.2: `compare.sh tier-d-before tier-d-final` 符合帕累托判据（PASS，24 样例全持平）
  - [x] SubTask 5.2.3: 全轮累计：`compare.sh tier-a-before tier-d-final`（tier-a-before tag = 2026-07-20-053914）PASS（24 样例全持平，累计 -298 行）
  - [x] SubTask 5.2.4: 更新 `/workspace/docs/布局路由算法简化重构方案-2026-07.md` 追加 "## 12. 第二轮 Tier D 执行结论" + "## 13. 第二轮总执行结论" 章节
  - [x] SubTask 5.2.5: 更新 `/workspace/docs/总结经验/简化重构经验-哪些不可精简-2026-07.md` 追加本轮新 keep-list（D.1 第 15 项：非正交原图 fallback 不能改走 dogleg）
  - [x] SubTask 5.2.6: 笔记收尾：在 `/tmp/tautcore-simplify-r2-backup/NOTES.md` 追加 Tier D 完整执行章节 + 全轮总结章节（含改动/基线/门禁/处置/经验）

# Task Dependencies

- Phase 1（当前）→ Phase 2：spec 批准后进入 Tier A
- Phase 2 → Phase 3：Tier A 全 PASS 后进入 Tier B（A 失败项回滚不影响 B 进入）
- Phase 3 → Phase 4：Tier B 全 PASS 后进入 Tier C
- Phase 4 → Phase 5：Tier C 完成后（含 FAIL 回滚项）进入 Tier D
- 同一 Phase 内 Step 顺序执行（每个 Step 依赖前一个 Step 的基线作为 before）
- 跨 Phase 的回滚不影响已完成 Phase 的成果（吸取第一轮 `git restore` 误伤教训，使用文件级精确回滚）

# 并行性说明

- 同一 Step 内的 SubTasks 顺序执行（涉及同一文件多次修改）
- 不同 Step 的备份目录创建可并行（但实施改动仍顺序，避免基线混乱）
- Tier D 的 SubTask 5.1.3（评估 dogleg 覆盖率）可与 SubTask 5.1.1/5.1.2 并行（只读分析）
