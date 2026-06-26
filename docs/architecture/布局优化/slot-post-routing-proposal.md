# 正交边路由分层架构设计

> 日期：2026-06-27（架构重构版）
> 范围：`crates/drawify-core/src/layout/edge/edge_routing_orthogonal/` 及 `edge_bundling/`
> 状态：设计提案（未实施）
> 验证用例：`showcase/architecture/c.layout-stress-nested.dfy`

---

## 一、总纲：我们要解决什么问题

正交边路由的最终目标是生成**美观、可读、确定性**的边路径。"美观"包含多个维度：

| 维度 | 含义 | 出问题时的表现 |
|------|------|---------------|
| **Side选择正确性** | 边从节点的正确方向出发/到达 | lb→biz_svc选Bottom→Top绕远路，实际Left→Right更短 |
| **Slot顺序正确性** | 同节点同侧边的出口顺序与实际走向一致，无交叉 | 左侧slot的边向右走、右侧slot的边向左走→节点附近交叉 |
| **路径质量** | 路径短、折点少、走组间间隙而非外道绕行 | db_master→db_replica绕cloud外做大U型迂回 |
| **边共线(Edge Bundling)** | 同源/同宿/几何相似边共享主干，减少视觉噪音 | 平行边挤在通道里重复画线段 |
| **箭头与标签** | 箭头方向一致、标签不重叠不压线 | 标签盖在边线上，箭头方向混乱 |

**核心挑战**：这些维度之间存在依赖关系——side选择影响路径走向，路径走向决定slot正确排序，路径形态影响bundling效果，bundling重写路径又影响端点处的对齐。如果每个阶段做决策时缺少后续阶段的关键信息，就会产生级联错误。

**本文件的主张**：采用**分层分治 + 前向信息契约 + 唯一一次后向反馈**的架构。分层保证可控性和确定性，前向契约让每层做决策时拥有充分信息，唯一一次反馈（路由→slot）解决slot对路由方向的依赖。不做全局联合优化（不可控），也不做瞎子分层（当前的问题）。

---

## 二、当前流水线问题诊断

### 2.1 当前流水线

```
choose_pair_sides（独立逐对选side）
  → coordinate_port_sides（事后同侧协调）
    → slot分配（按target坐标排序，固定精确锚点）
      → edge_order排序
        → 逐边路由（path.rs，固定端点，候选路径评分选最优）
          → fix_slot_inversions（冒泡交换相邻倒挂slot并重路由）
            → snap/repulse → bundling → label_relayout
```

### 2.2 问题证据（`c.layout-stress-nested` 坐标验证）

布局结构（从上到下）：
- data_subnet（y=126~366，x=138~794）：db_master、db_replica、redis、mq（水平排列）
- public_subnet（y=126~366，x=818~1001）：lb、gateway（与data_subnet水平排列，中间24px间隙）
- private_subnet（y=462~598）：auth_svc、biz_svc、async_worker
- external（y=686~926）：client、third_party
- 子网间96px水平/垂直间隙，子网内同行节点y相同

观测到的问题：

| 问题边 | 现象 | 根因所在层 |
|--------|------|-----------|
| lb→biz_svc | 走Bottom→Top绕到private_subnet底下方(y=646)再折返，总长~724px，4折点；走Left→Right经组间间隙仅~289px，1折点 | **Side选择层**（缺group感知） |
| lb→auth_svc | 同lb→biz_svc，应走Left侧经mq/redis间垂直通道 | **Side选择层**（缺group感知） |
| db_master→db_replica | 同组内垂直对齐(dx≈0)，却绕cloud左侧外 | **路由层**（通道选择偏好外道） |
| biz_svc→db_master | 水平段绕到cloud左边界外(x=106 < 138) | **路由层**（通道选择偏好外道） |
| async_worker→db_master | 同biz_svc→db_master，绕出cloud外(x=98) | **路由层**（通道选择偏好外道） |
| biz_svc→db_master 与 auth_svc→redis | 水平段(y=414)非必要交叉 | **Slot排序层**（slot按target排，非按实际走向排） |

### 2.3 问题的本质：瞎子分层

当前流水线的根本问题不是"分层不对"，而是**每层做决策时缺少必要信息**：

| 阶段 | 决策 | 当前输入 | 缺少的关键信息 | 后果 |
|------|------|---------|---------------|------|
| Side选择 | 走Top/Bottom/Left/Right | 两节点bbox投影+dx/dy比例 | group结构、组间间隙通道、从该侧出发是否需绕出父group | 跨水平排列group的边错误选垂直端口 |
| coordinate_port_sides | 少数派侧切到多数派侧 | 各侧边数统计 | 切换后是否引入更长迂回 | 可能把几何上更优的side切掉（当前未触发，但逻辑有隐患） |
| Slot分配 | 锚点精确位置+排序 | 对端中心坐标 | 边的实际路由出口方向 | slot顺序与真实走向倒挂→交叉 |
| 逐边路由 | 路径 | 固定锚点+障碍物 | 其他边的共享通道趋势、组间间隙优先 | 外道绕行、平行边不共享通道 |
| fix_slot_inversions | 交换倒挂slot | 相邻对有效出口方向 | 全局排序视角、非相邻倒挂 | 只能修相邻对、最多8轮、Concentrate模式跳过 |

**结论**：问题不在于分层本身，在于分层边界处的信息契约太硬——前一层传"钉死的结果"，不传"决策上下文和软约束"，导致后续层只能在错误的前提下修补。

---

## 三、架构原则

### 3.1 为什么不做全局联合优化？

把side、slot、路径、bundling做成一个联合优化问题理论上全局最优，但工程上不可行：

1. **状态空间爆炸**：side(4种) × slot(连续) × 路径(无穷多) → 搜索空间不可控
2. **不可调试**：全局优化是黑盒，出问题无法定位"为什么走了这条路"
3. **不可预测**：微小输入变化可能导致完全不同输出，违反[AGENTS.md](../../../AGENTS.md)确定性要求
4. **增量更新困难**：拖拽一个节点无法只重算受影响边

### 3.2 为什么纯分层不够？

当前架构已经是分层的，但因为每层缺少后续层的信息，导致：
- Side选择做了不可逆的错误决策（选了Bottom而不是Left）
- Slot分配基于错误的假设（target坐标=出口方向）
- 路由在错误的锚点之间找路径
- 后续层只能打补丁（fix_slot_inversions），无法修复上游错误

