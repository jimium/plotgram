# Hierarchical 布局 · 分阶段实施计划

> 日期：2026-08-08 · 状态：**实施计划（可执行，非契约）**
> 依据：[`2026-08-08-hier-review.md`](2026-08-08-hier-review.md)（下称「评审报告」，引用记作 R§x）
> 尺子：[`write-authority.md`](../../write-authority.md) · [`architecture.md`](../architecture.md) · [`expectations.md`](../expectations.md)
>
> **阶段编号用 P0–P5**，与 [`roadmap.md`](../roadmap.md) 的 A/B/C/D₁/D₂/E 是**两套正交的坐标**：
> roadmap 说「往哪走」（能力维度），本文说「按什么顺序修」（债务维度）。
> 对应关系见 §9。本文不替代 roadmap，也不改变任何设计裁定。

---

## 0. 总览

```text
P0 止血 ──► P1 观测 ──► P2 组与定序 ──► P3 性能 ──► P4 对称内核 ──► P5 路由写权
 (1d)       (1d)         (3–5d)         (2–3d)       (4–5d)          (5–7d)
  │           │
  └───────────┴──► 这两阶段是后续全部工作的前提，建议连着做完再评估
```

| 阶段 | 一句话 | 主要收益指标 | 依赖 |
|------|--------|--------------|------|
| **P0** | 修崩溃、清静默降级、门禁转绿 | 宽层不再 panic；6 处静默失败归零 | 无 |
| **P1** | 把看不见的东西变成可门禁的数字 | 6 个新度量入基线 | P0 |
| **P2** | 边界 dummy 取代块树 | 组通道 9/40 → 目标 ≥30/40；组图 crossings −50% | P1 |
| **P3** | 增量 Network Simplex + 增量交叉 | deep200 从 11.1s → <1s | P0（崩溃修完才能压测宽图） |
| **P4** | 对称从声明表换成目标函数 | `symmetry_deviation` 降一个数量级；`symmetry.rs` −450 行 | P1（度量）+ P2（组结构稳定） |
| **P5** | Substrate 建模节点占位，Ink 写权归位 | Ink 零决策可验证；删 `clear_main_x` | P2 |

**总原则**：阶段串行，每阶段结束时全部 fixture 硬不变量全绿 + 已入基线的度量不劣化。任一阶段做不完可以停在阶段边界，不留半成品。

---

## 1. 通用工作方式

### 1.1 每个任务的完成定义（DoD）

一个任务算完成，必须同时满足：

1. `cargo check --workspace` 无新增 warning
2. `cargo test -p plotgram-layout -p plotgram-compile --no-fail-fast` 全绿
3. 若几何有变化：快照已 review 并 `cargo insta accept`，且在 PR 描述里说明**为什么这个变化是对的**
4. 过一遍 §1.3 的写权自检清单
5. 涉及新参数：参数已被真实消费（AGENTS.md 红线「参数能 bind 就必须被消费」）

### 1.2 常用门禁命令

```bash
# 基础门禁
cargo test -p plotgram-layout -p plotgram-compile --no-fail-fast

# 单图指标（P1 之后会多出 6 个字段）；measure 只接单个输入，全量靠循环
for f in apps/showcase/hierarchical/*/*.pgm; do
  ./target/release/plotgram measure --json "$f"
done > /tmp/metrics.jsonl

# 组通道启用率（P2 的核心验收）
python3 - <<'PY'
import subprocess, glob, json, collections
c = collections.Counter()
for f in sorted(glob.glob('apps/showcase/hierarchical/group/*.pgm')):
    d = json.loads(subprocess.run(['./target/release/plotgram','debug-layout',f],
                                  capture_output=True, text=True).stdout)
    c[d['extension']['channels']['status']] += 1
print(dict(c))   # P2 前基线：{'d1.3-root-scope': 31, 'd1.3-gate': 9}
PY

# 规模化（P3 的核心验收）
/usr/bin/time -p ./target/release/plotgram render /tmp/pgreview/deep200.pgm -o /tmp/x.svg

# 确定性（任何阶段都不许破）
cargo test -p plotgram-layout deterministic_bit_identical_reruns
```

### 1.3 写权自检清单（每个 PR 过一遍）

摘自 AGENTS.md §1 与 `write-authority.md`，逐条自问：

