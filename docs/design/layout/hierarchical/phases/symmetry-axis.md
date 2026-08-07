# Hierarchical · 次轴对称轴（SymmetryAxis）

> 父页：[architecture](../architecture.md) §5 · 视觉裁定：[expectations §6.1](../expectations.md)  
> 上游：BK ideal（[`metric/bk.rs`](../../../../crates/plotgram-layout/src/layout/hierarchical/metric/bk.rs)）  
> 下游：VPSC 约束与 pass-2 desired（[`metric/cross_axis.rs`](../../../../crates/plotgram-layout/src/layout/hierarchical/metric/cross_axis.rs)）  
> 状态：**D2 已落地**（D1 Writer + 三图主链共线门禁；D3 扇叶相对轴可选）

## 1. 要解决什么

主轴节点中心共线与扇出几何居中，是**同一条次轴对称**的两个面。  
把「对称轴」寄生在 BK median 焊点、或用「凡 fan≥2 松开 + 只拉度数=1」补丁，都会在扇出结处打出阶梯折（`flat-rest-api` / `constrain-flat-chain` / `order-approval`）。

本页把对称决策上提为 Metric 内显式写者 **SymmetryAxisWriter**，对应 roadmap 阶段 A 熔断句：刚体成员资格不得继续以约束循环里的 ad-hoc continue 堆积。

## 2. 写者边界

| 自由度 | 写者 | 不得 |
|--------|------|------|
| BK 四候选合并 ideal | `bk_ideal` | 兼任对称轴真源 |
| **SymmetryAxis** / **RigidColumnClass** | **SymmetryAxisWriter**（本页） | Channel / Ink / 端口相改写 |
| 层内分离 gap、class 内硬共线 | `build_constraints`（**只读**成员表） | 内嵌 `even_fan` / `deg>=2` 特判 |
| pass-2 desired | 消费 Axis 表改写 class 成员 | 只拉度数=1 的隐式 follower 逻辑作为终态 |
| 折线像素 | Ink | 发明列位 |

```text
Pass-1 (BK ideal + soft VPSC)
    → SymmetryAxisWriter
         SymmetryAxis[fan_hub]
         RigidColumnClass[class_id] → {elem…}
    → Constraints（读表）+ Desired（轴坐标）
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

1. 取 hub 该侧 real 邻居，按 pass-1 横坐标排序（平局 elem index）；  
2. **奇数**个：`coord =` 中位邻居的 pass-1 中心；  
3. **偶数**个：`coord =` 两中位邻居中心的中点；  
4. 若 Down 与 Up 皆扇：`coord =` 两侧轴的均值。

奇偶只影响**轴公式**，不分别决定「硬/软」散落在约束循环里。

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
   - 下一节点是另一扇结；  
   - 节点为 dummy-aligned（median 抢过 dummy 的 real）；  
   - 组边界策略点（一期可先截断；二期接 group band）；  
4. **扇的孩子不进 class**（避免扇叶塌向轴）。

**邻接真源**：扇判定与 walk 读 `RealGraph` **正向**边的 real 端点（跨层长边仍算一跳，dummy 不藏扇）；**reversed 回边不计入**（避免环头被误判为扇）。

同一 elem 若被多把轴争抢：稳定规则取 **声明序更早的 hub**（或 `EdgeId` 更小的扇边），并进 diagnostics warning；禁止静默双写。

## 4. 约束与 desired 怎么消费

### 4.1 Constraints

- 层内相邻：分离 gap（不变）；  
- **class 内**任意需共线的成员对：对零 gap 硬共线；  
- 非 class 的 BK primary 同型 1:1 对：仍可硬共线；  
- dummy–dummy 链：保持现有硬/软策略；  
- **禁止**：`if even_fan` / `if deg >= 2 { soft }` 写在 `build_constraints` 内。

### 4.2 Desired（pass-2）

- class 内每个成员：`desired[e] = SymmetryAxis[axis_hub].coord`；  
- 扇叶：不因本步改写（相对轴的对称铺开 = **二期**）；  
- 端口锚点拉 dummy：仍在 Metric 内展开，不得拖动 class 外的无关 real（链拖拽已由成员资格截断）。

## 5. 与早期奇偶分治的关系

| | 早期奇偶 | 本方案 |
|--|----------|--------|
| 奇扇轴 | 希望 = median 列 | 显式用中位孩子 pass-1 中心，**不**信 BK 焊点 |
| 偶扇轴 | pass-2 两中位中点 | 同上，写入 Axis 表 |
| 主轴共线 | 奇扇靠硬锁；偶扇靠结移动 | **整条 RigidColumnClass 跟轴** |
| 扩展 | 再加守卫 → 约束循环 continue | 改成员谓词 / 截断条件 |

## 6. 分阶段落地

| 阶段 | 内容 | 验收 |
|------|------|------|
| **D0 文档** | 本文 + [expectations §6.1](../expectations.md) | 裁定无「脊/扇二选一」 |
| **D1 表 + 消费** | 实现 SymmetryAxisWriter；`build_constraints` / desired 只读表；删除凡 fan≥2 散落逻辑 | **已落地**（`metric/symmetry.rs` + `cross_axis` 双约束消费）；单测：奇/偶轴公式、class 含多跳上游链、dummy-aligned 截断 |
| **D2 代表图** | 刷 `hier_eval`；目视三图主链无阶梯折 | **已落地**：`flat-rest-api` / `constrain-flat-chain` / `order-approval` 主链共线门禁（`hier_eval::symmetry_axis_d2_*`）；扇叶相对轴重打包属 D3 |
| **D3（可选）** | 扇叶相对轴对称铺开；组边界细规则 | 密扇出宽度与交叉门禁 |

实现入口：[`metric/symmetry.rs`](../../../../crates/plotgram-layout/src/layout/hierarchical/metric/symmetry.rs)（Writer）+ [`metric/cross_axis.rs`](../../../../crates/plotgram-layout/src/layout/hierarchical/metric/cross_axis.rs)（消费）。

## 7. 刻意不做

- 图名 / profile 特判；  
- Channel 回写列位或端口 side；  
- Ink 发明对称；  
- 以「只恢复 `even_fan` 谓词」作为终态（可作 D1 过渡，但必须尽快换成成员表）。

## 8. 失败语义

- 成员表自相矛盾（同一对既要求硬共线又跨截断）→ `InternalInvariant`；  
- VPSC 不可行 → `InfeasibleConstraint`（与现 cross_axis 一致），不得静默降级为特判。
