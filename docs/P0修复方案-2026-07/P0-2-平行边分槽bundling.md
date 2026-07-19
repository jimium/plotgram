# P0-2：真正的平行边 bundling / 分槽

> 指标：`edge_parallel_overlap`（无关平行长边完全贴合）。样例：`c.plotgram-core-mod-deps` 单图 33 处平行重叠。  
> 目标：实现"全局同向平行长边归组 + 等距分槽"预规划，替代当前纯事后逐段偏移。

---

## 1. 现状数据流

```text
lane_assignment (4f)
   ├─ enforce_reverse_pair_min_gap        ← 正反向 2 点直连对
   ├─ assign_lanes                        ← ≥4 折点 interior 段冲突分槽
   └─ separate_unrelated_trunk_overlaps   ← 不可共享 trunk 边对逐段偏移
 → semantic_trunk_merge (S3)              ← 只处理 architecture FanIn
 → sanitize → ...
```

| 组件 | 文件 | 现状 / 局限 |
|------|------|-------------|
| `edge_bundling/` | `layout/edge/edge_bundling/` | **空目录，未在任何 mod.rs 声明，全仓库零引用**。 |
| `apply_semantic_trunk_merge_filtered` | [`semantic_trunk_merge.rs` L105-157](../../crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/semantic_trunk_merge.rs) | 非 Architecture 直接返回；只合并同宿 FanIn（2-5 条）。`SemanticMergeKey` 仅 `FanIn { to_id, to_port }` 一种变体（L43）。 |
| `assign_lanes` | [`lane_assignment.rs` L543-723](../../crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/lane_assignment.rs) | 只收集 ≥4 折点路径 interior 段，O(N²) 冲突检测，Union-Find 分组后对称偏移；仅 gap<min_gap 触发。 |
| `separate_unrelated_trunk_overlaps` | [`lane_assignment.rs` L809-889](../../crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/lane_assignment.rs) | 处理不可共享 trunk 边对，尝试 [12/18/24/36/48] 偏移，**节点密集区常被 `validate_shift` 拒绝**。 |
| `edges_may_share_trunk` | [`edge_merge_policy.rs` L88-104](../../crates/plotgram-core/src/layout/edge/edge_merge_policy.rs) | 共享 trunk ⟺ 共享至少一个非 SuperEdgePair 的 MergeGroup；不同源不同宿必然不可共享 → 判 NonSemanticTrunk。 |
| `classify_segment_pair` | [`segment_pair.rs` L219-280](../../crates/plotgram-core/src/layout/edge/segment_pair.rs) | ExactOverlap（gap<0.1px）且 non-semantic → NonSemanticTrunk；MIN_SHARED_TRUNK_LEN=24px；architecture min_gap=12px。 |
| lint 计数 | [`lint/mod.rs count_unrelated_parallel_overlaps` L673](../../crates/plotgram-core/src/layout/lint/mod.rs) | `edge_parallel_overlap_count` 只保留 NonSemanticTrunk。 |

---

## 2. 根因

1. **无 bundling 实现**：`edge_bundling/` 是空壳，从未实现"多条平行边等距分槽"的全局预规划。
2. **语义合并覆盖面窄**：`semantic_trunk_merge` 仅处理 architecture 的同宿 FanIn，无法处理"不同源、不同宿、但同向平行走线的无关长边"——而这正是 `edge_parallel_overlap` 的主体。
3. **纯事后偏移在密集区失败**：`separate_unrelated_trunk_overlaps` 逐段试偏移，节点密集时 `validate_shift` 因新偏移会穿节点而拒绝，导致重叠残留。它是"局部两两"决策，缺全局视角（手册 §1.2「孤立决策」失败模式）。

缺口：缺一个**全局阶段**，把所有同向、共轴、相邻的平行长边段作为一个群体（`BundleGroup`）统一规划到等距车道，而非事后逐对挤。

---

## 3. 修复方案