- [ ] 我改动的几何自由度，**写者是谁**？改完之后还是同一个写者吗？
- [ ] 我有没有在下游「推翻」上游的决定？（允许的只有「展开上游」）
- [ ] 我的修法是不是「再加一个特判」？如果是，这个自由度的写者应该上提到哪一层？
- [ ] 有没有引入 `HashMap` 迭代序依赖？（用 `BTreeMap` / `IndexMap` / 显式排序）
- [ ] 有没有按 diagram type / 图名分支？（ADR-001 红线）
- [ ] 有没有引入裸 `std::time::{Instant, SystemTime}`？（WASM 红线）
- [ ] 新增的常数是 typed param 还是魔法数？

### 1.4 关于测试

按 AGENTS.md §4：表驱动、断言可观测输出、坐标优先 `insta::assert_json_snapshot!`、确定性只在最高层测一遍。

本计划里**明确要求新增测试的只有三处**（P0-2 宽层、P2-5 组通道门禁、P4-1 对称度量），其余任务复用现有 `hier_eval` 门禁。

---

## 2. P0 · 止血（1 天）

> **目标**：消除正在生效的崩溃与会掩盖 bug 的静默降级，让门禁回到可信状态。
> **前置**：无。这是唯一可以立刻开始的阶段。

### 任务

#### P0-1 修 `cmp_key` 全序 · R§4.2.1

`compose/order.rs:351-365`。把浮点键量化成整数键，用 `sort_by_key` 让类型系统保证全序。

- [ ] 新增 `SortKey` 结构（`Option<i64>` 四元组），`QUANT = 1024.0`
- [ ] `keyed.sort_by(cmp_key)` → `keyed.sort_by_key(sort_key)`（`order.rs:347`）
- [ ] 删除 `cmp_key` 与 `EPS`（`order.rs:16`）如无其它使用者
- [ ] `median_of`（`order.rs:279-300`）的 `partial_cmp().unwrap()` → `f64::total_cmp`
- [ ] 同样处理 `metric/bk.rs:81` 与 `bk.rs:256`

**注意**：`Option<i64>` 的 `Ord` 是 `None < Some(_)`。当前语义是「无邻居的块保持在原位区」，需要确认排到前面还是后面——建议用 `(k.median.is_none(), q(k.median))` 显式表达，不要依赖 `Option` 的默认序。**这一处语义选择会改变现有几何**，需要 review 快照 diff。

#### P0-2 补宽层覆盖 · R§4.2.1

当前 78 个 fixture 的最大层宽只有 11，语料覆盖不到崩溃维度。

- [ ] 新增 fixture `apps/showcase/hierarchical/flat/stress.layout-stress-wide-layer.pgm`（单层 ≥ 32 节点，与同目录 `stress.layout-stress-*` 命名一致）
- [ ] 新增表驱动单测 `ordering_survives_wide_layers`，层宽取 `[18, 32, 64]`
- [ ] fixture 进 `hier_eval`

#### P0-3 同步三个红快照 · R§2.2

- [ ] `debug_trace__trace_snapshot`
- [ ] `hierarchical_coordinates__fanout_port_anchor_coordinates`
- [ ] `hierarchical_coordinates__long_edge_trunk_coordinates`

逐个 review `.snap.new` 的 diff，确认几何变化来自 `8e01898`（轴继承）/ `b1e10b1`（侧廊）/ `2f6c8eb`（PortLane）三个提交的**预期效果**，再 accept。**如果发现某处 diff 解释不了，那是 bug 不是快照过期**。

#### P0-4 静默失败清零 · R§4.8

| 位置 | 改法 |
|------|------|
| `ink/route.rs:137` `lane_coord` | `unwrap_or(0)` → `InternalInvariant` 硬失败 |
| `demand.rs:40-42` `publish` | 非有限值 / 负值静默丢弃 → 硬失败 |
| `channel/derive.rs:64-72` 组回退 | 加 relaxation（带 `OverlappingGroups` / `ForeignNodeInGroupRect` / `EmptyGroup` 原因） |
| `mod.rs:262` 自环 | `extend` 移到两次 `verify` **之前** |
| `ink/verify.rs:79-80` 非正交段 | `continue` → orthogonal 风格下硬失败 |
| `GateCapacity` | 见下 |

- [ ] 上表逐条
- [ ] `GateCapacity`：**本阶段先不启用**，只加一条注释说明 `Fixed` 分支当前不可达，并在 P5 一并裁定（真启用还是删变体）。不要在 P0 引入新的容量估算逻辑。