### 3.3 设计原则：分层分治 + 前向信息契约 + 一次反馈

```
原则1：粗粒度决策先做，细粒度决策后做
原则2：每层做决策时，必须拥有该决策所需的全部信息（前向契约）
原则3：层间传递"软约束+上下文"，而非仅传递"钉死的结果"
原则4：唯一允许一次后向反馈——路由→slot重规划；其余阶段不回头
原则5：每层的输出必须是确定性的（不依赖HashMap迭代序，全序tiebreaker）
```

**类比编译器架构**：
- Side选择 ≈ 语法分析：确定大结构框架，不回头
- 路由 ≈ 代码生成：在框架内填充具体路径，需知道"目标架构"（group通道）
- Slot重规划 ≈ 寄存器分配：代码生成后确定具体位置分配，唯一的后向调整
- Bundling ≈ 窥孔优化(peephole)：不改变语义的前提下局部合并优化
- 标签 ≈ 链接装配：最后填充元数据

编译器中，语法分析不会因为寄存器分配失败而重做；同理，side选择不应因为路由问题而回退。唯一的反馈是：路由知道了路径走向后，需要回调确定slot的精确排序。

---

## 四、分层设计规范

### 4.1 流水线总览

```
┌─────────────────────────────────────────────────────────────┐
│  Layer 1: Side选择（粗粒度，不可逆）                         │
│  输入：节点bbox + group结构 + 组间间隙通道 + 同节点其他边倾向  │
│  输出：(from_side, to_side) + side置信度 + 预期出口方向提示   │
└─────────────────────────┬───────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  Layer 2: 正交路由（核心路径生成）                           │
│  输入：side + 障碍物(节点/group/已有边) + 组间间隙通道         │
│        + 锚点区域（软约束，非精确点）+ 通道偏好提示           │
│  输出：路径(points) + 路径角色段(FromStub/Trunk/Fork/ToStub)  │
│        + 各端点实际出口方向                                   │
└─────────────────────────┬───────────────────────────────────┘
                          ↓ 唯一一次后向反馈
┌─────────────────────────────────────────────────────────────┐
│  Layer 3: Slot重规划（细粒度端点调整）                       │
│  输入：路径 + 角色段 + 实际出口方向 + 同侧边出口顺序          │
│  约束：Trunk中段不变，只调整FromStub/ToStub                   │
│  输出：调整后的slot锚点 + 重建的stub段                        │
└─────────────────────────┬───────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  Layer 4: Edge Bundling（边共线/主干共享）                   │
│  输入：最终路径 + 角色段 + 边兼容性(方向/线型/箭头)           │
│  约束：端点锚点不变，可以重写merge/fork段和trunk               │
│  输出：捆绑后的路径 + 共享trunk段                             │
└─────────────────────────┬───────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  Layer 5: 标签与箭头（最终修饰）                              │
│  输入：最终路径 + bundling结果                                │
│  输出：标签位置 + 箭头渲染信息                                │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 Layer 1: Side选择

**职责**：确定每条边从源节点的哪条边出发、到达目标节点的哪条边（Top/Bottom/Left/Right）。

**为什么Side必须在路由前确定**：正交路由的第一段必须沿水平或垂直方向，不知道side就无法确定初始方向。Side是粗粒度4选1决策，一旦选定就不应回退。

**输入信息（前向契约）**：
- 节点对的bbox和中心坐标（已有）
- 节点的leaf group归属（`GroupRoutingContext.node_to_groups`，已有）
- group bbox（`GroupRoutingContext.groups`，已有）
- 组间走廊信息（`GroupRoutingContext.corridors`，已有）
- sibling group关系和排列方向（需预计算，新增）
- 同节点其他边的side选择倾向（`coordinate_port_sides`已做，但应融入主决策）

**输出**：
- `(from_side, to_side)`：确定的端口
- `side_confidence`：该选择的几何置信度（强选择vs勉强可接受），供后续层参考
- `preferred_exit_dir`：预期从该side出发后的主走向提示（供路由层参考，但不约束）

**决策树设计（3级快速决策 + 1级轻量校验）**：

Side选择采用分级决策树，大部分边在前两级就快速返回，不需要复杂计算。

```
Step 1: 硬约束排除（O(1)）→ 排除方向错误、投影强制的不可能选项
  ↓
Step 2: 单候选直接返回（快速路径）→ 同组边多数在此结束
  ↓ 多候选时
Step 3: 按group关系分类调参（核心改进）→ 4种情况分别处理
  ↓
