# mech.layout-styles 对标 yFiles：现状评审与收敛方案

日期：2026-08-12 · 基准 fixture：`apps/showcase/hierarchical/flat/mech.layout-styles.pgm`（27 节点 / 34 边，拓扑转写自 yFiles `LayoutStyles.graphml`）

yFiles 真值摘录（勿再通读 32KB graphml）：[`mech.layout-styles.yfiles.md`](mech.layout-styles.yfiles.md) / [`mech.layout-styles.yfiles.json`](mech.layout-styles.yfiles.json)。精确 **sum_bends = 44**（不是 ≈50）、0 折 12 条、E/W 端口 0。

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
| 微偏移 snap | **已接线**：终局 VPSC 后端口差 `< node_gap/4` 的实–实 hop 加共线等式（symmetry-axis §4.9） |
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

---

## 10. 第四轮：把「路由需要的结构」上提到 P3/P4（2026-08-12 重写）

> 本节替换前一版「加端点直度项 → 视情况动 order」的方案。前一版两条都实现并实测过，**均失败并已回退**，见 §10.3。
> 新方案按 [`reference/yfiles/01`](../../../../reference/yfiles/01-sugiyama分层布局.md)、[`03`](../../../../reference/yfiles/03-正交边路由.md)、[`08`](../../../../reference/yfiles/08-分组泳道与端口约束.md)、[`16`](../../../../reference/yfiles/16-大图性能与wasm.md) 的相纪律重排。

### 10.0 结论先行

用户观察（现行渲染）：**n14 右移** → 回边 `n14→n11`（e29）可成竖直线；**n17 左移到 n21 正下** → 标准扇入，消掉 `e25`/`e13` 交叉。yFiles 真值：`n14.cx≈389 ≈ n11`（e29 **0 折**）；`n17.cx = n21.cx = 108.2`（e25 **0 折**）。

一个自然的想法是「次轴（P4）应该照顾日后路由的需求」。**参考实现不这么做**：P5 不得推翻 P3 的序与 P4 的坐标（[`01` §0](../../../../reference/yfiles/01-sugiyama分层布局.md)），P4 也不预测路由。它们的做法是把路由会用到的东西**提前物化成上游自由度**：

| 路由需要的东西 | yFiles 在哪一相物化 |
|----------------|---------------------|
| 长边走哪条竖廊、廊在谁左谁右 | **P3**：dummy 链参与定序，且链**作为一个整体**被移动 |
| 廊的确切列 | **P4**：整条链绑成**一个变量**（linear segment），端口列是对齐目标 |
| 廊自己占的横向空间 | **ρ**：虚节点有宽（线宽 + 边间距），不是 0 |
| 层间水平 track 的高度 | **Demand**：track 需求回写 `layer_gap`，唯一合法反向通道 |
| 端口面 / 绕行 | **组合相**：port dummy 参与定序；P4 对齐端口而非节点中心 |

我们四处偏离了这张表（§10.2），于是 e29 的竖廊被排在错的一侧，而次轴无论怎么加软项都翻不了案。

### 10.1 实测证据（探针，结论仍成立）

**(a) e29 的廊被 order 钉在 n12/n13 左侧**

| 观测 | 值 |
|------|-----|
| e29 两颗 dummy 的 desired（端口锚，pass2） | `v#0=352.9`、`v#1=263.4` —— **互相矛盾**，相差 90 |
| VV 段项等权（w=64）→ 解 | 两颗都落 `303.1`，≈两者折中，谁的端口都不对 |
| rank6 分离约束 | `n12.left − dummy = 24.0` **恰好取等 → 约束激活**，竖廊右移被 n12 物理挡住 |
| 我们的 rank6 order | `n16, n21, n7, v:e29, n12` —— 廊在**左** |
| yFiles 同图（w=30） | `n13=355.8`、`n12=363.3`、`n11=386.5`、`n14=389.0` —— 廊在**右**（n12 右边界 378，余量仅 11） |

`desired[v]=352.9` 说明 `J` 本来就想往右：**不是目标函数选错列，是 order 决定的左右关系把可行域切没了**。

**(b) n17 的扇入共线写成了两个绝对拉力**

| 观测 | 值 |
|------|-----|
| `center_h(n17)` = 父 x 中位数 | `axis = 71.0` = `best[n21]` —— pack 确实对到了中位父那一列 |
| pack 写入 | `desired[n17]=71 (w=16)`、`desired[n21]=71 (w=8)` |
| rank6 分离 | 解出 `n16=−18.2 / n21=45.8 / n7=109.8`，间距**恰好 64 / 64 → 整层取等** |
| 结果 | n21 被推离到 45.8；n17 权重大留在 71 → **错位 25**，`e25`/`e13` 交叉 |

一层排满时 VPSC 只能整体平移，弱的那个被牺牲，而强的不会跟着走。**共线必须写成相对量**。

**(c) `n13.x` 有两个写者**：n12（rank6）把 primary `n13` 钉到 `307.4+δ`，n11（rank4）随后按自己的轴写成 `396.9`，末次写入胜出，共线锁形同虚设 —— 直接违反 AGENTS.md §1。

### 10.2 真正根因：与参考实现的四处偏离