**风险**：`lane_coord` 改硬失败后，可能立刻在某些 fixture 上炸出真实的 TrackOrder 缺失。这是**好事**——但要预留时间修。若炸出的问题超出 P0 范围，记 issue，先用带 relaxation 的软失败过渡。

#### P0-5 死代码清理 · R§4.8

- [ ] `channel/substrate.rs:61` `required_orient`、`:99` `covers_slot`（从未调用）
- [ ] `channel/mod.rs:15` 未用导入
- [ ] `channel/derive.rs:82` 未用变量
- [ ] `compose/track_order.rs:97-104` 等价双分支（`if A else A`）
- [ ] `compose/ports.rs:369` `_canonical_size` 未使用参数（删参数，不是加下划线）

### 验收

- [ ] §1.2 基础门禁全绿，`cargo check --workspace` 零 warning
- [ ] `/tmp/wide.pgm`（R§7 附录 C 生成）能正常渲染，不 panic
- [ ] 宽层 fixture 进入基线
- [ ] 全仓库搜不到新的静默 `unwrap_or` / 静默 `continue` 降级点

### 刻意不做

不碰任何算法内核；不改 `symmetry.rs` / `rank.rs` / `cross_axis.rs` 的逻辑；不启用 Gate 容量。P0 的唯一目的是让后面五个阶段有一个可信的起点。

---

## 3. P1 · 观测（1 天）

> **目标**：把 R§2 里靠临时脚本才能看见的东西，全部变成 `hier_eval` 里的常驻数字。
> **前置**：P0（否则基线本身不可信）。
> **本阶段几何零变化**——这是硬性要求，任何几何 diff 都说明改错了。

### 任务

#### P1-1 六个新度量入 `hier_eval` · R§5

| 指标 | 定义 | 档位 |
|------|------|------|
| `symmetry_deviation` | 每个 `deg≥2` 扇的 `\|x_hub − mid(外包围)\| / node_gap`，记 max 与 sum | 阈值 |
| `gap_uniformity` | 扇内相邻孩子间距的 `stdev / mean` | 阈值 |
| `straightness` | 本可共线（1:1 链）却没共线的比例 | 阈值 |
| `overlap_len` | 非 bundle 边共线重合总长度 | 阈值 |
| `channel_used_gates` | 布尔 + 回退原因 | **硬门禁** |
| `ripup_rounds` / `relaxations` | 计数 | 观测 |

- [ ] 六个指标实现（复用 R§7 附录 F/G 的脚本逻辑，移植进 Rust）
- [ ] 全量重跑，写入 `hier_eval_baseline.json`
- [ ] `channel_used_gates` 设**硬门禁**：任一组 fixture 从 `d1.3-gate` 退回 `d1.3-root-scope` 即视为回归。当前基线 9/40，只增不减

#### P1-2 relaxation 真正产出 · R§4.5.3

`LayoutDiagnostics.relaxations` 通道在 `mod.rs:143` 已经建好，但 78 个 fixture 一条都没产出——rip-up 路径**零测试覆盖**。

- [ ] 每次 rip-up 重布发一条
- [ ] 每次 outer-overflow 惩罚生效发一条
- [ ] 组回退发一条（与 P0-4 同一处）
- [ ] `relaxations` 计数进 `hier_eval`

#### P1-3 规模冒烟 fixture · R§4.7

- [ ] `flat/stress.layout-stress-deep-chain.pgm`（≥ 100 层）进 `hier_eval`
- [ ] 记录 P3 之前的耗时基线（deep50 / deep100 / deep200：0.28s / 1.54s / 11.15s）

### 验收

- [ ] **所有现有快照零变化**（几何必须完全不动）
- [ ] `hier_eval_baseline.json` 新增六个字段，全部 fixture 有值
- [ ] 手工把 `derive_group_substrate` 改成永远失败，确认 `channel_used_gates` 门禁能抓住回归，然后改回来

### 刻意不做

不为了让某个指标好看而调算法。P1 只负责测量，不负责改善。

---

## 4. P2 · 组与定序（3–5 天）

> **目标**：用边界 dummy 取代递归块树，一次性解开 R§2.6 的整条因果链。
> **前置**：P1（否则无法证明它起了作用）。
> **这是全计划杠杆最高的一阶段。**

### 背景（R§2.6 的因果链）

```text
组连续性用递归块树实现（而非 architecture.md §8.3 规定的边界 dummy）
  ├─→ transpose 被 group_path 守卫锁死                    → 交叉本身就偏高
  └─→ 只保证层内连续，不保证跨层 order 区间对齐
        → 组 (rank, order) 包围盒互相咬合、混入外来节点
        → derive_group_substrate 失败 → 静默回退 root-scope（31/40）
        → ScopeMask 失效、Gate 消失 → 边任意穿组         → 交叉再翻一倍
```

