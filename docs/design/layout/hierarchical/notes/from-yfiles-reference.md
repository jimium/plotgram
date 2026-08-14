# Hierarchical：yFiles 参考文库的启发（Hier 构造与 v1 Atlas 迁移）

> 日期：2026-07-31  
> 状态：讨论纪要（现行设计参考，非进度债单）  
> 范围：读完 [`docs/reference/yfiles/`](../../../../reference/yfiles/00-索引与阅读指南.md) 后，对**构造 Hierarchical**、**重建 `plotgram-engine`**、**从 v1 Atlas 迁移**的启发收敛  
> 相关：[architecture.md](../architecture.md) · [scope.md](../scope.md) · [写权纪律](../../write-authority.md) · [reference 13 选型与路线图](../../../../reference/yfiles/13-实现路线图与选型.md)

本文是一次对照阅读的纪要，方便慢慢消化。算法细节仍以 reference 各篇为准；实现进度不写在这里。  
**现行架构以 [architecture.md](../architecture.md) 为准**；本文不替代之。  
否决路线：[anti-patterns.md](anti-patterns.md)。


---

## 0. 总判断

这套参考文库（尤其 **13 / 09 / 20 / 08 / 03 / 15 / 14**）对造 Hier、迁 v1 Atlas **高度可用**：不只是百科，而是已经收敛成「选型表 + 反模式 + 可对号入座的零件」。

| 资料层 | 对 Hier / Atlas 迁移的作用 |
|--------|---------------------------|
| **01 + 13 选型表** | 重建主路径该装哪些算法（FAS / NS / median / BK→VPSC）——可当验收清单 |
| **09 + Writer / Stage** | 解释 v1 为何痛、重建应如何把写权变成类型 |
| **03 + 18** | **证明** Atlas「删 OVG、走 Channel」是对的；并补上 track / nudging / demand 回写的标准形态 |
| **08** | Weak/StrongMacro、泳道机制 = 「边界 dummy + 连续块 + VPSC 组框」（与组同构，政策不同）；DSL/model 已落地 [PartitionGrid](../../../adr/008-partition-grid.md)，不是两套组几何 |
| **20 DemandBoard** | 把写权纪律里「唯一允许的反向影响」落成可实现协议 |
| **15** | Ink 边界写清：裁剪/圆角/规范化 ≠ 发明端口或挪节点 |
| **16 / 17 / 19** | 性能、动画、Sequence——**别挡 Hier MVP** |

与现有纪律高度同向；真正增量是：**把「该迁什么 / 该换什么 / 先做什么」钉死了**。

---

## 1. 对现有方向的强确认（少走回头路）

### 1.1 Hier = Sugiyama + 通道路由，不是 TSM / OVG 主路径

- 出处：13 §1、03 §1.3。  
- 分层图用**通道图**；OVG 留给自由位置 / 拖动后重路由。  
- v1 删 OVG、上 Channel，与文献结论一致——迁移时**不要**把 OVG 捡回来。

### 1.2 端口必须在组合相，不能 Ink 发明

- 出处：13「端口五档」、08、15，与 [写权纪律](../../write-authority.md) 同一句话。  
- 重建 stub 里端口 `unwrap_or` 默认侧是明确违规；迁 Atlas 时优先保 Plan 端口决议。

### 1.3 参数化 profile，禁止引擎图种分支

- 出处：20 与 [ADR-001](../../../adr/001-diagram-type-not-in-engine.md) 同构。  
- flowchart / architecture 共用 Hier，差异只在参数表（20 §5）。  
- 迁移目标仍是「**一个**组合相 + group policy 参数」，不是保留 flat / weak / strong 三宇宙。

### 1.4 参数回写，不是事后挪节点

- 走廊 demand → 层间距；端口数 → 节点最小尺寸。  
- 这解释了 Atlas「有界返工 / Demand」在试图做什么，以及为什么 Ink dogleg 是错的。  
- 写权允许的唯一反向影响应走显式 **DemandBoard**（见 §2.3），且必须在上游相执行**之前**汇总。

---

## 2. 相对 Atlas / 当前设计档的新启发

### 2.1 算法选型比 v1「够用」更具体