| # | 偏离 | 现状（代码） | 参考 | 后果 |
|---|------|--------------|------|------|
| **D1** | **长边链不是排序单元** | `sift_pass` 显式过滤零宽元素（`compose/order.rs`），dummy **永不 sift**；transpose 只做层内相邻对换 | [`16` §4](../../../../reference/yfiles/16-大图性能与wasm.md)：长边链的序应作为**一个整体**处理（块级 sifting, Bachmaier 2010） | 把 e29 的廊挪到 n13/n12 右侧需要 rank5 与 rank6 **同时**换侧；逐层局部动作每一步单看都不划算 → 局部最优出不去 |
| **D2** | **反转边被当二等公民** | order `segment_weight`：`reversed → 1.0`，绕过 1/2/8 里的 VV=8；`forward_real_adjacency` 整条跳过 reversed；ports 对 reversed 另开 E/W 侧廊策略 | [`01` §1/§3](../../../../reference/yfiles/01-sugiyama分层布局.md)：P1 之后 reversed 只是一个**方向位**，P3/P4/P5 一律按普通边处理；1/2/8 是让长边变直的关键配方 | 回边的廊拿到全场**最弱**的拉直权重，且 hub/fan/primary 完全看不见它的两个端点 |
| **D3** | **链在 P4 不是一个变量** | 只有落进同一 BK 块的相邻 **VV** 对才加硬等式（`metric/symmetry_objective.rs`），RV 恒软；`apply_port_anchor_desired` 让每颗 dummy 各自锚到就近端口 | [`01` §4.5](../../../../reference/yfiles/01-sugiyama分层布局.md)：linear segment —— 一条长边的**所有 dummy 绑成一个变量**，与层内分离不等式一起交给 VPSC | 同一条链收到互相矛盾的 desired，靠等权平均出一个两端都不满足的列（探针 (a)） |
| **D4** | **虚节点宽 = 0** | `elem_size`：`Virtual/Boundary/Pad → Size(0,0)`（`hierarchical/mod.rs`） | [`01` §4.6](../../../../reference/yfiles/01-sugiyama分层布局.md)：虚节点宽应 = **线宽 + 边间距**，否则两条长边贴死 | 廊没有自己的空间预算，只能贴 `node_gap` 生存；§9 单独试过加宽无效——因为 D1/D3 没解决时加宽只会撑画布 |

一句话：**我们把「链」当成了 N 个独立的零宽点**——在 P3 它不能整体移动，在 P4 它不是一个变量，在 ρ 它不占地方。次轴再怎么加项，都是在给一个被拆散的对象打补丁。

### 10.3 反模式登记（本轮已实现、实测、回退，勿重试）

| 试法 | 结果 | 根因 |
|------|------|------|
| **端点直度项 `w_end`**：`boost·span·w` 拉 `|(x_s+off_s)−(x_t+off_t)|`，进 `J` 和/或 median desired | 全边 span≥2 + median + boost2 → mech sum 50→44 但 **max_bends 2→4**，多 fixture 涨折/撑画布；仅 reverse → 54（更差）；仅 `J` 或温和权重 → 几何**与基线相同** | order 未放开时，端点项只能把折点从一端搬到另一端（探针 (a) 已预言） |
| **order 给 reverse dummy 加 `original_target` 列的 bary 偏置** | mech sum 50→**58**、crossings 4→6，e29 仍在左，且打乱 e30 的左盆 | 量纲错（拿跨层 order 下标当同层 bary 目标）＋与 crossing 主目标直接打架；**单元素**偏置也解决不了「整条链要一起换侧」 |
| 单槽脸滑端口 / 单独给 dummy 量宽 / 让 `J` 在 pack 后无约束翻盘 | 见 §9 | 同上：局部补丁 |

### 10.4 新方案：R1–R5

**R1 · P3：长边链块化排序（核心，先做）**

- 定义 **chain block** = 一条边的全部 dummy（跨多层的一列元素）。
- 新增**块级移动**：把整块在它覆盖的每一层同时插到相对位置 `k`，用现有 BJM 增量计数评估 **Δcrossings**；`sift_pass` 的候选集加入 chain block（当前它被 `is_zero_width` 过滤掉了）。
- 接受判据保守：**Δcrossings < 0 才接受**，平局用 `total_span` / 稳定序 tie-break，仍走 best-snapshot。
- 同时撤掉 D2 的降权特判，恢复 `reversed` 段的 1/2/8（若 e30 类左盆回归重现，用**块级移动**解决，不要再降权）。
- 验收：mech rank5/6 允许 `v:e29` 出现在 n13/n12 右侧；`hier_eval` crossings 不升。

**R2 · P4：linear segment（与 R1 同批验收）**

- 一条长边的所有 dummy 合成**一个 VPSC 变量**（或一组恒等约束），不再依赖 BK 块是否碰巧对齐；
- 两端 real 通过**端口偏移**接入该变量的目标项 `|x_chain − (x_end + off_end)|`（reversed 两端同样计入，因为 D2 撤销后它就是普通边）；
- 删除 `apply_port_anchor_desired` 的「按端各自锚定」——链的列由这**一个**写者决定（这正是原 P0′ 想做但没做的事，现在有了正确形态）。

