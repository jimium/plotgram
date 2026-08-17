# 布局事实语料与 Benchmark 商业化讨论

> 状态：proposed（讨论总结，待验证）  
> 日期：2026-08-18  
> 定位：围绕 ADR-007 layout-facts 通道，讨论「PNG + PGM + explain 三元组」对 VLM 的价值、对外 benchmark/语料生意的可行性，以及相对 Mermaid/Graphviz 的差异化路径。  
> **非 ADR、非引擎设计档**；市场判断是推断，需用真实潜在客户验证。技术契约以 [`ADR-007`](../design/adr/007-layout-facts-llm-channel.md) 与 `crates/plotgram-compile/src/explain.rs` 为准。

---

## 0. 一句话

**PNG + PGM + explain 三元组的价值不在「多产图」，而在「多层可信监督 + 可控对照实验」；对外应卖评测服务与合规能力，不卖静态 zip。**

与主战略备忘 [`README.md`](README.md) 的关系：本文是「专有资产 / benchmark」方向的展开；引擎仍是产品本体，三元组是弹药。

---

## 1. layout-facts 能传达多少认知

以 showcase 样例 `hierarchical/fan/auto_edge_grouping.pgm` 的 `--explain` 输出为基准（`layout-facts v0`）：

```text
layout-facts v0
summary: nodes=5 edges=6 groups=0 canvas=312x221
defects: crossings=0 node_overlaps=0 label_overlaps=0 group_intrusions=0
edge(e0) orch->pay bends=2 span=1 detour=right ratio=1.31
edge(e2) orch->ship bends=2 span=1 detour=left ratio=1.31
edge(e3) pay->agg bends=2 span=1 detour=right ratio=1.31
edge(e5) ship->agg bends=2 span=1 detour=left ratio=1.31
bands: 3
band 1 y≈39: orch
band 2 y≈110: pay inv ship
band 3 y≈181: agg
```

### 1.1 仅凭 facts 可直接读出

| 维度 | 内容 |
|------|------|
| 规模 | 5 节点、6 边、0 组；画布约 312×221 |
| 质量结论 | 四类缺陷均为 0 → 布局「干净」 |
| 分层 | 3 个 rank band：顶层 `orch`，中层 `pay inv ship`（左→右），底层 `agg` |
| 部分边路由 | 4 条边（e0/e2/e3/e5）：各 2 弯、跨 1 层、detour ratio≈1.31、左右绕向各异 |

### 1.2 可推理但未写明

- **完整拓扑**：6 边 − 已列 4 边 → 另有 `orch→inv`、`inv→agg` 未描述路由；结合 bands 可还原扇出 + 合流结构。
- **对称性**：外侧 `pay`/`ship` 绕向镜像（右/左），中间 `inv` 两条边未列入 anomaly → 合理推断更直。
- **布局方向**：自上而下层次流。

### 1.3 基本答不了

- 节点人类可读标签（如「编排器」「支付服务」）
- archetype、profile、`auto_edge_grouping` 等布局意图
- bus/trunk 分组、port 共享等机制细节
- 精确坐标、间距、节点框尺寸
- 未超阈值的边的完整路由（见 §1.4）

### 1.4 信息边界（实现契约）

`explain` 故意不写坐标，只输出谓词；边事实有筛选门槛（`explain.rs`）：

- `HIGH_BEND_THRESHOLD = 4`：弯折数低于此且非 detour 的边不列出
- `DETOUR_RATIO = 1.25`：路径/直线比低于此不算绕路

因此 anomaly 列表是**异常优先**，不是全量边目录。`auto_edge_grouping` 的 bus 语义当前不在 facts 词表内。

### 1.5 认知覆盖度（粗估）

| 维度 | 覆盖度 |
|------|--------|
| 布局质量（缺陷类） | ~100% |
| 全局分层结构 | ~90% |
| 完整拓扑 | ~80%（需一步推理） |
| 逐边路由细节 | ~67%（4/6 边） |
| 业务语义 / 布局参数 | ~0% |

**结论**：facts 适合「图干不干净、分层对不对、哪些边绕远了」；不适合「节点叫什么、边有没有分组、精确长什么样」。与 PGM 联读可补全语义与意图。

---

## 2. PNG + PGM + explain 三元组与 VLM

### 2.1 各模态角色

| 模态 | 作用 | 可靠度 |
|------|------|--------|
| **PNG** | VLM 视觉输入（题面） | 高（确定性渲染） |
| **PGM** | 语义 ground truth：节点、边、分组、布局参数 | 最高（意图源） |
| **explain** | 空间/美学 ground truth：分层、绕向、缺陷 | 高（从真实几何导出，零幻觉） |

三者来自同一次确定性求解，标签一致性可构造保证——相对「人写 caption / GPT 看图写描述」的核心差异。

### 2.2 可衍生的监督信号

一份 PGM 经管线可榨多种信号（ADR-007）：

