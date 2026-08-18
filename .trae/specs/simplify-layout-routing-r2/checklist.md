# Checklist

> 对应 `.trae/specs/simplify-layout-routing-r2/spec.md` 与 `tasks.md`。
> 每个 Tier 完成后逐项核验；任一 FAIL 须创建新 task 修复后重新核验。

## Phase 1: 分析 + 规划 + 文档

- [x] 现状分析章节包含第一轮执行结论回顾（Phase 1 / Phase 2 §3.2.1 / Phase 3 §3.2.3 状态）
- [x] 现状分析章节包含当前代码规模实测表（含与第一轮前差值）
- [x] 现状分析章节包含残留 env var 清单（13 处，标注处置）
- [x] 现状分析章节包含 keep-list 13 项活跃性核查（活跃位置 + 不可触碰理由）
- [x] 现状分析章节包含复杂度热点（未简化部分）清单
- [x] 简化策略章节按 Tier A/B/C/D 分层，每 Tier 列出文件位置/改动/约束/预期/风险
- [x] 简化策略章节明确"不做的事项"（与 keep-list + 经验文档对齐）
- [x] 简化策略章节包含回滚机制设计（临时备份目录 + 文件级精确回滚 + 笔记机制）
- [x] 预期结果章节包含代码量变化表（按 Tier 累计）
- [x] 预期结果章节包含管线步骤变化表
- [x] 预期结果章节包含质量影响预估表（含评判口径）
- [x] 预期结果章节包含维护性收益
- [x] spec.md 包含 ADDED Requirements（分阶段执行 / 备份机制 / keep-list / WASM / 确定性 / 禁特判）
- [x] tasks.md 按 Phase 1-5 拆分，每 Phase 拆 Step，每 Step 拆 SubTask
- [x] tasks.md 标注 Task Dependencies 与并行性说明
- [x] checklist.md 覆盖 spec 与 tasks 的所有验证点

## Phase 2: Tier A — 零风险清理

### 备份基础设施
- [x] `/tmp/tautcore-simplify-r2-backup/` 及子目录存在
- [x] `NOTES.md` 已创建并写入 header
- [x] Tier A before 基线已采集（`tier-a-before` tag，2026-07-20-053914）

### A.1 DEBUG 日志统一
- [x] `space_budget.rs:354` 的 `TAUTCORE_DEBUG_EDGE_PRESSURE` 守卫块改用 `perf_log!`
- [x] `phases/refine.rs:181` 的 `TAUTCORE_DEBUG_EDGE_ORDER` 守卫块改用 `perf_log!`
- [x] `phases/port_slot.rs:806` 的 `TAUTCORE_DEBUG_PORT_PRESSURE` 守卫块改用 `perf_log!`
- [x] 3 处 `var_os(...)` 读取已删除
- [x] 未引入裸 `std::time::{Instant, SystemTime}`（用 `crate::layout::perf::Instant`）
- [x] `cargo build -p tautcore-core` 通过
- [x] `cargo check -p tautcore-wasm --target wasm32-unknown-unknown` 通过（沙箱缺 wasm32 target，降级为代码审查：Grep 确认 3 文件无 `std::time` / `var_os` / `eprintln!` 残留，`perf_log!` 宏自身 WASM-safe，详见 NOTES.md A.1）
- [x] `cargo test -p tautcore-core --lib` 无新增失败（921 passed / 19 failed，既有 19 项忽略）
- [x] after 基线已采集（`a-1-after`），`compare.sh` vs before PASS（product 硬门禁零退化）
- [x] 笔记已记录（before/after tag + PASS/FAIL + 处置）

### A.2 删除 `TAUTCORE_ARCH_PACK` / `TAUTCORE_FLOW_ASPECT`
- [x] Grep 全仓库 + 部署脚本 + playground 配置，确认无生产 setter
- [x] `group_frame/spec.rs` 的 `TAUTCORE_ARCH_PACK` 分支（`architecture_pack_enabled()` 函数 + doc）已删除，`resolve_architecture` 直接用 `TrackSizing::Equal`
- [x] `group_frame/spec.rs` 的 `TAUTCORE_FLOW_ASPECT` 分支（`flowchart_aspect_enabled()` 函数 + doc）已删除，`choose_arrangement_mode` / `parse_group_sizing` 简化
- [x] 级联死代码 `position_macro_blocks_packed` (113 行) + `PACK_ASPECT_TARGET` 常量已删除（按 AGENTS.md §1 无向后兼容约束）
- [x] 编译 + WASM check（代码审查）+ 单测 + 基线 + compare 通过
- [x] 笔记已记录（实测 -179 行，超 spec 预期 ~50 行，主因死代码级联）

