# Pipeline Phase 序 + verify gate 位置

> 父页：[atlas-reference/README.md](README.md)  
> 对应新架构：[architecture.md §4 相序 / §12 落地顺序](../architecture.md)  
> Atlas 源：`crates/v1/plotgram-core/src/layout/atlas/{solve,pipeline,provenance_check}.rs`

## 这是什么

Atlas 的 `solve.rs` 是三路径（flat/weak/strong）分发 + Main LP L0–L3 松弛阶梯的总入口。它**不在内部调 InkVerifier**，验证分散在多个调用点。新架构 §4 要求「单组合相 + 类型化 Writer」，§11 要求「分相 Verifier」——Atlas 的 phase 序与 verify gate 位置是「踩坑后修出来的顺序」，值得参考；三路径壳是反模式，必须拆掉。

## 三路径分发

`solve_from_contract_with_prev` 按 `contraction::should_contract` + `profile.group_policy` 分发：

| 路径 | 函数 | 入口 |
|------|------|------|
| flat | `solve_atlas_flat` | 不收缩，直接 LayeredKernel + channel |
| weak | `solve_atlas_weak` | `contraction::solve_weak_contract_expand` |
| strong | `solve_atlas_strong` | `contraction::solve_strong_contract_expand`（Phase A→super→macro→expand） |

**新架构 §6 H6 要求「一个组合相」**——flat/weak/strong 应统一为单一 phase 序，差异通过参数注入。

## flat 路径 phase 序

```
1. LayeredKernel::compute → draft
2. postprocess::compute_layer_heights
3. build_channel_metric_with_opts（Phase I 选路，Forward 边序）
   · 成功 → run_main_axis_ladder_l0_l3
   · 失败 → ladder.push(L4)，返回 None
4. assign_coordinates_brandes_koepf_with_main_tops（M5 规范空间 horizontal=false + emit_canonical）
5. finalize_metric_tail_on_nodes（expand Main order 缝 + publish Main/Cross track_coords）
6. LayoutSession::materialize → groups
7. canvas_size + LayoutHints
```

## weak / strong 路径 phase 序

```
1. contraction::solve_{weak,strong}_contract_expand（contract → place → expand）
   · strong 多走 Phase A intra + super + macro + expand
2. metric_from_slots_publish_tracks
   · build_channel_metric_from_slots_with_opts（Phase I 选路）
   · run_metric_tail_after_channel
3. LayoutSession::materialize → groups
4. (legacy strong) phase_d_postprocess（seed→gutters→materialize）
5. canvas_size + LayoutHints
```

## Main LP 松弛阶梯（run_main_axis_ladder_l0_l3，三路径共用）

| 级 | 做什么 | 触发下一级条件 |
|----|--------|---------------|
| L0 | `solve_main_axis_with_cross_tracks` scale=1.0 | `Infeasible \|\| !audit_passed` |
| L1 | 同上 scale=0.85（缩 pitch） | 仍不可行 |
| L2 | `rebuild_reverse`：反向边序重跑 Phase I（不 skip，不热启动） | 仍不可行 |
| L3 | `inflate_layer_gaps`：启发式 tops（`vec![first_top]` 累进） | `still_bad=true`，但继续出图 |
| L4 | ChannelMetric 构建失败 | 返回 None（未真正出降级图） |

**注意**：`run_metric_tail_after_channel` 在 weak/strong 里清空 LP track_coords——注释「weak/strong 未跑 BK，节点未落到 LP 绝对层顶 → 清空 LP track_coords，改由节点几何 publish」。这是三路径壳的兜底，新实现应统一。

## apply_layer_tops_to_nodes（只扩不缩）

```rust
// 避免绝对覆盖打散 divide/strong 布局
// 不改变 rank0 绝对位置，不压缩已有间距
DIVIDE_MAX_GAP_BAND = 2 * clearance + 4 * pitch ≈ 88   // 分治路径单缝抬升上限
```

## verify gate 位置（关键）

Atlas **不在 solve.rs 内部调 InkVerifier 或 Plan::validate**。验证分散在：

