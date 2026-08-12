# mech.layout-styles 对标 yFiles：现状评审与收敛方案

日期：2026-08-12 · 基准 fixture：`apps/showcase/hierarchical/flat/mech.layout-styles.pgm`（27 节点 / 34 边，拓扑转写自 yFiles `LayoutStyles.graphml`）

> 本文是 notes（分析与提案），**不是契约**。裁定权仍在 [`expectations.md`](../expectations.md) 与 [`phases/`](../phases/)；若采纳需按 §5 同步改契约文档。

---

## 1. 结论摘要

1. **rank 与层内 order 已经与 yFiles 逐层逐位完全一致**（§2.1）。差距 100% 落在 **cross-axis 坐标** 与 **端口面 / 走廊角色** 两处，`compose/order.rs` 不需要再动。
2. 当前工作区未提交的 `ports.rs` 改动**方向对了一半**：`side_corridor_polarity` 用层内归一化位置替代裸 `order` 是实质修正（e29 从 6 折 1072 长降到 2 折 399）；但新增的 `forward_gutter_side` 是**为掩盖 cross-axis 缺陷而在下游加的第三趟特判**，且把 e18 推到了与 yFiles 相反的方向。建议**保留前者、撤掉后者**（§3）。
3. yFiles 在这张图上 **一个 E/W 端口都没有**：34 条边全部 South 出 / North 入，两条回边（`n21→n5`、`n14→n11`）走 N/S 主脊的 dummy 链，各 0 折点。我们现行契约（`channel-corridor-allocator.md` §6.3「span≥2 → cross_axis(E/W)」）与该参考直接冲突（§2.3、§4.1）。
4. 最大的一笔分在 **长边 dummy 走廊与真实端点共线**。yFiles 有 9 条 0 折点边，我们只有 3 条；我们有 5 条 4 折点边、1 条 6 折点边，yFiles 一条都没有（max_bends = 2）。
5. **当前 `develop` HEAD 本身是红的**（`hier_eval` 12 项 hard invariant 违规），未提交改动是净改善但仍未转绿。任何后续优化在转绿之前都无法用门禁验证 —— 这是 P0。

---

## 2. 事实基线

### 2.1 rank / order：已与 yFiles 完全一致

从图 1 的节点几何反推 yFiles 的层与层内序（节点无标签，靠边拓扑唯一确定映射），与我们 `debug-layout` 的输出逐位对照：

| 层 | yFiles 层内序（左→右） | plotgram 层内序（仅 real） | 一致 |
|----|------------------------|----------------------------|------|
| L0 | n18 | n18 | ✓ |
| L1 | n19 | n19 | ✓ |
| L2 | n26, n5, n0, n24 | n26, n5, n0, n24 | ✓ |
| L3 | n25, n6, n23, n9, n1, n8 | n25, n6, n23, n9, n1, n8 | ✓ |
| L4 | n10, n22, n2, n4, n11 | n10, n22, n2, n4, n11 | ✓ |
| L5 | n15, n20, n3, n13 | n15, n20, n3, n13 | ✓ |
| L6 | n16, n21, n7, n12 | n16, n21, n7, n12 | ✓ |
| L7 | n17, n14 | n17, n14 | ✓ |

每层节点数 1/1/4/6/5/4/4/2 也一致。**Sugiyama 前两阶段（rank + order）已达标，后续改动不应触碰 `compose/rank.rs` / `compose/order.rs`。**

### 2.2 量化差距

| 指标 | 记录基线<br>(`hier_eval_baseline.json`) | HEAD<br>(16f301f) | 工作区<br>(未提交改动) | 探针<br>(回边改 N/S，§4.1) | **yFiles 目标** |
|------|------|------|------|------|------|
| crossings | 19 | 26 | **11** | 13 | **0** |
| sum_bends | 74 | 82 | **74** | 74 | **≈ 50** |
| max_bends | 4 | 6 | 6 | **4** | **2** |
| 0 折点边数 | — | — | 3 | 3 | **9** |
| E/W 端口边数 | — | 3 | 3 | 1 | **0** |
| 正交总长 | — | 6413 | **5538** | 5908 | — |