### 3.1 新建 `edge_bundling` 模块

在 [`layout/edge/edge_bundling/`](../../crates/plotgram-core/src/layout/edge/) 实现全局平行段归组与等距分槽，并在 `layout/edge/mod.rs` 用 `mod edge_bundling;` 声明。核心数据结构：

```text
struct SegmentKey { axis: Horizontal|Vertical, coord: i32(量化), span: (lo,hi) }
struct BundleGroup { key: SegmentKey, members: Vec<EdgeSegRef> }  // members 按确定性排序
```

流程：
1. **全局段方向索引**：遍历所有已路由边的 interior 段，按 (轴向, 量化坐标) 建索引，收集共轴且 span 有重叠的段。
2. **归组**：把间距 < min_gap（architecture 12px / 其余按现有阈值）且不可共享 trunk（`edges_may_share_trunk == false`）的同向段归入同一 `BundleGroup`。
3. **等距分槽**：对每个 BundleGroup 按成员数 n 在垂直于轴向的方向分配 n 条等距车道（中心对称，间距 = min_gap），一次性平移各成员段到目标车道。
4. **穿障校验**：每个成员平移后用 P0-1 的 `PreparedObstacles::path_violations` 校验不新增穿节点/穿组；若某成员分槽后穿障，回退该成员（保留在原车道并标记，交后续 lane 处理），不牺牲全组。

### 3.2 插入管线位置

插在 `lane_assignment` **之后**、`semantic_trunk_merge` **之前**（手册 §3.1）：
- lane 之后：已完成基本 interior 段冲突分槽，bundling 处理其残留的全局平行群体。
- semantic merge 之前：避免与语义 FanIn 合并抢写权；语义应共享的边由 semantic merge 处理，bundling 只处理"不可共享 trunk"的无关平行边。

### 3.3 与 `separate_unrelated_trunk_overlaps` 的关系

新 bundling 承接全局平行群体分槽后，`separate_unrelated_trunk_overlaps` 退化为"处理 bundling 回退的残余两两冲突"的兜底，逻辑不变，只是输入变少。避免两处对同一段重复写（手册 §1.1 法则 6：明确最终写者——bundling 是平行段车道的最终写者）。

### 3.4 确定性

- SegmentKey 坐标量化取整；BundleGroup members 按 (edge_id, seg_index) 排序（AGENTS.md §2）。
- 车道分配用成员在排序后的 index 决定中心对称偏移，不依赖 HashMap 遍历序。

---

## 4. 改动点清单

| 文件 | 改动 |
|------|------|
| `layout/edge/edge_bundling/mod.rs`（新建） | `BundleGroup` / `SegmentKey` / `bundle_parallel_segments` |
| `layout/edge/mod.rs` | `mod edge_bundling;` 声明 + 导出 |
| 正交路由主编排（调用 lane_assignment 处） | 在 lane 后、semantic merge 前调用 `bundle_parallel_segments` |
| `edge_routing_orthogonal/lane_assignment.rs` | `separate_unrelated_trunk_overlaps` 降为残余兜底（可保留） |
| （复用）`context.rs` | 依赖 P0-1 的 `PreparedObstacles::path_violations` 做分槽后校验 |

---

## 5. 验证

1. `cargo run -p plotgram-cli` 渲染 `c.plotgram-core-mod-deps`，确认 33 处平行重叠显著下降、平行长边呈等距车道而非贴合。
2. 全量 showcase：`edge_parallel_overlap` 总量下降；`edge_node_crossings`/`edge_through_groups` **不上升**（分槽校验保证）。
3. 语义 FanIn（architecture 同宿合流）仍正确共享 trunk，未被 bundling 误拆——确认 semantic merge 写权未被抢。
4. 节点坐标不变；重叠严重度整体下降或可解释持平（手册 §1.3）。
6. 依赖项：本方案依赖 P0-1 的统一障碍查询接口，须在 P0-1 落地后实施。