Step 4: 轻量出口校验（AABB射线检测）→ 一出side就撞墙则切换备选
```

---

#### Step 1：硬约束排除

基于几何关系快速排除不可能的side pair：

1. **方向排除**：目标在节点下方(dy>0)时，Top→Bottom不可（不能从上面出去找下面的节点）；目标在上方、左方、右方同理
2. **投影重叠强制**：
   - 水平投影重叠(ox>EPS)且垂直投影无重叠(oy≤EPS) → 强制Top/Bottom（只能从上下走）
   - 垂直投影重叠(oy>EPS)且水平投影无重叠(ox≤EPS) → 强制Left/Right（只能从左右走）

经过Step 1后，候选side pair通常只剩1-2个。

#### Step 2：单候选快速返回

如果Step 1后只剩1个候选，直接返回。**同组内边的大多数情况在此快速结束**。

---

#### Step 3：Group关系分类决策（4种情况）

需要先对两个节点的group关系分类。需要预计算的辅助信息：
- `node_leaf_group: NodeId → GroupId`：每个节点的直接leaf group（从`node_to_groups`取最深层）
- `sibling_group_sets`：通过AST的`parent_id`+`child_group_ids`收集sibling group集合（可在GroupRoutingContext构建时一次性计算，参考[collect_sibling_sets](../../../crates/drawify-core/src/layout/group_frame/mod.rs)已有实现）
- `classify_sibling_orientation(Ga, Gb)`：通过比较两个group bbox的ox/oy判定排列方向

**情况A：同组内边（无group或共享同一个leaf group）**

| 属性 | 值 |
|------|-----|
| 判定 | `node_leaf_group[a] == node_leaf_group[b]`（或都为None） |
| 例子 | db_master→db_replica（同data_subnet），auth_svc→biz_svc（同private_subnet） |
| 垂直偏好阈值 | **0.4**（沿用现有逻辑） |
| 决策逻辑 | 现有投影重叠+主轴判定不变 |
| 性能 | O(1)，无额外计算 |

同组内没有group边界阻隔，现有几何比例法已经足够好。这类边的外道绕行问题（db_master→db_replica绕cloud外）是路由层问题（同组边不应将所在group视为障碍物），不是side选择问题。

**情况B：跨水平排列Sibling Group**

| 属性 | 值 |
|------|-----|
| 判定 | 两节点在不同leaf group，两group共享父group，group bbox垂直投影重叠大（oy≥0.5×min(h_a,h_b)），水平投影不重叠（ox≈0） |
| 例子 | data_subnet内节点 → public_subnet内节点（左右并排，中间有垂直走廊） |
| 垂直偏好阈值 | **0.8**（需要dy非常大才选Top/Bottom，否则走水平侧） |
| 决策逻辑 | \|dy\|≥\|dx\|×0.8 → Top/Bottom；否则 → Left/Right |
| 性能 | O(1)，查预计算的orientation表 |

水平排列的sibling group之间有垂直走廊（GroupCorridor with Vertical axis），走Left/Right可以直接穿过走廊到达目标group。0.8阈值意味着：除非垂直位移达到水平位移的80%以上，否则都走水平端口。

**情况C：跨垂直排列Sibling Group（最常见的问题场景）**

| 属性 | 值 |
|------|-----|
| 判定 | 两节点在不同leaf group，两group共享父group，group bbox水平投影有重叠，垂直投影不重叠（有水平走廊gap） |
| 例子 | lb→biz_svc, biz_svc→db_master, lb→auth_svc |
| 决策逻辑 | **先做位置对齐检查，再决定side** |
| 性能 | O(1)，1次范围包含检查 |

这是最复杂也最容易出问题的情况。垂直排列的group之间有水平走廊，但**源节点的水平位置可能不对齐目标group的x范围**，导致走Top/Bottom后必然绕外道。

```
位置对齐检查：source.center_x ∈ [target_group.x - margin, target_group.x + target_group.width + margin] ?

├─ 对齐 → 阈值0.4，正常走Top/Bottom
│         （直走上下可以直接进入目标group，无需迂回）
│
└─ 不对齐 → 检查是否存在水平走廊：
   ├─ 有水平走廊 + 源在目标右侧 → 直接返回(Left, Right)
   │  （向左出发进入水平走廊，再沿走廊到目标右侧）
   ├─ 有水平走廊 + 源在目标左侧 → 直接返回(Right, Left)
   │  （向右出发进入水平走廊，再沿走廊到目标左侧）
   └─ 无合适走廊 → 阈值0.5，主轴判定
```

验证：
- **lb→biz_svc**：lb center_x=924，private_subnet右边界≈850 → 924>850+margin → 不对齐，源在右 → 返回(Left, Right) ✅ 路径长从724px降到289px
- **biz_svc→db_master**：biz_svc center_x≈700，data_subnet右边界=794 → 700∈[138-margin, 794+margin] → 对齐？不对，要检查biz_svc是否在data_subnet的x范围内。data_subnet x=138~794，biz_svc x=612~788，center_x=700确实在data_subnet范围内（138~794），那为什么当前路径绕到cloud左侧外？因为这是路由通道选择问题，不是side问题。走Top方向从biz_svc顶出去，应进入y=366~462的水平走廊，然后左拐/右拐到db_master——但路由选择了绕外道。这是Layer 2要修复的。
- **假设一个对齐的正例**：如果lb center_x=600（在private_subnet x范围内）→ 对齐 → Bottom→Top ✅

**情况D：跨祖先分支（Cross-Ancestor）**

| 属性 | 值 |
|------|-----|
| 判定 | 两节点的leaf group不共享直接父group（LCA是更上层的group） |
| 例子 | client(external)→gateway(public_subnet)，external和data_subnet/public_subnet/private_subnet都是cloud的子group |
| 决策逻辑 | **朝向连通走廊选side** |
| 性能 | O(1)，查corridors表 |

```
1. 在GroupCorridors中查找连接两个group所在分支的走廊
2. 若找到走廊：选择朝向走廊的side（左走廊选Left，右走廊选Right，上走廊选Top，下走廊选Bottom）
3. 若无明确走廊：阈值0.5，主轴判定
```

---

#### Step 4：轻量出口可行性校验（AABB射线检测）

Step 3选出首选side pair后，做一个轻量校验：

```
从源锚点沿首选side外向方向投射 (PORT_CLEARANCE + stub_clearance) 长度的线段
检测该线段是否与任何sibling group的bbox相交？
├─ 不相交（通畅）→ 通过，返回首选
└─ 相交（一出side就撞兄弟group）→ 检查备选side pair：
   ├─ 备选通畅 → 返回备选
   └─ 备选也不通 → 返回首选（路由层负责绕障）