画布尺寸与相对间距不是问题：我们 562×687 / 节点 40 宽 ≈ 14 列宽，yFiles ≈ 12.4 列宽，同量级。

### 2.3 差距归因（逐边）

工作区当前 34 条边里，只有 **9 条**偏离 yFiles，且全部集中在 3 类根因：

| 边 | 我们 | yFiles | 根因 |
|----|------|--------|------|
| e30 `n21→n5` | W→W，6 折，长 580 | N→S，**0 折** | ① 回边被硬规则赶去侧廊 |
| e29 `n14→n11` | E→E，2 折，长 399 | N→S，**0 折** | ① 同上 |
| e18 `n18→n25` | W→W，2 折 | S→N，2 折 | ② `forward_gutter_side` 新特判 |
| e23 `n25→n15` | S→N，4 折 | S→N，**0 折** | ③ dummy 走廊未与端点共线（走廊 x=24，两端 x=53） |
| e6 `n11→n12` | S→N，4 折 | S→N，2 折 | ③ 走廊 x=488 vs 出针槽 x=476 |
| e32 `n9→n7` | S→N，4 折 | S→N，2 折 | ③ 走廊 x=185，两端 270 / 174 |
| e2 `n0→n4` | S→N，4 折 | S→N，2 折 | ③ 同上 |
| e5 `n6→n10` | S→N，2 折 | S→N，**0 折** | ③ n10 被 port anchor 拉到 68，父 n6 在 117 |
| e9 `n15→n16` | S→N，2 折 | S→N，**0 折** | ③ n16@46 vs n15@53，7px 微偏移即产生一次折 |

15 个几何交叉里，**e30 一条边独占 7 个**。

**根因 ①「幽灵列」值得单独点名**：`properify` 对所有 `span≥2` 边（含 reversed）都插了 dummy 链，e30 的三个 dummy 实实在在占着 L3/L4/L5 的 order 槽（x=161），把 n6/n23、n10/n22、n20/n3 顶开；但 `pick_reversed_side` 随后把这条边判给了 W 侧廊，走 x≈18 的左外沿。**代价付了两次，收益一次都没拿到** —— 这是当前布局横向被撑开、且左半区交叉密集的直接原因。

---

## 3. 对未提交改动的评审

改动范围：`compose/ports.rs`（+91/−26）、`tests/hier_eval.rs`、`phases/ports-and-channel.md`。

### 3.1 应当保留

**`side_corridor_polarity` 改用层内归一化位置。** 这是一处真实的量纲错误修正：跨层比较裸 `order` 索引在层宽不同时无意义（注释里 n14 order 1/2 vs n11 order 4/5 的例子准确）。效果可测：e29 从 `W→W / 6 折 / 长 1072` 变为 `E→E / 2 折 / 长 399`，全图 crossings 26→11。

同时它修好了 `product.ticket-triage`（HEAD 上 max_bends 4 / sum 18 → 恢复到基线 2 / 16）。测试断言从 East 翻成 West 是**跟随修复、不是迁就实现**：`expectations.md` 只规定「闭环走侧廊」，未钉极性；渲染确认 escalate→handle 走左侧廊是 2 折干净路径。这个断言翻转可以接受。

### 3.2 应当撤掉：`forward_gutter_side`

这是本次改动最需要商榷的部分，四条理由：

1. **违反 AGENTS.md §1「声明表堆谓词先问目标函数」。** `pick_reversed_side` 已经是一张四行认领表，本次又在它前面挂了一张带三个魔法阈值（`0.15` / `0.25` / `0.75`）的新表。这些阈值没有量纲、没有推导，只是拟合了 mech 的 e18 一条边。判断句正好命中 AGENTS.md 的反例：「修法若是『再加一个特判』，先问这个自由度的写者应该是谁」。

