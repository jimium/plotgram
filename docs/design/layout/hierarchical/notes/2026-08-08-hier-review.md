# Hierarchical 布局 · 综合评审与改进方案

> 日期：2026-08-08 · 状态：**评审报告（不是契约）**
> 评审对象：`crates/plotgram-layout/src/layout/hierarchical/`（14 588 行，HEAD = `23555ac`）
> 对标基准：yFiles `HierarchicalLayout` + 需求期望 [`expectations.md`](../expectations.md)
> 尺子：[`write-authority.md`](../../write-authority.md) · [`architecture.md`](../architecture.md)
>
> 本文只做三件事：**指出问题、给出证据、给出可执行的算法方案**。裁定权仍在 `expectations.md`。

---

## 0. 摘要

| # | 结论 | 严重度 | 证据 |
|---|------|--------|------|
| **A** | `compose/order.rs::cmp_key` **不是全序关系**，任何层宽 ≥18 的图直接 `panic` | **P0 崩溃** | §2.1 复现 |
| **B** | HEAD 上 **3 个快照门禁是红的** | **P0 流程** | §2.2 |
| **C** | 对称性在**树状图上完美、在 DAG 上塌陷**：多父节点一出现，扇心偏移可达 220px | **P1 质量** | §2.3 实测表 |
| **D** | 规模化是 **O(n^2.85)**：1600 节点 11.1s（release）。72% 在 Network Simplex，28% 在 transpose | **P1 性能** | §2.4 profile |
| **E** | Ink **确实在发明几何**：`cross_axis_stub_clear` / `horizontal_clear_at_y` 在落笔期做避障判定并改折线拓扑 | **P1 写权** | §2.5 |
| **F** | **组通道在 40 个组 fixture 里有 31 个静默失效**，回退 root-scope；Gate 生效组平均 1.7 交叉，回退组平均 **14.5** | **P0 质量** | §2.6 因果链 |
| **G** | `metric/track.rs::clear_main_x` 是典型的「事后挪线」：全图 X 投影合并后把竖廊推到画布边缘 | **P2 架构** | §2.7 |
| **H** | `InkVerifier` 只检测**完全相同**的折线；**自环在两次 verify 之后才加入，完全不被校验** | **P1 验真** | §2.8 |
| **I** | `GateCapacity::Fixed` 从未被产生（derive 恒 `Unbounded`），容量机制名存实亡；`verify_no_group_penetration` 不存在 | **P2 架构** | §2.10 |
| **J** | `ink::route::lane_coord` 在缺 TrackOrder 分配时静默 `unwrap_or(0)` 选 lane 0 | **P2 正确性** | §2.11 |

**一句话判断**：管线相位划分、写权文档化、确定性纪律都做得比一般开源实现好；**问题不在架构，在算法内核的形态**——当前次轴/对称是「贪心声明表 + 硬等式约束」，它在单亲树上能给出教科书级结果，但没有全局目标函数，因此在真实 DAG 上没有收敛保证，只能靠不断追加成员表谓词来救。§4 给出把它换成「目标函数 + 迭代求解」的详细方案。

**两条最值得先动的线**（收益远超其它）：

1. **`cmp_key` 全序**（§4.2.1，半天）——这是正在生效的崩溃。
2. **组连续性改用边界 dummy**（§4.2.2）——§2.6 证明了它一次性解开「transpose 被锁死 → 组包围盒不纯 → Channel 组通道静默回退 → 交叉爆炸」整条链，是全文杠杆最高的一处。

---

## 1. 评审口径与证据来源

**读过的代码**：`compose/`（cycle · rank · properify · order · graph_index · verify · track_order · bundle）、`metric/`（bk · cross_axis · symmetry · port_lane · main_axis · anchor · track）、`demand.rs`、`ink/verify.rs`、`ink/route.rs`（前 330 行）、`channel/search.rs`（代价模型）、`channel/substrate.rs`、`mod.rs`、`model.rs`、`params.rs`。

**跑过的验证**：

1. `cargo test -p plotgram-layout -p plotgram-compile --no-fail-fast`
2. 78 个 fixture 的 `hier_eval_baseline.json` 聚合统计
3. 合成压测图（宽层 / 窄深层两族，100–1600 节点）
4. macOS `sample` 采样 profile（debug 构建，带符号）
5. `debug-layout` JSON 的扇出/扇入对称性定量扫描
6. 代表图 SVG 目检

复现脚本见 §7。

---

## 2. 缺陷清单（按严重度）

### 2.1 【P0】`cmp_key` 不是全序 → 宽层直接崩溃

```351:365:crates/plotgram-layout/src/layout/hierarchical/compose/order.rs
fn cmp_key(a: &Key, b: &Key) -> Ordering {
    if let (Some(x), Some(y)) = (a.median, b.median) {
        if (x - y).abs() > EPS {
            return x.partial_cmp(&y).unwrap();
        }
    }
    if let (Some(x), Some(y)) = (a.barycenter, b.barycenter) {
        if (x - y).abs() > EPS {
            return x.partial_cmp(&y).unwrap();
        }
    }
    a.prev_pos
        .cmp(&b.prev_pos)
        .then(a.repr_decl.cmp(&b.repr_decl))
}
```

**两处破坏全序**：

1. **ε 阈值破坏传递性**。设 `a.median=0`、`b.median=0.6·EPS`、`c.median=1.2·EPS`。则 `a≈b`（落到 `prev_pos`）、`b≈c`（落到 `prev_pos`），但 `a<c`（`|0−1.2EPS| > EPS` 走 median 分支）。若 `prev_pos` 顺序是 `c<b<a`，比较器同时声称 `c<a` 与 `a<c`。
2. **`None` 与 `Some` 混排不一致**。`Some/Some` 用 median 比，`Some/None` 直接跳到 `prev_pos`。同一层里既有「有邻居」又有「无邻居」的块时（孤立节点、被反向边切断的节点），排序关系不满足传递性。

Rust 的 `driftsort` 在切片长度超过小排序阈值时会检测非法比较器并 `panic`。

**实测崩溃阈值**（`sort_blocks` 的输入是层内块数 = 节点数 + dummy 数）：

| 层宽 | 12 | 15 | **18** | 20 | 24 |
|------|----|----|----|----|----|
| 结果 | ok | ok | **panic** | panic | panic |

```
thread 'main' panicked at .../smallsort.rs:854:
user-provided comparison function does not correctly implement a total order
  at compose/order.rs:347   (sort_blocks)
  at compose/order.rs:379   (reorder_layer)
  at compose/order.rs:92    (order_layers)
```

**为什么至今没暴露**：全部 78 个 fixture 的最大层宽只有 11（`demo.plotgram-core-mod-deps`）。**任何有 18 个以上同层元素（含长边 dummy！）的真实图都会硬崩**——这是很低的门槛，一个 15 节点的图配上 3 条长边就能触发。

修法见 §4.2.1。

### 2.2 【P0】HEAD 上三个门禁是红的

```
plotgram-compile  debug_trace::trace_snapshot                      FAILED
plotgram-layout   hierarchical_coordinates::fanout_port_anchor_coordinates  FAILED
plotgram-layout   hierarchical_coordinates::long_edge_trunk_coordinates     FAILED
```

工作区干净（只有一个未跟踪的 `.snap.new`），所以是最近几个提交（`8e01898` 轴继承 / `b1e10b1` 侧廊 / `2f6c8eb` PortLane）改了几何但没同步快照。最后一个提交的信息是 `2`。

`debug_trace` 的 diff 显示端口锚点从 `x=48.0` 变到 `x=72.70`（`e4` 的 East 端），说明 PortLane 改写确实生效了但没走 review。这条不是算法问题，是**流程问题**：门禁不绿的时候，后续所有「几何有没有变化」的判断都失去基线。

### 2.3 【P1】对称性：树上完美，DAG 上塌陷

对每个 `deg≥2` 的扇，测量「孩子外包围的中点」与「扇心 x」的偏差：

| Fixture | 扇 | 孩子数 | 扇心偏移 | 间距 | 左右轨 y |
|---------|-----|--------|----------|------|----------|
| `symmetric-fanout` | review | 3 | **0.0** | 匀 | 对称 |
| `symmetric-fanout` | notify（汇入） | 3 | **0.0** | 匀 | 对称 |
| `fan-out-four` | hub | 4 | **0.0** | 匀 | 对称 |
| `ticket-triage` | type_gate | 3 | **0.0** | 匀 | 对称 |
| `ticket-triage` | handle（汇入） | 4 | 0.0 | **110 / 51.5 / 58.5** | — |
| `order-approval` | check | 2 | **55.0** | — | **不对称** |
| `order-approval` | approved（汇入） | 2 | **110.0** | — | 不对称 |
| `typical-microservice` | user_svc | 3 | **103.3** | 131 / 161 | 不对称 |
| `typical-microservice` | prometheus（汇入） | 6 | **155.8** | 42/88/24/113/4 | 不对称 |
| `stress.yfiles-pipeline` | n_sink（汇入） | 4 | **221.1** | 74/63/75 | 不对称 |

