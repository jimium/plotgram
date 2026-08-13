# Hierarchical · 次轴对称（目标函数）

> 父页：[architecture](../architecture.md) §5 · 视觉裁定：[expectations §6.1](../expectations.md)  
> 上游：BK ideal（[`metric/bk.rs`](../../../../crates/plotgram-layout/src/layout/hierarchical/metric/bk.rs)）  
> 下游：无 — 本写者即次轴终局（[`metric/cross_axis.rs`](../../../../crates/plotgram-layout/src/layout/hierarchical/metric/cross_axis.rs) → [`symmetry_objective.rs`](../../../../crates/plotgram-layout/src/layout/hierarchical/metric/symmetry_objective.rs)）  
> 状态：**P4 已落地**（声明表删除；组边界细则仍后续）

## 1. 要解决什么

主链节点中心共线与扇出几何居中，是**同一条次轴对称**的两个面。  
旧路径用 `SymmetryPlan`（轴 / 刚体列 / FanPack）+ 贪心 `claimed` 在树上精确、在 DAG/多父上塌陷，且只能靠再加谓词修补（roadmap 熔断点）。

现行写者最小化可观测目标 **`J(x)`**，硬约束只保留层内分离与 VV 共线；产品规则（主臂 / twin 占脊、扇叶铺开）进**权重与终局 snap**，不进约束循环里的 ad-hoc continue。

**写什么**：每个 plan elem 的 **次轴中心**（TB 下为 `x`）。  
**不写**：端口 `side`、脸内 along、轨坐标 —— 那些分别属 Compose / PortLane / TrackOrder。Compose 已定的 ports 只被消费：把链端 dummy 的 soft desired 拉到 `port_anchor.x`。

**共线说的是端口，不是中心**。一条边直不直，取决于两端 `port_anchor.x` 是否相等；节点若在一张脸上挂了好几个端口，就得用自己的中心去付这个偏移。于是段的代价项是

```text
J_edge = Σ_seg w · |(x_from + off_from) − (x_to + off_to)|
```

`off` = 该边在该端的槽位相对中心的偏移 `((order+1)/(count+1) − 1/2) · width`；虚元与 E/W 端口取 0（E/W 边横着走，x 上没有可对的列）。同一条纪律进 `snap_fan_pack_style` 的共线语句：主臂 / twin / 脊链上的 `desired[peer] = axis` 改成 `axis + (off_hub − off_peer)`，链上逐跳累加。

因此 **「主臂占脊」的门禁断言的是边的两个端口列，不是两个节点中心**（`order_approval` 门禁已按此改写）。

## 2. 写者边界

| 自由度 | 写者 | 不得 |
|--------|------|------|
| BK 四候选合并 ideal | `bk_ideal` | 兼任对称终局 |
| **次轴中心 `x[e]`** | **`solve_symmetry_objective`** | Channel / Ink / 端口相改写 side |
| 层内分离、VV 共线、（可行时）twin 跨层等式 | VPSC 硬约束 | `if even_fan` / 图名特判 |
| 扇叶槽 / 主臂·twin 占脊 / spine | 终局 snap 展开 J 已有的相对项；IPSEP 不再 1e6 绝对锁 | 第三趟 ad-hoc 拉回 |
| 折线像素 | Ink | 发明列位 |

```text
BK ideal
  → VPSC（可行初值）
  → K 轮 IPSEP：无约束 L2 步（邻域重心，twin/主臂/exclusive 脊带 boost + hub→center_h）→ VPSC 投影这一步的 x；J 更优则 snapshot
       （`symmetry_place: median` 仍走旧的 weighted-median desired 装箱）
  → snap（叶槽 / hub 扇心 / exclusive 脊 / exteriorize dummy；IPSEP 不 1e6 锁 twin/primary）
  → VPSC → 返回 x[]
```

## 3. 目标与约束

```text
J(x) = Σ_seg w_uv · boost_uv · |(x+off)_u − (x+off)_v|
     + λ_sym · Σ_hub |x_h − center_h(x)|
boost_uv = twin_spine_boost | chain_end_boost | primary_arm_boost
           （exclusive 1:1 计入；两端都是扇 hub 的 min-span 子不加；
             chain_end = 悬挂汇点↔长边 dummy，父不是 span-1 扇 hub）

硬约束：层内分离（node_gap）+ VV dummy 链共线
         + 链恒等：长边（≥2 颗 dummy）相邻 dummy 双向 0-gap 等式，
           整链绑成一个 VPSC 变量（yfiles/01 §4.5 linear segment）
         + twin 跨 rank 等式（不可行则丢掉等式，保留高 soft 权重）

降级链四档：链恒等骑前三档（随 twin / clamp 等式逐级松绑），末档去掉链恒等——
走廊过刚只降级，不冒 `Infeasible`。
```

