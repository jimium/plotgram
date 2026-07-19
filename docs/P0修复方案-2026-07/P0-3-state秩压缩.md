# P0-3：state 秩压缩（空 rank 折叠 + initial/final 就近安置）

> 症状图：[`showcase/state/c.layout-stress-transitions.pgm`](../../showcase/state/c.layout-stress-transitions.pgm)  
> 指标：面积利用率 ~1.5%、宽高比 4.49、initial/final 被孤立在极远 rank。

---

## ✅ 实施结果（2026-07-19）

**仅落地 §3.1（源头过滤自环）+ §3.2（repair 防御守卫）即根治**，§3.3/3.4/3.5（空 rank 压缩 / final 就近 / 空层高度）**未实施**——根因修复后秩已连续，无需追加启发式（遵守手册「禁止为消症状引入过度复杂化」）。

量化对比（`cargo run -p plotgram-cli` 渲染）：

| 指标 | 修复前 | 修复后 |
|------|--------|--------|
| 画布 | 686 × **3054** | 460 × **833** |
| 面积利用率 | 1.6% | **8.5%** |
| rank 布局 | init@20 →（**2260px 空洞**）→ pending@2280 | init@65 → pending@165 → … → done@747（连续） |

回归：flowchart `c.layout-stress-dag`（另一含自环的 sugiyama 图）修复前后**坐标完全一致**；sequence 走独立布局不受影响；`cargo test -p plotgram-core` 相对基线 **0 新增失败**（19 项为既有基线失败，前后一致）；3 条自环仍以自环装饰正常渲染（edge index 11/12/13 保留 label）。

---

## 1. 现状数据流

state 图布局入口 [`layout/node/state/mod.rs`](../../crates/plotgram-core/src/layout/node/state/mod.rs)：

```text
should_use_sugiyama (L104)
  └─ fas_reversal_ratio < 0.3 → 走 Sugiyama-v2；否则 Circular
Sugiyama-v2 (node/sugiyama_v2/)：
  build_graph / build_dag (graph.rs)      ← 不过滤自环
    → longest_path_ranks (rank.rs)
    → apply_state_semantic_rank_constraints (engine.rs L345)
         ├─ initial 钉 rank 0
         ├─ final 钉当前 max_rank
         └─ repair_rank_monotonicity (engine.rs L379)   ← 核心 Bug
    → compute_layer_heights (postprocess.rs)
    → assign_coordinates_brandes_koepf (coordinate.rs)
```

`c.layout-stress-transitions` 有 9 实体（init `initial` / done `final` / check `choice` / 6 个 state），13 条边，其中含 **3 条自环**（pending / locked / processing 各一）+ 回边 check→pending。自环与回边共同触发膨胀。

---

## 2. 根因

### 2.1 主根因：`repair_rank_monotonicity` 对自环边不收敛