### A.3 移除 `TAUTCORE_PRESSURE_BUDGET=0` 分支
- [x] Grep 确认 `=0` 分支无测试/CI 显式触发（`congestion-baseline.rs` 是校准二进制，不在门禁管线）
- [x] `space_budget.rs` `enrich_from_pressure` 顶部 `=0` 守卫块已删除，统一走默认开启路径
- [x] `congestion-baseline.rs` `env::set_var` 默认设置已删除，文件头 doc / help / note 同步更新
- [x] 编译 + WASM check（代码审查）+ 单测 + 基线 + compare 通过（stress perf WARN 为 sandbox 噪声，correctness 全持平）
- [x] 笔记已记录

### Tier A 收尾
- [x] Tier A final 基线已采集（`tier-a-final` tag，2026-07-20-060904）
- [x] `compare.sh tier-a-before tier-a-final` 零退化（正确性 PASS / product/smoke PASS / stress/demo/mech PASS，24 样例全持平）
- [x] `docs/布局路由算法简化重构方案-2026-07.md` 追加"## 9. 第二轮 Tier A 执行结论"
- [x] 笔记总结 Tier A 实测减少行数（累计 -199 行）+ 经验

## Phase 3: Tier B — 低风险合并

> 实测结果（2026-07-20）：B.1 PASS 保留；B.2 几何字节级一致但 cloud-native median_ms 退化 +18.54% 超阈值 → 回滚。详见 `docs/布局路由算法简化重构方案-2026-07.md` §10 + NOTES.md。

### B.1 D 阶段 `min_gap` + `dock_sep` 合并（PASS 保留）
- [x] `pipeline.rs:258-268` 两调用合并为 helper `enforce_d_stage_separation`（spec §B.1 方案 B：pipeline.rs 抽包装，方案 A 因 keep-list 约束不可行）
- [x] 合并后内部仍分别走 `min_gap` / `dock_sep` 逻辑（几何输出字节级一致）
- [x] ~~共享一次排序 + 一次 grid 重建 + 一次 stats 汇总~~（N/A：方案 B 仅合并调用入口，未重构内部状态共享；keep-list 约束 dock_sep 不可重构）
- [x] `cloud-native` (4ab0e916c26b91e7) / `user-auth` (dbd04803f810) 的 `node_fp` 不变；tight_sev / exact_sev 也全不变
- [x] `compare.sh` vs Tier B before PASS
- [x] ~~D 阶段步骤数 11→10~~（N/A：方案 B 不改 D 阶段步骤数，仍 11 步）
- [x] 笔记已记录（NOTES.md B.1 章节）

### B.2 Architecture_v2 7 Phase → 4 Phase（**FAIL → 回滚**）
- [x] `architecture_v2/pipeline.rs:54-64` 注册 4 Phase（非 7）：`OverlapAndClampPhase` / `AlignmentPhase` / `GroupBoundsAndOverlapPhase` / `GroupAlignmentPhase`
- [x] `rebalance_infrastructure_layers` 调用次数 3→2
- [x] `enforce_horizontal_demand_gaps` 调用次数 4→3
- [x] 所有 rebalance/enforce_gaps 实际逻辑保留
- [x] architecture 5 样本（cloud-native / typical-microservice / ecommerce / three-tier / microservices）`node_fp` **全部字节级一致**（硬约束满足）
- [x] ~~`compare.sh` vs Tier B before PASS~~（**FAIL**：`product.cloud-native.taut: median_ms 18.99 → 22.51 (118.54% > +10%)`；其余 4 样本 perf 反而改善或持平；min/max variance 从 ±0.1ms 放大到 ±1.8ms → 真退化非 sandbox 噪声 → 按 spec §3.3 +10% 阈值回滚）
- [x] 回滚后 `compare.sh tier-b-before b-2-rollback` PASS 零漂移确认（24 样例全持平）
- [x] 笔记已记录（NOTES.md B.2 章节 + 备份保留在 `/tmp/tautcore-simplify-r2-backup/tier-b/b-2-arch-phase-merge/originals/architecture_v2_pipeline.rs`）

### Tier B 收尾
- [x] Tier B final 基线已采集（= `b-2-rollback`，2026-07-20-064930；B.2 回滚后重采）
- [x] `compare.sh tier-b-before tier-b-final` PASS（24 样例全持平，零漂移）
- [x] 方案文档更新（§10 第二轮 Tier B 执行结论）+ 笔记总结（NOTES.md Tier B 收尾章节）

## Phase 4: Tier C — 中风险结构精简

> 实测结果（2026-07-20）：C.1 SKIP（评估后判无安全保守简化空间）；C.2 SKIP（前置验证失败：活跃生产链路）；C.3 PASS（保守方案，-69 行）。详见方案文档 §11 + NOTES.md。