2. **它是在下游掩盖上游缺陷。** e18 在 HEAD 上是 `S→N / 4 折`，yFiles 是 `S→N / 2 折`。多出来的两折来自 cross-axis：走廊 x=75，而 n25 中心 53、面宽只有 33–73，走廊差 2px 就落在面外，于是多一次 jog。yFiles 的走廊 x=122、n25 面宽 107–167，走廊落在面内，末端 jog 被端口槽吸收掉。**正确的修法是让端口槽吸附走廊 x（§4.2 P3），不是把整条边改判去侧廊。** 现在的做法把 4 折降到 2 折，代价是端口面与 yFiles 相反、且左外沿多一条纵向长廊。

3. **对全局收益极小、耦合极大。** 在 mech 上它只改变了 e18 一条边；但它作用于所有 `span≥2` 正向边，是 showcase 里数量最大的一类边，风险面远大于收益面。

4. **对称性靠巧合维持。** `forward_gutter_side(a, b)` 必须对两端给出同一个 Side（否则边两端脸不一致），目前靠函数体内 `ar.min/max(br)` 恰好对称成立。这是一个隐式不变量，既没有注释也没有断言。若采纳类似机制，应显式写成「边级一次决议、两端读取」。

### 3.3 其它评审意见

- **量纲修正只做了一半。** `pick_reversed_side` 里 `delta_order = own_order.abs_diff(peer_order)` 仍在跨层比裸 `order`，正是本次 commit 自己指出的错误。twin 短回边的 `Δorder≤1` 判据因此在层宽不等时不可靠。要么一并换成归一化位置，要么就别只修一半。

- **`layer_rightness` 的分母被 dummy 污染。** `plan.layers[rank].len()` 含 virtual / boundary / OrderPad。同一个真实节点，其 rightness 会随邻边长边数量漂移。若继续沿用，分母应只数**非零宽 elem**，或直接改用 §4.2 提到的 packed-x 估计。

- **单节点层返回 0.5 是个哑值。** 它让 `side_corridor_polarity` 掉进「同 rightness → 看 tip 半区」的兜底分支，而这个分支本身又是一条无依据的规则。单节点层的正确信息是「它在全图哪一半」，可由上下游邻居的 packed-x 估计给出。

- **死计算。** `cross_axis` 在第 151 行无条件求值，但第 153 行的 `if !reversed` 分支根本不用它。移到 `pick_reversed_side` 调用点之前即可。

- **契约文档不同步。** 只改了 `phases/ports-and-channel.md` §2 第 3 步；`phases/channel-corridor-allocator.md` §6.3「走廊角色表」仍是旧的裸 `order` 极性规则、也没有 `forward_gutter_side`。两份契约现在互相矛盾。

- **树是红的。** HEAD 已有 12 项 hard invariant 违规（`demo.k8s-*`、`demo.plotgram-core-mod-deps`、`demo.ai-agent-docops-pipeline` 的 bends 与 canvas-bloat，`stress.layout-stress-nested` 的 d1.3-gate 回退）。当前改动把违规项从 16 降到 13，但仍红。**在转绿之前不要再叠加优化**，否则无法区分改善与新伤。

---

## 4. 收敛方案

### 4.0 P0 — 先转绿（前置）

HEAD 的 `fix(hier): match yFiles layer order for layout-styles mech` 换来了正确的层内序，但连带把 6 个 group/demo 用例的 bends 与画布推出门禁。两条路选一条：

- **(a) 认账重录基线**：确认这些用例的视觉确实可接受（逐个渲染人工过目），把 `hier_eval_baseline.json` 与 `insta` 快照一起刷新，并在 commit message 里写清哪些指标退了、为什么可接受。
- **(b) 定位并修**：`demo.plotgram-core-mod-deps` 的 `sum_bends 70→103` 幅度异常（+47%），大概率与 order 变化后长边走廊落位有关，很可能与 §4.2 P2 是同一个根因。

