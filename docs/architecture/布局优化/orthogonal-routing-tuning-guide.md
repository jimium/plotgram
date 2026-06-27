# 正交边路由参数调优指南

> 日期：2026-06-27
> 适用版本：P0/P1/A 实施后
> 目的：记录所有可调参数、位置、作用、调优方向，方便后续慢慢微调

---

## 一、视觉验证 Checklist（发现问题先看这里）

渲染示例后，按以下顺序检查，先定位问题类别，再去对应章节调参数：

### 1.1 Side 选择问题（端口不对）

| 现象 | 可能原因 | 去哪调 |
|------|---------|-------|
| 跨水平排列 group 的边（左右并排的 group）走了 Top/Bottom 绕远路，应该走 Left/Right | 水平 sibling 阈值太高 | §2.2 `VERTICAL_PREFERENCE_THRESHOLD_HORIZONTAL_SIBLINGS` |
| 跨垂直排列 group 的边（上下堆叠的 group）明明对齐了却走 Left/Right 绕路 | 垂直 sibling 对齐阈值太严 | §2.3 `VERTICAL_PREFERENCE_THRESHOLD_VERTICAL_SIBLINGS_ALIGNED` + `SIDE_ALIGN_MARGIN` |
| 跨垂直排列 group 的边位置不对齐，走 Top/Bottom 必然外道绕行，应该走侧边 | 垂直 sibling 不对齐时阈值太偏垂直 | §2.3 `VERTICAL_PREFERENCE_THRESHOLD_VERTICAL_SIBLINGS_UNALIGNED` |
| 一出端口就撞兄弟 group 的边框，应该自动切备选 side | 出口校验未生效或距离太短 | §2.5 `EXIT_CHECK_DISTANCE`，§2.6 是否实现了 `has_clear_exit` |
| 同组内边 side 选择不合理（上下对齐却走左右） | 同组阈值偏离 0.4 | §2.1 `VERTICAL_PREFERENCE_THRESHOLD_SAME_GROUP` |
| 跨多层祖先的边（如 external → internal）side 方向不对 | 跨祖先阈值不合适 | §2.4 `VERTICAL_PREFERENCE_THRESHOLD_CROSS_ANCESTOR` |

### 1.2 路径质量问题（绕路/折点多）

| 现象 | 可能原因 | 去哪调 |
|------|---------|-------|
| 同组内边绕到了 group 外做大 U 型 | endpoint_groups 没包含祖先 | 代码逻辑问题（已修，检查是否回归） |
| 路径绕到障碍物外侧，不走 group 之间的间隙 | 通道候选没优先 corridors | §3.1 `EXTRA_CHANNEL_MARGINS`，检查 path.rs 是否传了 corridors |
| 路径太长，明明有短路径却选了长路径 | 路径长度惩罚不够/折点惩罚太低 | §4.2 `BEND_PENALTY`，可能需要加 length penalty |
| 平行边贴太近或重叠 | 平行间距太小 | §3.4 `ORTHO_PARALLEL_GAP` |
| 路径折点太多，绕来绕去 | bend penalty 太低 | §4.2 `BEND_PENALTY` |
| 路径穿越节点或 group 内部 | crossing penalty 不够大 | §4.1 `NODE_CROSSING_PENALTY`、§4.3 `GROUP_TRANSIT_PENALTY` |

### 1.3 Slot/锚点问题（节点附近交叉）

| 现象 | 可能原因 | 去哪调 |
|------|---------|-------|
| 同节点同侧边，左边 slot 的边向右走、右边 slot 的边向左走→交叉 | replan_slots 没修正倒挂 | §5 检查 replan_slots 是否生效，phase1_only 是否生成正确路径 |
| 多条边挤在一个锚点上分不开 | slot pitch 太小 / Compact pitch 太小 | §5.1 `ORTHO_SLOT_PITCH`、§5.2 `COMPACT_SLOT_PITCH` |
| 锚点太靠近节点边角，不好看 | slot margin 太小 | §5.3 `SLOT_MARGIN_RATIO` |
| stub 太短一出锚点就拐弯，不好看 | port clearance 太短 | §5.4 `PORT_CLEARANCE` |
| 4+ 条边没有自动汇流共享入口 | Concentrate 阈值太高 | §5.5 `choose_docking_strategy` match 臂（代码改） |

