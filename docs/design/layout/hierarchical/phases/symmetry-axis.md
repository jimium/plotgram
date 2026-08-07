# Hierarchical · 次轴对称轴（SymmetryAxis）

> 父页：[architecture](../architecture.md) §5 · 视觉裁定：[expectations §6.1](../expectations.md)  
> 上游：BK ideal（[`metric/bk.rs`](../../../../crates/plotgram-layout/src/layout/hierarchical/metric/bk.rs)）  
> 下游：VPSC 约束与 pass-2 desired（[`metric/cross_axis.rs`](../../../../crates/plotgram-layout/src/layout/hierarchical/metric/cross_axis.rs)）  
> 状态：**D3 已落地**（轴 + 刚体列 + FanPack 闭包；组边界截断细则仍留后续）

## 1. 要解决什么

主轴节点中心共线与扇出几何居中，是**同一条次轴对称**的两个面。  
把「对称轴」寄生在 BK median 焊点、或用「凡 fan≥2 松开 + 只拉度数=1」补丁，都会在扇出结处打出阶梯折（`flat-rest-api` / `constrain-flat-chain` / `order-approval`）。  
只焊 hub 到 pass-1 扇心、扇叶仍吃 BK ideal，则终态扇心漂移、一叶塌脊一叶飞边（`smoke.multi-rank-backedge`）。

本页把对称决策上提为 Metric 内显式写者 **SymmetryAxisWriter**，对应 roadmap 阶段 A 熔断句：刚体成员资格不得继续以约束循环里的 ad-hoc continue 堆积。

## 2. 写者边界

| 自由度 | 写者 | 不得 |
|--------|------|------|
| BK 四候选合并 ideal | `bk_ideal` | 兼任对称轴真源 |
| **SymmetryAxis** / **RigidColumnClass** / **FanPack** | **SymmetryAxisWriter**（本页） | Channel / Ink / 端口相改写 |
| 层内分离 gap、class 内硬共线 | `build_constraints`（**只读**成员表） | 内嵌 `even_fan` / `deg>=2` 特判 |
| pass-2 desired | 消费 Axis / FanPack 表 | 只拉度数=1 的隐式 follower 逻辑作为终态 |
| 折线像素 | Ink | 发明列位 |

```text
Pass-1 (BK ideal + soft VPSC)
    → SymmetryAxisWriter
         SymmetryAxis[fan_hub]
         RigidColumnClass[class_id] → {elem…}
         FanPack[hub] → slots (axis ± k·pitch)
    → Constraints（读 class 表）+ Desired（轴 + FanPack）
    → Pass-2 VPSC
```

## 3. 输出表

### 3.1 SymmetryAxis

```text
SymmetryAxis
  hub: ElemId          // 扇结（单侧 real 邻居 ≥ 2）
  side: Down | Up      // 扇朝哪一侧（双侧扇则两侧各算再合并）
  coord: f64           // 次轴目标（canonical TB 的 cross）
```

**轴怎么算**（不依赖 BK 焊点）：

1. 若该侧邻居含**双胞胎**对端：`coord =` hub 自身的 pass-1 中心（守脊）；  
2. 否则取该侧 real 邻居按 pass-1 横坐标排序（平局 elem index）：  
   - **奇数**个：`coord =` 中位邻居的 pass-1 中心；  
   - **偶数**个：`coord =` 两中位邻居中心的中点；  
3. 若 Down 与 Up 皆扇：`coord =` 两侧轴的均值。

奇偶只影响**无 twin 时的轴公式**，不分别决定「硬/软」散落在约束循环里。

### 3.2 RigidColumnClass

```text
RigidColumnClass
  id: ClassId
  axis_hub: ElemId     // 关联哪把 SymmetryAxis
  members: sorted ElemId[]
```

**成员谓词（构造时一次算清）**：

1. **必含** `hub`；  
2. 沿**非扇侧**走 forward real–real 1:1 链（见下「邻接真源」）：当前节点在路径前进方向上保持链状邻接，则整段加入同一 class  
   - 例：`Gateway → API → Worker(扇向下)` → `{Gateway, API, Worker}` 同 class（API 度数=2 也必须进，不能只收度数=1）；  
3. **截断**（升格后的链拖拽 / 边界守卫，不再是第三个 continue）：  
   - 下一节点是另一扇结——**例外（向下）**：`down_deg[cur]==1` 且下一 real 满足 `up_deg==1` 且为扇 hub 时，**将该子扇 hub 收入本 class（同轴）后停止**（不收扇叶；上定心）；  
   - 节点为 dummy-aligned（median 抢过 dummy 的 real）；  
   - 组边界策略点（后续接 group band；本期可先截断）；  
4. **扇的孩子不进 class**（避免扇叶塌向轴；叶位由 FanPack 写）——**例外**：与 hub 构成**双胞胎**（同无向端点对上既有正向边又有 reversed 边）的 forward 邻居**进入 class**，钉在轴上；**同一 rank 至多一个** twin（同层分离 gap 与零间隙共线不可并存；多 twin 时取 pass-1 距 hub 最近者）。对齐 yFiles：回环对端占脊，其余叶侧置（`mech.constrain-sink`）。

**轴坐标（上定心）**：hub 按 `(rank asc, elem index)` 处理。若 `up_deg==1` 且唯一上游 U 已有轴（U ∈ 某 RigidColumnClass，或 U 已是 axis hub）→ **继承该轴**；否则奇/偶邻居公式（含 twin 侧守 pass-1）。偶扇「两中位中点」**只用于尚无上游轴可继承的 hub**。**禁止**用下游叶中点反拉上游汇入结。