**分界线非常干净**：只要图是「每个节点最多一个父」的树，`SymmetryPlan` 给出精确对称；一旦出现多父/菱形，偏移立刻到几十上百像素。

**根因（四个结构性缺陷）**，都在 `metric/symmetry.rs::compute_symmetry_plan`：

```252:253:crates/plotgram-layout/src/layout/hierarchical/metric/symmetry.rs
    let mut claimed = vec![false; n];
    let mut fan_claimed = vec![false; n];
```

1. **先到先得的独占认领**。hub 按 `(rank, elem)` 从上往下遍历，第一个 hub 把孩子 `claimed`/`fan_claimed` 之后，后面的 hub 在 `append_fan_slots` 的过滤里就看不到这些孩子了（`symmetry.rs:625-631`）。菱形结构中一个孩子只能属于一把扇 → 另一个父亲拿不到任何居中目标，只能退回 BK ideal。这是**贪心，没有任何全局目标**，结果依赖遍历顺序。

2. **FanPack 写的是绝对 desired**。`axis + k·pitch`，`pitch` 取「相邻宽度需求」与「pass-1 相邻间距中位数」的较大者（`fan_pitch`，`symmetry.rs:582-606`）。这是**单一 pitch**，对不等宽孩子必然不匹配；VPSC 的分离约束随后把它们撑开，于是出现 `110 / 51.5 / 58.5` 这种间距。

3. **对称轴从 pass-1 算，之后不再回代**。`compute_symmetry_plan(plan, graph, &pass1, ...)`（`cross_axis.rs:97`）。pass-2 的分离约束会移动节点，但轴不会跟着更新——**没有不动点，只有一次快照**。

4. **硬等式熔断**。`RigidColumnClass` 成员两两 `harden_equal`（`cross_axis.rs:263-268`），加上 BK 块的 VV/RR 等式。硬等式一多就要靠 `pass1_hardenable_real_pair`、`dummy_aligned`、`class_of`、`fan_desired_for` 四层排除谓词互相躲让（`cross_axis.rs:280-297`）。`roadmap.md` §9 自己写了「若将来需要第三个守卫，不再往约束循环加排除条件」——代码里现在已经是**四个**。

另外两个次要问题：`SymmetryPlan::class_of` / `fan_desired_for`（`symmetry.rs:66-90`）是对全部 class × member 的线性扫描，在 `build_pass2_constraints` 的 `for block / for pair` 里被逐对调用 → O(n²)~O(n³)。

方案见 §4.3。

### 2.4 【P1】性能：O(n^2.85)，Network Simplex 占 72%

**窄层深图**（层宽固定 8，排除定序宽度因素），release：

| 图 | 节点 | 边 | 耗时 |
|----|------|-----|------|
| deep50 | 400 | 737 | 0.28 s |
| deep100 | 800 | 1487 | 1.54 s |
| deep200 | 1600 | 2983 | **11.15 s** |

翻倍规模 → 5.5x / 7.2x 耗时，指数 ≈ **2.85**。yFiles 在这个规模是百毫秒量级。

**采样 profile**（debug 构建，deep100，6370 样本）：

```
4597 (72%)  compose::rank::assign_ranks → refine_component
  3475        └─ rank::cut_value
  2422        └─ rank::TreeShape::in_subtree
1771 (28%)  compose::order::order_layers
  1756        └─ order::transpose_pass
  1749           └─ order::local_crossings
   879              └─ algo::crossing::count_bipartite_crossings
```

两个热点都有**明确的教科书解法**：

- `refine_component`（`rank.rs:120-222`）每次 pivot 都重建整棵 `TreeShape` 并对**每条树边**重算 `cut_value`（每次 O(E)）→ 每 pivot O(V·E)，预算 `8(V+E)+64` 次。模块注释已经承认这是刻意的 MVP 取舍。GKNV 1993 的增量 `low/lim` + 叶到根累加把它降到每 pivot **O(V+E)**。
- `local_crossings`（`order.rs:443-476`）每次相邻交换都**重新过滤整个 `plan.segments`** 构造 segs，再全量数一遍两层交叉。相邻交换的交叉增量有经典 O(deg(u)·deg(v)) 公式。

另有一批 O(E²) 的字符串线性查找散落各处，规模上去后会成为第三梯队热点：

| 位置 | 模式 |
|------|------|
| `ink/route.rs:89-93` | `plan.segments.iter().filter(\|s\| s.edge_id == edge_id)`，每边一次 |
| `compose/track_order.rs:51-55` | `graph.edges.iter().find(\|e\| e.edge_id == edge_id)`，每边每轨一次 |
| `metric/cross_axis.rs:319-324` | `chain_neighbor` 全量扫 `segments`，每边两次 |
| `metric/symmetry.rs:562-570` | `compose_order_key` 用 `layer.iter().position()` |

### 2.5 【P1】Ink 在发明几何（违反 H2「落笔零新决策」）

`ink/route.rs` 里有一整套**避障判定 + 拓扑分支**：

```217:230:crates/plotgram-layout/src/layout/hierarchical/ink/route.rs
/// Clear outward horizontal stub from a cross-axis port to a Main rail.
fn cross_axis_stub_clear(
    plan: &PlanGraph,
    frames: &[Rect],
    rank: usize,
    port: Point,
    rail_x: f64,
) -> bool {
    let Some(frame) = face_frame_at_x(plan, frames, rank, port.x) else {
        return false;
    };
    stub_outward_to_rail(port.x, rail_x, frame)
        && horizontal_clear_at_y(plan, frames, rank, port.x, rail_x, port.y)
}
```

`horizontal_clear_at_y`（`route.rs:183-211`）**扫描全部节点框**判断水平段是否穿框；`leave_to_main` / `arrive_from_main`（`route.rs:234-324`）据此在两种折线拓扑之间二选一：

```269:273:crates/plotgram-layout/src/layout/hierarchical/ink/route.rs
/// Arrive from a Main rail into a port. Final segment follows the port
/// normal when the cross-axis stub is clear (E/W → horizontal at `end.y`).
/// Otherwise fall back to a layer-gap join (may end with a vertical onto
/// the face) so we never pierce siblings just to keep the normal.
fn arrive_from_main(
```

「走哪条走廊、在哪拐弯」按 `architecture.md` §6.1 是 **L2 = Channel search** 的自由度。Ink 现在在 L5 阶段重新做这个决定，而且用的是 Channel 看不见的信息（像素框）。这正是反模式速查表第 1、8 条。

后果不只是纪律问题：Channel 已经按「这条边走 Main 廊」算过代价并占了 lane，Ink 却可能改走「先下到层缝再横移」——**TrackOrder 的 lane 分配与实际几何脱节**，`edge_gap` 的分道语义被悄悄破坏。

**但 Ink 并不是无缘无故越权的**。深读 Channel 后可以确认根因：**Channel 的 Substrate 完全不建模节点占位**。走廊沿 gap 线整条延伸，节点体只是 `ext` 上的奇数格坐标，搜索期**从不做「这条 track 会不会穿过某个节点」的判断**——「不穿节点」这个约束在整个 L2 阶段是不存在的，它第一次出现是在 Ink 的像素级 `verify_no_node_penetration`。

于是形成一个闭合的坏循环：Channel 给出的路径可能穿节点 → Ink 不能照画 → Ink 只好自己扫框、自己改拓扑。**所以 §4.6 不能只是「把判定搬回 Channel」，必须先让 Channel 拿到节点占位信息**（把节点体格标成不可穿越，或给 track 附带被节点切断的 ext 分段）。这也解释了为什么 `metric/track.rs::clear_main_x`（§2.7）会存在——同一个缺失的约束在下游被补了两次，两处实现还不一致。

### 2.6 【P0】组图交叉爆炸：一条完整的因果链

这是本次评审里**唯一一条能从头到尾闭合的因果链**，也是最值得先修的地方。

#### 现象

```
交叉总数 616（78 fixture），组图占绝大部分
最差 5 个：k8s-multi-cluster-federation 87 · k8s-multi-namespace-overview 86
          plotgram-core-mod-deps 68 · k8s-platform-stack 35 · k8s-tenant-isolation 34
平图最差：flat-mesh 35 · layout-stress-yfiles-pipeline 34
```