```

这是**保守校验**：只在明显"一出side就撞墙"时切换，大多数情况下首选直接通过。开销是2-4次射线-AABB相交测试，可以忽略不计。

---

**需要在GroupRoutingContext中预计算的辅助信息**（构建时一次性计算）：

| 映射 | 类型 | 来源 |
|------|------|------|
| `node_leaf_group` | `HashMap<String, String>` | 从`node_to_groups`取最深层group |
| `sibling_sets` | `Vec<Vec<String>>` | 通过AST的`parent_id`/`child_group_ids`收集（参考已有`collect_sibling_sets`） |
| `sibling_orientation` | `HashMap<(String,String), SiblingOrientation>` | 对每对sibling，比较group bbox的ox/oy判定Horizontal/Vertical |
| `group_ancestors` | `HashMap<String, Vec<String>>` | 沿parent_id链向上，用于跨祖先判断 |

`choose_pair_sides`的签名从 `fn(a: &NodeLayout, b: &NodeLayout) -> (Port, Port)` 扩展为 `fn(a: &NodeLayout, b: &NodeLayout, a_id: &str, b_id: &str, ctx: &GroupRoutingContext) -> (Port, Port)`。

### 4.3 Layer 2: 正交路由

**职责**：在确定side的前提下，为每条边生成正交路径，避开障碍物，优先走组间间隙通道。

**输入信息（前向契约）**：
- side选择结果（必须）
- 节点和group边框作为障碍物（已有）
- 已有边段作为障碍物（已有，SegmentGrid）
- 组间间隙通道位置（新增，需从group_ctx中提取可用通道）
- 锚点区域而非精确锚点（改进：给路由一个沿边的小范围自由度，而非钉死一个点）
- Layer 1传入的preferred_exit_dir（软提示，非硬约束）

**关键改进**：
1. **锚点区域替代精确锚点**：路由时端点不是精确的(x,y)，而是边上的一段区间 + 中心位置。路由可以在区间内微调出口点，以获得更顺的路径。Layer 3会最终确定精确位置。
2. **通道候选优先组间间隙**：`build_channel_detours`生成绕行通道时，优先使用group之间的间隙通道坐标，而非障碍物外侧坐标。需要group_ctx提供组间间隙信息。
3. **同组边障碍物豁免**：同组内节点之间的边，不应将所在group的边框视为需要绕行的障碍物（否则会出现db_master→db_replica绕cloud外的问题）。
4. **评分函数调整**：增加"路径长度"惩罚权重，降低"远离已有边"的奖励权重，避免外道绕行评分反而更高。

**输出**：
- 路径points（包含完整折线）
- 路径角色段（复用[edge_bundling的path decomposition](../../../crates/drawify-core/src/layout/edge/edge_bundling/compatibility.rs)）：
  - `FromStub`：从锚点沿端口外延方向的初始段
  - `FirstTurn`：第一个转向点
  - `Trunk`：中段主路径（Layer 3不能修改）
  - `LastTurn`：最后一个转向点
  - `ToStub`：到达目标节点前的末段
- `actual_exit_dir`：各端点实际出口方向（切线方向分量）

**确定性保证**：边处理顺序由layer_order确定（基于rank+degree），不依赖HashMap序；所有排序使用显式全序key。

### 4.4 Layer 3: Slot重规划（唯一后向反馈）

**职责**：路由完成后，根据各边的实际出口方向，重新规划同节点同侧边的slot排序和位置，重建stub段。

**为什么这是唯一的后向反馈**：slot的正确排序本质上依赖"边从哪个方向离开节点"，而这个信息只有路由完成后才知道。这不是side选择的失败，而是slot作为细粒度端点位置，其正确顺序天然依赖路径走向。

**约束（大布局不变）**：
- Trunk中段（FirstTurn之后到LastTurn之前）**完全不变**
- 不改变side选择
- 不重路由中段，只重建FromStub和ToStub

**具体步骤**：
1. **路径分解**：复用edge_bundling的`decompose_path`识别角色段
2. **提取真实出口方向**：从FirstTurn后的段方向提取，比现有`compute_effective_exit_dir`更鲁棒
3. **全局slot重排**：同(node_id, side)下，按真实出口切线坐标做**稳定排序**（全局排序，不是冒泡交换），重新分配slot frac
4. **Stub重建**：从新slot锚点到FirstTurn点重新生成正交stub
5. **局部冲突检测**：stub重建后检查同节点其他边的新stub是否交叉，做等距微调

**与fix_slot_inversions的关系**：Layer 3是fix_slot_inversions的彻底重构——用全局排序替代冒泡相邻交换，用stub重建替代重路由，覆盖所有slot倒挂而非仅相邻对。Layer 3完成后fix_slot_inversions可删除。

**高密度场景**：同子组边数≥4时（Concentrate策略触发条件），所有边共享一个slot（base_frac），此时slot排序无意义，跳过Layer 3的重排（但仍需stub重建）。

### 4.5 Layer 4: Edge Bundling（边共线）

**职责**：将几何相似、方向相近的边捆绑共享主干段，减少视觉噪音和ink量。

**输入**：Layer 3输出的最终路径（slot已确定、stub已重建）+ 路径角色段。

**约束**：
- 端点锚点不变（Layer 3已经确定了最终slot位置）
- FromStub和ToStub段可以重写为MergeLeg/ForkLeg（从各自slot汇聚到trunk/从trunk分叉到各自slot）
- Trunk段可以合并（几何相似的边共享trunk坐标）

**与Layer 3的边界**：
- Layer 3解决"同一节点同一侧边上，边的出口顺序是否正确"——不合并不同边
- Layer 4解决"不同边之间是否可以共享路径段"——不改变端点锚点
- 两者共享路径分解基础设施（segment角色标记），但操作粒度不同
- 执行顺序：Layer 3先确定正确端点→Layer 4再做边间合并

**现有代码**：[edge_bundling/](../../../crates/drawify-core/src/layout/edge/edge_bundling/)模块已实现聚类、trunk分配、路径重写，基本框架可用，需适配Layer 3的输出格式。

### 4.6 Layer 5: 标签与箭头

**职责**：确定标签位置、箭头方向和渲染信息。

**输入**：Layer 4的最终路径。

**规则**：
- 标签沿路径放置，避开捆绑段的重叠区域
- 箭头方向沿路径末段方向
- 不修改路径几何
- 这是纯修饰层，不回流

现有代码：[resolve_label_overlaps](../../../crates/drawify-core/src/layout/edge/common/label_avoidance.rs)和[label_placement](../../../crates/drawify-core/src/layout/edge/edge_bundling/label_placement.rs)已实现，需在Layer 4之后调用。

---

## 五、层间信息契约总结

| 从\到 | Layer 1(Side) | Layer 2(路由) | Layer 3(Slot) | Layer 4(Bundling) | Layer 5(标签) |
|-------|--------------|--------------|--------------|-------------------|--------------|
| Layer 1 | — | side(确定) + confidence + exit_dir_hint | — | — | — |
| Layer 2 | — | — | 路径 + 角色段 + actual_exit_dir | — | — |
| Layer 3 | — | — | — | 最终路径(含重建stub) + 角色段 | — |
| Layer 4 | — | — | — | — | 捆绑后路径 + trunk信息 |
| 外部输入 | 节点bbox + group结构 + 组间间隙 | 障碍物 + 通道信息 | 节点边长约束 | 兼容性(方向/线型/箭头) | 标签文本 |

**关键设计**：
- Layer 1→2传递的side是**硬约束**（确定的），但preferred_exit_dir是**软提示**（路由可以不遵守）
- Layer 2→3传递的路径中Trunk是**不可变的**（硬约束），FromStub/ToStub是**可重写的**（软区域）
- Layer 3→4传递的锚点是**硬约束**（确定的），merge/fork段是**可重写的**
- 每一层的"硬约束"构成下一层不可逾越的边界，"软提示/可重写区域"是下一层的优化空间

---

## 六、关键风险与对策

### 6.1 Group排列方向判定的可靠性

如何判断两个sibling group是水平排列还是垂直排列？
- **方法**：比较bbox的水平重叠ox和垂直重叠oy。水平排列的group ox小/为0、oy大（同层y范围重叠）；垂直排列反之。
- **风险**：斜向排列的group可能误判。
- **对策**：不是非黑即白判定，而是计算"水平排列度"和"垂直排列度"两个连续值，作为调整side选择阈值的权重，而非硬切换。

### 6.2 锚点区域给路由的自由度多大？

- **过小**（如±2px）：等价于固定锚点，没有意义
- **过大**（如±20px）：路由可能把端点放得太偏，Layer 3的slot重规划需要大幅调整stub，引入新交叉
- **建议**：初始设为slot_pitch的一半（约12px），在Concentrate模式下为0（共享锚点，不可滑动）

### 6.3 Layer 3的stub重建可能引入新交叉

- **场景**：slot重排后，边A的新stub和边B的stub在节点附近交叉
- **对策**：stub重建后做局部冲突检测，冲突时做等距微调（stub平行偏移），而非重路由。这和当前`fix_slot_inversions`的重路由不同——stub方向确定后，平行偏移不会改变中段。

### 6.4 高密度场景的回退

- 同子组边数≥4时（Concentrate策略），所有边共享一个锚点，slot排序无意义
- 此时跳过Layer 3的slot重排（锚点已由Concentrate确定），但仍执行Layer 4 bundling
- 极端高密度（≥8条同侧边）可考虑完全回退到当前"slot先行"流水线，作为安全网

### 6.5 性能影响

| 阶段 | 当前 | 新架构 | 变化 |
|------|------|--------|------|
| Side选择 | O(n) 逐对几何计算 | O(n) 逐对+group感知 | 略增（查group结构） |
| 路由 | O(n × candidates) | O(n × candidates)（锚点区域稍微增加候选） | 略增或持平 |
| Slot重规划 | fix_slot_inversions（冒泡交换+重路由） | 全局排序+stub重建（不重路由） | 可能更快（无重路由） |
| Bundling | 已有 | 已有（适配接口） | 不变 |
| 标签 | 已有 | 已有 | 不变 |

总体性能预期持平或略好（Layer 3比fix_slot_inversions少做重路由）。

---

## 七、与原始"slot后置提案"的关系

用户原始提案：
> 1. 把每个entity当做point，路由前不考虑磁吸点
> 2. 路由完成后，在不改变大布局的情况下规划磁吸点，考虑箭头合并
> 3. 确定磁吸点后微调

**评估**：
- 大方向（slot在路由后确定）✅ 正确，对应Layer 3
- "node当point"表述需修正：side必须在路由前确定，不是point-to-point对角线路由，对应Layer 1
- "不改变大布局"在本架构中形式化为"Trunk中段不变"，对应Layer 3的约束
- "箭头合并"属于Layer 4（bundling），不在Layer 3职责内
- "减少计算量"❌ 不成立，这不是性能优化，是质量优化
- 提案正确识别了slot需要路由后确定，但缺少：side选择需要group感知（Layer 1改进）、路由需要优先组间通道（Layer 2改进）、层间需要明确的信息契约

---

## 八、渐进实施路径（面向 AI Agent）

> 本节为 AI Agent 设计的可执行任务清单。每个任务（Task）都是**独立可编译、可测试、可回滚**的最小单元。每个 Task 包含：
> - **目标**：要做什么
> - **修改文件**：精确到文件和函数
> - **实现步骤**：按顺序执行的具体操作
> - **验证方法**：如何确认做对了
> - **回滚策略**：出问题怎么撤
> - **前置依赖**：必须先完成哪个Task
>
> Agent 执行规则：严格按顺序执行，完成一个Task并验证通过后再开始下一个。每个Task完成后运行 `cargo test -p drawify-core` 和 `cargo check` 确认无编译错误。

---

### 阶段 P0：Group感知的Side选择（Layer 1）

**目标**：让side选择感知group结构，跨水平/垂直排列sibling group时优先选择利用组间走廊的side。不改路由逻辑、不改bundling。

**预计影响文件**：`group/context.rs`、`edge/edge_routing_orthogonal/slot.rs`、`edge/edge_routing_orthogonal/mod.rs`

---

#### Task P0-1：GroupRoutingContext增加group关系预计算

**前置依赖**：无（可以从当前代码直接开始）

**修改文件**：
- `crates/drawify-core/src/layout/group/context.rs`：
  - 在 `GroupRoutingContext` 结构体中新增字段
  - 修改 `from_layout` 方法
  - 新增辅助函数

**实现步骤**：

1. 在 `context.rs` 中定义 `SiblingOrientation` 枚举：
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiblingOrientation {
    /// 水平排列（左右并排，垂直投影重叠大）：优先 Left/Right
    Horizontal,
    /// 垂直排列（上下堆叠，水平投影重叠）：优先 Top/Bottom 但需位置对齐检查
    Vertical,
}
```