推荐先做 (b) 的定位（成本低、信息量大），确认是同根因后合并到 P2 一起修；确实不是同根因的部分再走 (a)。

### 4.1 P1 — 走廊角色：从声明表换成目标函数

**问题定性。** 现行的 `pick_reversed_side` + `forward_gutter_side` 是一张按 `(reversed, span, has_twin, Δorder, rightness)` 分类的认领表，每来一个新反例就加一行。AGENTS.md §1 对这种形态的处方是明确的：改成**可最小化的 `J` + 少量硬约束 + typed 权重**。

**关键观察：Compose 阶段其实拿得到 x 的估计。** `metric/bk.rs::bk_ideal` 内部本就先做了一遍 left-to-right packing 得 `packed_centers`。把这一步提取成 `compose` 可复用的 `packed_x(plan) -> Vec<f64>`（order 定稿后即可算，纯函数、确定性），`rightness`、`Δorder`、`span` 这些互不可比的量就统一到同一个尺度上。

**提议的 `J_corridor(e)`。** 对每条边，在**边级**（不是逐端）比较两个候选角色：

```
J_spine(e) = w_jog · (|x̂_src − x̂_corr(first)| + |x̂_corr(last) − x̂_dst|)
           + w_share · (走廊列上已有的边数)

J_side(e)  = w_jog · (|x̂_src − x̂_gutter| + |x̂_gutter − x̂_dst|)
           + w_pierce · (侧廊纵向跨越的层里，被横穿的节点列数)
           + w_flow   · (与同层正向流互穿的估计条数)
```

其中 `x̂_corr(k)` 是该边 dummy 链第 k 个 dummy 的 packed-x，`x̂_gutter` 是所在半区外沿。取 `argmin`；平局按固定顺序（N/S 优先）保证确定性。

**这个 J 天然复现现有两条期望，不需要 span 硬规则：**

- mech `e30 (n21→n5, span=4)`：dummy 链已在 x̂≈161，两端 110 / 183 → `J_spine` 只有两次小 jog；走侧廊要从 110 横穿到左外沿再纵贯 4 层，`w_pierce` 项很大 ⇒ **选 N/S**，与 yFiles 一致。
- `product.ticket-triage` escalate→handle（短回边、无 dummy 链）：`J_spine` 的走廊就是主流程本身，`w_flow` 互穿项很大 ⇒ **选侧廊**，保住 `expectations.md` §3「闭环走侧廊」。
- `product.three-tier` / `user-auth` 的 twin 短回边：正向 twin 已占脊，`w_share` 让第二条并排，`J_spine` 仍最小 ⇒ **N/S 平行**，保住「平行折返」。

**写者上提（重要）。** 走廊角色只依赖 rank + FAS 结果 + 拓扑，**不依赖端口槽**，因此它的写者应该在 `properify` **之前**，而不是现在的 `assign_ports`（P7）。这样：

- 判给侧廊的边就**不插 dummy**，消灭 §2.3 的「幽灵列」；
- `order_layers` 看到的列占用才是真实的，交叉最小化不再为不存在的走廊买单；
- 符合「下游不得推翻上游，只得展开上游」：`assign_ports` 退化为「读角色 → 展开成 Side + 槽位」，不再自己发明角色。

代价：`packed_x` 需要在 order 之前先有一个粗估（可用 declaration order + 宽度累加），或者把角色决议放在 order 之后、properify 之前做一次轻量重排。这是本方案唯一需要动管线顺序的地方，值得在实施前单独确认。

**过渡方案（低风险）**：如果不想立刻动管线，先只做「把 `span≥2` 回边的默认从 `cross_axis` 改成 `J` 比较」，`properify` 照旧插 dummy。探针数据显示这一步单独就能把 mech 的 max_bends 6→4；crossings 会先从 11 升到 13（因为走廊仍未共线），要和 P2 一起才转正。

### 4.2 P2 — cross-axis：让长边走廊与真实端点共线（收益最大）