#### 关键实测：组通道有 77.5% 的时候是**关着的**

对 40 个组 fixture 逐个读 `debug-layout` 的 `extension.channels.status`：

| Substrate 模式 | fixture 数 | 平均交叉 |
|----------------|-----------|---------|
| `d1.3-gate`（组通道生效） | 9 | **1.7** |
| `d1.3-root-scope`（**静默回退**） | **31** | **14.5** |

**交叉数最高的 13 个组 fixture，全部是回退组，无一例外。**

#### 链条

`channel/derive.rs::derive_substrate` 在组矩形不干净时会静默退回 root-scope：

```64:72:crates/plotgram-layout/src/layout/hierarchical/channel/derive.rs
        Err(DeriveError::OverlappingGroups { .. })
        | Err(DeriveError::ForeignNodeInGroupRect { .. })
        | Err(DeriveError::EmptyGroup { .. }) => {
            // Weak-group layouts may not yield nested rectangles yet (no
            // group-frame). Fall back to D1.1 root-scope rather than hard-fail
            // the whole diagram; Gate activates when rects nest cleanly.
            let (s, idx) = derive_root_substrate(plan);
            Ok((s, idx, false))
        }
```

回退之后 `ScopeMask` 退化为「全部允许」、Gate 边全部消失——**边可以任意穿组，Channel 对组一无所知**。而 `used_gates` 只流向 debug trace（`mod.rs:322`），**不产生任何 relaxation、不进 `hier_eval`**，所以这件事在门禁上完全不可见。

那么为什么组矩形不干净？我按 `(rank, order)` 包围盒复算了一遍：

```
demo.k8s-multi-namespace-overview   6 个组，5 个的包围盒混入外来节点
    payment_ns 的 bbox 里混入 order_ns @ (rank 4, order 1)
    order_ns   的 bbox 里混入 payment_ns @ (rank 5, order 1)
product.cloud-native                6 个组，1 个不纯
    k8s 的 bbox 里混入 observability @ (rank 3, order 2)
```

注意：这两张图的**层内组连续性都是满足的**（0 处违反）。问题出在**跨层**——`payment_ns` 在第 4 层占 order 0-1、在第 5 层占 order 2-3，`order_ns` 反过来，两个组在层间「交叉换位」。每层各自连续，但两个组的 `(rank, order)` 包围盒互相咬合 → `ForeignNodeInGroupRect` → 回退。

**这正是块树实现与边界 dummy 实现的本质差别**：块树只约束「每层内同组元素相邻」，**不约束组的 order 区间在相邻层之间对齐**；边界 dummy 在相邻层的同组边界之间连高权重段，恰好提供的就是后者。

#### 完整链条

```
组连续性用递归块树实现（而非 architecture.md §8.3 规定的边界 dummy）
  ├─→ transpose 被 group_path 守卫锁死，块间顺序只靠一次 median            → 交叉本身就偏高
  └─→ 只保证层内连续，不保证跨层 order 区间对齐
        → 组 (rank, order) 包围盒互相咬合、混入外来节点
        → derive_group_substrate 失败
        → 静默回退 root-scope（31/40）
        → ScopeMask 失效、Gate 消失
        → 边任意穿组                                                      → 交叉再翻一倍
```

两条支路都指向同一个修法：**§4.2.2 的边界 dummy**。这也是本报告认为投入产出比最高的一项——它同时解决交叉最小化、组通道启用、以及「组框不是求解变量」（§2.7 的姊妹问题）。

#### 附：transpose 被锁死的具体位置

`architecture.md` §8.3 规定的是「边界 dummy 夹住同组节点 → 不改交叉最小化内核」，但代码用的是**递归块树**（`build_blocks`，`order.rs:199-225`），并且：

```418:424:crates/plotgram-layout/src/layout/hierarchical/compose/order.rs
        for r in 0..plan.layers.len() {
            let mut i = 0;
            while i + 1 < plan.layers[r].len() {
                let (u, v) = (plan.layers[r][i], plan.layers[r][i + 1]);
                if plan.elems[u].group_path != plan.elems[v].group_path {
                    i += 1;
                    continue;
                }
```

**transpose 只在 `group_path` 完全相同的相邻叶之间交换**。这意味着：
- 组内节点 vs 组外节点：永不交换；
- 不同组的兄弟：永不交换；
- **块与块之间的整体顺序只能靠 `sort_blocks` 的中位数排一次，没有任何局部搜索**。

于是组图基本只跑了「median sweep」，丢掉了 transpose 这一半的收益——而 transpose 恰恰是 Sugiyama 配方里性价比最高的部分。

### 2.7 【P2】`clear_main_x`：事后把竖廊挪开

```183:211:crates/plotgram-layout/src/layout/hierarchical/metric/track.rs
fn clear_main_x(x: f64, obstacles: &[(f64, f64, f64, f64)], edge_gap: f64) -> f64 {
    ...
    let mut intervals: Vec<(f64, f64)> = obstacles.iter().map(|&(l, _, r, _)| (l, r)).collect();
    ...
    for (l, r) in intervals {
        if let Some(last) = merged.last_mut() {
            // Merge if overlapping or closer than 2*margin (no room for a lane).
            if l <= last.1 + 2.0 * margin {
                last.1 = last.1.max(r);
```

三个问题：

1. **障碍是全图 X 投影**（`obstacles` 收集了所有层的所有节点，`track.rs:38-53`），完全丢掉了 y 信息。一条只跨两层的竖廊被当成要躲开全图所有列。
2. **2·margin 合并会级联**。任何两列的 X 区间只要靠得比 `edge_gap` 近就并成一块；在密图上整幅画会并成**一个**区间，竖廊被推到画布最左/最右。`hier_eval` 里那条 `check→approved stays near left leaf (not canvas x≈0)` 的断言就是在按住这个症状。
3. **竖廊 X 不是 VPSC 变量**。节点位置解完之后才给竖廊找缝，节点从来不会为侧廊让路。yFiles 的做法是把边路由的空间需求以 `minimumDistance` 形式参与节点放置。本仓已经有 `DemandBoard`——但它目前**只发布 `LayerGap`**（`demand.rs:12-16`，只有一个 key），缺 `NodeGap` / `OuterMargin`。

### 2.8 【P2】InkVerifier 覆盖面太窄

```37:64:crates/plotgram-layout/src/layout/hierarchical/ink/verify.rs
pub fn verify_no_illegal_overlap(
    edges: &[CanonicalEdge],
    bundles: &[BundlePlan],
) -> Result<(), LayoutError> {
    ...
            if !polylines_equal(a_pts, b_pts) {
                continue;
            }
```

只检测**首尾完全相同、点数完全相同**的两条折线。两条边共享一段 200px 的重合竖线但起终点不同 → 检测不到。而 `expectations.md` §3「多边不错叠」要求的正是这个。

`architecture.md` §11 列出的 InkVerifier 最低断言里，以下几条**没有实现**：

| 断言 | 状态 |
|------|------|
| path 首尾等于 Metric port point | 只在 `hier_eval` 里近似查（容差 1.0px） |
| orthogonal 轴对齐 | 只在 `hier_eval` 查，`ink/verify` 里没有 |
| 不穿非端点节点 | ✅ 已实现 |
| 穿组只经 gate | ❌ 未实现（D₂） |
| 非 bundle 边不完全重合 | ⚠️ 只查完全相同 |
| 圆角最小段长 | ❌ |

另外 `verify_no_illegal_overlap` 是 O(E²) 且每条边都 `clone` 一次点列（`polyline_samples` → `pts.clone()`）。

**更要紧的是自环完全绕过了校验**：

```252:262:crates/plotgram-layout/src/layout/hierarchical/mod.rs
    ink::verify::verify_no_illegal_overlap(&canonical_edges, &route_plan.bundles)?;
    let real_frames: Vec<(String, Rect)> = real_graph
        .ids
        ...
    ink::verify::verify_no_node_penetration(&canonical_edges, &real_frames)?;
    canonical_edges.extend(ink::selfloop::self_loop_edges(
```

`extend` 在两次 `verify` **之后**。自环几何（`ink/selfloop.rs`）本身就是整套硬编码——固定 East 侧、锚点写死 `0.3h`/`0.7h`、外扩 `24 + 10·idx`——既不经 PortPlan、不经 Channel、不经 Metric，还不接受任何校验。多个自环之间、自环与普通边之间是否重叠碰撞，当前**没有任何机制保证**。

### 2.9 【P2】其它