| Gate | 位置 | 做什么 | 失败处理 |
|------|------|--------|---------|
| Phase I 收录 | `Plan::record_route` | `status != Converged` → `PlanError::RouteNotConverged` | Plan 不收录 |
| Main LP audit | `try_main_axis_l0_l1` | `!audit_passed` | 进 L1/L2/L3 阶梯 |
| ChannelMetric 构建 | `build_channel_metric_*` | 构建失败 | ladder L4，返回 None |
| Provenance 覆盖 | 调用方在 Ink 前调 `assert_channel_provenance_coverage` | 缺溯源 | `PlanError::MissingEdgeRecord` |
| Plan validate | 调用方 | `Plan::validate` | 返回首个违例 |
| verify_no_group_penetration | substrate 内部 + 测试硬断言 | 穿组 | 测试 fail；生产仅 substrate 内部 |
| verify_route_scope | channel/verify + 测试硬断言 | scope 违规 | 测试 fail |
| InkVerifier | 调用方在 Ink 后调 `verify_ink_vs_plan` | hard_geom_count > 0 | 视为失真 |

## pipeline.rs 的 stage 串接（facade 层）

`pipeline.rs` 是更高层 facade，按 `DialectKind` 分发到 atlas 或旧管线：

```
run_pipeline:
  1. dispatch by DialectKind（Hierarchical/Tree/Sequence/Circular）  [v1-coupled 图种分支]
  2. atlas 路径：
     provenance_check.assert_channel_provenance_coverage
     → Plan::validate
     → verify_no_group_penetration
     → materialize_edges（ink）
     → verify_ink_vs_plan（pre-ink verify 在内部）
     → label placement
     → frozen.assert_unchanged
     → orientation transform
     → group_invariant.enforce_group_invariants
     → canvas finalize
```

**注意 phase 顺序里藏的约束**（踩坑后修出来的）：

- `frozen.assert_unchanged` 必须在 orientation 之前——orientation 是几何变换，会改 frozen 集合。
- `enforce_group_invariants` 在 canvas finalize 之前——group 框可能被推开后影响 canvas。
- `verify_ink_vs_plan` 在 orientation 之前——M5 调用约束（规范空间验证）。

## 不该照搬

1. **三路径壳**：`solve_atlas_flat/weak/strong` 三个独立函数，metric tail 逻辑在 `finalize_metric_tail_on_nodes` / `metric_from_slots_publish_tracks` / `run_metric_tail_after_channel` 间重复。新架构 §6 H6 要求单一组合相。
2. **`run_metric_tail_after_channel` 清空 LP track_coords**：weak/strong 与 LP 解耦不彻底，靠清空兜底。新实现应让 weak/strong 也跑 BK 或统一坐标源。
3. **`preset_for(diagram)` 按 `DiagramType` 选 preset** `[v1-coupled]`：图种特判（AGENTS §2 红线）。生产路径应用 `preset_from_profile(profile)`。
4. **`set_override_for_solve` / `clear_override_for_solve`** `[v1-coupled]`：全局可变状态传 GroupSizing——并发不安全。
5. **solve.rs 不调 InkVerifier**：验证与求解分离，失败语义（InvalidInput/Unsupported/InfeasibleConstraint/BudgetExceeded/InternalInvariant）无统一出口。新架构 §11 应在 phase 边界显式调 verifier。
6. **pipeline.rs 按 `DialectKind` 分发** `[v1-coupled]`：图种分支。
7. **L4 没有真正出降级图**：`ladder.push(L4, "metric/channel", "build failed")` 后返回 None。新实现应让 L4 真正出降级图（带 provenance 标记）。
8. **L3 启发式 tops 仍 `still_bad=true` 但继续出图**：静默降级。新实现应明确 L3 是「软偏好放宽」还是「硬约束放宽」，并报 `BudgetExceeded`。

## 新实现建议

- **统一单一 phase 序**（新架构 §4）：dispatch → Phase I 选路 → Main LP → Cross fallback → Metric tail → Groups → Ink → Verifier → Orientation → Canvas。flat/weak/strong 差异通过 `group_policy` 参数注入同一序列。
- **phase 边界显式调 Verifier**（新架构 §11）：
  - PlanVerifier（Phase I 后）：`Plan::validate` + provenance 覆盖 + scope 独立证明
  - MetricVerifier（Metric 后）：`verify_no_group_penetration` + 组包含 + 组框互不重叠
  - InkVerifier（Ink 后）：`verify_ink_vs_plan`（hard_geom_count == 0）
  - FacadeVerifier（Orientation 后）：frozen + group invariant
- **失败语义统一**（新架构 §3.4）：InvalidInput / Unsupported / InfeasibleConstraint / BudgetExceeded / InternalInvariant，每相 verifier 产出对应类型。
- **L4 真正出降级图**：带 `Provenance::Degraded` 标记，不返回 None。
- **保留 phase 顺序约束**：frozen 在 orientation 之前；group invariant 在 canvas finalize 之前；InkVerifier 在规范空间（orientation 之前）。
- **删除全局 override**：GroupSizing 由 profile 传，不走全局 mutable。