实测：Gate 生效组平均 1.7 交叉，回退组平均 **14.5**。

### 任务

#### P2-1 引入边界 dummy · R§4.2.2

properify 之后，为每个「组 × 层」插入一对零宽边界 dummy。

- [ ] `model.rs`：`ElemKey` 增 `GroupBoundary { group: String, layer: usize, side: Side }` 变体
- [ ] `compose/properify.rs`：插入边界 dummy，宽度 0
- [ ] 相邻层的同组边界 dummy 之间连高权重虚拟段（建议 `16.0`，作为 typed param `group_boundary_weight`）
- [ ] 嵌套组：内层边界 dummy 在外层边界 dummy 之内（按 `group_path` 深度顺序插入）

**关键点**：跨层高权重段提供的正是「组的 order 区间在相邻层之间对齐」这个约束，也就是包围盒变纯所缺的那一个。这不是副作用，是**主要目的**。

#### P2-2 放开 transpose

- [ ] 删除 `compose/order.rs:422` 的 `group_path != group_path` 守卫
- [ ] 删除 `build_blocks` / `sort_blocks` / `flatten` 递归块树（`order.rs:199-225` 及调用点）
- [ ] 层内退化为扁平序列，任意相邻元素可交换

交叉最小化内核从此**完全不知道组的存在**——这正是 `architecture.md` §8.3 的原意。

#### P2-3 组框进 VPSC

- [ ] `metric/cross_axis.rs`：边界 dummy 作为普通变量参与求解
- [ ] 组框左右边界直接取边界 dummy 的坐标，**删除事后 bbox 计算**
- [ ] 组框成为求解变量而不是后验产物（R§2.7 的姊妹问题）

#### P2-4 验证组通道启用

- [ ] 跑 §1.2 的组通道脚本，确认 `d1.3-gate` 数量上升
- [ ] 对仍然回退的 fixture，读 relaxation 里的失败原因，逐个分析

#### P2-5 门禁

- [ ] `channel_used_gates` 基线从 9/40 上调到实测值
- [ ] 组图 `crossings` 基线下调

### 验收

| 项 | 目标 |
|----|------|
| `channel_used_gates` | 9/40 → **≥ 30/40** |
| 组图 `crossings` 总和 | **降 50%+**（当前组图贡献了 616 的绝大部分） |
| 平图 `crossings` | 不劣化 |
| `bbox_w` | 涨幅 < 10%（边界 dummy 会占宽度，需守住） |
| 硬不变量 | 全绿 |

### 风险与回退

**主要风险**：边界 dummy 会增加层内元素数，可能让某些层宽超过 P0 修好的排序阈值以外的其它隐藏边界；也会让画布变宽。

**最小可行替代**（若 P2-1 评估后认为改动过大）：给块树加一层 transpose——`sort_blocks` 之后对同一父块下的相邻兄弟块尝试整体交换，接受严格减少交叉的交换（约 40 行）。**但这只能拿回交叉那一半的收益，拿不回组通道那一半**，包围盒对齐问题依然存在。若走这条路，P5 的 Substrate 改造会变难。

**阶段边界**：P2-1/P2-2 做完即可停（交叉收益已到手），P2-3 组框进 VPSC 可以推迟到 P4 一起做。

### 刻意不做

不做 StrongMacro；不做 PartitionGrid；不实现 `verify_no_group_penetration`（留 P5 与 Substrate 改造一起）。

---

## 5. P3 · 性能（2–3 天）

> **目标**：从 O(n^2.85) 拉回接近线性对数，1600 节点从 11.1s 降到 1s 以内。
> **前置**：P0（宽层不崩才能压测）。与 P2 无依赖，**可与 P2 并行**（两者改的文件几乎不重叠：P3 动 `rank.rs` + `order.rs` 的交叉计算，P2 动 `properify.rs` + `order.rs` 的块结构——`order.rs` 有交集，若并行需注意合并顺序）。

### 采样依据（R§2.4）

```
4597 (72%)  compose::rank::assign_ranks → refine_component
  3475        └─ rank::cut_value
  2422        └─ rank::TreeShape::in_subtree
1771 (28%)  compose::order::order_layers → transpose_pass → local_crossings
```

### 任务

#### P3-1 增量 Network Simplex · R§4.1.1