| 项 | 位置 | 说明 |
|----|------|------|
| **层内顶对齐** | `metric/main_axis.rs:18-34` | `top[e] = cursor`，同层不等高节点是**顶对齐**。yFiles 默认 `layerAlignment = 0.5`（居中）。当前 fixture 全部等高所以没暴露；标签换行一多就会出现「同层节点上边缘齐、下边缘参差」，南面出线 y 不一致 → 平白多折点 |
| **rank 缺 `min_span` / `weight`** | `rank.rs:15` `const MIN_SPAN: i64 = 1;` | 全图统一跨度 1、权重 1。yFiles 有 `minimumLayerDistance`、边权、层约束（same-layer / above-below）。`architecture.md` §9.1 的 `HierarchicalLayoutData { edge: { min_span?, weight? } }` 尚未接线 |
| **`normalize_dense` 全局压缩** | `rank.rs:425-434` | 对全图 rank 取 distinct 后压缩。跨弱连通分量共享压缩表，某分量的空层会被别的分量填掉；NS pivot 后若出现空 rank，长边会被静默压成短边（可行性不破，语义变了） |
| **Channel 严格字典序代价** | `channel/search.rs:98-135` | `bends ≻ length ≻ span_affinity ≻ congestion`。**折点是绝对优先**，长度/拥塞永远换不动一个折点 → 为省一个弯可以绕很远；`congestion` 排最后，实际上几乎不起作用。代价里**完全没有交叉项**。这解释了统计上的怪相：`max_bends` 在 38 个 fixture 上齐刷刷卡在 4，而交叉数很高 |
| **端口落在 bbox 而非形状轮廓** | `metric/anchor.rs:29-48` | `side_point` 按矩形四边取点。菱形（decision）节点上，三路扇出的端口落在 bbox 南边，视觉上线段起点悬在菱形斜边外的空白里。`RealGraph::shapes` 已经存了 `NodeShape`（`model.rs:46`）但 anchor 没消费 |
| **死代码 / 未用告警** | `channel/mod.rs:15`、`channel/derive.rs:82`、`channel/substrate.rs:61,99` | `derive_root_substrate` 未用导入、`required_orient` / `covers_slot` 从未调用。substrate 抽象有一半没接上 |
| **`compute` 是 300 行直筒函数** | `mod.rs:61-360` | 15 个阶段全部内联，中间变量 `prelim_gaps`/`main_prelim`/`prelim_frames`/`cross`/`main` 交织。加一个相位就要在正确位置插一段，容易插错顺序 |

### 2.10 【P2】Channel：Gate 容量名存实亡，rip-up 与设计漂移

**Gate 容量从未启用**。`GateCapacity::Fixed(c)` 在 `graph.rs:98` 有判定分支，但全仓库唯一的产生点是：

```274:274:crates/plotgram-layout/src/layout/hierarchical/channel/derive.rs
    let capacity = GateCapacity::Unbounded;
```

于是 `Occupancy::gate_open` 恒为 `true`，`route_edge` 里那段「Gate 满则跳过」的代码是**死分支**。组边界预算这个概念目前只是 IR 上的占位。

**走廊共享只有软拥塞，没有硬隔离**。同一条 L2 track 上可以 commit 任意多条边，`lane_demand` 只加进 `LexCost.congestion`——而 congestion 在严格字典序里排最后（§2.9），实际影响接近于零。真正把它们分开的是 L3 的 `TrackOrder` 区间着色。也就是说 **L2 拓扑层根本不知道自己在制造重叠，全靠 L3 事后分道**；一旦某条走廊分道后仍撑不开（Demand 只作用于 Cross 层缝，Main 竖廊不发布任何 Demand），就直接体现为视觉重叠。

**rip-up 与 `ports-and-channel.md` §8 不一致**：

| 设计 | 实现 |
|------|------|
| 触发：搜索失败 / track 偏序环 / 容量超限 | 只有 `peak occupancy > 1` |
| 初始 commit 失败时 rip-up 救场 | **直接 `Err` 硬失败**（`route_all.rs:335-339`） |
| 牺牲序 `(failure_count desc, priority, decl, id)` | `(critical asc, span asc, failure_count desc, ...)` |
| `history_cost`（多轮学习） | **未实现** |

预算 `4 轮 × 8 边`；超预算记 `channel-rip-up-budget` relaxation 但仍返回当前解。实测 78 个 fixture **没有产生过任何 relaxation**——rip-up 这条路径在现有语料上完全没被走过，等于没有测试覆盖。

**`verify_no_group_penetration`（`architecture.md` L6 第三道防线）在代码库中不存在。**

### 2.11 【P2】Ink / Demand：静默默认与坐标脱节

**`lane_coord` 静默选 lane 0**：

```136:137:crates/plotgram-layout/src/layout/hierarchical/ink/route.rs
    let hop = track_order.assignments.get(&(edge_id.to_string(), tid));
    let idx = hop.map(|h| h.track_index).unwrap_or(0);
```

TrackOrder 是 `track_index` 的唯一写者。查不到分配说明上游有 bug，此处应 `InternalInvariant` 硬失败；`unwrap_or(0)` 会把上游缺失变成**两条边悄悄挤同一条 lane**，而且因为 §2.8 的 verifier 只查完全相同折线，这个错误逃得掉。

**Curved 控制点是 Ink 发明的**（`route.rs:646`）：`d = clamp(|Δy|/3, 24.0, layer_gap + node_gap)`。`24.0` 和 `/3.0` 是纯魔法数，且曲线形状属于几何自由度，写者应是 Metric 或独立的 style writer。

**`layer_gap_y` 与 Demand 脱节**：Ink 用 `0.5 * (above + below)` 从**最终 frames** 反推层缝中线（`route.rs:489-517`），而 Metric 是用 `resolved_layer_gaps` 撑开层缝的。当某条层缝被 TrackOrder demand 撑大之后，Ink 反推出的「中线」和 Metric 实际分给各 lane 的 Y **不是同一套坐标**，逃生段可能正好落在某条 lane 上。

**`DemandBoard` 只实现了一个 key**。`demand.rs` 里 `DemandKey` 实质只有 `LayerGap(u32)`，且只有 Cross 内层走廊发布。`coordinate-and-demand.md` 规划的 label band、port stub、self-loop 跨度、`NodeMinSize`、`GroupMinSize`、`PartitionBand` 全部没有生产者。Main 竖廊的空间需求（§2.7 的 `clear_main_x` 想解决的问题）本应走 `NodeGap` demand，现在只能事后挪线。

**`publish` 静默丢弃非有限值 / 负值**（`demand.rs:40-42`）——同 `lane_coord`，坏值应该是 invariant error 而不是 no-op。

### 2.12 【P2】Ports / TrackOrder

**`along` 是两阶段双写者**。Compose `assign_ports` 写 `AlongSpec::Ordered{order, count}`（相对序），Metric `apply_port_lanes` 在**触及脸**上把同一字段覆写成 `AlongSpec::LocalOffset`（绝对像素）。`port-lanes.md` §2 显式裁定了这个分工（「相对序」与「绝对列位」是两个自由度），所以不算违规——但它在写权尺子上是个需要持续盯住的灰区：同一个节点可能同时存在 `LocalOffset` 的脸和 `Ordered` 的脸，两套语义并存。建议在 `write-authority.md` 里把这条例外写死，并考虑把 `AlongSpec` 拆成两个字段（`ordinal` + `resolved_offset: Option<Point>`），让类型系统表达「谁写哪一半」。

**多 hub 共享同一条 Cross 走廊时没有源级分区**。`fan_nest`（`track_order.rs:185-206`）只读该边自己端口的 `Ordered{order, count}`，**不看 hub 是谁**。两个不同 hub 的扇出汇入同一条层缝时，它们的边按 `nest → order → span → edge_id` 混排着色，源与源之间的嵌套关系完全不受控。这很可能是 §2.3 里 `prometheus` 六路汇入间距 `42/88/24/113/4` 的一部分成因。修法：给排序键前置一个 hub 标识（`(hub_elem, nest, order, ...)`），让同源的边先聚成一段再按外叶优先展开。

**Cross 着色不是全局最优**。`color_cross_outer_first` 是「nest 排序 + first-fit」，而 Main 用的 `color_intervals` 是区间图上的最优着色。Cross 侧为了扇出嵌套牺牲了最优性——这是有意的权衡，但应该在 `hier_eval` 里记录 Cross 轨道数与下界的差距，否则退化不可见。

**其它**：