**R3 · P4：成对共线改相对量**

- `primary` / `twin` 共线从「绝对 `desired` + 1e6」改成**相对代价项**（高权），消除探针 (c) 的双写者；
- 硬等式**只**保留链内（R2）与既有 gb/pb —— 全量硬 primary 等式已证过刚（§10.3 上一版记录）。

**R4 · ρ / Demand：给廊空间预算（跟随项，不单独上）**

- 虚节点宽 = `edge_gap`（线宽 + 边间距，[`01` §4.6](../../../../reference/yfiles/01-sugiyama分层布局.md)）；
- 层间 track 需求继续走 DemandBoard 回写 `layer_gap`，**保持唯一反向通道**；
- 只在 R1+R2 落地后再评估，否则重演 §9「加宽 = 撑画布」。

**R5 · 端口：撤 reversed 侧廊特判（评估项）**

- 回边与普通长边同构（N/S + 链）；E/W 侧廊只保留给 `span=1` 无 twin 等真正需要的情形；
- 依赖 R1/R2 先把链摆对，否则撤特判会让回边直接压到别的列上。

### 10.5 顺序、风险与验收

| 步 | 内容 | 风险 | 门禁 |
|----|------|------|------|
| 1 | R1 块级 sifting（保守判据）+ 撤 D2 降权 | 改的是 P3 主目标，全库敏感 | `hier_eval` crossings/bends 不升；mech order 出现右侧廊 |
| 2 | R2 linear segment + 删按端锚定 | 链过刚可能撑画布 | mech e29 折数下降；bbox 护栏 |
| 3 | R3 相对共线 | n17 类扇入受益，dense 层可能改变 | `order_approval` / `twin_spine` / D2·D3 门禁 |
| 4 | R4 / R5 | 仅在 1–3 转绿后 | 全库 bbox + 折点 |

失败判据（提前止损）：R1 单独做完若 mech 的 order 仍不出现右侧廊，说明「整体移动」这一假设也不成立，应回到 P3 目标函数本身（是否该把「长边直线率」写进 `J_order`），而不是继续在 P4 加项。

### 10.6 写权表（不变）

| 自由度 | 唯一写者 |
|--------|----------|
| 反向位 | P1 Greedy-FAS（之后 reversed 只是方向位） |
| 层号 | P2 |
| 层内序（含 **chain block**） | **P3 order** |
| 链的列 / 节点 x | **P4 SymmetryAxis**（linear segment） |
| 端口 side / along | Compose ports + PortLane |
| 折点 | Ink（零新决策） |
| 层间距 | DemandBoard → P4 之前 |

## 11. 实施记录：D1–D4 落地（R1/R2/R4）

对照 §10.5 的 1/2/4 步顺序（R3/R5 本轮不做）。

### 步 1 · R1 链块化排序 + 撤 D2 降权 ✅

- `segment_weight` 删 reversed 降权分支，恢复 1/2/8 基础权重（VV 仍不乘作者权重）。
- `forward_real_adjacency` 撤销为**针对性撤销**：span-1 reversed 计入 real-real 邻接，长 reversed 跳过。全撤会把远端端点引入 fan 邻接，把 spine head 变 hub（order-approval `rejected -> submit` 实证），导致 D2 spine 门禁 + 12 fixture 折数回归。
- `chain_block_sift_pass` 落地（非 grouped plan，与 `sift_pass` 同门控），含表驱动单测（链换侧需两层同时动 fixture + grouped 不动链 case）。
- 门禁：layout 177 单测、hier_eval 13/13 全绿，全库 crossings/bends 全 +0。

### 步 1 止损点实测（重要）

mech 右侧廊**未出现**，但失败判据的前提（机制不成立）被诊断探针否定：`CHAIN_SIFT_DBG` 实测 e29 链 k=7 右侧候选 crossings=0（与基座持平）、total_span +1 —— 右侧廊在 `J_order` 词典序下**非改善**，保守判据必然拒绝。块级机制本身有效，是目标函数不偏好右侧。经确认后继续步 2（R2 折数收益不依赖廊在哪侧），不回头改 `J_order`。

### 步 2 · R2 linear segment ✅（单写者形态修正）

- 链恒等约束落地：相邻 dummy 双向 0-gap 等式，降级链从 3 档扩为 4 档（链恒等骑前三档，末档丢弃）。
- **desired 层单写者（两端锚均值写全链）实测否决**：六轮归因（禁链恒等 / 仅两端锚 / 仅写首尾 / 按端各自锚 / N/S-only / 含 E/W）全部产生 fan/spine fixture 回归；链恒等单独完全无害。
- 最终形态：**链恒等在 VPSC 层实现单写者**（整链一个变量），`apply_port_anchor_desired` 按端锚定保留 —— 两端拉力经恒等等式折中成整链一个加权列。hier_eval 13/13、全 workspace 绿，坐标与基线 bit-identical，mech.layout-styles max_bends=2（目标达成）。

### 步 3 · R4 虚节点宽 = edge_gap ❌（实测否决，已回退）