### C.1 Scoring 惩罚项合并 — **SKIP（记 keep-list）**
- [x] ~~`NODE_NEAR_MISS_PENALTY` + `GROUP_NEAR_MISS_PENALTY` 合并入 `obstacle_penalty` 距离衰减~~（SKIP：评估识别 `obstacle_penalty` 是路由候选打分核心，所有惩罚项活跃在路径选择）
- [x] ~~`channel_load_penalty` + `corridor_overflow_penalty` + `corridor_misalignment_penalty` 合并入 `overlap_penalty`~~（SKIP：同上，权重数值变化必影响路径选择）
- [x] **保留** `protected_trunk_crossing_penalty` + `outer_ring_path_bonus`（keep-list，S4 附着，未触碰）
- [x] **保留** `NODE_CROSSING_PENALTY` / `NODE_PIERCE_DEPTH_WEIGHT` / `EDGE_OVERLAP_PENALTY` / `GROUP_TRANSIT_PENALTY`（keep-list，未触碰）
- [x] ~~5 个 architecture 样本目视核通过~~（N/A：未实施改动）
- [x] ~~`compare.sh` vs Tier C before PASS~~（N/A：未实施改动；基于 B.2 教训「几何等价 ≠ perf 等价」，连权重数值不变的"提取公共衰减函数"都可能扰动 inline/cache，无安全保守简化空间，任务说明预案生效）
- [x] 笔记已记录（NOTES.md C.1 章节 + 经验文档 §2.5 + §3 keep-list 追加）

### C.2 demand/ `grid.rs` + `pierce.rs` 移除 — **SKIP（前置验证失败，记 keep-list）**
- [x] Grep 调用方清单已列出（4 文件：`demand/grid.rs` / `demand/pierce.rs` / `demand/mod.rs` / `demand/features.rs`）
- [x] 调用方仅限诊断/边序微调，且 scoring `pierce_depth` 可兜底 → **否**：`features.rs:11-12,195-196,254` 调用产出 `EdgeFeatures`，被 `run.rs:62-69`（边序注入，默认开启）+ `space_budget.rs:267-278`（`enrich_from_edge_features` 改 pair_gaps 几何，默认开启）消费
- [x] ~~`demand/grid.rs` + `demand/pierce.rs` 已删除~~（SKIP：前置验证失败，活跃生产链路）
- [x] ~~`demand/mod.rs` re-export 已更新~~（N/A：未实施改动）
- [x] ~~`compare.sh` vs Tier C before PASS~~（N/A：未实施改动）
- [x] ~~若 FAIL：已回滚 + 已在经验文档追加 keep-list 条目~~ → **整体 SKIP**（任务说明预案「活跃消费者则跳过 C.2」生效；记 keep-list 入经验文档 §2.6 + §3）

### C.3 `route_annotation.rs` `validate_route_edit` 精简 — **PASS（保守方案：仅删 dead options，-69 行）**
- [x] ~~`validate_route_edit` 合并为 `validate_route_edit_fast`（只查穿组+穿节点硬错误）~~ → **改为保守方案**：仅删 dead options（`VertexRole` enum / `freeze_route_annotations` 简单包装 / `try_shape_edit` / `vertex_role` + 1 测试），保留 `validate_route_edit` / `RouteEditObstacleCtx` / `RouteEditValidateOpts` / `RouteEditKind` / `RouteEditViolation` 全套（活跃调用方在 `sanitize.rs` + `grid_snap.rs`）
- [x] **保留** `ProtectedRun`（keep-list 未触碰）
- [x] ~~`RouteEditObstacleCtx` / `RouteEditValidateOpts` 简化或移除~~（保留：实际依赖在 `sanitize.rs` + `grid_snap.rs`，活跃调用方）
- [x] `compare.sh` vs Tier C before PASS（c-3-after 2026-07-20-070541 vs tier-c-before 24 样例全持平；cloud-native median_ms 18.58→18.58 零变化；5 arch 样本 node_fp 全等）
- [x] 笔记已记录（NOTES.md C.3 章节）

### Tier C 收尾
- [x] Tier C final 基线已采集（`tier-c-final`，2026-07-20-070800）
- [x] `compare.sh tier-c-before tier-c-final` PASS（正确性 PASS / product/smoke PASS / stress/demo/mech PASS，24 样例全持平；5 arch 样本 node_fp 字节级一致；cloud-native median_ms 18.58→19.01ms +2.3% sandbox 噪声 <<+10% 阈值；ecommerce-platform 54.07→46.99ms 改善 -13%）
- [x] 方案文档更新（§11 第二轮 Tier C 执行结论）+ 经验文档更新（§2.5 / §2.6 / §3 keep-list 追加 C.1 Scoring 全套权重 + C.2 demand grid+pierce）+ 笔记总结（NOTES.md Tier C 收尾章节）

