# 共线与合流：产品定位与期望方案

> 日期：2026-07-18  
> 状态：**产品规定（文档）** — 只定义期望，不包含代码改动  
> 可视化对照：[`collinear-and-arrow-merge-comparison.html`](./collinear-and-arrow-merge-comparison.html)  
> 既有实现侧纲领（可演进）：[`../../已经实现的方案/collinear-problem-analysis-2026-07.md`](../../../backups/已经实现的方案/collinear-problem-analysis-2026-07.md)  
> 工程约束：[`../../总结经验/布局与路由核心手册-2026-07.md`](../../总结经验/布局与路由核心手册-2026-07.md)、[`AGENTS.md`](../../../AGENTS.md)

---

## 0. 文档定位

本文回答三件事：

1. **同行怎么选**（决策输入，非照搬）  
2. **本项目期望什么**（规范性产品规定 — 见 §2）  
3. **与现状的差距怎么读**（文档对照，不在此改代码）

**读法：** §2 是权威；§1 / §3 是依据与对照。实现若与 §2 冲突，以改代码对齐 §2 为准（另立执行计划），不以「代码现状」回写稀释产品规定。

---

## 1. 同行实践摘要（决策输入）

成熟正交路由产品对「共线」并非一锅炖，而是先分层再选型：

| 层 | 问题 | Graphviz | ELK | yWorks | 对本项目的启示 |
|----|------|----------|-----|--------|----------------|
| 表示 | 单边冗余折点 | 简化 | 简化 | 简化 | **等价删点**，与合流无关 |
| 事故 | 无关边 trunk 重合 | 常 squish | **nudging 分离** | **nudging + channel** | **默认分离** |
| 有意 | 同源/同宿合流 | `concentrate` 粗暴整边合并 | hyperedge（偏可选） | **bus routing** | **语义授权后才共干** |
| 端口 | 同侧箭头/入口 | 弱 | 一般 | **concentration 招牌** | **优先合 stub，不合整边** |

行业共识可压成一句：

> **端口侧可汇聚 · 主干侧按语义授权 · 无授权一律分车道**

本项目对标：**yWorks 的端口汇聚 + 轻量 bus 子集**，辅以 **ELK 式默认分离**；明确拒绝 Graphviz 式整边 `concentrate` 与后置全文 bundling。

三条决策轴（选型时先答这三问）：

1. **图种**：flowchart 要 fan-out 整齐；architecture 要边可追溯、跨组不糊。  
2. **合并落点**：stub/箭头 ≠ 长 trunk。  
3. **无许可共线**：分离，不靠「碰巧叠在一起」冒充合流。

裁决心智模型：

```text
两段边重叠？
  → 有 merge / docking 语义许可？
       Yes → Allowed（仅在声明共享区间内）
              └─ 越过 fork/merge 边界 → Forbid
       No  → NeedsSeparation（lane / nudging）
              └─ 空间不够 → Degraded(reason)，禁止偷偷穿模换「零重叠」
```

---

## 2. 本项目期望方案（规范性）

> **本节是 Plotgram 对「共线 / 重叠 / 合流 / 箭头汇聚」的产品规定。**  
> 后续算法、lint、基线、profile 开关均应以本节为验收语言。  
> 实现细节（Measure/Classify/Annotation 等）可演进，但不得违背本节的允许/禁止边界。

### 2.1 产品定位声明

| 维度 | 本项目选择 |
|------|------------|
| 产品身份 | **可追溯边的正交图**（Agent/架构/流程），不是「省墨海报」 |
| 合流哲学 | **语义授权合流**；禁止几何巧合冒充合流 |
| 端口哲学 | **分级汇流**（Single / Compact / Concentrate）；合锚点不合边身份 |
| 分离哲学 | 无授权 exact/tight → **必须可解释地分离或 Degraded** |
| 明确不做 | Graphviz 整边 concentrate；后置 path 全文 bundling；图名特判；渲染层反推路由语义 |

一句话定位：

> Plotgram 的「共线」是 **受控产品能力**（端口汇聚 + 有界 trunk 共享），不是 bug，也不是默认墨水压缩。

### 2.2 术语与分层（必须先分类再谈对错）

| 名称 | 定义 | 期望处置 |
|------|------|----------|
| **表示冗余** | 单边折线上三点严格共线 | **删除中点**（等价变换；不改变覆盖） |
| **微台阶 / overshoot** | 短 Z/L、冲过再折回 | **有上下文才改**；否则可暂留；禁止无验证「为更直而穿障」 |
| **非语义重合** | 无共享许可的 trunk exact 或 gap &lt; `parallel_gap` | **NeedsSeparation**；不能分离则 **Degraded(reason)** |
| **有意合流** | 经 merge policy 授权的共享 trunk run | **Allowed**；必须有共享区间端点与分叉/汇合边界 |
| **端口汇聚** | 同侧 stub 共享锚点或紧凑 slot | 按 `DockingStrategy` **Allowed** |
| **T/L 接头** | 端点触及其他边 | **通常 Allowed**（非 trunk 共干） |
| **正反向对** | 同几何对上的对向边 | **始终 Forbid 共干**；维持最小 gap |