- `elem_size` 对 Virtual 返回 `Size(edge_gap, 0)`（R1+R2 已落地前提下评估）。
- 实测：mech crossings 4→8、sum_bends +6、yfiles-pipeline 等多个 fixture 折数回归、geometry invariants 破。与 §9 结论一致：**加宽在现行 order/track 预算下 = 撑画布 + 涨折**。代码已回退，勿重试；再议前置是 track 需求/走廊预算体系先行。

### 基线与契约

- `hier_eval_baseline.json` 刷新：全库指标除两处可接受漂移外全 +0 —— `group-weak/product.microservices` 净改善（crossings 5→4、sum_bends 18→16）；`group-weak/demo.plotgram-core-mod-deps` crossings 81→83（bbox_w 与 symmetry_deviation 同时改善）。insta 快照零变更。
- 契约已同步：`composition.md` §6（reversed 不降权 + chain block sifting）、`symmetry-axis.md` §3/§4（链恒等 + 约束层单写者）、`coordinate-and-demand.md` §5.2/§5.3。

### mech 对照 §10.0 yFiles 真值

| 指标 | yFiles（摘录） | 落地前 | 落地后 |
|------|----------------|--------|--------|
| crossings | **0** | 4 | 4 |
| max_bends | 2 | 2 | 2 |
| sum_bends | **44** | 50 | 50 |
| 0 折点边 | **12** | 9 | 9 |
| E/W 端口 | 0 | 0 | 0 |

mech **几何零变化**。R1/R2 的价值是结构性的（链块可整体移动、链恒等防 zig-zag），当前 fixture 上未转化为指标。e29/e30 仍各 2 折（yFiles 0 折），合起来就是 50−44 差额里的 4 折；其余 +2 来自 e0/e3/e5/e15/e21/e23/e27 等单槽脸微错位（我们另有 6 条边比 yFiles 更直，但是错列换来的，见审核）。

---

## 12. 宏观能力：P4 要求解边直度，不是把 median 当 desired 去挤（2026-08-13）

> mech 只是检验标准。本节回答：要对齐 yFiles Hierarchical 那一档**一类图**的观感，算法上缺的是哪项能力。不是再给 e29 加一项。

### 12.1 一句话

我们的次轴求解器名义上最小化 `J(x)`（边直 + 扇心），**实际在最小化「到 median-desired 的 L2 距离」**。VPSC 的 API 是 `min Σ w(x − desired)² s.t. 分离`。当许多节点的 median 撞到同一列，投影就把整层**挤到 `node_gap` 下界**。yFiles / Graphviz dot / ELK `NETWORK_SIMPLEX` 解的是另一道题：

```text
min  Σ_e Ω(e)·ω(e)·|(x_u + off_u) − (x_v + off_v)|     （或 L2 平方）
s.t. 同层相邻 x_{i+1} − x_i ≥ ρ
```

分离是约束，**不是目标的副作用**。不相邻的边把节点拉向不同列时，层内会留下富余 —— 这正是 yFiles 层间距 76/53/30/30、我们整层 24 贴死的原因。

### 12.2 证据（mech 上能看见，但病在所有 dense DAG）

| | yFiles L4 中心 | 相邻净空（cx 差 − 30） |
|--|----------------|------------------------|
| n10–n22–n2–n4–n11 | 77.5, 183, 266.5, 326.5, 386.5 | **75, 53, 30, 30** |

我们同层从左到右几乎全是 `node_gap=24` 取等。§9 已经观察到「层被压在下限上整层刚化」，当时猜「dummy 要量宽」或「order 要把链错开」。R4 加宽失败、R1 换侧被 `J_order` 拒绝，都是因为**真正的自由度在 P4 的目标函数被用错了**：刚化不是 ρ 太小，是求解器把所有 slack 吃掉了。

同一机制解释一批「不像 e29」的差距：

- **n19**：yFiles `cx=266.5`（压在 n0/n1 脊上），我们 `119`（收在父 n18 正下）。median+pack 让短边 e11 把 n19 钉死；simplex 会让 n19 的两条下行边（→n0、→n24）把节点拉到扇的中位列，e11 允许 2 折。我们 e11 是 0 折，是**错列换来的假直**。
- **e3/e5/e0** 等单槽脸：两端差十几 px，脸上没有滑槽，只能挪节点；层刚化后 L1 判定不划算。yFiles 同层有 slack，节点能独立对列。
- **e29/e30**：链恒等已经把 dummy 焊成一根（R2），但这根被挤在错误的一侧；P4 再直也翻不了 P3 的左右。这是**第二项**能力，见 §12.4，不能代替 §12.3。

### 12.3 能力 A（主突破）· 次轴 = 边直度 LP / IPSEP，desired-packer 降为初值

文献与产品对应（[`01` §4](../../../../reference/yfiles/01-sugiyama分层布局.md)、[`13` 选型](../../../../reference/yfiles/13-实现路线图与选型.md)）：