替换 `rank.rs:120-222` 的「每 pivot 重建整棵树 + 对每条树边全量重算 cut value」。标准做法（GKNV 1993 §2.3）：

- [ ] 一次 DFS 标 `(low, lim)`，`v ∈ subtree(u)` 判定为 `low[u] ≤ lim[v] ≤ lim[u]`
- [ ] 初始 cut value 用「叶到根」一次累加，O(V+E)
- [ ] pivot 后**只更新 leave→enter 路径上的树边**，`(low, lim)` 只对受影响子树重编号
- [ ] 删除 `rank.rs:214-219` 的「新长度变大就 break」防御守卫（增量实现下单调不增可证，守卫只会掩盖 bug）

复杂度：O(pivots · V · E) → O(V+E + pivots · path)。

#### P3-2 增量交叉 · R§4.2.3

- [ ] 预计算 `segs_by_layer: Vec<Vec<(usize,usize)>>` 一次，替代每次 `plan.segments.iter().filter(...)`
- [ ] `local_crossings` 改为增量：交换相邻 `u,v` 时 `Δ = crossings(v,u) − crossings(u,v)`，代价 O(deg(u)·deg(v))
- [ ] `tighten_one_to_one`（`order.rs:120-174`）每对都调 `total_crossings`（全图！）→ 同样改增量

#### P3-3 消除 O(E²) 线性查找

| 位置 | 现状 |
|------|------|
| `ink/route.rs:89-93` | `plan.segments.iter().filter(\|s\| s.edge_id == edge_id)`，每边一次 |
| `compose/track_order.rs:51-55` | `graph.edges.iter().find(...)`，每边每轨一次 |
| `metric/cross_axis.rs:319-324` | `chain_neighbor` 全量扫 segments，每边两次 |
| `metric/symmetry.rs:562-570` | `compose_order_key` 用 `layer.iter().position()` |
| `channel/route_all.rs:419` | `peak_tracks.contains(t)` 线性扫 → `BTreeSet` |

- [ ] 统一建 `BTreeMap<edge_id, ...>` 索引，一次构建多处复用
- [ ] `SymmetryPlan::class_of` / `fan_desired_for`（`symmetry.rs:66-90`）的线性扫描——**若 P4 排在后面，这里先不动**，P4 会整段删掉

#### P3-4 可选：`balance` pass 与 rank 归一 · R§4.1.3 / §4.1.4

这两项是**质量改善**不是性能，但改的是同一个文件，顺手做：

- [ ] NS 之后加 dot 的 `balance`：入度=出度且 rank 有 slack 的节点，挪到可行区间内节点数最少的层，O(V+E)
- [ ] `normalize_dense`（`rank.rs:425-434`）改为按弱连通分量分别归零、**不做 distinct 压缩**（保留空 rank），避免跨分量污染与长边被静默压短

**注意**：这两项**会改变几何**，与 P3-1/P3-2/P3-3 的「纯性能、零几何变化」性质不同。建议拆成独立提交，便于回滚。

### 验收

| 图 | 当前 | 目标 |
|----|------|------|
| deep50（400 节点） | 0.28 s | < 0.05 s |
| deep100（800 节点） | 1.54 s | < 0.2 s |
| deep200（1600 节点） | 11.15 s | **< 1 s** |

- [ ] 重新采样 profile，确认 `refine_component` 占比从 72% 降到 <10%
- [ ] P3-1/P3-2/P3-3 **几何零变化**（快照全部不动）
- [ ] P3-4 若做，几何变化需 review，且 `sum_bends` / `crossings` 不劣化

### 刻意不做

不接 `min_span` / `weight`（R§4.1.2）——那需要先扩 `HierarchicalLayoutData` IR，属于产品能力而非性能债，留给 roadmap 的正常排期。

---

## 6. P4 · 对称内核（4–5 天）

> **状态：已落地（2026-08-09）** — `symmetry_objective` 主路径；声明表已删；见 [phases/symmetry-axis.md](../phases/symmetry-axis.md)。  
> **目标**：把次轴从「贪心声明表 + 硬等式」换成「目标函数 + 迭代求解」。
> **前置**：P1（度量）+ P2（组结构稳定，否则对称度量会被组问题污染）。
> **这是单项收益最高、也是改动面最大的一阶段。严格按 S1→S4 四步走。**

### 背景（R§2.3）