- `assign_ports` 227 行单函数；参数 `_canonical_size` **从未使用**（接口与实现漂移）
- `pick_reversed_side` 的软代价权重 `0/1/2` 与 `NS_LOAD_SPINE_THRESHOLD = 2` 无任何推导记录，建议表驱动化并直接链到 `expectations.md` §6 的裁定
- `track_order.rs:97-104` 的 `if (hi-lo).abs() < JOG_EPS { A } else { A }` **两个分支完全相同**，是残留的死代码
- `undirected_pair` 在 `ports.rs:85-91` 与 `port_lane.rs:358-364` 各写了一遍

---

## 3. 与 yFiles 的能力差距（对照表）

| 能力 | yFiles | 本实现 | 差距 |
|------|--------|--------|------|
| 去环 | Greedy-FAS + 用户指定 | Greedy-FAS + 环 reroot | ✅ 对齐（reroot 是加分项） |
| 分层 | Network Simplex + `minimumLayerDistance` + 层约束 | 简化 NS，均匀 span/weight | 缺 min_span / weight / 层约束；实现是非增量版 |
| 定序 | median + transpose + 多起点重启 | median + transpose + best snapshot | **transpose 被组块锁死**；无多起点重启 |
| 交叉计数 | BJM 累加树 | BJM（`plotgram_algo::crossing`） | ✅ |
| 坐标 | 分段线性 / 简化 BK + 优先级压紧 | BK ideal + 两趟 VPSC + 对称表 | 无全局目标函数；对称靠贪心表 |
| 层内对齐 | `layerAlignment` 0/0.5/1 | 固定顶对齐 | 缺参数 |
| 边路由 | 正交 + 通道 + 分组 + 交叉后处理 | 字典序 Dijkstra + 区间着色 | 代价无交叉项；拥塞项形同虚设 |
| 边分组 | `automaticEdgeGrouping` + 端口分组 | `auto_edge_grouping`（bus） | ✅ 有；但只在端总线 |
| 回边处理 | back-loop routing，侧廊 | 侧廊（East/West） | ✅ 有 |
| 组/子图 | 递归布局 + 组框进求解 | 块树连续性，组框后验 bbox | **组框不是求解变量**；Gate/Scope 在 31/40 组图上静默失效 |
| 泳道 | `PartitionGrid` | 未消费 | 后置 |
| 标签 | integrated labeling | 无 | 后置 |
| 增量 / from-sketch | 支持 | 无 | 后置 |
| 规模 | 万级节点秒级 | 1600 节点 11s，≥18 宽层崩溃 | **两个数量级** |

---

## 4. 改进方案

### 4.0 总纲：把「声明表 + 硬等式」换成「目标函数 + 迭代求解」

现在的次轴内核形态是：

```
BK ideal → VPSC(pass1) → 贪心扫描产出 {轴, 刚体列, FanPack} → VPSC(pass2, 硬等式 + 绝对 desired)
```

它的**根本问题不是哪个谓词写错了，而是它没有可以被最小化的量**。每次发现一张图不好看，唯一的修法就是往成员表的谓词上再加一个条件（现在已经有 `claimed` / `fan_claimed` / `dummy_aligned` / `twin` / `primary arm` 五层）。`roadmap.md` §9 早就预警过这个熔断点。

建议的形态是：

```
BK ideal（保留，作为初值）
  → 迭代 K 轮：desired = 邻居加权中位数（含对称项） ；x = VPSC(desired, 只有分离约束)
  → 每轮算目标函数 J(x)，保留最优快照
```

**关键转变**：对称从「硬等式 + 绝对槽位」变成「软 desired + 分离约束」。这不是放弃精度——§4.3.3 会说明，在纯扇场景下这个形式的**不动点恰好是精确对称**；而在 DAG 上它给出的是全局折中而不是「第一个 hub 赢」。

这个改动同时**不违反写权纪律**：`CoordWriter` 仍然是次轴唯一写者，变的是它内部的算法，不是写者归属。

---

### 4.1 P2 分层：Network Simplex 工程化

#### 4.1.1 增量 cut value（性能，72% → 目标 <5%）

替换 `rank.rs:120-222` 的「每 pivot 重建树 + 全量 cut_value」。标准做法（GKNV §2.3）：

1. 一次 DFS 给每个节点标 `(low, lim)`：`lim` 是后序编号，`low` 是子树内最小 `lim`。判定 `v ∈ subtree(u)` 变成 `low[u] ≤ lim[v] ≤ lim[u]`（当前 `TreeShape::in_subtree` 已经是这个思路，问题是**每 pivot 重建**）。
2. 初始 cut value 用「叶到根」一次累加求出：按 `lim` 升序处理，节点 `v` 的父边 cut value = `Σ(v 的非树关联边符号权重) − Σ(v 的子树内已算出的子边 cut value)`。总代价 O(V+E)。
3. pivot 之后**只更新 leave→enter 路径上的树边** cut value（路径长度 ≤ 树直径），其余不变；`(low, lim)` 只需对受影响子树重编号。

伪码：

```text
fn refine_component(comp, rank):
    tree = build_tight_tree(comp, rank)         # 不变
    (low, lim, parent_edge) = postorder(tree)
    cut = init_cut_values(tree, low, lim)       # O(V+E) 一次

    loop (budget = 4 * |comp|):
        e_leave = argmin{ cut[e] : cut[e] < 0 }         # 用小顶堆或线性扫（V 很小）
        if none: break
        (tail_side, head_side) = split(tree, e_leave)
        e_enter = min_slack_edge(head_side → tail_side)
        if none: break
        delta = slack(e_enter)
        shift(rank, tail_side, -delta)                   # 只动一侧
        exchange(tree, e_leave, e_enter)
        update_cut_values_on_path(cut, e_leave, e_enter) # O(path)
        relabel_lim(tree, affected_subtree)              # O(affected)
    normalize(rank)
    balance(rank)                                        # 见 4.1.3
```

**期望效果**：`refine_component` 从 O(pivots · V · E) 降到 O(V+E + pivots · path)。deep200 的 11.15s 应回到 1s 量级。

同时删掉 `rank.rs:214-219` 那个「新长度变大就 break」的防御性守卫——增量实现下 pivot 单调不增是可证的，守卫只会掩盖 bug。

#### 4.1.2 接线 `min_span` / `weight`

`MIN_SPAN` 从常量改成 `min_span(edge)`；`total_weighted_length` 的 `Σ(rank[t]−rank[s])` 改成 `Σ w_e·(rank[t]−rank[s])`；`cut_value` 的 `±1` 改成 `±w_e`。数据来源用 `architecture.md` §9.1 已经规划好的 `HierarchicalLayoutData.edge{ min_span, weight }`。

顺带把 properify 后 dummy 链的隐含权重表达出来：yFiles/dot 在**第二遍**分层时给长边加权，本实现的 properify 在分层之后，无法反馈。至少可以先支持作者显式的 `min_span`。

#### 4.1.3 balance pass

NS 之后加一遍 dot 的 `balance`：对入度 = 出度且 rank 有 slack 的节点，在可行区间内挪到**该区间内节点数最少的层**。这直接改善「一层一排」的均衡度，代价 O(V+E)。

#### 4.1.4 `normalize_dense` 按分量归一

改成对每个弱连通分量分别取 min 归零、**不做 distinct 压缩**（保留空 rank），避免跨分量污染与长边被静默压短。

---

### 4.2 P3 定序：修全序、放开 transpose、增量交叉

#### 4.2.1 修 `cmp_key`（P0，先做这个）

原则：**把浮点键量化成整数键，一次性排完序**，不要在比较器里做 ε 判断。

```rust
/// 排序键：全部量化为可全序比较的整数元组。
/// QUANT 把 [0, width) 的位置量化到 1/1024 格，吸收浮点噪声但保持传递性。
const QUANT: f64 = 1024.0;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct SortKey {
    /// None 排在最后（无邻居的块不参与中位数排序，保持在原位区）。
    median: Option<i64>,
    barycenter: Option<i64>,
    prev_pos: usize,
    repr_decl: usize,
}

fn sort_key(k: &Key) -> SortKey {
    let q = |v: Option<f64>| v.map(|x| (x * QUANT).round() as i64);
    SortKey {
        median: q(k.median),
        barycenter: q(k.barycenter),
        prev_pos: k.prev_pos,
        repr_decl: k.repr_decl,
    }
}

// 调用点：keyed.sort_by_key(|(k, _)| sort_key(k));
```

`Option<i64>` 的 `Ord` 是 `None < Some(_)`，需要「无邻居排后面」的话用 `Reverse` 或改成 `(k.median.is_none(), q(k.median))`。要点有三：