| 路线 | 目标 | 谁在用 |
|------|------|--------|
| B 网络单纯形 | `min Σ Ωw \|x_u−x_v\|` s.t. ρ | Graphviz `position.c`；yFiles **SimplexNodePlacer（Hierarchical 默认）**；ELK `NETWORK_SIMPLEX` |
| C Brandes–Köpf | 对齐块 + 块间紧致 | ELK 默认；我们已算 `bk_ideal` 却只当 VPSC **初值**，随后被 median 毁掉 |
| D IPSEP / linear-segment QP | `min Σ w(x_u−x_v)²` s.t. ρ，VPSC 投影 | ELK `LINEAR_SEGMENTS`；Dwyer–Marriott |

我们现在是路线 A 的变体（Sugiyama **Priority / median**）：质量文献明确写「快而糙」。选型表当年写「BK 起步，约束多了换 VPSC」—— VPSC 换对了，**目标没换**：仍在喂 per-node desired。

**正确用法（与现有零件兼容）**：

```text
硬约束（已有）：层内分离、链恒等、twin/gb/pb 等式
目标（已有公式，从未被求解器直接最小化）：
  J(x) = Σ_seg w·|(x+off)_u − (x+off)_v|  +  λ_sym·Σ_hub |x_h − center_h|

迭代（IPSEP 风格，替换「median → desired → VPSC」）：
  1. 对 J 做无约束下降（L1：邻域加权中位；或改 L2 用梯度）
  2. VPSC 投影到硬约束 —— 投影的是「这一步的 x」，不是「所有人共用的几个 median 点」
  3. 以 J 做 snapshot（已经在做）
snap：只保留无法写进 J 的产品规则（twin/primary 相对 δ），禁止再写「整层按 node_gap 铺叶」这种把 slack 杀光的绝对 desired
```

关键差：加权中位作为**无约束一步**是合法的（它就是 L1 边代价的最优）；把它写成 VPSC 的 `desired` 就变成「所有人吸向少数几个点再挤开」。无约束中位之后节点仍在不同 x 上，投影会保留边与边之间的缝。

**不做什么**：不上第三套求解器；不把 BK 当终局（组框/端口/链恒等已经证明 BK 不够）；不重试 dummy 量宽、`w_end`、跨层 bary 偏置。

**验收（能力，不是单图）**：

- 代表 dense DAG（mech、yfiles-pipeline、fan-out、order-approval）层内相邻净空的**中位数 > node_gap**（允许局部取等，禁止整层取等）。
- 单槽脸、两端本应同列的短边，0 折率上升；允许个别边为了占脊而 2 折（yFiles 的 e11 就是）。
- `hier_eval` 硬不变量绿；bbox 允许变宽（yFiles 也不是最窄装箱）。
- mech：n19 应落到 n0/n1 一带而不是 n18 正下；e3/e5 类微折应降。e29 侧不作为本步必达（见 B）。

### 12.4 能力 B（配套）· P3 在交叉持平时按端点列安放链块

R1 块级 sift 的机械装置是对的，**词典序错了**。`J_order = (crossings, source_moment, total_span)` 在 crossings 已平的时候用**无向 |Δorder|** 当第二键。长回边的廊换到端点一侧常常 crossings 不变、span 微增（mech e29：右侧候选 span+1）→ 永远拒绝。

yFiles / dot 的 1/2/8 是**交叉权重**，不是 span。长边直不直主要在 P4 用 Ω=8 拉齐；P3 只保证 dummy 链作为整体、且不要为了省一点 span 把廊夹在无关实节点中间。

**改法**（仍是 order 写者，不翻 P4）：

- 块级 trial 的接受：`crossings` 不增即可进入候选（不是必须降）。
- 交叉持平时的次键改为 **endpoint inversion**：链在各层相对「工作头 / 两端 real 的层内位」有多少实节点夹在中间。降低 inversion = 廊靠到端点那一侧。`total_span` 降为更后的 tie-break。
- 插入用**相对位**（相对某层的 head 分位，或各层独立对齐到同一分位），不要用跨层同一个绝对下标 `k`。

这是一类图的能力：任意 span≥2 的边（含 FAS 回边）在交叉不恶化时，廊出现在端点列那一侧，P4 的 linear segment 才有正确的可行域。

**验收**：mech e29 dummy 出现在 n13/n12 右侧、e30 链与 n21/n5 同侧；全库 crossings 不升。本步**在 A 之后或并行**，单独做只能换侧、层仍可能刚化。

### 12.5 能力 C（跟随，A 之后才评估）

| 项 | 何时 |
|----|------|
| R3 相对共线（primary/twin 进 J 的成对项，去掉绝对 1e6 双写） | A 把 snap 从「铺叶装箱」里解放之后；否则相对项仍被 pack 挤掉 |
| 虚节点宽 = edge_gap | A 让层有 slack 之后再试；现在加宽 = 撑画布（R4 已否决） |
| snap 瘦身 | A 落地后：叶槽 pitch 改为「J 已分开的列」，不再按 `node_gap` 强制铺 |

### 12.6 落地顺序

| 步 | 内容 | 风险 |
|----|------|------|
| **A0** | 旁路：关掉 median-desired 循环，直接 `bk_ideal` + 链恒等 VPSC，看层 slack / mech n19 / 折点（诊断，不留主路径） | 组框/twin 可能松 |

**A0 实测（2026-08-13）**：选项 `symmetry_place: bk`。mech 上：