这是拿分最多的一块，三件事：

**(a) 给 virtual elem 真实宽度。** 现在 `ElemKey::is_zero_width()` 让所有 dummy 宽度为 0，层内最小间隔只剩 `node_gap=24`。结果 `·e23#0` 被 n10 挤到 x=24，而它的两个真实端点 n25 / n15 都在 x=53 —— 走廊没有争取到自己那一列的权利。yFiles 给长边留整列。建议引入 `dummy_width`（默认 = `edge_gap`，随 `node_gap` preset 缩放），至少让走廊列的最小间隔与真实节点同量级。

**(b) BK primary block 的 real 端也进硬共线。** 目前 `hard_constraints` 只对 primary block 内**相邻 VV 对**下 `gap=0`（`symmetry_objective.rs` 约 1061 行），链两端的 real→virtual 段没有约束，于是每条长边天然带两次 jog。BK 论文里 block 是包含端点 real 的；把 `RV` 段也纳入（或以高权重软约束 + 可降级），配合 (a) 才能真正拉直 e2 / e6 / e23 / e32。

**(c) 大幅降低 port anchor 对节点 x 的拉力。** `apply_port_anchor_desired` 把 dummy 的 `desired` 钉到 real 节点上的 **port slot 像素位置**，并把权重抬到 `VIRTUAL_WEIGHT × 16 = 64`；而父子对齐的 real-real 边权只有 1。力量对比 64:1，结果就是 §2.3 里的 e5：n10 被拉到 68 去对齐 n15 的 `N#1` 槽（63），而它唯一的父 n6 在 117 —— yFiles 的选择正好相反（n10 与 n6 共线 262，n10→n15 允许一次 jog）。

  **正确的分工**：cross-axis 求解**节点中心**，端口槽是**求解之后**在节点面宽内挑一个位置，不该反过来决定节点中心。建议把 port anchor 降为弱项（量级与 RV 边权同阶），把它想解决的问题交给 P3。

**(d) 顺带清掉微偏移。** e9 `n15→n16` 只差 7px 就吃了一次折。VPSC 解完后应有一道「共线吸附」后处理：若 `|x_a − x_b| < ε_snap`（ε 取 `node_gap/4` 量级）且移动不违反硬约束，则强制相等。这类微偏移在 showcase 里普遍存在，收益是全局性的。

### 4.3 P3 — 端口槽吸附走廊（消掉最后一次 jog）

yFiles 的 9 条 0 折点边里，有 4 条（`n25→n15`、`n13→n12`、`n21→n5`、`n14→n11`）靠的不是"走廊与节点中心完全重合"，而是**走廊 x 落在节点面宽之内，端口槽直接开在那个 x 上**。

我们现在的 `AlongSpec::Ordered { order, count }` 把同侧端口均分成 `(k+1)/(count+1)`，是纯组合量、不看几何，所以永远会剩一次 jog（e6 就是走廊 488 vs 槽位 476 的 12px 差）。

建议扩展 `metric/port_lane.rs`（它已经在为 twin N/S 走廊做同类对齐，写者不变、不破写权）：

1. 对每个 `(node, side)`，先按对端几何 x 排序确定槽位**次序**（这一步保持现状，保证不交叉）；
2. 再在该次序下做一次**保序的 1D 吸附**：若某端口的对端走廊 x 落在节点面宽 `[cx − w/2 + pad, cx + w/2 − pad]` 内，且吸附后仍与相邻槽保持 `min_slot_gap` 且不破坏次序，就把槽位钉到走廊 x；
3. 其余槽位在剩余空间内均分。

这一步做完，e6 / e18 / e23 的末端 jog 都会消失，`forward_gutter_side` 想解决的问题（e18 的 4 折）也就自然没了 —— 这正是"上提写者"而不是"下游加特判"的正解。

---

## 5. 验收与契约同步

### 5.1 mech.layout-styles 门禁目标