1. **量化取代 ε**：`round()` 之后相等就是真相等，传递性由 `i64` 的全序保证。
2. **`None` 有确定位置**，不再「跳过一层再比」。
3. **用 `sort_by_key` 而不是 `sort_by`**，让类型系统保证全序。

同时把 `median_of`（`order.rs:279-300`）里的 `positions.sort_by(|a,b| a.partial_cmp(b).unwrap())` 换成 `sort_by(f64::total_cmp)`（`bk.rs:81,256` 同理）——虽然当前不会有 NaN，但 `total_cmp` 零成本且永不 panic。

**必须补的门禁**（这是本次评审最重要的一条测试建议）：

```rust
#[test]
fn ordering_survives_wide_layers() {
    // 层宽 64，制造大量 median 平局与 None 键
    for width in [18, 32, 64] { ... order_layers(...) ... }
}
```
外加一个 fixture：`stress.wide-layer.pgm`，单层 ≥ 32 节点。**当前 78 个 fixture 的最大层宽只有 11，语料本身覆盖不到这个维度。**

#### 4.2.2 用边界 dummy 取代块树，放开 transpose

按 `architecture.md` §8.3 的原定方案：properify 之后，为每个「组 × 层」插入一对**零宽边界 dummy**（`GroupBoundary { group, layer, side }`），并在相邻层的同组边界 dummy 之间连一条高权重（如 16.0）虚拟段。

四项好处，**其中第三项是本报告最看重的**：

1. 层内变成**扁平序列**，`transpose_pass` 的 `group_path` 特判整段删掉，任意相邻元素都可交换；
2. 组连续性由「边界 dummy 之间的高权重段 + 分离约束」自然维持，交叉最小化内核**完全不用知道组的存在**；
3. **高权重的跨层段会把同一个组在相邻层的 order 区间拉齐**——这正是 §2.6 里 `derive_group_substrate` 失败的直接原因（组在层间「交叉换位」导致 `(rank, order)` 包围盒互咬）。区间对齐之后包围盒自然变纯，**Channel 的组通道从 9/40 提升到接近全覆盖**，`ScopeMask` 与 Gate 才真正开始工作；
4. Metric 侧组框直接由边界 dummy 的坐标给出，组框成为 VPSC 变量而不是事后 bbox（§2.7 的姊妹问题）。

配套必须做的一件事：**让回退可观测**。`derive_substrate` 落到 root-scope 时发一条 relaxation（带上失败原因 `OverlappingGroups` / `ForeignNodeInGroupRect` / `EmptyGroup`），并把 `used_gates` 记进 `hier_eval`。否则这项改动做完了也无法证明它起了作用——**现在这件事在门禁上是完全隐形的**。

若认为边界 dummy 改动太大，**最小可行替代**是给块树加一层 transpose：在 `sort_blocks` 之后，对同一父块下的**相邻兄弟块**尝试整体交换，接受严格减少交叉的交换。这能拿回交叉那一半的收益（约 40 行），但**拿不回组通道那一半**——包围盒对齐问题依然存在。

#### 4.2.3 增量交叉 + 平台逃逸

- `local_crossings` 改成**只算受影响的两条边集合**：交换相邻的 `u, v` 时，交叉变化只与 `u`、`v` 各自的上下邻居有关，`Δ = crossings(v,u) − crossings(u,v)`，代价 O(deg(u)·deg(v))。
- 预计算 `segs_by_layer: Vec<Vec<(usize,usize)>>` 一次，避免每次 `plan.segments.iter().filter(...)`。
- transpose 增加**等值交换**（`Δ == 0` 也接受）并交替扫描方向，用于逃出平台；靠 best-snapshot 兜底不会变差。
- `tighten_one_to_one`（`order.rs:120-174`）目前每对都调用一次 `total_crossings`（全图！）→ 也改成增量。

---

### 4.3 P4 次轴与对称：内核重写（本方案的重点）

#### 4.3.1 目标：定义可最小化的量

```text
J(x) = Σ_{segments (u,v)}  w_uv · |x_u − x_v|          # 边直度（w = 1 / 2 / 8，沿用 order.rs 的边权）
     + λ_sym · Σ_{hub h}   |x_h − center_h(x)|         # 对称项
```

其中 `center_h(x)`：
- 下扇（`down_deg ≥ 2`）：孩子集合的**几何中点**（外包围中点）与**中位数**的均值；
- 上扇同理；两侧都有则取二者均值。

约束：层内分离 `x_{i+1} − x_i ≥ (w_i + w_{i+1})/2 + node_gap`（现有 `layer_separation` 不变）。

`λ_sym` 是 typed param（默认 1.0），不是魔法常数——满足「参数能 bind 就必须被消费」。

#### 4.3.2 求解：迭代 median + VPSC

```text
fn assign_cross_axis(plan, graph, ports, size_of, main, params) -> Vec<f64>:
    x = bk_ideal(plan, size_of, node_gap)            # 保留 BK 四候选合并作为初值
    hard = layer_separation(plan)                    # 唯一的硬约束集合
        + vv_chain_equalities(bk.primary_blocks)     # 仅 dummy-dummy：走廊确实是刚体

    best = x; best_j = J(x)
    for k in 0..K:                                   # K = 8，确定性预算
        d = vec![0.0; n]
        for e in 0..n:
            d[e] = weighted_median(neighbors(e), x, edge_weight)   # 上下邻居混合
        for h in hubs:                                # 对称项：hub 拉向孩子中心
            d[h] = lerp(d[h], center_h(x), λ_sym / (1 + λ_sym))
        for e in port_anchor_dummies:                 # 端口锚点（保留现有逻辑）
            d[e] = port_anchor(frame_of(owner(e)), port).x
        x = vpsc_solve(desired = d, weights = w, constraints = hard)
        j = J(x)
        if j < best_j - EPS: best = x; best_j = j
    return best
```

要点逐条：

- **只有两类硬约束**：层内分离（永远可行）+ dummy 链共线（同层唯一元素，不可能与分离冲突）。`InfeasibleConstraint` 的风险从「可能」降到「不可能」，`cross_axis.rs:164-166` 那个 `map_err` 变成纯防御。
- **`weighted_median` 用上下邻居合并**（不是像 BK 那样只看一侧），并按 `edge_weight(real-real=1, real-virt=2, virt-virt=8)` 加权。这让 dummy 链天然比实节点更「想直」，`VIRTUAL_WEIGHT = 4.0` 那个 VPSC 权重可以保留作为二阶调节。
- **多父节点自然平衡**：它的 desired 是**全部**父/子的加权中位数，不属于任何一把扇。§2.3 表里 `user_svc`（偏 103px）、`prometheus`（偏 156px）、`n_sink`（偏 221px）这类偏移的直接成因（被别的 hub 先认领走）消失。
- **确定性**：迭代轮数固定、`weighted_median` 平局按 `(rank, order, elem)` 破、VPSC 本身确定 → 逐位可复现，`deterministic_bit_identical_reruns` 测试原样保留。
- **`best snapshot`**：与 P3 的 `order_layers` 同构，行为可预期。

#### 4.3.3 为什么这样仍能给出「精确」对称

考察一把孤立的扇：hub `h`，孩子 `c_1..c_m` 等宽 `w`，孩子除 `h` 外无其它邻居。

- 每个 `c_i` 的邻居只有 `h` → `d[c_i] = x_h`（全部相同）。
- VPSC 求解 `min Σ w·(x_i − x_h)²  s.t.  x_{i+1} − x_i ≥ g`。等权、等间隙下，最优解是把这条链**整体居中于 `x_h`**：`x_{c_i} = x_h + (i − (m−1)/2)·g`。
- 这**正是** `slot_multipliers(m)`（`symmetry.rs:573-579`）现在硬写出来的那组系数 `i − (n−1)/2`。
- 反过来 `d[h] = median(x_{c_i}) = x_h` → 不动点。奇扇、偶扇都成立。

也就是说：**FanPack 现在用一张表硬写的东西，正是这个 QP 的解析解**。用求解器代替表，在纯扇上结果完全相同，在混合场景下才开始有区别——而那正是现在出问题的地方。孩子不等宽时，VPSC 给出的是「在宽度允许下尽量匀称」，这比统一 `pitch` 撑出 `110/51.5/58.5` 更符合人眼预期。

#### 4.3.4 「主臂占脊」「twin 占脊」怎么保留

这两条现在是 `unique_min_span_primary`（`symmetry.rs:181-218`）和 `twin_cands`（`symmetry.rs:318-343`）两段特判。改成**权重**：