| | median | A0 `bk` | yFiles |
|--|--------|---------|--------|
| n19.cx | 119（n18 正下） | **242**（靠近 n0/n1 脊） | 266.5 |
| 实节点相邻净空 中位数 | ≈24（几乎全取等） | **27**（19 缝里 9 条仍取等） | 明显更大（L4: 76/53/30/30） |
| sum_bends / 0 折 / crossings | 50 / 9 / 4 | 52 / 8 / 4 | 44 / 12 / 0 |
| e29 廊 | n12 左，2 折 | 仍在左，2 折 | n12 右，0 折 |

结论：**median-desired 确实是把 n19 收到父列、把层挤死的主因**（A0 一关，n19 自己跑到脊上）。BK 紧致化仍会局部取等，且不管 P3 的廊侧，所以折点没有变好。A0 不留主路径；A1 要的是「对 J 下降 + 投影」，不是把 BK 当终局。

| **A1** | 主路径：IPSEP 迭代（对 J 下降 + VPSC 投影）；J 继续当 snapshot；snap 叶槽先不动，但 IPSEP 不再 1e6 锁 primary/twin | dense 图变宽；折点可能先升 |

**A1 实测（2026-08-13）**：默认 `symmetry_place: ipsep`。无约束步 = 邻域 L2 重心（不是 median 列），VPSC 用基础权重投影这一步的 `x`。snap 仍铺叶，但两趟都不把 twin/primary 写成 1e6 绝对 desired（否则 n19 又被钉回 n18；`order-approval` 主臂门禁仍绿）。

| | median | A0 `bk` | **A1 `ipsep`** | yFiles |
|--|--------|---------|----------------|--------|
| n19.cx | 119（n18 正下） | 242 | **336**（n0=304 / n1=325 脊上；e11 改为 2 折） | 266.5 |
| 实节点相邻净空 中位数 | ≈24 | 27 | **47**（19 缝里 7 条仍取等） | L4: 76/53/30/30 |
| yfiles-pipeline 净空中位 | — | — | **48**（14 缝里 6 条取等） | — |
| sum_bends / max / 0 折 / crossings | 50 / 2 / 9 / 4 | 52 / 2 / 8 / 4 | 60 / 4 / 5 / 8 | 44 / 2 / 12 / 0 |
| e29 廊 | n12 左，2 折 | 仍左 | 仍左（dummy cx=400，n13=444），4 折 | n12 右，0 折 |

失败判据未触发：dense 层净空中位已明显高于 `node_gap`。n19 离开父列。e3/e5 微折还在（脸上无槽，属 A2/C 把叶槽从 `node_gap` 铺开改成 J 已分开的列）。e29 侧仍是 B。`hier_eval` D2/D3 / 主臂门禁绿；几何硬不变量绿；bend 回归门与两处 canvas-bloat（>10%）红——bbox 变宽是 A1 预期，基线未刷。

| **A2** | 把 twin/primary/exclusive 脊从绝对 desired 改成 J 的相对项（fan-hub 子不加 primary）；snap 叶槽仍按 J 缝铺，1e6 只留 median 档 | 门禁敏感 |

**A2 实测（2026-08-13）**：`J` / L2 步对 twin（×8）、eligible primary 与 exclusive 1:1（×4）加权；两端都是扇 hub 的 min-span 对（mech n18–n19）不加。snap 仍铺叶 + hub 扇心 + exclusive 跟列，IPSEP 两趟都不 1e6。

| | A1 | **A2** | yFiles |
|--|----|--------|--------|
| n19.cx | 336（脊上） | **354**（n0=319 / n1=326 脊上；e11 仍 2 折） | 266.5 |
| 实节点净空中位 | 47 | **47**（7/19 取等） | L4 更大 |
| sum_bends / max / 0 折 / crossings | 60 / 4 / 5 / 8 | 60 / 4 / 5 / 6 | 44 / 2 / 12 / 0 |
| e3/e5 微折 | 仍 2 折 | 仍 2 折（dx 4 / 13） | 0 折 |
| D2 / D3 / 主臂 / twin / ticket-triage | 绿 | **绿** | — |

D2 / D3 / 主臂门禁绿。叶槽孤叶装箱见 **C**；e29 侧见 **B**。

| **C** | snap 瘦身：孤叶保持 J 列，不再 `axis±node_gap` 强铺；≥2 叶仍 FanPack（D3） | 门禁敏感 |

**C 实测（2026-08-13）**：IPSEP 下每个 hub 的**恰好 1 个自由叶**不再被拽到 `node_gap` 步长；多叶扇仍按层内序等距（否则 `fan_spine_chain` / D3 红）。

| | A2 | **C** | yFiles |
|--|----|--------|--------|
| n19.cx | 354 脊上 | **354** 脊上 | 266.5 |
| 净空中位 / 取等 | 47 / 7/19 | **46** / 7/19 | L4 更大 |
| sum_bends / max / 0 折 / crossings | 60 / 4 / 5 / 6 | **58 / 2** / 5 / 8 | 44 / 2 / 12 / 0 |
| e3/e5 | dx 4 / 13，2 折 | 仍 2 折（dx 5 / 13） | 0 折 |
| e29 | 左，4 折 | 左，**2 折** | 右，0 折 |
| D2 / D3 / 主臂 | 绿 | **绿** | — |