| 相 | 资料建议 | v1 / 重建现状 | 迁移含义 |
|----|----------|---------------|----------|
| P1 | Greedy-FAS | v1 有；重建无显式反向 | **必迁**反向位到 Ink |
| P2 | Network Simplex | v1 NS 风格；重建最长路 | 重建不要长期停在 longest-path |
| P3 | median + transpose + **边权 1/2/8** + best snapshot | v1 median；边权/snapshot 未必齐 | **高 ROI 补丁**（13 §5 #1） |
| P4 | BK 起步 → 约束多了换 **VPSC** | v1 BK+LP；重建打包 | 组框/端口/标签一多，别在 BK 上叠特判；v1 PAVA 栈态度见 [笔记](../../../notes/v1-atlas-solver-vs-vpsc.md) |
| 路由下游 | A*（含方向）→ 区间着色 track → VPSC nudge | v1 Channel+LexA*；nudging/VPSC 未必统一 | 迁拓扑，**统一** track+nudge 零件 |

「先上真 MCF」资料也不当第一刀；**history cost + 拆线重布**（18）才是挤爆走廊的正解——与写权纪律「有界返工优先」一致。

### 2.2 Writer / Stage 是迁移架构的关键缺口

出处：09。

- 自由度用 `CycleWriter` / `LayerWriter` / `RouteTopoWriter` / `CoordWriter` 等收紧。  
- Stage 装饰器处理自环、平行边、分量、**Orientation（核心只实现 TB，四方向靠坐标变换）**。  

v1 Atlas 有三相语义，但组合相仍三路径、写权靠纪律不靠类型。重建若只抄 `rank / place / ports / ink` 文件名、不引入 Writer + Orientation Stage，会重演「四方向四套 / 下游偷改」。

**启发**：迁功能前先定 M0 地基（13）：LayoutGraph 稳定序 + Writer + DemandBoard + 度量，再搬 layered / channel。

### 2.3 DemandBoard = 写权的可实现协议

出处：20 §4。

把「唯一反向通道」写成显式板：

- 下游在预算阶段贡献需求（走廊、端口数→节点尺寸、标签带高等）；  
- 合并只用 `max`（顺序无关 → 确定性）；  
- **只有一个**参数写者在上游执行前写入；  
- 若需求在上游已执行后才产生 → 相顺序设计错误，不是「就地挪一下」。

Atlas 有 track demand / label band 等碎片；缺统一板时，迁移容易再散落赋值。  
**建议**：在 `design/layout/shared/`（或 routing）收成共享契约后，Hier pipeline 只引用。

### 2.4 组 / 泳道：机制同构，政策不同

出处：08；落地：[ADR-008](../../../adr/008-partition-grid.md) · [shared/partition](../../shared/partition.md)。

- 组与泳道都是「层内连续块」——**边界 dummy + 受约束排序**，机制同构、政策不同；  
- 泳道在 DSL / model 侧已是 **PartitionGrid 一等公民**（`partition { column … }` + `cell_col`；仅列 = 泳道），不再用 group「演」；Hier 引擎消费 **已落地**（PG-0–PG-4）；render 只读 band 画底色/标题；  
- 组框 / 列区间都是约束求解变量（VPSC 或等价分隔约束），**禁止事后包围盒当真源**。  

对照 archive 30·R3：Weak / StrongMacro 应是收缩 / 连续块**参数**；Horizontal 堆叠 ≠ PartitionGrid（ADR-008 明令禁止以 group Horizontal 冒充分区）。  
重建不要 `finalize` 后验组框当布局真源（与 08、13 反模式表直接冲突）。

### 2.5 Ink 该迁的是「规范化流水线」，不是决策

出处：15。

- 裁剪、共线合并、snap、降半径、箭头缩进——全是落笔；  
- 端口 / track 只读。  

v1 `ink_verify` 方向对；迁移时把 15 的六步规范化当 Ink 清单，避免把 channel 搜索逻辑塞进 Ink。

### 2.6 投入产出比可直接当迁移排序

出处：13 §5（及 14–20 补充）。

对「先迁什么」很实用（按单位工作量的观感/能力提升）：

1. 度量 / 分 profile 报告（否则不知迁坏没有）  
2. P3 边权 1/2/8 + best snapshot  
3. Orientation Stage  
4. VPSC（一物多用于间距、消重叠、nudging、组框等）  
5. 端口按对侧序分配  
6. label 空间预留（dummy / Demand）  
7. 走廊 demand 回写层间距  
8. history cost + 拆线重布（替代「给某条边加特判」）  

补充（读完 14–20 后）：

- 一维排列器（约 200 行）可插在前列——服务生命线 / 泳道 / 组内序 / circular 等多处（19）；  
- history rip-up 接在走廊 demand 之后（18）。  