2. 在 `GroupRoutingContext` 结构体中新增4个字段：
```rust
pub node_leaf_group: HashMap<String, String>,       // node_id → 最深层leaf group id
pub sibling_sets: Vec<Vec<String>>,                 // 每组sibling group列表
pub sibling_orientation: HashMap<(String, String), SiblingOrientation>, // (gid_a,gid_b)→排列方向（a<=b有序）
pub group_ancestors: HashMap<String, Vec<String>>,  // gid → 祖先链（从父到根）
```

3. 新增 `build_group_hierarchy(diagram: &Diagram) -> (node_leaf_group, sibling_sets, sibling_orientation, group_ancestors)` 函数：
   - 遍历 `diagram.groups`，通过 `parent_id`/`child_group_ids` 构建parent→children映射
   - BFS收集sibling_sets（从root group开始，每层的children是一个sibling set）
   - 对每个sibling set内的每对group，比较bbox的ox/oy判定orientation：
     - `oy >= 0.5 * min(h_a, h_b)` 且 `ox <= GROUP_GAP_THRESHOLD`（24px）→ Horizontal
     - `ox >= 0.5 * min(w_a, w_b)` 且 `oy <= GROUP_GAP_THRESHOLD` → Vertical
     - 其他情况：按主轴判定（|dy|≥|dx|→Vertical，否则Horizontal）
   - sibling_orientation的key使用有序对(lexicographic min, lexicographic max)保证确定性
   - node_leaf_group：对每个节点，在`node_to_groups`的列表中取depth最大的group（需要知道group depth，可从diagram.groups获取）
   - group_ancestors：沿parent_id链向上收集直到根