孤叶不再被装箱，mech `max_bends` 回到 2。e3/e5 的几像素差来自近脊节点中心没对齐（单槽脸 = 中心即端口），不是孤叶 pitch。多叶等距仍会局部取等。

| **B** | `J_order` 交叉持平 → endpoint inversion；块插入改相对位 | 改 P3 主目标，全库 crossings 门禁 |

**B 实测（2026-08-13）**：块级 trial 接受 crossings 不增；交叉持平时次键 = 各层 dummy 相对两端 real 分位之间夹着的实节点数；插入按 `k/max_k` 分位而不是跨层同一个绝对下标。外层 sift 循环只在 crossings 上升时回滚（否则 span+1 的换侧会被 `J_order` 立刻撤掉）。inversion 不进全局 `J_order` / barycenter / 逐元素 sift。

| | C | **B** | yFiles |
|--|---|--------|--------|
| n19.cx | 354 脊上 | **355** 脊上 | 266.5 |
| 净空中位 / 取等 | 46 / 7/19 | **48** / 8/19 | L4 更大 |
| sum_bends / max / 0 折 / crossings | 58 / 2 / 5 / 8 | **58 / 4** / 6 / 9 | 44 / 2 / 12 / 0 |
| e29 | n12 **左**，2 折 | n13/n12 **右**，仍 2 折（n14=311 / 廊=415 / n11=458） | 右，0 折 |
| e30 | 与 n21/n5 同侧，2 折 | 仍同侧（L3–L5 左盆），**4 折**（n21=108 / 廊=156 / n5=208） | 同侧，0 折 |
| 实节点层内序 | 与 yFiles 一致 | **仍一致** | — |
| D2 / D3 / 主臂 | 绿 | **绿** | — |

e29 换侧是 B 的能力验收：P3 把廊放到端点那一侧，P4 才有正确可行域。n14 还没被拉上廊、e30 变成三列 Z，都是 P4 还没把端点焊到链上，不是 order 写错侧。`hier_eval` 几何硬不变量绿；bend 回归门与 canvas-bloat 仍红（基线未刷）。layout 178 单测绿。

| **D** | P4 悬挂汇点跟长边廊；廊跟非叶端 port | 主臂/D2 敏感 |

**D 实测（2026-08-13）**：`chain_end_boost`（默认 8）进 J/L2，对象是「无正向子、恰一个 span-1 父、且父不是 span-1 扇 hub」的悬挂汇点（mech n14）。exclusive 脊与 snap 走链在该叶处停。port-anchor：非叶端仍把 dummy 拉向自己的 port；叶不反向拽廊，而是被拉到 dummy 列。扇叶（order-approval `rejected`）和 DAG 源（`submit`）不加，否则 D2 主臂散。

| | B | **D** | yFiles |
|--|---|--------|--------|
| n14.cx / e29 廊 / n11 | 311 / 415 / 458 | **448 / 448 / 458** | 389 / 396.5 / 386.5 |
| e29 | 右，2 折（dx≈107） | 右，2 折（**dx=10**，n11 多槽脸） | 右，0 折 |
| e28 n12→n14 | 0 折（假直，钉在 n12） | **2 折**（与 yFiles 同形态） | 2 折 |
| e30 | 同侧，4 折 | 仍 4 折（两端都不是悬挂汇点） | 0 折 |
| sum_bends / max / 0 折 / crossings | 58 / 4 / 6 / 9 | **60 / 4** / 5 / 9 | 44 / 2 / 12 / 0 |
| n19 | 脊上 | 脊上 | 266.5 |
| D2 / D3 / 主臂 | 绿 | **绿** | — |

n14 离开 n12 跟廊，是 D 的能力验收。e29 剩 10px 是 n11 多槽脸的端口偏移（共线说的是端口），不是 100px 错列。e30 两端都是扇/贯通点，不在本步范围。`hier_eval` 几何硬不变量绿；bend / canvas-bloat 仍红（基线未刷）。layout 178 单测绿。

| **E** | 两端都非叶时，长边廊不要两端折中；恰好一端是扇 hub 则非 hub 写整条链 | D2 脊敏感 |

**E 实测（2026-08-13）**：链恒等把 ≥2 颗 dummy 焊成一个变量后，两端等权 `port_anchor` 会把廊停在中间列（D 的 e30 三列 Z，`max_bends` 从 C 的 2 抬到 4）。单写者只在无歧义时启用：一个非叶端写整链（D）；两个非叶端且恰好一端是扇 hub、且 ≥2 颗 dummy → **非 hub** 写廊（扇 dummy 跟子列；e30 跟 n21 左盆）。两端都是 / 都不是 hub、或 span-2 单 dummy，仍逐端写入。hub 写廊会把扇 dummy 收到父列；「离链更近」的单写者会把 order-approval `rejected→submit` 的廊焊偏，D2 `submit/review/check` 散 1.8px。