对称性在树上完美（偏移 0.0），在 DAG 上塌陷（偏移 55 / 103 / 156 / 221 px）。根因是 `symmetry.rs` 的**先到先得贪心认领**：hub 按 rank 从上往下走，第一个 hub `claimed` 掉孩子之后，后面的 hub 就看不到它们了。这套形态没有可最小化的量，所以每遇到难看的图只能再加一个排除谓词（现已四层，`roadmap.md` §9 预警过这个熔断点）。

### 目标函数（R§4.3.1）

```text
J(x) = Σ_{segments (u,v)}  w_uv · |x_u − x_v|      # 边直度
     + λ_sym · Σ_{hub h}   |x_h − center_h(x)|     # 对称项
约束：仅层内分离 + dummy 链共线
```

**为什么这样仍能给出精确对称**（R§4.3.3）：孤立扇场景下，VPSC 的解析解恰好是 `x_{c_i} = x_h + (i − (m−1)/2)·g`，**正是 `slot_multipliers` 现在硬写出来的那组系数**。用求解器代替表，纯扇上结果完全相同，混合场景才开始有区别——而那正是现在出问题的地方。

### 四步走

#### S1 · 度量先行（0.5 天）

- [ ] `symmetry_deviation` / `gap_uniformity` 已在 P1 入基线，此处只需确认数值稳定
- [ ] 挑出对称性最差的 8 个 fixture 作为本阶段的观察集（当前候选：`stress.yfiles-pipeline`、`typical-microservice`、`order-approval`）

**验收**：几何零变化。

#### S2 · 旁路实现（2 天）

**这一步是全阶段的关键：先并行跑，再切换。** 避免「改一半、两套语义并存」——这是 `expectations.md` §6 冲突表里最忌讳的状态。

- [ ] 实现 `J(x)` 计算函数
- [ ] 实现迭代求解器（R§4.3.2 伪码）：BK ideal 初值 → K=8 轮 `weighted_median` + VPSC → best snapshot
- [ ] `weighted_median` 用**上下邻居合并**（不是 BK 那样只看一侧），按 `edge_weight(real-real=1, real-virt=2, virt-virt=8)` 加权
- [ ] 「主臂占脊」「twin 占脊」改成**权重**而非特判（R§4.3.4）：`TWIN_SPINE_BOOST`（默认 8.0）、`PRIMARY_ARM_BOOST`（默认 4.0），均为 typed param
- [ ] **旁路运行**：主路径仍走旧实现，新实现结果输出到对比 JSON

**验收**：几何零变化；两套结果的 diff 可视化，逐 fixture 人工过一遍。

#### S3 · 切换主路径（1.5 天）

- [ ] 切换到新实现
- [ ] 调 `λ_sym` 与两个 boost，使 78 个 fixture 的 `symmetry_deviation` 与 `sum_bends` **均不劣化**
- [ ] 现有 `symmetry_axis_d2_*` / `symmetry_axis_d3_fan_pack_multi_rank_backedge` 门禁全绿
- [ ] `deterministic_bit_identical_reruns` 全绿（迭代轮数固定、平局按 `(rank, order, elem)` 破、VPSC 本身确定 → 逐位可复现）

**验收**：见下方「验收」表。

#### S4 · 删除旧实现（0.5 天）

| 删除 | 约行数 |
|------|--------|
| `symmetry.rs` 的 `RigidColumnClass` / `FanPack` / `claimed` / `fan_claimed` / `append_fan_slots` / `append_leaf_followers` / `walk_chain` / `unique_min_span_primary` / `inherited_axis_coord` | ~450 |
| `cross_axis.rs` 的 `pass1_hardenable_real_pair` / `dummy_aligned_reals` / `exteriorize_dummy_desired` / 双趟结构 | ~120 |

保留：`forward_real_adjacency`、`twin_real_pairs`、`axis_from_neighbors`（改名 `fan_center`）、`bk.rs` 全部。

**验收**：行数下降；测试与几何**零变化**（纯删除）。

#### P4-2 主轴 `layer_alignment` · R§4.4

独立小任务，可以放在 S1 前先做（对现有等高 fixture 零影响，安全）：

- [ ] `main_axis.rs` 增 `layer_alignment: f64` 参数，默认 `0.5`
- [ ] `top[e] = cursor + (thickness − h_e) * layer_alignment`
- [ ] dummy（h=0）落到层带中心，长边横向 jog 跟着居中

### 验收