**硬原则：**

1. **是否允许重合** ← 路由语义（merge / docking / corridor），**不是**「坐标碰巧相同」。  
2. **是否允许改折点** ← 改后是否仍守约束，**不是**「少一个点更好」。  
3. **共享许可不得从最终几何反推**；只能来自 merge 组键 / docking / 已声明 Annotation。  
4. **共享判定单位** = 段对 + 重叠区间 + 组键交集；不是「每边一个 MergeId」，也不是「整图一条粗线」。

### 2.3 裁决结果（产品语言）

每一对相关平行段（或 stub 关系）在报告与验收中只能落入：

| 结果 | 含义 | 用户可见期望 |
|------|------|----------------|
| `Allowed` | 有语义许可，且落在声明区间内 | 看起来「故意合在一起」；可有 fork/join 形状 |
| `NeedsSeparation` | 无许可或越界共干 | 最终图中应被推开；若仍在，算缺陷或未收敛 |
| `Degraded(reason)` | 空间/障碍导致无法干净分离 | **允许残余**，但必须带原因；禁止用穿节点/穿组换零重叠 |

不以「共线计数归零」为成功标准；以「每条 exact/tight 可归类且无未解释事故」为标准。

### 2.4 产品规则矩阵（按 Profile）

正交路由按图种分型；**对外仍是 `edge_routing: orthogonal`**，对内用 Profile 表达期望差异。

| 场景 | Flowchart | Architecture | 备注 |
|------|-----------|--------------|------|
| 同侧 stub / 箭头 | 分级：1→Single，2–3→Compact，4+→Concentrate | **同左** | 两图种都要「入口整齐」 |
| 同源 fan-out 视觉束 | **期望**：trunk+fork（路由阶段生成） | 不作为默认目标 | flowchart 可读性优先 |
| Trunk 共干 | **默认 Forbid** 几何巧合共干 | **仅** `edges_may_share_trunk` 为真时允许 | 见 §2.5 |
| 共享区间 | 若启用 trunk+fork，须标 fork 点 | 仅声明的 trunk run；越界 Forbid | 清理器不得抹掉边界 |
| 正反向对 | 始终 Forbid；gap ≥ `parallel_gap` | 同左 | 窄域规则，不扩成万能重叠药 |
| 无关边共线 | lane 分离（通用） | **必须**分离 unrelated trunk | architecture 更严 |
| 走廊 / 跨组通道 | 弱偏好独占 lane | **强偏好** 走廊 lane 独占 | 不与无关边糊成一条 |
| 入向「箭头合并」 | **优先端口汇聚**；长 trunk 合并不作为默认目标 | 允许语义 trunk；分叉后 Forbid | 箭头合并 ≠ 整边合并 |

**Profile 一句话：**

- **Flowchart**：好看的 fan-out（端口 + trunk+fork）；偶然共干当缺陷。  
- **Architecture**：边可区分、跨组通道干净；合流必须「说得出组键」。

### 2.5 Trunk 共享授权（期望白名单）

仅当下列条件同时成立时，两条边才可在某区间 `Allowed` 共干：

1. 图种/Profile 允许该种合流（architecture：`semantic_merge`；flowchart：仅 trunk+fork 候选路径所声明的区间）。  
2. 两边至少共享一个 **有效** merge 组键，且该键类型在白名单内：  
   - ✅ `SameSourceFanOut`  
   - ✅ `SameTargetFanIn`  
   - ✅ `ParallelPair`（真平行对，非碰巧同向）  
   - ❌ `SuperEdgePair` **不授予** trunk 共享（只作其它协调，不授共干）  
3. 重叠段落在 **已声明的共享区间** 内；越过分叉/汇合端点后立即 Forbid。  
4. 不是正反向对。

除此之外的 exact/tight trunk 重叠，一律视为 **非语义重合**。

### 2.6 端口汇聚规定（箭头合并的唯一正道）

| 同侧边数 | 策略 | 期望视觉 | 边身份 |
|---------|------|----------|--------|
| 1 | `Single` | 居中 | 独立 |
| 2–3 | `Compact` | 紧凑间距（约 16px 量级） | 独立、可区分 |
| 4+ | `Concentrate` | 共享锚点；近端 stub 外再岔开 | **仍独立**（可标注、可追溯） |

**禁止**把端口汇聚实现成「多条逻辑边渲染成一条几何边且无法区分」。  
**禁止**用长距离 trunk 合并代替端口汇聚来「收拾箭头」。

### 2.7 表示层与改形状层（期望边界）

| 操作 | 期望 | 验证 |
|------|------|------|
| 严格共线删点 | 应做 | 可跳过重验证（等价） |
| lane 平移 / unrelated trunk 分离 | 应对 `NeedsSeparation` 收敛 | 改后不得破坏 stub 方向、merge 边界、穿障约束 |
| 换角 / overshoot 合并 / 强量化位移 | 仅在几何相对冻结后；失败则回退 | **必须**验证 |
| 标签避让 | 最后写标签 | **不改**边路径几何 |

### 2.8 非目标与反模式（期望禁止）