| 指标 | 当前 | P1 后 | P2 后 | P3 后（目标） | yFiles |
|------|------|-------|-------|---------------|--------|
| crossings | 11 | ≤ 13 | ≤ 4 | **≤ 2** | 0 |
| sum_bends | 74 | ≤ 74 | ≤ 60 | **≤ 54** | ≈ 50 |
| max_bends | 6 | **4** | 4 | **2** | 2 |
| 0 折点边 | 3 | 3 | ≥ 6 | **≥ 8** | 9 |
| E/W 端口边 | 3 | **0** | 0 | **0** | 0 |

P1 单独会让 crossings 短暂上升（探针实测 11→13），这是预期的：走廊角色对了但还没共线。**P1 与 P2 应作为一个门禁单元一起评估**，不要单独给 P1 设 crossings 阈值。

### 5.2 不得回归的既有期望

- `product.ticket-triage` escalate→handle 仍走侧廊（极性由 J 决定，不再硬编码 East/West）
- `product.order-approval` rejected→submit 同脸 East 共廊、不越过 submit 东侧面
- `product.three-tier` / `user-auth` twin 短回边仍 N/S 平行、无 mid-gap 横向 jog
- `smoke.multi-rank-backedge` 扇对称
- 全体 showcase 的 `det=true` 与 0 overlap

### 5.3 需要同步修改的契约文档

若采纳 P1，以下位置的「走廊角色表」必须从声明表改写为目标函数描述，且**三处口径一致**：

- `phases/ports-and-channel.md` §2 第 3 步
- `phases/channel-corridor-allocator.md` §6.3（当前仍是旧规则，已与代码不符）
- `expectations.md` §6「平行折返 vs 回边侧绕」的裁定行 —— 语义从"按 span 分类"改为"按互穿代价分类"，产品语义不变

---

## 6. 风险与开放问题

1. **P1 的写者上提会动管线顺序**（走廊角色 → properify → order）。若 `packed_x` 的粗估质量不够，可能导致角色决议与最终几何不符。实施前应先用现有 fixture 量一下「order 前粗估 x̂」与「metric 定稿 x」的相关性；相关性不足则退回过渡方案（角色仍在 ports 决议，但 properify 按角色跳过 dummy）。
2. **P2(b) 把 RV 段纳入 BK 硬共线，可能在窄图上让 VPSC 不可行**。必须走现有的约束降级链（full → twin 软 → 无 clamp 等式），并新增一档「RV 共线降级」。
3. **P2(c) 降 port anchor 权重会动很多 showcase 的绝对坐标**，`insta` 快照会大面积变更。建议单独一个 commit，且先在 `flat/` 子集上验证再推 `group-weak/`。
4. **yFiles 参考只有一张图。** 本文的 yFiles 层内序与坐标是从图 1 的像素反推的（27 节点全部对上、拓扑自洽，可信度高），但 bends / crossings 的目标值是目视统计，误差 ±2。不建议把 `sum_bends ≈ 50` 当硬门禁，用 `≤ 54` 留裕度。
5. **本 fixture 未被任何设计文档定为对标基准**（`docs/` 全库 grep `LayoutStyles` 零命中），它目前只是 `hier_eval` 基线里的观测项。若要把它升格为 yFiles 对标门禁，应先在 `expectations.md` 里立项。

---

## 7. 实施记录（2026-08-12）

已落地（本轮）：

| 项 | 状态 |
|----|------|
| 保留 `side_corridor_polarity` 归一化 rightness；撤掉 `forward_gutter_side` | ✅ |
| P1 过渡：`span≥3 ∧ near-column ∧ ¬cross_group` → N/S；其余 `span≥2` → E/W；twin 短一律 N/S | ✅ |
| P0：刷新 `hier_eval_baseline.json`；修正 d22b fallback 期望（2 fixture 已不再 fallback） | ✅ |
| 契约同步：`ports-and-channel.md` / `channel-corridor-allocator.md` / `expectations.md` §6 | ✅ |