```text
fn desired_weight(u, v) -> f64:
    base = edge_weight(u, v)                    # 1 / 2 / 8
    × if is_twin_pair(u, v)      { TWIN_SPINE_BOOST   }   # 默认 8.0
    × if is_unique_min_span(u, v){ PRIMARY_ARM_BOOST  }   # 默认 4.0
    × if is_critical(edge)       { 2.0 }                  # 已有语义
```

权重高的邻居在 `weighted_median` 里主导 hub 的 desired，也在 VPSC 里更强地把两端拉到同一列 —— 「占脊」自然浮现，不需要成员表。这同时满足 ADR-001「差异只经参数展开」：`TWIN_SPINE_BOOST` / `PRIMARY_ARM_BOOST` 是 typed params，可调可门禁，不是 `if` 分支。

作者显式约束（`from_side` / `to_side` / 未来的 `alignment_sets`）继续走**硬约束**通道——「作者胜」的裁定不变。

#### 4.3.5 删除清单

| 删除 | 行数 | 理由 |
|------|------|------|
| `metric/symmetry.rs` 的 `RigidColumnClass` / `FanPack` / `claimed` / `fan_claimed` / `append_fan_slots` / `append_leaf_followers` / `walk_chain` / `unique_min_span_primary` / `inherited_axis_coord` | ~450 | 被目标函数取代 |
| `cross_axis.rs::pass1_hardenable_real_pair` / `dummy_aligned_reals` / `exteriorize_dummy_desired` / 双趟结构 | ~120 | 只有一趟迭代，无需 pass1/pass2 分治 |
| `SymmetryPlan::class_of` / `fan_desired_for` 线性扫描 | — | O(n²) 消失 |

保留：`forward_real_adjacency`、`twin_real_pairs`、`axis_from_neighbors`（改名为 `fan_center`）、`bk.rs` 全部。

#### 4.3.6 分步落地（每步独立可门禁）

| 步 | 内容 | 验收 |
|----|------|------|
| **S1** | 加度量：`symmetry_deviation`（§5）进 `hier_eval`，记录当前基线 | 只加观测，几何零变化 |
| **S2** | 实现 `J(x)` 与迭代求解器，**旁路运行**并输出对比 JSON，不改产出 | 两套结果 diff 可视 |
| **S3** | 切换主路径；`λ_sym` / boost 调到 78 个 fixture 的 `symmetry_deviation` 与 `sum_bends` 均不劣化 | D2/D3 现有断言全绿 |
| **S4** | 删除 §4.3.5 清单 | 行数下降；测试不变 |

S2 这一步是关键：**先并行跑再切换**，避免「改一半、两套语义并存」——这正是 `expectations.md` §6 冲突表里最忌讳的状态。

---

### 4.4 主轴：`layer_alignment` 参数

`main_axis.rs` 增参数（默认 `0.5`）：

```rust
pub fn assign_main_axis(plan, size_of, layer_gaps, layer_alignment: f64) -> Vec<f64> {
    // thickness = max height；每个 elem 的 top:
    // top[e] = cursor + (thickness - h_e) * layer_alignment
    // dummy（h = 0）落在 cursor + thickness * layer_alignment（层带中心）
}
```

dummy 落到层带中心而不是顶端，长边的横向 jog 也会跟着居中，视觉上更整齐。对现有等高 fixture 零影响（`thickness − h_e = 0`），可以安全先做。

---

### 4.5 Channel：加权代价 + 交叉感知 + 真 rip-up

#### 4.5.1 代价从字典序改为加权标量

```rust
// 现在：bends ≻ length ≻ span_affinity ≻ congestion（严格字典序）
// 建议：
struct Cost {
    value: f64,      // w_bend*bends + w_len*length + w_span*span + w_cong*cong + w_cross*crossings
    tiebreak: (u32, u64, u32),  // (bends, length_bits, track_id) 保证确定性
}
```

默认权重建议 `w_bend = 10·edge_gap`（一个折点约等于绕 10 个轨距）、`w_len = 1`、`w_cross = 3·edge_gap`。这样：
- 绕行超过 10 个轨距去省一个折点**不再发生**（当前会）；
- 拥塞与交叉真正参与决策（当前 `congestion` 排在字典序最后，实际不起作用）。

`tiebreak` 保留原字典序作为**平局破解**，确定性不丢。

#### 4.5.2 交叉项

在 `Occupancy` 上记录每条 track 已占用的区间端点，扩展一跳时统计新区间与已有区间的**穿越数**，计入 `w_cross`。这是 `expectations.md` §3「多边不错叠」在**搜索期**的对应物；现在只有 TrackOrder 在**分道期**处理，搜索完全看不见交叉。

#### 4.5.3 rip-up 可观测

`LayoutDiagnostics.relaxations` 通道已经建好（`mod.rs:143`），但 §2 的 78 个 fixture 里没有任何 relaxation 产出。补：每次 rip-up 重布、每次 outer-overflow 惩罚生效，都发一条 relaxation，并在 `hier_eval` 里作为观测量记录。看不见就调不动。

---

### 4.6 Ink：把避障决策上提到 Channel

**前置条件（不可跳过）：先让 Channel 知道节点在哪。** 如 §2.5 所述，Substrate 当前完全不建模节点占位，所以单纯把判定代码搬回 Channel 是搬不动的。需要先做：

```text
Substrate 构造时，把被节点体占用的格子从 track 的 ext 中切掉：
  Main track (line = og) 上，若第 og-1 与 og 列之间实际没有间隙（相邻节点贴合），
    则该段 ext 不可用；
  Cross track (line = k) 天然位于层缝中，除非某节点超高侵入 —— 由 Metric 的
    LayerGap demand 保证不会发生。

这一步复用 derive.rs 已有的 cut_line 机制（现在只按 group 边界切，扩展为按
「节点占位」也切），不需要新抽象。
```

有了这个之后，Channel 搜出来的路径**天然不穿节点**，Ink 才可能真正做到零决策，同时 `metric/track.rs::clear_main_x`（§2.7）也可以整段删除——竖廊的 X 直接由 substrate 切分结果决定，不需要事后找缝。

在此基础上，**让 Channel 的路径显式包含逃逸段**：

```text
现在：ChannelPath { tracks: [T1, T2, ...] }
      Ink 自己决定「从 E 端口横着出去」还是「先下到层缝再横移」

改成：ChannelPath { tracks: [...], escape: EscapePlan { source: AtPortNormal | ViaGap(gap_line),
                                                         target: AtPortNormal | ViaGap(gap_line) } }
```

`EscapePlan` 由 Channel 在搜索时决定（它已经在 substrate 上知道端口所在的 slot 与相邻 track 的占用），Ink 只做 `match`。`horizontal_clear_at_y` / `cross_axis_stub_clear` / `face_frame_at_x` / `stub_outward_to_rail`（`route.rs:146-230`，约 90 行）整段从 Ink 删除。

配套：`compose/verify.rs::verify_routes_connected` 增加「每条边的 `escape` 与两端 `PortPlan.side` 相容」的断言，把「Ink 不需要猜」变成 Plan 期可验证的不变量。

---

### 4.7 Verifier 补强

| 新增 | 内容 |
|------|------|
| **段级重叠** | 把 `verify_no_illegal_overlap` 从「整条折线相同」改成「任意两条边存在长度 > `edge_gap/2` 的共线重合段」，非 bundle 即 FAIL。实现：把所有段按 (方向, 常量坐标) 分桶，桶内做区间求交，O(E log E) |
| **正交断言进 Ink** | `hier_eval` 里的「每段轴对齐」搬进 `ink/verify.rs`（`orthogonal` 风格时硬 FAIL），不要只在集成测试里查 |
| **端点精确落界** | `hier_eval` 现在容差 1.0px（`on_boundary`）。Ink 里应该是**精确等于** `port_anchor(frame, port)`，容差 1e-9 |
| **折点上界** | 每条边折点数 ≤ `max_bends_budget`（typed param，默认 6），超出报 relaxation |
| **规模冒烟** | 新增 `stress.wide-layer`（层宽 ≥ 32）与 `stress.deep-chain`（≥ 100 层）两个 fixture，进 `hier_eval` 硬门禁 |
| **自环进校验** | 把 `self_loop_edges` 的 `extend` 移到两次 `verify` **之前**（`mod.rs:262`），当前自环完全不受检 |

---

### 4.8 静默失败清零

本次评审发现的静默降级点，全部应改成「硬失败」或「发 relaxation」，一个都不留：