1. **图 → PGM**（de-rendering）：Pix2Struct / DePlot 路线
2. **图 → explain / 图 → QA**：程序化出题，答案绝对正确；直击 VLM 薄弱的空间关系（数交叉、判层级、判绕向）
3. **图 → 自然语言描述**：PGM + explain 模板化拼 caption；LLM 润色不得引入模板外新事实
4. **内容与呈现分离**：同 PGM、多 PNG（主题/字体/方向/间距增广）

### 2.3 eval vs 训练

| 用途 | 判断 | 说明 |
|------|------|------|
| **评测集（eval）** | 强烈推荐 | 可信 chart 理解 benchmark 稀缺；几千条精品即可钉引用；可 diff 布局参数变化 |
| **训练集（SFT）** | 有用但有边界 | 提升结构抽取、层级识别、简单路由/缺陷判断；域偏窄；explain v0 不完整 |
| **RL / 裁判** | 中等 | defects/detour 可当 reward；日常质检仍应优先符号 facts，VLM 当学生、facts 当阅卷标准 |

**ADR-007 立场**：优先做 eval 集而非训练集。

### 2.4 相对「PNG + caption」的护城河

```text
普通 VLM 语料:  PNG + 人/GPT 描述     → 可能幻觉、空间不准、不可 diff
Plotgram 三元组: PNG + PGM + explain   → 同源可验证、可程序化出题、零幻觉标签
```

---

## 3. 对外 benchmark / 语料能否成为生意

### 3.1 总判断

能做成生意，但**很难做成「卖数据集文件」的大生意**。更现实的是卖**可信评测、合规能力、行业垂直包**；数据集是交付物，不是产品核心。

| 方向 | 商业化难度 | 天花板 | 备注 |
|------|-----------|--------|------|
| 通用 chart VLM benchmark | 高 | 低～中 | 常需开源换引用 |
| 垂直行业评测包 | 中 | 中 | 绑具体采购场景 |
| 无障碍 / 合规描述（EAA/WCAG） | 中 | 中～偏高 | 政府、金融、教育 |
| 评测即服务（EaaS） | 中 | 中 | 语料不全量交付 |
| 裸卖 PNG+PGM+explain zip | 偏高 | 偏低 | 不推荐作主产品 |

### 3.2 真实买家

1. **大模型厂商**：要 model card、分项评测；多买服务/合作，少高价买静态语料
2. **图表 / 协作 SaaS**：要「AI 读图/改图」可信度证明；买垂直评测 + 格式定制
3. **合规 / 无障碍**：EAA、WCAG 硬需求；**比纯 VLM 训练更接近真采购**
4. **企业 AI / RAG 集成商**：要架构图/流程图结构化准确率报告 + API
5. **研究机构**：引用驱动，付费弱；价值在品牌与标准

### 3.3 较可行的商业模式

| 模式 | 要点 |
|------|------|
| **A. Benchmark-as-a-Service** | 提交模型 → 固定题库 → 分项报告；语料闭源 holdout |
| **B. 行业垂直评测包** | 微服务架构图、审批流等；含 baseline 与更新日志 |
| **C. 合规描述 API** | 图 → 无障碍叙述 + layout facts + 审计；与引擎绑定最深 |
| **D. 白标定制合成** | 客户 schema/词表 → 批量 PNG + 标签 + QA；项目制现金流 |

不太建议作主路径：**全开源公开 benchmark**（引用多、直接收入少）——宜作 lite 公开 + holdout 商业。

### 3.4 主要风险

1. 买家要的是 draw.io / 通用 JSON，不是 PGM → 需 ingest 管线
2. Benchmark 与商业化张力：要信任常需部分公开；全闭源难成标准
3. 规模：eval 几千条够；训练商品要更大量级
4. 持续维护：facts 版本、题型、baseline 需更新
5. **内部不 dogfood** 时，客户会问「你们自己靠这个做什么」

### 3.5 建议落地阶段

```text
阶段 1：垂直 benchmark 品牌（6～12 月）
  → 选窄赛道；公开 lite + leaderboard；换引用

阶段 2：评测服务（先赚钱）
  → 闭源 holdout + API 报告

阶段 3：合规 / 描述 API（放大）
  → 绑 EAA/WCAG 采购

阶段 4：定制合成（项目现金流）
  → 私有训练集，不交付引擎
```

---

## 4. 相对 Mermaid / Graphviz / 合成工具的差异化

### 4.1 不要打的仗

- 拼「再多 10 万张普通流程图」——Mermaid 训练量与生态碾压
- 拼「人类手写 DSL 分发」——红海

### 4.2 能力对比（摘要）