延期（P2/P3，避免强宏穿组 / primary-arm 回归）：

| 项 | 原因 |
|----|------|
| Virtual `dummy_width` | 拉开 group-frame gap、扰动 strong-macro |
| BK RV 硬共线 | 长支叶抢脊（order-approval） |
| 降 port-anchor 权重 | 扇出 dummy 列测试与多 showcase 坐标大挪 |
| 微偏移 snap | 暂留代码未接线 |
| PortLane 长边吸附 | 暂留 `adsorb_ns_ordered_to_corridors` 未接线 |

mech 当前观测（相对改前 HEAD crossings=26 / max_bends=6）：**crossings≈13，max_bends=4，e29/e30 走 N/S，e18 保持 S→N**。距 yFiles（0 交叉 / max_bends=2 / 9 条 0 折）仍差 P2/P3。


```bash
cargo run -p plotgram-cli -- measure      apps/showcase/hierarchical/flat/mech.layout-styles.pgm
cargo run -p plotgram-cli -- debug-layout apps/showcase/hierarchical/flat/mech.layout-styles.pgm -o /tmp/mech.json
cargo run -p plotgram-cli -- render       apps/showcase/hierarchical/flat/mech.layout-styles.pgm -o /tmp/mech.svg
cargo test -p plotgram-compile --test hier_eval
```

§4.1 的探针（回边改走 N/S 主脊）：把 `compose/ports.rs::pick_reversed_side` 里 `if span >= 2 { return cross_axis; }` 改成 `return rank_dir;`，重跑上面三条命令即可复现表中「探针」列。

---

## 8. 第二轮：Main 走廊（2026-08-12 下午）

有了 `mech.layout-styles.graphml`（yFiles 导出的真值坐标）之后可以逐边对：**yFiles 44 折 / max 2，我们 78 折 / max 4**。差额几乎全在「有 dummy 链的长边」上——它们都是 Z-Z 四折，而 yFiles 是单 Z 两折。

根因是两处写者各说各话：

1. **Channel search**：`main_span_dist` 在两端点 order 之间是**常数 0**，带内所有缝等价，选谁只由拥塞随机决定。可 Order 早就为这条长边留了一列（它的 dummy 链），信息在 hints 里丢了。
2. **Metric Main-X**：`main_line_backbone_x` 取全图均值，与这条边无关（见 `coordinate-and-demand.md` §8.1）。

修法不是再加特判，是给这两个自由度各补一个它们本该看见的量：

| 改动 | 位置 |
|------|------|
| `SpanAffinity.chain_order` + `main_chain_dist`：带内再叠「到自己 dummy 链列的距离」 | `channel/search.rs`、`channel/route_all.rs` |
| Main 走廊 X 改为在 `{端口 x, 链中位 x, backbone}` 上最小化 `J`，硬夹 E/W 侧界与 foreign group 包络 | `metric/track.rs`（`MainLaneFacts` / `best_lane_x`） |
| E/W 端口的 `x_bounds` 留 `edge_gap/2` 余量，竖廊不得贴脸（否则 Ink stub 法向校验判骑脸） | `hierarchical/mod.rs` |
| `band_obstacles` 只取走廊**真正跨过**的 rank，不是整个 span | `metric/track.rs` |

**结果**：mech `sum_bends 78 → 66`；7 条长边里 6 条从 4 折降到 **2 折**（= yFiles 同款单 Z），只剩 `e2`（n0→n4）因 rank3 上 n8 正压在 n4 列上仍是 4 折。`hier_eval` 全绿，全库 fixture 折点与 bbox 普遍下降，基线已刷新。

**踩过的两个坑**（都是「组」相关，记下来免得重犯）：

- 竖廊清障若不分组，会把**边完全内含**的那个组的框也当障碍绕出去 → 画布膨胀 + `channel_used_gates` 回归。正确语义是只避 **foreign group**（不同时含两端点的组）。
- `w_chain` 若按**求和**算漂移，链越长这项越大，长边会为贴链把 bend/len 全压掉 → 又一次 canvas bloat。改**均值**后归一。