4. 修改 `from_layout` 方法：在构建完 `node_to_groups` 和 `corridors` 后，调用 `build_group_hierarchy(diagram)` 填充新字段。

5. 为 `GroupRoutingContext` 新增便利方法：
```rust
pub fn node_leaf_group(&self, node_id: &str) -> Option<&str> { ... }
pub fn sibling_orientation(&self, ga: &str, gb: &str) -> Option<SiblingOrientation> { ... }
pub fn is_same_leaf_group(&self, a: &str, b: &str) -> bool { ... }
```

6. 更新context.rs中已有的单元测试，覆盖新字段。

**验证方法**：
- `cargo test -p drawify-core group::context` 所有测试通过
- 写一个单元测试：创建包含2个水平排列sibling group的diagram，验证sibling_orientation返回Horizontal
- 写一个单元测试：创建包含2个垂直排列sibling group的diagram，验证sibling_orientation返回Vertical

**回滚策略**：git revert 本次commit。新字段不影响现有代码（只新增，不修改已有字段和方法签名）。

---

#### Task P0-2：重写choose_pair_sides为分级决策树

**前置依赖**：P0-1

**修改文件**：
- `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/slot.rs`：重写 `choose_pair_sides` 函数，新增辅助函数

**实现步骤**：

1. 修改 `choose_pair_sides` 签名，增加参数：
```rust
pub fn choose_pair_sides(
    a: &NodeLayout, b: &NodeLayout,
    a_id: &str, b_id: &str,
    group_ctx: Option<&crate::layout::group::GroupRoutingContext>,
) -> (Port, Port)
```
使用`Option`保持向后兼容：`None`时走旧逻辑（用于测试和不感知group的场景）。

2. 在文件顶部新增常量：
```rust
const VERTICAL_PREFERENCE_THRESHOLD_SAME_GROUP: f64 = 0.4;
const VERTICAL_PREFERENCE_THRESHOLD_HORIZONTAL_SIBLINGS: f64 = 0.8;
const VERTICAL_PREFERENCE_THRESHOLD_VERTICAL_SIBLINGS_ALIGNED: f64 = 0.4;
const VERTICAL_PREFERENCE_THRESHOLD_VERTICAL_SIBLINGS_UNALIGNED: f64 = 0.5;
const VERTICAL_PREFERENCE_THRESHOLD_CROSS_ANCESTOR: f64 = 0.5;
const SIDE_ALIGN_MARGIN: f64 = 20.0;
const EXIT_CHECK_DISTANCE: f64 = 32.0; // PORT_CLEARANCE(16) + stub_clearance(16)
```

3. 实现决策树函数体（Step 1→2→3→4）：

**Step 1：硬约束排除**
- 计算ac, bc, dx, dy, ox, oy（已有）
- 生成合法候选列表：
  - dy > -EPS → (Bottom, Top) 合法
  - dy < EPS → (Top, Bottom) 合法
  - dx > -EPS → (Right, Left) 合法
  - dx < EPS → (Left, Right) 合法
- 投影重叠强制：ox>EPS且oy≤EPS时只保留Vertical对；oy>EPS且ox≤EPS时只保留Horizontal对

**Step 2：单候选快速返回**
- candidates.len() == 1 → 直接返回
- candidates.is_empty() → 回退到fallback_by_axis(0.4)

**Step 3：Group关系分类**（仅当group_ctx.is_some()时执行，否则全部用0.4阈值）
- 情况A（同组）：ctx.is_same_leaf_group(a_id, b_id) → 用0.4阈值
- 情况B（水平sibling）：orientation == Horizontal → 用0.8阈值
- 情况C（垂直sibling）：orientation == Vertical → 位置对齐检查
  - 获取目标group的bbox：`group_ctx.groups[target_gid]`
  - aligned = source_center.x 在 [gb.x - MARGIN, gb.x + gb.width + MARGIN]
  - 对齐 → 0.4阈值
  - 不对齐：检查是否有水平走廊（GroupCorridor with Horizontal axis在两个group之间），有走廊且source在目标右侧(dx<0)→返回(Left, Right)；有走廊且source在目标左侧(dx>0)→返回(Right, Left)；无走廊→0.5阈值
- 情况D（跨祖先）：LCA不同→查corridors找连接走廊，找到则选朝向走廊的side；找不到→0.5阈值

**Step 4：轻量出口校验**
- 对Step3选出的首选side，调用`has_clear_exit(from_nl, from_side, group_ctx)`
- 实现`has_clear_exit`：从锚点沿side外向方向投射EXIT_CHECK_DISTANCE长度的线段，用AABB检测是否与任何sibling group bbox相交（不相交→ture）
- 如果首选不通过，遍历candidates找第一个通过的备选，找到则用备选

4. 提取内部辅助函数（不pub，仅文件内使用）：
   - `fallback_by_axis(dx, dy, threshold) -> (Port, Port)`：按主轴阈值选side（现有逻辑）
   - `has_clear_exit(nl, side, ctx) -> bool`：AABB射线检测
   - `corridor_between_groups(ctx, ga, gb, axis) -> Option<&GroupCorridor>`：查找两group间特定轴的走廊

5. 保留旧的 `choose_pair_sides(a, b)` 作为简单包装（调用带group_ctx=None的新函数），确保不破坏现有单元测试。

**验证方法**：
- `cargo test -p drawify-core orthogonal` 现有测试全通过
- 新增单元测试：
  - 两节点同组 → 返回结果与旧逻辑一致
  - 两节点在水平排列sibling group中，dy小dx大 → 返回Left/Right
  - lb→biz_svc场景（public_subnet右下方到private_subnet左上方，位置不对齐）→ 返回Left→Right
- 跑 `showcase/architecture/c.layout-stress-nested.dfy` 导出layout，检查lb→biz_svc是否改走Left→Right

**回滚策略**：`choose_pair_sides` 加了`group_ctx: Option<&GroupRoutingContext>`参数，传None就完全是旧行为。如果新逻辑有问题，临时把调用处的group_ctx传None即可。

---

#### Task P0-3：适配调用方并更新coordinate_port_sides