| | D | **E** | yFiles |
|--|---|--------|--------|
| e30 | 4 折，廊=156，n21=108 / n5=208 | **2 折**，廊=135，n21=108 / n5=208 | 0 折 |
| e29 | 右，dx=10，2 折；n14 在廊上 | **同** | 右，0 折 |
| sum_bends / max / 0 折 / crossings | 60 / 4 / 5 / 9 | **60 / 2** / 4 / 10 | 44 / 2 / 12 / 0 |
| n19 | 脊上 | 脊上 | 266.5 |
| D2 / D3 / 主臂 | 绿 | **绿** | — |

e30 不再三列 Z，是 E 的能力验收：`max_bends` 回到 2。廊仍在 n21 与 n5 之间（非 hub 写廊，不是 0 折）。e29 叶跟廊不受影响。`hier_eval` 几何硬不变量绿；bend / canvas-bloat 仍红（基线未刷）。layout 178 单测绿。

| **F** | 两端都是 hub 的长链不要折中；snap 不要把长边 hub 拽回子心 | 主臂/D3 敏感 |

**F 实测（2026-08-13）**：E 之后最大的系统性偏差不是折点公式，是 **e18 dummy 停在 n18/n25 折中（x≈100）**，L2 整层被顶开（n26/n5 比 yFiles 偏右 ~90px），e30 只能 Z 穿左盆。e18 两端都是扇 hub，E 的「恰好一端是 hub」没覆盖。单写者推广为「至少一端是 hub」：两端都是 hub 时由**本层更靠边缘**的那端写廊（n25 最左，n18 独占 L0 算中心）。PortLane 按伙伴列重排同脸槽（n11 e29 不再倒在最左）。

snap 把长边 hub 钉回 `center_h` 会撤销 J 刚拉上的线性段。IPSEP 下连着 dummy 的 hub **保持 J 的列**，纯扇仍 `center_h`（`fan_spine_chain`）。否决：把 port-anchor 放进 IPSEP 迭代（整图无界左漂）；长边 hub 降权好让 slack 吸入（D3 / 主臂红）。

| | E + 伙伴列 | **F** | yFiles |
|--|------------|--------|--------|
| crossings | 7 | **2**（e0×e32、e6×e29） | 0 |
| e18 廊 | 折中 x≈100 | **x≈33**（跟 n25） | x≈10 |
| n26 / n5 | 144 / 208 | **77 / 176** | 47.5 / 116 |
| n12 / e27 | 308 / 2 折 | **366 / 0 折** | 363 / 0 折 |
| e30 | 2 折，廊=135，n5=208 | 2 折，廊=138，n5=176 | 0 折 |
| sum_bends / max / 0 折 | 60 / 2 / 4 | **58 / 2 / 5** | 44 / 2 / 12 |
| D2 / D3 / 主臂 / layout 178 | 绿 | **绿** | — |

e18 让出左盆、e27 共线，是 F 的能力验收。n5 仍不在 e30 上：IPSEP 迭代时 e18 dummy 还在旧列，snapshot 把 n5 停在 176；snap 再钉住长边 hub，腾出的 slack 吸不进去。下一步应让链列进下降步且**有左墙**（不能再无界左漂），而不是再加第三趟特判。`hier_eval` 几何硬不变量绿；bend / canvas-bloat 仍红（基线未刷）。

| **G** | 1:1 茎焊端口列；扇出 hub 在 1:1 茎上保持 J（不贴子心）；纯扇入汇点贴焊茎后的 `center_h`；扇入不 FanPack 父节点 | 主臂 / D3 / n14 廊 |

**G 实测（2026-08-13）**：小折与 n17 偏轴是同一自由度——「唯一向下邻居 = 唯一向上邻居」的茎应共一列。exclusive chain 在 hub 处停，叶被 FanPack 后不带 continuation。两端都是 hub 的茎（n6–n10）不焊：子贴父会拖动整簇、汇点离轴；父贴子会撞层内分离。该子改为保持 J。悬挂汇点叶不焊（D 的 n14 跟廊）。

| | F | **G** | yFiles |
|--|---|--------|--------|
| sum_bends / max / 0 折 | 58 / 2 / 5 | **44 / 2 / 12** | 44 / 2 / 12 |
| n15–n16 / n20–n21 / n21–n17 | 2 折小折 | **0 折** | 0 / 2 / 0（n20–n21 有端口差） |
| n17 vs 父母 midspan | +12px（贴 n21 右侧） | **−2px** | 0（n17=n21=midspan） |
| n6–n10 | 11px 折 | ~20px 折（两 hub 茎不焊） | 0 折 |
| n1–n2 | 2 折 ~5px | **0 折**（共线吸附，§4.2(d)） | 0 折 |
| D2 / D3 / 主臂 / layout 178 | 绿 | **绿** | — |

焊茎的 soft desired 经 VPSC 仍会剩 2–5px 横折。终局再投影一次：端口差 `< node_gap/4` 的实–实 hop 加 `x_to = x_from + δ` 硬等式（两端都可动）。有意的扇出横折大于 ε，不吸。

失败判据：A1 之后若 dense 层仍整层 `node_gap` 取等，说明投影仍在吃 slack（desired 仍在塌缩）——不要加宽 dummy，先查下降步是不是又把所有人映射到少数几个 x。

写权不变：P3 写左右，P4 写 x，P5 展开。变的是 **P4 真正最小化的那个函数**。