### 1.4 边距离/留白问题

| 现象 | 可能原因 | 去哪调 |
|------|---------|-------|
| 边离节点/ group 太近，有贴边感 | obstacle pad 太小 | §4.6 `NODE_OBSTACLE_PAD`、§4.7 `GROUP_OBSTACLE_PAD` |
| 边离障碍物太远，浪费空间 | obstacle pad 太大 | 同上 |
| 通道太窄边挤在一起，或太宽松散 | channel margin 不合适 | §3.2 `CHANNEL_MARGIN`、§3.3 `MIN_CHANNEL_CLEARANCE` |

---

## 二、Side 选择参数（Layer 1，P0 新增）

文件位置：[slot.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/slot.rs#L9-L15)

所有"垂直偏好阈值"的含义：`|dy| >= |dx| * threshold` 时选 Top/Bottom，否则选 Left/Right。
- **阈值越大** → 越不喜欢垂直端口，越倾向于走 Left/Right（水平侧）
- **阈值越小** → 越喜欢垂直端口，|dy| 略大于 |dx| 就走 Top/Bottom

### 2.1 `VERTICAL_PREFERENCE_THRESHOLD_SAME_GROUP`

| 属性 | 值 |
|------|-----|
| 当前值 | `0.4` |
| 作用 | 同组内边选择 Top/Bottom 的阈值 |
| 调大→0.6+ | 同组内更多走 Left/Right（水平侧），可能导致同组垂直对齐的边也绕路 |
| 调小→0.2- | 同组内更多走 Top/Bottom（上下侧），接近旧逻辑 |
| 建议范围 | 0.3 - 0.5 |
| 注意 | 同组内没有 group 边界阻隔，0.4 是经过验证的合理值，**不建议大改** |

### 2.2 `VERTICAL_PREFERENCE_THRESHOLD_HORIZONTAL_SIBLINGS`

| 属性 | 值 |
|------|-----|
| 当前值 | `0.8` |
| 作用 | 水平排列 sibling group（左右并排）的边选择 Top/Bottom 的阈值 |
| 调大→0.9+ | 几乎强制走 Left/Right，只有 dy 极大（垂直位移是水平位移 90%+）才走上下 |
| 调小→0.5-0.6 | 较多走 Top/Bottom，回到类似旧逻辑的行为，lb→biz_svc 问题会重现 |
| 建议范围 | 0.7 - 0.9 |
| 注意 | **这是解决 lb→biz_svc 绕路问题的核心参数**。当前 0.8 效果：只有垂直位移达到水平位移 80% 以上才走上下，大多数跨水平组边走侧边直连 |

### 2.3 `VERTICAL_PREFERENCE_THRESHOLD_VERTICAL_SIBLINGS_ALIGNED` / `_UNALIGNED`

| 属性 | ALIGNED 值 | UNALIGNED 值 |
|------|-----------|-------------|
| 当前值 | `0.4` | `0.5` |
| 作用 | 垂直排列 sibling group（上下堆叠）：<br>- ALIGNED：源节点 x 对齐目标 group x 范围→直走上下<br>- UNALIGNED：源节点 x 不对齐目标 group→倾向走侧边绕 |
| ALIGNED 调大→0.6 | 对齐场景也更多走 Left/Right（不推荐） |
| ALIGNED 调小→0.2 | 对齐场景几乎全走 Top/Bottom（合理） |
| UNALIGNED 调大→0.7+ | 不对齐场景也更多走 Left/Right（更激进走侧边） |
| UNALIGNED 调小→0.3- | 不对齐场景也倾向 Top/Bottom，外道绕行问题重现 |
| 建议范围 | ALIGNED: 0.3-0.5 / UNALIGNED: 0.5-0.7 |
| 注意 | 两个值的差应保持 0.1-0.2，UNALIGNED 应比 ALIGNED 大（更倾向侧边） |

### 2.4 `VERTICAL_PREFERENCE_THRESHOLD_CROSS_ANCESTOR`

| 属性 | 值 |
|------|-----|
| 当前值 | `0.5` |
| 作用 | 跨祖先分支的边（LCA 是更上层 group，如 external → internal）的阈值 |
| 调大→0.7 | 更多走 Left/Right |
| 调小→0.3 | 更多走 Top/Bottom |
| 建议范围 | 0.4 - 0.6 |
| 注意 | 这类边走走廊的判断在代码中通过 corridors 辅助，阈值是 fallback |

### 2.5 `SIDE_ALIGN_MARGIN`

| 属性 | 值 |
|------|-----|
| 当前值 | `20.0`（像素） |
| 作用 | 判断"源节点中心 x 是否在目标 group x 范围内"时的边距扩展 |
| 调大→30-40 | 更容易被判定为"对齐"，更多边走 Top/Bottom |
| 调小→5-10 | 更严格判定对齐，更多边被判为不对齐走侧边 |
| 建议范围 | 15 - 30 |
| 注意 | 此值太小会导致明明看着对齐却被判为不对齐；太大会导致不对齐也走 Top/Bottom 绕路 |

### 2.6 `EXIT_CHECK_DISTANCE`

| 属性 | 值 |
|------|-----|
| 当前值 | `32.0`（像素） |
| 作用 | Step 4 出口校验：从锚点沿 side 外向投射多长距离检测撞 group |
| 调大→48+ | 更早检测到撞墙，更积极切换备选 side（可能误切） |
| 调小→16- | 撞墙了才切，甚至不切 |
| 建议范围 | 24 - 40 |
| 注意 | 应 ≥ `PORT_CLEARANCE(16) + stub_clearance`，保证 stub 段长度内不撞墙。当前 32 = 16*2 |
| 重要提示 | **当前代码中 Step 4 的 `has_clear_exit` 可能尚未完全实现**，如果发现出边撞 group 的问题，先检查该函数是否实际在做检测 |

---

## 三、路径/通道参数（Layer 2，P1 修复）

### 3.1 `EXTRA_CHANNEL_MARGINS`

文件位置：[path.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/path.rs#L197)

| 属性 | 值 |
|------|-----|
| 当前值 | `[28.0, 40.0]`（像素） |
| 作用 | Z 字形/阶梯路径生成绕行通道时的两档外边距 |
| 调大 | 通道离障碍物更远，路径更松散但留白多 |
| 调小 | 通道离障碍物更近，路径更紧凑但可能贴边 |
| 建议范围 | [20, 36] - [32, 50] |
| 注意 | 这是**外道绕行**的边距，优先使用 group corridors 后此参数影响减小 |

### 3.2 `CHANNEL_MARGIN`

文件位置：[mod.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs#L61)

| 属性 | 值 |
|------|-----|
| 当前值 | `18.0`（像素） |
| 作用 | 侧通道绕行时距障碍节点的留白 |
| 调大→24-30 | 边离节点更远，更美观但占地大 |
| 调小→10-12 | 边离节点更近，更紧凑但可能显挤 |
| 建议范围 | 14 - 24 |

### 3.3 `MIN_CHANNEL_CLEARANCE`

文件位置：[mod.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs#L200)

| 属性 | 值 |
|------|-----|
| 当前值 | `10.0`（像素） |
| 作用 | 即便被 group 边框挤压也要保留的最小通道留白 |
| 调大→16 | 挤压时也保证较宽通道 |
| 调小→6 | 挤压时通道很窄 |
| 建议范围 | 8 - 16 |
| 注意 | 防止 group 间距太小时路径贴边穿过去 |

### 3.4 `ORTHO_PARALLEL_GAP`

文件位置：[constants.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/constants.rs#L44)

| 属性 | 值 |
|------|-----|
| 当前值 | `8.0`（像素） |
| 作用 | 平行边之间的最小间距（重叠检测阈值） |
| 调大→12-16 | 平行边间距更宽，更清晰但占空间 |
| 调小→4-6 | 平行边更紧凑，可能显乱 |
| 建议范围 | 6 - 14 |

---

## 四、评分函数权重（Layer 2 路径选择）

文件位置：[mod.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs#L189-L197)、[scoring.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/scoring.rs#L17-L30)

**评分逻辑**：分数越低越好。每条候选路径累加各项惩罚，选分数最低的。

### 4.1 `NODE_CROSSING_PENALTY`

| 属性 | 值 |
|------|-----|
| 当前值 | `10000.0` |
| 作用 | 穿越节点的惩罚（必须极大，硬约束） |
| 建议 | **不要调小**，必须保持压倒性大值防止穿节点 |
| 注意 | 这是安全阈值，调小会导致路径穿过节点 |

### 4.2 `BEND_PENALTY`

| 属性 | 值 |
|------|-----|
| 当前值 | `16.0`（每折点） |
| 作用 | 每个折点的惩罚，鼓励更少拐弯 |
| 调大→24-32 | 更强惩罚折点，路径更直但可能绕更长 |
| 调小→8-12 | 折点惩罚降低，接受更多拐弯换更短路径 |
| 建议范围 | 12 - 24 |
| 注意 | BEND_PENALTY 太小会导致 staircase 路径（多折短距离）比 Z 字形（少折长距离）得分低，路径会折来折去 |

### 4.3 `EDGE_OVERLAP_PENALTY`

| 属性 | 值 |
|------|-----|
| 当前值 | `1200.0` |
| 作用 | 与已有边段重叠的惩罚 |
| 调大→2000+ | 更强避免平行边重叠，边间距更大但可能绕远 |
| 调小→600-800 | 允许更多并行，路径更短但可能重叠显乱 |
| 建议范围 | 800 - 2000 |
| 注意 | 与 `ORTHO_PARALLEL_GAP` 协同：GAP 是检测阈值，PENALTY 是惩罚力度 |

### 4.4 `GROUP_TRANSIT_PENALTY`

文件位置：[scoring.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/scoring.rs#L26)

| 属性 | 值 |
|------|-----|
| 当前值 | `3000.0` |
| 作用 | 穿越非相关 group 内部的惩罚 |
| 建议 | 保持较大值，避免穿 group 内部 |
| 注意 | P1 修复后同组边的 group 已加入豁免，此惩罚只针对无关 group |

### 4.5 Near-miss 惩罚（scoring.rs）

| 参数 | 当前值 | 作用 |
|------|--------|------|
| `NODE_NEAR_MISS_PENALTY` | 2500 | 路径离节点太近（在 NODE_OBSTACLE_PAD 内但不穿越）的惩罚 |
| `NODE_NEAR_MISS_EXTRA` | 10 | near-miss 距离检测扩展 |
| `GROUP_NEAR_MISS_PENALTY` | 2000 | 路径离 group 边框太近的惩罚 |
| `GROUP_NEAR_MISS_EXTRA` | 8 | near-miss 距离检测扩展 |

这些值一般不建议大改。如果路径总是贴边，增大 near-miss penalty；如果路径绕太远，适当减小。

### 4.6/4.7 Obstacle Padding（scoring.rs）

| 参数 | 当前值 | 作用 |
|------|--------|------|
| `NODE_OBSTACLE_PAD` | 18 | 节点外扩多少范围内算接近障碍物 |
| `GROUP_OBSTACLE_PAD` | 12 (`GROUP_BORDER_SHELL_PAD`) | group 边框外扩多少作为壳层禁区 |

调大→边离障碍物更远；调小→边可以更近。

---

## 五、Slot 分布与 Stub 参数

### 5.1 `ORTHO_SLOT_PITCH`

文件位置：[constants.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/constants.rs#L41)

| 属性 | 值 |
|------|-----|
| 当前值 | `40.0`（像素） |
| 作用 | Single/Normal 模式下相邻磁吸点的理想间距 |
| 调大→50-60 | 锚点更分散，边在节点侧间距大更清晰 |
| 调小→28-32 | 锚点更紧凑，适合节点多的小图 |
| 建议范围 | 32 - 50 |
| 注意 | 边长不够时自动压缩，pitch 是理想值不是强制值 |

### 5.2 `COMPACT_SLOT_PITCH`

文件位置：[mod.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs#L58)

| 属性 | 值 |
|------|-----|
| 当前值 | `16.0`（像素） |
| 作用 | Compact 模式（2-3 条边）的紧凑间距 |
| 调大→20-24 | Compact 模式不那么紧凑 |
| 调小→10-12 | 更紧凑汇流 |
| 建议范围 | 12 - 20 |

### 5.3 `SLOT_MARGIN_RATIO`

文件位置：[mod.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs#L186)

| 属性 | 值 |
|------|-----|
| 当前值 | `0.12`（边长比例） |
| 作用 | slot 分布时保留的边界余量（占边长比例），避免锚点太靠近边角 |
| 调大→0.15-0.2 | 锚点更向中心集中，留白多 |
| 调小→0.05-0.08 | 锚点更分散，靠近边角 |
| 建议范围 | 0.08 - 0.18 |

### 5.4 `PORT_CLEARANCE`

文件位置：[mod.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs#L183)

| 属性 | 值 |
|------|-----|
| 当前值 | `16.0`（像素） |
| 作用 | 从节点边界向外延伸的 stub 段长度，避免一出线就折回节点 |
| 调大→20-24 | stub 更长，节点附近更整洁 |
| 调小→8-12 | stub 更短，路径更快转向 |
| 建议范围 | 12 - 24 |
| 注意 | stub 太短一出锚点就拐弯视觉上不好看；太长浪费空间 |

### 5.5 Docking Strategy 阈值（代码逻辑）

文件位置：[slot.rs:31-37](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/slot.rs#L31-L37)

```rust
pub fn choose_docking_strategy(count: usize) -> DockingStrategy {
    match count {
        0..=1 => DockingStrategy::Single,
        2..=3 => DockingStrategy::Compact,
        _ => DockingStrategy::Concentrate,  // 4+ 条边→汇流共享中心
    }
}
```

| 策略 | 触发条件 | 效果 |
|------|---------|------|
| Single | 1 条边 | 居中放置 |
| Compact | 2-3 条边 | 间距 16px 紧凑分布 |
| Concentrate | 4+ 条边 | 所有边共享中心锚点，自然汇流 |

如果觉得触发 Concentrate 太早（4 条就汇流看不清），可以把 `_ => Concentrate` 改成 `4..=5 => Compact, _ => Concentrate` 让 4-5 条也保持可区分。

---

## 六、Group 相关参数

文件位置：[group/constants.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/group/constants.rs)

| 参数 | 当前值 | 作用 |
|------|--------|------|
| `GROUP_BORDER_SHELL_PAD` | 12 | group 边框壳层厚度，边进入此区域算 near-miss |
| `PORT_STUB_CLEARANCE` | 16 | 从 group 内节点到 group 边框的 stub 预留长度 |

---

## 七、几何容差

| 参数 | 文件 | 当前值 | 作用 |
|------|------|--------|------|
| `EPS` | mod.rs:203 | 0.1 | 坐标比较容差（像素），小于此值视为相等 |
| `OVERLAP_EPS` | lint/geometry.rs:6 | 0.5 | 重叠检测容差 |
| `BORDER_EPS` | lint/geometry.rs:7 | 2.0 | 边框检测容差 |

这些一般不建议改，改了可能导致几何比较不稳定。

---

## 八、调优流程建议

按照"先粗后细、先大后小"的顺序调，不要一次改多个参数：

### 阶段 1：先看 Side 选择对不对

1. 渲染多个 architecture 示例，先看**跨 group 边的端口方向**对不对
2. 如果有左右并排 group 之间的边走了 Top/Bottom 绕路 → 调大 `VERTICAL_PREFERENCE_THRESHOLD_HORIZONTAL_SIBLINGS`
3. 如果有上下堆叠 group 之间对齐的边走了 Left/Right → 检查 `SIDE_ALIGN_MARGIN` 和 `VERTICAL_PREFERENCE_THRESHOLD_VERTICAL_SIBLINGS_ALIGNED`
4. 确认 side 都合理后再往下调

### 阶段 2：再看路径有没有外道绕行

1. 检查同组内边是否绕出 group 外（P1 修复的问题，如果回归了是代码 bug 不是参数问题）
2. 检查跨 group 边是否优先走了 group 间隙走廊，如果总是绕外 → 确认 path.rs 中 `group_gap_midpoints_on_axis` 确实使用了 corridors 坐标（代码问题）
3. 如果折点太多 → 适当调大 `BEND_PENALTY`
4. 如果路径总是很长但折点少 → 可能 BEND_PENALTY 太大了，适当调小（但注意当前没有显式 length penalty，路径长度通过少折来间接优化）

### 阶段 3：最后调美观细节

1. 节点附近有没有 slot 顺序导致的交叉 → replan_slots 应该处理，如果没处理是代码问题
2. 边离节点/ group 太近或太远 → 调 `NODE_OBSTACLE_PAD`、`CHANNEL_MARGIN`
3. 锚点分布太挤或太松 → 调 `ORTHO_SLOT_PITCH`、`COMPACT_SLOT_PITCH`
4. stub 长度不美观 → 调 `PORT_CLEARANCE`
5. 平行边间距 → 调 `ORTHO_PARALLEL_GAP`、`EDGE_OVERLAP_PENALTY`

### 调参原则

1. **一次只调一个参数**，每次调完渲染多个示例看效果
2. **幅度要小**，每次调 10-20% 的幅度，不要从 0.4 一下改到 0.9
3. **在多个示例上验证**，不要只看一个图：一个图变好了可能另一个图变差了
4. **简单图不要退化**：flowchart 等简单无 group 的图，参数变化后应保持原有行为
5. **记录每次调整**：调之前记一下原值，不好用就改回来

---

## 九、已知可改进但非参数的代码问题

这些不是调参数能解决的，如果遇到需要改代码：

1. **Step 4 出口校验（has_clear_exit）**：当前可能未完全实现，一出 side 就撞兄弟 group 的问题需要补全 AABB 射线检测
2. **路径长度显式惩罚**：当前没有 per-pixel length penalty，路径长度优化靠 BEND_PENALTY 间接驱动。如果短路径总因多一两个折点落选，可考虑在 scoring 中加 length_penalty = 0.3-0.5 每像素
3. **replan_slots 的 stub 平行偏移**：当前重路由是用 phase1_only 重新生成完整 stub，如果新 stub 之间交叉，需要加平行偏移微调而非重路由
4. **coordinate_port_sides 的 group 感知**：当前 `side_acceptable` 用固定 0.4 阈值，后续可改为按 group 关系动态阈值（与 choose_pair_sides 一致）

---

## 十、当前已暴露但待微调的观察点

实际渲染后如果发现以下现象，记录下来，按上面的指南调：

- [ ] **待观察**：水平 sibling 阈值 0.8 是否在垂直位移较大的场景（如左上→右下跨组）下太激进？
- [ ] **待观察**：SIDE_ALIGN_MARGIN=20 对于不同尺寸 group 是否合适？
- [ ] **待观察**：4 条边就触发 Concentrate 是否太早？大节点 6-7 条边汇流更合适？
- [ ] **待观察**：PORT_CLEARANCE=16 在大节点上是否 stub 显得太短？
- [ ] **待观察**：Compact pitch=16 在高分屏/导出时是否太挤？