**前置依赖**：P0-2

**修改文件**：
- `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs`：
  - 更新 `choose_pair_sides` 调用处（第306行）传入a_id, b_id, group_ctx
  - 更新 `side_acceptable` 函数（第1210行）以感知group关系
  - 保持 `coordinate_port_sides` 签名不变，但内部的 `side_acceptable` 调用需要传group_ctx

**实现步骤**：

1. 在 `route_edges_orthogonal_inner` 第306行，将：
```rust
let (side_a, side_b) = choose_pair_sides(a_nl, b_nl);
```
改为：
```rust
let (side_a, side_b) = choose_pair_sides(a_nl, b_nl, can_from, can_to, Some(&group_ctx));
```
需要把`can_from`/`can_to`（字符串id）传入。当前第298行已经有`can_from`和`can_to`变量。

2. 修改 `coordinate_port_sides` 函数签名，增加 `group_ctx: Option<&GroupRoutingContext>` 参数。在函数内部传递给 `side_acceptable`。

3. 修改 `side_acceptable` 函数：增加 `group_ctx: Option<&GroupRoutingContext>` 和 `from_id: &str, to_id: &str` 参数。当group_ctx为Some时，使用和choose_pair_sides相同的分类逻辑判断可接受性（不是固定0.4阈值，而是按group关系调整阈值）；当group_ctx为None时，保持0.4阈值不变。

4. 更新 `coordinate_port_sides` 的调用处（第326行），传入`Some(&group_ctx)`。

**验证方法**：
- `cargo check -p drawify-core` 无编译错误
- `cargo test -p drawify-core` 所有测试通过
- 运行 `c.layout-stress-nested.dfy` 验证：
  - lb→biz_svc 选Left→Right
  - 同组边（db_master→db_replica等）side选择与之前一致（仍走Top/Bottom）
  - 无group的简单流程图测试用例行为不变

**回滚策略**：在所有调用处传None给group_ctx参数，立即回退到旧行为。

---

### 阶段 P1：路由通道选择修复（Layer 2）

**目标**：修复path.rs中外道绕行问题——同组边不应绕出group外，通道候选应优先组间间隙。

**预计影响文件**：`edge/edge_routing_orthogonal/path.rs`、`edge/edge_routing_orthogonal/scoring.rs`

**前置依赖**：P0（但也可以独立于P0开发，P0只改side选择，P1改路由）

---

#### Task P1-1：修复同组边的group障碍物处理

**修改文件**：
- `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/scoring.rs`：修改 `obstacle_penalty` 中group障碍物判断
- `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/path.rs`：如有需要，修改候选生成中的group障碍物过滤

**实现步骤**：

1. 在 `scoring.rs` 的 `obstacle_penalty` 函数中（约第82-161行），找到group障碍物检测逻辑。
2. 当前逻辑：段穿越group内部且该group不在`endpoint_group_set`中→惩罚。
3. 修改：`endpoint_group_set` 应包含端点所在group的**所有祖先group**（不仅仅是直接所属group），参考lint模块中 `endpoint_related_groups` 的实现。因为边在同一个leaf group内，其leaf group的父group也不应被视为障碍物。
4. 同时：对于同leaf group内的边，其leaf group本身的borders也不应成为需要绕行的硬障碍（但border_shell壳层检测仍需保留，避免贴边平行）。
5. 验证：检查`path_avoids_group_interiors`函数是否也有同样问题（只检查直接group不检查祖先）。

**验证方法**：
- `cargo test -p drawify-core` 通过
- db_master→db_replica（同data_subnet内垂直对齐dx≈0）不再绕cloud外
- 跨组边（lb→biz_svc）仍正确避开无关group内部

**回滚策略**：git revert。

---

#### Task P1-2：候选通道优先组间间隙

**修改文件**：
- `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/path.rs`：修改 `build_channel_detours` 函数

**实现步骤**：

1. 找到 `build_channel_detours` 函数（生成绕行通道候选的函数）。
2. 当前问题：绕行通道坐标倾向于选择障碍物外侧（更大范围绕远），而非group之间的间隙。
3. 修改：在生成绕行通道坐标时，优先使用GroupCorridors中定义的走廊坐标（`group_ctx.corridors`）。
4. 对于需要绕行障碍物的Z-fold和staircase路径，优先尝试朝向最近的group走廊方向绕行，而非默认朝向外侧（x更小或更大的方向）。
5. 具体策略：
   - 绕行方向选择：不是简单地选min_x-epsilon或max_x+epsilon，而是检查是否有GroupCorridor在障碍物与目标之间，若有则朝走廊方向绕行
   - 通道Y/X坐标：优先使用走廊的coord值（走廊中线），而非障碍物边缘+margin

**验证方法**：
- `cargo test -p drawify-core` 通过
- biz_svc→db_master不再绕到cloud左侧外（应走y=366~462的水平走廊）
- 其他边路径不退化

**回滚策略**：git revert。

---

#### Task P1-3：评分函数权重调整

**修改文件**：
- `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/scoring.rs`

**实现步骤**：

1. 找到路径评分函数（`CandidateScorer::score`或相关），检查以下惩罚项的权重：
   - 路径长度（BEND_PENALTY是折点惩罚，需确认是否有length penalty）
   - 远离已有边的奖励/惩罚（EDGE_OVERLAP_PENALTY）
   - 靠近group边界的惩罚
2. 如果路径长度惩罚权重过低（或不存在），增加路径长度惩罚项，使得长的外道绕行评分高于短的内道走廊路径。
3. 初始建议：length_penalty_weight = 0.5（每像素0.5分惩罚，与BEND_PENALTY=16形成合理比例）。
4. 调整后运行测试，对比总路径长度变化。

**验证方法**：
- `cargo test -p drawify-core` 通过
- 对比P0+P1修复前后，`c.layout-stress-nested`的总路径长度应显著减少
- 折点数不增加（或减少）

**回滚策略**：恢复原权重常量值。

---

### 阶段 A：Slot重规划（Layer 3）

**目标**：路由完成后根据实际出口方向全局重排slot，重建stub段，替代fix_slot_inversions。

**前置依赖**：P0 + P1

---

#### Task A-1：实现路径分解辅助函数

**修改文件**：
- 新增 `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/slot_replan.rs`（或在mod.rs中实现，视代码量决定）

**实现步骤**：