**仍在的差距**（不属本轮 channel 范畴）：短边端口未按 yFiles 那样在脸上对齐成列，属 PortLane（§5 的 P3），需要时另起一轮。

---

## 9. 第三轮：端口列与次轴（2026-08-12 晚）

把 graphml 里的端口 `Ratio` 逐脸拉出来对，先排除了一个猜测：**端口沿脸的分布规则两边基本一样**（yFiles `(2k+1)/2n`，我们 `(k+1)/(n+1)`），差距不在这。

真正的分水岭在**谁为端口偏移买单**。yFiles 的端口恒在均分槽，它把偏移记在**节点中心**上：`e23`（n25→n15）里 n15 的中心比 n25 右 7.5，正好等于 n15 那张 2 端口脸上左槽的偏移，于是两个端口精确同列、边零折。我们的 `J(x)` 量的却是节点中心共线，端口偏移从头到尾没进过目标函数。

再把所有水平段按长度分桶，结论很干脆：**32 段里有 10 段 dx ≤ 25px**。这 10 段贡献 20 个折点，64 − 20 = 44 —— 正好是 yFiles 的数。也就是说剩余差距全是「差一点点就直」，而 25px 在半个节点宽以内。

### 做了什么

| 改动 | 位置 |
|------|------|
| `J` 的段项从中心共线改为**端口共线**；`snap_fan_pack_style` 的主臂 / twin / 脊链共线语句同步加 `AlignDeltas` | `metric/symmetry_objective.rs` |
| PortLane 从「只管 twin 走廊」泛化为「**共享脸**上的槽向伙伴列滑」，PAVA 投影 + 中位数池化 | `metric/port_lane.rs` |
| `fan_out_four` / `order_approval` 门禁改断言可观测列（端口 x），不再断言 `Ordered` 表示与节点中心 | `tests/hier_eval.rs` |

结果：mech `sum_bends 66 → 58`、`max_bends 4 → 2`（与 yFiles 齐）、零折 2 → 5；全 showcase `Σsum_bends −283`、`Σcrossings −49`，84 个 fixture 变化、**无单点回归**。

### 试过但不成立的

- **给 dummy 量宽**（P2 遗留项）：扫了 0 / 12 / 24 / 40，折点几乎不动（64→62），画布反而 +75。层挤不是靠 dummy 占位能松开的。
- **调端口锚点权重**（`VIRTUAL_WEIGHT × 1…16`）：折点纹丝不动。
- **让 J 在 pack 之后拍板**：J 确实找到更低值，但会把「主臂占脊」这类产品规则挤掉（`order_approval` 脊 spread 109.8）。J 目前还表达不了这些规则，pack 的规则表暂时不能撤——这条记在这里，是 §4 那笔「规则表 → 目标函数」欠账的一部分。
- **让单槽脸的端口也滑**：能到 46 折（离 yFiles 只差 2），但箭头会扎在盒子角上（`smoke.fan-out-four` 的 `svc_b`）。单端口的列就是节点的列，不归 PortLane。

### 下一轮的抓手

剩下的边（`e3` n1→n2 差 12.9、`e9` 差 6.5）两端都是单槽脸，脸上没有自由度，只能挪节点——而**层被压在 `node_gap=24` 下限上整层刚化**：rank4 从 x=46 到 467 全是 24 的间距，n2 想左移 13 就得推着 n22 和一整条 dummy 链走，L1 / L2 都判定不划算。yFiles 同层是 76 / 53 / 30 / 30，有富余。

所以下一个问题不是「端口」也不是「走廊」，是**次轴为什么把每层都压到下限**：`desired` 大量重合到同几列，VPSC 只能按序贴着排。要么在 `J` 里加紧凑度与直线度的显式权衡，要么让 order 阶段就把长边链的列错开。