**邻接真源**：扇判定与 walk 读 `RealGraph` **正向**边的 real 端点（跨层长边仍算一跳，dummy 不藏扇）；**reversed 回边不计入**（避免环头被误判为扇）。

同一 elem 若被多把轴争抢：稳定规则取 **更小 hub elem index**（同 rank 时；整体先上后下），禁止静默双写。已被上游 class 吸收的子扇 hub 不再另开刚体列，仍可写 FanPack。

### 3.3 FanPack

```text
FanPackSlot
  elem: ElemId
  desired: f64         // 绝对次轴目标 = axis.coord + offset

FanPack
  hub: ElemId
  slots: FanPackSlot[] // 该 hub 一侧或多侧扇叶
```

**槽位怎么算**（构造时一次算清）：

1. 取 hub 该侧 forward 邻居中**尚未进 RigidColumnClass** 的叶（双胞胎已占脊，不进本表），按 **Compose 层内序**排序；跨层按 `(rank, layer_order, elem)`；  
2. 该侧若含双胞胎：`axis.coord =` hub 的 pass-1 次轴（守脊）；否则仍用奇/偶邻居公式；  
3. **≥2** 自由叶：奇扇中位 `0`、两侧 `±k·pitch`；偶扇 `±0.5, ±1.5, …`；  
4. **恰 1** 自由叶（脊已被 twin 占用）：`desired = axis ± pitch`（符号取 pass-1 相对轴的侧）；  
5. `pitch = max(node_gap + 相邻半宽和, pass-1 相邻叶距中位数下界)`（单叶时用 hub/叶半宽 + gap）；  
6. `desired = axis.coord + offset`；叶争抢与 class 同构；叶下 exclusive 1:1 下游跟列。

回边 dummy 不进 FanPack（仍走 port-anchor）。

## 4. 约束与 desired 怎么消费

### 4.1 Constraints

- 层内相邻：分离 gap（不变）；  
- **class 内**任意需共线的成员对：对零 gap 硬共线；  
- 非 class 的 BK primary 同型 1:1 对：仍可硬共线；**不同 FanPack 槽位之间不焊**（防镜像塌缩）；同槽位叶+下游跟列可硬共线；  
- dummy–dummy 链：保持现有硬/软策略；  
- **禁止**：`if even_fan` / `if deg >= 2 { soft }` 写在 `build_constraints` 内；  
- FanPack **不**另开硬共线家族（镜像由 soft desired 构造）；同层 dummy 的 port-anchor soft 目标须落在 FanPack 叶序外侧，避免高权重虚节点把叶挤塌。

### 4.2 Desired（pass-2）

1. 基底：`bk.ideal`；  
2. class 内每个成员：`desired[e] = SymmetryAxis[axis_hub].coord`；  
3. FanPack 扇叶：`desired[e] = slot.desired`（**覆盖 BK ideal**；扇叶本不进 class，与第 2 步互斥）；  
4. 端口锚点拉 dummy：仍在 Metric 内展开，不得拖动 class 外的无关 real（链拖拽已由成员资格截断）。

## 5. 与早期奇偶分治的关系

| | 早期奇偶 | 本方案 |
|--|----------|--------|
| 奇扇轴 | 希望 = median 列 | 显式用中位孩子 pass-1 中心，**不**信 BK 焊点 |
| 偶扇轴 | pass-2 两中位中点 | 无上游轴时两中位中点；**有唯一上游轴则继承** |
| 主轴共线 | 奇扇靠硬锁；偶扇靠结移动 | **整条 RigidColumnClass 跟轴**（含向下吸收唯一子扇 hub） |
| 扇叶镜像 | 无 / 靠 BK 偶然 | **FanPack 槽位覆盖 BK** |
| 扩展 | 再加守卫 → 约束循环 continue | 改成员谓词 / FanPack / 截断条件 |

## 6. 分阶段落地

| 阶段 | 内容 | 验收 |
|------|------|------|
| **D0 文档** | 本文 + [expectations §6.1](../expectations.md) | 裁定无「脊/扇二选一」 |
| **D1 表 + 消费** | 实现 SymmetryAxisWriter；`build_constraints` / desired 只读表；删除凡 fan≥2 散落逻辑 | **已落地**（`metric/symmetry.rs` + `cross_axis` 双约束消费）；单测：奇/偶轴公式、class 含多跳上游链、dummy-aligned 截断 |
| **D2 代表图** | 刷 `hier_eval`；目视三图主链无阶梯折 | **已落地**：`flat-rest-api` / `constrain-flat-chain` / `order-approval` 主链共线门禁（`hier_eval::symmetry_axis_d2_*`） |
| **D3 FanPack** | 扇叶相对轴对称铺开；desired 覆盖 BK；删终态扇心漂移逃逸口 | **已落地**：`hier_eval::symmetry_axis_d3_fan_pack_multi_rank_backedge`；组边界细则仍后续 |

实现入口：[`metric/symmetry.rs`](../../../../crates/plotgram-layout/src/layout/hierarchical/metric/symmetry.rs)（Writer）+ [`metric/cross_axis.rs`](../../../../crates/plotgram-layout/src/layout/hierarchical/metric/cross_axis.rs)（消费）。

## 7. 刻意不做

- 图名 / profile 特判；  
- Channel 回写列位或端口 side；  
- Ink 发明对称；  
- 以「只恢复 `even_fan` 谓词」作为终态；  
- VPSC 中点硬等式 / pass-2 后第三趟 ad-hoc 拉回；  
- 本期组边界 / group band 截断细则。

## 8. 失败语义

- 成员表自相矛盾（同一对既要求硬共线又跨截断）→ `InternalInvariant`；  
- VPSC 不可行 → `InfeasibleConstraint`（与现 cross_axis 一致），不得静默降级为特判。