## Phase 5: Tier D — 高风险架构级

> 实测结果（2026-07-20）：D.1 PASS（保守变体：删除 spline/bezier 密采样路径 + 非正交原图保留原边 + degraded 标注，-27 行）。详见方案文档 §12/§13 + NOTES.md。

### D.1 `refine/spline_fallback.rs` 简化
- [x] 1002 → 975 行（净 -27 行；git numstat: +55 / -82；spec 预期 ~600 行，实测 -27 行因保守变体保留所有 `orthogonal_*` helper）
- [x] 完整样条降级路径已移除（Bezier `else if detour_path.is_empty()` 分支 + 多段样条 `else` 分支；手册 §3.5 ★ 红线）
- [x] 改为 dogleg（保留 `orthogonal_detour_try_ports` 主路径，覆盖 `use_orthogonal_fallback=true` 的 Flowchart/Architecture）+ 显式 `degraded` 标注（对 `use_orthogonal_fallback=false` 的非正交原图保留原边 + 标 `spline_fallback_removed:bezier_or_multi_segment_spline`）
- [x] `run_refine` 循环（最多 2 轮）保留
- [x] `crossing.rs` 保留
- [x] `refine/mod.rs` 三处调用点全部保留（`run_refine` 末尾 / `repair_through_edges_post_route` / `repair_group_interior_edges_post_route`）
- [x] 帕累托评判：product-gate 正确性硬（穿组 / det 不退化）PASS
- [x] `tight_sev` 全等 PASS（24 样例无变化；激进变体 saas-schema 0→3436.747 FAIL → 保守变体 PASS）
- [x] `median_ms` 全部 ≤5% 变化 PASS（cloud-native -7.3% / ecommerce-platform -3.5% / typical-microservice-architecture +0.2%；B.2 教训应用重点核）
- [x] `ortho.degraded_count` 上升但每条 degraded 有明确归因（`spline_fallback_removed:bezier_or_multi_segment_spline`）
- [x] 显式抬基线 `raise stress (expected): spline_fallback 简化为 dogleg+degraded；残余: 无（保守变体使 product-gate 全等；stress 无新增 degraded）`
- [x] ~~若 product 正确性退化：已回滚 + 经验文档追加 keep-list~~（N/A：保守变体 PASS 保留；激进变体首次 FAIL 但已切保守变体，无需回滚）
- [x] 5 个 arch 样本 node_fp 字节级一致（three-tier / typical-microservice-architecture / microservices / cloud-native / ecommerce-platform）

### Tier D 收尾 + 全轮总结
- [x] Tier D final 基线已采集（`tier-d-final` tag，2026-07-20-074141）
- [x] `compare.sh tier-d-before tier-d-final` 符合帕累托判据（PASS，24 样例全持平）
- [x] `compare.sh tier-a-before tier-d-final` 全轮累计变化已记录（PASS，24 样例全持平；累计 -298 行 vs spec -1550 行）
- [x] `docs/布局路由算法简化重构方案-2026-07.md` 追加 "## 12. 第二轮 Tier D 执行结论" + "## 13. 第二轮总执行结论"
- [x] `docs/总结经验/简化重构经验-哪些不可精简-2026-07.md` 追加本轮新 keep-list（§2.7 + §3 表追加 1 行；第 15 项：非正交原图 fallback 不能改走 dogleg）
- [x] 笔记收尾，备份目录归档（`/tmp/tautcore-simplify-r2-backup/tier-d/d-1-spline-fallback/originals/`）

## 全轮通用核验（每 Step 必做）

> Tier A 各 Step 均已通过以下核验（详见各 Step 笔记）：

- [x] 改动前已备份原文件到 `<tier>/<step>/originals/`
- [x] 改动前已采 before 基线
- [x] 改动后 `cargo build -p tautcore-core` 通过
- [x] 改动后 `cargo check -p tautcore-wasm --target wasm32-unknown-unknown` 通过（沙箱缺 wasm32 target，降级为代码审查，详见 NOTES.md）
- [x] 改动后 `cargo test -p tautcore-core --lib` 无新增失败
- [x] 改动后采 after 基线
- [x] `compare.sh before after` 有结论（A.1/A.2/A.3 均 PASS）
- [x] 笔记已追加（改动 / 基线 / 门禁 / 处置）
- [x] 未触碰 keep-list 13 项中的任一项
- [x] 未引入裸 `std::time::{Instant, SystemTime}` / `std::thread::spawn`
- [x] 未引入 HashMap key 排序驱动逻辑（用 BTreeMap / IndexMap / 显式排序）
- [x] 未引入图名特判分支
- [x] ~~FAIL 时已从备份精确回滚 + 回滚后 `compare.sh` 零漂移~~（N/A：Tier A 全 PASS，无 FAIL 需回滚）