1. 图名 / 节点名特判。  
2. 消灭一切视觉共线（有意合流是能力）。  
3. 无上下文「为更直」而穿节点、穿组、毁 lane/merge。  
4. HashMap 迭代序驱动合流/分道决策。  
5. 导出层（如 drawio）反写布局几何或反推合流语义。  
6. 靠抬高简化 eps「消掉」台阶，冒充修好共线。  
7. 把正反向 gap 扩成全局重叠万能修复器。  
8. 后置 bundling 全文重写路径，再叠二次共线。  
9. 渲染/lint 用 `points.len()` 或固定下标猜测 FanIn/共享干。  
10. 以「共线计数归零」或「单图好看」宣告无退化（须看严重度与节点坐标）。

### 2.9 产品完成定义（DoD）

满足下列全部，才算「共线规定落地」（实现阶段另测；此处是产品验收语言）：

1. 任一 exact/tight trunk 对，可归为 `Allowed` / `NeedsSeparation` / `Degraded(reason)` 之一，且依据可引用 §2.4–2.5。  
2. 有意合流必有组键交集 + 共享区间；不存在「只有坐标相同、说不出许可」的 Allowed。  
3. 端口 4+ 边场景呈现 Concentrate（或显式配置关闭），且边身份仍可区分。  
4. Flowchart 同源 fan-out 呈现 trunk+fork（或等价受控束），且 fork 后不再共干。  
5. Architecture 无关共干被分离或显式 Degraded；走廊不与无关边糊线。  
6. 正反向对始终分道且 gap 达标（或 Degraded 有因）。  
7. 表示冗余被清理；改形状失败可回退；无图名分支；同输入确定可复现。

---

## 3. 与现状的文档对照（不改代码）

> 下列对照只帮助定位「规定 vs 实现/旧文」；**不在本文改代码。**

| 期望（§2） | 现状文档/实现侧大致位置 | 差距读法 |
|------------|-------------------------|----------|
| 语义授权共干 | `edge_merge_policy`、`edges_may_share_trunk` | 方向一致；须保证一切裁决走同一入口 |
| 端口分级汇流 | `DockingStrategy` | 方向一致；须避免与长 trunk 合并混淆 |
| flowchart trunk+fork | Ortho FlowchartProfile / path 候选 | 方向一致；须声明区间，防 sanitize 抹边界 |
| architecture 无关分离 | `separate_unrelated_trunk_overlaps` | 方向一致；须与 Classify 同一语义 |
| Measure≠Classify | `collinear-problem-analysis` 旁路三件套 | 纲领已有；以 §2 为产品边界，实现按执行计划演进 |
| 禁止后置 bundling | `orthogonal-split-and-bundling-removal` | 战略已退役；规定层维持禁止回潮 |
| 完成定义用严重度 | `collinear-baseline` / compare 脚本 | 监测已有；产品 DoD 对齐严重度而非纯计数 |

**规定与旧文冲突时：** 以本文 **§2** 为产品准绳；实现纲领文更新其「产品合流规则」表与本文对齐即可。

---

## 4. 同行对照（压缩）

| 策略 | 代表 | 本项目 |
|------|------|--------|
| 整边合并 concentrate | Graphviz | ❌ 不做 |
| 默认全分离 + nudging | ELK | ✅ 无授权时采用 |
| 端口汇聚 + 语义 bus | yWorks | ✅ 对标（bus 取轻量子集） |
| 后置 edge bundling | 旧实验 | ❌ 禁止回潮 |
| 简单偏移 | draw.io | ❌ 不足以为规定 |

---

## 5. 示意（期望形态）

### 5.1 表示冗余 — 应删

```
before:  ●────●────●────●
after:   ●──────────────●
```

### 5.2 非语义共干 — 应分离

```
  [A]──┐          ┌──[C]
       │ 事故共干 │      →  lane A / lane B
  [B]──┘          └──[D]
```

### 5.3 有意合流 — 有界共享

```
        ┌──► T1
  S ════╪══► T2     ═ Allowed trunk
        └──► T3     ╪ 之后 Forbid 共干
```

### 5.4 端口汇聚 — 合锚点

```
  ─┐
  ─┼─►┌──┐     Concentrate：共享 dock，边仍独立
  ─┘  └──┘
```

### 5.5 Flowchart vs Architecture

```
Flowchart                         Architecture
   S ════╪══► …                      ┌g┐ laneA ┌g┐
        └──► …                       │A├───────┤C│
                                     │B├───────┤D│  ← 无关必分道
                                     └─┘ laneB └─┘
```

---

## 6. 后续（仅规划指针）

本文只冻结 **期望**。落地步骤不在此展开；若开执行，应另文引用 §2 为验收条款，并遵守：

- 只改文档 / 监测 / 行为时拆开；  
- 验证用 `cargo run -p plotgram-cli`；  
- 质量看严重度与节点坐标，不看单图观感。

可视化浏览：打开同目录 HTML。  
实现纲领（旁路 Measure/Classify 等）：见 `docs/已经实现的方案/collinear-problem-analysis-2026-07.md`（其产品表应以本文 §2 为准同步）。