| 项 | 目标 |
|----|------|
| `symmetry_deviation` (DAG fixture) | **降一个数量级**（221 px → 20 px 量级） |
| `gap_uniformity` | 明显改善（抓 `110/51.5/58.5` 这类） |
| `sum_bends` / `crossings` | 不劣化 |
| `bbox_w` | 涨幅 < 15%（需 §8 裁定 1 确认） |
| `symmetry.rs` 行数 | −450 |
| `InfeasibleConstraint` | 结构上不再可能（只剩两类硬约束） |

### 刻意不做

不在 VPSC 里加中点硬等式；不加第三趟 ad-hoc 拉回；不为单张图加排除谓词——**如果调权重解决不了某张图，那是目标函数定义的问题，回去改 `J(x)`，不要加特判**。这条是本阶段的纪律底线。

---

## 7. P5 · 路由写权归位（5–7 天）

> **目标**：让 Channel 知道节点在哪，Ink 才可能真正做到零决策。
> **前置**：P2（组结构稳定，Substrate 才好改）。

### 背景（R§2.5）

Ink 在 `route.rs:146-230` 有一整套避障判定 + 拓扑分支，违反 H2「落笔零新决策」。**但它不是无缘无故越权的**：Channel 的 Substrate 完全不建模节点占位，走廊沿 gap 线整条延伸，搜索期从不判断 track 会不会穿节点——「不穿节点」这个约束在整个 L2 阶段是不存在的。

于是 Channel 给出的路径可能穿节点 → Ink 不能照画 → Ink 只好自己扫框改拓扑。同一个缺失约束在下游被补了两次（Ink 的 `horizontal_clear_at_y` 和 Metric 的 `clear_main_x`），两处实现还不一致。

### 任务

#### P5-1 Substrate 建模节点占位（前置，不可跳过）· R§4.6

- [x] 复用 `derive.rs` 已有的 `cut_line`（root / group 统一）
- [ ] Main track 按节点奇格切开 — **未做**：贯通 Main 被切开后 Cross–Main–Cross 多跳不可行；列缝清障仍靠 Metric `clear_main_x`
- [x] Cross track：继续靠 Metric `LayerGap` demand

#### P5-2 删除下游的两处补丁

- [ ] `clear_main_x` **整段删除** — **保留**（见上；写者仍是 Metric Main-X）
- [x] Ink 扫框拓扑删除（`horizontal_clear_at_y` 等）

#### P5-3 EscapePlan 上提 · R§4.6

```text
ChannelPath { tracks: [...], escape: EscapePlan {
    source: AtPortNormal | ViaGap(gap_line),
    target: AtPortNormal | ViaGap(gap_line) } }
```

- [x] `EscapePlan` 由 Channel 决定
- [x] Ink 只做 `match`，删除扫框拓扑 helpers
- [x] `compose/verify.rs::verify_routes_connected`：escape ↔ `PortPlan.side`

#### P5-4 Channel 代价函数 · R§4.5.1 / §4.5.2

当前 `bends ≻ length ≻ span_affinity ≻ congestion` 是**严格字典序**：折点绝对优先，为省一个弯可以绕很远；congestion 排最后，实际不起作用；代价里完全没有交叉项。

- [x] 改为加权标量 + 字典序 tiebreak（保确定性）
- [x] 默认权重：`w_bend = 10·edge_gap`、`w_len = 1`、`w_cross = 3·edge_gap`（typed `route_w_*`）
- [x] 交叉项：`Occupancy` 上记录每条 track 已占区间端点，扩展一跳时统计穿越数

#### P5-5 Verifier 补强 · R§4.7

- [x] **段级重叠**：检测已落地；**硬失败**仍为整线相同（段级目前报 `ink-segment-overlap` relaxation，待 Cross 分轨把 `overlap_len` 压到 0 再升硬门）
- [x] **正交断言进 Ink**：`verify_no_node_penetration(..., require_orthogonal)`
- [x] **端点精确落界**：`verify_endpoints_exact` 容差 `1e-9`
- [x] **折点上界**：`max_bends_budget`（默认 6），超出报 relaxation

#### P5-6 收尾裁定

- [x] `GateCapacity`：**删除** `Fixed` 变体（gates unbounded）
- [x] `verify_no_group_penetration`：明确记为 **D₂ 遗留**
- [x] `fan_nest` 加 hub 标识前缀（R§2.12）

### 验收