| 参数 | 默认 | 作用 |
|------|------|------|
| `lambda_sym` | `1.0` | hub 贴扇心强度 |
| `twin_spine_boost` | `8.0` | 2-cycle 对端占脊（进 J / L2，不再靠 1e6 desired） |
| `primary_arm_boost` | `4.0` | 最短跨主臂与 exclusive 1:1 脊进 J；两端都是扇 hub 则不加 |
| `chain_end_boost` | `8.0` | 悬挂汇点跟长边廊（父非 span-1 扇 hub）；snap：叶跟廊；恰好一端是扇 hub 时非 hub 写整链（§12 E） |
| `symmetry_iters` | `8` | IPSEP / 中位迭代轮数 |
| `symmetry_place` | `ipsep` | 主路径 = 对 J 的 L2 无约束步 + VPSC 投影（§12 A1）。`median` = 旧 desired-packer；`bk` = A0 诊断（BK ideal + 硬约束，跳过迭代与 fan snap）。后两档**不是产品档**。 |
| `layer_alignment` | `0.5` | 主轴：实节点在层带内对齐；**零高 elem 钉层带顶边**（避免中心走廊穿同层节点） |

边权基：`real–real=1` / `real–virt=2` / `virt–virt=8`（作者 `weight` 乘基；`critical: true` 糖 = `2.0`）。  
`center_h`：该 hub 向下/上扇叶的中位或两中位中点（`axis_from_neighbors`）；多父取折中（邻位加权，非先到先得认领）。

**邻接真源**：`RealGraph` **正向** real 端点（长边一跳；**reversed 不计扇**）。辅助谓词仍在 [`metric/symmetry.rs`](../../../../crates/plotgram-layout/src/layout/hierarchical/metric/symmetry.rs)（`forward_real_adjacency` / `twin_plan_pairs` / `unique_min_span_primary` / `fan_pitch` / `slot_multipliers`）。

## 4. 终局 snap（展开，非新自由度）

在最佳 `J` 快照上写 soft desired 再 VPSC 一次（必要时二次 spine reclaim）：

1. hub → `center_h`（纯扇）；IPSEP 下**连着 dummy 链的 hub**保持 J 的列，不拽回子心（否则 mech n5 离开 e30）；  
2. ≥2 自由叶：`axis + slot_multipliers · pitch`（层内序；FanPack / D3）；  
3. 恰 1 自由叶：IPSEP **保持 J 的列**（`node_gap` 只当分离下限）；median 档仍 `axis ± pitch`；  
4. twin / 主臂 peer → 轴（**仅 `symmetry_place: median`** 仍 1e6 锁；IPSEP 主路径把相对共线写进 J，见 §12 A2）；  
5. exclusive 1:1 spine / follower 跟列；  
6. port-anchor 覆盖链端 dummy，再 `exteriorize` 同层 dummy。

链列单写者（yfiles/01 §4.5）分两层：约束层用链恒等把 ≥2 颗 dummy 焊成一个 VPSC 变量；desired 层只在写者无歧义时把**整条链**钉到一端的 `port_anchor.x`——恰好一个非叶端，或两端都非叶且至少一端是扇 hub、≥2 颗 dummy。恰好一端是 hub → **非 hub**写廊；两端都是 hub → **本层更靠边缘**的那端写廊（mech e18 的 n25，避免折中列把 L2 整层顶开）。两端都不是 hub、或只有一颗 dummy，仍逐端写入。悬挂汇点叶不反向拽廊，而是被拉到该 dummy 列。两端等权折中会把焊死的变量停在中间列；把「离链更近」的那端当单写者会漂 D2 脊；把 port-anchor 放进 IPSEP 迭代会让整图无界左漂。

纯扇场景下，snap 与旧 FanPack 槽位同构；DAG/多父靠 `J` + 权重折中，不再 `claimed`。

## 5. 与声明表时代的关系

| | SymmetryPlan（已删） | 现行 |
|--|----------------------|------|
| 形态 | 三张表 + pass1/pass2 | `J(x)` 迭代 + snap |
| DAG 多父 | 先到先得 `claimed` | 目标折中 |
| 扩展 | 再加谓词 / continue | 改 `J` / 权重 / snap |
| 代码 | ~1300 行 `symmetry.rs` 表逻辑 | helpers + `symmetry_objective.rs` |

历史动机与熔断预警见 [notes/2026-08-08-hier-review.md](../notes/2026-08-08-hier-review.md) §2.3 / §4.3、[improvement-plan P4](../notes/2026-08-08-hier-improvement-plan.md)。

## 6. 验收门禁

| 门禁 | 覆盖 |
|------|------|
| `hier_eval::symmetry_axis_d2_*` | 代表图主链共线 |
| `symmetry_axis_d3_fan_pack_multi_rank_backedge` | 跨层扇 + 回边不算扇 |
| `twin_spine_constrain_sink_*` / PortLane 相关 | twin 占脊 + 外叶序 |
| `order_approval` / `ticket_triage_*` | 主臂占脊 / 侧廊 |
| `hier_eval` 基线 | bends / crossings / `symmetry_deviation_*` 观测 |

## 7. 刻意不做

- 图名 / profile 特判；  
- Channel / Ink 回写次轴或 side；  
- VPSC 中点硬等式 / 第三趟 ad-hoc；  
- 组框进 VPSC（P2-3，组专项）；  
- 以「恢复 FanPack 声明表」为回退终态。

## 8. 失败语义

- VPSC 不可行 → `InfeasibleConstraint`（twin 等式可降级后重试；层内分离 + VV 仍硬）；  
- 不得静默图名特判或 Ink 抹坐标。