| 维度 | Mermaid / Graphviz | Plotgram（管线做满时） |
|------|-------------------|------------------------|
| 产图规模与生态 | 极强 | 弱 |
| 结构标签 | 有 | 有（PGM） |
| 布局质量 | 参差 | 可达 yFiles 档 |
| 空间谓词标签 | 基本没有 | explain + audit |
| 同语义改布局对照组 | 难系统化 | 改 layout 参数即可 |
| 语义 diff / patch | 弱 | diff2 |
| 确定性 + 可回归 | 部分 | 设计目标 |
| Agent 读 facts 迭代闭环 | 无 | `--explain` 钩子 |

### 4.3 四条护城河

**1. 标签层数更深（一图多监督）**

```text
PNG + PGM + explain + measure + diff delta + debug trace
```

对外叙事：multi-layer supervised diagram corpus，同源零幻觉。

**2. 对照实验题（counterfactual pairs）**

同一 PGM，只改 `auto_edge_grouping` / `spacing` 等 → PNG_A/B + explain_A/B；PGM 语义可相同。  
可自动出题：关系是否相同、crossings 是否变、哪条边绕向变。  
测的是 VLM **真理解布局**，非背模板；市面稀缺。

**3. 考「布局理解」，非仅「认箭头」**

独占题型示例：缺陷检测、层级归属、绕向、detour ratio、改参后缺陷变化、Agent facts→调参→再渲染。  
品类定位：**Diagram Layout Understanding Benchmark**（与 ChartQA 类区分）。

**4. 管线即产品：ingest → relayout → re-explain**

```text
Mermaid / Graphviz / draw.io → 规范化 graph → plotgram 布局 → PNG + explain + measure
```

卖「专业布局引擎漂洗后的可信标签」，非卖 PGM 作者格式。

### 4.4 可执行清单（按壁垒排序）

1. 扩全 `explain` 词表（bus/grouping、回边、组方位、port）
2. 稳定 `measure --json` schema，可机器阅卷
3. **counterfactual 生成器**：同 PGM 扫参数网格 → 自动 pair/triple
4. **QA 程序化出题器**：难度分级
5. **import 适配器**：外部格式 → internal graph（不必双向兼容语法）

### 4.5 数据集设计原则

- Lite 公开 + Holdout 闭源
- 每图 5～20 道可验证题（结构/空间/质量/变更）
- 绑 `layout-facts vN` 与 `params_hash`
- 垂直包：架构图、审批流、时序+层次、合规图
- **几千张精品 × 多题型** > 百万张弱标签

### 4.6 对外三句话

1. Labels are engine-verified, not human- or VLM-guessed.
2. We test layout understanding, not just diagram parsing.
3. Counterfactual pairs expose models that memorize pixels.

### 4.7 要避免的坑

- 只开源 showcase、不建 holdout → 题被抄光
- explain 太薄 → 与 SVG 启发式分析差距缩小
- 域窄且不肯 ingest → 客户「我的图怎么办」
- 跟 Mermaid 比「谁更好生成」→ 分发战，非数据战
- 静态卖 zip → 一锤子买卖

---

## 5. 战略收束

```text
同一语义 → 多种布局呈现 → 每种呈现有 engine-exported facts
  → 自动生成结构/空间/质量/变更四类 QA
  → 闭源 holdout 做评测服务
```

| 场景 | Plotgram 相对 Mermaid 语料 |
|------|---------------------------|
| 训 VLM 认结构 | 弱 |
| 评 VLM 空间关系 | 强 |
| Agent 画完自检闭环 | 强 |
| 无障碍合规交付 | 强 |
| 布局回归 / PR bot | 强 |

**品类定位**：diagram understanding 的「阅卷局」，不是「造图工厂」。合成工具是上游供应商；壁垒在引擎 + 评测体系 + 合规叙事。

---

## 6. 待验证 / 开放问题

1. **DLUB v0 题型清单**：20 种题型 × 标签来源 × Mermaid 是否可复现——作 benchmark 规格草稿。
2. **首批垂直**：架构图 / 合规无障碍 / 通用 chart QA 三选一或组合，各配 12 个月 MVP 工作量表。
3. **explain 扩词表优先级**：哪些事实对商业题型阻塞最大（bus、组相对方位、回边…）。
4. **import 首发格式**：Mermaid vs Graphviz vs draw.io，按目标客户图库占比定序。
5. **与主 README §5 专有资产对齐**：benchmark 闭源策略、lite 集规模、定价是否与「团队资产治理」打包。

---

## 7. 相关文档

| 文档 | 关系 |
|------|------|
| [`README.md`](README.md) | 产品总战略；专有资产 / benchmark 为付费层之一 |
| [`ADR-007`](../design/adr/007-layout-facts-llm-channel.md) | layout-facts 通道决策与语料定位 |
| [`debug-inspector.md`](../design/layout/debug-inspector.md) | Trace 与 Facts 共享旁路几何 |
| [`diff-and-patch.md`](../guides/diff-and-patch.md) | 语义变更监督信号 |
| [`layout-diagnostics.md`](../guides/layout-diagnostics.md) | measure / 诊断 JSON |