| 位置 | 现状 | 改成 |
|------|------|------|
| `derive_substrate` 组回退 | 静默 root-scope（31/40 命中） | **relaxation** + 进 `hier_eval` 观测量 |
| `ink::route::lane_coord` | `unwrap_or(0)` | **`InternalInvariant` 硬失败** |
| `DemandBoard::publish` 非有限/负值 | 静默丢弃 | **硬失败** |
| `verify_no_node_penetration` 遇非正交段 | `continue` 跳过 | orthogonal 风格下硬失败；curved 单独采样校验 |
| 自环 | 绕过全部 verify | 移到 verify 前 |
| `GateCapacity` | 恒 `Unbounded`，`Fixed` 分支永不执行 | 要么按 crossing 数估容量真正启用，要么删掉 `Fixed` 变体（不留半截抽象） |

同批清掉死代码：`channel/substrate.rs` 的 `required_orient` / `covers_slot`（从未调用）、`channel/mod.rs:15` 未用导入、`channel/derive.rs:82` 未用变量、`track_order.rs:97-104` 的等价双分支、`assign_ports` 的 `_canonical_size` 参数。

---

## 5. 建议新增的质量度量

`hier_eval` 现在记录 `max_bends / sum_bends / crossings / reversed_count / bbox`。建议补四个，全部可自动算、可设阈值：

| 指标 | 定义 | 为什么 |
|------|------|--------|
| `symmetry_deviation` | 对每个 `deg≥2` 的扇，`\|x_hub − mid(外包围)\| / node_gap`，取 max 与 sum | §2.3 的表就是用它做的；这是「对称性」唯一可门禁的表达 |
| `gap_uniformity` | 每把扇内相邻孩子间距的 `stdev / mean` | 抓 `110/51.5/58.5` 这类 |
| `straightness` | `Σ_edges |x_src_anchor − x_tgt_anchor|` 中，本可共线（1:1 链）却没共线的比例 | 抓阶梯折 |
| `overlap_len` | 非 bundle 边共线重合的总长度 | 抓「多边错叠」 |
| `channel_used_gates` | 布尔（+ 回退原因） | §2.6 的整条链现在在门禁上是隐形的；这一条最便宜、收益最直接 |
| `ripup_rounds` / `relaxations` | 计数 | 现在 78 个 fixture 一条 relaxation 都没有，rip-up 路径零覆盖 |

这六个加上现有五个，构成「正确性（硬）/ 合理性（阈值）/ 观测（打印）」三档，正好对上 `measure` 子命令的 §5.2 分类。

`channel_used_gates` 建议直接加**硬门禁**：一旦某个组 fixture 从 `d1.3-gate` 退回 `d1.3-root-scope`，视为回归。当前基线是 9/40，这个数字应该只增不减。

---

## 6. 建议的落地批次

**批次严格串行**；每批结束时 78 个 fixture 全绿 + 新度量不劣化。

| 批 | 内容 | 目标 | 预估 |
|----|------|------|------|
| **0 · 止血** | §4.2.1 修 `cmp_key`；补宽层 fixture 与测试；同步 3 个红快照；§4.8 静默失败清零 + 死代码清理 | 不再崩；不再静默降级；门禁绿 | 1 天 |
| **1 · 观测** | §5 六个新度量进 `hier_eval` 并记基线（`channel_used_gates` 设硬门禁）；§4.5.3 relaxation 产出；§4.7 自环进 verify | 后续所有改动可量化 | 1 天 |
| **2 · 组与定序** | §4.2.2 边界 dummy；组框进 VPSC | 组通道启用率 9/40 → 接近全覆盖；组图 `crossings` 目标降 50%+ | 3–5 天 |
| **3 · 性能** | §4.1.1 增量 NS；§4.2.3 增量交叉；消除 O(E²) 字符串查找 | 1600 节点 < 1s | 2–3 天 |
| **4 · 对称内核** | §4.3 S1→S4；§4.4 `layer_alignment` | `symmetry_deviation` 在 DAG fixture 上降一个数量级；`symmetry.rs` 减 ~450 行 | 4–5 天 |
| **5 · 路由** | Substrate 建模节点占位（§4.6 前置）；§4.5 加权代价 + 交叉项；§4.6 Ink 写权归位；删 `clear_main_x` | 折点/交叉可权衡；Ink 零决策可验证 | 5–7 天 |

**批次顺序相对初稿做了调整**：组与定序（原批 4）提到了性能与对称之前。理由是 §2.6 证明它是唯一一条能闭合的因果链，牵动交叉、组通道、组框三件事，且在数据上量级最大（组图平均交叉 14.5 vs 1.7）。对称内核（批 4）单项收益依然最高，但它的验收依赖批 1 的度量，且改动面大，适合放在组结构稳定之后。

批 0 和批 1 建议**立刻做**：前者是线上崩溃 + 一批会掩盖 bug 的静默降级，后者是所有后续判断的前提。

---

## 7. 附录：复现命令

```bash
# A. 门禁现状
cargo test -p plotgram-layout -p plotgram-compile --no-fail-fast

# B. 78 fixture 指标聚合
python3 -c "
import json
d=json.load(open('crates/plotgram-compile/tests/hier_eval_baseline.json'))
print('sum_bends', sum(v['sum_bends'] for v in d.values()))
print('crossings', sum(v['crossings'] for v in d.values()))
"

# C. 复现 P0 崩溃（生成层宽 18 的图）
python3 - <<'PY'
L,P=6,18
n=[f'n{l}_{i}' for l in range(L) for i in range(P)]
e=[(f'n{l}_{i}',f'n{l+1}_{(i*7+3)%P}') for l in range(L-1) for i in range(P)]
open('/tmp/wide.pgm','w').write(
  'diagram {\n  profile: flowchart,\n  layout: hierarchical { direction: top-to-bottom }\n\n'
  + ''.join(f'  node {x} {{ label: "{x}", archetype: process }}\n' for x in n)
  + ''.join(f'  {a} -> {b}\n' for a,b in e) + '}\n')
PY
cargo run -q --release -p plotgram-cli -- render /tmp/wide.pgm -o /tmp/wide.svg
# → panicked: user-provided comparison function does not correctly implement a total order

# D. 规模化（窄层深图，绕开 C 的崩溃）
#    生成 deep50/100/200 后：
/usr/bin/time -p ./target/release/plotgram render /tmp/deep200.pgm -o /tmp/x.svg

# E. 采样 profile（debug 构建才有符号）
./target/debug/plotgram render /tmp/deep100.pgm -o /tmp/x.svg & sample $! 8 -f /tmp/prof.txt

# F. 对称性定量扫描
cargo run -q -p plotgram-cli -- debug-layout <fixture>.pgm | \
  python3 -c "..."   # 按 common.nodes.center.x 分组统计扇心偏移

# G. 组通道启用率（§2.6 的核心证据，最值得先跑一遍）
python3 - <<'PY'
import subprocess, glob, json, collections
c = collections.Counter()
for f in sorted(glob.glob('apps/showcase/hierarchical/group/*.pgm')):
    d = json.loads(subprocess.run(
        ['./target/release/plotgram', 'debug-layout', f],
        capture_output=True, text=True).stdout)
    c[d['extension']['channels']['status']] += 1
print(dict(c))   # 当前：{'d1.3-root-scope': 31, 'd1.3-gate': 9}
PY

# H. 组包围盒纯度（回退真因）
#    对每个 group 取成员的 (rank, order) 包围盒，检查盒内是否有非成员节点
```

---

## 8. 需要产品/设计裁定的四个问题

这四条超出实现层，`expectations.md` 目前没有明确裁定：

1. **对称 vs 图宽的兑换率**。§4.3 的 `λ_sym` 需要一个默认值。对称性拉满会让 DAG 显著变宽（`§6 冲突表`说「清晰优先于极限压窄」，但没给量）。建议：先按 `λ_sym = 1.0` 跑基线，再按 `bbox_w` 涨幅上限（如 +15%）反推。
2. **多父节点归谁**。一个节点被两把扇同时要求居中时，是（a）折中，（b）按边权优先级归一把，还是（c）按「声明靠前」归。本报告方案默认 (a)。这需要写进 `expectations.md` §6。
3. **菱形等非矩形节点的端口**。是继续用 bbox 边（当前，线段起点会悬空），还是按形状轮廓取点（更好看，但端口的 `along` 语义要从「边上比例」改成「轮廓弧长比例」）。这会影响 `AlongSpec` 契约，得先定。
4. **组矩形不干净时该硬失败还是回退**。当前是静默回退（31/40 命中）。批 2 做完之后如果仍有 fixture 回退，是继续回退（但发 relaxation）、还是硬失败逼作者改图？前者宽容但会让「组内不穿越」这条语义在部分图上不成立；后者严格但可能挡住合法的 weak-group 图。建议先按「回退 + relaxation」跑一轮，看剩余回退数再定。