1. 阅读 `crates/drawify-core/src/layout/edge/edge_bundling/compatibility.rs` 中的 `decompose_path` 函数，理解其路径角色段标记逻辑。
2. 在slot_replan模块中实现或复用路径分解功能，识别FromStub→FirstTurn→Trunk→LastTurn→ToStub。
3. 如果edge_bundling的decompose_path可以直接复用（pub且功能足够），则直接引用；否则在本模块内实现轻量版（只需要找到FirstTurn和LastTurn点位置）。
4. 实现 `extract_exit_direction(path, side, is_from) -> Vec2`：从FirstTurn点提取实际出口切线方向。

**验证方法**：单元测试覆盖：
- Z-fold路径（3段）：正确识别stub→turn→trunk
- staircase路径（5段）：正确识别
- 直线路径（2段）：无trunk，stub即路径本身

**回滚策略**：删除新文件，不影响其他模块。

---

#### Task A-2：实现全局slot重排序

**修改文件**：
- `slot_replan.rs`：新增 `replanner` 函数
- `mod.rs`：在路由完成后调用slot replanner替代fix_slot_inversions

**实现步骤**：

1. 实现 `replan_slots` 函数：
   - 输入：edges, endpoint_map, from_side, to_side, nodes, cfg
   - 步骤：
     a. 按(node_id, side, is_from, arrow_type, line_style)分组（复用现有endpoint_bundling_key逻辑）
     b. 对每个子组，提取每条边的actual_exit_dir（切线方向分量）
     c. 按actual_exit_dir做稳定排序（不是冒泡交换，是一次性全局排序）
     d. 根据新排序重新分配slot frac和anchor坐标
     e. 重新生成FromStub/ToStub段：从新anchor到FirstTurn点生成正交stub
     f. Trunk段（FirstTurn到LastTurn之间）保持不变
2. 处理Concentrate策略（同子组边数≥4）：所有边共享中心锚点，跳过排序但仍需stub重建（如stub方向不变则无需重建）。
3. 实现局部冲突检测：stub重建后，检查同节点其他边的stub段是否相交，若相交则平行偏移stub（不重路由）。

**验证方法**：
- biz_svc→db_master与auth_svc→redis的水平段交叉消除
- Concentrate模式下行为不变
- `cargo test -p drawify-core` 通过

**回滚策略**：暂时保留fix_slot_inversions，通过配置开关切换新旧实现。

---

#### Task A-3：集成到路由流水线并移除fix_slot_inversions

**修改文件**：
- `mod.rs`：在route_edges_orthogonal_inner中用replan_slots替换fix_slot_inversions调用

**实现步骤**：

1. 在 `route_edges_orthogonal_inner` 第585行，将 `fix_slot_inversions(...)` 替换为 `replan_slots(...)`。
2. 删除或保留 `fix_slot_inversions` 函数（建议保留为deprecated私有函数一段时间，确认无问题后删除）。
3. 更新edge统计逻辑（ortho_stats），将fix_slot_inversions的统计改为replan_slots的统计。
4. 确保bundling流水线在replan_slots之后执行（顺序不变）。

**验证方法**：
- `cargo test -p drawify-core` 全部通过
- 所有showcase示例渲染无异常
- 交叉数比P0+P1阶段进一步减少

**回滚策略**：恢复fix_slot_inversions调用。

---

### 阶段 B：锚点区域路由模式（Layer 2增强，可选）

**前置依赖**：阶段A完成且效果验证通过

#### Task B-1：path.rs支持锚点区域端点

**修改文件**：
- `path.rs`：修改路径候选生成，端点允许在slot区域内微调
- `slot.rs`：Endpoint增加anchor_zone字段
- `context.rs`：EndpointPair的from/to改为支持区域

**说明**：此阶段涉及path.rs候选模型的较大改动，具体方案待阶段A效果评估后再细化。如果P0+P1+A已解决大部分美学问题，可跳过此阶段。

---

### 阶段 C：完整整合与bundling适配（可选）

**前置依赖**：阶段A或B完成

#### Task C-1：bundling适配Layer 3输出

#### Task C-2：高密度回退模式完善

#### Task C-3：性能基准测试

**说明**：收尾整合工作，具体方案待前面阶段完成后再细化。

---

## 九、待确认问题

1. ~~`c.layout-stress-nested`中具体哪些边出了什么问题？~~ **已通过坐标验证，详见§2.2**
2. **Side选择阈值参数**：
   - 情况B（水平sibling）的垂直偏好阈值0.8是否合适？还是用连续权重更好？
   - 情况C（垂直sibling不对齐时）是直接硬切换到朝向走廊的side，还是用阈值微调？（建议直接硬切换，因为位置不对齐时走垂直端口几乎必然绕外道）
   - 位置对齐检查的margin取多大？（建议corridor_misalignment_penalty相关值，约20px）
3. **Side选择Step 4的出口校验距离**：PORT_CLEARANCE+stub_clearance多长合适？过短检测不到撞墙，过长误判。
4. **coordinate_port_sides是否需要group感知**：当前`side_acceptable`用0.4固定阈值判断侧切是否可接受，P0阶段可先保留0.4，后续根据效果决定是否升级。
5. 阶段A的路径分解能否直接复用edge_bundling的`decompose_path`？需要哪些适配？
6. 锚点区域给路由多大自由度合适？（初始建议slot_pitch/2 ≈ 12px）
7. 评分函数中路径长度惩罚和"远离已有边"奖励的最优权重比是多少？
8. 高密度回退的阈值：同侧边数≥4用Concentrate+跳过slot重排，是否合适？

---

## 十、下一步行动

1. **P0-1：扩展GroupRoutingContext**——在构建时预计算node_leaf_group、sibling_orientation、group_ancestors映射
2. **P0-2：实现Side选择决策树**——按§4.2的4步决策树重写choose_pair_sides（硬约束→快速返回→4情况分类→出口校验）
3. **P0-3：适配验证**——更新调用方，用`c.layout-stress-nested`验证lb→biz_svc/auth_svc改走Left/Right，同组边和无group场景行为不变
4. **P1：修复路由通道选择**——让build_channel_detours优先组间间隙、同组边豁免group障碍物、调整评分权重
5. **阶段A：实现Slot重规划**——路由后全局排序+stub重建，替代fix_slot_inversions
6. **效果量化**：对比各阶段修复前后交叉数、总路径长度、折点数、平均ink量
7. **决定是否推进阶段B/C**：基于以上效果决定