[`engine.rs` L379-418](../../crates/plotgram-core/src/layout/node/sugiyama_v2/engine.rs#L379-L418)：

```rust
for (u, v) in edges {
    let ru = ranks[&u];
    let rv = ranks[&v];
    if ru >= rv {            // 自环 (u,u)：ru >= ru 恒为 true
        ranks.insert(v, ru + 1);   // 每轮把 rank(u) +1
        changed = true;            // 永远 true → 不提前 break
    }
}
// ...再扫出边把下游推开（L405-416），自环再 +1 一次
```

- 自环边 `(u,u)`：第一段循环里 `ru >= rv` 即 `ru >= ru` **恒真**，每轮令 `rank(u) += 1`；第二段出边扫描（L407-416）再令 `rank(u) += 1`（因 `ranks[v] <= ru` 也成立）。
- `changed` 永为 true，循环跑满 `node_count + 1` 轮（L385）。每轮膨胀约 +2，并把真实下游边一起级联后移。
- initial 被钉在 rank 0 不动，final 被钉在 `max_rank`（此时 max_rank 已被膨胀到极大），中间产生**大量空 rank**。
- 空 rank 经 [`compute_per_layer_gaps`](../../crates/plotgram-core/src/layout/node/sugiyama_v2/engine.rs#L197-L234)（空层 gap = 56 + min(load*2,20)）与 [`assign_coordinates_brandes_koepf`](../../crates/plotgram-core/src/layout/node/sugiyama_v2/coordinate.rs#L12-L85) 累加成巨大纵向空白 → 宽高比失衡、面积利用率极低。

### 2.2 次根因链

| 位置 | 问题 |
|------|------|
| [`graph.rs build_graph/build_dag`](../../crates/plotgram-core/src/layout/node/sugiyama_v2/graph.rs#L46-L135) | **不过滤自环** `from==to`，自环进入 DAG，污染 rank/topo。 |
| [`rank.rs topological_order`](../../crates/plotgram-core/src/layout/node/sugiyama_v2/rank.rs#L215-L259) | 自环致该节点入度永 ≥1，永不出队，被当遗留节点追加末尾；`edge_slack`（L825）自环 slack=-1。 |
| [`acyclic.rs greedy_fas`](../../crates/plotgram-core/src/layout/node/common/acyclic.rs#L28-L132) | 自环始终标 reversed（L118），但反转后仍是自环，未真正去除。 |
| [`postprocess.rs compute_layer_heights`](../../crates/plotgram-core/src/layout/node/sugiyama_v2/postprocess.rs#L11-L26) | `fold(default_h, f64::max)` 使仅含 dummy（高 8）的空层仍取 default_node_height=50，进一步撑高空 rank。 |
| final 安置策略 | `apply_state_semantic_rank_constraints` 把 final 钉到 `max_rank`，而非按其真实前驱**就近**安置，放大空洞。 |

---

## 3. 修复方案

### 3.1 从源头剔除自环（根本修复）

在 [`graph.rs`](../../crates/plotgram-core/src/layout/node/sugiyama_v2/graph.rs) 的 `build_graph` / `build_dag` 构建边时**跳过 `from == to` 的自环**：自环不参与 rank / 排序 / 坐标分配，仅作为渲染层的自环装饰（由 edge 路由阶段以节点自身为锚绘制小回环，不影响布局秩）。

- 保证 rank 语义只反映真实前后依赖。
- 需确认自环边在渲染阶段仍能被绘制（路由层从 diagram edges 读取，不依赖 DAG）。

### 3.2 `repair_rank_monotonicity` 加自环防护（防御性双保险）

即使 3.1 落地，仍在 [`engine.rs` L393](../../crates/plotgram-core/src/layout/node/sugiyama_v2/engine.rs#L393) 循环内**显式跳过 `u == v`**，避免任何未来自环边再次导致不收敛：

```rust
for (u, v) in edges {
    if u == v { continue; }   // 自环不参与单调修复
    ...
}
```
出边扫描段（L407-416）同样跳过 `v == u`。

### 3.3 空 rank 压缩后处理

在 rank 分配与语义约束之后、坐标分配之前，增加**空 rank 折叠**：收集所有被占用的 rank 值，重映射为 `0..k` 连续整数（保持相对顺序）。放在 `apply_state_semantic_rank_constraints` 之后执行，消除因回边/膨胀残留的空层。

- 确定性：按 rank 数值升序建立 `old_rank -> new_rank` 映射，遍历节点用排序后的 NodeIndex。

### 3.4 final 就近安置

将 [`apply_state_semantic_rank_constraints`](../../crates/plotgram-core/src/layout/node/sugiyama_v2/engine.rs#L345) 中 final 的安置从"钉 max_rank"改为"取所有前驱 rank 的最大值 + 1"，使 final 紧贴其最后一个真实前驱，而非被拉到全局末端。initial 保持 rank 0。

### 3.5 空层高度修正（可选，收益小）

[`compute_layer_heights`](../../crates/plotgram-core/src/layout/node/sugiyama_v2/postprocess.rs#L11-L26) 对不含 real 节点、仅含 dummy 的层，用 dummy 高度而非 default_node_height，压缩残余空层纵向占用。3.1+3.3 落地后此项收益变小，作为收尾优化。

---

## 4. 改动点清单

| 文件 | 函数 | 改动 |
|------|------|------|
| `sugiyama_v2/graph.rs` | `build_graph` / `build_dag` | 跳过 `from==to` 自环 |
| `sugiyama_v2/engine.rs` | `repair_rank_monotonicity` | 循环内 `if u==v continue`（两段扫描均加） |
| `sugiyama_v2/engine.rs` | 新增 `compress_empty_ranks` | rank 重映射为连续整数，在语义约束后调用 |
| `sugiyama_v2/engine.rs` | `apply_state_semantic_rank_constraints` | final 改为 `max(pred_rank)+1` 就近安置 |
| `sugiyama_v2/postprocess.rs` | `compute_layer_heights` | 纯 dummy 层用 dummy 高度（可选） |

---

## 5. 验证

1. `cargo run -p plotgram-cli` 渲染 `showcase/state/c.layout-stress-transitions.pgm`，确认 initial/final 紧邻真实前后驱、无巨大空 rank，面积利用率显著回升、宽高比回归正常量级。
2. 全量 `showcase/state/*` 重跑，确认其它 state 图节点坐标无非预期漂移（尤其无自环的图应完全不变或仅因空 rank 压缩合理变化）。
3. 确认 3 条自环仍以自环装饰渲染，语义未丢。
4. sugiyama_v2 既有单测通过；如坐标基线变化，说明为"消除膨胀空 rank 的预期收缩"。