这比「整包搬 Atlas」更可验收。

---

## 3. 对「构造 Hier + 迁 v1 Atlas」的可执行建议

### 3.1 文档对齐（低成本）

- `hierarchical/phases/*` 直接挂 01 / 03 / 08 / 15 章节，避免再写第二套百科。  
- [写权纪律](../../write-authority.md) 可补一句：唯一反向 = DemandBoard（链 20 §4）。  
- 13 的**反模式表**作迁移 code review 清单（路由挪节点、事后组框、HashMap 序、Ink 发明端口、四方向四套等）。

### 3.2 重建代码顺序（相对 stub）

**不要**「Atlas 目录原样拷贝」。建议：

```text
M0  Writer + 稳定 LayoutGraph + DemandBoard + 交叉/重叠度量
M1  FAS → NS → proper/dummy → median(+权) → BK → dummy 折线
M2  Channel 拓扑（从 v1 迁）+ track 着色 + demand 回写；共享 nudge
M3  端口五档 + label reserve
M4  边界 dummy 连续块 + 组框进约束求解（收掉三路径）
```

与 reference 13 的 M0–M4 对齐；细节验收标准见该篇。

### 3.3 从 v1 迁什么 / 不迁什么

| 优先迁（功能与坑的真源） | 慎迁或重写 |
|--------------------------|------------|
| Plan / Channel 拓扑 | flat / weak / strong 三路径 solve 壳 |
| 端口 along 决议 | 事后 `LabelSolver` 当终态 |
| `ink_verify` 门禁 | `finalize` 后验组框当真源 |
| FAS / 反向位 | 任何 Ink 发明（端口、track、穿组 dogleg） |
| NS / BK 骨架 | OVG / 第二宇宙兼容层 |

**一句话**：v1 是功能与坑的真源，**不是**目录结构的真源。

### 3.4 明确后置（不挡 Hier 主路径闭环）

- 16 大图降级阶梯  
- 17 动画 / morphing  
- from-sketch / PartialLayout  
- Hier 消费 PartitionGrid（P3 列→层内连续块 / P2 行→层区间 / P4 列区间变量）——DSL · model · parser 已落地（[ADR-008](../../../adr/008-partition-grid.md)），剩引擎接线  
- organic / 力导向  

### 3.5 一次验证参数域（约半天）

出处：20 §6「新图种落地测试」。

拿尚未支持的图种（如 BPMN、甘特）**只用参数表表达**；填不出的格 = 缺的通用能力（应扩参数域 / 机制），不是图种分支。  
提前暴露缺 `group_order_policy` / `bundling` / `port_granularity` 等，避免 M4 做到一半改 Contract。

引擎侧应用 CI / crate 边界守卫：grep `DiagramType` / `profile` 在 `plotgram-engine` 内应为 0（20 §6 检验 2）。

---

## 4. 推荐阅读路径（慢慢看）

结合 reference 索引的 Hier 路径，再加本纪要：

1. [写权纪律](../../write-authority.md)  
2. **本文**（全局启发）  
3. [13 选型与路线图](../../../../reference/yfiles/13-实现路线图与选型.md)（M0–M6 + 反模式 + ROI）  
4. [01 Sugiyama](../../../../reference/yfiles/01-sugiyama分层布局.md) → [03 正交路由](../../../../reference/yfiles/03-正交边路由.md) → [08 分组端口](../../../../reference/yfiles/08-分组泳道与端口约束.md)  
5. [09 引擎架构](../../../../reference/yfiles/09-yfiles类引擎架构.md) → [20 profile / DemandBoard](../../../../reference/yfiles/20-图种profile与参数映射.md) → [15 落笔层](../../../../reference/yfiles/15-几何与落笔层.md)  
6. 需要零件时翻 [14 工具箱](../../../../reference/yfiles/14-图论与优化工具箱.md)；术语卡壳翻 [21 术语表](../../../../reference/yfiles/21-术语表.md)  
7. 回溯决策动机时再读 `docs/archive/atlas/`（21 → 22 → 30，只读）

---

## 5. 一句话

这套资料把 Atlas 已走对的路（Channel、写权、profile、删 OVG）**证成了标准答案**，并把重建缺口钉成：

> **Writer / DemandBoard / VPSC / Orientation + 按 ROI 迁 P3 → 路由下游 → 端口标签 → 组连续块**

v1 是功能与坑的真源，不是目录结构的真源。