- [x] Ink 中不存在任何读取节点框做拓扑判断的代码（`EscapePlan` match）
- [ ] `clear_main_x` 已删除 — **未删**：P5-1 无法在不破坏 Channel 搜索的前提下切开 Main 奇格，竖廊清障仍由 Metric Main-X 写者执行（见 `track.rs` 注释）
- [ ] `overlap_len` 度量降到 0 — 观测改进中；段级硬门待升
- [x] `sum_bends` 与 `crossings` 的权衡可调（`route_w_bend` / `route_w_cross`）

### 刻意不做

不上真 MCF；不做 edge bundling；不做 octilinear。这些明确不挡主路径闭环（`roadmap.md` §7 原则 2）。

---

## 8. 需要先裁定的问题

这四条来自 R§8，**每条都挡住一个具体阶段**，需要在进入该阶段前给出答案：

| # | 问题 | 挡住 | 建议默认 |
|---|------|------|----------|
| 1 | **对称 vs 图宽的兑换率**：`λ_sym` 默认值？对称拉满会让 DAG 显著变宽 | P4-S3 | 先 `λ_sym = 1.0` 跑基线，按 `bbox_w` 涨幅上限（+15%）反推 |
| 2 | **多父节点归谁**：被两把扇同时要求居中时，(a) 折中 / (b) 按边权归一把 / (c) 按声明序归 | P4-S2 | (a) 折中；需写进 `expectations.md` §6 |
| 3 | **非矩形节点的端口**：继续用 bbox 边（起点悬空）还是按形状轮廓取点 | 未排期 | 影响 `AlongSpec` 契约，得先定再动 |
| 4 | **组矩形不干净时硬失败还是回退** | P2-4 | 先「回退 + relaxation」跑一轮，看 P2 之后剩余回退数再定 |

问题 1 和 2 建议在 P2 结束时一并提出，这样 P4 开工时不会卡住。

---

## 9. 与 roadmap 的对应关系

本文的 P0–P5 是**债务维度**，`roadmap.md` 的 A–E 是**能力维度**，两者正交：

| 本文阶段 | 对应 roadmap | 关系 |
|----------|--------------|------|
| P0 · 止血 | 无 | 纯债务，不推进任何能力 |
| P1 · 观测 | 阶段 C（诊断出口）的延续 | C 立了 relaxations 通道但无生产方，P1 补上 |
| P2 · 组与定序 | **D₂（组框 + Gate）的前置** | D₂ 想做的「组框进求解 / 跨组只经 Gate」，前提是包围盒能纯 |
| P3 · 性能 | 无 | 纯债务 |
| P4 · 对称内核 | **§9 backlog「SymmetryAxisWriter」的终局** | §9 已预警熔断点，P4 是兑现 |
| P5 · 路由写权 | **D₁ 的收口** | D1.0–D1.3 已交付，P5 补 Substrate 节点占位与 Ink 写权 |

`roadmap.md` §5 说「D₁ 与 D₂ 不要并行开两条」。本计划的 P2 → P5 顺序与之相容：先把 D₂ 的地基（组结构）打好，再回头收口 D₁。

阶段 E（StrongMacro / PartitionGrid / labeling）不在本计划范围内。

---

## 10. 全程不做清单

跨阶段的红线，任何时候都不破：

1. **不加图名 / diagram type 特判**（ADR-001）
2. **不在 Ink 发明几何**——修楼梯先问 Metric 或 Channel
3. **不加第三个守卫**——若某个自由度需要第三层排除谓词，说明写者选错了，上提写者
4. **不引入 `HashMap` 迭代序依赖**
5. **不用墙钟超时改变布局结果**
6. **不留 deprecated 转发层**（无向后兼容包袱，直接删）
7. **不留半截抽象**——`GateCapacity::Fixed` 这类「有类型没生产者」的东西，要么接上要么删掉
8. **不为让某个 fixture 好看而调死常数**——调的必须是 typed param，且要能解释为什么这个方向对所有图都成立

---

## 11. 附：任务总数与预估

| 阶段 | 任务数 | 预估 | 几何是否变化 |
|------|--------|------|--------------|
| P0 | 5 | 1 天 | 是（P0-1 排序语义 + P0-3 快照） |
| P1 | 3 | 1 天 | **否**（硬性要求） |
| P2 | 5 | 3–5 天 | 是（大幅） |
| P3 | 4 | 2–3 天 | P3-1/2/3 否，P3-4 是 |
| P4 | 5（S1–S4 + layer_alignment） | 4–5 天 | S1/S2/S4 否，S3 是 |
| P5 | 6 | 5–7 天 | 是 |

**合计约 16–22 天**。P0+P1 共 2 天，建议连着做完再决定后续投入。
